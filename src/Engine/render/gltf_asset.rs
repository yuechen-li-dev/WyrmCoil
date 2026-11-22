//! glTF mesh and animation import for opaque, rigid or skinned viewport meshes.

use std::collections::BTreeMap;
use std::path::Path;

use gltf::animation::{Interpolation, util::ReadOutputs};
use gltf::image::{Data, Format};

use crate::Scene::SceneMatrix;

type Point = [f32; 3];

#[derive(Debug, Default)]
pub struct MeshAtlas {
    pub Vertices: Vec<[f32; 4]>,
    pub Indices: Vec<u16>,
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

#[derive(Clone, Copy)]
struct Placement {
    translation: Point,
    rotation: [f32; 4],
    scale: Point,
}

impl Placement {
    fn Matrix(self) -> SceneMatrix {
        let [x, y, z, w] = self.rotation;
        let [sx, sy, sz] = self.scale;
        let [tx, ty, tz] = self.translation;
        SceneMatrix([
            [
                (1.0 - 2.0 * (y * y + z * z)) * sx,
                2.0 * (x * y + z * w) * sx,
                2.0 * (x * z - y * w) * sx,
                0.0,
            ],
            [
                2.0 * (x * y - z * w) * sy,
                (1.0 - 2.0 * (x * x + z * z)) * sy,
                2.0 * (y * z + x * w) * sy,
                0.0,
            ],
            [
                2.0 * (x * z + y * w) * sz,
                2.0 * (y * z - x * w) * sz,
                (1.0 - 2.0 * (x * x + y * y)) * sz,
                0.0,
            ],
            [tx, ty, tz, 1.0],
        ])
    }
}

struct Node {
    name: Option<String>,
    parent: Option<usize>,
    local: SceneMatrix,
    placement: Placement,
}

struct Skin {
    joints: Vec<usize>,
    inverse_bind: Vec<SceneMatrix>,
}

struct Part {
    node: usize,
    positions: Vec<Point>,
    colors: Vec<f32>,
    weights: Vec<[f32; 4]>,
    joints: Vec<[u16; 4]>,
    skin: Option<usize>,
}

pub struct MeshAsset {
    nodes: Vec<Node>,
    order: Vec<usize>,
    parts: Vec<Part>,
    skins: Vec<Skin>,
    indices: Vec<u16>,
}

impl MeshAsset {
    pub fn Read(path: &Path) -> Result<Self, String> {
        let (document, buffers, images) =
            gltf::import(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let source = |buffer: gltf::Buffer<'_>| Some(&buffers[buffer.index()].0[..]);
        let mut nodes: Vec<_> = document
            .nodes()
            .map(|node| {
                let (translation, rotation, scale) = node.transform().decomposed();
                Node {
                    name: node.name().map(str::to_owned),
                    parent: None,
                    local: SceneMatrix(node.transform().matrix()),
                    placement: Placement {
                        translation,
                        rotation,
                        scale,
                    },
                }
            })
            .collect();
        for node in document.nodes() {
            for child in node.children() {
                let entry = &mut nodes[child.index()];
                if entry.parent.replace(node.index()).is_some() {
                    return Err("glTF node has multiple parents".into());
                }
            }
        }
        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .ok_or("glTF is missing a scene")?;
        let mut order = Vec::new();
        let mut stack: Vec<_> = scene.nodes().collect();
        let mut seen = vec![false; nodes.len()];
        while let Some(node) = stack.pop() {
            if std::mem::replace(&mut seen[node.index()], true) {
                return Err("glTF scene contains a repeated or cyclic node".into());
            }
            order.push(node.index());
            stack.extend(node.children());
        }

        let mut skins = Vec::new();
        for skin in document.skins() {
            let joints: Vec<_> = skin.joints().map(|node| node.index()).collect();
            let inverse_bind = skin
                .reader(source)
                .read_inverse_bind_matrices()
                .map_or_else(
                    || vec![SceneMatrix::IDENTITY; joints.len()],
                    |matrices| matrices.map(SceneMatrix).collect(),
                );
            if inverse_bind.len() != joints.len() || joints.iter().any(|&joint| !seen[joint]) {
                return Err("glTF skin's joint or inverse bind data is invalid".into());
            }
            skins.push(Skin {
                joints,
                inverse_bind,
            });
        }

        let mut parts = Vec::new();
        let mut indices = Vec::new();
        let mut base = 0;
        for node in document.nodes().filter(|node| seen[node.index()]) {
            let Some(mesh) = node.mesh() else {
                continue;
            };
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles
                    || primitive.morph_targets().len() > 0
                {
                    return Err(
                        "only triangle glTF meshes without morph targets are supported".into(),
                    );
                }
                let material = primitive.material();
                if material.alpha_mode() != gltf::material::AlphaMode::Opaque {
                    return Err("viewport glTF import requires opaque base color".into());
                }
                let reader = primitive.reader(source);
                let positions: Vec<_> = reader
                    .read_positions()
                    .ok_or("glTF mesh is missing POSITION")?
                    .collect();
                let normals: Vec<_> = reader
                    .read_normals()
                    .ok_or("glTF mesh is missing NORMAL")?
                    .collect();
                let material = material.pbr_metallic_roughness();
                let factor = material.base_color_factor();
                let texture = material.base_color_texture();
                let uv: Vec<_> = match &texture {
                    Some(texture) => reader
                        .read_tex_coords(texture.tex_coord())
                        .ok_or("glTF texture is missing UVs")?
                        .into_f32()
                        .collect(),
                    None => vec![[0.0; 2]; positions.len()],
                };
                if positions.len() != normals.len()
                    || positions.len() != uv.len()
                    || base + positions.len() > usize::from(u16::MAX) + 1
                    || positions.iter().flatten().any(|x| !x.is_finite())
                    || uv.iter().flatten().any(|x| !x.is_finite())
                {
                    return Err("invalid glTF vertex data or too many vertices in one mesh".into());
                }
                let mut colors = Vec::with_capacity(positions.len());
                for (uv, normal) in uv.into_iter().zip(normals) {
                    let shade = (0.77 + normal[1] * 0.19 + normal[2] * 0.10).clamp(0.50, 1.0);
                    let color = match &texture {
                        Some(texture) => {
                            SamplePalette(&images[texture.texture().source().index()], uv)?
                        }
                        None => [1.0; 3],
                    };
                    colors.push(PackColor(std::array::from_fn(|i| {
                        color[i] * factor[i] * shade
                    })));
                }
                for index in reader
                    .read_indices()
                    .ok_or("glTF mesh is missing indices")?
                    .into_u32()
                {
                    if index as usize >= positions.len() {
                        return Err("glTF index out of bounds".into());
                    }
                    indices.push((base + index as usize) as u16);
                }
                base += positions.len();
                let skin = node.skin().map(|skin| skin.index());
                let mut weights = Vec::new();
                let mut joints = Vec::new();
                if let Some(index) = skin {
                    weights = reader
                        .read_weights(0)
                        .ok_or("skinned glTF mesh is missing WEIGHTS_0")?
                        .into_f32()
                        .collect();
                    joints = reader
                        .read_joints(0)
                        .ok_or("skinned glTF mesh is missing JOINTS_0")?
                        .into_u16()
                        .collect();
                    if weights.len() != positions.len()
                        || joints.len() != positions.len()
                        || joints
                            .iter()
                            .flatten()
                            .any(|&joint| usize::from(joint) >= skins[index].joints.len())
                        || weights.iter().any(|weight| {
                            weight.iter().any(|x| !x.is_finite() || *x < 0.0)
                                || (weight.iter().sum::<f32>() - 1.0).abs() > 0.01
                        })
                    {
                        return Err("invalid glTF skin weights or joints".into());
                    }
                }
                parts.push(Part {
                    node: node.index(),
                    positions,
                    colors,
                    weights,
                    joints,
                    skin,
                });
            }
        }
        if parts.is_empty() {
            return Err("glTF scene has no visible mesh".into());
        }
        Ok(Self {
            nodes,
            order,
            parts,
            skins,
            indices,
        })
    }

    pub fn SkinCount(&self) -> usize {
        self.skins.len()
    }

    /// Attach a rigid model at a named node so the pose carries it with the rig.
    pub fn Attach(&mut self, socket: &str, equipment: &Self) -> Result<(), String> {
        let node = self
            .order
            .iter()
            .copied()
            .find(|&index| self.nodes[index].name.as_deref() == Some(socket))
            .ok_or_else(|| format!("character rig is missing attachment socket '{socket}'"))?;
        if equipment.SkinCount() != 0 {
            return Err("equipped glTF mesh must be rigid".into());
        }
        let mut model = MeshAtlas::default();
        equipment.AppendPose(&mut model, None, 0.0)?;
        let start: usize = self.parts.iter().map(|part| part.positions.len()).sum();
        if start + model.Vertices.len() > usize::from(u16::MAX) + 1 {
            return Err("too many vertices in assembled character mesh".into());
        }
        self.indices.extend(
            model
                .Indices
                .iter()
                .map(|&index| (start + usize::from(index)) as u16),
        );
        self.parts.push(Part {
            node,
            positions: model
                .Vertices
                .iter()
                .map(|&[x, y, z, _]| [x, y, z])
                .collect(),
            colors: model.Vertices.iter().map(|vertex| vertex[3]).collect(),
            weights: Vec::new(),
            joints: Vec::new(),
            skin: None,
        });
        Ok(())
    }

    /// Evaluate the selected clip on the shared named rig, then prepare CPU-skinned vertices.
    pub fn AppendPose(
        &self,
        atlas: &mut MeshAtlas,
        clip: Option<&Clip>,
        time: f32,
    ) -> Result<[u32; 3], String> {
        let mut global = vec![SceneMatrix::IDENTITY; self.nodes.len()];
        for &index in &self.order {
            let node = &self.nodes[index];
            let mut local = node.local;
            if let Some(tracks) = node
                .name
                .as_ref()
                .and_then(|name| clip.and_then(|clip| clip.tracks.get(name)))
            {
                let mut placement = node.placement;
                for track in tracks {
                    let value = track.Sample(time);
                    match track.property {
                        Property::Translation => {
                            placement.translation = [value[0], value[1], value[2]]
                        }
                        Property::Rotation => placement.rotation = value,
                        Property::Scale => placement.scale = [value[0], value[1], value[2]],
                    }
                }
                local = placement.Matrix();
            }
            global[index] = node
                .parent
                .map_or(local, |parent| global[parent].Multiply(local));
        }

        let palettes: Vec<Vec<_>> = self
            .skins
            .iter()
            .map(|skin| {
                skin.joints
                    .iter()
                    .zip(&skin.inverse_bind)
                    .map(|(&index, &inverse)| global[index].Multiply(inverse))
                    .collect()
            })
            .collect();
        let mut vertices = Vec::new();
        for part in &self.parts {
            for (index, (&[x, y, z], &color)) in part.positions.iter().zip(&part.colors).enumerate()
            {
                let local = [x, y, z, 1.0];
                let mapped = match part.skin {
                    Some(skin) => {
                        let mut result = [0.0; 4];
                        for (&joint, weight) in part.joints[index].iter().zip(part.weights[index]) {
                            if weight > 0.0 {
                                let point = palettes[skin][usize::from(joint)].Transform(local);
                                for axis in 0..3 {
                                    result[axis] += point[axis] * weight;
                                }
                            }
                        }
                        result
                    }
                    None => global[part.node].Transform(local),
                };
                vertices.push([mapped[0], mapped[1], mapped[2], color]);
            }
        }
        atlas.Append(&vertices, &self.indices)
    }
}

fn SamplePalette(image: &Data, [u, v]: [f32; 2]) -> Result<[f32; 3], String> {
    let stride = match image.format {
        Format::R8G8B8 => 3,
        Format::R8G8B8A8 => 4,
        _ => return Err("glTF material requires an RGB8 or RGBA8 palette".into()),
    };
    let x = (u.clamp(0.0, 1.0) * (image.width - 1) as f32).round() as usize;
    let y = (v.clamp(0.0, 1.0) * (image.height - 1) as f32).round() as usize;
    let i = (y * image.width as usize + x) * stride;
    Ok(std::array::from_fn(|c| {
        f32::from(image.pixels[i + c]) / 255.0
    }))
}

pub struct Clip {
    tracks: BTreeMap<String, Vec<Track>>,
    pub Duration: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Property {
    Translation,
    Rotation,
    Scale,
}

struct Track {
    property: Property,
    step: bool,
    times: Vec<f32>,
    values: Vec<[f32; 4]>,
}

impl Track {
    fn Sample(&self, time: f32) -> [f32; 4] {
        let index = self.times.partition_point(|&at| at <= time);
        if index == 0 {
            return self.values[0];
        }
        if self.step || index == self.times.len() {
            return self.values[index - 1];
        }
        let from = self.values[index - 1];
        let mut to = self.values[index];
        let blend = (time - self.times[index - 1]) / (self.times[index] - self.times[index - 1]);
        let (a, b) = if self.property == Property::Rotation {
            let mut dot = from.iter().zip(to).map(|(&x, y)| x * y).sum::<f32>();
            if dot < 0.0 {
                to = to.map(|value| -value);
                dot = -dot;
            }
            if dot < 0.9995 {
                let angle = dot.clamp(0.0, 1.0).acos();
                (
                    ((1.0 - blend) * angle).sin() / angle.sin(),
                    (blend * angle).sin() / angle.sin(),
                )
            } else {
                (1.0 - blend, blend)
            }
        } else {
            (1.0 - blend, blend)
        };
        let mut value = std::array::from_fn(|axis| from[axis] * a + to[axis] * b);
        if self.property == Property::Rotation {
            let norm = value.iter().map(|v| v * v).sum::<f32>().sqrt();
            value = value.map(|axis| axis / norm);
        }
        value
    }
}

pub fn ReadClips(path: &Path, selected: &[&str]) -> Result<BTreeMap<String, Clip>, String> {
    let gltf::Gltf { document, blob } =
        gltf::Gltf::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let buffers =
        gltf::import_buffers(&document, path.parent(), blob).map_err(|error| error.to_string())?;
    let mut clips = BTreeMap::new();
    for animation in document.animations() {
        let Some(name) = animation.name().filter(|name| selected.contains(name)) else {
            continue;
        };
        let mut duration = 0.0_f32;
        let mut tracks: BTreeMap<String, Vec<Track>> = BTreeMap::new();
        for channel in animation.channels() {
            let node = channel.target().node();
            let node = node.name().ok_or("glTF clip animates an unnamed node")?;
            let step = match channel.sampler().interpolation() {
                Interpolation::Linear => false,
                Interpolation::Step => true,
                _ => return Err("glTF clip interpolation must be LINEAR or STEP".into()),
            };
            let reader = channel.reader(|buffer| Some(&buffers[buffer.index()].0[..]));
            let times: Vec<_> = reader
                .read_inputs()
                .ok_or("glTF clip has no time keys")?
                .collect();
            let vec3 = |[x, y, z]: [f32; 3]| [x, y, z, 0.0];
            let (property, values): (_, Vec<_>) = match reader
                .read_outputs()
                .ok_or("glTF clip has no output keys")?
            {
                ReadOutputs::Translations(output) => {
                    (Property::Translation, output.map(vec3).collect())
                }
                ReadOutputs::Scales(output) => (Property::Scale, output.map(vec3).collect()),
                ReadOutputs::Rotations(output) => (Property::Rotation, output.into_f32().collect()),
                _ => return Err("glTF mesh morph animation is unsupported".into()),
            };
            if times.is_empty()
                || times.len() != values.len()
                || times.iter().any(|x| !x.is_finite())
                || times.windows(2).any(|pair| pair[0] >= pair[1])
                || values.iter().flatten().any(|x| !x.is_finite())
            {
                return Err("invalid glTF animation keys".into());
            }
            duration = duration.max(*times.last().unwrap());
            tracks.entry(node.into()).or_default().push(Track {
                property,
                step,
                times,
                values,
            });
        }
        clips.insert(
            name.into(),
            Clip {
                tracks,
                Duration: duration,
            },
        );
    }
    if clips.len() != selected.len() {
        return Err(format!(
            "{} is missing a requested glTF clip",
            path.display()
        ));
    }
    Ok(clips)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baked_material_vertex_colors_compile_through_sdslv() {
        use crate::Engine::shader::sdslv::CompileCanonicalSourceToShaderArtifact;

        let source = include_str!("../../../integrations/zed/wyrmcoil_view/src/tactical.sdslv");
        let artifact = CompileCanonicalSourceToShaderArtifact("tactical.sdslv", source).unwrap();
        assert_eq!(artifact.EntryPoints[0].Name, "Tactical_VS");
        assert_eq!(artifact.EntryPoints[1].Name, "Tactical_PS");
        assert!(artifact.Hlsl.contains(">>"));
    }

    #[test]
    fn kaykit_gltf_and_glb_have_indexed_palette_materials_and_evaluated_skin_poses() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/games/last-watch/assets");
        let file = assets.join("environment/floor_tile_large.gltf");
        let floor = MeshAsset::Read(&file).unwrap();
        assert_eq!(floor.SkinCount(), 0);
        let mut atlas = MeshAtlas::default();
        let range = floor.AppendPose(&mut atlas, None, 0.0).unwrap();
        assert_eq!(range, [0, atlas.Indices.len() as u32, 0]);
        assert!(atlas.Vertices.iter().any(|vertex| vertex[0].abs() > 1.5));
        assert!(
            atlas
                .Vertices
                .iter()
                .any(|vertex| vertex[3] != atlas.Vertices[0][3])
        );

        let file = assets.join("characters/Knight.glb");
        let knight = MeshAsset::Read(&file).unwrap();
        assert_eq!(knight.SkinCount(), 1);
        let clips = ReadClips(
            &assets.join("animations/Rig_Medium_MovementBasic.glb"),
            &["Walking_A"],
        )
        .unwrap();
        let walk = &clips["Walking_A"];
        let first = knight.AppendPose(&mut atlas, Some(walk), 0.0).unwrap();
        let end = knight
            .AppendPose(&mut atlas, Some(walk), walk.Duration * 0.43)
            .unwrap();
        let count = (end[2] - first[2]) as usize;
        let poses = &atlas.Vertices[first[2] as usize..];
        assert_eq!(poses.len(), count * 2);
        assert!(poses.iter().flatten().all(|x| x.is_finite()));
        assert!(poses.iter().any(|vertex| vertex[1] > 0.80));
        assert!(
            poses[..count]
                .iter()
                .zip(&poses[count..])
                .any(|(a, b)| (a[1] - b[1]).abs() > 0.05)
        );
        assert!(
            poses[..count]
                .iter()
                .zip(&poses[count..])
                .all(|(a, b)| a[3] == b[3])
        );
    }
}
