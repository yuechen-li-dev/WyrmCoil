#![allow(non_snake_case)]

use crate::Engine::render::{
    ColorTargetFormat, HeadlessRenderTargetDesc, HeadlessRenderTargetError, RenderCommandPlan,
    RenderCommandPlanStatus, RenderPipelineLayoutPlan, ValidateHeadlessRenderTargetDesc,
    VertexBufferUploadPlan,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessDrawAssemblyPlan {
    pub Name: String,
    pub CommandName: String,
    pub PipelineName: String,
    pub VertexBufferLabel: String,
    pub TargetLabel: String,
    pub TargetWidth: u32,
    pub TargetHeight: u32,
    pub VertexCount: usize,
    pub VertexStrideBytes: u64,
    pub ColorFormat: ColorTargetFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlessDrawAssemblyError {
    EmptyName,
    CommandNotReady {
        Status: RenderCommandPlanStatus,
    },
    EmptyDraw,
    VertexCountMismatch {
        Command: usize,
        Upload: usize,
    },
    StrideMismatch {
        Command: u64,
        Upload: u64,
    },
    ByteCountMismatch {
        Command: usize,
        Upload: usize,
    },
    Target(HeadlessRenderTargetError),
    ColorFormatMismatch {
        Pipeline: ColorTargetFormat,
        Target: ColorTargetFormat,
    },
    PipelineNameMismatch {
        Command: String,
        Pipeline: String,
    },
}

pub fn BuildHeadlessDrawAssemblyPlan(
    name: &str,
    command: &RenderCommandPlan,
    pipeline: &RenderPipelineLayoutPlan,
    upload: &VertexBufferUploadPlan,
    target: &HeadlessRenderTargetDesc,
) -> Result<HeadlessDrawAssemblyPlan, HeadlessDrawAssemblyError> {
    if name.trim().is_empty() {
        return Err(HeadlessDrawAssemblyError::EmptyName);
    }
    if command.Status != RenderCommandPlanStatus::ReadyToDraw {
        return Err(HeadlessDrawAssemblyError::CommandNotReady {
            Status: command.Status,
        });
    }
    if command.VertexCount == 0 {
        return Err(HeadlessDrawAssemblyError::EmptyDraw);
    }
    if command.VertexCount != upload.VertexCount {
        return Err(HeadlessDrawAssemblyError::VertexCountMismatch {
            Command: command.VertexCount,
            Upload: upload.VertexCount,
        });
    }
    if command.VertexStrideBytes != upload.StrideBytes {
        return Err(HeadlessDrawAssemblyError::StrideMismatch {
            Command: command.VertexStrideBytes,
            Upload: upload.StrideBytes,
        });
    }
    if command.VertexByteCount != upload.Bytes.len() {
        return Err(HeadlessDrawAssemblyError::ByteCountMismatch {
            Command: command.VertexByteCount,
            Upload: upload.Bytes.len(),
        });
    }
    ValidateHeadlessRenderTargetDesc(target).map_err(HeadlessDrawAssemblyError::Target)?;
    if command.PipelineName != pipeline.Name {
        return Err(HeadlessDrawAssemblyError::PipelineNameMismatch {
            Command: command.PipelineName.clone(),
            Pipeline: pipeline.Name.clone(),
        });
    }
    if pipeline.ColorTarget.Format != target.Format {
        return Err(HeadlessDrawAssemblyError::ColorFormatMismatch {
            Pipeline: pipeline.ColorTarget.Format,
            Target: target.Format,
        });
    }

    Ok(HeadlessDrawAssemblyPlan {
        Name: name.to_string(),
        CommandName: command.Name.clone(),
        PipelineName: pipeline.Name.clone(),
        VertexBufferLabel: upload.Label.clone(),
        TargetLabel: target.Label.clone(),
        TargetWidth: target.Width,
        TargetHeight: target.Height,
        VertexCount: command.VertexCount,
        VertexStrideBytes: command.VertexStrideBytes,
        ColorFormat: target.Format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::{
        BuildHeadlessRenderTargetDesc, BuildRenderCommandPlan, BuildRenderPipelineLayoutPlan,
        BuildVertexBufferUploadPlan, ColorTargetDesc, CompiledPipelineDesc,
        CompiledShaderModuleDesc, CpuUploadRecord, DepthStencilDesc, ExtractSpriteVertices,
        RenderCommandPlanReason, RenderPipelineLayoutOptions, SpriteVertexBufferLayout,
        UploadExecutionMode, UploadExecutionReason, UploadExecutionResult,
    };
    use crate::Engine::wyrmcoil::{EntityId, RenderItem, RenderSnapshot, Vec2};

    fn BuildPipeline(name: &str, format: ColorTargetFormat) -> RenderPipelineLayoutPlan {
        BuildRenderPipelineLayoutPlan(
            CompiledPipelineDesc {
                Name: "SpritePipeline".to_string(),
                SourceName: "sprite.sdslv".to_string(),
                Vertex: CompiledShaderModuleDesc {
                    EntryPoint: "VSMain".to_string(),
                    TargetProfile: "vs_6_0".to_string(),
                    SpirvBytes: vec![1, 2, 3],
                },
                Pixel: CompiledShaderModuleDesc {
                    EntryPoint: "PSMain".to_string(),
                    TargetProfile: "ps_6_0".to_string(),
                    SpirvBytes: vec![4, 5],
                },
            },
            RenderPipelineLayoutOptions {
                Name: name.to_string(),
                VertexBuffers: vec![SpriteVertexBufferLayout()],
                ColorTarget: ColorTargetDesc { Format: format },
                Depth: Some(DepthStencilDesc {
                    Format: crate::Engine::render::DepthFormat::Depth24Plus,
                    DepthWriteEnabled: false,
                }),
            },
        )
        .expect("valid fake layout should build")
    }

    fn BuildUploadAndCommand() -> (VertexBufferUploadPlan, RenderCommandPlan) {
        let snapshot = RenderSnapshot {
            Frame: 5,
            Items: vec![RenderItem {
                Entity: EntityId(1),
                Position: Vec2 { X: 1.0, Y: 2.0 },
                SpriteId: 4,
            }],
        };
        let batch = ExtractSpriteVertices(&snapshot);
        let upload = BuildVertexBufferUploadPlan("SpriteVB", &batch)
            .expect("non-empty snapshot should produce upload plan");
        let pipeline = BuildPipeline("SpritePipelineLayout", ColorTargetFormat::Bgra8UnormSrgb);
        let execution = UploadExecutionResult {
            Mode: UploadExecutionMode::CpuRecordOnly,
            Reason: UploadExecutionReason::NoDeviceCpuFallback,
            RejectedModes: Vec::new(),
            CpuRecord: Some(CpuUploadRecord {
                Label: upload.Label.clone(),
                ByteCount: upload.Bytes.len(),
                VertexCount: upload.VertexCount,
                StrideBytes: upload.StrideBytes,
            }),
            GpuResource: None,
        };
        let command =
            BuildRenderCommandPlan("MainPassIntent", &pipeline, &upload, Some(&execution));
        (upload, command)
    }

    #[test]
    fn BuildHeadlessDrawAssemblyPlanAcceptsCompatibleInputs() {
        let pipeline = BuildPipeline("SpritePipelineLayout", ColorTargetFormat::Bgra8UnormSrgb);
        let (upload, command) = BuildUploadAndCommand();
        let target = BuildHeadlessRenderTargetDesc(
            "OffscreenTarget",
            64,
            32,
            ColorTargetFormat::Bgra8UnormSrgb,
        )
        .expect("valid target should build");

        let plan = BuildHeadlessDrawAssemblyPlan(
            "M33-DrawAssembly",
            &command,
            &pipeline,
            &upload,
            &target,
        )
        .expect("compatible metadata should assemble");

        assert_eq!(
            plan.Name, "M33-DrawAssembly",
            "assembly label should be preserved"
        );
        assert_eq!(
            plan.CommandName, command.Name,
            "command identity should be preserved"
        );
        assert_eq!(
            plan.PipelineName, pipeline.Name,
            "pipeline identity should be preserved"
        );
        assert_eq!(
            plan.VertexBufferLabel, upload.Label,
            "upload label should be preserved"
        );
        assert_eq!(
            plan.TargetLabel, target.Label,
            "target label should be preserved"
        );
        assert_eq!(plan.TargetWidth, 64, "target width should be preserved");
        assert_eq!(plan.TargetHeight, 32, "target height should be preserved");
        assert_eq!(
            plan.VertexCount, 1,
            "single item snapshot should yield one vertex"
        );
        assert_eq!(
            plan.VertexStrideBytes, 12,
            "sprite stride should stay 12 bytes"
        );
        assert_eq!(
            plan.ColorFormat,
            ColorTargetFormat::Bgra8UnormSrgb,
            "format should match pipeline/target"
        );
    }

    #[test]
    fn BuildHeadlessDrawAssemblyPlanRejectsReadinessAndMetadataMismatches() {
        let pipeline = BuildPipeline("SpritePipelineLayout", ColorTargetFormat::Bgra8UnormSrgb);
        let (upload, mut command) = BuildUploadAndCommand();
        let target = BuildHeadlessRenderTargetDesc(
            "OffscreenTarget",
            8,
            8,
            ColorTargetFormat::Bgra8UnormSrgb,
        )
        .expect("valid target should build");

        assert_eq!(
            BuildHeadlessDrawAssemblyPlan(" ", &command, &pipeline, &upload, &target).unwrap_err(),
            HeadlessDrawAssemblyError::EmptyName,
            "empty assembly names should be rejected"
        );

        command.Status = RenderCommandPlanStatus::NoOpEmptyBatch;
        command.Reason = RenderCommandPlanReason::EmptyBatch;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::CommandNotReady {
                Status: RenderCommandPlanStatus::NoOpEmptyBatch
            },
            "no-op command plans should be rejected"
        );

        command.Status = RenderCommandPlanStatus::Rejected;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::CommandNotReady {
                Status: RenderCommandPlanStatus::Rejected
            },
            "rejected command plans should be rejected"
        );

        command.Status = RenderCommandPlanStatus::ReadyToDraw;
        command.VertexCount = 0;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::EmptyDraw,
            "zero-vertex commands should be rejected"
        );

        command.VertexCount = upload.VertexCount + 1;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::VertexCountMismatch {
                Command: 2,
                Upload: 1
            },
            "vertex-count mismatches should be rejected"
        );

        command.VertexCount = upload.VertexCount;
        command.VertexStrideBytes = upload.StrideBytes + 4;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::StrideMismatch {
                Command: 16,
                Upload: 12
            },
            "stride mismatches should be rejected"
        );

        command.VertexStrideBytes = upload.StrideBytes;
        command.VertexByteCount = upload.Bytes.len() + 1;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::ByteCountMismatch {
                Command: 13,
                Upload: 12
            },
            "byte-count mismatches should be rejected"
        );
    }

    #[test]
    fn BuildHeadlessDrawAssemblyPlanRejectsTargetAndPipelineFormatMismatchAndNameMismatch() {
        let pipeline = BuildPipeline("SpritePipelineLayout", ColorTargetFormat::Bgra8UnormSrgb);
        let (upload, mut command) = BuildUploadAndCommand();
        let mut target = BuildHeadlessRenderTargetDesc(
            "OffscreenTarget",
            8,
            8,
            ColorTargetFormat::Bgra8UnormSrgb,
        )
        .expect("valid target should build");

        command.PipelineName = "DifferentPipeline".to_string();
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::PipelineNameMismatch {
                Command: "DifferentPipeline".to_string(),
                Pipeline: "SpritePipelineLayout".to_string()
            },
            "pipeline identity mismatch should be rejected"
        );

        command.PipelineName = pipeline.Name.clone();
        target.Format = ColorTargetFormat::Rgba8UnormSrgb;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::ColorFormatMismatch {
                Pipeline: ColorTargetFormat::Bgra8UnormSrgb,
                Target: ColorTargetFormat::Rgba8UnormSrgb,
            },
            "color format mismatches should be rejected"
        );

        target.Format = ColorTargetFormat::Bgra8UnormSrgb;
        target.Width = 0;
        assert_eq!(
            BuildHeadlessDrawAssemblyPlan("Draw", &command, &pipeline, &upload, &target)
                .unwrap_err(),
            HeadlessDrawAssemblyError::Target(HeadlessRenderTargetError::InvalidWidth),
            "invalid target metadata should map through structured target errors"
        );
    }

    #[test]
    fn BuildHeadlessDrawAssemblyPlanIsDeterministic() {
        let pipeline = BuildPipeline("SpritePipelineLayout", ColorTargetFormat::Bgra8UnormSrgb);
        let (upload, command) = BuildUploadAndCommand();
        let target = BuildHeadlessRenderTargetDesc(
            "OffscreenTarget",
            64,
            64,
            ColorTargetFormat::Bgra8UnormSrgb,
        )
        .expect("valid target should build");

        let a = BuildHeadlessDrawAssemblyPlan("Assembly", &command, &pipeline, &upload, &target)
            .expect("first assembly should succeed");
        let b = BuildHeadlessDrawAssemblyPlan("Assembly", &command, &pipeline, &upload, &target)
            .expect("second assembly should succeed");

        assert_eq!(a, b, "same inputs should produce identical assembly plans");
    }
}
