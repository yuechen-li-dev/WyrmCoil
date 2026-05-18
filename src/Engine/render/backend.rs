#![allow(non_snake_case)]

//! Render backend boundary for WyrmCoil.
//!
//! Backend-neutral render contracts live in sibling modules (`extract`, `upload`,
//! `command_plan`, lifecycle/buffering, and assembly planning).
//! Backend adapters consume those contracts.

pub mod wgpu {
    //! `wgpu` backend adapter modules.
    //!
    //! WyrmCoil uses `wgpu` to bootstrap the golden path, but these APIs are
    //! backend-specific adapter seams rather than render-core architecture.

    pub use crate::Engine::render::draw::*;
    pub use crate::Engine::render::headless_submit::*;
    pub use crate::Engine::render::headless_target::*;
    pub use crate::Engine::render::wgpu::*;
    pub use crate::Engine::render::wgpu_pipeline::*;
    pub use crate::Engine::render::wgpu_shader_module::*;
    pub use crate::Engine::render::wgpu_surface::*;
    pub use crate::Engine::render::wgpu_texture::*;
    pub use crate::Engine::render::wgpu_wgsl_pipeline::*;
}

pub mod vulkan {
    //! Future native Vulkan backend seam.
    //!
    //! This module intentionally has no implementation in M36.
}
