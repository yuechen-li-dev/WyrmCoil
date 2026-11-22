#![allow(non_snake_case)]

use std::sync::{Arc, OnceLock};

use crate::Engine::{Engine, World};
pub use crate::Scene::SceneMatrix as RoomMatrix;
use crate::Scene::{CompileScene, CompiledScene, Patrol, PlacedMesh, WalkBounds};
use crate::{
    Dw, DwActId, DwActRequest, DwBoard, DwControl, DwFrameCtx, DwFrameDef, DwFrameId,
    DwFrameRegistry, DwKey, DwMessage, DwMessagePayload,
};
pub use margaret_core::math::Vec3;

use crate::Scene as scene_authoring;
#[path = "../../examples/games/training-room/src/scene.rs"]
mod training_room;

const FRAME: DwFrameId = DwFrameId {
    Domain: 0x524f4f4d,
    Local: 1,
};
const NAVIGATE: DwActId = DwActId {
    Domain: FRAME.Domain,
    Local: 1,
};
const INPUT_KIND: u32 = 1;
const MOVE: DwKey<i32> = DwKey::New("RoomMove", 0);
const LOOK: DwKey<i32> = DwKey::New("RoomLook", 1);
const STEP: f32 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomInput {
    pub Strafe: i32,
    pub Forward: i32,
    pub Yaw: i32,
    pub Pitch: i32,
}

fn PackAxes(x: i32, y: i32) -> i32 {
    (x.signum() + 1) + 3 * (y.signum() + 1)
}

fn InputMessage(input: RoomInput) -> DwMessage {
    DwMessage::PairI32(
        INPUT_KIND,
        PackAxes(input.Strafe, input.Forward),
        PackAxes(input.Yaw, input.Pitch),
    )
}

fn Control(ctx: &mut DwFrameCtx) -> DwControl {
    while let Some(message) = ctx.MailboxMut().ConsumeFirstKind(INPUT_KIND) {
        if let DwMessagePayload::PairI32 {
            A: movement,
            B: look,
        } = message.Payload
        {
            ctx.BoardMut()
                .Set(MOVE, movement)
                .expect("room movement key");
            ctx.BoardMut().Set(LOOK, look).expect("room look key");
            ctx.Immediate(NAVIGATE);
        }
    }
    Dw::Stay()
}

fn Registry() -> DwFrameRegistry {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: FRAME,
            Step: Control,
            DebugName: "RoomNavigation",
        })
        .expect("room control frame registers once");
    registry
}

pub fn NewRoomEngine() -> Engine<RoomWorld, RoomInput> {
    NewRoomEngineFromScene(BuiltinRoomScene())
}

pub fn BuiltinRoomScene() -> &'static CompiledScene {
    static ROOM: OnceLock<CompiledScene> = OnceLock::new();
    ROOM.get_or_init(|| {
        CompileScene(&training_room::BuildScene()).expect("valid built-in training room")
    })
}

pub fn NewRoomEngineFromScene(scene: &CompiledScene) -> Engine<RoomWorld, RoomInput> {
    TryNewRoomEngineFromScene(scene).expect("room control session should construct")
}

pub fn TryNewRoomEngineFromScene(
    scene: &CompiledScene,
) -> Result<Engine<RoomWorld, RoomInput>, &'static str> {
    let scene = scene.Data();
    let [x, y, z] = scene.player.eye;
    let camera = scene.player.camera;
    Engine::TryConstruct(
        Registry,
        FRAME,
        RoomWorld {
            camera: RoomCamera {
                Eye: Vec3::New(x, y, z),
                Yaw: scene.player.yaw,
                Pitch: camera.pitch,
                VerticalFov: camera.vertical_fov,
                Near: camera.near,
                Far: camera.far,
            },
            input: RoomInput::default(),
            bounds: scene.bounds,
            floor: RoomObject::FromMesh(&scene.floor),
            boxes: scene.boxes.iter().map(RoomObject::FromMesh).collect(),
            guard: RoomObject::FromMesh(&scene.guard.body),
            patrol: scene.guard.patrol,
            patrol_tick: scene.guard.patrol.phase_ticks,
        },
        InputMessage,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomCamera {
    pub Eye: Vec3,
    pub Yaw: f32,
    pub Pitch: f32,
    pub VerticalFov: f32,
    pub Near: f32,
    pub Far: f32,
}

impl RoomCamera {
    pub fn Forward(&self) -> Vec3 {
        let (yaw_sin, yaw_cos) = self.Yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.Pitch.sin_cos();
        Vec3::New(yaw_sin * pitch_cos, pitch_sin, -yaw_cos * pitch_cos)
    }

    pub fn Right(&self) -> Vec3 {
        let (sin, cos) = self.Yaw.sin_cos();
        Vec3::New(cos, 0.0, sin)
    }

    pub fn ViewProjection(&self, aspect: f32) -> Option<RoomMatrix> {
        if ![
            self.Eye.x,
            self.Eye.y,
            self.Eye.z,
            self.Yaw,
            self.Pitch,
            self.VerticalFov,
            self.Near,
            self.Far,
            aspect,
        ]
        .into_iter()
        .all(f32::is_finite)
            || self.Pitch.abs() >= std::f32::consts::FRAC_PI_2
            || !(0.0 < self.VerticalFov && self.VerticalFov < std::f32::consts::PI)
            || !(0.0 < self.Near && self.Near < self.Far && aspect > 0.0)
        {
            return None;
        }
        let right = self.Right();
        let forward = self.Forward();
        let up = right.Cross(forward);
        let back = -forward;
        let eye = self.Eye;
        let view = RoomMatrix([
            [right.x, up.x, back.x, 0.0],
            [right.y, up.y, back.y, 0.0],
            [right.z, up.z, back.z, 0.0],
            [-right.Dot(eye), -up.Dot(eye), -back.Dot(eye), 1.0],
        ]);
        let focal = 1.0 / (self.VerticalFov * 0.5).tan();
        let depth = self.Near / (self.Far - self.Near);
        let projection = RoomMatrix([
            [focal / aspect, 0.0, 0.0, 0.0],
            [0.0, focal, 0.0, 0.0],
            [0.0, 0.0, depth, -1.0],
            [0.0, 0.0, depth * self.Far, 0.0],
        ]);
        let matrix = projection.Multiply(view);
        matrix
            .0
            .iter()
            .flatten()
            .all(|value| value.is_finite())
            .then_some(matrix)
    }
}

impl RoomMatrix {
    pub fn Model(position: Vec3, scale: Vec3) -> Self {
        Self::Placement(
            [position.x, position.y, position.z],
            [scale.x, scale.y, scale.z],
            0.0,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoomObject {
    pub Id: Arc<str>,
    pub Position: Vec3,
    pub Scale: Vec3,
    pub Model: RoomMatrix,
    pub Color: [f32; 4],
}

impl RoomObject {
    fn FromMesh(mesh: &PlacedMesh) -> Self {
        let [x, y, z, position] = mesh.model.0;
        let length = |[x, y, z, _]: [f32; 4]| Vec3::New(x, y, z).Length();
        let [r, g, b] = mesh.color;
        Self {
            Id: Arc::from(mesh.id.as_str()),
            Position: Vec3::New(position[0], position[1], position[2]),
            Scale: Vec3::New(length(x), length(y), length(z)),
            Model: mesh.model,
            Color: [r, g, b, 1.0],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoomSnapshot {
    pub Frame: u64,
    pub Camera: RoomCamera,
    pub Floor: RoomObject,
    pub Boxes: Arc<[RoomObject]>,
    pub Guard: RoomObject,
}

#[derive(Clone)]
pub struct RoomWorld {
    camera: RoomCamera,
    input: RoomInput,
    bounds: WalkBounds,
    floor: RoomObject,
    boxes: Arc<[RoomObject]>,
    guard: RoomObject,
    patrol: Patrol,
    patrol_tick: u32,
}

impl World for RoomWorld {
    type Chunk = Self;
    type Snapshot = RoomSnapshot;

    fn RefreshBoard(&self, _: &mut DwBoard) {}

    fn DispatchActs(&mut self, board: &DwBoard, acts: &[DwActRequest]) {
        if acts.iter().any(|act| act.Id == NAVIGATE) {
            let movement = board.GetOr(MOVE, 4);
            let look = board.GetOr(LOOK, 4);
            self.input = RoomInput {
                Strafe: movement % 3 - 1,
                Forward: movement / 3 - 1,
                Yaw: look % 3 - 1,
                Pitch: look / 3 - 1,
            };
        }
    }

    fn Tick(&mut self) {
        let input = self.input;
        self.camera.Yaw =
            (self.camera.Yaw + input.Yaw as f32 * (1.5 * STEP)).rem_euclid(std::f32::consts::TAU);
        self.camera.Pitch =
            (self.camera.Pitch + input.Pitch as f32 * (1.2 * STEP)).clamp(-1.42, 1.42);
        let right = self.camera.Right();
        let forward = Vec3::New(right.z, 0.0, -right.x);
        let direction = right * input.Strafe as f32 + forward * input.Forward as f32;
        self.camera.Eye = self.camera.Eye + direction.Normalized() * (3.2 * STEP);
        self.camera.Eye.x = self.camera.Eye.x.clamp(self.bounds.x[0], self.bounds.x[1]);
        self.camera.Eye.z = self.camera.Eye.z.clamp(self.bounds.z[0], self.bounds.z[1]);
        self.patrol_tick = (self.patrol_tick + 1) % (self.patrol.half_period_ticks * 2);
    }

    fn ExtractRenderSnapshot(&self, frame: u64) -> RoomSnapshot {
        let half_period = self.patrol.half_period_ticks;
        let tick = self.patrol_tick.min(half_period * 2 - self.patrol_tick);
        let offset = (tick as f32 / (half_period as f32 * 0.5) - 1.0) * self.patrol.half_span;
        let mut guard = self.guard.clone();
        guard.Position.x += offset;
        guard.Model.0[3][0] = guard.Position.x;
        RoomSnapshot {
            Frame: frame,
            Camera: self.camera,
            Floor: self.floor.clone(),
            Boxes: self.boxes.clone(),
            Guard: guard,
        }
    }

    fn ExportChunk(&self) -> Self {
        self.clone()
    }
    fn FromChunk(chunk: Self) -> Self {
        chunk
    }
}

/// xyz and a face brightness. The first six indices draw only the top face.
pub fn RoomCube() -> ([[f32; 4]; 24], [u16; 36]) {
    let faces = [
        (Vec3::Y, Vec3::X, -Vec3::Z, 1.0),
        (-Vec3::Y, Vec3::X, Vec3::Z, 0.58),
        (Vec3::X, -Vec3::Z, Vec3::Y, 0.76),
        (-Vec3::X, Vec3::Z, Vec3::Y, 0.69),
        (Vec3::Z, Vec3::X, Vec3::Y, 0.88),
        (-Vec3::Z, -Vec3::X, Vec3::Y, 0.72),
    ];
    let mut vertices = [[0.0; 4]; 24];
    let mut indices = [0; 36];
    for (face, (normal, right, up, shade)) in faces.into_iter().enumerate() {
        let base = face * 4;
        for (corner, (x, y)) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
            .into_iter()
            .enumerate()
        {
            let position = normal * 0.5 + right * x + up * y;
            vertices[base + corner] = [position.x, position.y, position.z, shade];
        }
        indices[face * 6..face * 6 + 6]
            .copy_from_slice(&[0, 1, 2, 0, 2, 3].map(|i| i + base as u16));
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn Near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }

    #[test]
    fn camera_axes_and_reversed_perspective_are_consistent() {
        let mut pose = NewRoomEngine().RenderSnapshot().Camera;
        pose.Eye = Vec3::ZERO;
        pose.Pitch = 0.0;
        assert_eq!(pose.Forward(), -Vec3::Z);
        assert_eq!(pose.Right(), Vec3::X);
        pose.Yaw = std::f32::consts::FRAC_PI_2;
        Near(pose.Forward().x, 1.0);
        Near(pose.Right().z, 1.0);
        pose.Yaw = 0.0;
        let square = pose.ViewProjection(1.0).unwrap();
        let wide = pose.ViewProjection(2.0).unwrap();
        let near = square.Transform([0.0, 0.0, -pose.Near, 1.0]);
        let far = square.Transform([0.0, 0.0, -pose.Far, 1.0]);
        Near(near[2] / near[3], 1.0);
        Near(far[2] / far[3], 0.0);
        let before = square.Transform([0.0, 0.0, -0.5 * pose.Near, 1.0]);
        assert!(before[2] > before[3]);
        assert!(square.Transform([0.0, 0.0, -2.0 * pose.Far, 1.0])[2] < 0.0);
        let point = square.Transform([1.0, 1.0, -4.0, 1.0]);
        let distant = square.Transform([1.0, 1.0, -8.0, 1.0]);
        let wide_point = wide.Transform([1.0, 1.0, -4.0, 1.0]);
        Near(point[0] / point[3], 2.0 * distant[0] / distant[3]);
        Near(point[0], 2.0 * wide_point[0]);
        Near(point[1], wide_point[1]);
    }

    #[test]
    fn object_world_and_clip_transforms_compose() {
        let mut pose = NewRoomEngine().RenderSnapshot().Camera;
        pose.Eye = Vec3::New(3.0, 2.0, 1.0);
        pose.Yaw = 0.4;
        pose.Pitch = 0.2;
        let position = pose.Eye + pose.Forward() * 6.0;
        let vp = pose.ViewProjection(1.8).unwrap();
        let model = RoomMatrix::Model(position, Vec3::New(2.0, 3.0, 4.0));
        let point = [0.5, -0.5, 0.5, 1.0];
        let world = model.Transform(point);
        Near(world[0], position.x + 1.0);
        Near(world[1], position.y - 1.5);
        Near(world[2], position.z + 2.0);
        for (composed, sequential) in vp
            .Multiply(model)
            .Transform(point)
            .into_iter()
            .zip(vp.Transform(world))
        {
            Near(composed, sequential);
        }
        let center = vp.Multiply(model).Transform([0.0, 0.0, 0.0, 1.0]);
        Near(center[0], 0.0);
        Near(center[1], 0.0);
        Near(center[3], 6.0);
    }

    #[test]
    fn invalid_or_nonfinite_camera_inputs_are_rejected() {
        let camera = NewRoomEngine().RenderSnapshot().Camera;
        assert!(camera.ViewProjection(0.0).is_none());
        assert!(camera.ViewProjection(f32::NAN).is_none());
        assert!(
            RoomCamera {
                Far: camera.Near,
                ..camera
            }
            .ViewProjection(1.0)
            .is_none()
        );
        assert!(
            RoomCamera {
                Near: -1.0,
                ..camera
            }
            .ViewProjection(1.0)
            .is_none()
        );
        assert!(
            RoomCamera {
                VerticalFov: std::f32::consts::PI,
                ..camera
            }
            .ViewProjection(1.0)
            .is_none()
        );
        assert!(
            RoomCamera {
                Pitch: std::f32::consts::FRAC_PI_2,
                ..camera
            }
            .ViewProjection(1.0)
            .is_none()
        );
        assert!(
            RoomCamera {
                Eye: Vec3::New(f32::INFINITY, 0.0, 0.0),
                ..camera
            }
            .ViewProjection(1.0)
            .is_none()
        );
    }

    #[test]
    fn indexed_cube_faces_point_outward_and_floor_is_upward() {
        let (vertices, indices) = RoomCube();
        let position = |index: u16| {
            let [x, y, z, _] = vertices[usize::from(index)];
            Vec3::New(x, y, z)
        };
        assert_eq!((vertices.len(), indices.len()), (24, 36));
        for face in indices.chunks_exact(3) {
            assert!(
                face.iter()
                    .all(|&index| usize::from(index) < vertices.len())
            );
            let a = position(face[0]);
            let b = position(face[1]);
            let c = position(face[2]);
            assert!((b - a).Cross(c - a).Dot((a + b + c) / 3.0) > 0.0);
        }
        assert!(indices[..6].iter().all(|&index| position(index).y == 0.5));
    }

    #[test]
    fn room_replays_navigation_and_guard_on_fixed_ticks() {
        let mut first = NewRoomEngine();
        let mut second = NewRoomEngine();
        let initial = first.RenderSnapshot();
        assert_eq!(initial, second.RenderSnapshot());
        let press = RoomInput {
            Strafe: 1,
            Forward: 1,
            Yaw: 0,
            Pitch: 0,
        };
        first.EnqueueInput(press);
        second.EnqueueInput(press);
        for _ in 0..60 {
            first.TickControl();
            first.TickSimulation();
            second.TickControl();
            second.TickSimulation();
        }
        let moved = first.RenderSnapshot();
        assert_eq!(moved, second.RenderSnapshot());
        Near((moved.Camera.Eye - initial.Camera.Eye).Length(), 3.2);
        assert_ne!(moved.Guard, initial.Guard);
        assert_eq!(moved.Boxes, initial.Boxes);
        first.EnqueueInput(RoomInput {
            Yaw: 1,
            Pitch: 1,
            ..RoomInput::default()
        });
        for _ in 0..120 {
            first.TickControl();
            first.TickSimulation();
        }
        let turned = first.RenderSnapshot();
        assert_eq!(turned.Camera.Eye, moved.Camera.Eye);
        assert_eq!(turned.Camera.Pitch, 1.42);
        Near(turned.Camera.Yaw, 3.0);
        assert_eq!(turned.Frame, 3);
        assert_eq!(NewRoomEngine().RenderSnapshot(), initial);
    }
}
