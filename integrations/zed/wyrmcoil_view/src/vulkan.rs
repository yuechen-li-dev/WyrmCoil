use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Instant;

use gpui::{GpuViewportMeshInstance, GpuViewportProducer, GpuViewportProducerSession};
use wyrmcoil::Demo::room::RoomCube;
use wyrmcoil::Engine::render::backend::vulkan::{VulkanRoom, VulkanRoomInstance};
use wyrmcoil::Engine::render::gltf_asset::MeshAtlas;
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
}

impl RoomProducer {
    pub fn New(artifact: SdslvShaderArtifact) -> Self {
        Self {
            artifact,
            stages: OnceLock::new(),
            status: Mutex::default(),
            atlas: None,
        }
    }

    pub fn WithAtlas(artifact: SdslvShaderArtifact, atlas: MeshAtlas) -> Self {
        let mut producer = Self::New(artifact);
        producer.atlas = Some(Arc::new(atlas));
        producer
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
        let room = VulkanRoom::New(
            adapter_luid,
            vertices,
            indices,
            first,
            (vs, vs_entry),
            (ps, ps_entry),
        )
        .map_err(|error| error.to_string())?;
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
}

impl GpuViewportProducerSession for RoomSession {
    fn name(&self) -> &str {
        self.room.DeviceName()
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
