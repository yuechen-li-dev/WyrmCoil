#![allow(non_snake_case)]

use std::future::poll_fn;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, MouseButton, ParentElement, Pixels, Point,
    Render, SharedString, Styled, Subscription, Task, WeakEntity, Window, WindowId, actions,
    canvas, div, fill, point, px, rgb, size,
};
use workspace::{Item, MultiWorkspace, Workspace};
use wyrmcoil::Demo::{DemoWorld, InputEvent};
use wyrmcoil::Engine::shader::sdslv::{CompileCanonicalSourceToShaderArtifact, SdslvShaderStage};
use wyrmcoil::Engine::{Engine, EntityId, RenderItem, RenderSnapshot, Vec2};
use wyrmcoil::Scene::ARTIFACT_PATH;

mod room;
use room::RoomPreview;

const SCENE_SETTLE: Duration = Duration::from_millis(100);

actions!(
    wyrmcoil,
    [
        /// Open the 3D WyrmCoil Preview tab.
        OpenPreview,
        /// Open the original 2D WyrmCoil Preview tab.
        OpenPlanarPreview,
        /// Mount the compiled scene artifact from the local project.
        ReloadScene
    ]
);

pub fn Init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &OpenPreview, window, cx| {
            Open(workspace, Mode::Room, false, window, cx);
        });
        workspace.register_action(|workspace, _: &OpenPlanarPreview, window, cx| {
            Open(workspace, Mode::Planar, false, window, cx);
        });
        workspace.register_action(|workspace, _: &ReloadScene, window, cx| {
            Open(workspace, Mode::Room, true, window, cx);
        });
    })
    .detach();
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Planar,
    Room,
}

fn Open(
    workspace: &mut Workspace,
    mode: Mode,
    reload: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let existing = workspace
        .items_of_type::<Preview>(cx)
        .find(|item| item.read(cx).scene.Mode() == mode);
    let created = existing.is_none();
    let preview = existing.unwrap_or_else(|| {
        let preview = cx.new(|cx| Preview::New(mode, cx));
        workspace.add_item_to_active_pane(Box::new(preview.clone()), None, true, window, cx);
        preview
    });
    if mode == Mode::Room && (created || reload) {
        let artifact = ProjectArtifact(workspace, cx);
        let workspace_handle = cx.weak_entity();
        preview.update(cx, |preview, cx| {
            if created && let Ok(path) = &artifact {
                preview.scene_watch =
                    Some(WatchScene(workspace, workspace_handle, path.clone(), cx));
            }
            if reload && let Scene::Room(room) = &mut preview.scene {
                if room.Reload(artifact.as_deref().map_err(Clone::clone)) {
                    cx.notify();
                }
            }
        });
    }
    if !created {
        workspace.activate_item(&preview, true, true, window, cx);
    }
}

fn ProjectArtifact(workspace: &Workspace, cx: &App) -> Result<PathBuf, String> {
    let mut roots = workspace.visible_worktrees(cx).filter_map(|worktree| {
        let worktree = worktree.read(cx);
        let path = worktree.abs_path();
        (worktree.is_local() && path.is_dir()).then_some(path)
    });
    let Some(root) = roots.next() else {
        return Err("Open the local game project folder to reload its scene.".into());
    };
    if roots.next().is_some() {
        return Err(
            "Open the game project in a window with one local folder to reload its scene.".into(),
        );
    }
    Ok(root.join(ARTIFACT_PATH))
}

fn WatchScene(
    workspace: &Workspace,
    workspace_handle: WeakEntity<Workspace>,
    artifact: PathBuf,
    cx: &Context<Preview>,
) -> Task<()> {
    let fs = workspace.project().read(cx).fs().clone();
    cx.spawn(async move |preview, cx| {
        let target = artifact.parent().expect("scene artifact has a parent");
        let root = target.parent().expect("scene artifact has a project root");
        let mut saw_artifact = false;
        let error = 'register: loop {
            let directory = if target.is_dir() { target } else { root };
            if fs.requires_poll_watcher(directory) {
                break "Native notifications unavailable for this filesystem".into();
            }
            let (mut events, watcher) = fs.watch(directory, SCENE_SETTLE).await;
            // Zed logs failures from watch(); add also returns a registration error to the caller.
            if let Err(error) = watcher.add(directory) {
                break format!("{} · {error:#}", directory.display());
            }
            'read: loop {
                if (directory == target) != target.is_dir() {
                    continue 'register;
                }
                let current = workspace_handle.read_with(cx, ProjectArtifact);
                if !matches!(current, Ok(Ok(path)) if path == artifact) {
                    break 'register "Project folder changed · reopen the preview to watch it"
                        .into();
                }
                if preview
                    .update(cx, |preview, cx| {
                        if let Scene::Room(room) = &mut preview.scene
                            && room.ReloadWatched(&artifact, &mut saw_artifact)
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    return;
                }
                while let Some(batch) = poll_fn(|cx| events.as_mut().poll_next(cx)).await {
                    if SceneBatchMatches(
                        &artifact,
                        directory,
                        batch.iter().map(|event| event.path.as_path()),
                    ) {
                        continue 'read;
                    }
                }
                break 'register "Filesystem notification stream closed".into();
            }
        };
        let _ = preview.update(cx, |preview, cx| {
            if let Scene::Room(room) = &mut preview.scene
                && room.SetNotice(format!("Scene watcher stopped · {error}"))
            {
                cx.notify();
            }
        });
    })
}

fn SceneBatchMatches<'a>(
    artifact: &Path,
    directory: &Path,
    paths: impl IntoIterator<Item = &'a Path>,
) -> bool {
    paths
        .into_iter()
        .any(|path| path == artifact || path == directory || Some(path) == artifact.parent())
}

enum Scene {
    Planar {
        session: PreviewSession,
        camera: Vec2,
        shader: Option<Arc<gpui::GpuViewportShader>>,
    },
    Room(RoomPreview),
}

impl Scene {
    fn Mode(&self) -> Mode {
        match self {
            Self::Planar { .. } => Mode::Planar,
            Self::Room(_) => Mode::Room,
        }
    }

    fn Input(&mut self) -> &mut MovementInput {
        match self {
            Self::Planar { session, .. } => &mut session.input,
            Self::Room(room) => &mut room.input,
        }
    }

    fn Suspend(&mut self) {
        match self {
            Self::Planar { session, .. } => session.Suspend(),
            Self::Room(room) => room.Suspend(),
        }
    }

    fn Advance(&mut self, now: Instant) -> bool {
        match self {
            Self::Planar {
                session, camera, ..
            } => {
                if !session.Advance(now) {
                    return false;
                }
                let player = session.engine.Player;
                if let Some(item) = session
                    .snapshot
                    .Items
                    .iter()
                    .find(|item| item.Entity == player)
                {
                    *camera = TrackPlayer(*camera, item.Position);
                }
                true
            }
            Self::Room(room) => room.Advance(now),
        }
    }
}

struct Preview {
    focus: FocusHandle,
    title: SharedString,
    scene: Scene,
    paused: bool,
    pending_frame: Option<WindowId>,
    window_observers: Option<[Subscription; 4]>,
    scene_watch: Option<Task<()>>,
}

impl Preview {
    fn New(mode: Mode, cx: &mut Context<Self>) -> Self {
        let scene = match mode {
            Mode::Planar => Scene::Planar {
                session: PreviewSession::New(),
                camera: Vec2::Zero(),
                shader: NewViewportShader(),
            },
            Mode::Room => Scene::Room(RoomPreview::New()),
        };
        let suffix = if mode == Mode::Planar {
            " Preview (2D)"
        } else {
            " Preview"
        };
        Self {
            focus: cx.focus_handle(),
            title: format!("{}{suffix}", wyrmcoil::ProjectName()).into(),
            scene,
            paused: false,
            pending_frame: None,
            window_observers: None,
            scene_watch: None,
        }
    }

    fn RequestFrame(&mut self, window: &Window, cx: &Context<Self>) {
        let window_id = window.window_handle().window_id();
        if self.pending_frame == Some(window_id) {
            return;
        }
        self.pending_frame = Some(window_id);
        // Context::on_next_frame owns a strong entity and a hidden window can defer delivery.
        let preview = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            if let Some(preview) = preview.upgrade() {
                preview.update(cx, |preview, cx| preview.OnFrame(window, cx));
            }
        });
    }

    fn OnFrame(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.pending_frame != Some(window.window_handle().window_id()) {
            return;
        }
        self.pending_frame = None;
        let selected = window
            .root::<MultiWorkspace>()
            .flatten()
            .is_some_and(|root| {
                let workspace = root.read(cx).workspace().read(cx);
                workspace
                    .active_item(cx)
                    .is_some_and(|item| item.item_id() == cx.entity_id())
            });
        if self.paused || !selected || !window.is_window_active() || !window.is_visible() {
            self.scene.Suspend();
            return;
        }

        if !self.TakesInput(window) || window.modifiers().modified() {
            self.scene.Input().Cancel();
        }
        if self.scene.Advance(Instant::now()) {
            cx.notify();
        } else {
            // Keep the clock live above 60 Hz without repainting an unchanged snapshot.
            self.RequestFrame(window, cx);
        }
    }

    fn ResetTiming(&mut self, cx: &mut Context<Self>) {
        self.scene.Suspend();
        cx.notify();
    }

    fn TakesInput(&self, window: &Window) -> bool {
        !self.paused
            && window.is_window_active()
            && window.is_visible()
            && self.focus.is_focused(window)
    }

    fn Pause(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.TakesInput(window) && !window.modifiers().modified() {
            self.paused = true;
            self.ResetTiming(cx);
        }
    }

    fn KeyDown(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.TakesInput(window) || event.keystroke.modifiers.modified() {
            return;
        }
        if event.keystroke.key == "escape" && !event.is_held {
            self.Pause(window, cx);
            cx.stop_propagation();
        } else if self.scene.Input().KeyDown(&event.keystroke, event.is_held) {
            cx.stop_propagation();
        }
    }

    fn KeyUp(&mut self, event: &KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.TakesInput(window) && self.scene.Input().KeyUp(&event.keystroke) {
            cx.stop_propagation();
        }
    }
}

fn BuildViewportShader() -> Result<gpui::GpuViewportShader, String> {
    BuildShader("viewport.sdslv", include_str!("viewport.sdslv"))
}

fn BuildShader(name: &str, source: &str) -> Result<gpui::GpuViewportShader, String> {
    let artifact = CompileCanonicalSourceToShaderArtifact(name, source)
        .map_err(|errors| format!("{name}: {errors:?}"))?;
    let [vertex, pixel] = artifact.EntryPoints.as_slice() else {
        return Err("expected exactly two viewport shader entries".to_string());
    };
    if vertex.Stage != SdslvShaderStage::Vertex || pixel.Stage != SdslvShaderStage::Pixel {
        return Err("expected a vertex and pixel pair".to_string());
    }
    Ok(gpui::GpuViewportShader {
        source: artifact.Hlsl,
        vertex_entry: vertex.Name.clone(),
        pixel_entry: pixel.Name.clone(),
        mesh: None,
    })
}

fn NewViewportShader() -> Option<Arc<gpui::GpuViewportShader>> {
    if !cfg!(target_os = "windows") || std::env::var_os("WYRMCOIL_GPUI_MARKER").is_some() {
        return None;
    }
    match BuildViewportShader() {
        Ok(shader) => Some(Arc::new(shader)),
        Err(error) => {
            eprintln!("WyrmCoil viewport shader: {error}");
            None
        }
    }
}

#[derive(Default)]
struct MovementInput {
    held: u8,
    pressed: u8,
    applied: (i32, i32),
}

impl MovementInput {
    fn KeyDown(&mut self, key: &Keystroke, repeat: bool) -> bool {
        let bit = Self::KeyBit(&key.key);
        if bit == 0 || key.modifiers.modified() {
            return false;
        }
        if !repeat && self.held & bit == 0 {
            self.held |= bit;
            self.pressed |= bit;
        }
        true
    }

    fn KeyUp(&mut self, key: &Keystroke) -> bool {
        let bit = Self::KeyBit(&key.key);
        let was_held = self.held & bit != 0;
        self.held &= !bit;
        was_held && !key.modifiers.modified()
    }

    fn KeyBit(key: &str) -> u8 {
        match key {
            "w" => 1 << 0,
            "up" => 1 << 1,
            "s" => 1 << 2,
            "down" => 1 << 3,
            "a" => 1 << 4,
            "left" => 1 << 5,
            "d" => 1 << 6,
            "right" => 1 << 7,
            _ => 0,
        }
    }

    fn Latch(&mut self) -> Option<InputEvent> {
        let active = self.TakeActive();
        let up = i32::from(active & 0b0000_0011 != 0);
        let down = i32::from(active & 0b0000_1100 != 0);
        let left = i32::from(active & 0b0011_0000 != 0);
        let right = i32::from(active & 0b1100_0000 != 0);
        let movement = (right - left, up - down);
        if movement == self.applied {
            return None;
        }
        self.applied = movement;
        Some(InputEvent::SetPlayerMovement {
            X: movement.0,
            Y: movement.1,
        })
    }

    fn TakeActive(&mut self) -> u8 {
        // A direction pressed and released between ticks still gets one step.
        self.held | std::mem::take(&mut self.pressed)
    }

    fn Cancel(&mut self) {
        self.held = 0;
        self.pressed = 0;
        // Keep the last applied direction so the next tick can send a stop.
    }
}

const SIMULATION_HZ: u128 = 60;
const MAX_STEPS: u128 = 4;
const STEP_CREDIT: u128 = Duration::from_secs(1).as_nanos();

#[derive(Default)]
struct FixedStep {
    previous: Option<Instant>,
    credit: u128,
}

impl FixedStep {
    fn Steps(&mut self, now: Instant) -> u128 {
        let Some(previous) = self.previous.replace(now) else {
            return 0;
        };
        // Scaled nanoseconds preserve the exact 1/60 s period across frame partitions.
        let elapsed_credit = now.saturating_duration_since(previous).as_nanos() * SIMULATION_HZ;
        self.credit = (self.credit + elapsed_credit).min(MAX_STEPS * STEP_CREDIT);
        let steps = self.credit / STEP_CREDIT;
        self.credit %= STEP_CREDIT;
        steps
    }

    fn Pause(&mut self) {
        *self = Self::default();
    }
}

struct PreviewSession {
    engine: Engine<DemoWorld, InputEvent>,
    snapshot: RenderSnapshot,
    step: FixedStep,
    input: MovementInput,
}

impl PreviewSession {
    fn New() -> Self {
        let mut engine = Engine::New();
        let snapshot = engine.RenderSnapshot();
        Self {
            engine,
            snapshot,
            step: FixedStep::default(),
            input: MovementInput::default(),
        }
    }

    fn Advance(&mut self, now: Instant) -> bool {
        let steps = self.step.Steps(now);
        for _ in 0..steps {
            if let Some(event) = self.input.Latch() {
                self.engine.EnqueueInput(event);
            }
            self.engine.TickControl();
            self.engine.TickSimulation();
        }
        if steps == 0 {
            return false;
        }
        self.snapshot = self.engine.RenderSnapshot();
        true
    }

    fn Suspend(&mut self) {
        self.input.Cancel();
        self.step.Pause();
    }
}

impl Focusable for Preview {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.paused && window.is_window_active() && window.is_visible() {
            self.RequestFrame(window, cx);
        }

        let controls = if self.paused {
            "Paused · click to resume"
        } else if !self.TakesInput(window) {
            "Click to play"
        } else if self.scene.Mode() == Mode::Room {
            "WASD walk · arrows look · Esc pauses"
        } else {
            "Game focused · WASD / arrows · Esc pauses"
        };
        let (status, viewport) = match &self.scene {
            Scene::Planar {
                session,
                camera,
                shader,
            } => DrawPlanar(session, *camera, shader.clone(), controls),
            Scene::Room(room) => room.Draw(controls),
        };

        div()
            .key_context("WyrmCoilPreview")
            .track_focus(&self.focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|preview, _, window, cx| {
                    preview.paused = false;
                    window.focus(&preview.focus, cx);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(Self::KeyDown))
            .on_key_up(cx.listener(Self::KeyUp))
            .on_modifiers_changed(cx.listener(
                |preview, event: &gpui::ModifiersChangedEvent, _, _| {
                    if event.modifiers.modified() {
                        preview.scene.Input().Cancel();
                    }
                },
            ))
            // Zed binds Escape to Unfollow in the workspace context.
            .on_action(cx.listener(|preview, _: &workspace::Unfollow, window, cx| {
                preview.Pause(window, cx);
                cx.propagate();
            }))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(0x151b26))
            .text_color(rgb(0xe7edf6))
            .child(status)
            .child(viewport.flex_1().w_full())
    }
}

fn Status(controls: &'static str, clock: wyrmcoil::Engine::EngineClock, frame: u64) -> gpui::Div {
    div()
        .flex()
        .flex_wrap()
        .gap_x_4()
        .px_3()
        .py_2()
        .text_sm()
        .child(controls)
        .child(format!(
            "Control {}   Simulation {}   Snapshot {}",
            clock.ControlTick, clock.SimulationTick, frame
        ))
}

fn DrawPlanar(
    session: &PreviewSession,
    camera: Vec2,
    gpu_shader: Option<Arc<gpui::GpuViewportShader>>,
    controls: &'static str,
) -> (gpui::Div, gpui::Canvas<()>) {
    let player = session.engine.Player;
    let guard = session.engine.Guard;
    let mut snapshot = session.snapshot.clone();
    let mut status = Status(controls, session.engine.Clock(), snapshot.Frame);
    for item in &snapshot.Items {
        let (name, color) = Appearance(item.Entity, player, guard);
        status = status.child(div().text_color(rgb(color)).child(format!(
            "{} #{} · sprite {} · ({:.1}, {:.1})",
            name, item.Entity.0, item.SpriteId, item.Position.X, item.Position.Y
        )));
    }
    let viewport = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            if bounds.is_empty() {
                return;
            }
            let viewport = Viewport::New(bounds, camera);
            viewport.PaintGrid(bounds, window);
            if let Some(shader) = &gpu_shader {
                let instances = viewport.Instances(bounds, &mut snapshot.Items, player, guard);
                if window.paint_gpu_viewport(
                    bounds,
                    shader,
                    gpui::GpuViewportDraw::Triangles(instances),
                ) {
                    return;
                }
            }
            for item in &snapshot.Items {
                let (center, color) = viewport.Marker(bounds, item, player, guard);
                let side = viewport.marker_size;
                let origin = point(center.x - side * 0.5, center.y - side * 0.5);
                window.paint_quad(fill(Bounds::new(origin, size(side, side)), rgb(color)));
            }
        },
    );
    (status, viewport)
}

fn Appearance(entity: EntityId, player: EntityId, guard: EntityId) -> (&'static str, u32) {
    if entity == player {
        ("Player", 0x67d4ee)
    } else if entity == guard {
        ("Guard", 0xffb160)
    } else {
        ("Entity", 0xd0d8e8)
    }
}

fn TrackPlayer(camera: Vec2, player: Vec2) -> Vec2 {
    const DEAD_ZONE: f32 = 18.0;
    Vec2 {
        X: camera.X.clamp(player.X - DEAD_ZONE, player.X + DEAD_ZONE),
        Y: camera.Y.clamp(player.Y - DEAD_ZONE, player.Y + DEAD_ZONE),
    }
}

struct Viewport {
    origin: Point<Pixels>,
    camera: Vec2,
    scale: Pixels,
    marker_size: Pixels,
}

impl Viewport {
    fn New(bounds: Bounds<Pixels>, camera: Vec2) -> Self {
        let side = bounds.size.width.min(bounds.size.height);
        Self {
            origin: bounds.center(),
            camera,
            scale: side * (1.0 / 80.0),
            marker_size: px(16.0).min(side * 0.12),
        }
    }

    fn Project(&self, position: Vec2) -> Point<Pixels> {
        point(
            self.origin.x + self.scale * (position.X - self.camera.X),
            self.origin.y - self.scale * (position.Y - self.camera.Y),
        )
    }

    fn ClipMarker(&self, bounds: Bounds<Pixels>, projected: Point<Pixels>) -> Point<Pixels> {
        let x = projected.x - self.origin.x;
        let y = projected.y - self.origin.y;
        let limit_x = (bounds.size.width * 0.5 - self.marker_size).max(px(1.0));
        let limit_y = (bounds.size.height * 0.5 - self.marker_size).max(px(1.0));
        let ratio = (x.abs() / limit_x).max(y.abs() / limit_y);
        if ratio <= 1.0 {
            return projected;
        }
        point(self.origin.x + x / ratio, self.origin.y + y / ratio)
    }

    fn Marker(
        &self,
        bounds: Bounds<Pixels>,
        item: &RenderItem,
        player: EntityId,
        guard: EntityId,
    ) -> (Point<Pixels>, u32) {
        let (_, color) = Appearance(item.Entity, player, guard);
        let projected = self.Project(item.Position);
        let center = self.ClipMarker(bounds, projected);
        if item.Entity == guard && center != projected {
            return (center, 0xb47742);
        }
        (center, color)
    }

    fn Instances(
        &self,
        bounds: Bounds<Pixels>,
        items: &mut [RenderItem],
        player: EntityId,
        guard: EntityId,
    ) -> gpui::GpuViewportInstances {
        let mut instances = gpui::GpuViewportInstances::new();
        if bounds.is_empty() {
            return instances;
        }
        // Higher entity IDs paint last. Sprite IDs break ties within an entity.
        items.sort_unstable_by_key(|item| (item.Entity.0, item.SpriteId));
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        let side = f32::from(self.marker_size);
        for item in items {
            let (center, color) = self.Marker(bounds, item, player, guard);
            let x = f32::from(center.x - bounds.left());
            let y = f32::from(center.y - bounds.top());
            instances.push(gpui::GpuViewportInstance {
                transform: [
                    2.0 * x / width - 1.0,
                    1.0 - 2.0 * y / height,
                    side / width,
                    side / height,
                ],
                color: [
                    ((color >> 16) & 255) as f32 / 255.0,
                    ((color >> 8) & 255) as f32 / 255.0,
                    (color & 255) as f32 / 255.0,
                    1.0,
                ],
            });
        }
        instances
    }

    fn PaintGrid(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let spacing = self.scale * 12.0;
        if spacing < px(8.0) {
            return;
        }
        let zero = self.Project(Vec2::Zero());
        let phase = |offset: Pixels| px(f32::from(offset).rem_euclid(f32::from(spacing)));
        let color = rgb(0x202a3b);
        let mut x = bounds.left() + phase(zero.x - bounds.left());
        while x < bounds.right() {
            window.paint_quad(fill(
                Bounds::new(point(x, bounds.top()), size(px(1.0), bounds.size.height)),
                color,
            ));
            x += spacing;
        }
        let mut y = bounds.top() + phase(zero.y - bounds.top());
        while y < bounds.bottom() {
            window.paint_quad(fill(
                Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(1.0))),
                color,
            ));
            y += spacing;
        }
    }
}

impl EventEmitter<()> for Preview {}

impl Item for Preview {
    type Event = ();

    fn tab_content_text(&self, _: usize, _: &App) -> SharedString {
        self.title.clone()
    }

    fn added_to_workspace(
        &mut self,
        _: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scene.Suspend();
        self.window_observers = Some([
            cx.observe_window_activation(window, |preview, _, cx| preview.ResetTiming(cx)),
            cx.observe_window_visibility(window, |preview, _, _, cx| preview.ResetTiming(cx)),
            cx.on_focus(&self.focus, window, |_, _, cx| cx.notify()),
            cx.on_blur(&self.focus, window, |preview, _, cx| {
                preview.scene.Input().Cancel();
                cx.notify();
            }),
        ]);
    }

    fn deactivated(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.scene.Suspend();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_viewport_shader_uses_vertex_id_and_instanced_transforms() {
        let shader = super::BuildViewportShader().unwrap();
        assert_eq!(shader.vertex_entry, "PreviewInstances_VS");
        assert_eq!(shader.pixel_entry, "PreviewInstances_PS");
        assert!(shader.source.contains("SV_VertexID"));
        assert!(shader.source.contains("SV_Target"));
        assert!(shader.source.contains("Transform : TEXCOORD0"));
        assert!(shader.source.contains("Color : TEXCOORD1"));
    }
    use super::*;

    #[test]
    fn artifact_watch_filters_source_and_atomic_publish_temporary_files() {
        let root = Path::new("training-room");
        let artifact = root.join(ARTIFACT_PATH);
        let target = artifact.parent().unwrap();
        let source = root.join("src/scene.rs");
        let temporary = artifact.with_extension("tmp.42.1");
        let binary = target.join("debug/training-room.exe");
        for directory in [root, target] {
            assert!(!SceneBatchMatches(
                &artifact,
                directory,
                [&*source, &*temporary, &*binary]
            ));
            assert!(SceneBatchMatches(
                &artifact,
                directory,
                [&*temporary, &*artifact]
            ));
            assert!(SceneBatchMatches(&artifact, directory, [&*artifact]));
        }
        assert!(SceneBatchMatches(&artifact, root, [target]));
        assert!(SceneBatchMatches(&artifact, target, [target]));
    }

    #[test]
    fn a_burst_with_duplicate_artifact_events_requests_one_read() {
        let artifact = Path::new(ARTIFACT_PATH);
        let target = artifact.parent().unwrap();
        let temporary = artifact.with_extension("tmp.42.1");
        let unrelated = [&*temporary; 32];
        let burst = [artifact; 256];
        let batches: [&[&Path]; 3] = [&unrelated, &burst, &unrelated];
        let reads = batches
            .iter()
            .filter(|batch| SceneBatchMatches(artifact, target, batch.iter().copied()))
            .count();
        assert_eq!(reads, 1);
    }

    fn Key(name: &str) -> Keystroke {
        Keystroke {
            key: name.to_owned(),
            key_char: None,
            modifiers: gpui::Modifiers::default(),
        }
    }

    fn Step(session: &mut PreviewSession, now: &mut Instant) {
        *now += Duration::from_nanos(16_666_667);
        assert!(session.Advance(*now));
    }

    fn PlayerPosition(session: &PreviewSession) -> Vec2 {
        session
            .engine
            .World
            .Transforms
            .Position(session.engine.Player)
            .unwrap()
    }

    #[test]
    fn movement_keys_press_hold_and_release() {
        for (key, x, y) in [
            ("w", 0, 1),
            ("up", 0, 1),
            ("s", 0, -1),
            ("down", 0, -1),
            ("a", -1, 0),
            ("left", -1, 0),
            ("d", 1, 0),
            ("right", 1, 0),
        ] {
            let mut input = MovementInput::default();
            let key = Key(key);
            assert!(input.KeyDown(&key, false));
            assert_eq!(
                input.Latch(),
                Some(InputEvent::SetPlayerMovement { X: x, Y: y })
            );
            assert_eq!(input.Latch(), None);
            assert!(input.KeyUp(&key));
            assert_eq!(
                input.Latch(),
                Some(InputEvent::SetPlayerMovement { X: 0, Y: 0 })
            );
        }
    }

    #[test]
    fn short_tap_survives_and_repeats_do_not_add_edges() {
        let mut session = PreviewSession::New();
        let mut now = Instant::now();
        session.Advance(now);
        let right = Key("d");
        assert!(session.input.KeyDown(&right, true));
        Step(&mut session, &mut now);
        assert_eq!(PlayerPosition(&session), Vec2::Zero());

        session.input.KeyDown(&right, false);
        session.input.KeyDown(&right, false);
        session.input.KeyDown(&right, true);
        session.input.KeyUp(&right);
        Step(&mut session, &mut now);
        assert_eq!(PlayerPosition(&session), Vec2 { X: 1.0, Y: 0.0 });
        Step(&mut session, &mut now);
        assert_eq!(PlayerPosition(&session), Vec2 { X: 1.0, Y: 0.0 });
    }

    #[test]
    fn sustained_movement_uses_the_real_demo_velocity() {
        let mut session = PreviewSession::New();
        let mut now = Instant::now();
        session.Advance(now);
        session.input.KeyDown(&Key("w"), false);
        for _ in 0..6 {
            Step(&mut session, &mut now);
        }
        assert_eq!(PlayerPosition(&session), Vec2 { X: 0.0, Y: 6.0 });
        session.input.KeyUp(&Key("w"));
        Step(&mut session, &mut now);
        assert_eq!(PlayerPosition(&session), Vec2 { X: 0.0, Y: 6.0 });
    }

    #[test]
    fn opposing_directions_cancel_and_aliases_release_independently() {
        let mut input = MovementInput::default();
        for key in ["a", "left", "d", "w", "down"] {
            input.KeyDown(&Key(key), false);
        }
        assert_eq!(input.Latch(), None);
        input.KeyUp(&Key("a"));
        assert_eq!(input.Latch(), None);
        input.KeyUp(&Key("left"));
        assert_eq!(
            input.Latch(),
            Some(InputEvent::SetPlayerMovement { X: 1, Y: 0 })
        );
    }

    #[test]
    fn diagonal_movement_is_normalized_and_guard_keeps_moving() {
        let mut session = PreviewSession::New();
        let mut now = Instant::now();
        session.Advance(now);
        for _ in 0..4 {
            Step(&mut session, &mut now);
        }
        let guard = session.snapshot.Items[1].Position;
        session.input.KeyDown(&Key("right"), false);
        session.input.KeyDown(&Key("up"), false);
        Step(&mut session, &mut now);
        let player = PlayerPosition(&session);
        assert_eq!(player.X, std::f32::consts::FRAC_1_SQRT_2);
        assert_eq!(player.X, player.Y);
        assert!((player.X.hypot(player.Y) - 1.0).abs() < 0.0001);
        assert!(session.snapshot.Items[1].Position.X > guard.X);
    }

    #[test]
    fn blur_cancels_movement_even_when_key_up_goes_elsewhere() {
        let mut session = PreviewSession::New();
        let mut now = Instant::now();
        session.Advance(now);
        let right = Key("d");
        session.input.KeyDown(&right, false);
        Step(&mut session, &mut now);
        let position = PlayerPosition(&session);

        session.input.Cancel();
        session.input.KeyDown(&right, true);
        Step(&mut session, &mut now);
        assert_eq!(PlayerPosition(&session), position);
        session.input.KeyDown(&right, false);
        Step(&mut session, &mut now);
        assert!(PlayerPosition(&session).X > position.X);
    }

    #[test]
    fn tab_and_window_suspension_drop_backlog_and_pending_taps() {
        let mut session = PreviewSession::New();
        let mut now = Instant::now();
        session.Advance(now);
        session.input.KeyDown(&Key("w"), false);
        Step(&mut session, &mut now);
        let position = PlayerPosition(&session);
        session.input.KeyDown(&Key("a"), false);
        session.input.KeyUp(&Key("a"));

        session.Suspend();
        now += Duration::from_secs(60);
        assert!(!session.Advance(now));
        session.input.KeyDown(&Key("w"), true);
        Step(&mut session, &mut now);
        assert_eq!(session.engine.Clock().SimulationTick, 2);
        assert_eq!(PlayerPosition(&session), position);

        let mut reopened = PreviewSession::New();
        assert!(!reopened.Advance(now));
        Step(&mut reopened, &mut now);
        assert_eq!(PlayerPosition(&reopened), Vec2::Zero());
    }

    #[test]
    fn identical_tick_indexed_input_replays_deterministically() {
        fn Run() -> PreviewSession {
            let mut session = PreviewSession::New();
            let mut now = Instant::now();
            session.Advance(now);
            for (key, pressed) in [("w", true), ("d", true), ("w", false), ("d", false)] {
                if pressed {
                    session.input.KeyDown(&Key(key), false);
                } else {
                    session.input.KeyUp(&Key(key));
                }
                for _ in 0..3 {
                    Step(&mut session, &mut now);
                }
            }
            session
        }
        assert_eq!(Run().engine.ExportChunk(), Run().engine.ExportChunk());
    }

    #[test]
    fn shortcut_modifiers_and_unbound_keys_are_not_consumed() {
        let mut input = MovementInput::default();
        for (name, modifiers) in [
            (
                "w",
                gpui::Modifiers {
                    control: true,
                    ..Default::default()
                },
            ),
            (
                "s",
                gpui::Modifiers {
                    control: true,
                    ..Default::default()
                },
            ),
            (
                "p",
                gpui::Modifiers {
                    control: true,
                    shift: true,
                    ..Default::default()
                },
            ),
            (
                "tab",
                gpui::Modifiers {
                    alt: true,
                    ..Default::default()
                },
            ),
            ("d", gpui::Modifiers::command()),
            ("space", gpui::Modifiers::default()),
        ] {
            let mut key = Key(name);
            key.modifiers = modifiers;
            assert!(!input.KeyDown(&key, false));
            assert!(!input.KeyUp(&key));
        }
        assert_eq!(input.Latch(), None);

        let mut left = Key("a");
        assert!(input.KeyDown(&left, false));
        assert!(input.Latch().is_some());
        left.modifiers.control = true;
        assert!(!input.KeyUp(&left));
        assert_eq!(
            input.Latch(),
            Some(InputEvent::SetPlayerMovement { X: 0, Y: 0 })
        );
    }

    #[test]
    fn carries_fractional_steps() {
        let start = Instant::now();
        let mut step = FixedStep::default();
        assert_eq!(step.Steps(start), 0);
        assert_eq!(step.Steps(start + Duration::from_millis(10)), 0);
        assert_eq!(step.Steps(start + Duration::from_millis(20)), 1);
        assert_eq!(step.Steps(start + Duration::from_millis(34)), 1);
    }

    #[test]
    fn frame_partitions_produce_the_same_simulation() {
        let start = Instant::now();
        let mut fast = PreviewSession::New();
        let mut slow = PreviewSession::New();
        fast.Advance(start);
        slow.Advance(start);
        for frame in 1..=100 {
            fast.Advance(start + Duration::from_millis(frame * 10));
        }
        for frame in 1..=25 {
            slow.Advance(start + Duration::from_millis(frame * 40));
        }

        assert_eq!(fast.engine.Clock().ControlTick, 60);
        assert_eq!(fast.engine.Clock().SimulationTick, 60);
        assert_eq!(slow.engine.Clock().ControlTick, 60);
        assert_eq!(slow.engine.Clock().SimulationTick, 60);
        assert_eq!(fast.snapshot.Items, slow.snapshot.Items);
        assert_ne!(fast.snapshot.Frame, slow.snapshot.Frame);
    }

    #[test]
    fn long_stall_drops_excess_ticks() {
        let start = Instant::now();
        let mut step = FixedStep::default();
        step.Steps(start);
        let resumed = start + Duration::from_secs(60);
        assert_eq!(step.Steps(resumed), 4);
        assert_eq!(step.Steps(resumed), 0);
        assert_eq!(step.Steps(resumed + Duration::from_millis(17)), 1);
    }

    #[test]
    fn pause_resets_elapsed_time_and_fractional_credit() {
        let start = Instant::now();
        let mut step = FixedStep::default();
        step.Steps(start);
        assert_eq!(step.Steps(start + Duration::from_millis(15)), 0);
        step.Pause();

        let resumed = start + Duration::from_secs(60);
        assert_eq!(step.Steps(resumed), 0);
        assert_eq!(step.Steps(resumed + Duration::from_millis(15)), 0);
        assert_eq!(step.Steps(resumed + Duration::from_millis(17)), 1);
    }

    #[test]
    fn demo_snapshot_uses_real_entity_ids_and_positions() {
        let start = Instant::now();
        let mut session = PreviewSession::New();
        let initial = session.snapshot.clone();
        assert_eq!(initial.Items.len(), 2);
        assert_eq!(initial.Items[0].Entity, session.engine.Player);
        assert_eq!(initial.Items[1].Entity, session.engine.Guard);
        assert_eq!(initial.Items[0].SpriteId, 0);
        assert_eq!(initial.Items[1].SpriteId, 0);

        session.Advance(start);
        for frame in 1..=20 {
            session.Advance(start + Duration::from_millis(frame * 17));
        }
        assert_eq!(session.snapshot.Items[0], initial.Items[0]);
        let guard = &session.snapshot.Items[1];
        assert_eq!(guard.Entity, session.engine.Guard);
        assert_eq!(
            guard.Position,
            session
                .engine
                .World
                .Transforms
                .Position(guard.Entity)
                .unwrap()
        );
        assert!(guard.Position.X > initial.Items[1].Position.X);
        assert!(guard.Position.Y > initial.Items[1].Position.Y);
    }

    #[test]
    fn reopening_creates_a_fresh_session() {
        let start = Instant::now();
        let mut old = PreviewSession::New();
        let initial = old.snapshot.clone();
        old.Advance(start);
        assert!(old.Advance(start + Duration::from_millis(17)));
        assert_eq!(old.engine.Clock().SimulationTick, 1);
        drop(old);

        let mut fresh = PreviewSession::New();
        assert_eq!(fresh.engine.Clock().ControlTick, 0);
        assert_eq!(fresh.engine.Clock().SimulationTick, 0);
        assert_eq!(fresh.snapshot, initial);
        assert!(!fresh.Advance(start + Duration::from_secs(60)));
    }

    #[test]
    fn viewport_keeps_scale_and_clips_distant_markers_after_resize() {
        let session = PreviewSession::New();
        let bounds = Bounds::new(point(px(40.0), px(60.0)), size(px(300.0), px(200.0)));
        let viewport = Viewport::New(bounds, Vec2::Zero());
        assert_eq!(viewport.Project(Vec2::Zero()), point(px(190.0), px(160.0)));
        let guard = session.snapshot.Items[1].Position;
        let projected = viewport.Project(guard);
        assert_eq!(projected, point(px(202.5), px(147.5)));
        assert_eq!(viewport.ClipMarker(bounds, projected), projected);
        let narrow = Bounds::new(point(px(40.0), px(60.0)), size(px(96.0), px(240.0)));
        let viewport = Viewport::New(narrow, Vec2::Zero());
        let distant = viewport.Project(Vec2 {
            X: 1_000.0,
            Y: 1_000.0,
        });
        let projected = viewport.ClipMarker(narrow, distant);
        assert!(narrow.contains(&projected));
        assert!(projected.x > narrow.center().x && projected.y < narrow.center().y);
    }

    #[test]
    fn gpu_instances_follow_the_real_snapshot_in_entity_order() {
        let mut session = PreviewSession::New();
        let bounds = Bounds::new(point(px(40.0), px(60.0)), size(px(320.0), px(240.0)));
        let player = session.engine.Player;
        let guard = session.engine.Guard;
        let viewport = Viewport::New(bounds, Vec2 { X: 8.0, Y: -4.0 });
        let mut snapshot = session.engine.RenderSnapshot();
        snapshot.Items.reverse();
        let initial = viewport.Instances(bounds, &mut snapshot.Items, player, guard);
        assert_eq!(snapshot.Items[0].Entity, player);
        assert_eq!(snapshot.Items[1].Entity, guard);
        assert_eq!(initial.len(), 2);
        assert!(!initial.spilled());
        assert_eq!(&initial[0].transform[2..], &[16.0 / 320.0, 16.0 / 240.0]);
        assert!((initial[0].transform[0] + 0.15).abs() < 1e-6);
        assert!((initial[0].transform[1] - 0.10).abs() < 1e-6);
        assert_ne!(initial[0].color, initial[1].color);

        let mut now = Instant::now();
        session.Advance(now);
        for _ in 0..20 {
            Step(&mut session, &mut now);
        }
        let advanced = viewport.Instances(bounds, &mut session.snapshot.Items, player, guard);
        assert_eq!(advanced[0], initial[0]);
        assert!(advanced[1].transform[0] > initial[1].transform[0]);
        assert!(advanced[1].transform[1] > initial[1].transform[1]);
    }

    #[test]
    fn gpu_instances_respect_visibility_empty_bounds_and_guard_edge() {
        let mut session = PreviewSession::New();
        let player = session.engine.Player;
        let guard = session.engine.Guard;
        let bounds = Bounds::new(point(px(40.0), px(60.0)), size(px(160.0), px(240.0)));
        let viewport = Viewport::New(bounds, Vec2::Zero());
        session.engine.World.Transforms.Alive[player.0] = false;
        session.engine.World.Transforms.Positions[guard.0] = Vec2 {
            X: 1000.0,
            Y: 1000.0,
        };
        let mut snapshot = session.engine.RenderSnapshot();
        let single = viewport.Instances(bounds, &mut snapshot.Items, player, guard);
        assert_eq!(single.len(), 1);
        assert_eq!(snapshot.Items[0].Entity, guard);
        let [x, y, half_x, half_y] = single[0].transform;
        assert!(x > 0.0 && x + half_x <= 1.0);
        assert!(y > 0.0 && y + half_y <= 1.0);
        assert_eq!(single[0].color[0], 180.0 / 255.0);
        let zero = Bounds::new(bounds.origin, size(px(0.0), bounds.size.height));
        assert!(
            viewport
                .Instances(zero, &mut snapshot.Items, player, guard)
                .is_empty()
        );

        session.engine.World.Renderables.Visible[guard.0] = false;
        let mut snapshot = session.engine.RenderSnapshot();
        assert!(
            viewport
                .Instances(bounds, &mut snapshot.Items, player, guard)
                .is_empty()
        );
    }

    #[test]
    fn guard_keeps_moving_forward_across_the_old_zoom_threshold() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)));
        let viewport = Viewport::New(bounds, Vec2::Zero());
        let before = viewport.Project(Vec2 { X: 12.0, Y: 12.0 });
        let after = viewport.Project(Vec2 { X: 12.5, Y: 12.5 });

        assert_eq!(after.x - before.x, px(3.75));
        assert_eq!(before.y - after.y, px(3.75));
    }

    #[test]
    fn player_moves_visibly_and_camera_follows_continuously() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)));
        let first = Vec2 { X: 18.0, Y: 0.0 };
        let camera = TrackPlayer(Vec2::Zero(), first);
        assert_eq!(camera, Vec2::Zero());
        let viewport = Viewport::New(bounds, camera);
        let first = viewport.Project(first);
        assert_eq!(first.x - viewport.Project(Vec2::Zero()).x, px(135.0));

        let next = Vec2 { X: 19.0, Y: 0.0 };
        let camera = TrackPlayer(camera, next);
        let viewport = Viewport::New(bounds, camera);
        assert_eq!(camera.X, 1.0);
        assert_eq!(viewport.Project(next), first);
        assert_eq!(viewport.Project(Vec2::Zero()).x, px(392.5));
    }
}
