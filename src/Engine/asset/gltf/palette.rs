//! CPU preparation for the existing packed-color viewport vertex layout.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Clip, ModelAsset, Primitive};
use crate::Scene::SceneMatrix;

#[derive(Debug, Default)]
pub struct PoseTimings {
    pub Evaluate: Duration,
    pub Skin: Duration,
    pub Append: Duration,
}

#[derive(Debug, Default)]
pub struct MeshAtlas {
    pub Vertices: Vec<[f32; 4]>,
    pub Indices: Vec<u16>,
    pub Timings: PoseTimings,
}

impl MeshAtlas {
    pub fn Append(&mut self, vertices: &[[f32; 4]], indices: &[u16]) -> Result<[u32; 3], String> {
        if indices.is_empty()
            || !indices.len().is_multiple_of(3)
            || indices
                .iter()
                .any(|&index| usize::from(index) >= vertices.len())
            || self.Vertices.len() + vertices.len() > 4_000_000
            || self.Indices.len() + indices.len() > 8_000_000
        {
            return Err("invalid or oversized viewport mesh range".into());
        }
        let range = [
            self.Indices.len() as u32,
            indices.len() as u32,
            self.Vertices.len() as u32,
        ];
        self.Vertices.extend_from_slice(vertices);
        self.Indices.extend_from_slice(indices);
        Ok(range)
    }
}

/// Pack a palette sample into an exactly representable 24-bit float.
pub fn PackColor([r, g, b]: [f32; 3]) -> f32 {
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    ((byte(r) << 16) | (byte(g) << 8) | byte(b)) as f32
}

pub struct PaletteMesh {
    model: Arc<ModelAsset>,
    colors: Vec<Vec<f32>>,
    attachments: Vec<(usize, Vec<[f32; 4]>)>,
    indices: Vec<u16>,
    vertices: usize,
}

impl PaletteMesh {
    pub fn New(model: Arc<ModelAsset>) -> Result<Self, String> {
        let mut base = 0;
        let mut colors = Vec::new();
        let mut indices = Vec::new();
        for part in &model.Primitives {
            if base + part.Positions.len() > usize::from(u16::MAX) + 1 {
                return Err("too many vertices for one packed viewport mesh".into());
            }
            indices.extend(
                part.Indices
                    .iter()
                    .map(|&index| (base + index as usize) as u16),
            );
            colors.push(BakeColors(&model, part));
            base += part.Positions.len();
        }
        Ok(Self {
            model,
            colors,
            attachments: Vec::new(),
            indices,
            vertices: base,
        })
    }

    /// Attach rigid, already prepared geometry to a named joint or node.
    pub fn Attach(&mut self, socket: &str, equipment: &Self) -> Result<(), String> {
        let node = self.model.FindNode(socket)?;
        if !equipment.model.Skins.is_empty() {
            return Err("equipped glTF mesh must be rigid".into());
        }
        let mut atlas = MeshAtlas::default();
        equipment.AppendPose(&mut atlas, None, 0.0)?;
        let base = self.vertices;
        if base + atlas.Vertices.len() > usize::from(u16::MAX) + 1 {
            return Err("too many vertices in assembled character mesh".into());
        }
        self.indices.extend(
            atlas
                .Indices
                .iter()
                .map(|&index| (base + usize::from(index)) as u16),
        );
        self.vertices += atlas.Vertices.len();
        self.attachments.push((node, atlas.Vertices));
        Ok(())
    }

    pub fn AppendPose(
        &self,
        atlas: &mut MeshAtlas,
        clip: Option<&Clip>,
        time: f32,
    ) -> Result<[u32; 3], String> {
        let started = Instant::now();
        let world = self.model.Pose(clip, time)?;
        let palettes: Vec<Vec<SceneMatrix>> = self
            .model
            .Skins
            .iter()
            .map(|skin| {
                skin.Joints
                    .iter()
                    .zip(&skin.InverseBind)
                    .map(|(&index, &inverse)| world[index].Multiply(inverse))
                    .collect()
            })
            .collect();
        atlas.Timings.Evaluate += started.elapsed();

        let skin_started = Instant::now();
        let mut vertices = Vec::with_capacity(self.vertices);
        for (part, colors) in self.model.Primitives.iter().zip(&self.colors) {
            for (index, (&[x, y, z], &color)) in part.Positions.iter().zip(colors).enumerate() {
                let local = [x, y, z, 1.0];
                let mapped = match part.Skin {
                    Some(skin) => {
                        let mut result = [0.0; 4];
                        for (&joint, weight) in part.Joints[index].iter().zip(part.Weights[index]) {
                            if weight > 0.0 {
                                let point = palettes[skin][usize::from(joint)].Transform(local);
                                for axis in 0..3 {
                                    result[axis] += point[axis] * weight;
                                }
                            }
                        }
                        result
                    }
                    None => world[part.Node].Transform(local),
                };
                if !mapped[..3].iter().all(|value| value.is_finite()) {
                    return Err("nonfinite posed vertex".into());
                }
                vertices.push([mapped[0], mapped[1], mapped[2], color]);
            }
        }
        for (socket, equipment) in &self.attachments {
            for &[x, y, z, color] in equipment {
                let point = world[*socket].Transform([x, y, z, 1.0]);
                vertices.push([point[0], point[1], point[2], color]);
            }
        }
        atlas.Timings.Skin += skin_started.elapsed();
        let append_started = Instant::now();
        let range = atlas.Append(&vertices, &self.indices)?;
        atlas.Timings.Append += append_started.elapsed();
        Ok(range)
    }
}

fn BakeColors(model: &ModelAsset, part: &Primitive) -> Vec<f32> {
    let material = part.Material.map(|index| &model.Materials[index]);
    let factor = material.map_or([1.0; 4], |mat| mat.BaseColor);
    let texture = material
        .and_then(|mat| mat.BaseColorTexture)
        .map(|slot| &model.Textures[slot]);
    let mut colors = Vec::with_capacity(part.Positions.len());
    for index in 0..part.Positions.len() {
        let normal = part.Normals.get(index).copied().unwrap_or([0.0, 1.0, 0.0]);
        let shade = (0.77 + normal[1] * 0.19 + normal[2] * 0.10).clamp(0.50, 1.0);
        let color = if let Some(texture) = texture {
            let [u, v] = part.TexCoords[index];
            let x = (u.clamp(0.0, 1.0) * (texture.Width - 1) as f32).round() as usize;
            let y = (v.clamp(0.0, 1.0) * (texture.Height - 1) as f32).round() as usize;
            let offset = (y * texture.Width as usize + x) * 4;
            std::array::from_fn(|channel| f32::from(texture.Rgba[offset + channel]) / 255.0)
        } else {
            [1.0; 3]
        };
        let vertex = part.VertexColors.get(index).copied().unwrap_or([1.0; 4]);
        colors.push(PackColor(std::array::from_fn(|i| {
            color[i] * factor[i] * vertex[i] * shade
        })));
    }
    colors
}
