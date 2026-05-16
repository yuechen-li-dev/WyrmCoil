#![allow(non_snake_case)]

use crate::Engine::render::{
    RenderPipelineLayoutPlan, UploadExecutionMode, UploadExecutionResult, VertexBufferUploadPlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCommandPlanStatus {
    ReadyToDraw,
    NoOpEmptyBatch,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCommandPlanReason {
    Ready,
    EmptyBatch,
    MissingPipelineVertexLayout,
    UploadRejected,
    UploadExecutionMissing,
    VertexCountZero,
    StrideMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCommandPlan {
    pub Name: String,
    pub Status: RenderCommandPlanStatus,
    pub Reason: RenderCommandPlanReason,
    pub PipelineName: String,
    pub VertexCount: usize,
    pub VertexStrideBytes: u64,
    pub VertexByteCount: usize,
    pub UsesGpuBuffer: bool,
    pub UsesCpuRecord: bool,
}

pub fn BuildRenderCommandPlan(
    name: &str,
    pipeline: &RenderPipelineLayoutPlan,
    upload: &VertexBufferUploadPlan,
    upload_execution: Option<&UploadExecutionResult>,
) -> RenderCommandPlan {
    let pipeline_stride = pipeline
        .VertexBuffers
        .first()
        .map(|x| x.StrideBytes)
        .unwrap_or(0);

    let mut plan = RenderCommandPlan {
        Name: name.to_string(),
        Status: RenderCommandPlanStatus::Rejected,
        Reason: RenderCommandPlanReason::UploadExecutionMissing,
        PipelineName: pipeline.Name.clone(),
        VertexCount: upload.VertexCount,
        VertexStrideBytes: upload.StrideBytes,
        VertexByteCount: upload.Bytes.len(),
        UsesGpuBuffer: false,
        UsesCpuRecord: false,
    };

    if pipeline_stride == 0 {
        plan.Reason = RenderCommandPlanReason::MissingPipelineVertexLayout;
        return plan;
    }

    if upload.StrideBytes != pipeline_stride {
        plan.Reason = RenderCommandPlanReason::StrideMismatch;
        return plan;
    }

    if upload_execution.is_none() {
        return plan;
    }

    let execution = upload_execution.expect("checked is_some");

    match execution.Mode {
        UploadExecutionMode::NoOpEmptyUpload => {
            plan.Status = RenderCommandPlanStatus::NoOpEmptyBatch;
            plan.Reason = RenderCommandPlanReason::EmptyBatch;
            plan
        }
        UploadExecutionMode::Rejected => {
            plan.Status = RenderCommandPlanStatus::Rejected;
            plan.Reason = RenderCommandPlanReason::UploadRejected;
            plan
        }
        UploadExecutionMode::CpuRecordOnly => {
            if execution.CpuRecord.is_none() {
                plan.Status = RenderCommandPlanStatus::Rejected;
                return plan;
            }
            if upload.VertexCount == 0 {
                plan.Status = RenderCommandPlanStatus::Rejected;
                plan.Reason = RenderCommandPlanReason::VertexCountZero;
                return plan;
            }
            plan.Status = RenderCommandPlanStatus::ReadyToDraw;
            plan.Reason = RenderCommandPlanReason::Ready;
            plan.UsesCpuRecord = true;
            plan
        }
        UploadExecutionMode::GpuBufferCreate => {
            if upload.VertexCount == 0 {
                plan.Status = RenderCommandPlanStatus::Rejected;
                plan.Reason = RenderCommandPlanReason::VertexCountZero;
                return plan;
            }
            plan.Status = RenderCommandPlanStatus::ReadyToDraw;
            plan.Reason = RenderCommandPlanReason::Ready;
            plan.UsesGpuBuffer = true;
            plan
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::primitives::{EntityId, RenderItem, RenderSnapshot, Vec2};
    use crate::Engine::render::{
        BuildRenderPipelineLayoutPlan, BuildVertexBufferUploadPlan, ColorTargetDesc,
        ColorTargetFormat, CompiledPipelineDesc, CompiledShaderModuleDesc, CpuUploadRecord,
        DepthStencilDesc, ExtractSpriteVertices, RenderPipelineLayoutOptions,
        SpriteVertexBufferLayout, UploadExecutionReason, ValidateVertexBufferUploadPlan,
        VertexBufferUploadPlan,
    };

    fn BuildPipeline() -> RenderPipelineLayoutPlan {
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
                Name: "SpritePipelineLayout".to_string(),
                VertexBuffers: vec![SpriteVertexBufferLayout()],
                ColorTarget: ColorTargetDesc {
                    Format: ColorTargetFormat::Bgra8UnormSrgb,
                },
                Depth: Some(DepthStencilDesc {
                    Format: crate::Engine::render::DepthFormat::Depth24Plus,
                    DepthWriteEnabled: false,
                }),
            },
        )
        .expect("valid fake layout should build")
    }

    fn BuildUpload(snapshot: &RenderSnapshot) -> VertexBufferUploadPlan {
        let batch = ExtractSpriteVertices(snapshot);
        BuildVertexBufferUploadPlan("SpriteVB", &batch).expect("valid upload expected")
    }

    #[test]
    fn BuildRenderCommandPlanReadyFromCpuRecord() {
        let snapshot = RenderSnapshot {
            Frame: 9,
            Items: vec![RenderItem {
                Entity: EntityId(1),
                Position: Vec2 { X: 1.0, Y: 2.0 },
                SpriteId: 11,
            }],
        };
        let upload = BuildUpload(&snapshot);
        let pipeline = BuildPipeline();

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
        assert_eq!(
            command.Status,
            RenderCommandPlanStatus::ReadyToDraw,
            "cpu record should be acceptable planning-only draw intent"
        );
        assert_eq!(
            command.Reason,
            RenderCommandPlanReason::Ready,
            "ready plans should carry ready reason"
        );
        assert!(
            command.UsesCpuRecord,
            "cpu record-only execution should flag cpu record usage"
        );
        assert!(
            !command.UsesGpuBuffer,
            "cpu record-only execution should not flag gpu buffer usage"
        );
        assert_eq!(
            command.PipelineName, pipeline.Name,
            "pipeline identity should be preserved"
        );
        assert_eq!(
            command.VertexCount, 1,
            "snapshot with one item should produce one planned vertex"
        );
        assert_eq!(
            command.VertexStrideBytes, 12,
            "sprite vertex stride should remain 12 bytes"
        );
        assert_eq!(
            command.VertexByteCount, 12,
            "one planned sprite vertex should consume one stride of bytes"
        );
    }

    #[test]
    fn BuildRenderCommandPlanReadyFromGpuPathMetadata() {
        let snapshot = RenderSnapshot {
            Frame: 1,
            Items: vec![RenderItem {
                Entity: EntityId(0),
                Position: Vec2 { X: 3.0, Y: 4.0 },
                SpriteId: 2,
            }],
        };
        let upload = BuildUpload(&snapshot);
        let pipeline = BuildPipeline();

        let execution = UploadExecutionResult {
            Mode: UploadExecutionMode::GpuBufferCreate,
            Reason: UploadExecutionReason::GpuDeviceAvailable,
            RejectedModes: Vec::new(),
            CpuRecord: None,
            GpuResource: None,
        };

        let command =
            BuildRenderCommandPlan("MainPassIntent", &pipeline, &upload, Some(&execution));
        assert_eq!(
            command.Status,
            RenderCommandPlanStatus::ReadyToDraw,
            "gpu-selected metadata path should map to ready command plan status"
        );
        assert!(
            command.UsesGpuBuffer,
            "gpu mode should mark gpu buffer usage"
        );
        assert!(
            !command.UsesCpuRecord,
            "gpu mode should not mark cpu record usage"
        );
    }

    #[test]
    fn BuildRenderCommandPlanNoOpForEmptyUpload() {
        let snapshot = RenderSnapshot {
            Frame: 13,
            Items: Vec::new(),
        };
        let upload = BuildUpload(&snapshot);
        let pipeline = BuildPipeline();

        let execution = UploadExecutionResult {
            Mode: UploadExecutionMode::NoOpEmptyUpload,
            Reason: UploadExecutionReason::EmptyUploadNoOp,
            RejectedModes: Vec::new(),
            CpuRecord: None,
            GpuResource: None,
        };

        let command =
            BuildRenderCommandPlan("MainPassIntent", &pipeline, &upload, Some(&execution));
        assert_eq!(
            command.Status,
            RenderCommandPlanStatus::NoOpEmptyBatch,
            "empty upload path should produce no-op command plan status"
        );
        assert_eq!(
            command.Reason,
            RenderCommandPlanReason::EmptyBatch,
            "empty upload path should carry explicit empty-batch reason"
        );
        assert_eq!(
            command.VertexCount, 0,
            "empty snapshot should remain zero vertices"
        );
        assert_eq!(
            command.VertexByteCount, 0,
            "empty snapshot should remain zero bytes"
        );
    }

    #[test]
    fn BuildRenderCommandPlanRejectedCasesAndDeterminism() {
        let snapshot = RenderSnapshot {
            Frame: 2,
            Items: vec![RenderItem {
                Entity: EntityId(5),
                Position: Vec2 { X: -5.0, Y: 2.0 },
                SpriteId: 20,
            }],
        };
        let upload = BuildUpload(&snapshot);
        let pipeline = BuildPipeline();

        let rejected_execution = UploadExecutionResult {
            Mode: UploadExecutionMode::Rejected,
            Reason: UploadExecutionReason::MissingLifecycleUploadAct,
            RejectedModes: Vec::new(),
            CpuRecord: None,
            GpuResource: None,
        };
        let rejected = BuildRenderCommandPlan(
            "MainPassIntent",
            &pipeline,
            &upload,
            Some(&rejected_execution),
        );
        assert_eq!(
            rejected.Status,
            RenderCommandPlanStatus::Rejected,
            "rejected upload execution should reject command plan"
        );
        assert_eq!(
            rejected.Reason,
            RenderCommandPlanReason::UploadRejected,
            "rejected upload execution should preserve structured rejection reason"
        );

        let missing_execution = BuildRenderCommandPlan("MainPassIntent", &pipeline, &upload, None);
        assert_eq!(
            missing_execution.Reason,
            RenderCommandPlanReason::UploadExecutionMissing,
            "missing upload execution should be surfaced as explicit command planning blocker"
        );

        let mut bad_upload = upload.clone();
        bad_upload.StrideBytes = 16;
        assert!(
            ValidateVertexBufferUploadPlan(&upload).is_ok(),
            "test control upload should stay valid"
        );
        let stride_rejected = BuildRenderCommandPlan(
            "MainPassIntent",
            &pipeline,
            &bad_upload,
            Some(&rejected_execution),
        );
        assert_eq!(
            stride_rejected.Reason,
            RenderCommandPlanReason::StrideMismatch,
            "pipeline/upload stride mismatch should reject planning"
        );

        let a = BuildRenderCommandPlan(
            "MainPassIntent",
            &pipeline,
            &upload,
            Some(&rejected_execution),
        );
        let b = BuildRenderCommandPlan(
            "MainPassIntent",
            &pipeline,
            &upload,
            Some(&rejected_execution),
        );
        assert_eq!(
            a, b,
            "identical inputs should produce deterministic command plans"
        );
    }
}
