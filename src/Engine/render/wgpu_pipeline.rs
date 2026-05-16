#![allow(non_snake_case)]

use crate::Engine::render::{
    ColorTargetFormat, DepthFormat, RenderPipelineLayoutPlan, VertexFormat, VertexStepMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgpuVertexAttributeDesc {
    pub ShaderLocation: u32,
    pub OffsetBytes: u64,
    pub Format: wgpu::VertexFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuVertexBufferLayoutDesc {
    pub StrideBytes: u64,
    pub StepMode: wgpu::VertexStepMode,
    pub Attributes: Vec<WgpuVertexAttributeDesc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuRenderPipelineDescriptorPlan {
    pub Name: String,
    pub VertexEntry: String,
    pub PixelEntry: String,
    pub VertexBuffers: Vec<WgpuVertexBufferLayoutDesc>,
    pub ColorFormat: wgpu::TextureFormat,
    pub DepthFormat: Option<wgpu::TextureFormat>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuPipelineResourceError {
    MissingShaderBytes { Stage: String },
    EmptyShaderBytes { Stage: String },
}

pub struct WgpuRenderPipelineResource {
    pub Name: String,
    pub Pipeline: wgpu::RenderPipeline,
}

pub struct WgpuShaderModules<'a> {
    pub Vertex: &'a wgpu::ShaderModule,
    pub Pixel: &'a wgpu::ShaderModule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuRenderPipelineCreateError {
    EmptyShaderBytes { Stage: String },
}

pub fn MapVertexFormatToWgpu(format: VertexFormat) -> wgpu::VertexFormat {
    match format {
        VertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
        VertexFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
        VertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
        VertexFormat::Uint32 => wgpu::VertexFormat::Uint32,
    }
}

pub fn MapVertexStepModeToWgpu(step_mode: VertexStepMode) -> wgpu::VertexStepMode {
    match step_mode {
        VertexStepMode::Vertex => wgpu::VertexStepMode::Vertex,
        VertexStepMode::Instance => wgpu::VertexStepMode::Instance,
    }
}

pub fn MapColorTargetFormatToWgpu(format: ColorTargetFormat) -> wgpu::TextureFormat {
    match format {
        ColorTargetFormat::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        ColorTargetFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
    }
}

pub fn MapDepthFormatToWgpu(format: DepthFormat) -> wgpu::TextureFormat {
    match format {
        DepthFormat::Depth24Plus => wgpu::TextureFormat::Depth24Plus,
        DepthFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
    }
}

pub fn BuildWgpuRenderPipelineDescriptorPlan(
    plan: &RenderPipelineLayoutPlan,
) -> Result<WgpuRenderPipelineDescriptorPlan, WgpuPipelineResourceError> {
    if plan.Shaders.Vertex.SpirvBytes.is_empty() {
        return Err(WgpuPipelineResourceError::MissingShaderBytes {
            Stage: "vertex".to_string(),
        });
    }
    if plan.Shaders.Pixel.SpirvBytes.is_empty() {
        return Err(WgpuPipelineResourceError::MissingShaderBytes {
            Stage: "pixel".to_string(),
        });
    }

    let mut vertex_buffers = Vec::with_capacity(plan.VertexBuffers.len());
    for source_buffer in &plan.VertexBuffers {
        let mut attributes = Vec::with_capacity(source_buffer.Attributes.len());
        for source_attribute in &source_buffer.Attributes {
            attributes.push(WgpuVertexAttributeDesc {
                ShaderLocation: source_attribute.Location,
                OffsetBytes: source_attribute.OffsetBytes,
                Format: MapVertexFormatToWgpu(source_attribute.Format),
            });
        }

        vertex_buffers.push(WgpuVertexBufferLayoutDesc {
            StrideBytes: source_buffer.StrideBytes,
            StepMode: MapVertexStepModeToWgpu(source_buffer.StepMode),
            Attributes: attributes,
        });
    }

    Ok(WgpuRenderPipelineDescriptorPlan {
        Name: plan.Name.clone(),
        VertexEntry: plan.Shaders.Vertex.EntryPoint.clone(),
        PixelEntry: plan.Shaders.Pixel.EntryPoint.clone(),
        VertexBuffers: vertex_buffers,
        ColorFormat: MapColorTargetFormatToWgpu(plan.ColorTarget.Format),
        DepthFormat: plan
            .Depth
            .as_ref()
            .map(|depth| MapDepthFormatToWgpu(depth.Format)),
    })
}

pub fn ValidateWgpuShaderBytesForPipeline(
    plan: &WgpuRenderPipelineDescriptorPlan,
    layout: &RenderPipelineLayoutPlan,
) -> Result<(), WgpuRenderPipelineCreateError> {
    if layout.Shaders.Vertex.SpirvBytes.is_empty() {
        return Err(WgpuRenderPipelineCreateError::EmptyShaderBytes {
            Stage: "vertex".to_string(),
        });
    }
    if layout.Shaders.Pixel.SpirvBytes.is_empty() {
        return Err(WgpuRenderPipelineCreateError::EmptyShaderBytes {
            Stage: "pixel".to_string(),
        });
    }
    if plan.VertexEntry.trim().is_empty() {
        return Err(WgpuRenderPipelineCreateError::EmptyShaderBytes {
            Stage: "vertex".to_string(),
        });
    }
    if plan.PixelEntry.trim().is_empty() {
        return Err(WgpuRenderPipelineCreateError::EmptyShaderBytes {
            Stage: "pixel".to_string(),
        });
    }
    Ok(())
}

pub fn CreateWgpuRenderPipelineFromModules(
    device: &wgpu::Device,
    descriptor_plan: &WgpuRenderPipelineDescriptorPlan,
    layout_plan: &RenderPipelineLayoutPlan,
    modules: WgpuShaderModules<'_>,
) -> Result<WgpuRenderPipelineResource, WgpuRenderPipelineCreateError> {
    ValidateWgpuShaderBytesForPipeline(descriptor_plan, layout_plan)?;

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&descriptor_plan.Name),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let mut owned_attributes: Vec<Vec<wgpu::VertexAttribute>> =
        Vec::with_capacity(descriptor_plan.VertexBuffers.len());
    for buffer in &descriptor_plan.VertexBuffers {
        let mut attrs = Vec::with_capacity(buffer.Attributes.len());
        for attribute in &buffer.Attributes {
            attrs.push(wgpu::VertexAttribute {
                format: attribute.Format,
                offset: attribute.OffsetBytes,
                shader_location: attribute.ShaderLocation,
            });
        }
        owned_attributes.push(attrs);
    }

    let mut vertex_buffers = Vec::with_capacity(descriptor_plan.VertexBuffers.len());
    for (index, buffer) in descriptor_plan.VertexBuffers.iter().enumerate() {
        vertex_buffers.push(wgpu::VertexBufferLayout {
            array_stride: buffer.StrideBytes,
            step_mode: buffer.StepMode,
            attributes: &owned_attributes[index],
        });
    }

    let color_target = wgpu::ColorTargetState {
        format: descriptor_plan.ColorFormat,
        blend: Some(wgpu::BlendState::REPLACE),
        write_mask: wgpu::ColorWrites::ALL,
    };

    let depth_stencil = descriptor_plan
        .DepthFormat
        .map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: layout_plan
                .Depth
                .as_ref()
                .map(|x| x.DepthWriteEnabled)
                .unwrap_or(false),
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&descriptor_plan.Name),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: modules.Vertex,
            entry_point: Some(&descriptor_plan.VertexEntry),
            buffers: &vertex_buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: modules.Pixel,
            entry_point: Some(&descriptor_plan.PixelEntry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(color_target)],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    Ok(WgpuRenderPipelineResource {
        Name: descriptor_plan.Name.clone(),
        Pipeline: pipeline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::{
        BuildRenderPipelineLayoutPlan, ColorTargetDesc, CompiledPipelineDesc,
        CompiledShaderModuleDesc, DepthStencilDesc, RenderPipelineLayoutOptions,
        VertexAttributeDesc, VertexBufferLayoutDesc,
    };

    fn BuildValidLayoutPlan() -> RenderPipelineLayoutPlan {
        let compiled = CompiledPipelineDesc {
            Name: "FlatColorPlan".to_string(),
            SourceName: "flat_color.sdslv".to_string(),
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
        };

        BuildRenderPipelineLayoutPlan(
            compiled,
            RenderPipelineLayoutOptions {
                Name: "SpritePipelineLayout".to_string(),
                VertexBuffers: vec![VertexBufferLayoutDesc {
                    StrideBytes: 20,
                    StepMode: VertexStepMode::Vertex,
                    Attributes: vec![
                        VertexAttributeDesc {
                            Name: "Position".to_string(),
                            Location: 0,
                            Format: VertexFormat::Float32x3,
                            OffsetBytes: 0,
                        },
                        VertexAttributeDesc {
                            Name: "Uv".to_string(),
                            Location: 1,
                            Format: VertexFormat::Float32x2,
                            OffsetBytes: 12,
                        },
                    ],
                }],
                ColorTarget: ColorTargetDesc {
                    Format: ColorTargetFormat::Rgba8UnormSrgb,
                },
                Depth: Some(DepthStencilDesc {
                    Format: DepthFormat::Depth24Plus,
                    DepthWriteEnabled: true,
                }),
            },
        )
        .expect("test setup should build a valid M20 layout plan")
    }

    #[test]
    fn WgpuMappingsCoverM21FormatAndStepModes() {
        assert_eq!(
            MapVertexFormatToWgpu(VertexFormat::Float32x2),
            wgpu::VertexFormat::Float32x2,
            "Float32x2 should map to wgpu Float32x2"
        );
        assert_eq!(
            MapVertexFormatToWgpu(VertexFormat::Float32x3),
            wgpu::VertexFormat::Float32x3,
            "Float32x3 should map to wgpu Float32x3"
        );
        assert_eq!(
            MapVertexFormatToWgpu(VertexFormat::Float32x4),
            wgpu::VertexFormat::Float32x4,
            "Float32x4 should map to wgpu Float32x4"
        );
        assert_eq!(
            MapVertexFormatToWgpu(VertexFormat::Uint32),
            wgpu::VertexFormat::Uint32,
            "Uint32 should map to wgpu Uint32"
        );

        assert_eq!(
            MapVertexStepModeToWgpu(VertexStepMode::Vertex),
            wgpu::VertexStepMode::Vertex,
            "Vertex step mode should map to wgpu vertex step mode"
        );
        assert_eq!(
            MapVertexStepModeToWgpu(VertexStepMode::Instance),
            wgpu::VertexStepMode::Instance,
            "Instance step mode should map to wgpu instance step mode"
        );

        assert_eq!(
            MapColorTargetFormatToWgpu(ColorTargetFormat::Bgra8UnormSrgb),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            "Bgra8UnormSrgb should map to wgpu Bgra8UnormSrgb"
        );
        assert_eq!(
            MapColorTargetFormatToWgpu(ColorTargetFormat::Rgba8UnormSrgb),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "Rgba8UnormSrgb should map to wgpu Rgba8UnormSrgb"
        );

        assert_eq!(
            MapDepthFormatToWgpu(DepthFormat::Depth24Plus),
            wgpu::TextureFormat::Depth24Plus,
            "Depth24Plus should map to wgpu Depth24Plus"
        );
        assert_eq!(
            MapDepthFormatToWgpu(DepthFormat::Depth32Float),
            wgpu::TextureFormat::Depth32Float,
            "Depth32Float should map to wgpu Depth32Float"
        );
    }

    #[test]
    fn BuildWgpuRenderPipelineDescriptorPlanPreservesM20LayoutMetadata() {
        let plan = BuildValidLayoutPlan();
        let converted = BuildWgpuRenderPipelineDescriptorPlan(&plan)
            .expect("valid layout metadata should convert into a wgpu descriptor plan");

        assert_eq!(
            converted.Name, "SpritePipelineLayout",
            "plan name should be preserved"
        );
        assert_eq!(
            converted.VertexEntry, "FlatColor_VS",
            "vertex entry point should be preserved"
        );
        assert_eq!(
            converted.PixelEntry, "FlatColor_PS",
            "pixel entry point should be preserved"
        );
        assert_eq!(
            converted.ColorFormat,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "color format should map from M20 metadata"
        );
        assert_eq!(
            converted.DepthFormat,
            Some(wgpu::TextureFormat::Depth24Plus),
            "depth format should map from M20 metadata"
        );

        assert_eq!(
            converted.VertexBuffers.len(),
            1,
            "one vertex buffer should be preserved"
        );
        assert_eq!(
            converted.VertexBuffers[0].StrideBytes, 20,
            "buffer stride should be preserved"
        );
        assert_eq!(
            converted.VertexBuffers[0].StepMode,
            wgpu::VertexStepMode::Vertex,
            "buffer step mode should map correctly"
        );
        assert_eq!(
            converted.VertexBuffers[0].Attributes.len(),
            2,
            "attributes should be preserved"
        );
        assert_eq!(
            converted.VertexBuffers[0].Attributes[0].ShaderLocation, 0,
            "first attribute location should be preserved"
        );
        assert_eq!(
            converted.VertexBuffers[0].Attributes[0].OffsetBytes, 0,
            "first attribute offset should be preserved"
        );
        assert_eq!(
            converted.VertexBuffers[0].Attributes[0].Format,
            wgpu::VertexFormat::Float32x3,
            "first attribute format should map correctly"
        );
        assert_eq!(
            converted.VertexBuffers[0].Attributes[1].Format,
            wgpu::VertexFormat::Float32x2,
            "second attribute format should map correctly"
        );
    }

    #[test]
    fn BuildWgpuRenderPipelineDescriptorPlanOwnsConvertedAttributeData() {
        let converted = {
            let plan = BuildValidLayoutPlan();
            BuildWgpuRenderPipelineDescriptorPlan(&plan)
                .expect("conversion should produce owned descriptor metadata")
        };

        assert_eq!(
            converted.VertexBuffers[0].Attributes[0].ShaderLocation, 0,
            "converted metadata should remain accessible after source plan drops"
        );
    }

    #[test]
    fn ValidateWgpuShaderBytesForPipelineRejectsEmptyVertexBytes() {
        let mut plan = BuildValidLayoutPlan();
        plan.Shaders.Vertex.SpirvBytes.clear();
        let descriptor = BuildWgpuRenderPipelineDescriptorPlan(&BuildValidLayoutPlan())
            .expect("control descriptor should be valid");

        assert_eq!(
            ValidateWgpuShaderBytesForPipeline(&descriptor, &plan).unwrap_err(),
            WgpuRenderPipelineCreateError::EmptyShaderBytes {
                Stage: "vertex".to_string()
            },
            "empty vertex bytes should return a structured error"
        );
    }

    #[test]
    fn ValidateWgpuShaderBytesForPipelineRejectsEmptyPixelBytes() {
        let mut plan = BuildValidLayoutPlan();
        plan.Shaders.Pixel.SpirvBytes.clear();
        let descriptor = BuildWgpuRenderPipelineDescriptorPlan(&BuildValidLayoutPlan())
            .expect("control descriptor should be valid");

        assert_eq!(
            ValidateWgpuShaderBytesForPipeline(&descriptor, &plan).unwrap_err(),
            WgpuRenderPipelineCreateError::EmptyShaderBytes {
                Stage: "pixel".to_string()
            },
            "empty pixel bytes should return a structured error"
        );
    }
}
