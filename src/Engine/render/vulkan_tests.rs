use std::ffi::CString;
use std::io::Cursor;
use std::time::{Duration, Instant};

use crate::Demo::room::{NewRoomEngine, RoomCube};
use crate::Engine::shader::sdslv::{
    CompileArtifactEntryWithDxc, CompileCanonicalSourceToShaderArtifact, DxcOptions,
    SdslvShaderStage,
};
use windows::Win32::Foundation::{CloseHandle, HMODULE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::{Interface, PCWSTR};

use super::{Buffer, Device, VulkanRoom, VulkanRoomInstance, vk};

#[path = "vulkan_skin_tests.rs"]
mod skin_tests;

#[test]
#[ignore = "requires Windows D3D11/Vulkan external memory and DXC"]
fn room_renders_and_dispatches_compute_on_the_same_vulkan_device() {
    let artifact = CompileCanonicalSourceToShaderArtifact(
        "room.sdslv",
        include_str!("../../../integrations/zed/wyrmcoil_view/src/room.sdslv"),
    )
    .unwrap();
    let options = DxcOptions {
        ExtraArgs: vec!["-fspv-target-env=vulkan1.3".into()],
        ..Default::default()
    };
    let vertex = &artifact.EntryPoints[0];
    let pixel = &artifact.EntryPoints[1];
    let vs = CompileArtifactEntryWithDxc(&artifact, vertex, &options).unwrap();
    let ps = CompileArtifactEntryWithDxc(&artifact, pixel, &options).unwrap();
    let vs_name = CString::new(vertex.Name.as_str()).unwrap();
    let ps_name = CString::new(pixel.Name.as_str()).unwrap();
    let (mesh, indices) = RoomCube();

    // Test-only D3D11 device on the same first DXGI adapter selected by GPUI.
    unsafe {
        let factory = CreateDXGIFactory1::<IDXGIFactory1>().unwrap();
        let adapter = factory.EnumAdapters1(0).unwrap();
        let desc = adapter.GetDesc1().unwrap();
        let mut luid = [0; 8];
        luid[..4].copy_from_slice(&desc.AdapterLuid.LowPart.to_le_bytes());
        luid[4..].copy_from_slice(&desc.AdapterLuid.HighPart.to_le_bytes());
        let mut dx = None;
        let mut ctx = None;
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut dx),
            None,
            Some(&mut ctx),
        )
        .unwrap();
        let dx = dx.unwrap();
        let ctx = ctx.unwrap();
        let build = |luid| {
            VulkanRoom::New(
                luid,
                &mesh,
                &indices,
                6,
                (&vs.OutputBytes, &vs_name),
                (&ps.OutputBytes, &ps_name),
            )
        };
        assert!(build([0; 8]).err().unwrap().contains("matches DXGI LUID"));
        let mut room = build(luid).unwrap();
        println!("Vulkan on {} / DXGI LUID {luid:02x?}", room.DeviceName());
        assert!(room.Attach(0, 0, 0).is_err());
        let snapshot = NewRoomEngine().RenderSnapshot();
        for [width, height] in [[160, 128], [200, 144]] {
            let vp = snapshot
                .Camera
                .ViewProjection(width as f32 / height as f32)
                .unwrap();
            let objects = std::iter::once(&snapshot.Floor)
                .chain(snapshot.Boxes.iter())
                .chain(std::iter::once(&snapshot.Guard));
            let frames: Vec<VulkanRoomInstance> = objects
                .map(|object| {
                    let [x, y, z, t] = vp.Multiply(object.Model).0;
                    [x, y, z, t, object.Color]
                })
                .collect();
            let mut desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET).0 as u32,
                MiscFlags: (D3D11_RESOURCE_MISC_SHARED_NTHANDLE
                    | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX)
                    .0 as u32,
                ..Default::default()
            };
            let mut texture = None;
            dx.CreateTexture2D(&desc, None, Some(&mut texture)).unwrap();
            let texture = texture.unwrap();
            let mutex = texture.cast::<IDXGIKeyedMutex>().unwrap();
            let handle = texture
                .cast::<IDXGIResource1>()
                .unwrap()
                .CreateSharedHandle(
                    None,
                    (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0,
                    PCWSTR::null(),
                )
                .unwrap();
            let attached = room.Attach(handle.0 as usize, width, height);
            CloseHandle(handle).unwrap();
            attached.unwrap();
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.MiscFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            dx.CreateTexture2D(&desc, None, Some(&mut staging)).unwrap();
            let staging = staging.unwrap();

            let mut expected = Vec::new();
            for reverse in [false, true] {
                let mut objects = frames.clone();
                if reverse {
                    objects[1..].reverse();
                }
                let now = Instant::now();
                if reverse {
                    let mut ranges = vec![[0, indices.len() as u32, 0]; objects.len()];
                    ranges[0][1] = 6;
                    assert!(room.SubmitRanges(&objects, &ranges).unwrap());
                } else {
                    assert!(room.Submit(&objects).unwrap());
                }
                while !room.IsReady().unwrap() {
                    assert!(now.elapsed() < Duration::from_secs(2));
                    std::thread::yield_now();
                }
                // Check the complete HRESULT. WAIT_TIMEOUT is a nonnegative status.
                assert_eq!((mutex.vtable().AcquireSync)(mutex.as_raw(), 1, 0).0, 0);
                ctx.CopyResource(&staging, &texture);
                mutex.ReleaseSync(0).unwrap();
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
                let pixels: Vec<_> = (0..height as usize)
                    .flat_map(|row| {
                        std::slice::from_raw_parts(
                            mapped
                                .pData
                                .cast::<u8>()
                                .add(row * mapped.RowPitch as usize),
                            width as usize * 4,
                        )
                    })
                    .copied()
                    .collect();
                ctx.Unmap(&staging, 0);
                let colors: std::collections::BTreeSet<_> = pixels.chunks_exact(4).collect();
                eprintln!(
                    "{width}x{height}, reverse={reverse}, {} colors; center BGRA {:?}",
                    colors.len(),
                    &pixels[(height as usize / 2 * width as usize + width as usize / 2) * 4..][..4]
                );
                assert!(
                    colors.len() >= 8,
                    "the real room must rasterize its colored cubes and floor"
                );
                if reverse {
                    assert_eq!(
                        expected, pixels,
                        "depth ordering must be independent of submission"
                    );
                } else {
                    expected = pixels;
                }
                if width == 160 && !reverse {
                    RunVectorAdd(&room, &options);
                }
            }
        }
    }
}

fn RunVectorAdd(room: &VulkanRoom, options: &DxcOptions) {
    let artifact = CompileCanonicalSourceToShaderArtifact(
        "vector-add.sdslv",
        include_str!("../../../tests/fixtures/sdslv/vector-add.sdslv"),
    )
    .unwrap();
    let [entry] = artifact.EntryPoints.as_slice() else {
        panic!("expected one compute entry");
    };
    assert_eq!(entry.Stage, SdslvShaderStage::Compute);
    let spirv = CompileArtifactEntryWithDxc(&artifact, entry, options).unwrap();
    let words = ash::util::read_spv(&mut Cursor::new(&spirv.OutputBytes)).unwrap();
    let name = CString::new(entry.Name.as_str()).unwrap();
    let elements = 32usize;
    let left: Vec<f32> = (0..elements).map(|index| index as f32 * 0.5).collect();
    let right: Vec<f32> = (0..elements)
        .map(|index| 3.0 - index as f32 * 0.25)
        .collect();
    let mut proof = ComputeProof::New(room);
    let range = (elements * size_of::<f32>()) as u64;
    for _ in 0..3 {
        proof.buffers.push(
            Buffer::New(
                &room.device,
                range as usize,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
            .unwrap(),
        );
    }
    proof.buffers[0].Upload(&left).unwrap();
    proof.buffers[1].Upload(&right).unwrap();
    proof.buffers[2].Upload(&vec![f32::NAN; elements]).unwrap();
    proof.Dispatch(&words, &name, range, [4, 1, 1]);
    let expected: Vec<_> = left.iter().zip(&right).map(|(a, b)| a + b).collect();
    let output = proof.buffers[2].memory;
    // The completed submission makes shader writes visible before mapping the coherent buffer.
    unsafe {
        let device = &room.device.raw;
        let ptr = device
            .map_memory(output, 0, range, vk::MemoryMapFlags::empty())
            .unwrap();
        let actual = std::slice::from_raw_parts(ptr.cast::<f32>(), elements).to_vec();
        device.unmap_memory(output);
        assert_eq!(actual, expected);
        eprintln!(
            "VectorAdd_CS computed {elements} values across four workgroups on the room device (first {}, last {})",
            actual[0],
            actual[elements - 1],
        );
    }
}

/// Test-only descriptors, pipeline and work submitted to the live room's device.
struct ComputeProof<'a> {
    device: &'a Device,
    commands: vk::CommandPool,
    fence: vk::Fence,
    pending: bool,
    set_layout: vk::DescriptorSetLayout,
    descriptors: vk::DescriptorPool,
    layout: vk::PipelineLayout,
    shader: vk::ShaderModule,
    pipeline: vk::Pipeline,
    buffers: Vec<Buffer>,
}

impl<'a> ComputeProof<'a> {
    fn New(room: &'a VulkanRoom) -> Self {
        assert!(room.IsReady().unwrap());
        Self {
            device: &room.device,
            commands: vk::CommandPool::null(),
            fence: vk::Fence::null(),
            pending: false,
            set_layout: vk::DescriptorSetLayout::null(),
            descriptors: vk::DescriptorPool::null(),
            layout: vk::PipelineLayout::null(),
            shader: vk::ShaderModule::null(),
            pipeline: vk::Pipeline::null(),
            buffers: Vec::new(),
        }
    }

    fn Dispatch(&mut self, words: &[u32], entry: &std::ffi::CStr, range: u64, groups: [u32; 3]) {
        let device = &self.device.raw;
        // Every handle stays owned until the test fence retires the commands and storage.
        unsafe {
            let bindings = [0, 1, 2].map(|index| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(index)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            });
            self.set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .unwrap();
            let sizes = [vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 3,
            }];
            self.descriptors = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&sizes),
                    None,
                )
                .unwrap();
            let layouts = [self.set_layout];
            let set = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptors)
                        .set_layouts(&layouts),
                )
                .unwrap()[0];
            let infos = [0, 1, 2].map(|index| {
                [vk::DescriptorBufferInfo::default()
                    .buffer(self.buffers[index].raw)
                    .range(range)]
            });
            let writes = [0, 1, 2].map(|index| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(index as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&infos[index])
            });
            device.update_descriptor_sets(&writes, &[]);

            self.layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts),
                    None,
                )
                .unwrap();
            self.shader = device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(words), None)
                .unwrap();
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(self.shader)
                .name(entry);
            let info = vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(self.layout);
            self.pipeline =
                match device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None) {
                    Ok(pipelines) => pipelines[0],
                    Err((pipelines, error)) => {
                        for pipeline in pipelines {
                            device.destroy_pipeline(pipeline, None);
                        }
                        panic!("create vector-add pipeline: {error:?}");
                    }
                };
            self.commands = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default().queue_family_index(self.device.family),
                    None,
                )
                .unwrap();
            let commands = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(self.commands)
                        .command_buffer_count(1),
                )
                .unwrap();
            let command = commands[0];
            device
                .begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            let bind = vk::PipelineBindPoint::COMPUTE;
            device.cmd_bind_pipeline(command, bind, self.pipeline);
            device.cmd_bind_descriptor_sets(command, bind, self.layout, 0, &[set], &[]);
            device.cmd_dispatch(command, groups[0], groups[1], groups[2]);
            let host_read = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ);
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[host_read],
                &[],
                &[],
            );
            device.end_command_buffer(command).unwrap();
            self.fence = device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .unwrap();
            let submit = vk::SubmitInfo::default().command_buffers(&commands);
            device
                .queue_submit(self.device.queue, &[submit], self.fence)
                .unwrap();
            self.pending = true;
            device
                .wait_for_fences(&[self.fence], true, 2_000_000_000)
                .unwrap();
            self.pending = false;
        }
    }
}

impl Drop for ComputeProof<'_> {
    fn drop(&mut self) {
        // If the bounded test wait failed, retire outstanding work before destroying its resources.
        unsafe {
            let device = &self.device.raw;
            if self.pending {
                let _ = device.wait_for_fences(&[self.fence], true, u64::MAX);
            }
            device.destroy_fence(self.fence, None);
            device.destroy_command_pool(self.commands, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_shader_module(self.shader, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.descriptors, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}
