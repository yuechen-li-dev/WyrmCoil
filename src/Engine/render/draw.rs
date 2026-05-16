#![allow(non_snake_case)]

use crate::Engine::render::{
    RenderCommandPlan, RenderCommandPlanStatus, WgpuRenderPipelineResource,
    WgpuVertexBufferResource,
};

#[derive(Debug, Clone, PartialEq)]
pub struct WgpuDrawOptions {
    pub Label: String,
    pub LoadMode: RenderTargetLoadMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderTargetLoadMode {
    Load,
    Clear(wgpu::Color),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuDrawCommandResult {
    pub Label: String,
    pub VertexCount: usize,
    pub InstanceCount: usize,
}

pub struct WgpuDrawResources<'a> {
    pub Pipeline: &'a WgpuRenderPipelineResource,
    pub VertexBuffer: &'a WgpuVertexBufferResource,
    pub TargetView: &'a wgpu::TextureView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuDrawError {
    CommandNotReady {
        Status: RenderCommandPlanStatus,
    },
    EmptyDraw,
    MissingGpuBuffer,
    VertexCountTooLarge {
        VertexCount: usize,
    },
    VertexCountExceedsBuffer {
        CommandVertexCount: usize,
        BufferVertexCount: usize,
    },
    StrideMismatch {
        CommandStrideBytes: u64,
        BufferStrideBytes: u64,
    },
    PipelineMismatch {
        CommandPipelineName: String,
        ResourcePipelineName: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuVertexBufferResourceDesc {
    pub Label: String,
    pub VertexCount: usize,
    pub StrideBytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuRenderPipelineResourceDesc {
    pub Name: String,
}

pub fn ValidateWgpuDrawInputs(
    command: &RenderCommandPlan,
    vertex: &WgpuVertexBufferResourceDesc,
    pipeline: &WgpuRenderPipelineResourceDesc,
) -> Result<WgpuDrawCommandResult, WgpuDrawError> {
    if command.Status != RenderCommandPlanStatus::ReadyToDraw {
        return Err(WgpuDrawError::CommandNotReady {
            Status: command.Status,
        });
    }
    if command.VertexCount == 0 {
        return Err(WgpuDrawError::EmptyDraw);
    }
    if command.VertexCount > u32::MAX as usize {
        return Err(WgpuDrawError::VertexCountTooLarge {
            VertexCount: command.VertexCount,
        });
    }
    if !command.UsesGpuBuffer {
        return Err(WgpuDrawError::MissingGpuBuffer);
    }
    if command.VertexCount > vertex.VertexCount {
        return Err(WgpuDrawError::VertexCountExceedsBuffer {
            CommandVertexCount: command.VertexCount,
            BufferVertexCount: vertex.VertexCount,
        });
    }
    if command.VertexStrideBytes != vertex.StrideBytes {
        return Err(WgpuDrawError::StrideMismatch {
            CommandStrideBytes: command.VertexStrideBytes,
            BufferStrideBytes: vertex.StrideBytes,
        });
    }
    if command.PipelineName != pipeline.Name {
        return Err(WgpuDrawError::PipelineMismatch {
            CommandPipelineName: command.PipelineName.clone(),
            ResourcePipelineName: pipeline.Name.clone(),
        });
    }

    Ok(WgpuDrawCommandResult {
        Label: command.Name.clone(),
        VertexCount: command.VertexCount,
        InstanceCount: 1,
    })
}

pub fn RecordWgpuDrawCommand(
    encoder: &mut wgpu::CommandEncoder,
    resources: WgpuDrawResources<'_>,
    command: &RenderCommandPlan,
    options: &WgpuDrawOptions,
) -> Result<WgpuDrawCommandResult, WgpuDrawError> {
    let validate_result = ValidateWgpuDrawInputs(
        command,
        &WgpuVertexBufferResourceDesc {
            Label: resources.VertexBuffer.Label.clone(),
            VertexCount: resources.VertexBuffer.VertexCount,
            StrideBytes: resources.VertexBuffer.StrideBytes,
        },
        &WgpuRenderPipelineResourceDesc {
            Name: resources.Pipeline.Name.clone(),
        },
    )?;

    let color_ops = match options.LoadMode {
        RenderTargetLoadMode::Load => wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
        RenderTargetLoadMode::Clear(clear) => wgpu::Operations {
            load: wgpu::LoadOp::Clear(clear),
            store: wgpu::StoreOp::Store,
        },
    };

    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(&options.Label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: resources.TargetView,
            resolve_target: None,
            ops: color_ops,
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
    });

    render_pass.set_pipeline(&resources.Pipeline.Pipeline);
    render_pass.set_vertex_buffer(0, resources.VertexBuffer.Buffer.slice(..));
    render_pass.draw(0..validate_result.VertexCount as u32, 0..1);

    Ok(WgpuDrawCommandResult {
        Label: options.Label.clone(),
        VertexCount: validate_result.VertexCount,
        InstanceCount: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::{
        RenderCommandPlan, RenderCommandPlanReason, RenderCommandPlanStatus,
    };

    fn ReadyCommand() -> RenderCommandPlan {
        RenderCommandPlan {
            Name: "MainDraw".to_string(),
            Status: RenderCommandPlanStatus::ReadyToDraw,
            Reason: RenderCommandPlanReason::Ready,
            PipelineName: "SpritePipeline".to_string(),
            VertexCount: 3,
            VertexStrideBytes: 12,
            VertexByteCount: 36,
            UsesGpuBuffer: true,
            UsesCpuRecord: false,
        }
    }

    #[test]
    fn ValidateWgpuDrawInputsAcceptsReadyMatchingMetadata() {
        let command = ReadyCommand();
        let result = ValidateWgpuDrawInputs(
            &command,
            &WgpuVertexBufferResourceDesc {
                Label: "SpriteVB".to_string(),
                VertexCount: 3,
                StrideBytes: 12,
            },
            &WgpuRenderPipelineResourceDesc {
                Name: "SpritePipeline".to_string(),
            },
        )
        .expect("ready draw with matching metadata should validate");

        assert_eq!(
            result.Label, "MainDraw",
            "validation result should preserve command label"
        );
        assert_eq!(
            result.VertexCount, 3,
            "validated draw should preserve vertex count"
        );
        assert_eq!(
            result.InstanceCount, 1,
            "minimal M31 draw should use one instance"
        );
    }

    #[test]
    fn ValidateWgpuDrawInputsRejectsNotDrawablePlansAndMismatches() {
        let mut command = ReadyCommand();
        command.Status = RenderCommandPlanStatus::NoOpEmptyBatch;
        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: 3,
                    StrideBytes: 12
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "SpritePipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::CommandNotReady {
                Status: RenderCommandPlanStatus::NoOpEmptyBatch
            },
            "no-op command plans should not be drawable"
        );

        command = ReadyCommand();
        command.Status = RenderCommandPlanStatus::Rejected;
        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: 3,
                    StrideBytes: 12
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "SpritePipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::CommandNotReady {
                Status: RenderCommandPlanStatus::Rejected
            },
            "rejected command plans should not be drawable"
        );

        command = ReadyCommand();
        command.VertexCount = 0;
        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: 3,
                    StrideBytes: 12
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "SpritePipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::EmptyDraw,
            "zero-vertex draws should be rejected"
        );

        command = ReadyCommand();
        command.UsesGpuBuffer = false;
        command.UsesCpuRecord = true;
        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: 3,
                    StrideBytes: 12
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "SpritePipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::MissingGpuBuffer,
            "cpu-record-only command plans should be rejected for real draw recording"
        );

        command = ReadyCommand();
        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: 2,
                    StrideBytes: 12
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "SpritePipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::VertexCountExceedsBuffer {
                CommandVertexCount: 3,
                BufferVertexCount: 2
            },
            "command vertex count must fit in available vertex buffer vertices"
        );

        command = ReadyCommand();
        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: 3,
                    StrideBytes: 16
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "SpritePipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::StrideMismatch {
                CommandStrideBytes: 12,
                BufferStrideBytes: 16
            },
            "stride mismatch should reject draw recording"
        );

        command = ReadyCommand();
        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: 3,
                    StrideBytes: 12
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "OtherPipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::PipelineMismatch {
                CommandPipelineName: "SpritePipeline".to_string(),
                ResourcePipelineName: "OtherPipeline".to_string()
            },
            "pipeline name mismatch should be rejected"
        );
    }

    #[test]
    fn ValidateWgpuDrawInputsRejectsTooLargeVertexCount() {
        let mut command = ReadyCommand();
        command.VertexCount = (u32::MAX as usize) + 1;

        assert_eq!(
            ValidateWgpuDrawInputs(
                &command,
                &WgpuVertexBufferResourceDesc {
                    Label: "VB".to_string(),
                    VertexCount: command.VertexCount,
                    StrideBytes: 12
                },
                &WgpuRenderPipelineResourceDesc {
                    Name: "SpritePipeline".to_string()
                }
            )
            .unwrap_err(),
            WgpuDrawError::VertexCountTooLarge {
                VertexCount: (u32::MAX as usize) + 1
            },
            "draw helper should reject vertex counts that do not fit u32 draw ranges"
        );
    }
}
