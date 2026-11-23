#![allow(non_snake_case)]

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::emitter::EmitVdMirHlsl;
use super::lower::LowerToVdMir;
use super::parser::{ParseCanonicalSource, ParseSource};
use super::vdmir::VdMirEntryKind;
use crate::Engine::startup::StartupStage;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvShaderArtifact {
    pub SourceName: String,
    pub Hlsl: String,
    pub EntryPoints: Vec<SdslvEntryPoint>,
    pub(super) SourceDigest: Option<[u8; 32]>,
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
    let module = StartupStage::Call("sdslv_parse", || ParseSource(source))?;
    let mut artifact = BuildShaderArtifact(source_name, &module)?;
    artifact.SourceDigest = Some(Sha256::digest(source).into());
    Ok(artifact)
}

pub fn CompileCanonicalSourceToShaderArtifact(
    source_name: &str,
    source: &str,
) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>> {
    let module = StartupStage::Call("sdslv_parse", || ParseCanonicalSource(source))?;
    let mut artifact = BuildShaderArtifact(source_name, &module)?;
    artifact.SourceDigest = Some(Sha256::digest(source).into());
    Ok(artifact)
}

pub fn BuildShaderArtifact(
    source_name: &str,
    module: &SdslvModule,
) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>> {
    let mir = LowerToVdMir(module)?;
    let hlsl = StartupStage::Call("sdslv_hlsl", || EmitVdMirHlsl(&mir))?;
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
        SourceDigest: None,
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
