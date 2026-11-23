use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::{GpuViewportMeshInstance, GpuViewportProducer, GpuViewportProducerSession};
use wyrmcoil::Demo::room::RoomCube;
use wyrmcoil::Engine::asset::gltf::{Clip, MeshAtlas, SkinMesh};
use wyrmcoil::Engine::render::backend::vulkan::{VulkanRoom, VulkanRoomInstance};
use wyrmcoil::Engine::shader::sdslv::{
    CompileArtifactEntryWithDxc, DxcOptions, SdslvShaderArtifact,
};

type SpirvStage = (Vec<u8>, CString);

#[derive(Debug, Default)]
struct Status {
    text: String,
    changed: bool,
    pending: bool,
}

#[derive(Debug)]
pub(super) struct RoomProducer {
    artifact: SdslvShaderArtifact,
    stages: OnceLock<Result<[SpirvStage; 2], String>>,
    status: Mutex<Status>,
    atlas: Option<Arc<MeshAtlas>>,
    skin: Option<(SdslvShaderArtifact, Arc<SkinSource>)>,
}

#[derive(Debug)]
pub(super) struct SkinSource {
    mesh: SkinMesh,
    frame: Mutex<SkinFrame>,
}

#[derive(Debug, Default)]
struct SkinFrame {
    instance: Option<VulkanRoomInstance>,
    sample: Option<(usize, u32)>,
    palette: Vec<[[f32; 4]; 4]>,
    revision: u64,
    errors: Option<String>,
    samples: u64,
    evaluate: Duration,
}

impl SkinSource {
    pub fn New(mesh: SkinMesh) -> Self {
        Self {
            mesh,
            frame: Mutex::default(),
        }
    }

    pub fn Prepare(&self, draw: Option<(VulkanRoomInstance, usize, u32, &Clip, f32)>) {
        let mut frame = self.frame.lock().unwrap();
        let Some((instance, motion, tick, clip, time)) = draw else {
            if frame.instance.take().is_some() {
                frame.revision += 1;
            }
            return;
        };
        if frame.sample != Some((motion, tick)) {
            let now = Instant::now();
            match self.mesh.Palette(Some(clip), time) {
                Ok(palette) => {
                    frame.palette = palette;
                    frame.sample = Some((motion, tick));
                    frame.revision += 1;
                    frame.samples += 1;
                    frame.evaluate += now.elapsed();
                }
                Err(error) => {
                    frame.errors = Some(error);
                    frame.revision += 1;
                }
            }
        }
        if frame.instance != Some(instance) {
            frame.instance = Some(instance);
            frame.revision += 1;
        }
    }
}

impl RoomProducer {
    pub fn New(artifact: SdslvShaderArtifact) -> Self {
        Self {
            artifact,
            stages: OnceLock::new(),
            status: Mutex::default(),
            atlas: None,
            skin: None,
        }
    }

    pub fn WithAtlas(artifact: SdslvShaderArtifact, atlas: MeshAtlas) -> Self {
        let mut producer = Self::New(artifact);
        producer.atlas = Some(Arc::new(atlas));
        producer
    }

    pub fn WithSkin(mut self, artifact: SdslvShaderArtifact, source: Arc<SkinSource>) -> Self {
        self.skin = Some((artifact, source));
        self
    }
}

impl GpuViewportProducer for RoomProducer {
    fn create(&self, adapter_luid: [u8; 8]) -> Result<Box<dyn GpuViewportProducerSession>, String> {
        let start = Instant::now();
        let [(vs, vs_entry), (ps, ps_entry)] = self
            .stages
            .get_or_init(|| CompileRoom(&self.artifact))
            .as_ref()
            .map_err(Clone::clone)?;
        let shader_time = start.elapsed();
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
            let [(vs, vs_name), (ps, ps_name)] = CompileRoom(artifact)?;
            let mesh = &source.mesh;
            room.ConfigureSkin(
                &mesh.Vertices,
                &mesh.Indices,
                mesh.PaletteSize(),
                (&vs, &vs_name),
                (&ps, &ps_name),
            )?;
            log::info!(
                "Vulkan skin: {} bind vertex bytes, {} index bytes, {} byte palette in {} slots",
                size_of_val(mesh.Vertices.as_slice()),
                size_of_val(mesh.Indices.as_slice()),
                mesh.PaletteSize() * 64,
                mesh.PaletteSize()
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
            let (uploads, upload_time) = self.room.SkinUploadStats();
            log::info!(
                "Vulkan skin retired: {} frames submitted in {:.2?}, {} CPU palette evaluations in {:.2?}, {uploads} palette uploads in {:.2?}",
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
            if let Some(instance) = frame.instance {
                let first = self.instances.len();
                self.instances.push(instance);
                let submitted = unsafe {
                    self.room
                        .SubmitSkin(&self.instances, &self.ranges, first, &frame.palette)
                }?;
                self.rendered += u64::from(submitted);
                return Ok(submitted);
            }
        }
        unsafe { self.room.SubmitRanges(&self.instances, &self.ranges) }
            .map_err(|error| error.to_string())
    }
}

fn CompileRoom(artifact: &SdslvShaderArtifact) -> Result<[SpirvStage; 2], String> {
    let [vertex, pixel] = artifact.EntryPoints.as_slice() else {
        return Err("expected exactly two room shader entries".into());
    };
    let options = DxcOptions {
        DxcPath: DxcPath(),
        ExtraArgs: vec!["-fspv-target-env=vulkan1.3".into()],
        ..Default::default()
    };
    let compile = |entry| {
        let spirv = CompileArtifactEntryWithDxc(artifact, entry, &options)
            .map_err(|error| format!("DXC SPIR-V: {error:?}"))?;
        let name = CString::new(entry.Name.as_str()).map_err(|error| error.to_string())?;
        Ok::<_, String>((spirv.OutputBytes, name))
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
