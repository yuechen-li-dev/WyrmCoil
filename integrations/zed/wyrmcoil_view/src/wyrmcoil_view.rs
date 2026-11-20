#![allow(non_snake_case)]

use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Pixels, Point, Render, SharedString, Styled, Subscription, Window,
    WindowId, actions, canvas, div, fill, point, px, rgb, size,
};
use workspace::{Item, MultiWorkspace, Workspace};
use wyrmcoil::Demo::{DemoWorld, InputEvent};
use wyrmcoil::Engine::{Engine, EntityId, RenderSnapshot, Vec2};

actions!(
    wyrmcoil,
    [
        /// Open the WyrmCoil Preview tab.
        OpenPreview
    ]
);

pub fn Init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &OpenPreview, window, cx| {
            if let Some(preview) = workspace.item_of_type::<Preview>(cx) {
                workspace.activate_item(&preview, true, true, window, cx);
                return;
            }

            let preview = cx.new(Preview::New);
            workspace.add_item_to_active_pane(Box::new(preview), None, true, window, cx);
        });
    })
    .detach();
}

struct Preview {
    focus: FocusHandle,
    title: SharedString,
    session: PreviewSession,
    pending_frame: Option<WindowId>,
    window_observers: Option<[Subscription; 2]>,
}

impl Preview {
    fn New(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            title: format!("{} Preview", wyrmcoil::ProjectName()).into(),
            session: PreviewSession::New(),
            pending_frame: None,
            window_observers: None,
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
        if !selected || !window.is_window_active() || !window.is_visible() {
            self.session.step.Pause();
            return;
        }

        if self.session.Advance(Instant::now()) {
            cx.notify();
        } else {
            // Keep the clock live above 60 Hz without repainting an unchanged snapshot.
            self.RequestFrame(window, cx);
        }
    }

    fn ResetTiming(&mut self, cx: &mut Context<Self>) {
        self.session.step.Pause();
        cx.notify();
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
}

impl PreviewSession {
    fn New() -> Self {
        let mut engine = Engine::New();
        let snapshot = engine.RenderSnapshot();
        Self {
            engine,
            snapshot,
            step: FixedStep::default(),
        }
    }

    fn Advance(&mut self, now: Instant) -> bool {
        let steps = self.step.Steps(now);
        for _ in 0..steps {
            self.engine.TickControl();
            self.engine.TickSimulation();
        }
        if steps == 0 {
            return false;
        }
        self.snapshot = self.engine.RenderSnapshot();
        true
    }
}

impl Focusable for Preview {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if window.is_window_active() && window.is_visible() {
            self.RequestFrame(window, cx);
        }

        let clock = self.session.engine.Clock();
        let player = self.session.engine.Player;
        let guard = self.session.engine.Guard;
        let snapshot = self.session.snapshot.clone();
        let mut status = div()
            .flex()
            .flex_wrap()
            .gap_x_4()
            .px_3()
            .py_2()
            .text_sm()
            .child(format!(
                "Control {}   Simulation {}   Snapshot {}",
                clock.ControlTick, clock.SimulationTick, snapshot.Frame
            ));
        for item in &snapshot.Items {
            let (name, color) = Appearance(item.Entity, player, guard);
            status = status.child(div().text_color(rgb(color)).child(format!(
                "{} #{} · sprite {} · ({:.1}, {:.1})",
                name, item.Entity.0, item.SpriteId, item.Position.X, item.Position.Y
            )));
        }

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(0x151b26))
            .text_color(rgb(0xe7edf6))
            .child(status)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| {
                        let viewport = Viewport::Fit(bounds, &snapshot);
                        for item in &snapshot.Items {
                            let (_, color) = Appearance(item.Entity, player, guard);
                            let center = viewport.Project(item.Position);
                            let side = viewport.marker_size;
                            let origin = point(center.x - side * 0.5, center.y - side * 0.5);
                            window.paint_quad(fill(
                                Bounds::new(origin, size(side, side)),
                                rgb(color),
                            ));
                        }
                    },
                )
                .flex_1()
                .w_full(),
            )
    }
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

struct Viewport {
    origin: Point<Pixels>,
    scale: Pixels,
    marker_size: Pixels,
}

impl Viewport {
    fn Fit(bounds: Bounds<Pixels>, snapshot: &RenderSnapshot) -> Self {
        let mut extent = 0.0_f32;
        for item in &snapshot.Items {
            extent = extent.max(item.Position.X.abs()).max(item.Position.Y.abs());
        }
        // Keep both markers in view as the guard travels without snapping between zoom levels.
        let half_span = extent + 18.0;
        let side = bounds.size.width.min(bounds.size.height);
        Self {
            origin: bounds.center(),
            scale: side * (0.42 / half_span),
            marker_size: px(16.0).min(side * 0.12),
        }
    }

    fn Project(&self, position: Vec2) -> Point<Pixels> {
        point(
            self.origin.x + self.scale * position.X,
            self.origin.y - self.scale * position.Y,
        )
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
        self.session.step.Pause();
        self.window_observers = Some([
            cx.observe_window_activation(window, |preview, _, cx| preview.ResetTiming(cx)),
            cx.observe_window_visibility(window, |preview, _, _, cx| preview.ResetTiming(cx)),
        ]);
    }

    fn deactivated(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.session.step.Pause();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn viewport_keeps_origin_and_fits_world_positions_after_resize() {
        let mut session = PreviewSession::New();
        let bounds = Bounds::new(point(px(40.0), px(60.0)), size(px(300.0), px(200.0)));
        let viewport = Viewport::Fit(bounds, &session.snapshot);
        assert_eq!(viewport.Project(Vec2::Zero()), point(px(190.0), px(160.0)));
        let guard = session.snapshot.Items[1].Position;
        let projected = viewport.Project(guard);
        let offset = px(420.0 / 23.0);
        assert_eq!(projected, point(px(190.0) + offset, px(160.0) - offset));

        session.snapshot.Items[1].Position = Vec2 {
            X: 1_000.0,
            Y: 1_000.0,
        };
        let narrow = Bounds::new(point(px(40.0), px(60.0)), size(px(96.0), px(240.0)));
        let viewport = Viewport::Fit(narrow, &session.snapshot);
        let projected = viewport.Project(session.snapshot.Items[1].Position);
        assert!(narrow.contains(&projected));
        assert!(projected.x > narrow.center().x && projected.y < narrow.center().y);
    }

    #[test]
    fn guard_keeps_moving_forward_across_the_old_zoom_threshold() {
        let mut session = PreviewSession::New();
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)));
        session.snapshot.Items[1].Position = Vec2 { X: 12.0, Y: 12.0 };
        let before =
            Viewport::Fit(bounds, &session.snapshot).Project(session.snapshot.Items[1].Position);
        session.snapshot.Items[1].Position = Vec2 { X: 12.5, Y: 12.5 };
        let after =
            Viewport::Fit(bounds, &session.snapshot).Project(session.snapshot.Items[1].Position);

        assert!(after.x > before.x && after.x - before.x < px(3.0));
        assert!(after.y < before.y && before.y - after.y < px(3.0));
    }
}
