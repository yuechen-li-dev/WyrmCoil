#![allow(non_snake_case)]

use super::sdslv::DxcCompileRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlslShaderArtifact {
    pub SourceName: String,
    pub Hlsl: String,
    pub EntryPoints: Vec<HlslEntryPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlslEntryPoint {
    pub Name: String,
    pub Stage: HlslShaderStage,
    pub TargetProfile: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlslShaderStage {
    Vertex,
    Pixel,
    Compute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlslShaderArtifactError {
    EmptySourceName,
    EmptySource,
    MissingEntryPoints,
    EmptyEntryName,
    EmptyTargetProfile,
    StageProfileMismatch {
        Stage: HlslShaderStage,
        TargetProfile: String,
    },
    DuplicateEntryPoint {
        Name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlslDxcBridgeError {
    EntryPointNotFound {
        EntryPoint: String,
        SourceName: String,
    },
}

impl HlslEntryPoint {
    pub fn Vertex(name: &str) -> Self {
        Self::WithStageAndProfile(name, HlslShaderStage::Vertex, "vs_6_0")
    }

    pub fn Pixel(name: &str) -> Self {
        Self::WithStageAndProfile(name, HlslShaderStage::Pixel, "ps_6_0")
    }

    pub fn Compute(name: &str) -> Self {
        Self::WithStageAndProfile(name, HlslShaderStage::Compute, "cs_6_0")
    }

    pub fn WithStageAndProfile(name: &str, stage: HlslShaderStage, target_profile: &str) -> Self {
        Self {
            Name: name.to_string(),
            Stage: stage,
            TargetProfile: target_profile.to_string(),
        }
    }
}

pub fn BuildHlslShaderArtifact(
    source_name: &str,
    hlsl_source: &str,
    entry_points: Vec<HlslEntryPoint>,
) -> Result<HlslShaderArtifact, HlslShaderArtifactError> {
    if source_name.trim().is_empty() {
        return Err(HlslShaderArtifactError::EmptySourceName);
    }
    if hlsl_source.trim().is_empty() {
        return Err(HlslShaderArtifactError::EmptySource);
    }
    if entry_points.is_empty() {
        return Err(HlslShaderArtifactError::MissingEntryPoints);
    }

    let mut seen_entries = std::collections::BTreeSet::new();
    for entry in &entry_points {
        if entry.Name.trim().is_empty() {
            return Err(HlslShaderArtifactError::EmptyEntryName);
        }
        if entry.TargetProfile.trim().is_empty() {
            return Err(HlslShaderArtifactError::EmptyTargetProfile);
        }
        if !TargetProfileMatchesStage(entry.Stage, &entry.TargetProfile) {
            return Err(HlslShaderArtifactError::StageProfileMismatch {
                Stage: entry.Stage,
                TargetProfile: entry.TargetProfile.clone(),
            });
        }
        if !seen_entries.insert(entry.Name.clone()) {
            return Err(HlslShaderArtifactError::DuplicateEntryPoint {
                Name: entry.Name.clone(),
            });
        }
    }

    Ok(HlslShaderArtifact {
        SourceName: source_name.to_string(),
        Hlsl: hlsl_source.to_string(),
        EntryPoints: entry_points,
    })
}

pub fn BuildDxcRequestsForHlslArtifact(artifact: &HlslShaderArtifact) -> Vec<DxcCompileRequest> {
    artifact
        .EntryPoints
        .iter()
        .map(|entry| DxcCompileRequest {
            SourceName: artifact.SourceName.clone(),
            Hlsl: artifact.Hlsl.clone(),
            EntryPoint: entry.Name.clone(),
            TargetProfile: entry.TargetProfile.clone(),
        })
        .collect()
}

pub fn BuildDxcRequestForHlslEntry(
    artifact: &HlslShaderArtifact,
    entry_name: &str,
) -> Result<DxcCompileRequest, HlslDxcBridgeError> {
    let entry = artifact
        .EntryPoints
        .iter()
        .find(|candidate| candidate.Name == entry_name)
        .ok_or_else(|| HlslDxcBridgeError::EntryPointNotFound {
            EntryPoint: entry_name.to_string(),
            SourceName: artifact.SourceName.clone(),
        })?;

    Ok(DxcCompileRequest {
        SourceName: artifact.SourceName.clone(),
        Hlsl: artifact.Hlsl.clone(),
        EntryPoint: entry.Name.clone(),
        TargetProfile: entry.TargetProfile.clone(),
    })
}

fn TargetProfileMatchesStage(stage: HlslShaderStage, target_profile: &str) -> bool {
    match stage {
        HlslShaderStage::Vertex => target_profile.starts_with("vs_"),
        HlslShaderStage::Pixel => target_profile.starts_with("ps_"),
        HlslShaderStage::Compute => target_profile.starts_with("cs_"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::shader::sdslv::{BuildDxcCommand, DxcOptions};

    const TEST_HLSL: &str = "float4 VSMain(float4 p: POSITION): SV_Position { return p; }\nfloat4 PSMain(): SV_Target { return float4(1,1,1,1); }";

    #[test]
    fn ValidArtifactBuildsWithVertexAndPixelEntries() {
        let artifact = BuildHlslShaderArtifact(
            "legacy_flat.hlsl",
            TEST_HLSL,
            vec![
                HlslEntryPoint::Vertex("VSMain"),
                HlslEntryPoint::Pixel("PSMain"),
            ],
        )
        .expect("valid HLSL wrapper metadata should build artifact");
        assert_eq!(artifact.SourceName, "legacy_flat.hlsl");
        assert_eq!(artifact.EntryPoints.len(), 2);
    }

    #[test]
    fn ValidationRejectsInvalidMetadata() {
        assert_eq!(
            BuildHlslShaderArtifact(" ", TEST_HLSL, vec![HlslEntryPoint::Vertex("VSMain")])
                .unwrap_err(),
            HlslShaderArtifactError::EmptySourceName
        );
        assert_eq!(
            BuildHlslShaderArtifact("a.hlsl", "\n\t", vec![HlslEntryPoint::Vertex("VSMain")])
                .unwrap_err(),
            HlslShaderArtifactError::EmptySource
        );
        assert_eq!(
            BuildHlslShaderArtifact("a.hlsl", TEST_HLSL, vec![]).unwrap_err(),
            HlslShaderArtifactError::MissingEntryPoints
        );
        assert_eq!(
            BuildHlslShaderArtifact(
                "a.hlsl",
                TEST_HLSL,
                vec![HlslEntryPoint::WithStageAndProfile(
                    "",
                    HlslShaderStage::Vertex,
                    "vs_6_0"
                )],
            )
            .unwrap_err(),
            HlslShaderArtifactError::EmptyEntryName
        );
        assert_eq!(
            BuildHlslShaderArtifact(
                "a.hlsl",
                TEST_HLSL,
                vec![HlslEntryPoint::WithStageAndProfile(
                    "VSMain",
                    HlslShaderStage::Vertex,
                    " "
                )],
            )
            .unwrap_err(),
            HlslShaderArtifactError::EmptyTargetProfile
        );
        assert_eq!(
            BuildHlslShaderArtifact(
                "a.hlsl",
                TEST_HLSL,
                vec![
                    HlslEntryPoint::Vertex("VSMain"),
                    HlslEntryPoint::Vertex("VSMain"),
                ],
            )
            .unwrap_err(),
            HlslShaderArtifactError::DuplicateEntryPoint {
                Name: "VSMain".to_string()
            }
        );
    }

    #[test]
    fn StageProfileMismatchRejectedAndDefaultsUse6_0Profiles() {
        assert_eq!(HlslEntryPoint::Vertex("VSMain").TargetProfile, "vs_6_0");
        assert_eq!(HlslEntryPoint::Pixel("PSMain").TargetProfile, "ps_6_0");
        assert_eq!(HlslEntryPoint::Compute("CSMain").TargetProfile, "cs_6_0");

        let mismatch = BuildHlslShaderArtifact(
            "a.hlsl",
            TEST_HLSL,
            vec![HlslEntryPoint::WithStageAndProfile(
                "VSMain",
                HlslShaderStage::Vertex,
                "ps_6_0",
            )],
        )
        .unwrap_err();
        assert_eq!(
            mismatch,
            HlslShaderArtifactError::StageProfileMismatch {
                Stage: HlslShaderStage::Vertex,
                TargetProfile: "ps_6_0".to_string(),
            }
        );
    }

    #[test]
    fn DxcBridgeBuildsRequestsAndPreservesSourceAndCommandShape() {
        let artifact = BuildHlslShaderArtifact(
            "legacy_flat.hlsl",
            TEST_HLSL,
            vec![
                HlslEntryPoint::Vertex("VSMain"),
                HlslEntryPoint::Pixel("PSMain"),
            ],
        )
        .expect("valid artifact should build");

        let requests = BuildDxcRequestsForHlslArtifact(&artifact);
        assert_eq!(requests.len(), 2, "two entries should yield two requests");
        assert_eq!(requests[0].Hlsl, TEST_HLSL, "HLSL source must be preserved");

        let vertex_request = BuildDxcRequestForHlslEntry(&artifact, "VSMain")
            .expect("known vertex entry should map to request");
        let vertex_cmd = BuildDxcCommand(&vertex_request, &DxcOptions::default()).join(" ");
        assert!(vertex_cmd.contains("-E VSMain"));
        assert!(vertex_cmd.contains("-T vs_6_0"));

        let pixel_request = BuildDxcRequestForHlslEntry(&artifact, "PSMain")
            .expect("known pixel entry should map to request");
        let pixel_cmd = BuildDxcCommand(&pixel_request, &DxcOptions::default()).join(" ");
        assert!(pixel_cmd.contains("-E PSMain"));
        assert!(pixel_cmd.contains("-T ps_6_0"));

        let missing = BuildDxcRequestForHlslEntry(&artifact, "Missing").unwrap_err();
        assert_eq!(
            missing,
            HlslDxcBridgeError::EntryPointNotFound {
                EntryPoint: "Missing".to_string(),
                SourceName: "legacy_flat.hlsl".to_string(),
            }
        );
    }
}
