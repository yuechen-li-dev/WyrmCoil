#![allow(non_snake_case)]

use crate::Engine::render::{
    RecordWgpuDrawCommand, RenderCommandPlan, RenderTargetLoadMode, WgpuDrawError, WgpuDrawOptions,
    WgpuDrawResources, WgpuHeadlessRenderTarget, WgpuRenderPipelineResource,
    WgpuVertexBufferResource,
};

pub struct HeadlessDrawSubmissionResources<'a> {
    pub Pipeline: &'a WgpuRenderPipelineResource,
    pub VertexBuffer: &'a WgpuVertexBufferResource,
    pub Target: &'a WgpuHeadlessRenderTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadlessDrawSubmissionOptions {
    pub Label: String,
    pub LoadMode: RenderTargetLoadMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessDrawSubmissionResult {
    pub Label: String,
    pub VertexCount: usize,
    pub SubmittedCommandBuffers: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlessDrawSubmissionError {
    EmptyLabel,
    Draw(WgpuDrawError),
}

pub fn ValidateHeadlessDrawSubmissionOptions(
    options: &HeadlessDrawSubmissionOptions,
) -> Result<(), HeadlessDrawSubmissionError> {
    if options.Label.trim().is_empty() {
        return Err(HeadlessDrawSubmissionError::EmptyLabel);
    }
    Ok(())
}

pub fn SubmitHeadlessDraw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: HeadlessDrawSubmissionResources<'_>,
    command: &RenderCommandPlan,
    options: &HeadlessDrawSubmissionOptions,
) -> Result<HeadlessDrawSubmissionResult, HeadlessDrawSubmissionError> {
    ValidateHeadlessDrawSubmissionOptions(options)?;

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some(&options.Label),
    });

    let draw = RecordWgpuDrawCommand(
        &mut encoder,
        WgpuDrawResources {
            Pipeline: resources.Pipeline,
            VertexBuffer: resources.VertexBuffer,
            TargetView: &resources.Target.View,
        },
        command,
        &WgpuDrawOptions {
            Label: options.Label.clone(),
            LoadMode: options.LoadMode,
        },
    )
    .map_err(HeadlessDrawSubmissionError::Draw)?;

    let command_buffer = encoder.finish();
    queue.submit(std::iter::once(command_buffer));

    Ok(HeadlessDrawSubmissionResult {
        Label: draw.Label,
        VertexCount: draw.VertexCount,
        SubmittedCommandBuffers: 1,
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
    fn ValidateHeadlessDrawSubmissionOptionsRejectsEmptyLabel() {
        let options = HeadlessDrawSubmissionOptions {
            Label: "  ".to_string(),
            LoadMode: RenderTargetLoadMode::Load,
        };

        assert_eq!(
            ValidateHeadlessDrawSubmissionOptions(&options).unwrap_err(),
            HeadlessDrawSubmissionError::EmptyLabel,
            "submission options should reject empty labels"
        );
    }

    #[test]
    fn ValidateHeadlessDrawSubmissionOptionsPreservesLoadModeAndLabel() {
        let options = HeadlessDrawSubmissionOptions {
            Label: "M34.Submit".to_string(),
            LoadMode: RenderTargetLoadMode::Clear(wgpu::Color::BLACK),
        };

        ValidateHeadlessDrawSubmissionOptions(&options)
            .expect("non-empty submission labels should validate");

        assert_eq!(options.Label, "M34.Submit", "label should be preserved");
        assert_eq!(
            options.LoadMode,
            RenderTargetLoadMode::Clear(wgpu::Color::BLACK),
            "load mode should be preserved"
        );
    }

    #[test]
    fn HeadlessDrawSubmissionResultIsDeterministicData() {
        let result = HeadlessDrawSubmissionResult {
            Label: "M34.Submit".to_string(),
            VertexCount: 3,
            SubmittedCommandBuffers: 1,
        };

        assert_eq!(result.Label, "M34.Submit", "label should be preserved");
        assert_eq!(result.VertexCount, 3, "vertex count should be preserved");
        assert_eq!(
            result.SubmittedCommandBuffers, 1,
            "headless submit helper should report exactly one submitted command buffer"
        );
    }

    #[test]
    fn CommandNotReadyMapsToSubmissionDrawError() {
        let mut command = ReadyCommand();
        command.Status = RenderCommandPlanStatus::Rejected;

        let draw_error = crate::Engine::render::ValidateWgpuDrawInputs(
            &command,
            &crate::Engine::render::WgpuVertexBufferResourceDesc {
                Label: "VB".to_string(),
                VertexCount: 3,
                StrideBytes: 12,
            },
            &crate::Engine::render::WgpuRenderPipelineResourceDesc {
                Name: "SpritePipeline".to_string(),
            },
        )
        .unwrap_err();

        let submission_error = HeadlessDrawSubmissionError::Draw(draw_error);
        assert_eq!(
            submission_error,
            HeadlessDrawSubmissionError::Draw(WgpuDrawError::CommandNotReady {
                Status: RenderCommandPlanStatus::Rejected
            }),
            "submission helper should preserve structured draw errors"
        );
    }
}
