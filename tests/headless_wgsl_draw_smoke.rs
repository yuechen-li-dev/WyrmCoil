#![allow(non_snake_case)]

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use wyrmcoil::Dunewyrm::DwActRequest;
use wyrmcoil::Engine::render::*;
use wyrmcoil::Engine::wyrmcoil::{EntityId, RenderItem, RenderSnapshot, Vec2};

fn BlockOnReady<F: Future>(future: F) -> F::Output {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut pinned = pin!(future);

    loop {
        match pinned.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn BuildSmokeLayoutPlan(format: ColorTargetFormat) -> RenderPipelineLayoutPlan {
    BuildRenderPipelineLayoutPlan(
        CompiledPipelineDesc {
            Name: "M39.SmokeCompiled".to_string(),
            SourceName: "m39_smoke.sdslv".to_string(),
            Vertex: CompiledShaderModuleDesc {
                EntryPoint: "FlatColor_VS".to_string(),
                TargetProfile: "vs_6_0".to_string(),
                SpirvBytes: vec![1, 2, 3],
            },
            Pixel: CompiledShaderModuleDesc {
                EntryPoint: "FlatColor_PS".to_string(),
                TargetProfile: "ps_6_0".to_string(),
                SpirvBytes: vec![4, 5, 6],
            },
        },
        RenderPipelineLayoutOptions {
            Name: "M39.SmokePipeline".to_string(),
            VertexBuffers: vec![SpriteVertexBufferLayout()],
            ColorTarget: ColorTargetDesc { Format: format },
            Depth: None,
        },
    )
    .expect("M39 smoke layout plan should build")
}

#[test]
fn HeadlessWgslDrawSmokeCpuAssemblyPlanRemainsGpuFree() {
    let format = ColorTargetFormat::Rgba8UnormSrgb;
    let target = BuildHeadlessRenderTargetDesc("M39.CpuTarget", 16, 16, format)
        .expect("M39 CPU target descriptor should build");
    let layout = BuildSmokeLayoutPlan(format);
    let snapshot = RenderSnapshot {
        Frame: 1,
        Items: vec![RenderItem {
            Entity: EntityId(1),
            Position: Vec2 { X: 0.0, Y: 0.0 },
            SpriteId: 7,
        }],
    };
    let batch = ExtractSpriteVertices(&snapshot);
    let upload = BuildVertexBufferUploadPlan("M39.CpuUpload", &batch)
        .expect("M39 CPU upload plan should build");

    let acts = vec![DwActRequest {
        Id: LifecycleUploadIntentActId(),
    }];
    let execution_plan =
        PlanUploadExecution(&acts, &upload, true, UploadExecutionConstraints::default());
    assert_eq!(
        execution_plan.Mode,
        UploadExecutionMode::GpuBufferCreate,
        "M39 CPU assembly test should choose GPU mode metadata when has_device is true"
    );

    let execution = UploadExecutionResult {
        Mode: execution_plan.Mode,
        Reason: execution_plan.Reason,
        RejectedModes: execution_plan.RejectedModes.clone(),
        CpuRecord: None,
        GpuResource: None,
    };

    let command = BuildRenderCommandPlan("M39.CpuCommand", &layout, &upload, Some(&execution));
    let assembly =
        BuildHeadlessDrawAssemblyPlan("M39.CpuAssembly", &command, &layout, &upload, &target)
            .expect("M39 CPU assembly plan should validate without creating GPU objects");

    assert_eq!(
        assembly.VertexCount, 1,
        "one item should produce one draw vertex in CPU assembly test"
    );
    assert_eq!(
        assembly.TargetWidth, 16,
        "CPU assembly target width should match descriptor"
    );
}

#[test]
#[ignore = "M39 optional real-device headless smoke; set WYRMCOIL_RUN_WGPU_TESTS=1"]
fn HeadlessWgslDrawSmokeSubmitsOffscreenDrawWhenEnabled() {
    if std::env::var("WYRMCOIL_RUN_WGPU_TESTS").ok().as_deref() != Some("1") {
        eprintln!(
            "M39 smoke probe skipped: set WYRMCOIL_RUN_WGPU_TESTS=1 to enable real-device run"
        );
        return;
    }

    let instance = ::wgpu::Instance::new(&::wgpu::InstanceDescriptor::default());
    let Some(adapter) =
        BlockOnReady(instance.request_adapter(&::wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("M39 smoke probe unavailable: no adapter found for headless run");
        return;
    };

    let device_request = adapter.request_device(&::wgpu::DeviceDescriptor::default(), None);
    let Ok((device, queue)) = BlockOnReady(device_request) else {
        eprintln!("M39 smoke probe unavailable: adapter could not create device/queue");
        return;
    };

    let format = ColorTargetFormat::Rgba8UnormSrgb;
    let target_desc = BuildHeadlessRenderTargetDesc("M39.GpuTarget", 16, 16, format)
        .expect("M39 GPU target descriptor should build");
    let target = CreateWgpuHeadlessRenderTarget(&device, &target_desc)
        .expect("M39 smoke probe should create headless target");

    let layout = BuildSmokeLayoutPlan(format);
    let shader_plan = BuildWgslShaderModulePlan(
        "M39.Wgsl",
        "minimal_sprite.wgsl",
        MINIMAL_SPRITE_WGSL_FIXTURE,
    )
    .expect("M39 smoke probe should build WGSL module plan");
    let _wgsl_pipeline_plan =
        BuildWgslPipelinePlan(&shader_plan, &WgslPipelineCreateOptions::default())
            .expect("M39 smoke probe should build WGSL pipeline plan");

    let pipeline = CreateWgpuRenderPipelineFromWgsl(
        &device,
        &shader_plan,
        &layout,
        &WgslPipelineCreateOptions::default(),
    )
    .expect("M39 smoke probe should create WGSL render pipeline");

    let snapshot = RenderSnapshot {
        Frame: 2,
        Items: vec![RenderItem {
            Entity: EntityId(2),
            Position: Vec2 { X: 0.0, Y: 0.0 },
            SpriteId: 11,
        }],
    };
    let batch = ExtractSpriteVertices(&snapshot);
    let upload = BuildVertexBufferUploadPlan("M39.GpuUpload", &batch)
        .expect("M39 smoke probe should build vertex upload plan");
    let vertex = CreateWgpuVertexBuffer(&device, &upload)
        .expect("M39 smoke probe should create vertex buffer resource");

    let acts = vec![DwActRequest {
        Id: LifecycleUploadIntentActId(),
    }];
    let execution_plan =
        PlanUploadExecution(&acts, &upload, true, UploadExecutionConstraints::default());
    let execution = UploadExecutionResult {
        Mode: execution_plan.Mode,
        Reason: execution_plan.Reason,
        RejectedModes: execution_plan.RejectedModes.clone(),
        CpuRecord: None,
        GpuResource: None,
    };
    let command = BuildRenderCommandPlan("M39.GpuCommand", &layout, &upload, Some(&execution));
    let _assembly =
        BuildHeadlessDrawAssemblyPlan("M39.GpuAssembly", &command, &layout, &upload, &target_desc)
            .expect("M39 smoke probe should assemble compatible headless draw metadata");

    let submit = SubmitHeadlessDraw(
        &device,
        &queue,
        HeadlessDrawSubmissionResources {
            Pipeline: &pipeline,
            VertexBuffer: &vertex,
            Target: &target,
        },
        &command,
        &HeadlessDrawSubmissionOptions {
            Label: "M39.Submit".to_string(),
            LoadMode: RenderTargetLoadMode::Clear(::wgpu::Color::BLACK),
        },
    )
    .expect("M39 smoke probe should submit one headless draw command buffer");

    assert_eq!(
        submit.SubmittedCommandBuffers, 1,
        "M39 smoke probe should submit exactly one command buffer"
    );
    assert_eq!(
        submit.VertexCount, upload.VertexCount,
        "M39 smoke probe submission vertex count should match upload plan"
    );
}
