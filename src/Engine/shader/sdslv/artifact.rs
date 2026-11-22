#![allow(non_snake_case)]

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::emitter::EmitVdMirHlsl;
use super::lower::LowerToVdMir;
use super::parser::{ParseCanonicalSource, ParseSource};
use super::vdmir::VdMirEntryKind;

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

pub fn CompileCanonicalSourceToShaderArtifact(
    source_name: &str,
    source: &str,
) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>> {
    let module = ParseCanonicalSource(source)?;
    BuildShaderArtifact(source_name, &module)
}

pub fn BuildShaderArtifact(
    source_name: &str,
    module: &SdslvModule,
) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>> {
    let mir = LowerToVdMir(module)?;
    let hlsl = EmitVdMirHlsl(&mir)?;
    let entry_points = mir
        .EntryPoints()
        .map(|(function, entry)| {
            let stage = match entry.Kind {
                VdMirEntryKind::Vertex => SdslvShaderStage::Vertex,
                VdMirEntryKind::Pixel(_) => SdslvShaderStage::Pixel,
                VdMirEntryKind::Compute(_) => SdslvShaderStage::Compute,
            };
            SdslvEntryPoint {
                Name: function.Name.clone(),
                Stage: stage,
                ShaderName: entry.Program.clone(),
                MethodName: entry.Method.clone(),
                TargetProfile: DefaultTargetProfile(stage).to_string(),
            }
        })
        .collect();
    Ok(SdslvShaderArtifact {
        SourceName: source_name.to_string(),
        Hlsl: hlsl,
        EntryPoints: entry_points,
    })
}

pub fn EmitHlsl(module: &SdslvModule) -> Result<String, Vec<SdslvDiagnostic>> {
    let mir = LowerToVdMir(module)?;
    EmitVdMirHlsl(&mir)
}

pub fn CompileSourceToHlsl(source: &str) -> Result<String, Vec<SdslvDiagnostic>> {
    let module = ParseSource(source)?;
    EmitHlsl(&module)
}

pub fn DefaultTargetProfile(stage: SdslvShaderStage) -> &'static str {
    match stage {
        SdslvShaderStage::Vertex => "vs_6_0",
        SdslvShaderStage::Pixel => "ps_6_0",
        SdslvShaderStage::Compute => "cs_6_0",
    }
}
