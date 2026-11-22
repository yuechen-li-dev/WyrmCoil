use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{GpuViewportMesh, GpuViewportMeshInstance, GpuViewportShader};
use wyrmcoil::Demo::room::{RoomCube, RoomMatrix};
use wyrmcoil::Engine::asset::gltf::{AssetLibrary, MeshAtlas, PackColor, PaletteMesh};
use wyrmcoil::Engine::shader::sdslv::CompileCanonicalSourceToShaderArtifact;
use wyrmcoil::GameRuntime::{ActorMotion, TacticalArt, TacticalOrder, TacticalView};

pub(super) struct TacticalGraphics {
    pub shader: Arc<GpuViewportShader>,
    props: Vec<(RoomMatrix, [u32; 3])>,
    actors: BTreeMap<u16, [PoseRanges; 6]>,
    marker: [u32; 3],
}

#[derive(Clone)]
struct PoseRanges {
    ranges: Vec<[u32; 3]>,
    clip_ticks: u32,
}

const POSE_HZ: f32 = 10.0;

impl TacticalGraphics {
    pub fn New(root: &Path, art: &TacticalArt) -> Result<Self, String> {
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
                    clip_ticks: (clip.Duration * 60.0).round().max(1.0) as u32,
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
        if !std::env::var("WYRMCOIL_GRAPHICS_BACKEND")
            .is_ok_and(|value| value.eq_ignore_ascii_case("d3d11"))
        {
            shader.producer = Some(Arc::new(super::vulkan::RoomProducer::WithAtlas(
                artifact, atlas,
            )));
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
        for actor in &view.Actors {
            let [x, z] = actor.Position;
            let Some(poses) = self.actors.get(&actor.Id) else {
                continue;
            };
            let (motion, looped) = match actor.Motion {
                ActorMotion::Idle => (0, true),
                ActorMotion::Walk => (1, true),
                ActorMotion::Attack => (2, false),
                ActorMotion::Ability => (3, false),
                ActorMotion::Hit => (4, false),
                ActorMotion::Death => (5, false),
            };
            let pose = &poses[motion];
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
            push(
                RoomMatrix::Placement([x, 0.0, z], [scale; 3], -actor.Yaw),
                tint,
                pose.At(ticks, looped),
            );
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
        let ticks = if looped {
            ticks % self.clip_ticks
        } else {
            ticks.min(self.clip_ticks)
        };
        let index = ticks as usize * (self.ranges.len() - 1) / self.clip_ticks as usize;
        self.ranges[index]
    }
}
