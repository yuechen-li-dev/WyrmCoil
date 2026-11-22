use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    Canvas, Div, GpuViewportDraw, GpuViewportMesh, GpuViewportMeshInstance, GpuViewportShader,
    ParentElement, Styled, canvas, div,
};
use wyrmcoil::Demo::room::{
    BuiltinRoomScene, NewRoomEngineFromScene, RoomCube, RoomInput, RoomSnapshot, RoomWorld,
};
use wyrmcoil::Engine::Engine;
use wyrmcoil::Scene::ReadArtifact;

use super::{BuildShader, FixedStep, MovementInput, Status};

pub(super) struct RoomPreview {
    engine: Engine<RoomWorld, RoomInput>,
    snapshot: RoomSnapshot,
    step: FixedStep,
    pub input: MovementInput,
    applied: RoomInput,
    shader: Result<Arc<GpuViewportShader>, String>,
    identity: String,
    notice: String,
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
            shader: if cfg!(target_os = "windows") {
                BuildRoomShader().map(Arc::new)
            } else {
                Err("The 3D room requires the Windows D3D11 backend.".into())
            },
        }
    }

    pub fn Reload(&mut self, path: Result<&Path, String>) {
        let start = Instant::now();
        match path.and_then(ReadArtifact) {
            Err(error) => {
                self.notice = format!("Scene reload failed · {}", error.replace(['\r', '\n'], " "))
            }
            Ok(scene) => {
                if self.identity == scene.Identity() {
                    self.notice = format!(
                        "Scene already current · {} · {}",
                        scene.Data().name,
                        scene.Identity()
                    );
                    return;
                }
                let mut engine = NewRoomEngineFromScene(&scene);
                let snapshot = engine.RenderSnapshot();
                self.engine = engine;
                self.snapshot = snapshot;
                self.applied = RoomInput::default();
                self.Suspend();
                self.identity = scene.Identity().to_owned();
                self.notice = format!(
                    "Scene loaded · {} · {} · {:.2} ms",
                    scene.Data().name,
                    scene.Identity(),
                    start.elapsed().as_secs_f64() * 1000.0
                );
            }
        }
    }

    pub fn Advance(&mut self, now: Instant) -> bool {
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

    pub fn Suspend(&mut self) {
        self.input.Cancel();
        self.step.Pause();
    }

    pub fn Draw(&self, controls: &'static str) -> (Div, Canvas<()>) {
        let snapshot = self.snapshot.clone();
        let pose = snapshot.Camera;
        let guard = snapshot.Guard.Position;
        // Keep changing counters and status text from resizing the GPU target.
        let mut status = div()
            .flex_none()
            .child(
                Status(controls, self.engine.Clock(), snapshot.Frame)
                    .flex_nowrap()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(format!(
                        "Eye ({:.2}, {:.2}, {:.2}) · yaw {:.2} · pitch {:.2} · guard x {:.2}",
                        pose.Eye.x, pose.Eye.y, pose.Eye.z, pose.Yaw, pose.Pitch, guard.x
                    )),
            )
            .child(
                div()
                    .truncate()
                    .px_3()
                    .pb_2()
                    .text_sm()
                    .child(self.notice.clone()),
            );
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
        let shader = self.shader.as_ref().ok().cloned();
        (
            status,
            canvas(
                |_, _, _| (),
                move |bounds, (), window, _| {
                    let Some(shader) = &shader else {
                        return;
                    };
                    let size = window.gpu_viewport_size(bounds);
                    if size.width.0 <= 0.0 || size.height.0 <= 0.0 {
                        return;
                    }
                    if let Some(instances) = Instances(&snapshot, size.width.0 / size.height.0) {
                        window.paint_gpu_viewport(
                            bounds,
                            shader,
                            GpuViewportDraw::Mesh(Arc::from(instances)),
                        );
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
    let mut shader = BuildShader("room.sdslv", include_str!("room.sdslv"))?;
    let (vertices, indices) = RoomCube();
    shader.mesh = Some(GpuViewportMesh {
        vertices: vertices.to_vec(),
        indices: indices.to_vec(),
        first_instance_indices: 6,
    });
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
        room.Reload(Ok(&path));
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
        std::fs::remove_file(path).unwrap();
    }
}
