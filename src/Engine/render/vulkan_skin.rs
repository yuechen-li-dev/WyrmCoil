use super::{
    Buffer, Check, CreatePipeline, Device, Result, StartupStage, VulkanSkinGeometry,
    VulkanSkinInstance, vk,
};
use std::ffi::CStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

type JointMatrix = [[f32; 4]; 4];

struct BindRange {
    first: u32,
    count: u32,
    vertex: i32,
    joints: usize,
}

pub(super) struct SkinPass {
    device: Arc<Device>,
    vertices: Buffer,
    indices: Buffer,
    meshes: Vec<BindRange>,
    instances: Buffer,
    palette: Buffer,
    last_palette: Vec<JointMatrix>,
    pool: vk::DescriptorPool,
    set_layout: vk::DescriptorSetLayout,
    set: vk::DescriptorSet,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    pub(super) upload_time: Duration,
    pub(super) uploads: u64,
    pub(super) uploaded_bytes: u64,
}

impl SkinPass {
    pub(super) fn New(
        device: &Arc<Device>,
        geometry: &[VulkanSkinGeometry<'_>],
        slots: usize,
        capacity: usize,
        vertex: (&[u8], &CStr),
        pixel: (&[u8], &CStr),
        color: vk::Format,
    ) -> Result<Self> {
        let bytes = slots
            .checked_mul(size_of::<JointMatrix>())
            .filter(|&bytes| bytes > 0 && bytes <= device.max_storage)
            .ok_or("Vulkan joint palette exceeds the device storage buffer range")?;
        if geometry.is_empty() || !(1..=super::MAX_INSTANCES).contains(&capacity) {
            return Err("invalid Vulkan skin mesh or instance capacity".into());
        }
        let mut meshes = Vec::with_capacity(geometry.len());
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for mesh in geometry {
            if mesh.Indices.is_empty()
                || !mesh.Indices.len().is_multiple_of(3)
                || mesh
                    .Indices
                    .iter()
                    .any(|&index| usize::from(index) >= mesh.Vertices.len())
                || mesh.Joints == 0
                || mesh.Joints > slots
            {
                return Err("invalid Vulkan skin geometry range".into());
            }
            for vertex in mesh.Vertices {
                if vertex
                    .Joints
                    .iter()
                    .any(|&joint| joint as usize >= mesh.Joints)
                    || vertex
                        .Weights
                        .iter()
                        .any(|weight| !weight.is_finite() || *weight < 0.0)
                    || (vertex.Weights.iter().sum::<f32>() - 1.0).abs() > 0.01
                    || vertex.PositionColor.iter().any(|value| !value.is_finite())
                {
                    return Err("invalid Vulkan skin joint or vertex weight".into());
                }
            }
            meshes.push(BindRange {
                first: u32::try_from(indices.len()).map_err(|_| "too many Vulkan skin indices")?,
                count: u32::try_from(mesh.Indices.len())
                    .map_err(|_| "too many Vulkan skin indices")?,
                vertex: i32::try_from(vertices.len())
                    .map_err(|_| "too many Vulkan skin vertices")?,
                joints: mesh.Joints,
            });
            vertices.extend_from_slice(mesh.Vertices);
            indices.extend_from_slice(mesh.Indices);
        }
        let buffers = StartupStage::Begin("skin_buffers_and_upload");
        let vertex_buffer = Buffer::New(
            device,
            size_of_val(vertices.as_slice()),
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        vertex_buffer.Upload(&vertices)?;
        let index_buffer = Buffer::New(
            device,
            size_of_val(indices.as_slice()),
            vk::BufferUsageFlags::INDEX_BUFFER,
        )?;
        index_buffer.Upload(&indices)?;
        let mut skin = Self {
            device: device.clone(),
            vertices: vertex_buffer,
            indices: index_buffer,
            meshes,
            instances: Buffer::New(
                device,
                capacity * size_of::<VulkanSkinInstance>(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )?,
            palette: Buffer::New(device, bytes, vk::BufferUsageFlags::STORAGE_BUFFER)?,
            last_palette: Vec::with_capacity(slots),
            pool: vk::DescriptorPool::null(),
            set_layout: vk::DescriptorSetLayout::null(),
            set: vk::DescriptorSet::null(),
            layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            upload_time: Duration::ZERO,
            uploads: 0,
            uploaded_bytes: 0,
        };
        drop(buffers);
        // All handles belong to the in-construction pass and are dropped on error.
        unsafe {
            let raw = &device.raw;
            let descriptor = StartupStage::Begin("skin_descriptors");
            let bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX)];
            skin.set_layout = Check(
                "Create Vulkan skin descriptor layout",
                raw.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                ),
            )?;
            let sizes = [vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
            }];
            skin.pool = Check(
                "Create Vulkan skin descriptor pool",
                raw.create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&sizes),
                    None,
                ),
            )?;
            let layouts = [skin.set_layout];
            skin.set = Check(
                "Allocate Vulkan skin descriptor",
                raw.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(skin.pool)
                        .set_layouts(&layouts),
                ),
            )?[0];
            let buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(skin.palette.raw)
                .range(bytes as u64)];
            raw.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(skin.set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&buffer_info)],
                &[],
            );
            skin.layout = Check(
                "Create Vulkan skin pipeline layout",
                raw.create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts),
                    None,
                ),
            )?;
            drop(descriptor);
            skin.pipeline = CreatePipeline(raw, skin.layout, vertex, pixel, true, color)?;
        }
        Ok(skin)
    }

    /// The room's submission fence must have completed before any upload.
    pub(super) fn Prepare(
        &mut self,
        draws: &[VulkanSkinInstance],
        palette: &[JointMatrix],
    ) -> Result<()> {
        let mut end = 0;
        for draw in draws {
            let mesh = self
                .meshes
                .get(draw.Mesh as usize)
                .ok_or("Vulkan skin mesh is missing")?;
            let base = draw.PaletteBase as usize;
            if base < end
                || base
                    .checked_add(mesh.joints)
                    .is_none_or(|end| end > palette.len())
            {
                return Err("overlapping or out-of-range Vulkan skin palette".into());
            }
            end = base + mesh.joints;
        }
        self.instances.Upload(draws)?;
        self.Upload(palette)
    }

    pub(super) fn Upload(&mut self, palette: &[JointMatrix]) -> Result<()> {
        if size_of_val(palette) != self.palette.size {
            return Err("wrong Vulkan joint palette length".into());
        }
        let now = Instant::now();
        let (first, end) = if self.last_palette.len() != palette.len() {
            (0, palette.len())
        } else {
            let mut before = self.last_palette.iter().zip(palette);
            let Some(first) = before.clone().position(|(old, new)| old != new) else {
                return Ok(());
            };
            let end = before.rposition(|(old, new)| old != new).unwrap() + 1;
            (first, end)
        };
        self.palette
            .UploadAt(first * size_of::<JointMatrix>(), &palette[first..end])?;
        if self.last_palette.is_empty() {
            self.last_palette.extend_from_slice(palette);
        } else {
            self.last_palette[first..end].copy_from_slice(&palette[first..end]);
        }
        self.upload_time += now.elapsed();
        self.uploads += 1;
        self.uploaded_bytes += size_of_val(&palette[first..end]) as u64;
        Ok(())
    }

    /// Record inside the room's active graphics pass after Prepare.
    pub(super) fn Draw(&self, command: vk::CommandBuffer, draws: &[VulkanSkinInstance]) {
        unsafe {
            let raw = &self.device.raw;
            raw.cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            raw.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &[self.set],
                &[],
            );
            raw.cmd_bind_vertex_buffers(
                command,
                0,
                &[self.vertices.raw, self.instances.raw],
                &[0, 0],
            );
            raw.cmd_bind_index_buffer(command, self.indices.raw, 0, vk::IndexType::UINT16);
            let mut first = 0;
            while first < draws.len() {
                let mesh = &self.meshes[draws[first].Mesh as usize];
                let mut end = first + 1;
                while end < draws.len() && draws[end].Mesh == draws[first].Mesh {
                    end += 1;
                }
                raw.cmd_draw_indexed(
                    command,
                    mesh.count,
                    (end - first) as u32,
                    mesh.first,
                    mesh.vertex,
                    first as u32,
                );
                first = end;
            }
        }
    }
}

impl Drop for SkinPass {
    fn drop(&mut self) {
        // VulkanRoom retires its submission before dropping the pass.
        unsafe {
            let raw = &self.device.raw;
            raw.destroy_pipeline(self.pipeline, None);
            raw.destroy_pipeline_layout(self.layout, None);
            raw.destroy_descriptor_pool(self.pool, None);
            raw.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

pub(super) struct DrawTimings {
    device: Arc<Device>,
    pool: vk::QueryPool,
    recorded: bool,
    skinned: bool,
    frames: u64,
    frame_ns: f64,
    skins: u64,
    skin_ns: f64,
}

impl DrawTimings {
    pub(super) fn New(device: &Arc<Device>) -> Result<Option<Self>> {
        if device.timestamp_bits == 0 {
            return Ok(None);
        }
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(4);
        let pool = Check("Create Vulkan timestamp queries", unsafe {
            device.raw.create_query_pool(&info, None)
        })?;
        Ok(Some(Self {
            device: device.clone(),
            pool,
            recorded: false,
            skinned: false,
            frames: 0,
            frame_ns: 0.0,
            skins: 0,
            skin_ns: 0.0,
        }))
    }

    pub(super) fn Begin(&self, command: vk::CommandBuffer) {
        // Query reuse is guarded by the room fence and occurs before rendering.
        unsafe {
            self.device
                .raw
                .cmd_reset_query_pool(command, self.pool, 0, 4);
            self.device.raw.cmd_write_timestamp(
                command,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pool,
                0,
            );
        }
    }

    pub(super) fn Mark(&self, command: vk::CommandBuffer, index: u32) {
        unsafe {
            self.device.raw.cmd_write_timestamp(
                command,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.pool,
                index,
            );
        }
    }

    pub(super) fn Recorded(&mut self, skinned: bool) {
        self.recorded = true;
        self.skinned = skinned;
    }

    pub(super) fn Collect(&mut self) {
        if !self.recorded {
            return;
        }
        self.recorded = false;
        let mut stamps = [0u64; 4];
        let count = if self.skinned { 4 } else { 2 };
        let result = unsafe {
            self.device.raw.get_query_pool_results(
                self.pool,
                0,
                &mut stamps[..count],
                vk::QueryResultFlags::TYPE_64,
            )
        };
        if result.is_err() {
            return;
        }
        let mask = u64::MAX >> (64 - self.device.timestamp_bits);
        let nanos = |start: u64, end: u64| {
            (end.wrapping_sub(start) & mask) as f64 * f64::from(self.device.timestamp_period)
        };
        self.frames += 1;
        self.frame_ns += nanos(stamps[0], stamps[1]);
        if self.skinned {
            self.skins += 1;
            self.skin_ns += nanos(stamps[2], stamps[3]);
        }
    }

    pub(super) fn Stats(&self) -> (u64, Duration, u64, Duration) {
        (
            self.frames,
            Duration::from_secs_f64(self.frame_ns * 1e-9),
            self.skins,
            Duration::from_secs_f64(self.skin_ns * 1e-9),
        )
    }
}

impl Drop for DrawTimings {
    fn drop(&mut self) {
        unsafe {
            self.device.raw.destroy_query_pool(self.pool, None);
        }
    }
}
