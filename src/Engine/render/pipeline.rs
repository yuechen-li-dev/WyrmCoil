#![allow(non_snake_case)]

use crate::Engine::shader::sdslv::{SdslvEntryPoint, SdslvShaderArtifact, SdslvShaderStage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderStagePlan {
    pub EntryPoint: String,
    pub TargetProfile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPipelinePlan {
    pub Name: String,
    pub SourceName: String,
    pub Hlsl: String,
    pub VertexEntry: ShaderStagePlan,
    pub PixelEntry: ShaderStagePlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderPipelinePlanError {
    MissingEntryPoint {
        Name: String,
    },
    WrongStage {
        Name: String,
        Expected: SdslvShaderStage,
        Found: SdslvShaderStage,
    },
    EmptyHlsl,
    DuplicateEntryPoint {
        Name: String,
    },
}

pub fn BuildRenderPipelinePlan(
    name: &str,
    artifact: &SdslvShaderArtifact,
    vertex_entry: &str,
    pixel_entry: &str,
) -> Result<RenderPipelinePlan, RenderPipelinePlanError> {
    if artifact.Hlsl.trim().is_empty() {
        return Err(RenderPipelinePlanError::EmptyHlsl);
    }

    let vertex = FindUniqueEntry(artifact, vertex_entry)?;
    if vertex.Stage != SdslvShaderStage::Vertex {
        return Err(RenderPipelinePlanError::WrongStage {
            Name: vertex_entry.to_string(),
            Expected: SdslvShaderStage::Vertex,
            Found: vertex.Stage,
        });
    }

    let pixel = FindUniqueEntry(artifact, pixel_entry)?;
    if pixel.Stage != SdslvShaderStage::Pixel {
        return Err(RenderPipelinePlanError::WrongStage {
            Name: pixel_entry.to_string(),
            Expected: SdslvShaderStage::Pixel,
            Found: pixel.Stage,
        });
    }

    Ok(RenderPipelinePlan {
        Name: name.to_string(),
        SourceName: artifact.SourceName.clone(),
        Hlsl: artifact.Hlsl.clone(),
        VertexEntry: ShaderStagePlan {
            EntryPoint: vertex.Name.clone(),
            TargetProfile: vertex.TargetProfile.clone(),
        },
        PixelEntry: ShaderStagePlan {
            EntryPoint: pixel.Name.clone(),
            TargetProfile: pixel.TargetProfile.clone(),
        },
    })
}

fn FindUniqueEntry<'a>(
    artifact: &'a SdslvShaderArtifact,
    entry_name: &str,
) -> Result<&'a SdslvEntryPoint, RenderPipelinePlanError> {
    let matches = artifact
        .EntryPoints
        .iter()
        .filter(|entry| entry.Name == entry_name)
        .collect::<Vec<&SdslvEntryPoint>>();

    if matches.is_empty() {
        return Err(RenderPipelinePlanError::MissingEntryPoint {
            Name: entry_name.to_string(),
        });
    }

    if matches.len() > 1 {
        return Err(RenderPipelinePlanError::DuplicateEntryPoint {
            Name: entry_name.to_string(),
        });
    }

    Ok(matches[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::shader::sdslv::CompileSourceToShaderArtifact;

    #[test]
    fn BuildRenderPipelinePlanValidFlatShader() {
        let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
        let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
            .expect("flat color artifact should compile");

        let plan =
            BuildRenderPipelinePlan("FlatColorPlan", &artifact, "FlatColor_VS", "FlatColor_PS")
                .expect("valid vertex/pixel entries should produce a plan");

        assert_eq!(plan.Name, "FlatColorPlan", "plan name should be preserved");
        assert_eq!(
            plan.SourceName, "flat_color.sdslv",
            "artifact source name should be preserved"
        );
        assert_eq!(
            plan.VertexEntry.EntryPoint, "FlatColor_VS",
            "vertex entry point should match requested entry"
        );
        assert_eq!(
            plan.VertexEntry.TargetProfile, "vs_6_0",
            "vertex target should map from artifact metadata"
        );
        assert_eq!(
            plan.PixelEntry.EntryPoint, "FlatColor_PS",
            "pixel entry point should match requested entry"
        );
        assert_eq!(
            plan.PixelEntry.TargetProfile, "ps_6_0",
            "pixel target should map from artifact metadata"
        );
        assert!(
            plan.Hlsl.contains("FlatColor_PS"),
            "plan should carry emitted HLSL text"
        );
    }

    #[test]
    fn BuildRenderPipelinePlanMissingVertexAndPixelEntries() {
        let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
        let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
            .expect("flat color artifact should compile");

        let missing_vertex =
            BuildRenderPipelinePlan("MissingVertex", &artifact, "Missing_VS", "FlatColor_PS")
                .unwrap_err();
        assert_eq!(
            missing_vertex,
            RenderPipelinePlanError::MissingEntryPoint {
                Name: "Missing_VS".to_string()
            },
            "missing vertex entry should return MissingEntryPoint"
        );

        let missing_pixel =
            BuildRenderPipelinePlan("MissingPixel", &artifact, "FlatColor_VS", "Missing_PS")
                .unwrap_err();
        assert_eq!(
            missing_pixel,
            RenderPipelinePlanError::MissingEntryPoint {
                Name: "Missing_PS".to_string()
            },
            "missing pixel entry should return MissingEntryPoint"
        );
    }

    #[test]
    fn BuildRenderPipelinePlanWrongStageErrors() {
        let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
        let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
            .expect("flat color artifact should compile");

        let vertex_is_pixel = BuildRenderPipelinePlan(
            "WrongVertexStage",
            &artifact,
            "FlatColor_PS",
            "FlatColor_PS",
        )
        .unwrap_err();
        assert_eq!(
            vertex_is_pixel,
            RenderPipelinePlanError::WrongStage {
                Name: "FlatColor_PS".to_string(),
                Expected: SdslvShaderStage::Vertex,
                Found: SdslvShaderStage::Pixel,
            },
            "pixel used as vertex should return WrongStage"
        );

        let pixel_is_vertex =
            BuildRenderPipelinePlan("WrongPixelStage", &artifact, "FlatColor_VS", "FlatColor_VS")
                .unwrap_err();
        assert_eq!(
            pixel_is_vertex,
            RenderPipelinePlanError::WrongStage {
                Name: "FlatColor_VS".to_string(),
                Expected: SdslvShaderStage::Pixel,
                Found: SdslvShaderStage::Vertex,
            },
            "vertex used as pixel should return WrongStage"
        );
    }

    #[test]
    fn BuildRenderPipelinePlanRejectsHelpersAsEntryPoints() {
        let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            fn BaseColor(input: VertexOut) -> float4 { return input.Color; }
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return FlatColor_BaseColor(input);
            }
        }
    "#;
        let artifact = CompileSourceToShaderArtifact("flat_with_helper.sdslv", src)
            .expect("generic fixture artifact should compile");

        let helper_error = BuildRenderPipelinePlan(
            "HelperRejected",
            &artifact,
            "FlatColor_VS",
            "FlatMaterial_BaseColor",
        )
        .unwrap_err();
        assert_eq!(
            helper_error,
            RenderPipelinePlanError::MissingEntryPoint {
                Name: "FlatMaterial_BaseColor".to_string()
            },
            "helper method names should not resolve as entry points"
        );

        let flow_src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
        flow PickMode(useSoft: bool, quality: i32) -> i32 {
            state Select {
                when {
                    case useSoft -> return 2
                    else -> return quality
                }
            }
        }
    "#;
        let flow_artifact = CompileSourceToShaderArtifact("flow_value_lowering.sdslv", flow_src)
            .expect("flow fixture artifact should compile");

        let flow_error =
            BuildRenderPipelinePlan("FlowRejected", &flow_artifact, "FlatColor_VS", "PickMode")
                .unwrap_err();
        assert_eq!(
            flow_error,
            RenderPipelinePlanError::MissingEntryPoint {
                Name: "PickMode".to_string()
            },
            "flow helper names should not resolve as entry points"
        );
    }

    #[test]
    fn BuildRenderPipelinePlanSupportsCompileAliasEntry() {
        let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        interface IBaseColor { fn BaseColor(input: VertexOut) -> float4; }
        shader FlatMaterial implements IBaseColor {
            material { Color: float4; }
            override fn BaseColor(input: VertexOut) -> float4 { return Color; }
        }
        shader ForwardPass<TMat> where TMat : IBaseColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut, mat: TMat) -> float4 {
                return mat.BaseColor(input);
            }
        }
        compile ForwardPass<FlatMaterial> as ForwardFlatMaterial;
    "#;
        let artifact = CompileSourceToShaderArtifact("generic_forward_pass.sdslv", src)
            .expect("generic fixture artifact should compile");

        let plan = BuildRenderPipelinePlan(
            "ForwardFlatPlan",
            &artifact,
            "ForwardFlatMaterial_VS",
            "ForwardFlatMaterial_PS",
        )
        .expect("compile alias vertex+pixel should build a plan");

        assert_eq!(
            plan.PixelEntry.EntryPoint, "ForwardFlatMaterial_PS",
            "compile alias pixel entry should be accepted"
        );
        assert_eq!(
            plan.VertexEntry.EntryPoint, "ForwardFlatMaterial_VS",
            "compile alias vertex entry should be accepted"
        );

        let generic_template = BuildRenderPipelinePlan(
            "RejectTemplate",
            &artifact,
            "ForwardPass_VS",
            "ForwardFlatMaterial_PS",
        )
        .unwrap_err();
        assert_eq!(
            generic_template,
            RenderPipelinePlanError::MissingEntryPoint {
                Name: "ForwardPass_VS".to_string()
            },
            "generic template entry should not resolve from artifact metadata"
        );
    }

    #[test]
    fn BuildRenderPipelinePlanIsDeterministic() {
        let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
        let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
            .expect("flat color artifact should compile");

        let a = BuildRenderPipelinePlan("FlatPlan", &artifact, "FlatColor_VS", "FlatColor_PS")
            .expect("first plan build should succeed");
        let b = BuildRenderPipelinePlan("FlatPlan", &artifact, "FlatColor_VS", "FlatColor_PS")
            .expect("second plan build should succeed");

        assert_eq!(
            a, b,
            "pipeline plans should be deterministic for same input"
        );
    }

    #[test]
    fn BuildRenderPipelinePlanRejectsEmptyHlslAndDuplicateEntries() {
        let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
        let mut artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
            .expect("flat color artifact should compile");

        artifact.Hlsl.clear();
        let empty = BuildRenderPipelinePlan("Empty", &artifact, "FlatColor_VS", "FlatColor_PS")
            .unwrap_err();
        assert_eq!(
            empty,
            RenderPipelinePlanError::EmptyHlsl,
            "empty HLSL should return EmptyHlsl error"
        );

        let mut duplicate = CompileSourceToShaderArtifact("flat_color.sdslv", src)
            .expect("flat color artifact should compile");
        let duplicate_entry = duplicate.EntryPoints[0].clone();
        duplicate.EntryPoints.push(duplicate_entry);

        let duplicate_error =
            BuildRenderPipelinePlan("Duplicate", &duplicate, "FlatColor_VS", "FlatColor_PS")
                .unwrap_err();
        assert_eq!(
            duplicate_error,
            RenderPipelinePlanError::DuplicateEntryPoint {
                Name: "FlatColor_VS".to_string()
            },
            "duplicate metadata names should produce structured duplicate error"
        );
    }
}
