#![allow(non_snake_case)]

use crate::Engine::shader::sdslv::{
    CompileHlslWithDxc, DxcCompileRequest, DxcCompileResult, DxcError, DxcOptions, SdslvEntryPoint,
    SdslvShaderArtifact, SdslvShaderStage,
};

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
pub struct PipelineDxcRequests {
    pub Vertex: DxcCompileRequest,
    pub Pixel: DxcCompileRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPipelineShaders {
    pub Vertex: DxcCompileResult,
    pub Pixel: DxcCompileResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledShaderModuleDesc {
    pub EntryPoint: String,
    pub TargetProfile: String,
    pub SpirvBytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPipelineDesc {
    pub Name: String,
    pub SourceName: String,
    pub Vertex: CompiledShaderModuleDesc,
    pub Pixel: CompiledShaderModuleDesc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
    Uint32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexAttributeDesc {
    pub Name: String,
    pub Location: u32,
    pub Format: VertexFormat,
    pub OffsetBytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexStepMode {
    Vertex,
    Instance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexBufferLayoutDesc {
    pub StrideBytes: u64,
    pub StepMode: VertexStepMode,
    pub Attributes: Vec<VertexAttributeDesc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTargetFormat {
    Bgra8UnormSrgb,
    Rgba8UnormSrgb,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorTargetDesc {
    pub Format: ColorTargetFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthFormat {
    Depth24Plus,
    Depth32Float,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthStencilDesc {
    pub Format: DepthFormat,
    pub DepthWriteEnabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPipelineLayoutOptions {
    pub Name: String,
    pub VertexBuffers: Vec<VertexBufferLayoutDesc>,
    pub ColorTarget: ColorTargetDesc,
    pub Depth: Option<DepthStencilDesc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPipelineLayoutPlan {
    pub Name: String,
    pub Shaders: CompiledPipelineDesc,
    pub VertexBuffers: Vec<VertexBufferLayoutDesc>,
    pub ColorTarget: ColorTargetDesc,
    pub Depth: Option<DepthStencilDesc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderPipelineLayoutPlanError {
    EmptyName,
    MissingVertexBuffers,
    EmptyVertexBufferAttributes {
        BufferIndex: usize,
    },
    DuplicateAttributeLocation {
        Location: u32,
    },
    DuplicateAttributeName {
        Name: String,
    },
    AttributeOffsetOutOfBounds {
        Name: String,
        OffsetBytes: u64,
        StrideBytes: u64,
    },
    ZeroStride {
        BufferIndex: usize,
    },
    MissingShaderBytes,
}

pub fn BuildRenderPipelineLayoutPlan(
    compiled: CompiledPipelineDesc,
    options: RenderPipelineLayoutOptions,
) -> Result<RenderPipelineLayoutPlan, RenderPipelineLayoutPlanError> {
    if options.Name.trim().is_empty() {
        return Err(RenderPipelineLayoutPlanError::EmptyName);
    }
    if options.VertexBuffers.is_empty() {
        return Err(RenderPipelineLayoutPlanError::MissingVertexBuffers);
    }
    if compiled.Vertex.SpirvBytes.is_empty() || compiled.Pixel.SpirvBytes.is_empty() {
        return Err(RenderPipelineLayoutPlanError::MissingShaderBytes);
    }

    let mut seen_locations = std::collections::BTreeSet::new();
    let mut seen_names = std::collections::BTreeSet::new();
    for (buffer_index, buffer) in options.VertexBuffers.iter().enumerate() {
        if buffer.StrideBytes == 0 {
            return Err(RenderPipelineLayoutPlanError::ZeroStride {
                BufferIndex: buffer_index,
            });
        }
        if buffer.Attributes.is_empty() {
            return Err(RenderPipelineLayoutPlanError::EmptyVertexBufferAttributes {
                BufferIndex: buffer_index,
            });
        }
        for attribute in &buffer.Attributes {
            if attribute.OffsetBytes >= buffer.StrideBytes {
                return Err(RenderPipelineLayoutPlanError::AttributeOffsetOutOfBounds {
                    Name: attribute.Name.clone(),
                    OffsetBytes: attribute.OffsetBytes,
                    StrideBytes: buffer.StrideBytes,
                });
            }
            if !seen_locations.insert(attribute.Location) {
                return Err(RenderPipelineLayoutPlanError::DuplicateAttributeLocation {
                    Location: attribute.Location,
                });
            }
            if !seen_names.insert(attribute.Name.clone()) {
                return Err(RenderPipelineLayoutPlanError::DuplicateAttributeName {
                    Name: attribute.Name.clone(),
                });
            }
        }
    }

    Ok(RenderPipelineLayoutPlan {
        Name: options.Name,
        Shaders: compiled,
        VertexBuffers: options.VertexBuffers,
        ColorTarget: options.ColorTarget,
        Depth: options.Depth,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilePipelineShadersError {
    Request(PipelineDxcRequestError),
    Vertex(DxcError),
    Pixel(DxcError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineDxcRequestError {
    EmptyHlsl,
    EmptyEntryPoint { Stage: SdslvShaderStage },
    EmptyTargetProfile { Stage: SdslvShaderStage },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledPipelineDescError {
    EmptyVertexBytes,
    EmptyPixelBytes,
    VertexEntryPointMismatch { Expected: String, Found: String },
    PixelEntryPointMismatch { Expected: String, Found: String },
    VertexTargetProfileMismatch { Expected: String, Found: String },
    PixelTargetProfileMismatch { Expected: String, Found: String },
}

pub fn BuildCompiledPipelineDesc(
    plan: &RenderPipelinePlan,
    compiled: &CompiledPipelineShaders,
) -> Result<CompiledPipelineDesc, CompiledPipelineDescError> {
    if compiled.Vertex.OutputBytes.is_empty() {
        return Err(CompiledPipelineDescError::EmptyVertexBytes);
    }
    if compiled.Pixel.OutputBytes.is_empty() {
        return Err(CompiledPipelineDescError::EmptyPixelBytes);
    }
    if compiled.Vertex.EntryPoint != plan.VertexEntry.EntryPoint {
        return Err(CompiledPipelineDescError::VertexEntryPointMismatch {
            Expected: plan.VertexEntry.EntryPoint.clone(),
            Found: compiled.Vertex.EntryPoint.clone(),
        });
    }
    if compiled.Pixel.EntryPoint != plan.PixelEntry.EntryPoint {
        return Err(CompiledPipelineDescError::PixelEntryPointMismatch {
            Expected: plan.PixelEntry.EntryPoint.clone(),
            Found: compiled.Pixel.EntryPoint.clone(),
        });
    }
    if compiled.Vertex.TargetProfile != plan.VertexEntry.TargetProfile {
        return Err(CompiledPipelineDescError::VertexTargetProfileMismatch {
            Expected: plan.VertexEntry.TargetProfile.clone(),
            Found: compiled.Vertex.TargetProfile.clone(),
        });
    }
    if compiled.Pixel.TargetProfile != plan.PixelEntry.TargetProfile {
        return Err(CompiledPipelineDescError::PixelTargetProfileMismatch {
            Expected: plan.PixelEntry.TargetProfile.clone(),
            Found: compiled.Pixel.TargetProfile.clone(),
        });
    }

    Ok(CompiledPipelineDesc {
        Name: plan.Name.clone(),
        SourceName: plan.SourceName.clone(),
        Vertex: CompiledShaderModuleDesc {
            EntryPoint: compiled.Vertex.EntryPoint.clone(),
            TargetProfile: compiled.Vertex.TargetProfile.clone(),
            SpirvBytes: compiled.Vertex.OutputBytes.clone(),
        },
        Pixel: CompiledShaderModuleDesc {
            EntryPoint: compiled.Pixel.EntryPoint.clone(),
            TargetProfile: compiled.Pixel.TargetProfile.clone(),
            SpirvBytes: compiled.Pixel.OutputBytes.clone(),
        },
    })
}

pub fn BuildDxcRequestsForPipelinePlan(
    plan: &RenderPipelinePlan,
) -> Result<PipelineDxcRequests, PipelineDxcRequestError> {
    if plan.Hlsl.trim().is_empty() {
        return Err(PipelineDxcRequestError::EmptyHlsl);
    }
    if plan.VertexEntry.EntryPoint.trim().is_empty() {
        return Err(PipelineDxcRequestError::EmptyEntryPoint {
            Stage: SdslvShaderStage::Vertex,
        });
    }
    if plan.PixelEntry.EntryPoint.trim().is_empty() {
        return Err(PipelineDxcRequestError::EmptyEntryPoint {
            Stage: SdslvShaderStage::Pixel,
        });
    }
    if plan.VertexEntry.TargetProfile.trim().is_empty() {
        return Err(PipelineDxcRequestError::EmptyTargetProfile {
            Stage: SdslvShaderStage::Vertex,
        });
    }
    if plan.PixelEntry.TargetProfile.trim().is_empty() {
        return Err(PipelineDxcRequestError::EmptyTargetProfile {
            Stage: SdslvShaderStage::Pixel,
        });
    }

    Ok(PipelineDxcRequests {
        Vertex: DxcCompileRequest {
            SourceName: plan.SourceName.clone(),
            Hlsl: plan.Hlsl.clone(),
            EntryPoint: plan.VertexEntry.EntryPoint.clone(),
            TargetProfile: plan.VertexEntry.TargetProfile.clone(),
        },
        Pixel: DxcCompileRequest {
            SourceName: plan.SourceName.clone(),
            Hlsl: plan.Hlsl.clone(),
            EntryPoint: plan.PixelEntry.EntryPoint.clone(),
            TargetProfile: plan.PixelEntry.TargetProfile.clone(),
        },
    })
}

pub fn CompilePipelineShadersWithDxc(
    plan: &RenderPipelinePlan,
    options: &DxcOptions,
) -> Result<CompiledPipelineShaders, CompilePipelineShadersError> {
    let requests =
        BuildDxcRequestsForPipelinePlan(plan).map_err(CompilePipelineShadersError::Request)?;

    let vertex = CompileHlslWithDxc(&requests.Vertex, options)
        .map_err(CompilePipelineShadersError::Vertex)?;
    let pixel =
        CompileHlslWithDxc(&requests.Pixel, options).map_err(CompilePipelineShadersError::Pixel)?;

    Ok(CompiledPipelineShaders {
        Vertex: vertex,
        Pixel: pixel,
    })
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

    #[test]
    fn BuildDxcRequestsForPipelinePlanValidConversionAndCommandCompatibility() {
        use crate::Engine::shader::sdslv::{BuildDxcCommand, DxcOptions};

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
                .expect("valid plan should build for flat shader");

        let requests = BuildDxcRequestsForPipelinePlan(&plan)
            .expect("valid plan should convert into deterministic vertex/pixel DXC requests");

        assert_eq!(
            requests.Vertex.TargetProfile, "vs_6_0",
            "vertex target profile should be preserved"
        );
        assert_eq!(
            requests.Pixel.TargetProfile, "ps_6_0",
            "pixel target profile should be preserved"
        );
        assert_eq!(
            requests.Vertex.SourceName, plan.SourceName,
            "vertex request should preserve source name"
        );
        assert_eq!(
            requests.Pixel.SourceName, plan.SourceName,
            "pixel request should preserve source name"
        );
        assert_eq!(
            requests.Vertex.EntryPoint, plan.VertexEntry.EntryPoint,
            "vertex request should preserve entry point"
        );
        assert_eq!(
            requests.Pixel.EntryPoint, plan.PixelEntry.EntryPoint,
            "pixel request should preserve entry point"
        );
        assert_eq!(
            requests.Vertex.Hlsl, plan.Hlsl,
            "vertex request should preserve plan HLSL payload"
        );
        assert_eq!(
            requests.Pixel.Hlsl, plan.Hlsl,
            "pixel request should preserve plan HLSL payload"
        );
        assert_eq!(
            requests.Vertex.Hlsl, requests.Pixel.Hlsl,
            "vertex and pixel requests should share identical HLSL text"
        );

        let options = DxcOptions::default();
        let vertex_command = BuildDxcCommand(&requests.Vertex, &options).join(" ");
        assert!(
            vertex_command.contains("-E FlatColor_VS"),
            "vertex DXC command should contain the vertex entry point"
        );
        assert!(
            vertex_command.contains("-T vs_6_0"),
            "vertex DXC command should contain the vertex target profile"
        );

        let pixel_command = BuildDxcCommand(&requests.Pixel, &options).join(" ");
        assert!(
            pixel_command.contains("-E FlatColor_PS"),
            "pixel DXC command should contain the pixel entry point"
        );
        assert!(
            pixel_command.contains("-T ps_6_0"),
            "pixel DXC command should contain the pixel target profile"
        );
    }

    #[test]
    fn BuildDxcRequestsForPipelinePlanSupportsCompileAliasEntry() {
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
        .expect("compile alias plan should build");

        let requests = BuildDxcRequestsForPipelinePlan(&plan)
            .expect("compile alias plan should convert to DXC requests");
        assert_eq!(
            requests.Pixel.EntryPoint, "ForwardFlatMaterial_PS",
            "compile alias pixel entry should be preserved in DXC request"
        );
    }

    #[test]
    fn BuildDxcRequestsForPipelinePlanStructuredErrorsForEmptyFields() {
        let mut plan = RenderPipelinePlan {
            Name: "InvalidPlan".to_string(),
            SourceName: "invalid.sdslv".to_string(),
            Hlsl: "float4 main() : SV_Target { return 0.0.xxxx; }".to_string(),
            VertexEntry: ShaderStagePlan {
                EntryPoint: "VertexEntry".to_string(),
                TargetProfile: "vs_6_0".to_string(),
            },
            PixelEntry: ShaderStagePlan {
                EntryPoint: "PixelEntry".to_string(),
                TargetProfile: "ps_6_0".to_string(),
            },
        };

        plan.Hlsl.clear();
        assert_eq!(
            BuildDxcRequestsForPipelinePlan(&plan).unwrap_err(),
            PipelineDxcRequestError::EmptyHlsl,
            "empty HLSL should return a structured empty-hlsl error"
        );

        plan.Hlsl = "float4 main() : SV_Target { return 0.0.xxxx; }".to_string();
        plan.VertexEntry.EntryPoint.clear();
        assert_eq!(
            BuildDxcRequestsForPipelinePlan(&plan).unwrap_err(),
            PipelineDxcRequestError::EmptyEntryPoint {
                Stage: SdslvShaderStage::Vertex
            },
            "empty vertex entry point should return structured stage-specific error"
        );

        plan.VertexEntry.EntryPoint = "VertexEntry".to_string();
        plan.PixelEntry.TargetProfile.clear();
        assert_eq!(
            BuildDxcRequestsForPipelinePlan(&plan).unwrap_err(),
            PipelineDxcRequestError::EmptyTargetProfile {
                Stage: SdslvShaderStage::Pixel
            },
            "empty pixel target profile should return structured stage-specific error"
        );
    }

    #[test]
    fn CompilePipelineShadersWithDxcReturnsVertexToolUnavailableForValidPlan() {
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
                .expect("valid plan should build");
        let options = DxcOptions {
            DxcPath: "wyrmcoil_missing_dxc_m18".to_string(),
            OutputSpirv: true,
            ExtraArgs: Vec::new(),
        };

        let error = CompilePipelineShadersWithDxc(&plan, &options).unwrap_err();
        assert_eq!(
            error,
            CompilePipelineShadersError::Vertex(DxcError::ToolUnavailable {
                Path: "wyrmcoil_missing_dxc_m18".to_string()
            }),
            "valid plans should reach vertex compile and return structured vertex tool-unavailable errors"
        );
    }

    #[test]
    fn CompilePipelineShadersWithDxcPropagatesRequestConstructionErrors() {
        let plan = RenderPipelinePlan {
            Name: "InvalidPlan".to_string(),
            SourceName: "invalid.sdslv".to_string(),
            Hlsl: String::new(),
            VertexEntry: ShaderStagePlan {
                EntryPoint: "VertexEntry".to_string(),
                TargetProfile: "vs_6_0".to_string(),
            },
            PixelEntry: ShaderStagePlan {
                EntryPoint: "PixelEntry".to_string(),
                TargetProfile: "ps_6_0".to_string(),
            },
        };
        let options = DxcOptions {
            DxcPath: "wyrmcoil_missing_dxc_m18".to_string(),
            OutputSpirv: true,
            ExtraArgs: Vec::new(),
        };

        let error = CompilePipelineShadersWithDxc(&plan, &options).unwrap_err();
        assert_eq!(
            error,
            CompilePipelineShadersError::Request(PipelineDxcRequestError::EmptyHlsl),
            "request-construction errors should be wrapped without invoking DXC"
        );
    }

    fn BuildFakeCompileResult(entry: &str, target: &str, bytes: &[u8]) -> DxcCompileResult {
        DxcCompileResult {
            Success: true,
            EntryPoint: entry.to_string(),
            TargetProfile: target.to_string(),
            Stdout: String::new(),
            Stderr: String::new(),
            OutputBytes: bytes.to_vec(),
        }
    }

    #[test]
    fn BuildCompiledPipelineDescValidPreservesDeterministicFields() {
        let plan = RenderPipelinePlan {
            Name: "FlatColorPlan".to_string(),
            SourceName: "flat_color.sdslv".to_string(),
            Hlsl: "float4 main() : SV_Target { return 1.0; }".to_string(),
            VertexEntry: ShaderStagePlan {
                EntryPoint: "FlatColor_VS".to_string(),
                TargetProfile: "vs_6_0".to_string(),
            },
            PixelEntry: ShaderStagePlan {
                EntryPoint: "FlatColor_PS".to_string(),
                TargetProfile: "ps_6_0".to_string(),
            },
        };
        let compiled = CompiledPipelineShaders {
            Vertex: BuildFakeCompileResult("FlatColor_VS", "vs_6_0", &[1, 2, 3, 4]),
            Pixel: BuildFakeCompileResult("FlatColor_PS", "ps_6_0", &[5, 6, 7, 8]),
        };

        let desc = BuildCompiledPipelineDesc(&plan, &compiled)
            .expect("valid compiled results should build descriptor data without GPU");
        assert_eq!(desc.Name, "FlatColorPlan", "plan name should be preserved");
        assert_eq!(
            desc.SourceName, "flat_color.sdslv",
            "plan source name should be preserved"
        );
        assert_eq!(
            desc.Vertex.EntryPoint, "FlatColor_VS",
            "vertex entry point should be preserved"
        );
        assert_eq!(
            desc.Pixel.EntryPoint, "FlatColor_PS",
            "pixel entry point should be preserved"
        );
        assert_eq!(
            desc.Vertex.TargetProfile, "vs_6_0",
            "vertex target profile should be preserved"
        );
        assert_eq!(
            desc.Pixel.TargetProfile, "ps_6_0",
            "pixel target profile should be preserved"
        );
        assert_eq!(
            desc.Vertex.SpirvBytes,
            vec![1, 2, 3, 4],
            "vertex shader bytes should be preserved"
        );
        assert_eq!(
            desc.Pixel.SpirvBytes,
            vec![5, 6, 7, 8],
            "pixel shader bytes should be preserved"
        );
    }

    #[test]
    fn BuildCompiledPipelineDescRejectsEmptyBytes() {
        let plan = RenderPipelinePlan {
            Name: "Plan".to_string(),
            SourceName: "source.sdslv".to_string(),
            Hlsl: "hlsl".to_string(),
            VertexEntry: ShaderStagePlan {
                EntryPoint: "V".to_string(),
                TargetProfile: "vs_6_0".to_string(),
            },
            PixelEntry: ShaderStagePlan {
                EntryPoint: "P".to_string(),
                TargetProfile: "ps_6_0".to_string(),
            },
        };

        let empty_vertex = CompiledPipelineShaders {
            Vertex: BuildFakeCompileResult("V", "vs_6_0", &[]),
            Pixel: BuildFakeCompileResult("P", "ps_6_0", &[1]),
        };
        assert_eq!(
            BuildCompiledPipelineDesc(&plan, &empty_vertex).unwrap_err(),
            CompiledPipelineDescError::EmptyVertexBytes,
            "empty vertex bytes should be rejected"
        );

        let empty_pixel = CompiledPipelineShaders {
            Vertex: BuildFakeCompileResult("V", "vs_6_0", &[1]),
            Pixel: BuildFakeCompileResult("P", "ps_6_0", &[]),
        };
        assert_eq!(
            BuildCompiledPipelineDesc(&plan, &empty_pixel).unwrap_err(),
            CompiledPipelineDescError::EmptyPixelBytes,
            "empty pixel bytes should be rejected"
        );
    }

    #[test]
    fn BuildCompiledPipelineDescRejectsEntryAndTargetMismatches() {
        let plan = RenderPipelinePlan {
            Name: "Plan".to_string(),
            SourceName: "source.sdslv".to_string(),
            Hlsl: "hlsl".to_string(),
            VertexEntry: ShaderStagePlan {
                EntryPoint: "ExpectedVS".to_string(),
                TargetProfile: "vs_6_0".to_string(),
            },
            PixelEntry: ShaderStagePlan {
                EntryPoint: "ExpectedPS".to_string(),
                TargetProfile: "ps_6_0".to_string(),
            },
        };

        let wrong_vertex_entry = CompiledPipelineShaders {
            Vertex: BuildFakeCompileResult("WrongVS", "vs_6_0", &[1]),
            Pixel: BuildFakeCompileResult("ExpectedPS", "ps_6_0", &[1]),
        };
        assert_eq!(
            BuildCompiledPipelineDesc(&plan, &wrong_vertex_entry).unwrap_err(),
            CompiledPipelineDescError::VertexEntryPointMismatch {
                Expected: "ExpectedVS".to_string(),
                Found: "WrongVS".to_string()
            },
            "vertex entry mismatch should be rejected"
        );

        let wrong_pixel_target = CompiledPipelineShaders {
            Vertex: BuildFakeCompileResult("ExpectedVS", "vs_6_0", &[1]),
            Pixel: BuildFakeCompileResult("ExpectedPS", "ps_6_7", &[1]),
        };
        assert_eq!(
            BuildCompiledPipelineDesc(&plan, &wrong_pixel_target).unwrap_err(),
            CompiledPipelineDescError::PixelTargetProfileMismatch {
                Expected: "ps_6_0".to_string(),
                Found: "ps_6_7".to_string()
            },
            "pixel target mismatch should be rejected"
        );
    }
}

#[cfg(test)]
mod m20_layout_tests {
    use super::*;

    fn BuildFakeCompiledPipelineDesc() -> CompiledPipelineDesc {
        CompiledPipelineDesc {
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
        }
    }

    fn BuildValidOptions() -> RenderPipelineLayoutOptions {
        RenderPipelineLayoutOptions {
            Name: "SpriteLayout".to_string(),
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
                Format: ColorTargetFormat::Bgra8UnormSrgb,
            },
            Depth: None,
        }
    }

    #[test]
    fn BuildRenderPipelineLayoutPlanValidPlanPreservesFields() {
        let compiled = BuildFakeCompiledPipelineDesc();
        let options = BuildValidOptions();

        let plan = BuildRenderPipelineLayoutPlan(compiled.clone(), options.clone())
            .expect("valid inputs should build a deterministic plain-data layout plan");

        assert_eq!(plan.Name, "SpriteLayout", "name should be preserved");
        assert_eq!(
            plan.Shaders, compiled,
            "compiled shader metadata should be preserved"
        );
        assert_eq!(
            plan.VertexBuffers, options.VertexBuffers,
            "vertex buffer layouts should be preserved"
        );
        assert_eq!(
            plan.ColorTarget, options.ColorTarget,
            "color target metadata should be preserved"
        );
        assert_eq!(
            plan.Depth, None,
            "missing depth metadata should remain None"
        );
    }

    #[test]
    fn BuildRenderPipelineLayoutPlanValidationErrors() {
        let compiled = BuildFakeCompiledPipelineDesc();

        let mut empty_name = BuildValidOptions();
        empty_name.Name = "   ".to_string();
        assert_eq!(
            BuildRenderPipelineLayoutPlan(compiled.clone(), empty_name).unwrap_err(),
            RenderPipelineLayoutPlanError::EmptyName,
            "empty plan names should be rejected"
        );

        let mut missing_buffers = BuildValidOptions();
        missing_buffers.VertexBuffers.clear();
        assert_eq!(
            BuildRenderPipelineLayoutPlan(compiled.clone(), missing_buffers).unwrap_err(),
            RenderPipelineLayoutPlanError::MissingVertexBuffers,
            "missing vertex buffers should be rejected"
        );

        let mut zero_stride = BuildValidOptions();
        zero_stride.VertexBuffers[0].StrideBytes = 0;
        assert_eq!(
            BuildRenderPipelineLayoutPlan(compiled.clone(), zero_stride).unwrap_err(),
            RenderPipelineLayoutPlanError::ZeroStride { BufferIndex: 0 },
            "zero vertex stride should be rejected"
        );

        let mut empty_attrs = BuildValidOptions();
        empty_attrs.VertexBuffers[0].Attributes.clear();
        assert_eq!(
            BuildRenderPipelineLayoutPlan(compiled.clone(), empty_attrs).unwrap_err(),
            RenderPipelineLayoutPlanError::EmptyVertexBufferAttributes { BufferIndex: 0 },
            "empty attribute lists should be rejected"
        );

        let mut dup_location = BuildValidOptions();
        dup_location.VertexBuffers.push(VertexBufferLayoutDesc {
            StrideBytes: 4,
            StepMode: VertexStepMode::Vertex,
            Attributes: vec![VertexAttributeDesc {
                Name: "ExtraAttr".to_string(),
                Location: 1,
                Format: VertexFormat::Uint32,
                OffsetBytes: 0,
            }],
        });
        assert_eq!(
            BuildRenderPipelineLayoutPlan(compiled.clone(), dup_location).unwrap_err(),
            RenderPipelineLayoutPlanError::DuplicateAttributeLocation { Location: 1 },
            "duplicate locations across buffers should be rejected"
        );

        let mut dup_name = BuildValidOptions();
        dup_name.VertexBuffers.push(VertexBufferLayoutDesc {
            StrideBytes: 4,
            StepMode: VertexStepMode::Vertex,
            Attributes: vec![VertexAttributeDesc {
                Name: "Position".to_string(),
                Location: 4,
                Format: VertexFormat::Uint32,
                OffsetBytes: 0,
            }],
        });
        assert_eq!(
            BuildRenderPipelineLayoutPlan(compiled.clone(), dup_name).unwrap_err(),
            RenderPipelineLayoutPlanError::DuplicateAttributeName {
                Name: "Position".to_string()
            },
            "duplicate names across buffers should be rejected"
        );

        let mut bad_offset = BuildValidOptions();
        bad_offset.VertexBuffers[0].Attributes[1].OffsetBytes = 12;
        assert_eq!(
            BuildRenderPipelineLayoutPlan(compiled.clone(), bad_offset).unwrap_err(),
            RenderPipelineLayoutPlanError::AttributeOffsetOutOfBounds {
                Name: "SpriteId".to_string(),
                OffsetBytes: 12,
                StrideBytes: 12,
            },
            "attribute offsets must remain below stride"
        );

        let mut empty_bytes = BuildFakeCompiledPipelineDesc();
        empty_bytes.Vertex.SpirvBytes.clear();
        assert_eq!(
            BuildRenderPipelineLayoutPlan(empty_bytes, BuildValidOptions()).unwrap_err(),
            RenderPipelineLayoutPlanError::MissingShaderBytes,
            "compiled descriptors with missing bytes should be rejected"
        );
    }

    #[test]
    fn BuildRenderPipelineLayoutPlanOptionalDepthAndDeterminism() {
        let compiled = BuildFakeCompiledPipelineDesc();
        let mut with_depth = BuildValidOptions();
        with_depth.Depth = Some(DepthStencilDesc {
            Format: DepthFormat::Depth24Plus,
            DepthWriteEnabled: true,
        });

        let a = BuildRenderPipelineLayoutPlan(compiled.clone(), with_depth.clone())
            .expect("depth-enabled plan should be accepted");
        let b = BuildRenderPipelineLayoutPlan(compiled, with_depth)
            .expect("same depth-enabled inputs should produce equivalent outputs");

        assert_eq!(a, b, "layout planning should be deterministic");
        assert_eq!(
            a.Depth,
            Some(DepthStencilDesc {
                Format: DepthFormat::Depth24Plus,
                DepthWriteEnabled: true,
            }),
            "depth metadata should be preserved"
        );
    }
}
