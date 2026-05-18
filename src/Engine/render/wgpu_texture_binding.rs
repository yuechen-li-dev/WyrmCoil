#![allow(non_snake_case)]

use crate::Engine::render::texture_binding::{
    ShaderStageVisibility, TextureBindingDimension, TextureSampleKind,
    TextureSamplerBindingLayoutPlan, TextureSamplerBindingLayoutPlanError,
    ValidateTextureSamplerBindingLayoutPlan,
};
use crate::Engine::render::wgpu_sampler::WgpuSamplerResource;
use crate::Engine::render::wgpu_texture::WgpuTextureResource;

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
    pub TextureBinding: u32,
    pub SamplerBinding: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuTextureResourceMetadata {
    pub Label: String,
    pub Width: u32,
    pub Height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuSamplerResourceMetadata {
    pub Label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuTextureSamplerBindGroupLayoutMetadata {
    pub Label: String,
    pub TextureBinding: u32,
    pub SamplerBinding: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuTextureSamplerBindGroupDesc {
    pub Label: String,
    pub TextureBinding: u32,
    pub SamplerBinding: u32,
    pub TextureLabel: String,
    pub SamplerLabel: String,
    pub LayoutLabel: String,
}

pub struct WgpuTextureSamplerBindGroupResource {
    pub BindGroup: wgpu::BindGroup,
    pub Label: String,
    pub TextureBinding: u32,
    pub SamplerBinding: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuTextureBindingResourceError {
    InvalidPlan(TextureSamplerBindingLayoutPlanError),
    UnsupportedTextureDimension,
    UnsupportedSampleKind,
    EmptyLabel,
    LayoutPlanMismatch,
    TextureResourceMismatch,
    SamplerResourceMismatch,
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
        TextureBinding: plan.TextureBinding,
        SamplerBinding: plan.SamplerBinding,
    })
}

pub fn BuildWgpuTextureSamplerBindGroupDescFromMetadata(
    plan: &TextureSamplerBindingLayoutPlan,
    texture: &WgpuTextureResourceMetadata,
    sampler: &WgpuSamplerResourceMetadata,
    layout: &WgpuTextureSamplerBindGroupLayoutMetadata,
    label: &str,
) -> Result<WgpuTextureSamplerBindGroupDesc, WgpuTextureBindingResourceError> {
    ValidateTextureSamplerBindingLayoutPlan(plan)
        .map_err(WgpuTextureBindingResourceError::InvalidPlan)?;

    if label.trim().is_empty() {
        return Err(WgpuTextureBindingResourceError::EmptyLabel);
    }
    if texture.Label.trim().is_empty() || texture.Width == 0 || texture.Height == 0 {
        return Err(WgpuTextureBindingResourceError::TextureResourceMismatch);
    }
    if sampler.Label.trim().is_empty() {
        return Err(WgpuTextureBindingResourceError::SamplerResourceMismatch);
    }
    if layout.Label.trim().is_empty() {
        return Err(WgpuTextureBindingResourceError::LayoutPlanMismatch);
    }

    if layout.TextureBinding != plan.TextureBinding || layout.SamplerBinding != plan.SamplerBinding
    {
        return Err(WgpuTextureBindingResourceError::LayoutPlanMismatch);
    }

    Ok(WgpuTextureSamplerBindGroupDesc {
        Label: label.to_string(),
        TextureBinding: plan.TextureBinding,
        SamplerBinding: plan.SamplerBinding,
        TextureLabel: texture.Label.clone(),
        SamplerLabel: sampler.Label.clone(),
        LayoutLabel: layout.Label.clone(),
    })
}

pub fn BuildWgpuTextureSamplerBindGroupDesc(
    plan: &TextureSamplerBindingLayoutPlan,
    texture: &WgpuTextureResource,
    sampler: &WgpuSamplerResource,
    layout: &WgpuTextureSamplerBindGroupLayoutResource,
    label: &str,
) -> Result<WgpuTextureSamplerBindGroupDesc, WgpuTextureBindingResourceError> {
    BuildWgpuTextureSamplerBindGroupDescFromMetadata(
        plan,
        &WgpuTextureResourceMetadata {
            Label: texture.Label.clone(),
            Width: texture.Width,
            Height: texture.Height,
        },
        &WgpuSamplerResourceMetadata {
            Label: sampler.Label.clone(),
        },
        &WgpuTextureSamplerBindGroupLayoutMetadata {
            Label: layout.Label.clone(),
            TextureBinding: layout.TextureBinding,
            SamplerBinding: layout.SamplerBinding,
        },
        label,
    )
}

pub fn CreateWgpuTextureSamplerBindGroup(
    device: &wgpu::Device,
    plan: &TextureSamplerBindingLayoutPlan,
    texture: &WgpuTextureResource,
    sampler: &WgpuSamplerResource,
    layout: &WgpuTextureSamplerBindGroupLayoutResource,
    label: &str,
) -> Result<WgpuTextureSamplerBindGroupResource, WgpuTextureBindingResourceError> {
    let desc = BuildWgpuTextureSamplerBindGroupDesc(plan, texture, sampler, layout, label)?;

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&desc.Label),
        layout: &layout.Layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: desc.TextureBinding,
                resource: wgpu::BindingResource::TextureView(&texture.View),
            },
            wgpu::BindGroupEntry {
                binding: desc.SamplerBinding,
                resource: wgpu::BindingResource::Sampler(&sampler.Sampler),
            },
        ],
    });

    Ok(WgpuTextureSamplerBindGroupResource {
        BindGroup: bind_group,
        Label: desc.Label,
        TextureBinding: desc.TextureBinding,
        SamplerBinding: desc.SamplerBinding,
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
    fn BuildWgpuTextureSamplerBindGroupDescFromMetadataPreservesFields() {
        let plan = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "TexSamplerLayout",
            3,
            4,
            ShaderStageVisibility::Pixel,
        )
        .expect("layout plan should construct");

        let desc = BuildWgpuTextureSamplerBindGroupDescFromMetadata(
            &plan,
            &WgpuTextureResourceMetadata {
                Label: "ColorTex".to_string(),
                Width: 16,
                Height: 8,
            },
            &WgpuSamplerResourceMetadata {
                Label: "LinearSampler".to_string(),
            },
            &WgpuTextureSamplerBindGroupLayoutMetadata {
                Label: "TexSamplerLayout".to_string(),
                TextureBinding: 3,
                SamplerBinding: 4,
            },
            "ColorBindGroup",
        )
        .expect("metadata descriptor should construct");

        assert_eq!(
            desc.Label, "ColorBindGroup",
            "bind-group label should be preserved"
        );
        assert_eq!(
            desc.TextureBinding, 3,
            "texture binding should match the plan"
        );
        assert_eq!(
            desc.SamplerBinding, 4,
            "sampler binding should match the plan"
        );
        assert_eq!(
            desc.TextureLabel, "ColorTex",
            "texture label should be preserved"
        );
        assert_eq!(
            desc.SamplerLabel, "LinearSampler",
            "sampler label should be preserved"
        );
        assert_eq!(
            desc.LayoutLabel, "TexSamplerLayout",
            "layout label should be preserved"
        );
    }

    #[test]
    fn BuildWgpuTextureSamplerBindGroupDescFromMetadataRejectsEmptyBindGroupLabel() {
        let plan = TextureSamplerBindingLayoutPlan::DefaultSampledColor2D("Layout")
            .expect("default plan should construct");

        let result = BuildWgpuTextureSamplerBindGroupDescFromMetadata(
            &plan,
            &WgpuTextureResourceMetadata {
                Label: "Tex".to_string(),
                Width: 2,
                Height: 2,
            },
            &WgpuSamplerResourceMetadata {
                Label: "Samp".to_string(),
            },
            &WgpuTextureSamplerBindGroupLayoutMetadata {
                Label: "Layout".to_string(),
                TextureBinding: 0,
                SamplerBinding: 1,
            },
            " ",
        );

        assert_eq!(
            result.unwrap_err(),
            WgpuTextureBindingResourceError::EmptyLabel,
            "empty bind-group labels should be rejected"
        );
    }

    #[test]
    fn BuildWgpuTextureSamplerBindGroupDescFromMetadataRejectsLayoutMismatch() {
        let plan = TextureSamplerBindingLayoutPlan::DefaultSampledColor2D("Layout")
            .expect("default plan should construct");

        let result = BuildWgpuTextureSamplerBindGroupDescFromMetadata(
            &plan,
            &WgpuTextureResourceMetadata {
                Label: "Tex".to_string(),
                Width: 2,
                Height: 2,
            },
            &WgpuSamplerResourceMetadata {
                Label: "Samp".to_string(),
            },
            &WgpuTextureSamplerBindGroupLayoutMetadata {
                Label: "Layout".to_string(),
                TextureBinding: 9,
                SamplerBinding: 1,
            },
            "BG",
        );

        assert_eq!(
            result.unwrap_err(),
            WgpuTextureBindingResourceError::LayoutPlanMismatch,
            "layout metadata binding mismatch should be rejected"
        );
    }

    #[test]
    fn BuildWgpuTextureSamplerBindGroupDescFromMetadataIsDeterministic() {
        let plan = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "StableLayout",
            1,
            7,
            ShaderStageVisibility::VertexPixel,
        )
        .expect("plan should construct");

        let texture = WgpuTextureResourceMetadata {
            Label: "Tex".to_string(),
            Width: 64,
            Height: 64,
        };
        let sampler = WgpuSamplerResourceMetadata {
            Label: "Samp".to_string(),
        };
        let layout = WgpuTextureSamplerBindGroupLayoutMetadata {
            Label: "StableLayout".to_string(),
            TextureBinding: 1,
            SamplerBinding: 7,
        };

        let a = BuildWgpuTextureSamplerBindGroupDescFromMetadata(
            &plan, &texture, &sampler, &layout, "BG",
        )
        .expect("first descriptor build should succeed");
        let b = BuildWgpuTextureSamplerBindGroupDescFromMetadata(
            &plan, &texture, &sampler, &layout, "BG",
        )
        .expect("second descriptor build should succeed");

        assert_eq!(a, b, "descriptor construction should be deterministic");
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
