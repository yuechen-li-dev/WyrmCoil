#![allow(non_snake_case)]

use crate::Engine::shader::{
    ShaderSourceMode, ShaderSourceStrategyDecision, ShaderSourceStrategyRequest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgslShaderModulePlan {
    pub Label: String,
    pub SourceName: String,
    pub Source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgslShaderModulePlanError {
    EmptyLabel,
    EmptySourceName,
    EmptySource,
    StrategyDidNotSelectWgsl,
    StrategyRequestMissingWgslSource,
}

pub struct WgpuShaderModuleResource {
    pub Label: String,
    pub SourceName: String,
    pub Module: wgpu::ShaderModule,
}

pub fn ValidateWgslShaderModulePlan(
    plan: &WgslShaderModulePlan,
) -> Result<(), WgslShaderModulePlanError> {
    if plan.Label.trim().is_empty() {
        return Err(WgslShaderModulePlanError::EmptyLabel);
    }
    if plan.SourceName.trim().is_empty() {
        return Err(WgslShaderModulePlanError::EmptySourceName);
    }
    if plan.Source.trim().is_empty() {
        return Err(WgslShaderModulePlanError::EmptySource);
    }

    Ok(())
}

pub fn BuildWgslShaderModulePlan(
    label: &str,
    source_name: &str,
    source: &str,
) -> Result<WgslShaderModulePlan, WgslShaderModulePlanError> {
    let plan = WgslShaderModulePlan {
        Label: label.to_string(),
        SourceName: source_name.to_string(),
        Source: source.to_string(),
    };
    ValidateWgslShaderModulePlan(&plan)?;
    Ok(plan)
}

pub fn BuildWgslPlanFromStrategyRequest(
    decision: &ShaderSourceStrategyDecision,
    request: &ShaderSourceStrategyRequest,
    label: &str,
    source_name: &str,
) -> Result<WgslShaderModulePlan, WgslShaderModulePlanError> {
    if decision.SelectedMode != ShaderSourceMode::Wgsl {
        return Err(WgslShaderModulePlanError::StrategyDidNotSelectWgsl);
    }

    let source = request
        .WgslSource
        .as_deref()
        .ok_or(WgslShaderModulePlanError::StrategyRequestMissingWgslSource)?;

    BuildWgslShaderModulePlan(label, source_name, source)
}

pub fn CreateWgpuShaderModuleFromWgsl(
    device: &wgpu::Device,
    plan: &WgslShaderModulePlan,
) -> Result<WgpuShaderModuleResource, WgslShaderModulePlanError> {
    ValidateWgslShaderModulePlan(plan)?;

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&plan.Label),
        source: wgpu::ShaderSource::Wgsl(plan.Source.clone().into()),
    });

    Ok(WgpuShaderModuleResource {
        Label: plan.Label.clone(),
        SourceName: plan.SourceName.clone(),
        Module: module,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::shader::{
        SelectShaderSourceStrategy, ShaderSourceStrategyConstraints, ShaderSourceStrategyRequest,
    };

    const TEST_WGSL_SOURCE: &str = r#"
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

    #[test]
    fn ValidWgslPlanBuildsSuccessfully() {
        let plan = BuildWgslShaderModulePlan(
            "SpritePipeline.WGSL",
            "sprite_pipeline.wgsl",
            TEST_WGSL_SOURCE,
        )
        .expect("valid WGSL source should build a module plan");

        assert_eq!(
            plan.Label, "SpritePipeline.WGSL",
            "label should be preserved"
        );
        assert_eq!(
            plan.SourceName, "sprite_pipeline.wgsl",
            "source name should be preserved"
        );
        assert_eq!(
            plan.Source, TEST_WGSL_SOURCE,
            "source text should be preserved"
        );
    }

    #[test]
    fn EmptyLabelRejected() {
        assert_eq!(
            BuildWgslShaderModulePlan("  \n\t", "sprite_pipeline.wgsl", TEST_WGSL_SOURCE)
                .unwrap_err(),
            WgslShaderModulePlanError::EmptyLabel,
            "empty or whitespace label should be rejected"
        );
    }

    #[test]
    fn EmptySourceNameRejected() {
        assert_eq!(
            BuildWgslShaderModulePlan("SpritePipeline.WGSL", "", TEST_WGSL_SOURCE).unwrap_err(),
            WgslShaderModulePlanError::EmptySourceName,
            "empty source name should be rejected"
        );
    }

    #[test]
    fn EmptyOrWhitespaceSourceRejected() {
        assert_eq!(
            BuildWgslShaderModulePlan("SpritePipeline.WGSL", "sprite_pipeline.wgsl", " \n\t")
                .unwrap_err(),
            WgslShaderModulePlanError::EmptySource,
            "whitespace-only WGSL source should be rejected"
        );
    }

    #[test]
    fn StrategySelectedWgslCanBuildPlan() {
        let request = ShaderSourceStrategyRequest {
            Label: "ShaderStrategyRequest".to_string(),
            SdslVSource: Some("shader S {}".to_string()),
            WgslSource: Some(TEST_WGSL_SOURCE.to_string()),
            HlslSource: None,
            Constraints: ShaderSourceStrategyConstraints {
                PreferWgsl: true,
                ..ShaderSourceStrategyConstraints::default()
            },
        };

        let decision = SelectShaderSourceStrategy(&request);
        assert_eq!(
            decision.SelectedMode,
            ShaderSourceMode::Wgsl,
            "prefer-WGSL should choose WGSL when source exists"
        );

        let plan = BuildWgslPlanFromStrategyRequest(
            &decision,
            &request,
            "SpritePipeline.FromStrategy",
            "sprite_pipeline.strategy.wgsl",
        )
        .expect("WGSL-selected strategy should build a WGSL module plan");

        assert_eq!(
            plan.Source, TEST_WGSL_SOURCE,
            "strategy-selected WGSL source should be used by WGSL module planning"
        );
    }

    #[test]
    fn StrategyHelperRejectsNonWgslSelection() {
        let request = ShaderSourceStrategyRequest {
            Label: "ShaderStrategyRequest".to_string(),
            SdslVSource: Some("shader S {}".to_string()),
            WgslSource: Some(TEST_WGSL_SOURCE.to_string()),
            HlslSource: None,
            Constraints: ShaderSourceStrategyConstraints::default(),
        };
        let decision = SelectShaderSourceStrategy(&request);

        assert_eq!(
            BuildWgslPlanFromStrategyRequest(
                &decision,
                &request,
                "SpritePipeline.FromStrategy",
                "sprite_pipeline.strategy.wgsl",
            )
            .unwrap_err(),
            WgslShaderModulePlanError::StrategyDidNotSelectWgsl,
            "helper should reject plans when strategy did not select WGSL"
        );
    }
}
