#![allow(non_snake_case)]

use crate::Dunewyrm::{DwActRequest, SelectHighestUtilityTarget};
use crate::Engine::render::{
    CreateWgpuVertexBuffer, LifecycleUploadIntentActId, ValidateVertexBufferUploadPlan,
    VertexBufferUploadPlan, VertexBufferUploadPlanError, WgpuUploadError, WgpuVertexBufferResource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadExecutionMode {
    GpuBufferCreate,
    CpuRecordOnly,
    NoOpEmptyUpload,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadExecutionReason {
    GpuDeviceAvailable,
    NoDeviceCpuFallback,
    GpuDisabledCpuFallback,
    EmptyUploadNoOp,
    InvalidUploadPlan,
    MissingLifecycleUploadAct,
    CpuFallbackDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectedUploadExecutionReason {
    MissingDevice,
    GpuDisabled,
    CpuFallbackDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedUploadExecutionMode {
    pub Mode: UploadExecutionMode,
    pub Reason: RejectedUploadExecutionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadExecutionConstraints {
    pub AllowGpu: bool,
    pub AllowCpuRecordOnly: bool,
}

impl Default for UploadExecutionConstraints {
    fn default() -> Self {
        Self {
            AllowGpu: true,
            AllowCpuRecordOnly: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadExecutionPlan {
    pub Mode: UploadExecutionMode,
    pub Reason: UploadExecutionReason,
    pub RejectedModes: Vec<RejectedUploadExecutionMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuUploadRecord {
    pub Label: String,
    pub ByteCount: usize,
    pub VertexCount: usize,
    pub StrideBytes: u64,
}

pub struct UploadExecutionResult {
    pub Mode: UploadExecutionMode,
    pub Reason: UploadExecutionReason,
    pub RejectedModes: Vec<RejectedUploadExecutionMode>,
    pub CpuRecord: Option<CpuUploadRecord>,
    pub GpuResource: Option<WgpuVertexBufferResource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadExecutionError {
    MissingDevice,
    InvalidUploadPlan(VertexBufferUploadPlanError),
    WgpuUploadFailed(WgpuUploadError),
}

pub fn ContainsUploadIntent(acts: &[DwActRequest]) -> bool {
    let begin_stage = LifecycleUploadIntentActId();
    acts.iter().any(|x| x.Id == begin_stage)
}

pub fn PlanUploadExecution(
    acts: &[DwActRequest],
    upload: &VertexBufferUploadPlan,
    has_device: bool,
    constraints: UploadExecutionConstraints,
) -> UploadExecutionPlan {
    if ValidateVertexBufferUploadPlan(upload).is_err() {
        return UploadExecutionPlan {
            Mode: UploadExecutionMode::Rejected,
            Reason: UploadExecutionReason::InvalidUploadPlan,
            RejectedModes: Vec::new(),
        };
    }

    if upload.Bytes.is_empty() {
        return UploadExecutionPlan {
            Mode: UploadExecutionMode::NoOpEmptyUpload,
            Reason: UploadExecutionReason::EmptyUploadNoOp,
            RejectedModes: Vec::new(),
        };
    }

    if !ContainsUploadIntent(acts) {
        return UploadExecutionPlan {
            Mode: UploadExecutionMode::Rejected,
            Reason: UploadExecutionReason::MissingLifecycleUploadAct,
            RejectedModes: Vec::new(),
        };
    }

    let mut rejected = Vec::new();
    let mut scored_modes: Vec<(UploadExecutionMode, f32)> = Vec::new();
    let mut mode_reason: Vec<(UploadExecutionMode, UploadExecutionReason)> = Vec::new();

    if constraints.AllowGpu {
        if has_device {
            scored_modes.push((UploadExecutionMode::GpuBufferCreate, 1.0));
            mode_reason.push((
                UploadExecutionMode::GpuBufferCreate,
                UploadExecutionReason::GpuDeviceAvailable,
            ));
        } else {
            rejected.push(RejectedUploadExecutionMode {
                Mode: UploadExecutionMode::GpuBufferCreate,
                Reason: RejectedUploadExecutionReason::MissingDevice,
            });
        }
    } else {
        rejected.push(RejectedUploadExecutionMode {
            Mode: UploadExecutionMode::GpuBufferCreate,
            Reason: RejectedUploadExecutionReason::GpuDisabled,
        });
    }

    if constraints.AllowCpuRecordOnly {
        let reason = if constraints.AllowGpu {
            UploadExecutionReason::NoDeviceCpuFallback
        } else {
            UploadExecutionReason::GpuDisabledCpuFallback
        };
        scored_modes.push((UploadExecutionMode::CpuRecordOnly, 0.6));
        mode_reason.push((UploadExecutionMode::CpuRecordOnly, reason));
    } else {
        rejected.push(RejectedUploadExecutionMode {
            Mode: UploadExecutionMode::CpuRecordOnly,
            Reason: RejectedUploadExecutionReason::CpuFallbackDisabled,
        });
    }

    if let Some(selected) = SelectHighestUtilityTarget(&scored_modes) {
        return UploadExecutionPlan {
            Mode: selected.0,
            Reason: mode_reason
                .iter()
                .find(|x| x.0 == selected.0)
                .map(|x| x.1)
                .unwrap_or(UploadExecutionReason::CpuFallbackDisabled),
            RejectedModes: rejected,
        };
    }

    UploadExecutionPlan {
        Mode: UploadExecutionMode::Rejected,
        Reason: UploadExecutionReason::CpuFallbackDisabled,
        RejectedModes: rejected,
    }
}

pub fn ExecuteUploadPlan(
    plan: &UploadExecutionPlan,
    upload: &VertexBufferUploadPlan,
    device: Option<&wgpu::Device>,
) -> Result<UploadExecutionResult, UploadExecutionError> {
    ValidateVertexBufferUploadPlan(upload).map_err(UploadExecutionError::InvalidUploadPlan)?;

    match plan.Mode {
        UploadExecutionMode::GpuBufferCreate => {
            let Some(device_ref) = device else {
                return Err(UploadExecutionError::MissingDevice);
            };
            let resource = CreateWgpuVertexBuffer(device_ref, upload)
                .map_err(UploadExecutionError::WgpuUploadFailed)?;
            Ok(UploadExecutionResult {
                Mode: plan.Mode,
                Reason: plan.Reason,
                RejectedModes: plan.RejectedModes.clone(),
                CpuRecord: None,
                GpuResource: Some(resource),
            })
        }
        UploadExecutionMode::CpuRecordOnly => Ok(UploadExecutionResult {
            Mode: plan.Mode,
            Reason: plan.Reason,
            RejectedModes: plan.RejectedModes.clone(),
            CpuRecord: Some(CpuUploadRecord {
                Label: upload.Label.clone(),
                ByteCount: upload.Bytes.len(),
                VertexCount: upload.VertexCount,
                StrideBytes: upload.StrideBytes,
            }),
            GpuResource: None,
        }),
        UploadExecutionMode::NoOpEmptyUpload | UploadExecutionMode::Rejected => {
            Ok(UploadExecutionResult {
                Mode: plan.Mode,
                Reason: plan.Reason,
                RejectedModes: plan.RejectedModes.clone(),
                CpuRecord: None,
                GpuResource: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dunewyrm::{DwActId, DwActRequest};
    use crate::Engine::render::GpuBufferUsageIntent;

    fn Upload(bytes: usize) -> VertexBufferUploadPlan {
        VertexBufferUploadPlan {
            Label: "UploadLabel".to_string(),
            Bytes: vec![0; bytes],
            VertexCount: bytes / 12,
            StrideBytes: 12,
            Usage: GpuBufferUsageIntent::Vertex,
        }
    }

    #[test]
    fn ContainsUploadIntentOnlyAcceptsLifecycleUploadAct() {
        let upload_act = DwActRequest {
            Id: LifecycleUploadIntentActId(),
        };
        let unrelated = DwActRequest {
            Id: DwActId {
                Domain: 24025,
                Local: 77,
            },
        };

        assert!(ContainsUploadIntent(&[upload_act]));
        assert!(!ContainsUploadIntent(&[unrelated]));
    }

    #[test]
    fn NoUploadActRejectsWithMissingLifecycleReason() {
        let plan = PlanUploadExecution(
            &[],
            &Upload(12),
            false,
            UploadExecutionConstraints::default(),
        );
        assert_eq!(plan.Mode, UploadExecutionMode::Rejected);
        assert_eq!(
            plan.Reason,
            UploadExecutionReason::MissingLifecycleUploadAct
        );
    }

    #[test]
    fn EmptyUploadIsNoOp() {
        let acts = [DwActRequest {
            Id: LifecycleUploadIntentActId(),
        }];
        let plan = PlanUploadExecution(
            &acts,
            &Upload(0),
            false,
            UploadExecutionConstraints::default(),
        );
        assert_eq!(plan.Mode, UploadExecutionMode::NoOpEmptyUpload);
        assert_eq!(plan.Reason, UploadExecutionReason::EmptyUploadNoOp);

        let result =
            ExecuteUploadPlan(&plan, &Upload(0), None).expect("no-op execution should not fail");
        assert!(result.CpuRecord.is_none());
        assert!(result.GpuResource.is_none());
    }

    #[test]
    fn InvalidUploadRejectedBeforeExecution() {
        let acts = [DwActRequest {
            Id: LifecycleUploadIntentActId(),
        }];
        let invalid = VertexBufferUploadPlan {
            Label: "Bad".to_string(),
            Bytes: vec![1, 2],
            VertexCount: 1,
            StrideBytes: 12,
            Usage: GpuBufferUsageIntent::Vertex,
        };
        let plan = PlanUploadExecution(
            &acts,
            &invalid,
            false,
            UploadExecutionConstraints::default(),
        );
        assert_eq!(plan.Mode, UploadExecutionMode::Rejected);
        assert_eq!(plan.Reason, UploadExecutionReason::InvalidUploadPlan);
    }

    #[test]
    fn NoDeviceUsesCpuFallbackAndRecordsMetadata() {
        let acts = [DwActRequest {
            Id: LifecycleUploadIntentActId(),
        }];
        let upload = Upload(24);
        let plan =
            PlanUploadExecution(&acts, &upload, false, UploadExecutionConstraints::default());
        assert_eq!(plan.Mode, UploadExecutionMode::CpuRecordOnly);

        let result =
            ExecuteUploadPlan(&plan, &upload, None).expect("cpu fallback execution should succeed");
        let record = result.CpuRecord.expect("cpu record should exist");
        assert_eq!(record.Label, "UploadLabel");
        assert_eq!(record.ByteCount, 24);
        assert_eq!(record.VertexCount, 2);
        assert_eq!(record.StrideBytes, 12);
    }

    #[test]
    fn DeviceEligibleSelectsGpuAtPlanningLayer() {
        let acts = [DwActRequest {
            Id: LifecycleUploadIntentActId(),
        }];
        let upload = Upload(24);
        let plan = PlanUploadExecution(&acts, &upload, true, UploadExecutionConstraints::default());
        assert_eq!(plan.Mode, UploadExecutionMode::GpuBufferCreate);
        assert_eq!(plan.Reason, UploadExecutionReason::GpuDeviceAvailable);
    }

    #[test]
    fn GpuPlanExecutionWithoutDeviceReturnsStructuredError() {
        let acts = [DwActRequest {
            Id: LifecycleUploadIntentActId(),
        }];
        let upload = Upload(24);
        let plan = PlanUploadExecution(&acts, &upload, true, UploadExecutionConstraints::default());
        let result = ExecuteUploadPlan(&plan, &upload, None);
        assert!(result.is_err());
        match result {
            Err(err) => assert_eq!(err, UploadExecutionError::MissingDevice),
            Ok(_) => panic!("gpu mode without device should return missing-device error"),
        }
    }

    #[test]
    fn CpuFallbackDisabledRejectsWithoutDevice() {
        let acts = [DwActRequest {
            Id: LifecycleUploadIntentActId(),
        }];
        let upload = Upload(24);
        let constraints = UploadExecutionConstraints {
            AllowGpu: true,
            AllowCpuRecordOnly: false,
        };
        let plan = PlanUploadExecution(&acts, &upload, false, constraints);
        assert_eq!(plan.Mode, UploadExecutionMode::Rejected);
        assert_eq!(plan.Reason, UploadExecutionReason::CpuFallbackDisabled);
    }

    #[test]
    fn GpuDisabledFallsBackToCpuRecordOnly() {
        let acts = [DwActRequest {
            Id: LifecycleUploadIntentActId(),
        }];
        let upload = Upload(24);
        let constraints = UploadExecutionConstraints {
            AllowGpu: false,
            AllowCpuRecordOnly: true,
        };
        let plan = PlanUploadExecution(&acts, &upload, true, constraints);
        assert_eq!(plan.Mode, UploadExecutionMode::CpuRecordOnly);
        assert_eq!(plan.Reason, UploadExecutionReason::GpuDisabledCpuFallback);
    }

    #[test]
    fn UtilitySelectionPrefersGpuWhenBothFeasible() {
        let selected = SelectHighestUtilityTarget(&[
            (UploadExecutionMode::CpuRecordOnly, 0.6),
            (UploadExecutionMode::GpuBufferCreate, 1.0),
        ]);
        assert_eq!(selected, Some((UploadExecutionMode::GpuBufferCreate, 1.0)));
    }
}
