#![allow(non_snake_case)]

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::emitter::EmitHlsl;
use super::parser::ParseSource;
use super::validation::ValidateModule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvShaderArtifact {
    pub SourceName: String,
    pub Hlsl: String,
    pub EntryPoints: Vec<SdslvEntryPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvEntryPoint {
    pub Name: String,
    pub Stage: SdslvShaderStage,
    pub ShaderName: String,
    pub MethodName: String,
    pub TargetProfile: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdslvShaderStage {
    Vertex,
    Pixel,
    Compute,
}

pub fn CompileSourceToShaderArtifact(
    source_name: &str,
    source: &str,
) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>> {
    let module = ParseSource(source)?;
    BuildShaderArtifact(source_name, &module)
}

pub fn BuildShaderArtifact(
    source_name: &str,
    module: &SdslvModule,
) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>> {
    ValidateModule(module)?;
    let hlsl = EmitHlsl(module)?;
    let entry_points = CollectEntryPoints(module);

    Ok(SdslvShaderArtifact {
        SourceName: source_name.to_string(),
        Hlsl: hlsl,
        EntryPoints: entry_points,
    })
}

fn CollectEntryPoints(module: &SdslvModule) -> Vec<SdslvEntryPoint> {
    let mut entries = Vec::new();

    for declaration in &module.Declarations {
        match declaration {
            SdslvDecl::Shader(shader) => {
                if !shader.GenericParameters.is_empty() {
                    continue;
                }
                for method in &shader.StageMethods {
                    if let Some(stage) = MapStage(method.Stage.as_deref()) {
                        entries.push(NewEntryPoint(&shader.Name, method, stage));
                    }
                }
            }
            SdslvDecl::Compile(compile_decl) => {
                let generic_name = compile_decl.GenericShader.Segments.join(".");
                let generic_shader =
                    module
                        .Declarations
                        .iter()
                        .find_map(|candidate| match candidate {
                            SdslvDecl::Shader(shader) if shader.Name == generic_name => {
                                Some(shader)
                            }
                            _ => None,
                        });
                let Some(shader) = generic_shader else {
                    continue;
                };
                for method in &shader.StageMethods {
                    if let Some(stage) = MapStage(method.Stage.as_deref()) {
                        entries.push(NewEntryPoint(&compile_decl.Alias, method, stage));
                    }
                }
            }
            _ => {}
        }
    }

    entries
}

fn NewEntryPoint(
    shader_name: &str,
    method: &SdslvFunctionDecl,
    stage: SdslvShaderStage,
) -> SdslvEntryPoint {
    SdslvEntryPoint {
        Name: format!("{}_{}", shader_name, method.Name),
        Stage: stage,
        ShaderName: shader_name.to_string(),
        MethodName: method.Name.clone(),
        TargetProfile: DefaultTargetProfile(stage).to_string(),
    }
}

fn MapStage(stage: Option<&str>) -> Option<SdslvShaderStage> {
    match stage {
        Some("vertex") => Some(SdslvShaderStage::Vertex),
        Some("pixel") => Some(SdslvShaderStage::Pixel),
        Some("compute") => Some(SdslvShaderStage::Compute),
        _ => None,
    }
}

pub fn DefaultTargetProfile(stage: SdslvShaderStage) -> &'static str {
    match stage {
        SdslvShaderStage::Vertex => "vs_6_0",
        SdslvShaderStage::Pixel => "ps_6_0",
        SdslvShaderStage::Compute => "cs_6_0",
    }
}
