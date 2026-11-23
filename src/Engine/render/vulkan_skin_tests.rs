use super::*;
use crate::Engine::asset::gltf::{AssetLibrary, MeshAtlas, PaletteMesh};
use crate::Scene::SceneMatrix;
use std::path::Path;

use super::super::{Image, VulkanSkinGeometry, VulkanSkinInstance, skin::SkinPass};

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
        let cell: u32 = system.Id / 3u + uint(instance.Color.x);
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
            Color: Project(SkinPosition(input, instance.PaletteBase), instance);
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
    for (name, equipment, attack, ability) in [
        (
            "Knight",
            &[
                ("handslot.r", "sword_1handed"),
                ("handslot.l", "shield_badge"),
            ][..],
            MELEE,
            ("CombatMelee", "Melee_Block_Attack"),
        ),
        (
            "Rogue",
            &[("handslot.r", "dagger"), ("handslot.l", "dagger")][..],
            MELEE,
            ("CombatMelee", "Melee_Dualwield_Attack_Stab"),
        ),
        (
            "Mage",
            &[("handslot.r", "staff")][..],
            ("CombatRanged", "Ranged_Magic_Shoot"),
            ("CombatRanged", "Ranged_Magic_Raise"),
        ),
        (
            "Ranger",
            &[("handslot.r", "sword_1handed")][..],
            MELEE,
            MELEE,
        ),
        (
            "Barbarian",
            &[("handslot.r", "sword_1handed")][..],
            MELEE,
            MELEE,
        ),
    ] {
        CompareRig(
            &device,
            &mut library,
            name,
            equipment,
            [attack, ability],
            (&vs, &vs_entry),
            (&ps, &ps_entry),
        );
    }

    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let instance = [identity[0], identity[1], identity[2], identity[3], [0.0; 4]];
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
        const INSTANCES: usize = 12;
        let mut proof = SkinPass::New(
            &device,
            &[VulkanSkinGeometry {
                Vertices: &[vertex; 3],
                Indices: &[0, 1, 2],
                Joints: palette.len(),
            }],
            palette.len() * INSTANCES,
            INSTANCES,
            (&vs, &vs_entry),
            (&ps, &ps_entry),
            FORMAT,
        )
        .unwrap();
        let mut joint_matrices = Vec::with_capacity(palette.len() * INSTANCES);
        let draws: [_; INSTANCES] = std::array::from_fn(|actor| {
            let mut transform = instance;
            transform[4][0] = actor as f32;
            for matrix in palette {
                let mut shifted = *matrix;
                shifted[3][0] += actor as f32 * 5.0;
                joint_matrices.push(shifted);
            }
            VulkanSkinInstance {
                Transform: transform,
                PaletteBase: (actor * palette.len()) as u32,
                Mesh: 0,
            }
        });
        assert!(
            proof
                .Prepare(&[draws[0], draws[0]], &joint_matrices)
                .is_err()
        );
        assert!(
            proof
                .Prepare(&draws, &joint_matrices[..palette.len()])
                .is_err()
        );
        for count in [1, 3, 6, INSTANCES] {
            let actual = Render(&device, &mut proof, &draws[..count], &joint_matrices);
            for (actor, position) in actual[..count].iter().enumerate() {
                let mut expected = expected;
                expected[0] += actor as f32 * 5.0;
                assert_eq!(*position, expected);
            }
            assert_eq!(actual[count], [0.0; 4]);
            assert_eq!(proof.uploads, 1);
        }
        let before = proof.uploaded_bytes;
        joint_matrices[palette.len()][3][0] += 2.0;
        let actual = Render(&device, &mut proof, &draws, &joint_matrices);
        assert_eq!(actual[0], expected);
        assert!(actual[1][0] > expected[0] + 5.0);
        assert_eq!(proof.uploaded_bytes - before, 64);
    }
}

const MELEE: (&str, &str) = ("CombatMelee", "Melee_1H_Attack_Slice_Diagonal");

fn CompareRig(
    device: &std::sync::Arc<Device>,
    library: &mut AssetLibrary,
    rig: &str,
    equipment: &[(&str, &str)],
    attacks: [(&str, &str); 2],
    vertex: (&[u8], &std::ffi::CStr),
    pixel: (&[u8], &std::ffi::CStr),
) {
    let name = rig;
    let model = library
        .Load(&Path::new("characters").join(format!("{name}.glb")))
        .unwrap();
    assert!(model.Primitives.len() > 1);
    let mut mesh = PaletteMesh::New(model.clone()).unwrap();
    for &(socket, file) in equipment {
        let path = format!("equipment/{file}.gltf");
        let equipment = PaletteMesh::New(library.Load(Path::new(&path)).unwrap()).unwrap();
        mesh.Attach(socket, &equipment).unwrap();
    }
    let skin = mesh.SkinMesh().unwrap();
    assert_eq!(
        skin.PaletteSize(),
        model.Skins[0].Joints.len() + equipment.len()
    );
    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    for &vertex in &skin.Vertices {
        for _ in 0..3 {
            indices.push(u16::try_from(vertices.len()).unwrap());
            vertices.push(vertex);
        }
    }
    assert!(skin.Vertices.len() <= (SIZE.width * SIZE.height) as usize);

    let geometry = [VulkanSkinGeometry {
        Vertices: &vertices,
        Indices: &indices,
        Joints: skin.PaletteSize(),
    }];
    let build = |mesh: &[VulkanSkinGeometry<'_>], slots| {
        SkinPass::New(device, mesh, slots, 1, vertex, pixel, FORMAT)
    };
    let mut pass = build(&geometry, skin.PaletteSize() + 2).unwrap();
    assert!(build(&geometry, usize::MAX).is_err());
    let mut malformed = vertices[..3].to_vec();
    let mut bad_weights = malformed.clone();
    malformed[0].Joints[0] = skin.PaletteSize() as u32;
    bad_weights[0].Weights = [0.0; 4];
    let malformed_mesh = |vertices| {
        [VulkanSkinGeometry {
            Vertices: vertices,
            Indices: &[0, 1, 2],
            Joints: skin.PaletteSize(),
        }]
    };
    assert!(build(&malformed_mesh(&malformed), skin.PaletteSize()).is_err());
    assert!(build(&malformed_mesh(&bad_weights), skin.PaletteSize()).is_err());

    let model = SceneMatrix([
        [0.0, 0.0, 0.7, 0.0],
        [0.0, 1.3, 0.0, 0.0],
        [-0.5, 0.0, 0.0, 0.0],
        [2.0, -0.3, -1.0, 1.0],
    ]);
    let mut max_error = 0.0_f32;
    let total = Instant::now();
    let mut samples = 0;
    let mut total_error = 0.0_f64;
    let mut components = 0_u64;
    let mut compare = |clip: Option<&crate::Engine::asset::gltf::Clip>, time| {
        let mut palette = vec![SceneMatrix::IDENTITY.0; 2];
        palette.extend(skin.Palette(clip, time).unwrap());
        let uploads = pass.uploads;
        pass.Upload(&palette).unwrap();
        let after = pass.uploads;
        assert!(after <= uploads + 1);
        pass.Upload(&palette).unwrap();
        assert_eq!(after, pass.uploads);
        let [x, y, z, t] = model.0;
        let draw = VulkanSkinInstance {
            Transform: [x, y, z, t, [0.0, 1.0, 1.0, 1.0]],
            PaletteBase: 2,
            Mesh: 0,
        };
        let actual = Render(device, &mut pass, &[draw], &palette);
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
                total_error += f64::from(error);
                components += 1;
            }
        }
        samples += 1;
    };
    compare(None, 0.0);
    for (file, name) in [
        ("General", "Idle_A"),
        ("MovementBasic", "Walking_A"),
        attacks[0],
        attacks[1],
        ("General", "Hit_A"),
        ("General", "Death_A"),
    ] {
        let path = format!("animations/Rig_Medium_{file}.glb");
        let clips = library.LoadClips(Path::new(&path), &[name]).unwrap();
        let clip = &clips.Clips[name];
        let bound = skin.BindClip(clips.clone(), name).unwrap();
        for fraction in [0.0, 0.5, 1.0] {
            let time = clip.Duration * fraction;
            let mut palette = vec![SceneMatrix::IDENTITY.0; skin.PaletteSize()];
            skin.Sample(&bound, time, &mut Default::default(), &mut palette)
                .unwrap();
            assert_eq!(palette, skin.Palette(Some(clip), time).unwrap());
            compare(Some(clip), time);
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
        "{} {rig}: {samples} poses × {} vertices, max error {max_error}; mean component error {:.3e}; probe {:.2?}; {} palette uploads in {:.2?}",
        device.name,
        skin.Vertices.len(),
        total_error / components as f64,
        total.elapsed(),
        pass.uploads,
        pass.upload_time
    );
    if rig == "Knight" {
        let clips = library
            .LoadClips(
                Path::new("animations/Rig_Medium_MovementBasic.glb"),
                &["Walking_A"],
            )
            .unwrap();
        let clip = skin.BindClip(clips, "Walking_A").unwrap();
        let geometry = [VulkanSkinGeometry {
            Vertices: &skin.Vertices,
            Indices: &skin.Indices,
            Joints: skin.PaletteSize(),
        }];
        const UPDATES: u32 = 360;
        for count in [1, 3, 6, 12] {
            let slots = count * skin.PaletteSize();
            let mut batch =
                SkinPass::New(device, &geometry, slots, count, vertex, pixel, FORMAT).unwrap();
            let mut matrices = vec![SceneMatrix::IDENTITY.0; slots];
            let mut scratch = crate::Engine::asset::gltf::SkinPose::default();
            let mut evaluated = std::time::Duration::ZERO;
            for tick in 0..UPDATES {
                let start = Instant::now();
                for (actor, output) in matrices.chunks_exact_mut(skin.PaletteSize()).enumerate() {
                    let time = (tick as f32 / 60.0 + actor as f32 / 15.0) % clip.Duration();
                    skin.Sample(&clip, time, &mut scratch, output).unwrap();
                }
                evaluated += start.elapsed();
                batch.Upload(&matrices).unwrap();
            }
            assert_eq!(batch.uploads, u64::from(UPDATES));
            eprintln!(
                "Knight walk x{count}, {UPDATES} updates, 1 shared mesh, {} palette bytes, {:.2?} eval/update, {:.2?} mapped copy/update, {} transferred bytes",
                size_of_val(matrices.as_slice()),
                evaluated / UPDATES,
                batch.upload_time / UPDATES,
                batch.uploaded_bytes
            );
        }
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
    pass: &mut SkinPass,
    draws: &[VulkanSkinInstance],
    palette: &[[[f32; 4]; 4]],
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
    pass.Prepare(draws, palette).unwrap();
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
        pass.Draw(command, draws);
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
