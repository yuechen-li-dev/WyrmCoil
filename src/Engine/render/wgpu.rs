#![allow(non_snake_case)]

use crate::Engine::wyrmcoil::RenderSnapshot;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearColor {
    pub R: f64,
    pub G: f64,
    pub B: f64,
    pub A: f64,
}

impl Default for ClearColor {
    fn default() -> Self {
        Self {
            R: 0.05,
            G: 0.05,
            B: 0.08,
            A: 1.0,
        }
    }
}

impl ClearColor {
    pub fn ToWgpu(&self) -> wgpu::Color {
        wgpu::Color {
            r: self.R,
            g: self.G,
            b: self.B,
            a: self.A,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RendererConfig {
    pub ClearColor: ClearColor,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            ClearColor: ClearColor::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderStats {
    pub SnapshotFrame: u64,
    pub RenderItems: usize,
    pub ClearColor: ClearColor,
}

pub struct RenderBackend {
    Config: RendererConfig,
    LastStats: Option<RenderStats>,
}

impl RenderBackend {
    pub fn New(config: RendererConfig) -> Self {
        Self {
            Config: config,
            LastStats: None,
        }
    }

    pub fn Config(&self) -> RendererConfig {
        self.Config
    }

    pub fn RenderSnapshot(&mut self, snapshot: &RenderSnapshot) -> RenderStats {
        let stats = RenderStats {
            SnapshotFrame: snapshot.Frame,
            RenderItems: snapshot.Items.len(),
            ClearColor: self.Config.ClearColor,
        };
        self.LastStats = Some(stats);
        stats
    }

    pub fn LastStats(&self) -> Option<RenderStats> {
        self.LastStats
    }

    pub fn BuildClearOp(&self) -> wgpu::Operations<wgpu::Color> {
        wgpu::Operations {
            load: wgpu::LoadOp::Clear(self.Config.ClearColor.ToWgpu()),
            store: wgpu::StoreOp::Store,
        }
    }
}
