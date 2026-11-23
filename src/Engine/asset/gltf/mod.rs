//! Owned glTF 2.0 viewport assets, independent of the renderer and window system.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::Scene::SceneMatrix;
use ::gltf::{
    self,
    animation::{Interpolation, util::ReadOutputs},
};

mod palette;
mod source;
pub use palette::{
    MeshAtlas, PackColor, PaletteMesh, PoseTimings, SkinClip, SkinMesh, SkinPose, SkinVertex,
};
pub use source::{AssetError, AssetStage, ImageTimings, ImportTimings, Texture};
use source::{ImageCache, Resolve, Source};

pub struct AssetLibrary {
    root: PathBuf,
    models: BTreeMap<PathBuf, Arc<ModelAsset>>,
    textures: ImageCache,
    clips: BTreeMap<(PathBuf, Vec<String>), Arc<ClipSet>>,
}

impl AssetLibrary {
    pub fn New(root: &Path) -> Result<Self, AssetError> {
        let root = root
            .canonicalize()
            .map_err(|error| AssetError::New(root, AssetStage::Resolve, error))?;
        if !root.is_dir() {
            return Err(AssetError::New(
                &root,
                AssetStage::Resolve,
                "expected an asset directory",
            ));
        }
        Ok(Self {
            root,
            models: BTreeMap::new(),
            textures: ImageCache::default(),
            clips: BTreeMap::new(),
        })
    }

    pub fn Load(&mut self, relative: &Path) -> Result<Arc<ModelAsset>, AssetError> {
        let path = Resolve(&self.root, &self.root, relative)?;
        if let Some(model) = self.models.get(&path) {
            return Ok(model.clone());
        }
        let mut source = Source::Read(&self.root, &path)?;
        let textures = source.DecodeTextures(&self.root, &path, &mut self.textures)?;
        let model = Extract(Identity(&self.root, &path), path.clone(), source, textures)
            .map_err(|error| AssetError::New(&path, AssetStage::Geometry, error))?;
        let model = Arc::new(model);
        self.models.insert(path, model.clone());
        Ok(model)
    }

    /// An empty selection reads every clip. Names are sorted for canonical cache identity.
    pub fn LoadClips(
        &mut self,
        relative: &Path,
        names: &[&str],
    ) -> Result<Arc<ClipSet>, AssetError> {
        let path = Resolve(&self.root, &self.root, relative)?;
        let mut names: Vec<_> = names.iter().map(|&name| name.to_owned()).collect();
        names.sort();
        names.dedup();
        let key = (path, names);
        if let Some(clips) = self.clips.get(&key) {
            return Ok(clips.clone());
        }
        let source = Source::Read(&self.root, &key.0)?;
        let clips = ReadClips(Identity(&self.root, &key.0), &key.0, source, &key.1)
            .map_err(|error| AssetError::New(&key.0, AssetStage::Animation, error))?;
        let clips = Arc::new(clips);
        self.clips.insert(key, clips.clone());
        Ok(clips)
    }

    pub fn Timings(&self) -> ImportTimings {
        let mut total = ImportTimings::default();
        for timing in self
            .models
            .values()
            .map(|model| &model.Timings)
            .chain(self.clips.values().map(|clips| &clips.Timings))
        {
            total.SourceRead += timing.SourceRead;
            total.Parse += timing.Parse;
            total.Buffers += timing.Buffers;
            total.Textures += timing.Textures;
            total.Images.Add(&timing.Images);
            total.Geometry += timing.Geometry;
            total.Skins += timing.Skins;
            total.Animation += timing.Animation;
        }
        total
    }

    pub fn Inspect(&mut self, relative: &Path, poses: usize) -> Result<String, String> {
        if poses > 1_000 {
            return Err("inspection supports at most 1,000 pose samples".into());
        }
        let started = Instant::now();
        let model = self.Load(relative)?;
        let clips = self.LoadClips(relative, &[])?;
        let vertices: usize = model
            .Primitives
            .iter()
            .map(|part| part.Positions.len())
            .sum();
        let indices: usize = model.Primitives.iter().map(|part| part.Indices.len()).sum();
        let joints: usize = model.Skins.iter().map(|skin| skin.Joints.len()).sum();
        let texture_bytes: usize = model.Textures.iter().map(|image| image.Rgba.len()).sum();
        let lanes: usize = model
            .Primitives
            .iter()
            .map(|part| {
                std::mem::size_of_val(part.Positions.as_slice())
                    + std::mem::size_of_val(part.Normals.as_slice())
                    + std::mem::size_of_val(part.TexCoords.as_slice())
                    + std::mem::size_of_val(part.VertexColors.as_slice())
                    + std::mem::size_of_val(part.Indices.as_slice())
                    + std::mem::size_of_val(part.Weights.as_slice())
                    + std::mem::size_of_val(part.Joints.as_slice())
            })
            .sum();
        let keys: usize = clips
            .Clips
            .values()
            .flat_map(|clip| &clip.Tracks)
            .map(|track| {
                std::mem::size_of_val(track.Times.as_slice())
                    + std::mem::size_of_val(track.Values.as_slice())
            })
            .sum();
        let total = started.elapsed();
        let warm = Instant::now();
        let _ = self.Load(relative)?;
        let _ = self.LoadClips(relative, &[])?;
        let warm = warm.elapsed();
        let mut clips_report = String::new();
        for (name, clip) in &clips.Clips {
            use std::fmt::Write;
            let _ = writeln!(
                clips_report,
                "  {name}: {:.3} s, {} tracks",
                clip.Duration,
                clip.Tracks.len()
            );
        }
        let mut report = format!(
            "Asset {}\nMeshes {} · primitives {} · nodes {}\nVertices {vertices} · indices {indices}\nMaterials {} · textures {} ({texture_bytes} decoded bytes)\nSkins {} · joints {joints} · clips {}\n{clips_report}CPU payload estimate {} bytes (vertex lanes, textures and clip keys)\nOne packed viewport pose estimate {} bytes\nSource asset timings: {:?}\nClip source timings: {:?}\nCold model + clips {:.2?} · warm cached lookup {:.2?}\nSupported: opaque triangles, base-color PNG/JPEG, linear/step TRS, joint skinning\n",
            model.Identity,
            model.MeshCount,
            model.Primitives.len(),
            model.Nodes.len(),
            model.Materials.len(),
            model.Textures.len(),
            model.Skins.len(),
            clips.Clips.len(),
            lanes + texture_bytes + keys,
            vertices * 16 + indices * 2,
            model.Timings,
            clips.Timings,
            total,
            warm
        );
        if poses > 0 {
            if model.Primitives.iter().any(|part| part.Normals.is_empty()) {
                report.push_str(
                    "Palette preview uses upward shading normals for primitives missing NORMAL.\n",
                );
            }
            let prepared = Instant::now();
            let palette = PaletteMesh::New(model)?;
            let palette_elapsed = prepared.elapsed();
            let mut atlas = MeshAtlas::default();
            let clip = clips.Clips.values().next();
            for index in 0..poses {
                let time = clip.map_or(0.0, |clip| {
                    clip.Duration * index as f32 / (poses - 1).max(1) as f32
                });
                palette.AppendPose(&mut atlas, clip, time)?;
            }
            use std::fmt::Write;
            let _ = writeln!(
                report,
                "Palette preparation {:.2?} · sampling and color packing {:.2?} · {poses} poses · evaluation {:.2?} · skinning {:.2?} · atlas append {:.2?} · {} bytes",
                palette_elapsed,
                palette.ColorPreparation,
                atlas.Timings.Evaluate,
                atlas.Timings.Skin,
                atlas.Timings.Append,
                atlas.Vertices.len() * 16 + atlas.Indices.len() * 2
            );
        }
        Ok(report)
    }
}

fn Identity(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap()
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Clone, Copy)]
struct Placement {
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
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

pub struct ModelNode {
    pub Name: Option<String>,
    pub Parent: Option<usize>,
    pub Local: SceneMatrix,
    placement: Placement,
    matrix_only: bool,
}

pub struct Skin {
    pub Joints: Vec<usize>,
    pub InverseBind: Vec<SceneMatrix>,
}

pub struct Material {
    pub BaseColor: [f32; 4],
    pub BaseColorTexture: Option<usize>,
    pub TexCoord: u32,
    pub Metallic: f32,
    pub Roughness: f32,
    pub DoubleSided: bool,
}

pub struct Primitive {
    pub Node: usize,
    pub Positions: Vec<[f32; 3]>,
    pub Normals: Vec<[f32; 3]>,
    pub TexCoords: Vec<[f32; 2]>,
    pub VertexColors: Vec<[f32; 4]>,
    pub Weights: Vec<[f32; 4]>,
    pub Joints: Vec<[u16; 4]>,
    pub Skin: Option<usize>,
    pub Material: Option<usize>,
    pub Indices: Vec<u32>,
}

pub struct ModelAsset {
    pub Identity: String,
    pub MeshCount: usize,
    pub Nodes: Vec<ModelNode>,
    pub Skins: Vec<Skin>,
    pub Primitives: Vec<Primitive>,
    pub Materials: Vec<Material>,
    pub Textures: Vec<Arc<Texture>>,
    pub Timings: ImportTimings,
    source_path: PathBuf,
    order: Vec<usize>,
}

#[derive(Default)]
struct PoseScratch {
    local: Vec<Option<Placement>>,
    world: Vec<SceneMatrix>,
}

impl ModelAsset {
    /// Evaluate all active world matrices in the original glTF coordinate space.
    pub fn Pose(&self, clip: Option<&Clip>, time: f32) -> Result<Vec<SceneMatrix>, String> {
        let mut scratch = PoseScratch::default();
        let targets = clip.map(|clip| self.ClipTargets(clip)).transpose()?;
        self.PoseInto(clip.zip(targets.as_deref()), time, &mut scratch)?;
        Ok(scratch.world)
    }

    fn ClipTargets(&self, clip: &Clip) -> Result<Vec<usize>, String> {
        let mut targets = Vec::with_capacity(clip.Tracks.len());
        for track in &clip.Tracks {
            let index = if self.source_path == clip.source_path {
                track.Node
            } else {
                self.FindNode(
                    track
                        .Name
                        .as_deref()
                        .ok_or("external clip has an unnamed target")?,
                )?
            };
            if index >= self.Nodes.len()
                || self.Nodes[index].matrix_only
                || !self.order.contains(&index)
            {
                return Err(format!("invalid animation target node {index}"));
            }
            targets.push(index);
        }
        Ok(targets)
    }

    fn PoseInto(
        &self,
        bound: Option<(&Clip, &[usize])>,
        time: f32,
        scratch: &mut PoseScratch,
    ) -> Result<(), String> {
        if !time.is_finite() || time < 0.0 {
            return Err("animation time must be finite and nonnegative".into());
        }
        scratch.local.resize(self.Nodes.len(), None);
        scratch.local.fill(None);
        if let Some((clip, targets)) = bound {
            for (track, &index) in clip.Tracks.iter().zip(targets) {
                let placement = scratch.local[index].get_or_insert(self.Nodes[index].placement);
                let value = track.Sample(time);
                match track.Property {
                    Property::Translation => placement.translation = [value[0], value[1], value[2]],
                    Property::Rotation => placement.rotation = value,
                    Property::Scale => placement.scale = [value[0], value[1], value[2]],
                }
            }
        }
        scratch
            .world
            .resize(self.Nodes.len(), SceneMatrix::IDENTITY);
        for &index in &self.order {
            let node = &self.Nodes[index];
            let local = scratch.local[index].map_or(node.Local, Placement::Matrix);
            scratch.world[index] = node
                .Parent
                .map_or(local, |parent| scratch.world[parent].Multiply(local));
            if !ValidMatrix(scratch.world[index]) {
                return Err(format!("nonfinite world transform at node {index}"));
            }
        }
        Ok(())
    }

    pub fn FindNode(&self, name: &str) -> Result<usize, String> {
        let mut matches = self
            .order
            .iter()
            .copied()
            .filter(|&index| self.Nodes[index].Name.as_deref() == Some(name));
        let index = matches
            .next()
            .ok_or_else(|| format!("{} is missing named node '{name}'", self.Identity))?;
        if matches.next().is_some() {
            return Err(format!("{} repeats named node '{name}'", self.Identity));
        }
        Ok(index)
    }
}

fn ValidMatrix(matrix: SceneMatrix) -> bool {
    matrix.0.iter().flatten().all(|value| value.is_finite())
        && [
            matrix.0[0][3],
            matrix.0[1][3],
            matrix.0[2][3],
            matrix.0[3][3],
        ] == [0.0, 0.0, 0.0, 1.0]
}

fn Extract(
    identity: String,
    source_path: PathBuf,
    source: Source,
    textures: Vec<Arc<Texture>>,
) -> Result<ModelAsset, String> {
    let Source {
        document,
        buffers,
        mut timings,
    } = source;
    let started = Instant::now();
    let source = |buffer: gltf::Buffer<'_>| Some(&buffers[buffer.index()].0[..]);
    let mut nodes = Vec::new();
    for node in document.nodes() {
        let transform = node.transform();
        let matrix_only = matches!(transform, gltf::scene::Transform::Matrix { .. });
        let local = SceneMatrix(transform.clone().matrix());
        let (translation, rotation, scale) = transform.decomposed();
        let norm = rotation.iter().map(|x| x * x).sum::<f32>();
        if !ValidMatrix(local)
            || !norm.is_finite()
            || (norm - 1.0).abs() > 0.01
            || scale.iter().any(|x| !x.is_finite() || *x == 0.0)
        {
            return Err(format!("invalid transform at node {}", node.index()));
        }
        nodes.push(ModelNode {
            Name: node.name().map(str::to_owned),
            Parent: None,
            Local: local,
            placement: Placement {
                translation,
                rotation,
                scale,
            },
            matrix_only,
        });
    }
    for node in document.nodes() {
        for child in node.children() {
            if nodes[child.index()].Parent.replace(node.index()).is_some() {
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
        if std::mem::replace(&mut seen[node.index()], true)
            || nodes[node.index()]
                .Parent
                .is_some_and(|parent| !seen[parent])
        {
            return Err("glTF scene contains an invalid root, repeated or cyclic node".into());
        }
        order.push(node.index());
        stack.extend(node.children());
    }

    let skin_start = Instant::now();
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
        let mut unique = joints.clone();
        unique.sort_unstable();
        unique.dedup();
        if joints.is_empty()
            || inverse_bind.len() != joints.len()
            || unique.len() != joints.len()
            || inverse_bind.iter().any(|matrix| !ValidMatrix(*matrix))
        {
            return Err(format!(
                "skin {} has invalid joints or inverse bind matrices",
                skin.index()
            ));
        }
        skins.push(Skin {
            Joints: joints,
            InverseBind: inverse_bind,
        });
    }
    timings.Skins = skin_start.elapsed();

    let mut materials = Vec::new();
    for material in document.materials() {
        let pbr = material.pbr_metallic_roughness();
        if material.alpha_mode() != gltf::material::AlphaMode::Opaque
            || material.normal_texture().is_some()
            || material.occlusion_texture().is_some()
            || pbr.metallic_roughness_texture().is_some()
            || material.emissive_texture().is_some()
            || material.emissive_factor() != [0.0; 3]
        {
            return Err(format!(
                "unsupported material channels or alpha in material {:?}",
                material.index()
            ));
        }
        let texture = pbr.base_color_texture();
        let factor = pbr.base_color_factor();
        if !factor
            .into_iter()
            .chain([pbr.metallic_factor(), pbr.roughness_factor()])
            .all(f32::is_finite)
        {
            return Err("nonfinite material value".into());
        }
        materials.push(Material {
            BaseColor: factor,
            BaseColorTexture: texture
                .as_ref()
                .map(|texture| texture.texture().source().index()),
            TexCoord: texture.map_or(0, |texture| texture.tex_coord()),
            Metallic: pbr.metallic_factor(),
            Roughness: pbr.roughness_factor(),
            DoubleSided: material.double_sided(),
        });
    }

    let mut primitives = Vec::new();
    let mut vertex_total = 0;
    for node in document.nodes().filter(|node| seen[node.index()]) {
        let Some(mesh) = node.mesh() else {
            continue;
        };
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.morph_targets().len() > 0
            {
                return Err("only triangles without morph targets are supported".into());
            }
            if primitive.get(&gltf::Semantic::Joints(1)).is_some()
                || primitive.get(&gltf::Semantic::Weights(1)).is_some()
            {
                return Err("at most four influences per vertex are supported".into());
            }
            let reader = primitive.reader(source);
            let positions = reader.read_positions().ok_or("mesh is missing POSITION")?;
            vertex_total += positions.len();
            if vertex_total > 2_000_000 {
                return Err("asset exceeds the vertex budget".into());
            }
            let positions: Vec<_> = positions.collect();
            let normals: Vec<_> = reader
                .read_normals()
                .map_or_else(Vec::new, |normals| normals.collect());
            let material = primitive.material().index();
            let slot = material.map(|index| &materials[index]);
            let uv_slot = slot.map_or(0, |slot| slot.TexCoord);
            let uv: Vec<_> = match reader.read_tex_coords(uv_slot) {
                Some(uv) => uv.into_f32().collect(),
                None if slot.is_some_and(|slot| slot.BaseColorTexture.is_some()) => {
                    return Err("base-color texture is missing UVs".into());
                }
                None => Vec::new(),
            };
            let colors: Vec<_> = reader
                .read_colors(0)
                .map_or_else(Vec::new, |colors| colors.into_rgba_f32().collect());
            if (!normals.is_empty() && positions.len() != normals.len())
                || (!uv.is_empty() && positions.len() != uv.len())
                || (!colors.is_empty() && positions.len() != colors.len())
                || positions
                    .iter()
                    .chain(&normals)
                    .flatten()
                    .any(|x| !x.is_finite())
                || uv
                    .iter()
                    .flatten()
                    .chain(colors.iter().flatten())
                    .any(|x| !x.is_finite())
            {
                return Err("invalid glTF vertex lanes".into());
            }
            let indices: Vec<_> = match reader.read_indices() {
                Some(indices) => indices.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            if indices.is_empty()
                || !indices.len().is_multiple_of(3)
                || indices
                    .iter()
                    .any(|&index| index as usize >= positions.len())
            {
                return Err("invalid triangle index count or out-of-range index".into());
            }
            let skin = node.skin().map(|skin| skin.index());
            let mut weights = Vec::new();
            let mut joints = Vec::new();
            if let Some(index) = skin {
                if skins[index].Joints.iter().any(|&joint| !seen[joint]) {
                    return Err("skin references a joint outside the active scene".into());
                }
                weights = reader
                    .read_weights(0)
                    .ok_or("skin is missing WEIGHTS_0")?
                    .into_f32()
                    .collect();
                joints = reader
                    .read_joints(0)
                    .ok_or("skin is missing JOINTS_0")?
                    .into_u16()
                    .collect();
                if weights.len() != positions.len()
                    || joints.len() != positions.len()
                    || joints
                        .iter()
                        .flatten()
                        .any(|&joint| usize::from(joint) >= skins[index].Joints.len())
                    || weights.iter().any(|weight| {
                        weight.iter().any(|x| !x.is_finite() || *x < 0.0)
                            || (weight.iter().sum::<f32>() - 1.0).abs() > 0.01
                    })
                {
                    return Err("invalid glTF skin weights or joints".into());
                }
            }
            primitives.push(Primitive {
                Node: node.index(),
                Positions: positions,
                Normals: normals,
                TexCoords: uv,
                VertexColors: colors,
                Weights: weights,
                Joints: joints,
                Skin: skin,
                Material: material,
                Indices: indices,
            });
        }
    }
    timings.Geometry = started.elapsed().saturating_sub(timings.Skins);
    let model = ModelAsset {
        Identity: identity,
        MeshCount: document.meshes().len(),
        Nodes: nodes,
        Skins: skins,
        Primitives: primitives,
        Materials: materials,
        Textures: textures,
        Timings: timings,
        source_path,
        order,
    };
    model.Pose(None, 0.0)?;
    Ok(model)
}

pub struct ClipSet {
    pub Clips: BTreeMap<String, Clip>,
    pub Timings: ImportTimings,
}

pub struct Clip {
    pub Source: String,
    source_path: PathBuf,
    pub Tracks: Vec<Track>,
    pub Duration: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Property {
    Translation,
    Rotation,
    Scale,
}

pub struct Track {
    pub Node: usize,
    pub Name: Option<String>,
    pub Property: Property,
    pub Step: bool,
    pub Times: Vec<f32>,
    pub Values: Vec<[f32; 4]>,
}

impl Track {
    fn Sample(&self, time: f32) -> [f32; 4] {
        let index = self.Times.partition_point(|&at| at <= time);
        if index == 0 {
            return self.Values[0];
        }
        if self.Step || index == self.Times.len() {
            return self.Values[index - 1];
        }
        let from = self.Values[index - 1];
        let mut to = self.Values[index];
        let blend = (time - self.Times[index - 1]) / (self.Times[index] - self.Times[index - 1]);
        let (a, b) = if self.Property == Property::Rotation {
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
        if self.Property == Property::Rotation {
            let norm = value.iter().map(|v| v * v).sum::<f32>().sqrt();
            value = value.map(|axis| axis / norm);
        }
        value
    }
}

fn ReadClips(
    identity: String,
    path: &Path,
    source: Source,
    selected: &[String],
) -> Result<ClipSet, String> {
    let Source {
        document,
        buffers,
        mut timings,
    } = source;
    let started = Instant::now();
    let mut clips = BTreeMap::new();
    for animation in document.animations() {
        let name = animation
            .name()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("clip-{}", animation.index()));
        if !selected.is_empty() && !selected.contains(&name) {
            continue;
        }
        let mut duration = 0.0_f32;
        let mut tracks = Vec::new();
        let mut targets = std::collections::BTreeSet::new();
        for channel in animation.channels() {
            let node = channel.target().node();
            let step = match channel.sampler().interpolation() {
                Interpolation::Linear => false,
                Interpolation::Step => true,
                _ => {
                    return Err(format!(
                        "clip '{name}' interpolation must be LINEAR or STEP"
                    ));
                }
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
                || times.iter().any(|x| !x.is_finite() || *x < 0.0)
                || times.windows(2).any(|pair| pair[0] >= pair[1])
                || values.iter().flatten().any(|x| !x.is_finite())
                || !targets.insert((node.index(), property))
                || (property == Property::Rotation
                    && values
                        .iter()
                        .any(|q| (q.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() > 0.01))
            {
                return Err(format!(
                    "invalid animation keys or repeated target in '{name}'"
                ));
            }
            duration = duration.max(*times.last().unwrap());
            tracks.push(Track {
                Node: node.index(),
                Name: node.name().map(str::to_owned),
                Property: property,
                Step: step,
                Times: times,
                Values: values,
            });
        }
        if clips
            .insert(
                name.clone(),
                Clip {
                    Source: identity.clone(),
                    source_path: path.to_owned(),
                    Tracks: tracks,
                    Duration: duration,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate clip '{name}'"));
        }
    }
    for name in selected {
        if !clips.contains_key(name) {
            return Err(format!("missing clip '{name}'"));
        }
    }
    timings.Animation = started.elapsed();
    Ok(ClipSet {
        Clips: clips,
        Timings: timings,
    })
}

#[cfg(test)]
mod tests;
