#![allow(non_snake_case)]

use crate::Engine::render::{
    BuildWgpuRenderPipelineDescriptorPlan, CreateWgpuRenderPipelineFromModules,
    RenderPipelineLayoutPlan, WgpuRenderPipelineCreateError, WgpuRenderPipelineDescriptorPlan,
    WgpuRenderPipelineResource, WgpuShaderModules,
};
use crate::Engine::render::{
    CreateWgpuShaderModuleFromWgsl, ValidateWgslShaderModulePlan, WgslShaderModulePlan,
    WgslShaderModulePlanError,
};

pub const MINIMAL_SPRITE_WGSL_FIXTURE: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) sprite_id: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    return output;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 1.0, 1.0);
}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgslPipelineCreateOptions {
    pub VertexEntry: String,
    pub PixelEntry: String,
}

impl Default for WgslPipelineCreateOptions {
    fn default() -> Self {
        Self {
            VertexEntry: "vs_main".to_string(),
            PixelEntry: "fs_main".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgslPipelinePlan {
    pub Label: String,
    pub SourceName: String,
    pub VertexEntry: String,
    pub PixelEntry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgslPipelineCreateError {
    ShaderPlan(WgslShaderModulePlanError),
    EmptyVertexEntry,
    EmptyPixelEntry,
    Pipeline(WgpuRenderPipelineCreateError),
}

pub fn BuildWgslPipelinePlan(
    shader_plan: &WgslShaderModulePlan,
    options: &WgslPipelineCreateOptions,
) -> Result<WgslPipelinePlan, WgslPipelineCreateError> {
    ValidateWgslShaderModulePlan(shader_plan).map_err(WgslPipelineCreateError::ShaderPlan)?;

    if options.VertexEntry.trim().is_empty() {
        return Err(WgslPipelineCreateError::EmptyVertexEntry);
    }
    if options.PixelEntry.trim().is_empty() {
        return Err(WgslPipelineCreateError::EmptyPixelEntry);
    }

    Ok(WgslPipelinePlan {
        Label: shader_plan.Label.clone(),
        SourceName: shader_plan.SourceName.clone(),
        VertexEntry: options.VertexEntry.clone(),
        PixelEntry: options.PixelEntry.clone(),
    })
}

pub fn CreateWgpuRenderPipelineFromWgsl(
    device: &wgpu::Device,
    shader_plan: &WgslShaderModulePlan,
    layout_plan: &RenderPipelineLayoutPlan,
    options: &WgslPipelineCreateOptions,
) -> Result<WgpuRenderPipelineResource, WgslPipelineCreateError> {
    let wgsl_plan = BuildWgslPipelinePlan(shader_plan, options)?;
    let shader_module = CreateWgpuShaderModuleFromWgsl(device, shader_plan)
        .map_err(WgslPipelineCreateError::ShaderPlan)?;

    let mut descriptor_plan: WgpuRenderPipelineDescriptorPlan =
        BuildWgpuRenderPipelineDescriptorPlan(layout_plan).map_err(|_| {
            WgslPipelineCreateError::Pipeline(WgpuRenderPipelineCreateError::EmptyShaderBytes {
                Stage: "layout".to_string(),
            })
        })?;
    descriptor_plan.Name = wgsl_plan.Label;
    descriptor_plan.VertexEntry = wgsl_plan.VertexEntry;
    descriptor_plan.PixelEntry = wgsl_plan.PixelEntry;

    CreateWgpuRenderPipelineFromModules(
        device,
        &descriptor_plan,
        layout_plan,
        WgpuShaderModules {
            Vertex: &shader_module.Module,
            Pixel: &shader_module.Module,
        },
    )
    .map_err(WgslPipelineCreateError::Pipeline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::{
        BuildRenderPipelineLayoutPlan, BuildWgslPlanFromStrategyRequest, BuildWgslShaderModulePlan,
        ColorTargetDesc, ColorTargetFormat, CompiledPipelineDesc, CompiledShaderModuleDesc,
        RenderPipelineLayoutOptions, VertexAttributeDesc, VertexBufferLayoutDesc, VertexFormat,
        VertexStepMode,
    };
    use crate::Engine::shader::{
        SelectShaderSourceStrategy, ShaderSourceMode, ShaderSourceStrategyConstraints,
        ShaderSourceStrategyRequest,
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
                    StrideBytes: 12,
                    StepMode: VertexStepMode::Vertex,
                    Attributes: vec![
                        VertexAttributeDesc {
                            Name: "Position".to_string(),
                            Location: 0,
                            Format: VertexFormat::Float32x2,
                            OffsetBytes: 0,
                        },
                        VertexAttributeDesc {
                            Name: "SpriteId".to_string(),
                            Location: 1,
                            Format: VertexFormat::Uint32,
                            OffsetBytes: 8,
                        },
                    ],
                }],
                ColorTarget: ColorTargetDesc {
                    Format: ColorTargetFormat::Rgba8UnormSrgb,
                },
                Depth: None,
            },
        )
        .expect("valid test layout plan should build")
    }

    #[test]
    fn ValidWgslPipelinePlanWithDefaultEntriesSucceeds() {
        let shader_plan =
            BuildWgslShaderModulePlan("Sprite.WGSL", "sprite.wgsl", MINIMAL_SPRITE_WGSL_FIXTURE)
                .expect("valid fixture should build WGSL module plan");

        let plan = BuildWgslPipelinePlan(&shader_plan, &WgslPipelineCreateOptions::default())
            .expect("default WGSL entry names should plan successfully");
        assert_eq!(
            plan.VertexEntry, "vs_main",
            "default vertex entry should be vs_main"
        );
        assert_eq!(
            plan.PixelEntry, "fs_main",
            "default pixel entry should be fs_main"
        );
    }

    #[test]
    fn EmptyVertexEntryRejected() {
        let shader_plan =
            BuildWgslShaderModulePlan("Sprite.WGSL", "sprite.wgsl", MINIMAL_SPRITE_WGSL_FIXTURE)
                .expect("valid fixture should build WGSL module plan");
        let options = WgslPipelineCreateOptions {
            VertexEntry: "\t ".to_string(),
            PixelEntry: "fs_main".to_string(),
        };
        assert_eq!(
            BuildWgslPipelinePlan(&shader_plan, &options).unwrap_err(),
            WgslPipelineCreateError::EmptyVertexEntry,
            "empty vertex entry should be rejected"
        );
    }

    #[test]
    fn EmptyPixelEntryRejected() {
        let shader_plan =
            BuildWgslShaderModulePlan("Sprite.WGSL", "sprite.wgsl", MINIMAL_SPRITE_WGSL_FIXTURE)
                .expect("valid fixture should build WGSL module plan");
        let options = WgslPipelineCreateOptions {
            VertexEntry: "vs_main".to_string(),
            PixelEntry: " ".to_string(),
        };
        assert_eq!(
            BuildWgslPipelinePlan(&shader_plan, &options).unwrap_err(),
            WgslPipelineCreateError::EmptyPixelEntry,
            "empty pixel entry should be rejected"
        );
    }

    #[test]
    fn InvalidWgslPlanErrorPropagates() {
        let shader_plan = WgslShaderModulePlan {
            Label: "Bad".to_string(),
            SourceName: "bad.wgsl".to_string(),
            Source: " ".to_string(),
        };
        assert_eq!(
            BuildWgslPipelinePlan(&shader_plan, &WgslPipelineCreateOptions::default()).unwrap_err(),
            WgslPipelineCreateError::ShaderPlan(WgslShaderModulePlanError::EmptySource),
            "invalid WGSL module plan should propagate a structured shader-plan error"
        );
    }

    #[test]
    fn M35StrategySelectionCanFeedWgslPipelinePlan() {
        let request = ShaderSourceStrategyRequest {
            Label: "ShaderStrategyRequest".to_string(),
            SdslVSource: Some("shader S {}".to_string()),
            WgslSource: Some(MINIMAL_SPRITE_WGSL_FIXTURE.to_string()),
            Constraints: ShaderSourceStrategyConstraints {
                PreferWgsl: true,
                ..ShaderSourceStrategyConstraints::default()
            },
        };
        let decision = SelectShaderSourceStrategy(&request);
        assert_eq!(
            decision.SelectedMode,
            ShaderSourceMode::Wgsl,
            "prefer WGSL should select WGSL mode when available"
        );

        let shader_plan =
            BuildWgslPlanFromStrategyRequest(&decision, &request, "FromStrategy", "strategy.wgsl")
                .expect("selected WGSL strategy should build WGSL module plan");
        let pipeline_plan =
            BuildWgslPipelinePlan(&shader_plan, &WgslPipelineCreateOptions::default())
                .expect("valid WGSL module plan should build pipeline plan");
        assert_eq!(
            pipeline_plan.SourceName, "strategy.wgsl",
            "source name should flow through planning helpers"
        );
    }

    #[test]
    fn MinimalFixtureIsNonEmptyAndPreserved() {
        let shader_plan =
            BuildWgslShaderModulePlan("Fixture", "fixture.wgsl", MINIMAL_SPRITE_WGSL_FIXTURE)
                .expect("fixture source should build a module plan");
        assert!(
            !MINIMAL_SPRITE_WGSL_FIXTURE.trim().is_empty(),
            "fixture WGSL text should be non-empty"
        );
        assert_eq!(
            shader_plan.Source, MINIMAL_SPRITE_WGSL_FIXTURE,
            "fixture WGSL text should be preserved exactly"
        );
    }

    #[test]
    fn PipelinePlanUsesCallerEntriesNotSdslvMetadataEntries() {
        let layout = BuildValidLayoutPlan();
        let desc = BuildWgpuRenderPipelineDescriptorPlan(&layout)
            .expect("descriptor conversion should succeed");
        assert_eq!(
            desc.VertexEntry, "FlatColor_VS",
            "control confirms SDSL-V descriptor entries differ from default WGSL entries"
        );

        let shader_plan =
            BuildWgslShaderModulePlan("Sprite.WGSL", "sprite.wgsl", MINIMAL_SPRITE_WGSL_FIXTURE)
                .expect("valid fixture should build WGSL module plan");
        let planned = BuildWgslPipelinePlan(&shader_plan, &WgslPipelineCreateOptions::default())
            .expect("WGSL pipeline planning should succeed with defaults");
        assert_eq!(
            planned.VertexEntry, "vs_main",
            "WGSL pipeline plan should use caller WGSL entry names"
        );
    }
}
