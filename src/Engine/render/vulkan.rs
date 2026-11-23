//! Native Windows Vulkan room rendering. The host supplies an NT texture handle
//! and consumes the completed image using the keyed mutex.

use crate::Engine::asset::gltf::SkinVertex;
pub use crate::Engine::startup::StartupStage;
use ash::{Entry, vk};
use std::ffi::CStr;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "vulkan_skin.rs"]
mod skin;
use skin::{DrawTimings, SkinPass};

#[cfg(test)]
#[path = "vulkan_tests.rs"]
mod tests;

const COLOR: vk::Format = vk::Format::B8G8R8A8_UNORM;
const DEPTH: vk::Format = vk::Format::D32_SFLOAT;
const SHARED: vk::ExternalMemoryHandleTypeFlags = vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE;
const MAX_INSTANCES: usize = 2048;

pub type VulkanRoomInstance = [[f32; 4]; 5];

pub struct VulkanSkinGeometry<'a> {
    pub Vertices: &'a [SkinVertex],
    pub Indices: &'a [u16],
    pub Joints: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct VulkanSkinInstance {
    pub Transform: VulkanRoomInstance,
    pub PaletteBase: u32,
    pub Mesh: u32,
}

type Result<T> = std::result::Result<T, String>;

fn Check<T>(operation: &str, result: ash::prelude::VkResult<T>) -> Result<T> {
    result.map_err(|error| format!("{operation}: {error:?}"))
}

struct Instance {
    _entry: Entry,
    raw: ash::Instance,
}

impl Drop for Instance {
    fn drop(&mut self) {
        // The device owns a reference to this instance and is destroyed first.
        unsafe { self.raw.destroy_instance(None) };
    }
}

struct Device {
    instance: Instance,
    raw: ash::Device,
    memory: vk::PhysicalDeviceMemoryProperties,
    queue: vk::Queue,
    family: u32,
    max_size: vk::Extent2D,
    max_storage: usize,
    timestamp_bits: u32,
    timestamp_period: f32,
    name: String,
}

impl Device {
    fn New(luid: [u8; 8]) -> Result<Arc<Self>> {
        // No surface or swapchain extensions are requested. ash loads the installed loader.
        unsafe {
            let entry = StartupStage::Call("loader", || Entry::load())
                .map_err(|error| format!("Load Vulkan: {error}"))?;
            let info = vk::ApplicationInfo::default()
                .application_name(c"WyrmCoil")
                .api_version(vk::API_VERSION_1_3);
            let raw = Check(
                "Create Vulkan instance",
                StartupStage::Call("instance", || {
                    entry.create_instance(
                        &vk::InstanceCreateInfo::default().application_info(&info),
                        None,
                    )
                }),
            )?;
            let instance = Instance { _entry: entry, raw };
            let raw = &instance.raw;
            let adapters = Check(
                "Enumerate Vulkan adapters",
                StartupStage::Call("enumerate_adapters", || raw.enumerate_physical_devices()),
            )?;
            let matching = StartupStage::Begin("match_adapter_luid");
            let mut selected = None;
            for physical in adapters {
                let mut identity = vk::PhysicalDeviceIDProperties::default();
                let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut identity);
                raw.get_physical_device_properties2(physical, &mut props);
                let props = props.properties;
                if identity.device_luid_valid != 0 && identity.device_luid == luid {
                    selected = Some((physical, props));
                    break;
                }
            }
            let (physical, props) = selected
                .ok_or_else(|| format!("No Vulkan adapter matches DXGI LUID {luid:02x?}"))?;
            drop(matching);
            let capabilities = StartupStage::Begin("physical_capabilities");
            let name = CStr::from_ptr(props.device_name.as_ptr())
                .to_string_lossy()
                .into_owned();
            if props.api_version < vk::API_VERSION_1_3 {
                return Err(format!("{name} requires Vulkan 1.3 for dynamic rendering"));
            }
            let extensions = [
                ash::khr::external_memory_win32::NAME,
                ash::khr::win32_keyed_mutex::NAME,
            ];
            let supported = Check(
                "Enumerate Vulkan device extensions",
                raw.enumerate_device_extension_properties(physical),
            )?;
            for extension in extensions {
                if !supported
                    .iter()
                    .any(|value| CStr::from_ptr(value.extension_name.as_ptr()) == extension)
                {
                    return Err(format!("{name} lacks {}", extension.to_string_lossy()));
                }
            }
            let mut dynamic = vk::PhysicalDeviceDynamicRenderingFeatures::default();
            raw.get_physical_device_features2(
                physical,
                &mut vk::PhysicalDeviceFeatures2::default().push_next(&mut dynamic),
            );
            if dynamic.dynamic_rendering == 0 {
                return Err(format!("{name} does not support dynamic rendering"));
            }
            let families = raw.get_physical_device_queue_family_properties(physical);
            let family = families
                .iter()
                .position(|family| {
                    family.queue_count > 0
                        && family
                            .queue_flags
                            .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                })
                .ok_or("No Vulkan graphics/compute queue family")? as u32;
            let mut external =
                vk::PhysicalDeviceExternalImageFormatInfo::default().handle_type(SHARED);
            let query = vk::PhysicalDeviceImageFormatInfo2::default()
                .format(COLOR)
                .ty(vk::ImageType::TYPE_2D)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .push_next(&mut external);
            let mut sharing = vk::ExternalImageFormatProperties::default();
            let mut format = vk::ImageFormatProperties2::default().push_next(&mut sharing);
            Check(
                "Query BGRA8 D3D11 shared image compatibility",
                raw.get_physical_device_image_format_properties2(physical, &query, &mut format),
            )?;
            let limits = format.image_format_properties;
            if !sharing
                .external_memory_properties
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
                || !sharing
                    .external_memory_properties
                    .compatible_handle_types
                    .contains(SHARED)
                || !limits.sample_counts.contains(vk::SampleCountFlags::TYPE_1)
            {
                return Err(format!(
                    "{name} cannot import a single-sample BGRA8 D3D11 texture"
                ));
            }
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&[1.0])];
            let names = extensions.map(CStr::as_ptr);
            let info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queues)
                .enabled_extension_names(&names)
                .push_next(&mut dynamic);
            let memory = raw.get_physical_device_memory_properties(physical);
            let caps = props.limits;
            drop(capabilities);
            if StartupStage::Enabled() {
                eprintln!(
                    "wyrmcoil_vk adapter={name:?} luid={luid:02x?} api={} driver={:#x}",
                    props.api_version, props.driver_version
                );
            }
            let device = Check(
                "Create Vulkan graphics device",
                StartupStage::Call("logical_device", || {
                    raw.create_device(physical, &info, None)
                }),
            )?;
            let queue = StartupStage::Call("graphics_compute_queue", || {
                device.get_device_queue(family, 0)
            });
            let max_size = vk::Extent2D {
                width: limits.max_extent.width,
                height: limits.max_extent.height,
            };
            Ok(Arc::new(Self {
                instance,
                raw: device,
                memory,
                queue,
                family,
                max_size,
                max_storage: caps.max_storage_buffer_range as usize,
                timestamp_bits: families[family as usize].timestamp_valid_bits,
                timestamp_period: caps.timestamp_period,
                name,
            }))
        }
    }

    fn MemoryType(&self, mask: u32, flags: vk::MemoryPropertyFlags) -> Result<u32> {
        for i in 0..self.memory.memory_type_count {
            if mask & (1 << i) != 0
                && self.memory.memory_types[i as usize]
                    .property_flags
                    .contains(flags)
            {
                return Ok(i);
            }
        }
        Err(format!(
            "No Vulkan memory type for mask {mask:#x} and flags {flags:?}"
        ))
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // All renderer objects and command submissions have retired by this point.
        unsafe { self.raw.destroy_device(None) };
    }
}

struct Buffer {
    device: Arc<Device>,
    raw: vk::Buffer,
    memory: vk::DeviceMemory,
    size: usize,
}

impl Buffer {
    fn New(device: &Arc<Device>, size: usize, usage: vk::BufferUsageFlags) -> Result<Self> {
        let mut buffer = Self {
            device: device.clone(),
            raw: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            size,
        };
        // The local guard owns each handle as soon as it is acquired.
        unsafe {
            let raw = &device.raw;
            let info = vk::BufferCreateInfo::default()
                .size(size as u64)
                .usage(usage);
            buffer.raw = Check("Create Vulkan buffer", raw.create_buffer(&info, None))?;
            let req = raw.get_buffer_memory_requirements(buffer.raw);
            let index = device.MemoryType(
                req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let info = vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(index);
            buffer.memory = Check("Allocate Vulkan buffer", raw.allocate_memory(&info, None))?;
            Check(
                "Bind Vulkan buffer",
                raw.bind_buffer_memory(buffer.raw, buffer.memory, 0),
            )?;
        }
        Ok(buffer)
    }

    fn Upload<T: Copy>(&self, data: &[T]) -> Result<()> {
        self.UploadAt(0, data)
    }

    fn UploadAt<T: Copy>(&self, offset: usize, data: &[T]) -> Result<()> {
        let length = std::mem::size_of_val(data);
        if length == 0 || offset.checked_add(length).is_none_or(|end| end > self.size) {
            return Err(format!(
                "Vulkan upload is {length} bytes, capacity {}",
                self.size
            ));
        }
        // Only initialized arrays of floats or indices reach this buffer. The caller
        // checks the last submit's fence before writing the coherent instance range.
        unsafe {
            let ptr = Check(
                "Map Vulkan upload",
                self.device.raw.map_memory(
                    self.memory,
                    0,
                    (offset + length) as u64,
                    vk::MemoryMapFlags::empty(),
                ),
            )?;
            std::ptr::copy_nonoverlapping(
                data.as_ptr().cast::<u8>(),
                ptr.cast::<u8>().add(offset),
                length,
            );
            self.device.raw.unmap_memory(self.memory);
        }
        Ok(())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.device.raw.destroy_buffer(self.raw, None);
            self.device.raw.free_memory(self.memory, None);
        }
    }
}

struct Image {
    device: Arc<Device>,
    raw: vk::Image,
    view: vk::ImageView,
    memory: vk::DeviceMemory,
    aspect: vk::ImageAspectFlags,
}

impl Image {
    fn New(
        device: &Arc<Device>,
        size: vk::Extent2D,
        external_handle: Option<usize>,
    ) -> Result<Self> {
        let color = external_handle.is_some();
        let format = if color { COLOR } else { DEPTH };
        let aspect = if color {
            vk::ImageAspectFlags::COLOR
        } else {
            vk::ImageAspectFlags::DEPTH
        };
        let usage = if color {
            vk::ImageUsageFlags::COLOR_ATTACHMENT
        } else {
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
        };
        Self::Allocate(device, size, format, aspect, usage, external_handle)
    }

    fn Allocate(
        device: &Arc<Device>,
        size: vk::Extent2D,
        format: vk::Format,
        aspect: vk::ImageAspectFlags,
        usage: vk::ImageUsageFlags,
        external_handle: Option<usize>,
    ) -> Result<Self> {
        let mut image = Self {
            device: device.clone(),
            raw: vk::Image::null(),
            view: vk::ImageView::null(),
            memory: vk::DeviceMemory::null(),
            aspect,
        };
        let mut external = vk::ExternalMemoryImageCreateInfo::default().handle_types(SHARED);
        let mut info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: size.width,
                height: size.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage);
        if external_handle.is_some() {
            info = info.push_next(&mut external);
        }

        // The descriptor matches the D3D11 texture. The caller owns the NT handle,
        // which the Vulkan import retains independently of the handle reference.
        unsafe {
            let raw = &device.raw;
            image.raw = Check(
                "Create Vulkan image",
                StartupStage::Call("image_create", || raw.create_image(&info, None)),
            )?;
            let req = raw.get_image_memory_requirements(image.raw);
            let mut mask = req.memory_type_bits;
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image.raw);
            let mut import = vk::ImportMemoryWin32HandleInfoKHR::default();
            let mut alloc = vk::MemoryAllocateInfo::default().allocation_size(req.size);
            let mut properties = vk::MemoryPropertyFlags::DEVICE_LOCAL;
            if let Some(handle) = external_handle {
                let memory =
                    ash::khr::external_memory_win32::Device::new(&device.instance.raw, raw);
                let mut compatible = vk::MemoryWin32HandlePropertiesKHR::default();
                Check(
                    "Query D3D11 shared handle memory types",
                    StartupStage::Call("external_handle_memory_types", || {
                        memory.get_memory_win32_handle_properties(
                            SHARED,
                            handle as _,
                            &mut compatible,
                        )
                    }),
                )?;
                mask &= compatible.memory_type_bits;
                import = import.handle_type(SHARED).handle(handle as _);
                alloc = alloc.push_next(&mut dedicated).push_next(&mut import);
                properties = vk::MemoryPropertyFlags::empty();
            }
            alloc = alloc.memory_type_index(device.MemoryType(mask, properties)?);
            let stage = if external_handle.is_some() {
                "external_memory_import"
            } else {
                "image_memory"
            };
            image.memory = Check(
                "Allocate Vulkan image memory",
                StartupStage::Call(stage, || raw.allocate_memory(&alloc, None)),
            )?;
            Check(
                "Bind Vulkan image memory",
                StartupStage::Call("image_bind", || {
                    raw.bind_image_memory(image.raw, image.memory, 0)
                }),
            )?;
            let view = vk::ImageViewCreateInfo::default()
                .image(image.raw)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(image.Range());
            image.view = Check(
                "Create Vulkan image view",
                StartupStage::Call("image_view", || raw.create_image_view(&view, None)),
            )?;
        }
        Ok(image)
    }

    fn Range(&self) -> vk::ImageSubresourceRange {
        vk::ImageSubresourceRange::default()
            .aspect_mask(self.aspect)
            .level_count(1)
            .layer_count(1)
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.device.raw.destroy_image_view(self.view, None);
            self.device.raw.destroy_image(self.raw, None);
            self.device.raw.free_memory(self.memory, None);
        }
    }
}

struct Targets {
    color: Image,
    depth: Image,
    size: vk::Extent2D,
    submitted: bool,
}

/// Owns one room pipeline and one in-flight submission. The host serializes
/// access on its render thread.
pub struct VulkanRoom {
    device: Arc<Device>,
    targets: Option<Targets>,
    vertices: Buffer,
    indices: Buffer,
    instances: Buffer,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    in_flight: bool,
    first_submit_done: bool,
    index_count: u32,
    floor_indices: u32,
    mesh_upload_time: Duration,
    skin: Option<SkinPass>,
    timing: Option<DrawTimings>,
}

impl VulkanRoom {
    pub fn New(
        luid: [u8; 8],
        mesh: &[[f32; 4]],
        indices: &[u16],
        floor_indices: u32,
        vertex: (&[u8], &CStr),
        pixel: (&[u8], &CStr),
    ) -> Result<Self> {
        let index_count = u32::try_from(indices.len()).map_err(|error| error.to_string())?;
        if mesh.is_empty()
            || indices.is_empty()
            || !index_count.is_multiple_of(3)
            || floor_indices > index_count
            || !floor_indices.is_multiple_of(3)
            || indices
                .iter()
                .any(|&index| usize::from(index) >= mesh.len())
        {
            return Err("Invalid Vulkan room mesh".into());
        }
        let device = Device::New(luid)?;
        let buffers = StartupStage::Begin("room_buffers_and_upload");
        let upload_started = Instant::now();
        let vertices = Buffer::New(
            &device,
            std::mem::size_of_val(mesh),
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        vertices.Upload(mesh)?;
        let index_buffer = Buffer::New(
            &device,
            std::mem::size_of_val(indices),
            vk::BufferUsageFlags::INDEX_BUFFER,
        )?;
        index_buffer.Upload(indices)?;
        let mesh_upload_time = upload_started.elapsed();
        let instances = Buffer::New(
            &device,
            MAX_INSTANCES * size_of::<VulkanRoomInstance>(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        drop(buffers);
        let mut room = Self {
            device,
            targets: None,
            vertices,
            indices: index_buffer,
            instances,
            layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            in_flight: false,
            first_submit_done: false,
            index_count,
            floor_indices,
            mesh_upload_time,
            skin: None,
            timing: None,
        };
        // All handles are owned immediately and partial construction is droppable.
        unsafe {
            let raw = &room.device.raw;
            let info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(room.device.family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            room.pool = Check(
                "Create room command pool",
                StartupStage::Call("command_pool", || raw.create_command_pool(&info, None)),
            )?;
            room.command = Check(
                "Allocate room commands",
                StartupStage::Call("command_buffer", || {
                    raw.allocate_command_buffers(
                        &vk::CommandBufferAllocateInfo::default()
                            .command_pool(room.pool)
                            .command_buffer_count(1),
                    )
                }),
            )?[0];
            room.fence = Check(
                "Create room fence",
                StartupStage::Call("submission_fence", || {
                    raw.create_fence(&vk::FenceCreateInfo::default(), None)
                }),
            )?;
            room.layout = Check(
                "Create room pipeline layout",
                raw.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default(), None),
            )?;
            room.pipeline = CreatePipeline(raw, room.layout, vertex, pixel, false, COLOR)?;
        }
        Ok(room)
    }

    pub fn DeviceName(&self) -> &str {
        &self.device.name
    }

    pub fn MeshUploadTime(&self) -> Duration {
        self.mesh_upload_time
    }

    pub fn SkinUploadStats(&self) -> (u64, u64, Duration) {
        self.skin.as_ref().map_or((0, 0, Duration::ZERO), |skin| {
            (skin.uploads, skin.uploaded_bytes, skin.upload_time)
        })
    }

    pub fn EnableDrawTimings(&mut self) -> Result<()> {
        if self.timing.is_none() && !self.in_flight {
            self.timing = DrawTimings::New(&self.device)?;
        }
        Ok(())
    }

    /// Completed observations only. Retirement may still have a frame outstanding.
    pub fn DrawTimingStats(&self) -> Option<(u64, Duration, u64, Duration)> {
        self.timing.as_ref().map(|timing| timing.Stats())
    }

    /// Upload the immutable bind meshes before the viewport's first frame.
    pub fn ConfigureSkin(
        &mut self,
        meshes: &[VulkanSkinGeometry<'_>],
        slots: usize,
        capacity: usize,
        vertex: (&[u8], &CStr),
        pixel: (&[u8], &CStr),
    ) -> Result<()> {
        if self.skin.is_some() || self.in_flight {
            return Err("Vulkan skin must be configured once before submitting frames".into());
        }
        self.skin = Some(SkinPass::New(
            &self.device,
            meshes,
            slots,
            capacity,
            vertex,
            pixel,
            COLOR,
        )?);
        Ok(())
    }

    /// The host retains a valid single-mip, single-sample BGRA8 texture and
    /// holds the NT handle open for the duration of this call.
    ///
    /// # Safety
    /// The dimensions and sharing flags must match the D3D11 texture. The host
    /// must use key 1 for every read and release key 0 before asking for a submit.
    pub unsafe fn Attach(&mut self, handle: usize, width: u32, height: u32) -> Result<()> {
        if width == 0
            || height == 0
            || width > self.device.max_size.width
            || height > self.device.max_size.height
        {
            return Err(format!("Unsupported Vulkan viewport size {width}x{height}"));
        }
        if !self.IsReady()? {
            return Err("Vulkan viewport still owns the previous texture".into());
        }
        let size = vk::Extent2D { width, height };
        let color = StartupStage::Call("attach_shared_color", || {
            Image::New(&self.device, size, Some(handle))
        })?;
        let depth = StartupStage::Call("depth_resource", || Image::New(&self.device, size, None))?;
        self.targets = Some(Targets {
            color,
            depth,
            size,
            submitted: false,
        });
        Ok(())
    }

    pub fn IsReady(&self) -> Result<bool> {
        if !self.in_flight {
            return Ok(true);
        }
        // A fence status query never waits for the graphics queue.
        Check("Query Vulkan room fence", unsafe {
            self.device.raw.get_fence_status(self.fence)
        })
    }

    /// # Safety
    /// The host must have released key 0 and must not use the image until it
    /// acquires key 1. No other thread may submit work to this private queue.
    pub unsafe fn Submit(&mut self, instances: &[VulkanRoomInstance]) -> Result<bool> {
        // The caller owns the same keyed-mutex handoff as SubmitRanges.
        unsafe { self.SubmitRanges(instances, &[]) }
    }

    /// # Safety
    /// The host must follow the same keyed-mutex handoff as Submit. Each
    /// range must reference vertices that exist in the uploaded mesh.
    pub unsafe fn SubmitRanges(
        &mut self,
        instances: &[VulkanRoomInstance],
        ranges: &[[u32; 3]],
    ) -> Result<bool> {
        unsafe { self.SubmitFrame(instances, ranges, None) }
    }

    /// # Safety
    /// The keyed-mutex and range contracts of SubmitRanges apply. The skin
    /// uses the specified instance transform and the configured bind mesh.
    pub unsafe fn SubmitSkin(
        &mut self,
        instances: &[VulkanRoomInstance],
        ranges: &[[u32; 3]],
        skinned: &[VulkanSkinInstance],
        palette: &[[[f32; 4]; 4]],
    ) -> Result<bool> {
        unsafe { self.SubmitFrame(instances, ranges, Some((skinned, palette))) }
    }

    unsafe fn SubmitFrame(
        &mut self,
        instances: &[VulkanRoomInstance],
        ranges: &[[u32; 3]],
        skin: Option<(&[VulkanSkinInstance], &[[[f32; 4]; 4]])>,
    ) -> Result<bool> {
        if instances.is_empty() && skin.is_none() {
            return Ok(false);
        }
        if !ranges.is_empty()
            && (ranges.len() != instances.len()
                || ranges.iter().any(|&[start, count, base]| {
                    count == 0
                        || !count.is_multiple_of(3)
                        || base > i32::MAX as u32
                        || !start
                            .checked_add(count)
                            .is_some_and(|end| end <= self.index_count)
                }))
        {
            return Err("Invalid Vulkan mesh draw ranges".into());
        }
        if !self.IsReady()? {
            return Ok(false);
        }
        if let Some(timing) = &mut self.timing
            && self.in_flight
        {
            timing.Collect();
        }
        let target = self
            .targets
            .as_ref()
            .ok_or("Vulkan room target is missing")?;
        if let Some((skinned, palette)) = skin {
            self.skin
                .as_mut()
                .ok_or("Vulkan skin pipeline is missing")?
                .Prepare(skinned, palette)?;
        }
        if !instances.is_empty() {
            self.instances.Upload(instances)?;
        }
        let device = &self.device;
        let command = self.command;
        // Fence completion permits rewriting both the instance buffer and command buffer.
        unsafe {
            Check(
                "Reset Vulkan room commands",
                device
                    .raw
                    .reset_command_buffer(command, vk::CommandBufferResetFlags::empty()),
            )?;
            Check(
                "Begin Vulkan room commands",
                device.raw.begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                ),
            )?;
            RecordRoom(
                self,
                target,
                instances.len() as u32,
                ranges,
                skin.map_or(&[], |(draws, _)| draws),
            );
            Check(
                "Finish Vulkan room commands",
                device.raw.end_command_buffer(command),
            )?;
            Check(
                "Reset Vulkan room fence",
                device.raw.reset_fences(&[self.fence]),
            )?;
            self.in_flight = false;
            let memory = [target.color.memory];
            let mut keyed = vk::Win32KeyedMutexAcquireReleaseInfoKHR::default()
                .acquire_syncs(&memory)
                .acquire_keys(&[0])
                .acquire_timeouts(&[0])
                .release_syncs(&memory)
                .release_keys(&[1]);
            let commands = [command];
            let submit = vk::SubmitInfo::default()
                .command_buffers(&commands)
                .push_next(&mut keyed);
            let first_submit =
                (!self.first_submit_done).then(|| StartupStage::Begin("first_queue_submit"));
            Check(
                "Submit Vulkan room with keyed mutex",
                device.raw.queue_submit(device.queue, &[submit], self.fence),
            )?;
            drop(first_submit);
        }
        self.first_submit_done = true;
        self.targets.as_mut().unwrap().submitted = true;
        self.in_flight = true;
        if let Some(timing) = &mut self.timing {
            timing.Recorded(skin.is_some());
        }
        Ok(true)
    }
}

impl Drop for VulkanRoom {
    fn drop(&mut self) {
        // This is retirement only. No queue/device idle waits occur in the render loop.
        unsafe {
            let device = &self.device.raw;
            if self.in_flight {
                let _ = device.wait_for_fences(&[self.fence], true, u64::MAX);
            }
            device.destroy_fence(self.fence, None);
            device.destroy_command_pool(self.pool, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

fn CreatePipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vertex: (&[u8], &CStr),
    pixel: (&[u8], &CStr),
    skinned: bool,
    color: vk::Format,
) -> Result<vk::Pipeline> {
    let words = |bytes: &[u8]| {
        ash::util::read_spv(&mut Cursor::new(bytes)).map_err(|error| error.to_string())
    };
    let vs = words(vertex.0)?;
    let ps = words(pixel.0)?;
    // Both stages have validated SPIR-V and matching vertex/fragment interfaces.
    unsafe {
        let modules = StartupStage::Begin("shader_modules");
        let vertex_module = Check(
            "Create Vulkan room vertex shader",
            device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&vs), None),
        )?;
        let pixel_module = match device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&ps), None)
        {
            Ok(module) => module,
            Err(error) => {
                device.destroy_shader_module(vertex_module, None);
                return Err(format!("Create Vulkan room pixel shader: {error:?}"));
            }
        };
        drop(modules);
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(vertex.1),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(pixel_module)
                .name(pixel.1),
        ];
        let bindings = [
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: if skinned {
                    size_of::<SkinVertex>() as u32
                } else {
                    16
                },
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: if skinned {
                    size_of::<VulkanSkinInstance>() as u32
                } else {
                    80
                },
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ];
        let standard: [_; 6] =
            std::array::from_fn(|location| vk::VertexInputAttributeDescription {
                location: location as u32,
                binding: u32::from(location > 0),
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: location.saturating_sub(1) as u32 * 16,
            });
        let mut attributes = standard.to_vec();
        if skinned {
            attributes.push(vk::VertexInputAttributeDescription {
                location: 6,
                binding: 0,
                format: vk::Format::R32G32B32A32_UINT,
                offset: 16,
            });
            attributes.push(vk::VertexInputAttributeDescription {
                location: 7,
                binding: 0,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: 32,
            });
            attributes.push(vk::VertexInputAttributeDescription {
                location: 8,
                binding: 1,
                format: vk::Format::R32_UINT,
                offset: std::mem::offset_of!(VulkanSkinInstance, PaletteBase) as u32,
            });
        }
        let input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes);
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .line_width(1.0)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE);
        let samples = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::GREATER);
        let attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
        let dynamic = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        let formats = [color];
        let mut rendering = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&formats)
            .depth_attachment_format(DEPTH);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&input)
            .input_assembly_state(&assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&samples)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .push_next(&mut rendering);
        let stage = if skinned {
            "skin_graphics_pipeline"
        } else {
            "room_graphics_pipeline"
        };
        let pipeline = StartupStage::Call(stage, || {
            device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
        });
        device.destroy_shader_module(pixel_module, None);
        device.destroy_shader_module(vertex_module, None);
        match pipeline {
            Ok(pipelines) => Ok(pipelines[0]),
            Err((pipelines, error)) => {
                for pipeline in pipelines {
                    device.destroy_pipeline(pipeline, None);
                }
                Err(format!("Create Vulkan room pipeline: {error:?}"))
            }
        }
    }
}

fn RecordRoom(
    room: &VulkanRoom,
    target: &Targets,
    count: u32,
    ranges: &[[u32; 3]],
    skinned: &[VulkanSkinInstance],
) {
    let device = &room.device.raw;
    let command = room.command;
    let family = room.device.family;
    let area = vk::Rect2D::default().extent(target.size);
    let before = [
        vk::ImageMemoryBarrier::default()
            .image(target.color.raw)
            .subresource_range(target.color.Range())
            .old_layout(if target.submitted {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            })
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
            .dst_queue_family_index(family)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE),
        vk::ImageMemoryBarrier::default()
            .image(target.depth.raw)
            .subresource_range(target.depth.Range())
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE),
    ];
    let output = [vk::RenderingAttachmentInfo::default()
        .image_view(target.color.view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.08, 0.10, 0.14, 1.0],
            },
        })];
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(target.depth.view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 0.0,
                stencil: 0,
            },
        });
    let render = vk::RenderingInfo::default()
        .render_area(area)
        .layer_count(1)
        .color_attachments(&output)
        .depth_attachment(&depth);
    let viewport = vk::Viewport::default()
        .y(target.size.height as f32)
        .width(target.size.width as f32)
        .height(-(target.size.height as f32))
        .max_depth(1.0);
    let after = [vk::ImageMemoryBarrier::default()
        .image(target.color.raw)
        .subresource_range(target.color.Range())
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(family)
        .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];

    // Called with an active command buffer and views from the same device. The
    // submission acquires the keyed import before the first command runs.
    unsafe {
        if let Some(timing) = &room.timing {
            timing.Begin(command);
        }
        device.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &before,
        );
        device.cmd_begin_rendering(command, &render);
        device.cmd_set_viewport(command, 0, &[viewport]);
        device.cmd_set_scissor(command, 0, &[area]);
        device.cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, room.pipeline);
        device.cmd_bind_vertex_buffers(
            command,
            0,
            &[room.vertices.raw, room.instances.raw],
            &[0, 0],
        );
        device.cmd_bind_index_buffer(command, room.indices.raw, 0, vk::IndexType::UINT16);
        if !ranges.is_empty() {
            let mut first = 0;
            while first < ranges.len() {
                let [start, count, base] = ranges[first];
                let mut end = first + 1;
                while end < ranges.len() && ranges[end] == ranges[first] {
                    end += 1;
                }
                device.cmd_draw_indexed(
                    command,
                    count,
                    (end - first) as u32,
                    start,
                    base as i32,
                    first as u32,
                );
                first = end;
            }
        } else if count > 0 {
            if room.floor_indices > 0 {
                device.cmd_draw_indexed(command, room.floor_indices, 1, 0, 0, 0);
                if count > 1 {
                    device.cmd_draw_indexed(command, room.index_count, count - 1, 0, 0, 1);
                }
            } else {
                device.cmd_draw_indexed(command, room.index_count, count, 0, 0, 0);
            }
        }
        if !skinned.is_empty() {
            if let Some(timing) = &room.timing {
                timing.Mark(command, 2);
            }
            room.skin.as_ref().unwrap().Draw(command, skinned);
            if let Some(timing) = &room.timing {
                timing.Mark(command, 3);
            }
        }
        device.cmd_end_rendering(command);
        if let Some(timing) = &room.timing {
            timing.Mark(command, 1);
        }
        device.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &after,
        );
    }
}
