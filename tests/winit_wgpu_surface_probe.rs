#![allow(non_snake_case)]

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use wyrmcoil::Engine::render::*;

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

#[test]
#[ignore = "M40 optional real window+surface probe; set WYRMCOIL_RUN_WGPU_TESTS=1"]
fn WinitWgpuSurfaceCanBeConfiguredWhenEnabled() {
    if std::env::var("WYRMCOIL_RUN_WGPU_TESTS").ok().as_deref() != Some("1") {
        eprintln!("M40 surface probe skipped: set WYRMCOIL_RUN_WGPU_TESTS=1 to enable");
        return;
    }

    let event_loop = match winit::event_loop::EventLoop::new() {
        Ok(loop_value) => loop_value,
        Err(err) => {
            eprintln!("M40 surface probe unavailable: event loop creation failed: {err}");
            return;
        }
    };

    let window_attributes =
        winit::window::Window::default_attributes().with_title("WyrmCoil M40 Probe");
    let window = match event_loop.create_window(window_attributes) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("M40 surface probe unavailable: window creation failed: {err}");
            return;
        }
    };

    let instance = ::wgpu::Instance::new(&::wgpu::InstanceDescriptor::default());
    let surface = match instance.create_surface(&window) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("M40 surface probe unavailable: surface creation failed: {err}");
            return;
        }
    };

    let Some(adapter) = BlockOnReady(instance.request_adapter(&::wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..::wgpu::RequestAdapterOptions::default()
    })) else {
        eprintln!("M40 surface probe unavailable: no compatible adapter found");
        return;
    };

    let Ok((device, _queue)) =
        BlockOnReady(adapter.request_device(&::wgpu::DeviceDescriptor::default(), None))
    else {
        eprintln!("M40 surface probe unavailable: adapter could not create device");
        return;
    };

    let capabilities = surface.get_capabilities(&adapter);
    let info = BuildWgpuSurfaceCapabilitiesInfo(&capabilities);
    let plan = BuildWgpuSurfaceConfigPlan(
        SurfaceSize {
            Width: 64,
            Height: 64,
        },
        &info,
        WgpuSurfaceConfigPreferences::default(),
    )
    .expect("M40 probe should select a valid surface config plan");

    let configuration = BuildWgpuSurfaceConfiguration(plan);
    surface.configure(&device, &configuration);
}
