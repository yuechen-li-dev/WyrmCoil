use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::{
    BackgroundExecutor, GpuViewportMeshInstance, GpuViewportProducer, GpuViewportProducerSession,
    Task,
};
use wyrmcoil::Demo::room::{RoomCube, RoomMatrix};
use wyrmcoil::Engine::asset::gltf::{MeshAtlas, SkinClip, SkinMesh, SkinPose};
use wyrmcoil::Engine::render::backend::vulkan::{
    StartupStage, VulkanRoom, VulkanRoomInstance, VulkanSkinGeometry, VulkanSkinInstance,
};
use wyrmcoil::Engine::shader::sdslv::SdslvShaderArtifact;
use wyrmcoil::Engine::shader::sdslv::spirv_cache::{DxcSpirvCompiler, SpirvConfig};

type SpirvStage = (Vec<u8>, CString);

#[derive(Debug, Default)]
struct Status {
    text: String,
    changed: bool,
    pending: bool,
}

pub(super) struct RoomProducer {
    executor: Option<BackgroundExecutor>,
    artifact: SdslvShaderArtifact,
    compiler: OnceLock<Result<DxcSpirvCompiler, String>>,
    stages: OnceLock<Result<[SpirvStage; 2], String>>,
    status: Mutex<Status>,
    atlas: Option<Arc<MeshAtlas>>,
    skin: Option<(SdslvShaderArtifact, Arc<SkinSource>)>,
    skin_stages: OnceLock<Result<[SpirvStage; 2], String>>,
}

impl std::fmt::Debug for RoomProducer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomProducer").finish_non_exhaustive()
    }
}

pub(super) struct SkinActor {
    pub id: u16,
    pub mesh: u32,
    pub clips: [Arc<SkinClip>; 6],
}

pub(super) struct SkinSource {
    meshes: Vec<SkinMesh>,
    actors: Vec<SkinActor>,
    frame: Mutex<SkinFrame>,
}

impl std::fmt::Debug for SkinSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkinSource")
            .field("meshes", &self.meshes)
            .field("actors", &self.actors.len())
            .finish()
    }
}

#[derive(Default)]
struct ActorPose {
    sample: Option<(usize, u32)>,
    instance: Option<VulkanSkinInstance>,
}

struct SkinFrame {
    actors: Vec<ActorPose>,
    draws: Vec<VulkanSkinInstance>,
    scratch: SkinPose,
    palette: Vec<[[f32; 4]; 4]>,
    revision: u64,
    errors: Option<String>,
    samples: u64,
    evaluate: Duration,
}

impl SkinSource {
    pub fn New(meshes: Vec<SkinMesh>, actors: Vec<SkinActor>) -> Self {
        let slots = actors
            .iter()
            .map(|actor| meshes[actor.mesh as usize].PaletteSize())
            .sum();
        let count = actors.len();
        Self {
            meshes,
            actors,
            frame: Mutex::new(SkinFrame {
                actors: (0..count).map(|_| ActorPose::default()).collect(),
                draws: Vec::with_capacity(count),
                scratch: SkinPose::default(),
                palette: vec![[[0.0; 4]; 4]; slots],
                revision: 0,
                errors: None,
                samples: 0,
                evaluate: Duration::ZERO,
            }),
        }
    }

    pub fn Count(&self) -> usize {
        self.actors.len()
    }

    pub fn Prepare(&self, view: &wyrmcoil::GameRuntime::TacticalView, projection: RoomMatrix) {
        let mut lock = self.frame.lock().unwrap();
        let frame = &mut *lock;
        if frame.errors.is_some() {
            return;
        }
        let mut base = 0;
        frame.draws.clear();
        for (actor, pose) in self.actors.iter().zip(&mut frame.actors) {
            let mesh = &self.meshes[actor.mesh as usize];
            let end = base + mesh.PaletteSize();
            let offset = base;
            base = end;
            let Some(state) = view.Actors.iter().find(|state| state.Id == actor.id) else {
                if pose.instance.take().is_some() {
                    frame.revision += 1;
                }
                continue;
            };
            let (motion, looped) = super::tactical_art::Motion(state.Motion);
            let clip = &actor.clips[motion];
            let clip_ticks = super::tactical_art::ClipTicks(clip.Duration());
            let tick =
                super::tactical_art::SampleTick(u32::from(state.MotionTicks), looped, clip_ticks);
            if pose.sample != Some((motion, tick)) {
                let now = Instant::now();
                let time = clip.Duration() * tick as f32 / clip_ticks as f32;
                if let Err(error) = mesh.Sample(
                    clip,
                    time,
                    &mut frame.scratch,
                    &mut frame.palette[offset..end],
                ) {
                    frame.errors = Some(format!("actor {} skin: {error}", actor.id));
                    frame.revision += 1;
                    return;
                }
                pose.sample = Some((motion, tick));
                frame.revision += 1;
                frame.samples += 1;
                frame.evaluate += now.elapsed();
            }
            let (model, tint) = super::tactical_art::ActorPlacement(state);
            let [x, y, z, t] = projection.Multiply(model).0;
            let draw = VulkanSkinInstance {
                Transform: [x, y, z, t, tint],
                PaletteBase: offset as u32,
                Mesh: actor.mesh,
            };
            if pose.instance != Some(draw) {
                pose.instance = Some(draw);
                frame.revision += 1;
            }
            frame.draws.push(draw);
        }
    }
}

impl RoomProducer {
    pub fn New(artifact: SdslvShaderArtifact, executor: Option<BackgroundExecutor>) -> Self {
        Self {
            executor,
            artifact,
            compiler: OnceLock::new(),
            stages: OnceLock::new(),
            status: Mutex::default(),
            atlas: None,
            skin: None,
            skin_stages: OnceLock::new(),
        }
    }

    pub fn WithAtlas(
        artifact: SdslvShaderArtifact,
        atlas: MeshAtlas,
        executor: Option<BackgroundExecutor>,
    ) -> Self {
        let mut producer = Self::New(artifact, executor);
        producer.atlas = Some(Arc::new(atlas));
        producer
    }

    pub fn WithSkin(mut self, artifact: SdslvShaderArtifact, source: Arc<SkinSource>) -> Self {
        self.skin = Some((artifact, source));
        self
    }
    fn Build(&self, adapter_luid: [u8; 8]) -> Result<Box<dyn GpuViewportProducerSession>, String> {
        let start = Instant::now();
        let _startup = StartupStage::Begin("producer_build");
        let compiler = || {
            self.compiler
                .get_or_init(|| DxcSpirvCompiler::New(&DxcPath()))
                .as_ref()
                .map_err(Clone::clone)
        };
        let [(vs, vs_entry), (ps, ps_entry)] = self
            .stages
            .get_or_init(|| {
                StartupStage::Call("room_shader_artifacts", || {
                    CompileRoom(&self.artifact, compiler()?)
                })
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let mut shader_time = start.elapsed();
        let (vertices, indices) = RoomCube();
        let (vertices, indices, first) = self
            .atlas
            .as_ref()
            .map_or((vertices.as_slice(), indices.as_slice(), 6), |atlas| {
                (atlas.Vertices.as_slice(), atlas.Indices.as_slice(), 0)
            });
        let mut room = VulkanRoom::New(
            adapter_luid,
            vertices,
            indices,
            first,
            (vs, vs_entry),
            (ps, ps_entry),
        )
        .map_err(|error| error.to_string())?;
        let mut skin = None;
        if let Some((artifact, source)) = &self.skin {
            let shader_start = Instant::now();
            let [(vs, vs_name), (ps, ps_name)] = self
                .skin_stages
                .get_or_init(|| {
                    StartupStage::Call("skin_shader_artifacts", || {
                        CompileRoom(artifact, compiler()?)
                    })
                })
                .as_ref()
                .map_err(Clone::clone)?;
            shader_time += shader_start.elapsed();
            let geometry: Vec<_> = source
                .meshes
                .iter()
                .map(|mesh| VulkanSkinGeometry {
                    Vertices: &mesh.Vertices,
                    Indices: &mesh.Indices,
                    Joints: mesh.PaletteSize(),
                })
                .collect();
            let slots = source.frame.lock().unwrap().palette.len();
            let upload = Instant::now();
            room.ConfigureSkin(
                &geometry,
                slots,
                source.Count(),
                (vs, vs_name),
                (ps, ps_name),
            )?;
            let vertices: usize = geometry.iter().map(|mesh| size_of_val(mesh.Vertices)).sum();
            let indices: usize = geometry.iter().map(|mesh| size_of_val(mesh.Indices)).sum();
            log::info!(
                "Vulkan skin: {} actors, {} meshes, {vertices} bind vertex bytes, {indices} index bytes, {} palette bytes, prepared in {:.2?}",
                source.Count(),
                geometry.len(),
                slots * 64,
                upload.elapsed()
            );
            skin = Some(source.clone());
        }
        if std::env::var("WYRMCOIL_GPU_TIMING").as_deref() == Ok("1")
            && let Err(error) = room.EnableDrawTimings()
        {
            log::warn!("Vulkan timestamp queries: {error}");
        }
        log::info!(
            "GPU viewport: {} Vulkan/SPIR-V session ready in {:.2?} (shader artifacts {:.2?}; static mesh buffer allocation and upload {:.2?})",
            room.DeviceName(),
            start.elapsed(),
            shader_time,
            room.MeshUploadTime(),
        );
        Ok(Box::new(RoomSession {
            room,
            instances: Vec::new(),
            ranges: Vec::new(),
            explicit: self.atlas.is_some(),
            skin,
            rendered: 0,
            created: Instant::now(),
        }))
    }
}

impl GpuViewportProducer for RoomProducer {
    fn create(
        self: Arc<Self>,
        adapter_luid: [u8; 8],
    ) -> Task<Result<Box<dyn GpuViewportProducerSession>, String>> {
        if let Some(executor) = self.executor.clone() {
            return executor.spawn(async move { self.Build(adapter_luid) });
        }
        Task::ready(self.Build(adapter_luid))
    }

    fn report(&self, text: String, pending: bool) {
        let mut status = self.status.lock().unwrap();
        if status.text != text {
            status.text = text;
            status.changed = true;
        }
        status.pending = pending;
    }

    fn status(&self) -> String {
        let status = self.status.lock().unwrap();
        if status.text.is_empty() {
            return "Vulkan · initializing".into();
        }
        status.text.clone()
    }

    fn pending(&self) -> bool {
        let mut status = self.status.lock().unwrap();
        std::mem::take(&mut status.changed) || status.pending
    }
}

struct RoomSession {
    room: VulkanRoom,
    instances: Vec<VulkanRoomInstance>,
    ranges: Vec<[u32; 3]>,
    explicit: bool,
    skin: Option<Arc<SkinSource>>,
    rendered: u64,
    created: Instant,
}

impl Drop for RoomSession {
    fn drop(&mut self) {
        if let Some(source) = &self.skin {
            let frame = source.frame.lock().unwrap();
            let (uploads, bytes, upload_time) = self.room.SkinUploadStats();
            log::info!(
                "Vulkan skin retired: {} frames submitted in {:.2?}, {} CPU palette evaluations in {:.2?}, {uploads} palette uploads ({bytes} bytes) in {:.2?}",
                self.rendered,
                self.created.elapsed(),
                frame.samples,
                frame.evaluate,
                upload_time
            );
        }
        if let Some((frames, draw_time, skins, skin_time)) = self.room.DrawTimingStats() {
            log::info!(
                "Vulkan GPU timings: {frames} room renders in {:.2?}; {skins} skin draws in {:.2?}",
                draw_time,
                skin_time
            );
        }
    }
}

impl GpuViewportProducerSession for RoomSession {
    fn name(&self) -> &str {
        self.room.DeviceName()
    }

    fn revision(&self) -> u64 {
        self.skin
            .as_ref()
            .map_or(0, |source| source.frame.lock().unwrap().revision)
    }

    fn ready(&self) -> Result<bool, String> {
        self.room.IsReady().map_err(|error| error.to_string())
    }

    unsafe fn attach(&mut self, nt_handle: usize, width: u32, height: u32) -> Result<(), String> {
        // GPUI created the live BGRA8 texture on the matched adapter.
        unsafe { self.room.Attach(nt_handle, width, height) }.map_err(|error| error.to_string())
    }

    unsafe fn submit(&mut self, instances: &[GpuViewportMeshInstance]) -> Result<bool, String> {
        self.instances.clear();
        self.instances.extend(instances.iter().map(|instance| {
            let [x, y, z, t] = instance.clip_from_model;
            [x, y, z, t, instance.color]
        }));
        self.ranges.clear();
        if self.explicit {
            self.ranges
                .extend(instances.iter().map(|instance| instance.indices));
        }
        // GPUI has handed ownership back at key 0 or created a new texture.
        if let Some(source) = &self.skin {
            let frame = source.frame.lock().unwrap();
            if let Some(error) = &frame.errors {
                return Err(error.clone());
            }
            if !frame.draws.is_empty() {
                let submitted = unsafe {
                    self.room.SubmitSkin(
                        &self.instances,
                        &self.ranges,
                        &frame.draws,
                        &frame.palette,
                    )
                }?;
                self.rendered += u64::from(submitted);
                return Ok(submitted);
            }
        }
        unsafe { self.room.SubmitRanges(&self.instances, &self.ranges) }
            .map_err(|error| error.to_string())
    }
}

fn CompileRoom(
    artifact: &SdslvShaderArtifact,
    compiler: &DxcSpirvCompiler,
) -> Result<[SpirvStage; 2], String> {
    let [vertex, pixel] = artifact.EntryPoints.as_slice() else {
        return Err("expected exactly two room shader entries".into());
    };
    let compile = |entry| {
        let spirv = compiler.Compile(artifact, entry, SpirvConfig::VULKAN_13)?;
        let name = CString::new(entry.Name.as_str()).map_err(|error| error.to_string())?;
        Ok::<_, String>((spirv, name))
    };
    Ok([compile(vertex)?, compile(pixel)?])
}

fn DxcPath() -> String {
    if let Ok(path) = std::env::var("WYRMCOIL_DXC") {
        return path;
    }
    for variable in ["VULKAN_SDK", "VK_SDK_PATH"] {
        if let Some(root) = std::env::var_os(variable) {
            let path = PathBuf::from(root).join("Bin").join("dxc.exe");
            if path.is_file() {
                return path.to_string_lossy().into_owned();
            }
        }
    }
    "dxc".into()
}
