#![allow(non_snake_case)]

mod artifact;
mod compile;

pub use artifact::{ARTIFACT_PATH, BuildArtifact, DecodeArtifact, EncodeArtifact, ReadArtifact};
pub use compile::{CompileScene, CompiledScene, PlacedMesh, SceneData, SceneMatrix};
use serde::{Deserialize, Serialize};

/// Immutable authoring data. Construct new documents to change a scene.
pub struct SceneDocument {
    name: String,
    bounds: WalkBounds,
    nodes: Vec<SceneNode>,
}

#[derive(Clone)]
pub struct SceneNode {
    name: String,
    translation: [f32; 3],
    scale: [f32; 3],
    yaw: f32,
    kind: Kind,
}

/// An owned template. Each instance receives its own scoped paths.
#[derive(Clone)]
pub struct SceneFragment(Vec<SceneNode>);

#[derive(Clone)]
enum Kind {
    Group(Vec<SceneNode>),
    Floor([f32; 2], [f32; 3]),
    Box([f32; 3], [f32; 3]),
    Player(PlayerCamera),
    Guard([f32; 3], [f32; 3], Patrol),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkBounds {
    pub x: [f32; 2],
    pub z: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerCamera {
    pub eye_height: f32,
    pub pitch: f32,
    pub vertical_fov: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patrol {
    pub half_span: f32,
    pub half_period_ticks: u32,
    pub phase_ticks: u32,
}

pub struct Scene;

impl Scene {
    pub fn World(
        name: &str,
        bounds: WalkBounds,
        nodes: impl Into<Vec<SceneNode>>,
    ) -> SceneDocument {
        SceneDocument {
            name: name.into(),
            bounds,
            nodes: nodes.into(),
        }
    }

    pub fn Group(name: &str, nodes: impl Into<Vec<SceneNode>>) -> SceneNode {
        SceneNode::New(name, Kind::Group(nodes.into()))
    }

    pub fn Fragment(nodes: impl Into<Vec<SceneNode>>) -> SceneFragment {
        SceneFragment(nodes.into())
    }

    pub fn Instance(name: &str, fragment: SceneFragment) -> SceneNode {
        Self::Group(name, fragment.0)
    }

    pub fn Floor(name: &str, width_depth: [f32; 2], color: [f32; 3]) -> SceneNode {
        SceneNode::New(name, Kind::Floor(width_depth, color))
    }

    pub fn Box(name: &str, dimensions: [f32; 3], color: [f32; 3]) -> SceneNode {
        SceneNode::New(name, Kind::Box(dimensions, color))
    }

    pub fn Player(name: &str, camera: PlayerCamera) -> SceneNode {
        SceneNode::New(name, Kind::Player(camera))
    }

    pub fn Guard(name: &str, dimensions: [f32; 3], color: [f32; 3], patrol: Patrol) -> SceneNode {
        SceneNode::New(name, Kind::Guard(dimensions, color, patrol))
    }
}

impl SceneNode {
    fn New(name: &str, kind: Kind) -> Self {
        Self {
            name: name.into(),
            translation: [0.0; 3],
            scale: [1.0; 3],
            yaw: 0.0,
            kind,
        }
    }

    pub fn At(mut self, x: f32, y: f32, z: f32) -> Self {
        self.translation = [x, y, z];
        self
    }

    pub fn Yaw(mut self, radians: f32) -> Self {
        self.yaw = radians;
        self
    }

    pub fn Scale(mut self, x: f32, y: f32, z: f32) -> Self {
        self.scale = [x, y, z];
        self
    }
}
