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
    BuildWgpuSurfaceCapabilitiesInfo, BuildWgpuSurfaceConfigPlan, BuildWgpuSurfaceConfiguration,
    SurfaceSize, WgpuSurfaceConfigPreferences,
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
        winit::window::Window::default_attributes().with_title("WyrmCoil M41 Window Loop Skeleton");
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
                let _snapshot = engine.RenderSnapshot();

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
                    Err(wgpu::SurfaceError::Timeout) => {
                        return;
                    }
                    Err(wgpu::SurfaceError::Other) => {
                        return;
                    }
                };

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("M41.WindowLoopSkeleton.ClearOnly"),
                });

                {
                    let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("M41.WindowLoopSkeleton.ClearPass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.03,
                                    g: 0.03,
                                    b: 0.06,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                }

                queue.submit([encoder.finish()]);
                frame.present();
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}
