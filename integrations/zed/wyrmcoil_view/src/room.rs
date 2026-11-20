use std::sync::Arc;
use std::time::Instant;

use gpui::{
    Canvas, Div, GpuViewportDraw, GpuViewportMesh, GpuViewportMeshInstance, GpuViewportShader,
    canvas,
};
use wyrmcoil::Demo::room::{
    NewRoomEngine, RoomCube, RoomInput, RoomMatrix, RoomObject, RoomSnapshot, RoomWorld,
};
use wyrmcoil::Engine::Engine;

use super::{BuildShader, FixedStep, MovementInput, Status};

pub(super) struct RoomPreview {
    engine: Engine<RoomWorld, RoomInput>,
    snapshot: RoomSnapshot,
    step: FixedStep,
    pub input: MovementInput,
    applied: RoomInput,
    shader: Result<Arc<GpuViewportShader>, String>,
}

impl RoomPreview {
    pub fn New() -> Self {
        let mut engine = NewRoomEngine();
        let snapshot = engine.RenderSnapshot();
        Self {
            engine,
            snapshot,
            step: FixedStep::default(),
            input: MovementInput::default(),
            applied: RoomInput::default(),
            shader: if cfg!(target_os = "windows") {
                BuildRoomShader().map(Arc::new)
            } else {
                Err("The 3D room requires the Windows D3D11 backend.".into())
            },
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
        use gpui::ParentElement;
        let snapshot = self.snapshot;
        let pose = snapshot.Camera;
        let guard = snapshot.Guard.Position;
        let mut status = Status(controls, self.engine.Clock(), snapshot.Frame).child(format!(
            "Eye ({:.2}, {:.2}, {:.2}) · yaw {:.2} · pitch {:.2} · guard x {:.2}",
            pose.Eye.x, pose.Eye.y, pose.Eye.z, pose.Yaw, pose.Pitch, guard.x
        ));
        if let Err(error) = &self.shader {
            status = status.child(error.clone());
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

fn Instances(snapshot: &RoomSnapshot, aspect: f32) -> Option<[GpuViewportMeshInstance; 8]> {
    let view_projection = snapshot.Camera.ViewProjection(aspect)?;
    let instance = |object: RoomObject| GpuViewportMeshInstance {
        clip_from_model: view_projection
            .Multiply(RoomMatrix::Model(object.Position, object.Scale))
            .0,
        color: object.Color,
    };
    let [a, b, c, back, left, right] = snapshot.Boxes;
    Some([
        instance(snapshot.Floor),
        instance(a),
        instance(b),
        instance(c),
        instance(back),
        instance(left),
        instance(right),
        instance(snapshot.Guard),
    ])
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
        let initial = preview.snapshot;
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
        let moved = preview.snapshot;
        assert!(moved.Camera.Eye.z < initial.Camera.Eye.z && moved.Camera.Yaw > initial.Camera.Yaw);
        preview.Suspend();
        assert!(!preview.Advance(start + Duration::from_secs(120)));
        assert!(preview.Advance(start + Duration::from_secs(120) + Duration::from_millis(17)));
        assert_eq!(preview.snapshot.Camera, moved.Camera);
        assert_ne!(preview.snapshot.Guard, moved.Guard);
        assert_eq!(RoomPreview::New().snapshot, initial);
    }
}
