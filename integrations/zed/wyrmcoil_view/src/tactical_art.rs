use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use gpui::{BackgroundExecutor, GpuViewportMesh, GpuViewportMeshInstance, GpuViewportShader};
use wyrmcoil::Demo::room::{RoomCube, RoomMatrix};
use wyrmcoil::Engine::asset::gltf::{AssetLibrary, MeshAtlas, PackColor, PaletteMesh};
use wyrmcoil::Engine::shader::sdslv::CompileCanonicalSourceToShaderArtifact;
use wyrmcoil::GameRuntime::{ActorMotion, TacticalActor, TacticalArt, TacticalOrder, TacticalView};

use super::tactical::PlayerSelection;
#[cfg(target_os = "windows")]
use super::vulkan::{SkinActor, SkinSource};
#[cfg(target_os = "windows")]
use wyrmcoil::Engine::asset::gltf::{SkinClip, SkinMesh};

pub(super) struct TacticalGraphics {
    pub shader: Arc<GpuViewportShader>,
    props: Vec<(RoomMatrix, [u32; 3])>,
    actors: BTreeMap<u16, [PoseRanges; 6]>,
    marker: [u32; 3],
    #[cfg(target_os = "windows")]
    skin: Option<Arc<SkinSource>>,
}

#[derive(Clone)]
struct PoseRanges {
    ranges: Vec<[u32; 3]>,
    clip_ticks: u32,
}

const POSE_HZ: f32 = 10.0;

impl TacticalGraphics {
    pub fn New(
        root: &Path,
        art: &TacticalArt,
        last_watch: bool,
        fallback: bool,
        executor: Option<BackgroundExecutor>,
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        let d3d11 = fallback
            || std::env::var("WYRMCOIL_GRAPHICS_BACKEND")
                .is_ok_and(|value| value.eq_ignore_ascii_case("d3d11"));
        let count = if last_watch && !d3d11 {
            match std::env::var("WYRMCOIL_SKINNING")
                .ok()
                .map(|value| value.to_ascii_lowercase())
                .as_deref()
            {
                Some("cpu") => 0,
                Some("alda") => 1,
                Some("party") => 3,
                _ => 6,
            }
        } else {
            0
        };
        Self::Build(root, art, d3d11, count, executor, cancelled)
    }

    pub fn Starting(&self) -> bool {
        #[cfg(target_os = "windows")]
        if let Some(producer) = &self.shader.producer {
            return matches!(
                producer.status().as_str(),
                "Vulkan · initializing" | "Vulkan · waiting for first frame"
            );
        }
        false
    }

    pub fn SkinStatus(&self, backend: &str) -> String {
        #[cfg(target_os = "windows")]
        if let Some(skin) = &self.skin {
            if backend.starts_with("Vulkan · ")
                && !matches!(
                    backend,
                    "Vulkan · initializing" | "Vulkan · waiting for first frame"
                )
            {
                return format!(
                    "Vulkan vertex skinning · {} GPU / {} CPU",
                    skin.Count(),
                    self.actors.len()
                );
            }
            return format!("Vulkan skinning pending · {} actors", skin.Count());
        }
        "Characters · CPU cached poses".into()
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

    fn Build(
        root: &Path,
        art: &TacticalArt,
        d3d11: bool,
        gpu_skin: u16,
        executor: Option<BackgroundExecutor>,
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        let started = Instant::now();
        if art.Actors.len() > 32 || art.Props.len() > 128 {
            return Err("tactical art exceeds the viewport instance budget".into());
        }
        let mut library = AssetLibrary::New(&root.join("assets"))?;
        let mut palette_time = std::time::Duration::ZERO;
        let mut color_time = std::time::Duration::ZERO;
        let mut atlas = MeshAtlas::default();
        let mut props = Vec::new();
        let mut static_ranges = BTreeMap::new();
        for prop in &art.Props {
            CheckCancelled(cancelled)?;
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
                color_time += mesh.ColorPreparation;
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
            CheckCancelled(cancelled)?;
            let names: Vec<_> = selected.into_iter().collect();
            clips.insert(file, library.LoadClips(Path::new(file), &names)?);
        }
        let mut meshes = BTreeMap::new();
        let mut equipment = BTreeMap::new();
        let mut cached_poses = BTreeMap::new();
        let mut actors = BTreeMap::new();
        #[cfg(target_os = "windows")]
        let mut bind_meshes: Vec<SkinMesh> = Vec::new();
        #[cfg(target_os = "windows")]
        let mut gpu_meshes = BTreeMap::new();
        #[cfg(target_os = "windows")]
        let mut bound_clips = BTreeMap::<(u32, &str, &str), Arc<SkinClip>>::new();
        #[cfg(target_os = "windows")]
        let mut gpu_actors = Vec::new();
        let mut ids = BTreeSet::new();
        for actor in &art.Actors {
            CheckCancelled(cancelled)?;
            if !ids.insert(actor.Id) {
                return Err("duplicate tactical actor art".into());
            }
            let mesh_key = (actor.Mesh.as_str(), actor.Equipment.as_slice());
            if !meshes.contains_key(&mesh_key) {
                let asset = library.Load(Path::new(&actor.Mesh))?;
                if asset.Skins.is_empty() {
                    return Err(format!("{} is missing its character skin", actor.Mesh));
                }
                let prepared = Instant::now();
                let mut mesh = PaletteMesh::New(asset)?;
                palette_time += prepared.elapsed();
                color_time += mesh.ColorPreparation;
                if actor.Equipment.len() > 4 {
                    return Err("too many attachments on a tactical actor".into());
                }
                for attachment in &actor.Equipment {
                    if !equipment.contains_key(&attachment.Mesh) {
                        let asset = library.Load(Path::new(&attachment.Mesh))?;
                        let prepared = Instant::now();
                        let mesh = PaletteMesh::New(asset)?;
                        color_time += mesh.ColorPreparation;
                        equipment.insert(attachment.Mesh.clone(), mesh);
                        palette_time += prepared.elapsed();
                    }
                    mesh.Attach(&attachment.Socket, &equipment[&attachment.Mesh])?;
                }
                meshes.insert(mesh_key, mesh);
            }
            let mesh = &meshes[&mesh_key];
            #[cfg(target_os = "windows")]
            if actor.Id < gpu_skin {
                let start = Instant::now();
                let binding = (|| -> Result<SkinActor, String> {
                    let mesh_index = match gpu_meshes.get(&mesh_key) {
                        Some(&mesh_index) => mesh_index,
                        None => {
                            let mesh_index = bind_meshes.len() as u32;
                            bind_meshes.push(mesh.SkinMesh()?);
                            gpu_meshes.insert(mesh_key, mesh_index);
                            mesh_index
                        }
                    };
                    let mut bindings = Vec::with_capacity(actor.Clips.len());
                    for clip in &actor.Clips {
                        let key = (mesh_index, clip.File.as_str(), clip.Name.as_str());
                        if let Some(binding) = bound_clips.get(&key) {
                            bindings.push(binding.clone());
                        } else {
                            let mesh = &bind_meshes[mesh_index as usize];
                            let binding = Arc::new(
                                mesh.BindClip(clips[clip.File.as_str()].clone(), &clip.Name)?,
                            );
                            bound_clips.insert(key, binding.clone());
                            bindings.push(binding);
                        }
                    }
                    Ok(SkinActor {
                        id: actor.Id,
                        mesh: mesh_index,
                        clips: bindings.try_into().map_err(|_| "invalid GPU clip count")?,
                    })
                })();
                palette_time += start.elapsed();
                match binding {
                    Ok(binding) => {
                        gpu_actors.push(binding);
                        continue;
                    }
                    Err(error) => log::warn!("Actor {} CPU skin fallback: {error}", actor.Id),
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
                    CheckCancelled(cancelled)?;
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
            "Tactical asset timings: {:?}; model and palette {:.2?}; sample and pack {:.2?}; poses {:?}",
            library.Timings(),
            palette_time,
            color_time,
            atlas.Timings
        );
        CheckCancelled(cancelled)?;
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
        let skin = if gpu_actors.is_empty() {
            None
        } else {
            gpu_actors.sort_by_key(|actor| (actor.mesh, actor.id));
            Some(Arc::new(SkinSource::New(bind_meshes, gpu_actors)))
        };
        #[cfg(target_os = "windows")]
        if !d3d11 {
            let mut producer = super::vulkan::RoomProducer::WithAtlas(artifact, atlas, executor);
            if let Some(skin) = &skin {
                let artifact = CompileCanonicalSourceToShaderArtifact(
                    "tactical_skin.sdslv",
                    include_str!("tactical_skin.sdslv"),
                )
                .map_err(|errors| format!("tactical skin shader: {errors:?}"))?;
                producer = producer.WithSkin(artifact, skin.clone());
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
        let selected = PlayerSelection(view);
        for actor in &view.Actors {
            if let Some(poses) = self.actors.get(&actor.Id) {
                let (motion, looped) = Motion(actor.Motion);
                let (model, tint) = ActorPlacement(actor);
                push(
                    model,
                    tint,
                    poses[motion].At(u32::from(actor.MotionTicks), looped),
                );
            }
            let [x, z] = actor.Position;
            if actor.Health > 0 {
                let marker = if selected == Some(actor.Id) {
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
            skin.Prepare(view, projection);
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

fn CheckCancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("tactical art preparation cancelled".into());
    }
    Ok(())
}

impl PoseRanges {
    fn At(&self, ticks: u32, looped: bool) -> [u32; 3] {
        let ticks = SampleTick(ticks, looped, self.clip_ticks);
        let index = ticks as usize * (self.ranges.len() - 1) / self.clip_ticks as usize;
        self.ranges[index]
    }
}

pub(super) fn ClipTicks(duration: f32) -> u32 {
    (duration * 60.0).round().max(1.0) as u32
}

pub(super) fn SampleTick(ticks: u32, looped: bool, clip_ticks: u32) -> u32 {
    if looped {
        ticks % clip_ticks
    } else {
        ticks.min(clip_ticks)
    }
}

pub(super) fn Motion(motion: ActorMotion) -> (usize, bool) {
    match motion {
        ActorMotion::Idle => (0, true),
        ActorMotion::Walk => (1, true),
        ActorMotion::Attack => (2, false),
        ActorMotion::Ability => (3, false),
        ActorMotion::Hit => (4, false),
        ActorMotion::Death => (5, false),
    }
}

pub(super) fn ActorPlacement(actor: &TacticalActor) -> (RoomMatrix, [f32; 4]) {
    let [x, z] = actor.Position;
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
    (
        RoomMatrix::Placement([x, 0.0, z], [scale; 3], -actor.Yaw),
        tint,
    )
}
