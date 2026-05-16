#![allow(non_snake_case)]

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use wyrmcoil::Engine::Engine;
use wyrmcoil::Engine::backend::winit::QueueWinitPhysicalKey;
use wyrmcoil::Engine::render::{
    BuildRenderCommandPlan, BuildRenderPipelineLayoutPlan, BuildVertexBufferUploadPlan,
    BuildVisiblePrimitiveDemoBatch, BuildWgpuSurfaceCapabilitiesInfo, BuildWgpuSurfaceConfigPlan,
    BuildWgpuSurfaceConfiguration, BuildWgslShaderModulePlan, ColorTargetDesc, ColorTargetFormat,
    CompiledPipelineDesc, CompiledShaderModuleDesc, CreateWgpuRenderPipelineFromWgsl,
    CreateWgpuVertexBuffer, DepthFormat, DepthStencilDesc, MINIMAL_SPRITE_WGSL_FIXTURE,
    RecordWgpuDrawCommand, RenderPipelineLayoutOptions, RenderTargetLoadMode, SurfaceSize,
    UploadExecutionMode, UploadExecutionReason, UploadExecutionResult, WgpuDrawOptions,
    WgpuSurfaceConfigPreferences,
};

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

fn main() {
    let mut engine = Engine::New();

    let event_loop = EventLoop::new().expect("event loop should create");
    let window_attributes =
        winit::window::Window::default_attributes().with_title("WyrmCoil M42 Visible Primitive");
    let window = event_loop
        .create_window(window_attributes)
        .expect("window should create");

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let surface = instance
        .create_surface(&window)
        .expect("surface should create");

    let adapter = BlockOnReady(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..wgpu::RequestAdapterOptions::default()
    }))
    .expect("adapter should be available for window surface");

    let (device, queue) =
        BlockOnReady(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .expect("device/queue should create for surface adapter");

    let shader_plan = BuildWgslShaderModulePlan(
        "M42.Sprite.WGSL",
        "M42.sprite.wgsl",
        MINIMAL_SPRITE_WGSL_FIXTURE,
    )
    .expect("WGSL fixture plan should build");
    let pipeline_layout = BuildRenderPipelineLayoutPlan(
        CompiledPipelineDesc {
            Name: "M42.SpritePipeline".to_string(),
            SourceName: "M42.sprite.wgsl".to_string(),
            Vertex: CompiledShaderModuleDesc {
                EntryPoint: "vs_main".to_string(),
                TargetProfile: "wgsl-vs".to_string(),
                SpirvBytes: vec![1],
            },
            Pixel: CompiledShaderModuleDesc {
                EntryPoint: "fs_main".to_string(),
                TargetProfile: "wgsl-fs".to_string(),
                SpirvBytes: vec![1],
            },
        },
        RenderPipelineLayoutOptions {
            Name: "M42.SpritePipeline".to_string(),
            VertexBuffers: vec![wyrmcoil::Engine::render::SpriteVertexBufferLayout()],
            ColorTarget: ColorTargetDesc {
                Format: ColorTargetFormat::Bgra8UnormSrgb,
            },
            Depth: Some(DepthStencilDesc {
                Format: DepthFormat::Depth24Plus,
                DepthWriteEnabled: false,
            }),
        },
    )
    .expect("pipeline layout should build");

    let pipeline_resource = CreateWgpuRenderPipelineFromWgsl(
        &device,
        &shader_plan,
        &pipeline_layout,
        &Default::default(),
    )
    .expect("wgpu render pipeline should create");

    let mut current_size = window.inner_size();
    let capabilities = surface.get_capabilities(&adapter);
    let info = BuildWgpuSurfaceCapabilitiesInfo(&capabilities);
    let mut configured = false;

    if current_size.width > 0 && current_size.height > 0 {
        let plan = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: current_size.width,
                Height: current_size.height,
            },
            &info,
            WgpuSurfaceConfigPreferences::default(),
        )
        .expect("surface config plan should build");
        let configuration = BuildWgpuSurfaceConfiguration(plan);
        surface.configure(&device, &configuration);
        configured = true;
    }

    let _ = event_loop.run(|event, window_target| {
        window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => window_target.exit(),
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { event, .. },
                ..
            } => {
                if event.state == ElementState::Pressed || event.state == ElementState::Released {
                    let _ = QueueWinitPhysicalKey(&mut engine, event.physical_key, event.state);
                }
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                current_size = size;
                if current_size.width == 0 || current_size.height == 0 {
                    configured = false;
                } else {
                    let plan = BuildWgpuSurfaceConfigPlan(
                        SurfaceSize {
                            Width: current_size.width,
                            Height: current_size.height,
                        },
                        &info,
                        WgpuSurfaceConfigPreferences::default(),
                    )
                    .expect("surface reconfigure plan should build for non-zero size");
                    let configuration = BuildWgpuSurfaceConfiguration(plan);
                    surface.configure(&device, &configuration);
                    configured = true;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if !configured {
                    return;
                }

                let _runtime_tick = engine.TickControl();
                engine.TickSimulation();
                let snapshot = engine.RenderSnapshot();

                let extracted = BuildVisiblePrimitiveDemoBatch(&snapshot);
                let upload_plan = BuildVertexBufferUploadPlan("M42.SpriteVB", &extracted)
                    .expect("upload plan should build");
                let vertex_buffer = CreateWgpuVertexBuffer(&device, &upload_plan)
                    .expect("upload execution should create GPU vertex buffer for non-empty batch");
                let upload_result = UploadExecutionResult {
                    Mode: UploadExecutionMode::GpuBufferCreate,
                    Reason: UploadExecutionReason::GpuDeviceAvailable,
                    RejectedModes: Vec::new(),
                    CpuRecord: None,
                    GpuResource: Some(vertex_buffer),
                };

                let command_plan = BuildRenderCommandPlan(
                    "M42.MainPass",
                    &pipeline_layout,
                    &upload_plan,
                    Some(&upload_result),
                );

                let frame = match surface.get_current_texture() {
                    Ok(value) => value,
                    Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                        if current_size.width > 0 && current_size.height > 0 {
                            let plan = BuildWgpuSurfaceConfigPlan(
                                SurfaceSize {
                                    Width: current_size.width,
                                    Height: current_size.height,
                                },
                                &info,
                                WgpuSurfaceConfigPreferences::default(),
                            )
                            .expect("surface recover reconfigure plan should build");
                            let configuration = BuildWgpuSurfaceConfiguration(plan);
                            surface.configure(&device, &configuration);
                            configured = true;
                        }
                        return;
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        window_target.exit();
                        return;
                    }
                    Err(wgpu::SurfaceError::Timeout) | Err(wgpu::SurfaceError::Other) => return,
                };

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("M42.VisiblePrimitive.Pass"),
                });

                if let Some(vertex_buffer) = upload_result.GpuResource.as_ref() {
                    let _ = RecordWgpuDrawCommand(
                        &mut encoder,
                        wyrmcoil::Engine::render::WgpuDrawResources {
                            Pipeline: &pipeline_resource,
                            VertexBuffer: vertex_buffer,
                            TargetView: &view,
                        },
                        &command_plan,
                        &WgpuDrawOptions {
                            Label: "M42.VisiblePrimitive.Draw".to_string(),
                            LoadMode: RenderTargetLoadMode::Clear(wgpu::Color {
                                r: 0.03,
                                g: 0.03,
                                b: 0.06,
                                a: 1.0,
                            }),
                        },
                    );
                }

                queue.submit([encoder.finish()]);
                frame.present();
            }
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    });
}
