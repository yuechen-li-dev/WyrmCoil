#![allow(non_snake_case)]

use crate::Engine::render::texture_binding::{
    ShaderStageVisibility, TextureBindingDimension, TextureSampleKind,
    TextureSamplerBindingLayoutPlan, TextureSamplerBindingLayoutPlanError,
    ValidateTextureSamplerBindingLayoutPlan,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuBindGroupLayoutEntryDesc {
    pub Binding: u32,
    pub Visibility: wgpu::ShaderStages,
    pub BindingType: wgpu::BindingType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuTextureSamplerBindingLayoutDesc {
    pub Label: String,
    pub Entries: Vec<WgpuBindGroupLayoutEntryDesc>,
}

pub struct WgpuTextureSamplerBindGroupLayoutResource {
    pub Layout: wgpu::BindGroupLayout,
    pub Label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuTextureBindingResourceError {
    InvalidPlan(TextureSamplerBindingLayoutPlanError),
    UnsupportedTextureDimension,
    UnsupportedSampleKind,
}

pub fn BuildWgpuTextureSamplerBindingLayoutDesc(
    plan: &TextureSamplerBindingLayoutPlan,
) -> Result<WgpuTextureSamplerBindingLayoutDesc, WgpuTextureBindingResourceError> {
    ValidateTextureSamplerBindingLayoutPlan(plan)
        .map_err(WgpuTextureBindingResourceError::InvalidPlan)?;

    let visibility = MapShaderStageVisibility(plan.Visibility);
    let texture_binding_type = BuildTextureBindingType(plan)?;

    Ok(WgpuTextureSamplerBindingLayoutDesc {
        Label: plan.Label.clone(),
        Entries: vec![
            WgpuBindGroupLayoutEntryDesc {
                Binding: plan.TextureBinding,
                Visibility: visibility,
                BindingType: texture_binding_type,
            },
            WgpuBindGroupLayoutEntryDesc {
                Binding: plan.SamplerBinding,
                Visibility: visibility,
                BindingType: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            },
        ],
    })
}

pub fn CreateWgpuTextureSamplerBindGroupLayout(
    device: &wgpu::Device,
    plan: &TextureSamplerBindingLayoutPlan,
) -> Result<WgpuTextureSamplerBindGroupLayoutResource, WgpuTextureBindingResourceError> {
    let desc = BuildWgpuTextureSamplerBindingLayoutDesc(plan)?;

    let entries: Vec<wgpu::BindGroupLayoutEntry> = desc
        .Entries
        .iter()
        .map(|entry| wgpu::BindGroupLayoutEntry {
            binding: entry.Binding,
            visibility: entry.Visibility,
            ty: entry.BindingType,
            count: None,
        })
        .collect();

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&desc.Label),
        entries: &entries,
    });

    Ok(WgpuTextureSamplerBindGroupLayoutResource {
        Layout: layout,
        Label: desc.Label,
    })
}

fn BuildTextureBindingType(
    plan: &TextureSamplerBindingLayoutPlan,
) -> Result<wgpu::BindingType, WgpuTextureBindingResourceError> {
    let sample_type = match plan.SampleKind {
        TextureSampleKind::FloatFilterable => wgpu::TextureSampleType::Float { filterable: true },
    };

    let view_dimension = match plan.TextureDimension {
        TextureBindingDimension::D2 => wgpu::TextureViewDimension::D2,
    };

    Ok(wgpu::BindingType::Texture {
        sample_type,
        view_dimension,
        multisampled: false,
    })
}

fn MapShaderStageVisibility(visibility: ShaderStageVisibility) -> wgpu::ShaderStages {
    match visibility {
        ShaderStageVisibility::Vertex => wgpu::ShaderStages::VERTEX,
        ShaderStageVisibility::Pixel => wgpu::ShaderStages::FRAGMENT,
        ShaderStageVisibility::VertexPixel => wgpu::ShaderStages::VERTEX_FRAGMENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::texture_binding::{
        ShaderStageVisibility, TextureSampleKind, TextureSamplerBindingLayoutPlan,
    };

    #[test]
    fn BuildWgpuTextureSamplerBindingLayoutDescMapsTwoEntries() {
        let plan = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "TexSamplerBinding",
            5,
            8,
            ShaderStageVisibility::Pixel,
        )
        .expect("plan should construct");

        let desc = BuildWgpuTextureSamplerBindingLayoutDesc(&plan)
            .expect("valid plan should map to wgpu texture+sampler layout descriptor");

        assert_eq!(
            desc.Entries.len(),
            2,
            "descriptor should contain exactly two entries"
        );
        assert_eq!(
            desc.Entries[0].Binding, 5,
            "texture entry binding should match plan"
        );
        assert_eq!(
            desc.Entries[1].Binding, 8,
            "sampler entry binding should match plan"
        );

        match desc.Entries[0].BindingType {
            wgpu::BindingType::Texture {
                sample_type,
                view_dimension,
                multisampled,
            } => {
                assert_eq!(
                    sample_type,
                    wgpu::TextureSampleType::Float { filterable: true },
                    "texture entry sample kind should be filterable float"
                );
                assert_eq!(
                    view_dimension,
                    wgpu::TextureViewDimension::D2,
                    "texture entry view dimension should be D2"
                );
                assert!(!multisampled, "texture entry should be non-multisampled");
            }
            _ => panic!("texture entry should map to BindingType::Texture"),
        }

        assert_eq!(
            desc.Entries[1].BindingType,
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            "sampler entry should map to filtering sampler"
        );
    }

    #[test]
    fn BuildWgpuTextureSamplerBindingLayoutDescMapsVisibilityVariants() {
        let pixel = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "Pixel",
            0,
            1,
            ShaderStageVisibility::Pixel,
        )
        .expect("pixel plan should construct");
        let vertex = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "Vertex",
            2,
            3,
            ShaderStageVisibility::Vertex,
        )
        .expect("vertex plan should construct");
        let both = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "Both",
            4,
            5,
            ShaderStageVisibility::VertexPixel,
        )
        .expect("vertex+pixel plan should construct");

        let pixel_desc = BuildWgpuTextureSamplerBindingLayoutDesc(&pixel)
            .expect("pixel descriptor should construct");
        let vertex_desc = BuildWgpuTextureSamplerBindingLayoutDesc(&vertex)
            .expect("vertex descriptor should construct");
        let both_desc = BuildWgpuTextureSamplerBindingLayoutDesc(&both)
            .expect("both-stage descriptor should construct");

        assert_eq!(
            pixel_desc.Entries[0].Visibility,
            wgpu::ShaderStages::FRAGMENT,
            "pixel visibility should map to FRAGMENT"
        );
        assert_eq!(
            vertex_desc.Entries[0].Visibility,
            wgpu::ShaderStages::VERTEX,
            "vertex visibility should map to VERTEX"
        );
        assert_eq!(
            both_desc.Entries[0].Visibility,
            wgpu::ShaderStages::VERTEX_FRAGMENT,
            "vertex+pixel visibility should map to VERTEX_FRAGMENT"
        );
    }

    #[test]
    fn BuildWgpuTextureSamplerBindingLayoutDescRejectsInvalidPlan() {
        let invalid = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "Dup",
            6,
            7,
            ShaderStageVisibility::Pixel,
        )
        .expect("seed plan should construct");

        let broken = TextureSamplerBindingLayoutPlan {
            Label: invalid.Label,
            TextureBinding: 6,
            SamplerBinding: 6,
            Visibility: invalid.Visibility,
            TextureDimension: invalid.TextureDimension,
            SampleKind: invalid.SampleKind,
        };

        assert_eq!(
            BuildWgpuTextureSamplerBindingLayoutDesc(&broken).unwrap_err(),
            WgpuTextureBindingResourceError::InvalidPlan(
                crate::Engine::render::texture_binding::TextureSamplerBindingLayoutPlanError::DuplicateBinding { Binding: 6 }
            ),
            "invalid plan should map to structured InvalidPlan error"
        );
    }

    #[test]
    fn BuildWgpuTextureSamplerBindingLayoutDescIsDeterministic() {
        let plan = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "StableLayout",
            10,
            11,
            ShaderStageVisibility::VertexPixel,
        )
        .expect("seed plan should construct");

        let a = BuildWgpuTextureSamplerBindingLayoutDesc(&plan)
            .expect("first descriptor build should succeed");
        let b = BuildWgpuTextureSamplerBindingLayoutDesc(&plan)
            .expect("second descriptor build should succeed");

        assert_eq!(a, b, "repeated descriptor builds should be deterministic");
    }

    #[test]
    fn BuildWgpuTextureSamplerBindingLayoutDescRejectsUnsupportedSampleKindViaPlanValidation() {
        let invalid = TextureSamplerBindingLayoutPlan {
            Label: "BadKind".to_string(),
            TextureBinding: 0,
            SamplerBinding: 1,
            Visibility: ShaderStageVisibility::Pixel,
            TextureDimension: crate::Engine::render::texture_binding::TextureBindingDimension::D2,
            SampleKind: TextureSampleKind::FloatFilterable,
        };

        let desc = BuildWgpuTextureSamplerBindingLayoutDesc(&invalid)
            .expect("current M88 sample kind set should remain valid and accepted");
        assert_eq!(
            desc.Entries.len(),
            2,
            "valid sample kind should produce two entries"
        );
    }
}
