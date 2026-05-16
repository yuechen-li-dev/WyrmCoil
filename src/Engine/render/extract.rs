#![allow(non_snake_case)]

use crate::Engine::render::pipeline::{
    VertexAttributeDesc, VertexBufferLayoutDesc, VertexFormat, VertexStepMode,
};
use crate::Engine::wyrmcoil::RenderSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteVertex {
    pub X: f32,
    pub Y: f32,
    pub SpriteId: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedRenderBatch {
    pub Frame: u64,
    pub Vertices: Vec<SpriteVertex>,
}

impl ExtractedRenderBatch {
    pub fn VertexBytes(&self) -> Vec<u8> {
        PackSpriteVertices(&self.Vertices)
    }
}

pub fn ExtractSpriteVertices(snapshot: &RenderSnapshot) -> ExtractedRenderBatch {
    let mut vertices = Vec::with_capacity(snapshot.Items.len());
    for item in &snapshot.Items {
        vertices.push(SpriteVertex {
            X: item.Position.X,
            Y: item.Position.Y,
            SpriteId: item.SpriteId,
        });
    }

    ExtractedRenderBatch {
        Frame: snapshot.Frame,
        Vertices: vertices,
    }
}

pub fn PackSpriteVertices(vertices: &[SpriteVertex]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vertices.len() * SpriteVertexStrideBytes());
    for vertex in vertices {
        bytes.extend_from_slice(&vertex.X.to_le_bytes());
        bytes.extend_from_slice(&vertex.Y.to_le_bytes());
        bytes.extend_from_slice(&vertex.SpriteId.to_le_bytes());
    }
    bytes
}

pub fn SpriteVertexStrideBytes() -> usize {
    12
}

pub fn SpriteVertexBufferLayout() -> VertexBufferLayoutDesc {
    VertexBufferLayoutDesc {
        StrideBytes: SpriteVertexStrideBytes() as u64,
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::wyrmcoil::{EntityId, RenderItem, RenderSnapshot, Vec2};

    #[test]
    fn ExtractSpriteVerticesHandlesEmptySnapshot() {
        let snapshot = RenderSnapshot {
            Frame: 17,
            Items: Vec::new(),
        };

        let batch = ExtractSpriteVertices(&snapshot);

        assert_eq!(
            batch.Frame, 17,
            "frame should be preserved for empty snapshots"
        );
        assert_eq!(
            batch.Vertices.len(),
            0,
            "empty snapshot should extract zero vertices"
        );
    }

    #[test]
    fn ExtractSpriteVerticesMapsFieldsAndPreservesOrder() {
        let snapshot = RenderSnapshot {
            Frame: 22,
            Items: vec![
                RenderItem {
                    Entity: EntityId(3),
                    Position: Vec2 { X: 1.5, Y: -2.25 },
                    SpriteId: 99,
                },
                RenderItem {
                    Entity: EntityId(8),
                    Position: Vec2 { X: -4.0, Y: 7.0 },
                    SpriteId: 7,
                },
            ],
        };

        let batch = ExtractSpriteVertices(&snapshot);

        assert_eq!(batch.Frame, 22, "frame should be copied from snapshot");
        assert_eq!(
            batch.Vertices.len(),
            2,
            "two items should produce two vertices"
        );
        assert_eq!(
            batch.Vertices[0],
            SpriteVertex {
                X: 1.5,
                Y: -2.25,
                SpriteId: 99
            },
            "first item fields should map to first vertex"
        );
        assert_eq!(
            batch.Vertices[1],
            SpriteVertex {
                X: -4.0,
                Y: 7.0,
                SpriteId: 7
            },
            "second item fields should map to second vertex and preserve order"
        );
    }

    #[test]
    fn ExtractSpriteVerticesIsDeterministicAndDoesNotMutateSnapshot() {
        let snapshot = RenderSnapshot {
            Frame: 41,
            Items: vec![RenderItem {
                Entity: EntityId(0),
                Position: Vec2 { X: 3.0, Y: 9.0 },
                SpriteId: 2,
            }],
        };
        let before = snapshot.clone();

        let first = ExtractSpriteVertices(&snapshot);
        let second = ExtractSpriteVertices(&snapshot);

        assert_eq!(
            snapshot, before,
            "extraction should not mutate input snapshot"
        );
        assert_eq!(first, second, "repeated extraction should be deterministic");
    }

    #[test]
    fn PackSpriteVerticesUsesExpectedLittleEndianLayout() {
        let vertices = vec![
            SpriteVertex {
                X: 1.0,
                Y: -2.0,
                SpriteId: 0x11223344,
            },
            SpriteVertex {
                X: 0.5,
                Y: 4.0,
                SpriteId: 9,
            },
        ];

        let bytes = PackSpriteVertices(&vertices);

        assert_eq!(
            bytes.len(),
            vertices.len() * SpriteVertexStrideBytes(),
            "packed bytes should equal vertex_count * stride"
        );

        let expected_first = [
            0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0xc0, 0x44, 0x33, 0x22, 0x11,
        ];
        assert_eq!(
            &bytes[0..12],
            &expected_first,
            "first vertex bytes should be f32 LE x, f32 LE y, u32 LE sprite id"
        );

        let bytes_again = PackSpriteVertices(&vertices);
        assert_eq!(
            bytes, bytes_again,
            "packing should be deterministic across repeated calls"
        );
    }

    #[test]
    fn SpriteVertexLayoutMatchesPackingContract() {
        let layout = SpriteVertexBufferLayout();

        assert_eq!(
            layout.StrideBytes, 12,
            "sprite layout stride should be 12 bytes"
        );
        assert_eq!(
            layout.StepMode,
            VertexStepMode::Vertex,
            "sprite layout step mode should be per-vertex"
        );
        assert_eq!(
            layout.Attributes.len(),
            2,
            "sprite layout should expose exactly two attributes"
        );
        assert_eq!(layout.Attributes[0].Name, "Position");
        assert_eq!(layout.Attributes[0].Location, 0);
        assert_eq!(layout.Attributes[0].OffsetBytes, 0);
        assert_eq!(layout.Attributes[0].Format, VertexFormat::Float32x2);
        assert_eq!(layout.Attributes[1].Name, "SpriteId");
        assert_eq!(layout.Attributes[1].Location, 1);
        assert_eq!(layout.Attributes[1].OffsetBytes, 8);
        assert_eq!(layout.Attributes[1].Format, VertexFormat::Uint32);

        let packed = PackSpriteVertices(&[SpriteVertex {
            X: 10.0,
            Y: 20.0,
            SpriteId: 4,
        }]);
        assert_eq!(
            packed.len() as u64,
            layout.StrideBytes,
            "single packed vertex should consume exactly one layout stride"
        );
    }

    #[test]
    fn ExtractedRenderBatchVertexBytesMatchesPackHelper() {
        let snapshot = RenderSnapshot {
            Frame: 9,
            Items: vec![RenderItem {
                Entity: EntityId(2),
                Position: Vec2 { X: 3.25, Y: -7.5 },
                SpriteId: 42,
            }],
        };

        let batch = ExtractSpriteVertices(&snapshot);

        assert_eq!(
            batch.VertexBytes(),
            PackSpriteVertices(&batch.Vertices),
            "batch byte helper should match direct pack helper"
        );
    }
}
