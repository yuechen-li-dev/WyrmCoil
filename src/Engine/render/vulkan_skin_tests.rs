use super::*;
use crate::Engine::asset::gltf::{AssetLibrary, MeshAtlas, PaletteMesh};
use crate::Scene::SceneMatrix;
use std::path::Path;

use super::super::{Image, skin::SkinPass};

const SOURCE: &str =
    include_str!("../../../integrations/zed/wyrmcoil_view/src/tactical_skin.sdslv");
const FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;
const SIZE: vk::Extent2D = vk::Extent2D {
    width: 128,
    height: 64,
};

#[test]
#[ignore = "requires Vulkan hardware, DXC and a matching DXGI adapter"]
fn kaykit_skin_executes_vertex_deformation_against_cpu_oracle() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/games/last-watch/assets");
    let mut library = AssetLibrary::New(&assets).unwrap();
    let model = library.Load(Path::new("characters/Knight.glb")).unwrap();
    assert!(model.Primitives.len() > 1);
    let mut mesh = PaletteMesh::New(model.clone()).unwrap();
    for (socket, file) in [
        ("handslot.r", "equipment/sword_1handed.gltf"),
        ("handslot.l", "equipment/shield_badge.gltf"),
    ] {
        let equipment = PaletteMesh::New(library.Load(Path::new(file)).unwrap()).unwrap();
        mesh.Attach(socket, &equipment).unwrap();
    }
    let skin = mesh.SkinMesh().unwrap();
    assert_eq!(skin.PaletteSize(), model.Skins[0].Joints.len() + 2);
    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    for &vertex in &skin.Vertices {
        for _ in 0..3 {
            indices.push(u16::try_from(vertices.len()).unwrap());
            vertices.push(vertex);
        }
    }
    assert!(skin.Vertices.len() <= (SIZE.width * SIZE.height) as usize);

    // Only the raster position/diagnostic output changes. The functions that
    // fetch, deform and compose the instance transform are from the live VS.
    let source = SOURCE.replace(
        "shader TacticalSkin",
        "stream ProbeSystem { [builtin(vertex_id)] Id: u32; }\nshader TacticalSkin",
    );
    let start = source.find("    stage vertex").unwrap();
    let end = source.find("    stage pixel").unwrap();
    let source = format!(
        "{}{}{}",
        &source[..start],
        r#"
    stage vertex fn VS(input: SkinVertex, instance: Instance, system: ProbeSystem) -> Surface {
        let cell: u32 = system.Id / 3u;
        let corner: u32 = system.Id % 3u;
        let x: f32 = (float(cell % 128u) + 0.5) * (2.0 / 128.0) - 1.0;
        let y: f32 = 1.0 - (float(cell / 128u) + 0.5) * (2.0 / 64.0);
        var dx: f32 = 0.0;
        var dy: f32 = 0.9 / 64.0;
        if (corner == 0u) {
            dx = -0.9 / 128.0;
            dy = -0.9 / 64.0;
        }
        if (corner == 1u) {
            dx = 0.9 / 128.0;
            dy = -0.9 / 64.0;
        }
        return Surface {
            Position: float4(x + dx, y + dy, 0.5, 1.0);
            Color: Project(SkinPosition(input), instance);
        };
    }
"#,
        &source[end..]
    );
    let artifact = CompileCanonicalSourceToShaderArtifact("skin-probe.sdslv", &source).unwrap();
    let live_artifact =
        CompileCanonicalSourceToShaderArtifact("tactical_skin.sdslv", SOURCE).unwrap();
    let options = DxcOptions {
        DxcPath: std::env::var("WYRMCOIL_DXC").unwrap_or_else(|_| "dxc".into()),
        ExtraArgs: vec!["-fspv-target-env=vulkan1.3".into()],
        ..Default::default()
    };
    let compile = |artifact: &crate::Engine::shader::sdslv::SdslvShaderArtifact| {
        [0, 1].map(|index| {
            let entry = &artifact.EntryPoints[index];
            let shader = CompileArtifactEntryWithDxc(artifact, entry, &options).unwrap();
            (
                shader.OutputBytes,
                CString::new(entry.Name.as_str()).unwrap(),
            )
        })
    };
    let [(vs, vs_entry), (ps, ps_entry)] = compile(&artifact);
    let _ = compile(&live_artifact);
    let device = Device::New(AdapterLuid()).unwrap();
    let mut pass = SkinPass::New(
        &device,
        &vertices,
        &indices,
        skin.PaletteSize(),
        (&vs, &vs_entry),
        (&ps, &ps_entry),
        FORMAT,
    )
    .unwrap();
    assert!(
        SkinPass::New(
            &device,
            &vertices,
            &indices,
            usize::MAX,
            (&vs, &vs_entry),
            (&ps, &ps_entry),
            FORMAT
        )
        .is_err()
    );
    let mut malformed = vertices[..3].to_vec();
    malformed[0].Joints[0] = skin.PaletteSize() as u32;
    assert!(
        SkinPass::New(
            &device,
            &malformed,
            &[0, 1, 2],
            skin.PaletteSize(),
            (&vs, &vs_entry),
            (&ps, &ps_entry),
            FORMAT
        )
        .is_err()
    );
    malformed[0].Joints[0] = 0;
    malformed[0].Weights = [0.0; 4];
    assert!(
        SkinPass::New(
            &device,
            &malformed,
            &[0, 1, 2],
            skin.PaletteSize(),
            (&vs, &vs_entry),
            (&ps, &ps_entry),
            FORMAT
        )
        .is_err()
    );

    let model = SceneMatrix([
        [0.0, 0.0, 0.7, 0.0],
        [0.0, 1.3, 0.0, 0.0],
        [-0.5, 0.0, 0.0, 0.0],
        [2.0, -0.3, -1.0, 1.0],
    ]);
    let mut max_error = 0.0_f32;
    let total = Instant::now();
    let mut samples = 0;
    let mut compare = |clip: Option<&crate::Engine::asset::gltf::Clip>, time| {
        let palette = skin.Palette(clip, time).unwrap();
        let uploads = pass.uploads;
        pass.Upload(&palette).unwrap();
        let after = pass.uploads;
        assert!(after <= uploads + 1);
        pass.Upload(&palette).unwrap();
        assert_eq!(after, pass.uploads);
        let [x, y, z, t] = model.0;
        let actual = Render(&device, &pass, [x, y, z, t, [1.0; 4]]);
        let mut cpu = MeshAtlas::default();
        mesh.AppendPose(&mut cpu, clip, time).unwrap();
        for (index, &[x, y, z, _]) in cpu.Vertices.iter().enumerate() {
            let expected = model.Transform([x, y, z, 1.0]);
            for axis in 0..4 {
                let error = (expected[axis] - actual[index][axis]).abs();
                assert!(
                    error <= 0.00002,
                    "vertex {index}, axis {axis}, sample {time}: {expected:?} versus {:?}",
                    actual[index]
                );
                max_error = max_error.max(error);
            }
        }
        samples += 1;
    };
    compare(None, 0.0);
    for (file, name) in [
        ("General", "Idle_A"),
        ("MovementBasic", "Walking_A"),
        ("CombatMelee", "Melee_1H_Attack_Slice_Diagonal"),
        ("CombatMelee", "Melee_Block_Attack"),
        ("General", "Hit_A"),
        ("General", "Death_A"),
    ] {
        let path = format!("animations/Rig_Medium_{file}.glb");
        let clips = library.LoadClips(Path::new(&path), &[name]).unwrap();
        let clip = &clips.Clips[name];
        for fraction in [0.0, 0.5, 1.0] {
            compare(Some(clip), clip.Duration * fraction);
        }
        for track in clip
            .Tracks
            .iter()
            .filter(|track| track.Step && track.Times.len() > 1)
            .take(1)
        {
            let boundary = track.Times[1];
            compare(Some(clip), boundary.next_down().max(0.0));
            compare(Some(clip), boundary);
        }
    }
    eprintln!(
        "{}: {samples} poses × {} vertices, max error {max_error}; probe {:.2?}; {} palette uploads in {:.2?}",
        device.name,
        skin.Vertices.len(),
        total.elapsed(),
        pass.uploads,
        pass.upload_time
    );

    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let instance = [identity[0], identity[1], identity[2], identity[3], [1.0; 4]];
    let only_joint = super::super::SkinVertex {
        PositionColor: [0.25, 0.5, -0.75, 0.0],
        Joints: [0; 4],
        Weights: [1.0, 0.0, 0.0, 0.0],
    };
    let blend = super::super::SkinVertex {
        Joints: [0, 1, 2, 3],
        Weights: [0.125, 0.25, 0.5, 0.125],
        ..only_joint
    };
    let mut matrices = [identity; 4];
    matrices[1][3][0] = 2.0;
    matrices[2][3][1] = 1.0;
    matrices[3][3][2] = -4.0;
    for (vertex, palette, expected) in [
        (only_joint, &matrices[..1], [0.25, 0.5, -0.75, 1.0]),
        (blend, &matrices[..], [0.75, 1.0, -1.25, 1.0]),
    ] {
        let mut proof = SkinPass::New(
            &device,
            &[vertex; 3],
            &[0, 1, 2],
            palette.len(),
            (&vs, &vs_entry),
            (&ps, &ps_entry),
            FORMAT,
        )
        .unwrap();
        proof.Upload(palette).unwrap();
        assert_eq!(Render(&device, &proof, instance)[0], expected);
    }
}

fn AdapterLuid() -> [u8; 8] {
    // Same DXGI enumeration used by the existing interop test.
    unsafe {
        let factory = CreateDXGIFactory1::<IDXGIFactory1>().unwrap();
        let desc = factory.EnumAdapters1(0).unwrap().GetDesc1().unwrap();
        let mut luid = [0; 8];
        luid[..4].copy_from_slice(&desc.AdapterLuid.LowPart.to_le_bytes());
        luid[4..].copy_from_slice(&desc.AdapterLuid.HighPart.to_le_bytes());
        luid
    }
}

fn Render(
    device: &std::sync::Arc<Device>,
    pass: &SkinPass,
    instance: VulkanRoomInstance,
) -> Vec<[f32; 4]> {
    let color = Image::Allocate(
        device,
        SIZE,
        FORMAT,
        vk::ImageAspectFlags::COLOR,
        vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
        None,
    )
    .unwrap();
    let depth = Image::New(device, SIZE, None).unwrap();
    let bytes = (SIZE.width * SIZE.height) as usize * size_of::<[f32; 4]>();
    let readback = Buffer::New(device, bytes, vk::BufferUsageFlags::TRANSFER_DST).unwrap();
    let instances = Buffer::New(
        device,
        size_of_val(&instance),
        vk::BufferUsageFlags::VERTEX_BUFFER,
    )
    .unwrap();
    instances.Upload(&[instance]).unwrap();
    let raw = &device.raw;
    // Test-only float target. All resources are retained until queue completion.
    unsafe {
        let pool = raw
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(device.family),
                None,
            )
            .unwrap();
        let command = raw
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .command_buffer_count(1),
            )
            .unwrap()[0];
        raw.begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())
            .unwrap();
        let color_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let depth_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        let barriers = [
            vk::ImageMemoryBarrier::default()
                .image(color.raw)
                .subresource_range(color.Range())
                .new_layout(color_layout)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED),
            vk::ImageMemoryBarrier::default()
                .image(depth.raw)
                .subresource_range(depth.Range())
                .new_layout(depth_layout)
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED),
        ];
        raw.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &barriers,
        );
        let area = vk::Rect2D::default().extent(SIZE);
        let attachment = [vk::RenderingAttachmentInfo::default()
            .image_view(color.view)
            .image_layout(color_layout)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)];
        let depth_info = vk::RenderingAttachmentInfo::default()
            .image_view(depth.view)
            .image_layout(depth_layout)
            .load_op(vk::AttachmentLoadOp::CLEAR);
        raw.cmd_begin_rendering(
            command,
            &vk::RenderingInfo::default()
                .render_area(area)
                .layer_count(1)
                .color_attachments(&attachment)
                .depth_attachment(&depth_info),
        );
        raw.cmd_set_viewport(
            command,
            0,
            &[vk::Viewport::default()
                .width(SIZE.width as f32)
                .y(SIZE.height as f32)
                .height(-(SIZE.height as f32))
                .max_depth(1.0)],
        );
        raw.cmd_set_scissor(command, 0, &[area]);
        pass.Draw(command, instances.raw, 0);
        raw.cmd_end_rendering(command);
        let layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
        let copy = vk::ImageMemoryBarrier::default()
            .image(color.raw)
            .subresource_range(color.Range())
            .old_layout(color_layout)
            .new_layout(layout)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
        raw.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[copy],
        );
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: SIZE.width,
                height: SIZE.height,
                depth: 1,
            });
        raw.cmd_copy_image_to_buffer(command, color.raw, layout, readback.raw, &[region]);
        let host = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ);
        raw.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[host],
            &[],
            &[],
        );
        raw.end_command_buffer(command).unwrap();
        raw.queue_submit(
            device.queue,
            &[vk::SubmitInfo::default().command_buffers(&[command])],
            vk::Fence::null(),
        )
        .unwrap();
        raw.queue_wait_idle(device.queue).unwrap();
        raw.destroy_command_pool(pool, None);
        let ptr = raw
            .map_memory(
                readback.memory,
                0,
                bytes as u64,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let result =
            std::slice::from_raw_parts(ptr.cast::<[f32; 4]>(), bytes / size_of::<[f32; 4]>())
                .to_vec();
        raw.unmap_memory(readback.memory);
        result
    }
}
