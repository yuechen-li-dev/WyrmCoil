use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{GpuViewportMesh, GpuViewportMeshInstance, GpuViewportShader};
use wyrmcoil::Demo::room::{RoomCube, RoomMatrix};
use wyrmcoil::Engine::asset::gltf::{AssetLibrary, MeshAtlas, PackColor, PaletteMesh};
use wyrmcoil::Engine::shader::sdslv::CompileCanonicalSourceToShaderArtifact;
use wyrmcoil::GameRuntime::{ActorMotion, TacticalArt, TacticalOrder, TacticalView};

#[cfg(target_os = "windows")]
use super::vulkan::SkinSource;
#[cfg(target_os = "windows")]
use wyrmcoil::Engine::asset::gltf::ClipSet;

pub(super) struct TacticalGraphics {
    pub shader: Arc<GpuViewportShader>,
    props: Vec<(RoomMatrix, [u32; 3])>,
    actors: BTreeMap<u16, [PoseRanges; 6]>,
    marker: [u32; 3],
    #[cfg(target_os = "windows")]
    skin: Option<SkinAnimation>,
}

#[cfg(target_os = "windows")]
struct SkinAnimation {
    actor: u16,
    source: Arc<SkinSource>,
    clips: [(Arc<ClipSet>, String); 6],
}

#[derive(Clone)]
struct PoseRanges {
    ranges: Vec<[u32; 3]>,
    clip_ticks: u32,
}

const POSE_HZ: f32 = 10.0;

impl TacticalGraphics {
    pub fn New(root: &Path, art: &TacticalArt, last_watch: bool) -> Result<Self, String> {
        let d3d11 = std::env::var("WYRMCOIL_GRAPHICS_BACKEND")
            .is_ok_and(|value| value.eq_ignore_ascii_case("d3d11"));
        let gpu_skin = last_watch
            && !d3d11
            && !std::env::var("WYRMCOIL_SKINNING")
                .is_ok_and(|value| value.eq_ignore_ascii_case("cpu"));
        Self::Build(root, art, d3d11, gpu_skin)
    }

    pub fn D3dFallback(root: &Path, art: &TacticalArt) -> Result<Self, String> {
        Self::Build(root, art, true, false)
    }

    pub fn SkinStatus(&self, backend: &str) -> &'static str {
        #[cfg(target_os = "windows")]
        if self.skin.is_some() {
            if backend.starts_with("Vulkan · ")
                && !matches!(
                    backend,
                    "Vulkan · initializing" | "Vulkan · waiting for first frame"
                )
            {
                return "Alda · Vulkan vertex skinning";
            }
            return "Alda · Vulkan skinning pending";
        }
        "Characters · CPU cached poses"
    }

    pub fn FallbackReason(&self) -> Option<String> {
        #[cfg(target_os = "windows")]
        if self.skin.is_some()
            && let Some(producer) = &self.shader.producer
        {
            let status = producer.status();
            if status.starts_with("D3D11 fallback") {
                return Some(status);
            }
        }
        None
    }

    fn Build(root: &Path, art: &TacticalArt, d3d11: bool, gpu_skin: bool) -> Result<Self, String> {
        let started = Instant::now();
        if art.Actors.len() > 32 || art.Props.len() > 128 {
            return Err("tactical art exceeds the viewport instance budget".into());
        }
        let mut library = AssetLibrary::New(&root.join("assets"))?;
        let mut palette_time = std::time::Duration::ZERO;
        let mut atlas = MeshAtlas::default();
        let mut props = Vec::new();
        let mut static_ranges = BTreeMap::new();
        for prop in &art.Props {
            if !prop
                .Position
                .into_iter()
                .chain(prop.Scale)
                .chain([prop.Yaw])
                .all(f32::is_finite)
                || prop.Scale.into_iter().any(|axis| axis <= 0.0)
            {
                return Err("invalid tactical prop transform".into());
            }
            let range = if let Some(&cached) = static_ranges.get(&prop.Mesh) {
                cached
            } else {
                let asset = library.Load(Path::new(&prop.Mesh))?;
                let prepared = Instant::now();
                let mesh = PaletteMesh::New(asset)?;
                palette_time += prepared.elapsed();
                let range = mesh.AppendPose(&mut atlas, None, 0.0)?;
                static_ranges.insert(prop.Mesh.clone(), range);
                range
            };
            props.push((
                RoomMatrix::Placement(prop.Position, prop.Scale, prop.Yaw),
                range,
            ));
        }
        let mut clip_files: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for actor in &art.Actors {
            for clip in &actor.Clips {
                clip_files.entry(&clip.File).or_default().insert(&clip.Name);
            }
        }
        let mut clips = BTreeMap::new();
        for (file, selected) in clip_files {
            let names: Vec<_> = selected.into_iter().collect();
            clips.insert(file, library.LoadClips(Path::new(file), &names)?);
        }
        let mut meshes = BTreeMap::new();
        let mut equipment = BTreeMap::new();
        let mut cached_poses = BTreeMap::new();
        let mut actors = BTreeMap::new();
        #[cfg(target_os = "windows")]
        let mut skin = None;
        for actor in &art.Actors {
            let mesh_key = (actor.Mesh.as_str(), actor.Equipment.as_slice());
            if !meshes.contains_key(&mesh_key) {
                let asset = library.Load(Path::new(&actor.Mesh))?;
                if asset.Skins.is_empty() {
                    return Err(format!("{} is missing its character skin", actor.Mesh));
                }
                let prepared = Instant::now();
                let mut mesh = PaletteMesh::New(asset)?;
                palette_time += prepared.elapsed();
                if actor.Equipment.len() > 4 {
                    return Err("too many attachments on a tactical actor".into());
                }
                for attachment in &actor.Equipment {
                    if !equipment.contains_key(&attachment.Mesh) {
                        let asset = library.Load(Path::new(&attachment.Mesh))?;
                        let prepared = Instant::now();
                        equipment.insert(attachment.Mesh.clone(), PaletteMesh::New(asset)?);
                        palette_time += prepared.elapsed();
                    }
                    mesh.Attach(&attachment.Socket, &equipment[&attachment.Mesh])?;
                }
                meshes.insert(mesh_key, mesh);
            }
            let mesh = &meshes[&mesh_key];
            #[cfg(target_os = "windows")]
            if gpu_skin && actor.Id == 0 && actor.Mesh == "characters/Knight.glb" {
                if skin.is_some() {
                    return Err("duplicate tactical actor art".into());
                }
                match mesh.SkinMesh() {
                    Ok(mesh) => {
                        skin = Some(SkinAnimation {
                            actor: actor.Id,
                            source: Arc::new(SkinSource::New(mesh)),
                            clips: actor
                                .Clips
                                .each_ref()
                                .map(|clip| (clips[clip.File.as_str()].clone(), clip.Name.clone())),
                        });
                        continue;
                    }
                    Err(error) => log::warn!("Alda CPU skin fallback: {error}"),
                }
            }
            let mut poses = Vec::new();
            for requested in &actor.Clips {
                let key = (mesh_key, requested.File.as_str(), requested.Name.as_str());
                if let Some(cached) = cached_poses.get(&key) {
                    poses.push(Clone::clone(cached));
                    continue;
                }
                let clip = &clips[requested.File.as_str()].Clips[&requested.Name];
                let samples = ((clip.Duration * POSE_HZ).ceil() as usize).clamp(1, 47) + 1;
                let mut ranges = Vec::with_capacity(samples);
                for index in 0..samples {
                    let time = clip.Duration * index as f32 / (samples - 1) as f32;
                    ranges.push(mesh.AppendPose(&mut atlas, Some(clip), time)?);
                }
                let pose = PoseRanges {
                    ranges,
                    clip_ticks: ClipTicks(clip.Duration),
                };
                cached_poses.insert(key, pose.clone());
                poses.push(pose);
            }
            let poses = poses.try_into().map_err(|_| "invalid actor pose count")?;
            if actors.insert(actor.Id, poses).is_some() {
                return Err("duplicate tactical actor art".into());
            }
        }
        let (vertices, indices) = RoomCube();
        let vertices = vertices.map(|[x, y, z, shade]| [x, y, z, PackColor([shade; 3])]);
        let marker = atlas.Append(&vertices, &indices)?;
        let vertex_bytes = atlas.Vertices.len() * 16;
        let index_bytes = atlas.Indices.len() * 2;
        let total_poses: usize = cached_poses
            .values()
            .map(|pose: &PoseRanges| pose.ranges.len())
            .sum();
        log::info!(
            "Tactical asset timings: {:?}; palette {:.2?}; poses {:?}",
            library.Timings(),
            palette_time,
            atlas.Timings
        );
        let artifact = CompileCanonicalSourceToShaderArtifact(
            "tactical.sdslv",
            include_str!("tactical.sdslv"),
        )
        .map_err(|errors| format!("tactical shader: {errors:?}"))?;
        let mut shader = super::ViewportShader(&artifact)?;
        shader.mesh = Some(GpuViewportMesh {
            vertices: atlas.Vertices.clone(),
            indices: atlas.Indices.clone(),
            first_instance_indices: 0,
        });
        #[cfg(target_os = "windows")]
        if !d3d11 {
            let mut producer = super::vulkan::RoomProducer::WithAtlas(artifact, atlas);
            if let Some(skin) = &skin {
                let artifact = CompileCanonicalSourceToShaderArtifact(
                    "tactical_skin.sdslv",
                    include_str!("tactical_skin.sdslv"),
                )
                .map_err(|errors| format!("tactical skin shader: {errors:?}"))?;
                producer = producer.WithSkin(artifact, skin.source.clone());
            }
            shader.producer = Some(Arc::new(producer));
        }
        log::info!(
            "Tactical art: {} meshes, {} clips, {total_poses} cached poses, {vertex_bytes} vertex bytes, {index_bytes} index bytes, prepared in {:.2?}",
            static_ranges.len() + meshes.len() + equipment.len(),
            clips.values().map(|clips| clips.Clips.len()).sum::<usize>(),
            started.elapsed()
        );
        Ok(Self {
            shader: Arc::new(shader),
            props,
            actors,
            marker,
            #[cfg(target_os = "windows")]
            skin,
        })
    }

    pub fn Instances(
        &self,
        view: &TacticalView,
        projection: RoomMatrix,
    ) -> Vec<GpuViewportMeshInstance> {
        let mut instances =
            Vec::with_capacity(self.props.len() + view.Actors.len() * 2 + view.Orders.len());
        let mut push = |model, color, indices| {
            instances.push(GpuViewportMeshInstance {
                clip_from_model: projection.Multiply(model).0,
                color,
                indices,
            })
        };
        for &(model, range) in &self.props {
            push(model, [1.0; 4], range);
        }
        #[cfg(target_os = "windows")]
        let mut skin_frame = None;
        for actor in &view.Actors {
            let [x, z] = actor.Position;
            let (motion, looped) = match actor.Motion {
                ActorMotion::Idle => (0, true),
                ActorMotion::Walk => (1, true),
                ActorMotion::Attack => (2, false),
                ActorMotion::Ability => (3, false),
                ActorMotion::Hit => (4, false),
                ActorMotion::Death => (5, false),
            };
            let ticks = u32::from(actor.MotionTicks);
            let scale = if actor.PartySlot.is_none() && actor.MaxHealth >= 150 {
                0.96
            } else {
                0.80
            };
            let tint = if actor.PartySlot.is_some() {
                [1.0; 4]
            } else {
                [1.0, 0.64, 0.60, 1.0]
            };
            let model = RoomMatrix::Placement([x, 0.0, z], [scale; 3], -actor.Yaw);
            if let Some(poses) = self.actors.get(&actor.Id) {
                push(model, tint, poses[motion].At(ticks, looped));
            }
            #[cfg(target_os = "windows")]
            if let Some(skin) = &self.skin
                && skin.actor == actor.Id
            {
                let (set, name) = &skin.clips[motion];
                let clip = &set.Clips[name];
                let clip_ticks = ClipTicks(clip.Duration);
                let tick = SampleTick(ticks, looped, clip_ticks);
                let time = clip.Duration * tick as f32 / clip_ticks as f32;
                let [x, y, z, t] = projection.Multiply(model).0;
                skin_frame = Some(([x, y, z, t, tint], motion, tick, clip, time));
            }
            if actor.Health > 0 {
                let marker = if actor.Id == view.Selected {
                    [0.25, 0.82, 1.0, 1.0]
                } else if view.Target == Some(actor.Id) {
                    [1.0, 0.55, 0.28, 1.0]
                } else {
                    continue;
                };
                push(
                    RoomMatrix::Placement([x, 0.01, z], [0.95, 0.05, 0.95], 0.0),
                    marker,
                    self.marker,
                );
            }
        }
        #[cfg(target_os = "windows")]
        if let Some(skin) = &self.skin {
            skin.source.Prepare(skin_frame);
        }
        for order in &view.Orders {
            if let TacticalOrder::MoveTo([x, z]) = *order {
                push(
                    RoomMatrix::Placement([x, 0.025, z], [0.25, 0.10, 0.25], 0.78),
                    [0.30, 0.86, 1.0, 1.0],
                    self.marker,
                );
            }
        }
        instances
    }
}

impl PoseRanges {
    fn At(&self, ticks: u32, looped: bool) -> [u32; 3] {
        let ticks = SampleTick(ticks, looped, self.clip_ticks);
        let index = ticks as usize * (self.ranges.len() - 1) / self.clip_ticks as usize;
        self.ranges[index]
    }
}

fn ClipTicks(duration: f32) -> u32 {
    (duration * 60.0).round().max(1.0) as u32
}

fn SampleTick(ticks: u32, looped: bool, clip_ticks: u32) -> u32 {
    if looped {
        ticks % clip_ticks
    } else {
        ticks.min(clip_ticks)
    }
}
