use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Canvas, Div, GpuViewportDraw, GpuViewportMesh, GpuViewportMeshInstance,
    GpuViewportShader, ParentElement, Pixels, Styled, canvas, div,
};
use wyrmcoil::Demo::room::{
    BuiltinRoomScene, NewRoomEngineFromScene, RoomCube, RoomInput, RoomSnapshot, RoomWorld,
    TryNewRoomEngineFromScene,
};
use wyrmcoil::Engine::shader::sdslv::CompileCanonicalSourceToShaderArtifact;
use wyrmcoil::Engine::{Engine, EngineClock};
use wyrmcoil::GameRuntime::{GameSession, Observation, TacticalArt};
use wyrmcoil::Scene::{CompiledScene, ReadArtifact};

use super::tactical::{self, TacticalPreview};
use super::tactical_art::TacticalGraphics;
use super::{FixedStep, MovementInput, Status, ViewportShader};

pub(super) struct RoomPreview {
    engine: Engine<RoomWorld, RoomInput>,
    snapshot: RoomSnapshot,
    step: FixedStep,
    pub input: MovementInput,
    applied: RoomInput,
    shader: Result<Arc<GpuViewportShader>, String>,
    identity: String,
    notice: String,
    game: Option<GameSession>,
    game_started: Instant,
    game_notice: Option<String>,
    game_observation: Option<Observation>,
    game_clock: Option<EngineClock>,
    pub tactical: Option<TacticalPreview>,
    game_art: Option<(PathBuf, TacticalArt, Arc<TacticalGraphics>)>,
    art_error: Option<String>,
}

impl RoomPreview {
    pub fn New() -> Self {
        let scene = BuiltinRoomScene();
        let mut engine = NewRoomEngineFromScene(scene);
        let snapshot = engine.RenderSnapshot();
        Self {
            engine,
            snapshot,
            step: FixedStep::default(),
            input: MovementInput::default(),
            applied: RoomInput::default(),
            identity: scene.Identity().to_owned(),
            notice: format!("Built-in {} · {}", scene.Data().name, scene.Identity()),
            game: None,
            game_started: Instant::now(),
            game_notice: None,
            game_observation: None,
            game_clock: None,
            tactical: None,
            game_art: None,
            art_error: None,
            shader: if cfg!(target_os = "windows") {
                BuildRoomShader().map(Arc::new)
            } else {
                Err("The 3D room requires the Windows D3D11 backend.".into())
            },
        }
    }

    pub fn SetNotice(&mut self, notice: String) -> bool {
        if self.notice == notice {
            return false;
        }
        self.notice = notice;
        true
    }

    pub fn Reload(&mut self, path: Result<&Path, String>) -> bool {
        let start = Instant::now();
        let scene = match path.and_then(ReadArtifact) {
            Ok(scene) => scene,
            Err(error) => return self.ReloadError(&error),
        };
        let read_time = start.elapsed();
        if self.identity == scene.Identity() {
            return self.SetNotice(format!(
                "Scene already current · {} · {}",
                scene.Data().name,
                scene.Identity()
            ));
        }
        let mount_start = Instant::now();
        let engine = TryNewRoomEngineFromScene(&scene);
        self.Mount(&scene, engine, read_time, mount_start)
    }

    pub fn ReloadWatched(&mut self, path: &Path, saw_artifact: &mut bool) -> bool {
        if self.game_notice.is_some() {
            return false;
        }
        if matches!(path.try_exists(), Ok(false)) {
            if !*saw_artifact {
                return false;
            }
            return self.ReloadError(&format!(
                "Compiled artifact missing or deleted · {}",
                path.display()
            ));
        }
        *saw_artifact = true;
        self.Reload(Ok(path))
    }

    fn ReloadError(&mut self, error: &str) -> bool {
        self.SetNotice(format!(
            "Scene reload failed · {}",
            error.replace(['\r', '\n'], " ")
        ))
    }

    fn Mount(
        &mut self,
        scene: &CompiledScene,
        engine: Result<Engine<RoomWorld, RoomInput>, &'static str>,
        read_time: Duration,
        mount_start: Instant,
    ) -> bool {
        let mut engine = match engine {
            Ok(engine) => engine,
            Err(error) => return self.ReloadError(error),
        };
        let snapshot = engine.RenderSnapshot();
        self.engine = engine;
        self.snapshot = snapshot;
        self.applied = RoomInput::default();
        self.Suspend();
        self.identity = scene.Identity().to_owned();
        self.SetNotice(format!(
            "Scene loaded · {} · {} · read and validate {:.2} ms · mount {:.2} ms",
            scene.Data().name,
            scene.Identity(),
            read_time.as_secs_f64() * 1000.0,
            mount_start.elapsed().as_secs_f64() * 1000.0,
        ))
    }

    pub fn Advance(&mut self, now: Instant) -> bool {
        if self.game_notice.is_some() {
            return match self.AdvanceGame(now) {
                Ok(changed) => changed,
                Err(error) => {
                    self.GameError(&error);
                    true
                }
            };
        }
        let steps = self.step.Steps(now);
        for _ in 0..steps {
            let input = Navigation(self.input.TakeActive());
            if input != self.applied {
                self.engine.EnqueueInput(input);
                self.applied = input;
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

    pub fn StartGame(&mut self, root: Result<&Path, String>) {
        self.Suspend();
        self.game_started = Instant::now();
        match root.and_then(GameSession::Start) {
            Ok(game) => {
                self.game = Some(game);
                self.game_notice = Some("Starting game…".into());
            }
            Err(error) => self.GameError(&error),
        }
    }

    fn GameError(&mut self, error: &str) {
        self.game = None;
        let message = error.replace(['\r', '\n'], " ");
        log::warn!("WyrmCoil game: {message}");
        self.game_notice = Some(format!("Game failed · {message}"));
        self.Suspend();
    }

    pub fn StopGame(&mut self) {
        if self.game_notice.is_some() {
            self.game = None;
            self.game_notice = Some("Game stopped · Run Game to play".into());
            self.Suspend();
        }
    }

    pub fn UseRoom(&mut self) {
        if self.game_notice.take().is_some() {
            self.game = None;
            self.game_clock = None;
            self.game_observation = None;
            self.tactical = None;
            self.game_art = None;
            self.art_error = None;
            self.snapshot = self.engine.RenderSnapshot();
            self.Suspend();
        }
    }

    pub fn CanAdvance(&self) -> bool {
        self.game_notice.is_none()
            || self
                .game
                .as_ref()
                .is_some_and(|game| game.Busy() || game.Playing())
    }

    fn AdvanceGame(&mut self, now: Instant) -> Result<bool, String> {
        if let Some((root, art, graphics)) = &mut self.game_art
            && let Some(reason) = graphics.FallbackReason()
        {
            match TacticalGraphics::D3dFallback(root, art) {
                Ok(cpu) => {
                    *graphics = Arc::new(cpu);
                    self.art_error = Some(reason);
                }
                Err(error) => {
                    self.game_art = None;
                    self.art_error = Some(format!("{reason} · CPU art failed: {error}"));
                }
            }
            self.step.Pause();
            return Ok(true);
        }
        let Some(game) = &mut self.game else {
            return Ok(false);
        };
        let mut changed = game.Poll(Duration::ZERO)?;
        if game.Busy() {
            self.step.Pause();
            return Ok(false);
        }
        let mounting = self.game_notice.as_deref() == Some("Starting game…") && changed;
        let steps = self.step.Steps(now);
        let mut camera_changed = false;
        for _ in 0..if mounting { 0 } else { steps } {
            if !game.Playing() {
                break;
            }
            let navigation = Navigation(self.input.TakeActive());
            if game.Tactical().is_some() {
                let commands = if let Some(tactical) = &mut self.tactical {
                    camera_changed |= tactical.Pan(navigation.Strafe, navigation.Forward);
                    tactical.TakeCommands()
                } else {
                    Vec::new()
                };
                if commands.is_empty() && game.Tactical().is_some_and(|view| view.Paused) {
                    continue;
                }
                game.BeginTacticalTick(commands)?;
            } else {
                game.BeginTick(navigation)?;
            }
            if game.Poll(Duration::from_millis(1))? {
                changed = true;
            } else {
                self.step.Pause();
                break;
            }
        }
        if changed {
            if let Some(snapshot) = game.Snapshot() {
                self.snapshot = snapshot;
            }
            if let Some(view) = game.Tactical() {
                if mounting {
                    self.tactical = Some(TacticalPreview::New(view.clone(), self.snapshot.Camera));
                    if let Some(art) = game.TacticalArt() {
                        let root = game.Project();
                        let cached = self
                            .game_art
                            .as_ref()
                            .is_some_and(|(path, config, _)| path == root && config == art);
                        if !cached {
                            self.game_art = None;
                            self.art_error = None;
                            match TacticalGraphics::New(root, art, game.Name() == "the-last-watch")
                            {
                                Ok(graphics) => {
                                    self.game_art =
                                        Some((root.to_path_buf(), art.clone(), Arc::new(graphics)))
                                }
                                Err(error) => {
                                    log::warn!("WyrmCoil tactical art: {error}");
                                    self.art_error = Some(error);
                                }
                            }
                        }
                        log::info!(
                            "Tactical game mount in {:.2?}; art cache hit: {cached}",
                            self.game_started.elapsed()
                        );
                    } else {
                        self.game_art = None;
                        self.art_error = None;
                    }
                    self.step.Pause();
                } else if let Some(tactical) = &mut self.tactical {
                    tactical.view = view.clone();
                }
            } else {
                self.tactical = None;
                self.game_art = None;
                self.art_error = None;
            }
            self.game_clock = game.Clock();
            self.game_observation = game.Observation().cloned();
            self.game_notice = Some(format!("Game · {}", game.Name()));
        }
        Ok(changed || camera_changed)
    }

    pub fn Suspend(&mut self) {
        self.CancelInput();
        self.step.Pause();
    }

    pub fn CancelInput(&mut self) {
        self.input.Cancel();
        if let Some(tactical) = &mut self.tactical {
            tactical.Cancel();
        }
    }

    pub fn Draw(
        &self,
        controls: &'static str,
        record_bounds: impl FnOnce(Bounds<Pixels>, &mut App) + 'static,
    ) -> (Div, Canvas<()>) {
        let mut snapshot = self.snapshot.clone();
        let tactical_view = self.tactical.as_ref().map(|tactical| {
            snapshot.Camera = tactical.camera;
            tactical.view.clone()
        });
        let tactical_graphics = self
            .game_art
            .as_ref()
            .map(|(_, _, graphics)| graphics.clone());
        let shader = tactical_graphics
            .as_ref()
            .map(|graphics| graphics.shader.clone())
            .or_else(|| self.shader.as_ref().ok().cloned());
        let pose = snapshot.Camera;
        let guard = snapshot.Guard.Position;
        let notice = match (&self.game_notice, &self.game_observation) {
            (Some(notice), Some(game)) => format!(
                "{notice} · {:?} · {} remaining · {}",
                game.State, game.Remaining, game.Objective,
            ),
            (Some(notice), None) => notice.clone(),
            (None, _) => self.notice.clone(),
        };
        // Keep changing counters and status text from resizing the GPU target.
        let mut status = div()
            .flex_none()
            .child(
                Status(
                    controls,
                    self.game_clock.unwrap_or_else(|| self.engine.Clock()),
                    snapshot.Frame,
                )
                .flex_nowrap()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(format!(
                    "Eye ({:.2}, {:.2}, {:.2}) · yaw {:.2} · pitch {:.2} · guard ({:.2}, {:.2})",
                    pose.Eye.x, pose.Eye.y, pose.Eye.z, pose.Yaw, pose.Pitch, guard.x, guard.z
                )),
            )
            .child(div().truncate().px_3().pb_2().text_sm().child(notice));
        if let Err(error) = &self.shader {
            status = status.child(
                div()
                    .truncate()
                    .px_3()
                    .pb_2()
                    .text_sm()
                    .child(error.clone()),
            );
        }
        if let Some(view) = &tactical_view {
            status = tactical::Status(view, controls);
            if let Some(notice) = self.game_notice.as_ref() {
                if notice.starts_with("Game stopped")
                    || notice.starts_with("Game failed")
                    || notice.starts_with("Starting")
                {
                    status = status.child(notice.clone());
                }
            }
            if let Some(error) = &self.art_error {
                let message = if tactical_graphics.is_some() {
                    error.clone()
                } else {
                    format!("Art unavailable · diagnostic geometry · {error}")
                };
                status = status.child(div().truncate().px_3().text_sm().child(message));
            }
        }
        #[cfg(target_os = "windows")]
        if let Some(shader) = &shader {
            let backend = shader
                .producer
                .as_ref()
                .map(|producer| producer.status())
                .unwrap_or_else(|| {
                    if self
                        .art_error
                        .as_deref()
                        .is_some_and(|error| error.starts_with("D3D11 fallback"))
                    {
                        "D3D11 · CPU fallback".into()
                    } else {
                        "D3D11 · selected explicitly".into()
                    }
                });
            if let Some(graphics) = &tactical_graphics {
                let skin = graphics.SkinStatus(&backend);
                status = status.child(div().truncate().px_3().text_sm().child(skin));
            }
            status = status.child(div().truncate().px_3().pb_2().text_sm().child(backend));
        }
        (
            status,
            canvas(
                move |bounds, _, app| record_bounds(bounds, app),
                move |bounds, (), window, _| {
                    let Some(shader) = &shader else {
                        return;
                    };
                    let size = window.gpu_viewport_size(bounds);
                    if size.width.0 <= 0.0 || size.height.0 <= 0.0 {
                        return;
                    }
                    let aspect = size.width.0 / size.height.0;
                    if let Some(mut instances) = Instances(&snapshot, aspect) {
                        let projection = snapshot.Camera.ViewProjection(aspect);
                        if let (Some(view), Some(projection)) = (&tactical_view, projection) {
                            if let Some(graphics) = &tactical_graphics {
                                instances = graphics.Instances(view, projection);
                            } else {
                                instances.pop();
                                tactical::AppendInstances(view, projection, &mut instances);
                            }
                        }
                        window.paint_gpu_viewport(
                            bounds,
                            shader,
                            GpuViewportDraw::Mesh(Arc::from(instances)),
                        );
                        if let (Some(view), Some(projection)) = (&tactical_view, projection) {
                            tactical::PaintHealth(window, bounds, projection, view);
                        }
                    }
                },
            ),
        )
    }
}

fn Navigation(active: u8) -> RoomInput {
    let down = |bit: u32| i32::from((active & (1u8 << bit)) != 0u8);
    RoomInput {
        Strafe: down(6) - down(4),
        Forward: down(0) - down(2),
        Yaw: down(7) - down(5),
        Pitch: down(1) - down(3),
    }
}

fn BuildRoomShader() -> Result<GpuViewportShader, String> {
    let artifact = CompileCanonicalSourceToShaderArtifact("room.sdslv", include_str!("room.sdslv"))
        .map_err(|errors| format!("room.sdslv: {errors:?}"))?;
    let mut shader = ViewportShader(&artifact)?;
    let (vertices, indices) = RoomCube();
    shader.mesh = Some(GpuViewportMesh {
        vertices: vertices.to_vec(),
        indices: indices.to_vec(),
        first_instance_indices: 6,
    });
    #[cfg(target_os = "windows")]
    if !std::env::var("WYRMCOIL_GRAPHICS_BACKEND")
        .is_ok_and(|value| value.eq_ignore_ascii_case("d3d11"))
    {
        shader.producer = Some(Arc::new(super::vulkan::RoomProducer::New(artifact)));
    }
    Ok(shader)
}

fn Instances(snapshot: &RoomSnapshot, aspect: f32) -> Option<Vec<GpuViewportMeshInstance>> {
    let view_projection = snapshot.Camera.ViewProjection(aspect)?;
    let mut instances = Vec::with_capacity(snapshot.Boxes.len() + 2);
    for object in std::iter::once(&snapshot.Floor)
        .chain(snapshot.Boxes.iter())
        .chain(std::iter::once(&snapshot.Guard))
    {
        instances.push(GpuViewportMeshInstance {
            clip_from_model: view_projection.Multiply(object.Model).0,
            color: object.Color,
            indices: [0; 3],
        });
    }
    Some(instances)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn room_shader_and_instances_use_one_shared_indexed_mesh() {
        let shader = BuildRoomShader().unwrap();
        assert_eq!(shader.vertex_entry, "Room_VS");
        assert_eq!(shader.pixel_entry, "Room_PS");
        assert!(shader.source.contains("ColumnW : TEXCOORD4"));
        assert!(shader.source.contains("Color : TEXCOORD5"));
        let mesh = shader.mesh.unwrap();
        assert_eq!(
            (
                mesh.vertices.len(),
                mesh.indices.len(),
                mesh.first_instance_indices
            ),
            (24, 36, 6)
        );
        let room = RoomPreview::New();
        let instances = Instances(&room.snapshot, 1.7).unwrap();
        assert_eq!(instances.len(), 8);
        assert_eq!(instances[0].color, room.snapshot.Floor.Color);
        assert_eq!(instances[7].color, room.snapshot.Guard.Color);
        assert_ne!(instances[1].clip_from_model, instances[7].clip_from_model);
        assert!(Instances(&room.snapshot, 0.0).is_none());
    }

    #[test]
    fn room_input_latches_separate_walk_and_look_axes_and_pause_resets_time() {
        let mut preview = RoomPreview::New();
        let initial = preview.snapshot.clone();
        let start = Instant::now();
        let key = |name| gpui::Keystroke {
            key: name,
            modifiers: Default::default(),
            key_char: None,
        };
        let forward = key("w".to_owned());
        let yaw = key("right".to_owned());
        preview.input.KeyDown(&forward, false);
        preview.input.KeyDown(&yaw, false);
        assert!(!preview.Advance(start));
        assert!(preview.Advance(start + Duration::from_millis(17)));
        let moved = preview.snapshot.clone();
        assert!(moved.Camera.Eye.z < initial.Camera.Eye.z && moved.Camera.Yaw > initial.Camera.Yaw);
        preview.Suspend();
        assert!(!preview.Advance(start + Duration::from_secs(120)));
        assert!(preview.Advance(start + Duration::from_secs(120) + Duration::from_millis(17)));
        assert_eq!(preview.snapshot.Camera, moved.Camera);
        assert_ne!(preview.snapshot.Guard, moved.Guard);
        assert_eq!(RoomPreview::New().snapshot, initial);
    }

    #[test]
    fn room_reload_is_transactional_and_reuses_the_shader_for_changed_geometry() {
        use wyrmcoil::Scene::{BuildArtifact, Patrol, PlayerCamera, Scene, WalkBounds};

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("room-reload-test-{}.toml", std::process::id()));
        let mut room = RoomPreview::New();
        let shader = room.shader.as_ref().ok().cloned();
        let start = Instant::now();
        room.Advance(start);
        room.Advance(start + Duration::from_millis(17));
        let running = room.snapshot.clone();
        let clock = room.engine.Clock();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"broken scene").unwrap();
        room.Reload(Ok(&path));
        assert!(room.notice.contains("reload failed"));
        assert_eq!(room.engine.Clock(), clock);
        assert_eq!(room.snapshot, running);

        let document = Scene::World(
            "new-room",
            WalkBounds {
                x: [-10.0, 10.0],
                z: [-10.0, 10.0],
            },
            [
                Scene::Floor("floor", [20.0; 2], [0.3; 3]),
                Scene::Player(
                    "player",
                    PlayerCamera {
                        eye_height: 1.65,
                        pitch: -0.13,
                        vertical_fov: 1.0,
                        near: 0.1,
                        far: 70.0,
                    },
                )
                .At(2.0, 0.0, 8.0),
                Scene::Guard(
                    "guard",
                    [1.0; 3],
                    [1.0, 0.5, 0.0],
                    Patrol {
                        half_span: 2.0,
                        half_period_ticks: 60,
                        phase_ticks: 30,
                    },
                )
                .At(0.0, 0.5, -2.0),
                Scene::Box("purple-crate", [1.0; 3], [0.65, 0.15, 1.0]).At(-2.0, 0.5, 0.0),
            ],
        );
        let compiled = BuildArtifact(&document, &path).unwrap();
        let mut saw_artifact = false;
        assert!(room.ReloadWatched(&path, &mut saw_artifact));
        assert!(saw_artifact);
        assert_eq!(room.identity, compiled.Identity());
        assert_eq!(room.snapshot.Camera.Eye.x, 2.0);
        assert_eq!(room.applied, RoomInput::default());
        let instances = Instances(&room.snapshot, 1.7).unwrap();
        assert_eq!(instances.len(), 3);
        assert_eq!(instances[1].color, [0.65, 0.15, 1.0, 1.0]);
        assert_eq!(
            instances[1].clip_from_model,
            room.snapshot
                .Camera
                .ViewProjection(1.7)
                .unwrap()
                .Multiply(compiled.Data().boxes[0].model)
                .0,
        );
        if let Some(shader) = shader {
            assert!(Arc::ptr_eq(&shader, room.shader.as_ref().unwrap()));
        }

        assert!(!room.Advance(start + Duration::from_secs(20)));
        assert!(room.Advance(start + Duration::from_secs(20) + Duration::from_millis(17)));
        let mounted = room.snapshot.clone();
        room.Reload(Ok(&path));
        assert!(room.notice.contains("already current"));
        assert_eq!(room.snapshot, mounted);
        assert!(!room.ReloadWatched(&path, &mut saw_artifact));
        assert_eq!(room.snapshot, mounted);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn watched_file_probe_handles_first_publish_truncation_removal_and_recovery() {
        use wyrmcoil::Scene::EncodeArtifact;

        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("watch-probe-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("wyrmcoil-scene.toml");
        let temporary = directory.join("wyrmcoil-scene.tmp");
        let mut room = RoomPreview::New();
        let mut saw_artifact = false;
        let start = Instant::now();
        room.Advance(start);
        room.Advance(start + Duration::from_millis(17));
        let clock = room.engine.Clock();
        let snapshot = room.snapshot.clone();

        assert!(!room.ReloadWatched(&path, &mut saw_artifact));
        assert!(!saw_artifact);
        let text = EncodeArtifact(BuiltinRoomScene()).unwrap();
        std::fs::write(&temporary, &text).unwrap();
        std::fs::rename(&temporary, &path).unwrap();
        assert!(room.ReloadWatched(&path, &mut saw_artifact));
        assert!(saw_artifact);
        assert!(room.notice.contains("already current"));

        std::fs::write(&path, b"version = ").unwrap();
        assert!(room.ReloadWatched(&path, &mut saw_artifact));
        assert!(room.notice.contains("invalid scene artifact"));
        assert!(!room.ReloadWatched(&path, &mut saw_artifact));
        std::fs::remove_file(&path).unwrap();
        assert!(room.ReloadWatched(&path, &mut saw_artifact));
        assert!(room.notice.contains("missing or deleted"));
        assert!(!room.ReloadWatched(&path, &mut saw_artifact));
        assert!(room.ReloadWatched(&directory, &mut saw_artifact));
        assert!(room.notice.contains("reload failed"));

        std::fs::write(&temporary, &text).unwrap();
        std::fs::rename(&temporary, &path).unwrap();
        assert!(room.ReloadWatched(&path, &mut saw_artifact));
        assert!(room.notice.contains("already current"));
        assert_eq!(room.engine.Clock(), clock);
        assert_eq!(room.snapshot, snapshot);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn failed_engine_construction_leaves_the_running_room_intact() {
        use wyrmcoil::{DwFrameId, DwFrameRegistry, DwMessage};

        let mut room = RoomPreview::New();
        let start = Instant::now();
        room.Advance(start);
        room.Advance(start + Duration::from_millis(17));
        let running = room.snapshot.clone();
        let clock = room.engine.Clock();
        let engine = Engine::TryConstruct(
            DwFrameRegistry::New,
            DwFrameId {
                Domain: 0,
                Local: 0,
            },
            room.engine.World.clone(),
            |_| DwMessage::PairI32(0, 0, 0),
        );
        assert!(room.Mount(BuiltinRoomScene(), engine, Duration::ZERO, Instant::now()));
        assert!(room.notice.contains("root frame not found"));
        assert_eq!(room.engine.Clock(), clock);
        assert_eq!(room.snapshot, running);
        assert_eq!(room.identity, BuiltinRoomScene().Identity());
    }
}
