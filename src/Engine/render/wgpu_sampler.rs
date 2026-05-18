#![allow(non_snake_case)]

use crate::Engine::render::sampler::{
    SamplerPlan, SamplerPlanError, TextureAddressMode, TextureFilterMode, ValidateSamplerPlan,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuSamplerDesc {
    pub Label: String,
    pub AddressModeU: wgpu::AddressMode,
    pub AddressModeV: wgpu::AddressMode,
    pub AddressModeW: wgpu::AddressMode,
    pub MagFilter: wgpu::FilterMode,
    pub MinFilter: wgpu::FilterMode,
    pub MipmapFilter: wgpu::FilterMode,
}

pub struct WgpuSamplerResource {
    pub Sampler: wgpu::Sampler,
    pub Label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuSamplerResourceError {
    InvalidPlan(SamplerPlanError),
}

pub fn BuildWgpuSamplerDesc(
    plan: &SamplerPlan,
) -> Result<WgpuSamplerDesc, WgpuSamplerResourceError> {
    ValidateSamplerPlan(plan).map_err(WgpuSamplerResourceError::InvalidPlan)?;

    Ok(WgpuSamplerDesc {
        Label: plan.Label.clone(),
        AddressModeU: MapTextureAddressMode(plan.AddressU),
        AddressModeV: MapTextureAddressMode(plan.AddressV),
        AddressModeW: MapTextureAddressMode(plan.AddressW),
        MagFilter: MapTextureFilterMode(plan.MagFilter),
        MinFilter: MapTextureFilterMode(plan.MinFilter),
        MipmapFilter: MapTextureFilterMode(plan.MipmapFilter),
    })
}

pub fn CreateWgpuSamplerResource(
    device: &wgpu::Device,
    plan: &SamplerPlan,
) -> Result<WgpuSamplerResource, WgpuSamplerResourceError> {
    let desc = BuildWgpuSamplerDesc(plan)?;

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(&desc.Label),
        address_mode_u: desc.AddressModeU,
        address_mode_v: desc.AddressModeV,
        address_mode_w: desc.AddressModeW,
        mag_filter: desc.MagFilter,
        min_filter: desc.MinFilter,
        mipmap_filter: desc.MipmapFilter,
        ..Default::default()
    });

    Ok(WgpuSamplerResource {
        Sampler: sampler,
        Label: desc.Label,
    })
}

fn MapTextureFilterMode(mode: TextureFilterMode) -> wgpu::FilterMode {
    match mode {
        TextureFilterMode::Nearest => wgpu::FilterMode::Nearest,
        TextureFilterMode::Linear => wgpu::FilterMode::Linear,
    }
}

fn MapTextureAddressMode(mode: TextureAddressMode) -> wgpu::AddressMode {
    match mode {
        TextureAddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        TextureAddressMode::Repeat => wgpu::AddressMode::Repeat,
        TextureAddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::sampler::{SamplerPlan, TextureAddressMode, TextureFilterMode};

    #[test]
    fn BuildWgpuSamplerDescMapsFilterAndAddressModes() {
        let plan = SamplerPlan::Build(
            "SamplerMap",
            TextureFilterMode::Nearest,
            TextureFilterMode::Linear,
            TextureFilterMode::Nearest,
            TextureAddressMode::ClampToEdge,
            TextureAddressMode::Repeat,
            TextureAddressMode::MirrorRepeat,
        )
        .expect("valid sampler plan should construct");

        let desc = BuildWgpuSamplerDesc(&plan)
            .expect("valid sampler plan should map to wgpu sampler descriptor");

        assert_eq!(desc.Label, plan.Label, "label should be preserved");
        assert_eq!(
            desc.MagFilter,
            wgpu::FilterMode::Nearest,
            "nearest mag filter should map to wgpu nearest"
        );
        assert_eq!(
            desc.MinFilter,
            wgpu::FilterMode::Linear,
            "linear min filter should map to wgpu linear"
        );
        assert_eq!(
            desc.MipmapFilter,
            wgpu::FilterMode::Nearest,
            "nearest mipmap filter should map to wgpu nearest"
        );
        assert_eq!(
            desc.AddressModeU,
            wgpu::AddressMode::ClampToEdge,
            "clamp address U should map to wgpu clamp"
        );
        assert_eq!(
            desc.AddressModeV,
            wgpu::AddressMode::Repeat,
            "repeat address V should map to wgpu repeat"
        );
        assert_eq!(
            desc.AddressModeW,
            wgpu::AddressMode::MirrorRepeat,
            "mirror-repeat address W should map to wgpu mirror-repeat"
        );
    }

    #[test]
    fn BuildWgpuSamplerDescRejectsInvalidPlan() {
        let invalid = SamplerPlan {
            Label: " ".to_string(),
            MagFilter: TextureFilterMode::Linear,
            MinFilter: TextureFilterMode::Linear,
            MipmapFilter: TextureFilterMode::Nearest,
            AddressU: TextureAddressMode::ClampToEdge,
            AddressV: TextureAddressMode::ClampToEdge,
            AddressW: TextureAddressMode::ClampToEdge,
        };

        assert_eq!(
            BuildWgpuSamplerDesc(&invalid).unwrap_err(),
            WgpuSamplerResourceError::InvalidPlan(SamplerPlanError::EmptyLabel),
            "invalid labels should be rejected via InvalidPlan"
        );
    }

    #[test]
    fn BuildWgpuSamplerDescIsDeterministic() {
        let plan = SamplerPlan::LinearClamp("StableSampler")
            .expect("linear-clamp plan should be valid for deterministic mapping checks");
        let a = BuildWgpuSamplerDesc(&plan).expect("first descriptor build should succeed");
        let b = BuildWgpuSamplerDesc(&plan).expect("second descriptor build should succeed");
        assert_eq!(a, b, "repeated descriptor builds should be deterministic");
    }
}
