#![allow(non_snake_case)]

use crate::Dunewyrm::SelectHighestUtilityTarget;
use crate::Engine::render::VertexBufferUploadPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferingMode {
    FixedDoubleDefault,
    PullLagPressure,
    SerialJitSurvival,
    NoBufferingModeFeasible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferingTransitionReason {
    DefaultFeasible,
    MemoryPressure,
    PullLagSafetyBreach,
    SerialSurvival,
    HardFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferingFallbackReason {
    PullLagLateStageStarvation,
    PullLagMemoryEdgeRejected,
    PullLagVarianceMiss,
    PullLagComputeUnstable,
    PullLagWipWasteExceeded,
    FixedDoubleMemoryInfeasible,
    PullLagMemoryInfeasible,
    SerialMemoryInfeasible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferingHardFailureReason {
    NoBufferingModeFeasible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferVarianceClass {
    Low,
    Moderate,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputePredictabilityClass {
    Stable,
    Drifting,
    Unstable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PullLagPressureTelemetry {
    pub ReadyButUnusedTime: u64,
    pub StarvationTime: u64,
    pub LateStageCount: u32,
    pub EarlyStageCount: u32,
    pub CeilingViolationCount: u32,
    pub WipWasteExceeded: bool,
    pub MemoryEdgeRejected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferingConstraints {
    pub MemoryBudgetBytes: usize,
    pub TransferVariance: TransferVarianceClass,
    pub ComputePredictability: ComputePredictabilityClass,
    pub PullLagTelemetry: PullLagPressureTelemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferingMemoryRequirements {
    pub FixedDoubleBytes: usize,
    pub PullLagBytes: usize,
    pub SerialJitBytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferingFeasibility {
    pub FixedDoubleFeasible: bool,
    pub PullLagFeasible: bool,
    pub SerialJitFeasible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferingPlannerTelemetry {
    pub CommittedMemoryBytes: usize,
    pub ActiveSlotCount: u32,
    pub WipDepth: u32,
    pub ReadyButUnusedTime: u64,
    pub StarvationTime: u64,
    pub LateStageCount: u32,
    pub EarlyStageCount: u32,
    pub CeilingViolationCount: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedBufferingMode {
    pub Mode: BufferingMode,
    pub Reason: BufferingFallbackReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferingPlanDecision {
    pub SelectedMode: BufferingMode,
    pub TransitionReason: BufferingTransitionReason,
    pub RejectedModes: Vec<RejectedBufferingMode>,
    pub Feasibility: BufferingFeasibility,
    pub Requirements: BufferingMemoryRequirements,
    pub FallbackReason: Option<BufferingFallbackReason>,
    pub HardFailureReason: Option<BufferingHardFailureReason>,
    pub Telemetry: BufferingPlannerTelemetry,
}

pub fn PlanVertexBuffering(
    upload: &VertexBufferUploadPlan,
    constraints: BufferingConstraints,
) -> BufferingPlanDecision {
    let upload_bytes = upload.Bytes.len();
    let fixed_double_bytes = upload_bytes.checked_mul(2).unwrap_or(usize::MAX);
    let pull_lag_bytes = upload_bytes;
    let serial_bytes = upload_bytes;

    let requirements = BufferingMemoryRequirements {
        FixedDoubleBytes: fixed_double_bytes,
        PullLagBytes: pull_lag_bytes,
        SerialJitBytes: serial_bytes,
    };

    let fixed_double_feasible = constraints.MemoryBudgetBytes >= fixed_double_bytes;
    let serial_feasible = constraints.MemoryBudgetBytes >= serial_bytes;

    let mut rejected = Vec::new();
    let pull_lag_safe = PullLagSafetyReason(&constraints).is_none();
    let pull_lag_feasible = constraints.MemoryBudgetBytes >= pull_lag_bytes && pull_lag_safe;

    if !fixed_double_feasible {
        rejected.push(RejectedBufferingMode {
            Mode: BufferingMode::FixedDoubleDefault,
            Reason: BufferingFallbackReason::FixedDoubleMemoryInfeasible,
        });
    }

    if !pull_lag_feasible {
        let reason = if constraints.MemoryBudgetBytes < pull_lag_bytes {
            BufferingFallbackReason::PullLagMemoryInfeasible
        } else {
            PullLagSafetyReason(&constraints)
                .unwrap_or(BufferingFallbackReason::PullLagMemoryEdgeRejected)
        };
        rejected.push(RejectedBufferingMode {
            Mode: BufferingMode::PullLagPressure,
            Reason: reason,
        });
    }

    if !serial_feasible {
        rejected.push(RejectedBufferingMode {
            Mode: BufferingMode::SerialJitSurvival,
            Reason: BufferingFallbackReason::SerialMemoryInfeasible,
        });
    }

    let feasibility = BufferingFeasibility {
        FixedDoubleFeasible: fixed_double_feasible,
        PullLagFeasible: pull_lag_feasible,
        SerialJitFeasible: serial_feasible,
    };

    let mut scored_modes = Vec::new();
    if fixed_double_feasible {
        scored_modes.push((BufferingMode::FixedDoubleDefault, 1.0_f32));
    }
    if pull_lag_feasible {
        scored_modes.push((BufferingMode::PullLagPressure, 0.7_f32));
    }
    if serial_feasible {
        scored_modes.push((BufferingMode::SerialJitSurvival, 0.3_f32));
    }

    let selected = SelectHighestUtilityTarget(&scored_modes).map(|entry| entry.0);

    match selected {
        Some(BufferingMode::FixedDoubleDefault) => Decision(
            BufferingMode::FixedDoubleDefault,
            BufferingTransitionReason::DefaultFeasible,
            rejected,
            feasibility,
            requirements,
            None,
            None,
            if upload_bytes == 0 { 0 } else { 2 },
            if upload_bytes == 0 { 0 } else { 2 },
            if upload_bytes == 0 { 0 } else { fixed_double_bytes },
            constraints.PullLagTelemetry,
        ),
        Some(BufferingMode::PullLagPressure) => Decision(
            BufferingMode::PullLagPressure,
            BufferingTransitionReason::MemoryPressure,
            rejected,
            feasibility,
            requirements,
            Some(BufferingFallbackReason::FixedDoubleMemoryInfeasible),
            None,
            1,
            2,
            pull_lag_bytes,
            constraints.PullLagTelemetry,
        ),
        Some(BufferingMode::SerialJitSurvival) => {
            let fallback = if pull_lag_safe {
                BufferingFallbackReason::PullLagMemoryInfeasible
            } else {
                PullLagSafetyReason(&constraints)
                    .unwrap_or(BufferingFallbackReason::PullLagMemoryEdgeRejected)
            };
            Decision(
                BufferingMode::SerialJitSurvival,
                BufferingTransitionReason::SerialSurvival,
                rejected,
                feasibility,
                requirements,
                Some(fallback),
                None,
                1,
                1,
                serial_bytes,
                constraints.PullLagTelemetry,
            )
        }
        _ => Decision(
            BufferingMode::NoBufferingModeFeasible,
            BufferingTransitionReason::HardFailure,
            rejected,
            feasibility,
            requirements,
            Some(BufferingFallbackReason::SerialMemoryInfeasible),
            Some(BufferingHardFailureReason::NoBufferingModeFeasible),
            0,
            0,
            0,
            constraints.PullLagTelemetry,
        ),
    }
}

fn Decision(
    mode: BufferingMode,
    reason: BufferingTransitionReason,
    rejected: Vec<RejectedBufferingMode>,
    feasibility: BufferingFeasibility,
    requirements: BufferingMemoryRequirements,
    fallback: Option<BufferingFallbackReason>,
    hard: Option<BufferingHardFailureReason>,
    active_slots: u32,
    wip_depth: u32,
    committed_memory: usize,
    pull: PullLagPressureTelemetry,
) -> BufferingPlanDecision {
    BufferingPlanDecision {
        SelectedMode: mode,
        TransitionReason: reason,
        RejectedModes: rejected,
        Feasibility: feasibility,
        Requirements: requirements,
        FallbackReason: fallback,
        HardFailureReason: hard,
        Telemetry: BufferingPlannerTelemetry {
            CommittedMemoryBytes: committed_memory,
            ActiveSlotCount: active_slots,
            WipDepth: wip_depth,
            ReadyButUnusedTime: pull.ReadyButUnusedTime,
            StarvationTime: pull.StarvationTime,
            LateStageCount: pull.LateStageCount,
            EarlyStageCount: pull.EarlyStageCount,
            CeilingViolationCount: pull.CeilingViolationCount,
        },
    }
}

fn PullLagSafetyReason(constraints: &BufferingConstraints) -> Option<BufferingFallbackReason> {
    if constraints.PullLagTelemetry.MemoryEdgeRejected {
        return Some(BufferingFallbackReason::PullLagMemoryEdgeRejected);
    }
    if constraints.TransferVariance == TransferVarianceClass::High {
        return Some(BufferingFallbackReason::PullLagVarianceMiss);
    }
    if constraints.ComputePredictability != ComputePredictabilityClass::Stable {
        return Some(BufferingFallbackReason::PullLagComputeUnstable);
    }
    if constraints.PullLagTelemetry.StarvationTime > 0
        || constraints.PullLagTelemetry.LateStageCount > 0
    {
        return Some(BufferingFallbackReason::PullLagLateStageStarvation);
    }
    if constraints.PullLagTelemetry.WipWasteExceeded {
        return Some(BufferingFallbackReason::PullLagWipWasteExceeded);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::{GpuBufferUsageIntent, VertexBufferUploadPlan};

    fn Upload(bytes: usize) -> VertexBufferUploadPlan {
        VertexBufferUploadPlan {
            Label: "Plan".to_string(),
            Bytes: vec![0; bytes],
            VertexCount: bytes / 12,
            StrideBytes: 12,
            Usage: GpuBufferUsageIntent::Vertex,
        }
    }

    fn SafeConstraints(memory: usize) -> BufferingConstraints {
        BufferingConstraints {
            MemoryBudgetBytes: memory,
            TransferVariance: TransferVarianceClass::Low,
            ComputePredictability: ComputePredictabilityClass::Stable,
            PullLagTelemetry: PullLagPressureTelemetry {
                ReadyButUnusedTime: 0,
                StarvationTime: 0,
                LateStageCount: 0,
                EarlyStageCount: 0,
                CeilingViolationCount: 0,
                WipWasteExceeded: false,
                MemoryEdgeRejected: false,
            },
        }
    }

    #[test]
    fn FixedDoubleSelectedWhenFeasible() {
        let d = PlanVertexBuffering(&Upload(64), SafeConstraints(128));
        assert_eq!(
            d.SelectedMode,
            BufferingMode::FixedDoubleDefault,
            "fixed-double should win whenever feasible"
        );
    }
    #[test]
    fn PullLagSelectedWhenFixedDoubleInfeasibleButSafe() {
        let d = PlanVertexBuffering(&Upload(64), SafeConstraints(64));
        assert_eq!(
            d.SelectedMode,
            BufferingMode::PullLagPressure,
            "pull-lag should be selected under memory pressure when safe"
        );
    }
    #[test]
    fn SerialSelectedWhenPullLagUnsafe() {
        let mut c = SafeConstraints(64);
        c.TransferVariance = TransferVarianceClass::High;
        let d = PlanVertexBuffering(&Upload(64), c);
        assert_eq!(
            d.SelectedMode,
            BufferingMode::SerialJitSurvival,
            "serial should be selected when pull-lag is unsafe"
        );
        assert!(
            d.Telemetry.ActiveSlotCount <= 1,
            "serial must keep active slot count <= 1"
        );
        assert!(d.Telemetry.WipDepth <= 1, "serial must keep WIP depth <= 1");
        assert!(
            d.Telemetry.CommittedMemoryBytes <= 64,
            "serial committed memory must stay within one upload slot"
        );
    }
    #[test]
    fn HardFailureWhenAllModesInfeasible() {
        let d = PlanVertexBuffering(&Upload(64), SafeConstraints(63));
        assert_eq!(
            d.SelectedMode,
            BufferingMode::NoBufferingModeFeasible,
            "hard failure mode should be selected when all modes are infeasible"
        );
        assert_eq!(
            d.HardFailureReason,
            Some(BufferingHardFailureReason::NoBufferingModeFeasible),
            "hard failure reason should be structured"
        );
    }
    #[test]
    fn PullLagRejectionReasonsAreStructured() {
        let mut variance = SafeConstraints(64);
        variance.TransferVariance = TransferVarianceClass::High;
        let d1 = PlanVertexBuffering(&Upload(64), variance);
        assert!(
            d1.RejectedModes
                .iter()
                .any(|x| x.Mode == BufferingMode::PullLagPressure
                    && x.Reason == BufferingFallbackReason::PullLagVarianceMiss),
            "high transfer variance should reject pull-lag with PullLagVarianceMiss"
        );

        let mut compute = SafeConstraints(64);
        compute.ComputePredictability = ComputePredictabilityClass::Unstable;
        let d2 = PlanVertexBuffering(&Upload(64), compute);
        assert!(
            d2.RejectedModes
                .iter()
                .any(|x| x.Reason == BufferingFallbackReason::PullLagComputeUnstable),
            "unstable compute should reject pull-lag with PullLagComputeUnstable"
        );

        let mut late = SafeConstraints(64);
        late.PullLagTelemetry.LateStageCount = 1;
        let d3 = PlanVertexBuffering(&Upload(64), late);
        assert!(
            d3.RejectedModes
                .iter()
                .any(|x| x.Reason == BufferingFallbackReason::PullLagLateStageStarvation),
            "late-stage starvation should reject pull-lag"
        );
    }

    #[test]
    fn EmptyUploadIsNoOpFixedDouble() {
        let d = PlanVertexBuffering(&Upload(0), SafeConstraints(0));
        assert_eq!(
            d.SelectedMode,
            BufferingMode::FixedDoubleDefault,
            "empty uploads should be a no-op success under default mode"
        );
        assert_eq!(
            d.Telemetry.CommittedMemoryBytes, 0,
            "empty uploads should commit zero bytes"
        );
        assert_eq!(
            d.Telemetry.ActiveSlotCount, 0,
            "empty uploads should have zero active slots"
        );
    }

    #[test]
    fn UsesDunewyrmUtilitySelectionHelper() {
        let winner = SelectHighestUtilityTarget(&[
            (BufferingMode::PullLagPressure, 0.7),
            (BufferingMode::FixedDoubleDefault, 1.0),
        ]);
        assert_eq!(
            winner,
            Some((BufferingMode::FixedDoubleDefault, 1.0)),
            "planner selection should flow through Dunewyrm utility max-score helper"
        );
    }
}
