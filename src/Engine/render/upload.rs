#![allow(non_snake_case)]

use crate::Engine::render::extract::{
    ExtractedRenderBatch, PackSpriteVertices, SpriteVertexStrideBytes,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBufferUsageIntent {
    Vertex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexBufferUploadPlan {
    pub Label: String,
    pub Bytes: Vec<u8>,
    pub VertexCount: usize,
    pub StrideBytes: u64,
    pub Usage: GpuBufferUsageIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VertexBufferUploadPlanError {
    EmptyLabel,
    ByteLengthMismatch { Expected: usize, Actual: usize },
    StrideMismatch { Expected: u64, Actual: u64 },
}

pub fn BuildVertexBufferUploadPlan(
    label: &str,
    batch: &ExtractedRenderBatch,
) -> Result<VertexBufferUploadPlan, VertexBufferUploadPlanError> {
    if label.trim().is_empty() {
        return Err(VertexBufferUploadPlanError::EmptyLabel);
    }

    let bytes = PackSpriteVertices(&batch.Vertices);
    let stride_bytes = SpriteVertexStrideBytes() as u64;

    let plan = VertexBufferUploadPlan {
        Label: label.to_string(),
        Bytes: bytes,
        VertexCount: batch.Vertices.len(),
        StrideBytes: stride_bytes,
        Usage: GpuBufferUsageIntent::Vertex,
    };

    ValidateVertexBufferUploadPlan(&plan)?;
    Ok(plan)
}

pub fn ValidateVertexBufferUploadPlan(
    plan: &VertexBufferUploadPlan,
) -> Result<(), VertexBufferUploadPlanError> {
    let expected_stride = SpriteVertexStrideBytes() as u64;
    if plan.StrideBytes != expected_stride {
        return Err(VertexBufferUploadPlanError::StrideMismatch {
            Expected: expected_stride,
            Actual: plan.StrideBytes,
        });
    }

    let expected_byte_len = plan.VertexCount * SpriteVertexStrideBytes();
    if plan.Bytes.len() != expected_byte_len {
        return Err(VertexBufferUploadPlanError::ByteLengthMismatch {
            Expected: expected_byte_len,
            Actual: plan.Bytes.len(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::extract::{
        ExtractSpriteVertices, SpriteVertex, SpriteVertexBufferLayout,
    };
    use crate::Engine::wyrmcoil::{EntityId, RenderItem, RenderSnapshot, Vec2};

    #[test]
    fn BuildVertexBufferUploadPlanAllowsEmptyBatch() {
        let batch = ExtractedRenderBatch {
            Frame: 77,
            Vertices: Vec::new(),
        };

        let plan = BuildVertexBufferUploadPlan("SpriteVertices", &batch)
            .expect("empty batches should build valid empty upload plans");

        assert_eq!(plan.Label, "SpriteVertices", "label should be preserved");
        assert_eq!(
            plan.VertexCount, 0,
            "empty batch should preserve zero count"
        );
        assert_eq!(plan.Bytes.len(), 0, "empty batch should produce zero bytes");
        assert_eq!(
            plan.StrideBytes,
            SpriteVertexStrideBytes() as u64,
            "empty batch should still publish canonical sprite vertex stride"
        );
    }

    #[test]
    fn BuildVertexBufferUploadPlanOneAndManyVertices() {
        let single = ExtractedRenderBatch {
            Frame: 1,
            Vertices: vec![SpriteVertex {
                X: 1.25,
                Y: -2.5,
                SpriteId: 3,
            }],
        };
        let one = BuildVertexBufferUploadPlan("Single", &single)
            .expect("single-vertex batch should produce a valid plan");
        assert_eq!(
            one.VertexCount, 1,
            "single batch should preserve one vertex"
        );
        assert_eq!(
            one.Bytes.len(),
            SpriteVertexStrideBytes(),
            "single batch byte length should equal one stride"
        );

        let many = ExtractedRenderBatch {
            Frame: 1,
            Vertices: vec![
                SpriteVertex {
                    X: 9.0,
                    Y: 10.0,
                    SpriteId: 11,
                },
                SpriteVertex {
                    X: -3.0,
                    Y: 5.5,
                    SpriteId: 22,
                },
            ],
        };
        let many_plan = BuildVertexBufferUploadPlan("Many", &many)
            .expect("multi-vertex batch should produce a valid plan");
        assert_eq!(many_plan.VertexCount, 2, "vertex count should be preserved");
        assert_eq!(
            many_plan.Bytes,
            PackSpriteVertices(&many.Vertices),
            "plan bytes should exactly preserve extraction packing order"
        );
    }

    #[test]
    fn BuildVertexBufferUploadPlanRejectsEmptyLabel() {
        let batch = ExtractedRenderBatch {
            Frame: 0,
            Vertices: Vec::new(),
        };

        assert_eq!(
            BuildVertexBufferUploadPlan("  ", &batch).unwrap_err(),
            VertexBufferUploadPlanError::EmptyLabel,
            "empty labels should be rejected with a structured error"
        );
    }

    #[test]
    fn ValidateVertexBufferUploadPlanChecksByteLengthAndStride() {
        let valid = VertexBufferUploadPlan {
            Label: "Valid".to_string(),
            Bytes: vec![0; SpriteVertexStrideBytes()],
            VertexCount: 1,
            StrideBytes: SpriteVertexStrideBytes() as u64,
            Usage: GpuBufferUsageIntent::Vertex,
        };
        ValidateVertexBufferUploadPlan(&valid)
            .expect("valid plan byte length and stride should pass validation");

        let mut bad_stride = valid.clone();
        bad_stride.StrideBytes = 999;
        assert_eq!(
            ValidateVertexBufferUploadPlan(&bad_stride).unwrap_err(),
            VertexBufferUploadPlanError::StrideMismatch {
                Expected: SpriteVertexStrideBytes() as u64,
                Actual: 999,
            },
            "plans with non-canonical stride should be rejected"
        );

        let mut bad_length = valid;
        bad_length.Bytes.push(0);
        assert_eq!(
            ValidateVertexBufferUploadPlan(&bad_length).unwrap_err(),
            VertexBufferUploadPlanError::ByteLengthMismatch {
                Expected: SpriteVertexStrideBytes(),
                Actual: SpriteVertexStrideBytes() + 1,
            },
            "plans with mismatched byte length should be rejected"
        );
    }

    #[test]
    fn BuildVertexBufferUploadPlanIsDeterministicFromBatchAndSnapshotPath() {
        let batch = ExtractedRenderBatch {
            Frame: 8,
            Vertices: vec![
                SpriteVertex {
                    X: 0.0,
                    Y: 1.0,
                    SpriteId: 2,
                },
                SpriteVertex {
                    X: 3.0,
                    Y: 4.0,
                    SpriteId: 5,
                },
            ],
        };
        let first = BuildVertexBufferUploadPlan("Deterministic", &batch)
            .expect("first plan build should succeed");
        let second = BuildVertexBufferUploadPlan("Deterministic", &batch)
            .expect("second plan build should succeed");
        assert_eq!(
            first, second,
            "same extracted batch should produce identical upload plans"
        );

        let snapshot = RenderSnapshot {
            Frame: 13,
            Items: vec![
                RenderItem {
                    Entity: EntityId(3),
                    Position: Vec2 { X: 1.0, Y: 2.0 },
                    SpriteId: 99,
                },
                RenderItem {
                    Entity: EntityId(4),
                    Position: Vec2 { X: -8.5, Y: 7.25 },
                    SpriteId: 77,
                },
            ],
        };
        let extracted_a = ExtractSpriteVertices(&snapshot);
        let extracted_b = ExtractSpriteVertices(&snapshot);
        let plan_a = BuildVertexBufferUploadPlan("SnapshotPath", &extracted_a)
            .expect("snapshot extraction path should build a plan");
        let plan_b = BuildVertexBufferUploadPlan("SnapshotPath", &extracted_b)
            .expect("repeated snapshot extraction path should build a plan");
        assert_eq!(
            plan_a, plan_b,
            "snapshot -> extraction -> upload-plan should remain deterministic"
        );
    }

    #[test]
    fn UploadPlanStrideMatchesLayoutContract() {
        let batch = ExtractedRenderBatch {
            Frame: 3,
            Vertices: vec![SpriteVertex {
                X: 5.0,
                Y: 6.0,
                SpriteId: 7,
            }],
        };
        let plan = BuildVertexBufferUploadPlan("LayoutCompat", &batch)
            .expect("valid batch should produce layout-compatible plan");
        let layout = SpriteVertexBufferLayout();

        assert_eq!(
            plan.StrideBytes, layout.StrideBytes,
            "upload plan stride should match sprite vertex layout stride"
        );
        assert_eq!(
            plan.Bytes.len() as u64,
            plan.StrideBytes * plan.VertexCount as u64,
            "byte payload length should align to vertex count and stride"
        );
    }
}
