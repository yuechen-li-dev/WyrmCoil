use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Kind, Patrol, PlayerCamera, SceneDocument, SceneNode, WalkBounds};

const MAX_VALUE: f32 = 100_000.0;
const MAX_NODES: usize = 2_048;
const MAX_DEPTH: usize = 16;

/// Columns acting on column vectors, as used by the room's clip transform.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SceneMatrix(pub [[f32; 4]; 4]);

impl SceneMatrix {
    pub const IDENTITY: Self = Self([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    pub fn Placement(translation: [f32; 3], scale: [f32; 3], yaw: f32) -> Self {
        let (sin, cos) = yaw.sin_cos();
        let [x, y, z] = translation;
        Self([
            [cos * scale[0], 0.0, sin * scale[0], 0.0],
            [0.0, scale[1], 0.0, 0.0],
            [-sin * scale[2], 0.0, cos * scale[2], 0.0],
            [x, y, z, 1.0],
        ])
    }

    pub fn Transform(self, vector: [f32; 4]) -> [f32; 4] {
        let mut result = [0.0; 4];
        for (column, scale) in self.0.into_iter().zip(vector) {
            for (output, element) in result.iter_mut().zip(column) {
                *output += element * scale;
            }
        }
        result
    }

    pub fn Multiply(self, rhs: Self) -> Self {
        Self(rhs.0.map(|column| self.Transform(column)))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedMesh {
    pub id: String,
    pub model: SceneMatrix,
    pub color: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedPlayer {
    pub id: String,
    pub eye: [f32; 3],
    pub yaw: f32,
    pub camera: PlayerCamera,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedGuard {
    pub body: PlacedMesh,
    pub patrol: Patrol,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneData {
    pub name: String,
    pub bounds: WalkBounds,
    pub floor: PlacedMesh,
    pub player: PlacedPlayer,
    pub guard: PlacedGuard,
    pub boxes: Vec<PlacedMesh>,
}

#[derive(Debug, PartialEq)]
pub struct CompiledScene {
    data: SceneData,
    identity: String,
}

impl CompiledScene {
    pub fn Data(&self) -> &SceneData {
        &self.data
    }
    pub fn Identity(&self) -> &str {
        &self.identity
    }
}

#[derive(Default)]
struct Placement {
    count: usize,
    floor: Option<PlacedMesh>,
    player: Option<PlacedPlayer>,
    guard: Option<PlacedGuard>,
    boxes: Vec<PlacedMesh>,
}

pub fn CompileScene(document: &SceneDocument) -> Result<CompiledScene, String> {
    if !IsSegment(&document.name) {
        return Err("scene name must be a valid ID segment".into());
    }
    let mut placement = Placement::default();
    Resolve(
        &document.nodes,
        "",
        SceneMatrix::IDENTITY,
        0,
        &mut placement,
    )?;
    ValidateCompiled(SceneData {
        name: document.name.clone(),
        bounds: document.bounds,
        floor: placement
            .floor
            .ok_or_else(|| format!("{}: expected one floor", document.name))?,
        player: placement
            .player
            .ok_or_else(|| format!("{}: expected one player", document.name))?,
        guard: placement
            .guard
            .ok_or_else(|| format!("{}: expected one guard", document.name))?,
        boxes: placement.boxes,
    })
}

fn Resolve(
    nodes: &[SceneNode],
    prefix: &str,
    parent: SceneMatrix,
    depth: usize,
    out: &mut Placement,
) -> Result<(), String> {
    if depth >= MAX_DEPTH {
        return Err(format!("{prefix}: scene nesting exceeds {MAX_DEPTH}"));
    }
    let mut names = BTreeSet::new();
    for node in nodes {
        let path = if prefix.is_empty() {
            node.name.clone()
        } else {
            format!("{prefix}.{}", node.name)
        };
        out.count += 1;
        if out.count > MAX_NODES {
            return Err(format!("{path}: scene exceeds {MAX_NODES} nodes"));
        }
        if !IsSegment(&node.name) {
            return Err(format!("{path:?}: invalid ID segment"));
        }
        if !names.insert(node.name.as_str()) {
            return Err(format!("{path}: duplicate identity"));
        }
        if !node.translation.into_iter().all(Bounded)
            || !node.scale.into_iter().all(Positive)
            || !Bounded(node.yaw)
        {
            return Err(format!("{path}: invalid local transform"));
        }
        let model = parent.Multiply(SceneMatrix::Placement(
            node.translation,
            node.scale,
            node.yaw,
        ));
        CheckModel(&path, model, false)?;
        match &node.kind {
            Kind::Group(children) => {
                if children.is_empty() {
                    return Err(format!("{path}: empty group or fragment"));
                }
                Resolve(children, &path, model, depth + 1, out)?;
            }
            Kind::Floor([width, depth], color) => {
                let floor = Mesh(&path, model, [*width, 0.0, *depth], *color, true)?;
                PutOnce(&mut out.floor, floor, &path, "floor")?;
            }
            Kind::Box(dimensions, color) => {
                out.boxes
                    .push(Mesh(&path, model, *dimensions, *color, false)?)
            }
            Kind::Player(camera) => {
                let [x, y, z, _] = model.Transform([0.0, camera.eye_height, 0.0, 1.0]);
                let yaw = (-model.0[2][0]).atan2(model.0[2][2]);
                let player = PlacedPlayer {
                    id: path.clone(),
                    eye: [x, y, z],
                    yaw,
                    camera: *camera,
                };
                PutOnce(&mut out.player, player, &path, "player")?;
            }
            Kind::Guard(dimensions, color, patrol) => {
                let body = Mesh(&path, model, *dimensions, *color, false)?;
                PutOnce(
                    &mut out.guard,
                    PlacedGuard {
                        body,
                        patrol: *patrol,
                    },
                    &path,
                    "guard",
                )?;
            }
        }
    }
    Ok(())
}

fn PutOnce<T>(slot: &mut Option<T>, value: T, path: &str, kind: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{path}: more than one {kind}"));
    }
    *slot = Some(value);
    Ok(())
}

fn Mesh(
    path: &str,
    parent: SceneMatrix,
    dimensions: [f32; 3],
    color: [f32; 3],
    floor: bool,
) -> Result<PlacedMesh, String> {
    if !Positive(dimensions[0]) || !Positive(dimensions[2]) || (!floor && !Positive(dimensions[1]))
    {
        return Err(format!(
            "{path}: invalid dimensions (expected finite positive sizes)"
        ));
    }
    let model = parent.Multiply(SceneMatrix::Placement([0.0; 3], dimensions, 0.0));
    Ok(PlacedMesh {
        id: path.into(),
        model,
        color,
    })
}

fn IsSegment(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn Bounded(value: f32) -> bool {
    value.is_finite() && value.abs() <= MAX_VALUE
}
fn Positive(value: f32) -> bool {
    Bounded(value) && value > 0.0
}

fn CheckModel(path: &str, model: SceneMatrix, floor: bool) -> Result<(), String> {
    let [x, y, z, t] = model.0;
    let horizontal_area = x[0] * z[2] - z[0] * x[2];
    if !model.0.into_iter().flatten().all(Bounded)
        || [x[3], y[3], z[3], t[3]] != [0.0, 0.0, 0.0, 1.0]
        || [x[1], y[0], y[2], z[1]] != [0.0; 4]
        || horizontal_area <= 0.0
        || if floor { y[1] != 0.0 } else { y[1] <= 0.0 }
    {
        return Err(format!("{path}: invalid resolved transform"));
    }
    Ok(())
}

fn CheckMesh(mesh: &PlacedMesh, floor: bool) -> Result<(), String> {
    CheckModel(&mesh.id, mesh.model, floor)?;
    if !mesh
        .color
        .into_iter()
        .all(|value| (0.0..=1.0).contains(&value))
    {
        return Err(format!("{}: invalid RGB color", mesh.id));
    }
    Ok(())
}

pub(super) fn ValidateCompiled(mut data: SceneData) -> Result<CompiledScene, String> {
    if !IsSegment(&data.name) || data.boxes.len() > MAX_NODES - 3 {
        return Err("invalid scene name or object count".into());
    }
    let [min_x, max_x] = data.bounds.x;
    let [min_z, max_z] = data.bounds.z;
    if ![min_x, max_x, min_z, max_z].into_iter().all(Bounded) || min_x >= max_x || min_z >= max_z {
        return Err(format!("{}: invalid walk bounds", data.name));
    }
    let player = &data.player;
    let camera = player.camera;
    if !player.eye.into_iter().all(Bounded)
        || !Bounded(player.yaw)
        || !(min_x..=max_x).contains(&player.eye[0])
        || !(min_z..=max_z).contains(&player.eye[2])
        || !Positive(camera.eye_height)
        || !(-1.42..=1.42).contains(&camera.pitch)
        || !(0.01..3.13).contains(&camera.vertical_fov)
        || !Positive(camera.near)
        || camera.near < 0.0001
        || !Positive(camera.far)
        || camera.far <= camera.near
    {
        return Err(format!(
            "{}: invalid camera or spawn outside walk bounds",
            player.id
        ));
    }

    CheckMesh(&data.floor, true)?;
    CheckMesh(&data.guard.body, false)?;
    let patrol = data.guard.patrol;
    if !Positive(patrol.half_span)
        || !(1..=1_000_000).contains(&patrol.half_period_ticks)
        || patrol.phase_ticks >= patrol.half_period_ticks * 2
    {
        return Err(format!(
            "{}: invalid patrol span, period or phase",
            data.guard.body.id
        ));
    }
    for object in &data.boxes {
        CheckMesh(object, false)?;
    }

    let mut ids = BTreeSet::new();
    for id in [&data.floor.id, &player.id, &data.guard.body.id]
        .into_iter()
        .chain(data.boxes.iter().map(|mesh| &mesh.id))
    {
        if id.split('.').count() > MAX_DEPTH
            || !id.split('.').all(IsSegment)
            || !ids.insert(id.as_str())
        {
            return Err(format!("{id}: invalid or duplicate identity"));
        }
    }
    for &id in &ids {
        let mut scope = id;
        while let Some((parent, _)) = scope.rsplit_once('.') {
            if ids.contains(parent) {
                return Err(format!("{id}: object {parent} cannot be a group"));
            }
            scope = parent;
        }
    }
    data.boxes.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    let identity = super::artifact::ContentIdentity(&data)?;
    Ok(CompiledScene { data, identity })
}
