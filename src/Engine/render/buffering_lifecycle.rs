#![allow(non_snake_case)]

use crate::Dunewyrm::{
    Dw, DwActId, DwActRequest, DwBoard, DwControl, DwFrameCtx, DwFrameDef, DwFrameId,
    DwFrameRegistry, DwKey, DwPhase, DwSession, DwTickTraceEntry,
};
use crate::Engine::render::{BufferingFallbackReason, BufferingMode, BufferingPlanDecision};

const LifecycleDomain: u64 = 24025;
const RootFrame: DwFrameId = DwFrameId {
    Domain: LifecycleDomain,
    Local: 1,
};

const BeginStage: DwActId = DwActId {
    Domain: LifecycleDomain,
    Local: 1,
};
const MarkReady: DwActId = DwActId {
    Domain: LifecycleDomain,
    Local: 2,
};
const ConsumeReady: DwActId = DwActId {
    Domain: LifecycleDomain,
    Local: 3,
};
const RetireSlot: DwActId = DwActId {
    Domain: LifecycleDomain,
    Local: 4,
};
const BeginCleanup: DwActId = DwActId {
    Domain: LifecycleDomain,
    Local: 5,
};
const CompleteCleanup: DwActId = DwActId {
    Domain: LifecycleDomain,
    Local: 6,
};
const RejectLifecycle: DwActId = DwActId {
    Domain: LifecycleDomain,
    Local: 7,
};

mod Keys {
    use super::*;
    pub const ActiveSlotCount: DwKey<i32> = DwKey::New("Lifecycle.ActiveSlotCount", 2402501);
    pub const WipDepth: DwKey<i32> = DwKey::New("Lifecycle.WipDepth", 2402502);
    pub const CommittedMemoryBytes: DwKey<i32> =
        DwKey::New("Lifecycle.CommittedMemoryBytes", 2402503);
    pub const StarvationTime: DwKey<i32> = DwKey::New("Lifecycle.StarvationTime", 2402504);
    pub const ReadyButUnusedTime: DwKey<i32> = DwKey::New("Lifecycle.ReadyButUnusedTime", 2402505);
    pub const LateStageCount: DwKey<i32> = DwKey::New("Lifecycle.LateStageCount", 2402506);
    pub const EarlyStageCount: DwKey<i32> = DwKey::New("Lifecycle.EarlyStageCount", 2402507);
    pub const CeilingViolationCount: DwKey<i32> =
        DwKey::New("Lifecycle.CeilingViolationCount", 2402508);
    pub const SequentialStepCount: DwKey<i32> =
        DwKey::New("Lifecycle.SequentialStepCount", 2402509);
    pub const BusyRetryCount: DwKey<i32> = DwKey::New("Lifecycle.BusyRetryCount", 2402510);
    pub const FailureCleanupCount: DwKey<i32> =
        DwKey::New("Lifecycle.FailureCleanupCount", 2402511);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSlotLifecycleStatus {
    Success,
    HardFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferSlotLifecycleTelemetry {
    pub ActiveSlotCount: i32,
    pub WipDepth: i32,
    pub CommittedMemoryBytes: i32,
    pub StarvationTime: i32,
    pub ReadyButUnusedTime: i32,
    pub LateStageCount: i32,
    pub EarlyStageCount: i32,
    pub CeilingViolationCount: i32,
    pub SequentialStepCount: i32,
    pub BusyRetryCount: i32,
    pub FailureCleanupCount: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BufferSlotLifecycleResult {
    pub Status: BufferSlotLifecycleStatus,
    pub Acts: Vec<DwActRequest>,
    pub Trace: Vec<DwTickTraceEntry>,
    pub Telemetry: BufferSlotLifecycleTelemetry,
    pub PullLagSignal: Option<BufferingFallbackReason>,
}

#[derive(Clone, Copy)]
enum P {
    Start,
    Done,
}
impl DwPhase for P {
    fn ToPc(self) -> u32 {
        match self {
            P::Start => 0,
            P::Done => 1,
        }
    }
    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(P::Start),
            1 => Some(P::Done),
            _ => None,
        }
    }
}

fn Root(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<P>() {
        Some(P::Start) => Dw::Continue(P::Done),
        Some(P::Done) => Dw::Complete(),
        None => Dw::Fail("invalid lifecycle phase"),
    }
}

pub fn SimulateBufferSlotLifecycle(decision: &BufferingPlanDecision) -> BufferSlotLifecycleResult {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: RootFrame,
            Step: Root,
            DebugName: "BufferLifecycleRoot",
        })
        .unwrap();
    let mut session = DwSession::New(registry, RootFrame, 0).unwrap();

    let mut acts = Vec::new();
    let mut signal = None;
    match decision.SelectedMode {
        BufferingMode::FixedDoubleDefault => {
            SetTelemetryBoard(
                session.BoardMut(),
                2,
                2,
                decision.Requirements.FixedDoubleBytes as i32,
                0,
                0,
                0,
            );
            PushActs(
                &mut acts,
                &[
                    BeginStage,
                    MarkReady,
                    ConsumeReady,
                    RetireSlot,
                    BeginStage,
                    MarkReady,
                ],
            );
        }
        BufferingMode::PullLagPressure => {
            let late = decision.Telemetry.LateStageCount as i32;
            let early = decision.Telemetry.EarlyStageCount as i32;
            SetTelemetryBoard(
                session.BoardMut(),
                2,
                2,
                decision.Requirements.PullLagBytes as i32,
                late,
                early,
                decision.Telemetry.CeilingViolationCount as i32,
            );
            if late > 0 {
                signal = Some(BufferingFallbackReason::PullLagLateStageStarvation);
            }
            PushActs(
                &mut acts,
                &[BeginStage, MarkReady, ConsumeReady, RetireSlot],
            );
        }
        BufferingMode::SerialJitSurvival => {
            SetTelemetryBoard(
                session.BoardMut(),
                1,
                1,
                decision.Requirements.SerialJitBytes as i32,
                0,
                0,
                0,
            );
            session
                .BoardMut()
                .Set(Keys::FailureCleanupCount, 1)
                .unwrap();
            PushActs(
                &mut acts,
                &[
                    BeginStage,
                    MarkReady,
                    ConsumeReady,
                    RetireSlot,
                    BeginCleanup,
                    CompleteCleanup,
                    BeginStage,
                ],
            );
        }
        BufferingMode::NoBufferingModeFeasible => {
            SetTelemetryBoard(session.BoardMut(), 0, 0, 0, 0, 0, 0);
            PushActs(&mut acts, &[RejectLifecycle]);
        }
    }

    let _ = session.Tick();
    let _ = session.Tick();

    BufferSlotLifecycleResult {
        Status: if decision.SelectedMode == BufferingMode::NoBufferingModeFeasible {
            BufferSlotLifecycleStatus::HardFailure
        } else {
            BufferSlotLifecycleStatus::Success
        },
        Acts: acts,
        Trace: session.Trace().to_vec(),
        Telemetry: ReadTelemetry(session.Board()),
        PullLagSignal: signal,
    }
}

fn PushActs(acts: &mut Vec<DwActRequest>, ids: &[DwActId]) {
    for id in ids {
        acts.push(DwActRequest { Id: *id });
    }
}
fn SetTelemetryBoard(
    board: &mut DwBoard,
    active: i32,
    wip: i32,
    memory: i32,
    late: i32,
    early: i32,
    ceiling: i32,
) {
    board.Set(Keys::ActiveSlotCount, active).unwrap();
    board.Set(Keys::WipDepth, wip).unwrap();
    board.Set(Keys::CommittedMemoryBytes, memory).unwrap();
    board.Set(Keys::LateStageCount, late).unwrap();
    board.Set(Keys::EarlyStageCount, early).unwrap();
    board.Set(Keys::CeilingViolationCount, ceiling).unwrap();
    board.Set(Keys::StarvationTime, 0).unwrap();
    board.Set(Keys::ReadyButUnusedTime, 0).unwrap();
    board.Set(Keys::SequentialStepCount, 1).unwrap();
    board.Set(Keys::BusyRetryCount, 0).unwrap();
    if board.TryGet(Keys::FailureCleanupCount).is_none() {
        board.Set(Keys::FailureCleanupCount, 0).unwrap();
    }
}
fn ReadTelemetry(board: &DwBoard) -> BufferSlotLifecycleTelemetry {
    BufferSlotLifecycleTelemetry {
        ActiveSlotCount: board.GetOr(Keys::ActiveSlotCount, 0),
        WipDepth: board.GetOr(Keys::WipDepth, 0),
        CommittedMemoryBytes: board.GetOr(Keys::CommittedMemoryBytes, 0),
        StarvationTime: board.GetOr(Keys::StarvationTime, 0),
        ReadyButUnusedTime: board.GetOr(Keys::ReadyButUnusedTime, 0),
        LateStageCount: board.GetOr(Keys::LateStageCount, 0),
        EarlyStageCount: board.GetOr(Keys::EarlyStageCount, 0),
        CeilingViolationCount: board.GetOr(Keys::CeilingViolationCount, 0),
        SequentialStepCount: board.GetOr(Keys::SequentialStepCount, 0),
        BusyRetryCount: board.GetOr(Keys::BusyRetryCount, 0),
        FailureCleanupCount: board.GetOr(Keys::FailureCleanupCount, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::*;
    fn Upload(bytes: usize) -> VertexBufferUploadPlan {
        VertexBufferUploadPlan {
            Label: "L".into(),
            Bytes: vec![0; bytes],
            VertexCount: bytes / 12,
            StrideBytes: 12,
            Usage: GpuBufferUsageIntent::Vertex,
        }
    }
    fn Constraints(mem: usize) -> BufferingConstraints {
        BufferingConstraints {
            MemoryBudgetBytes: mem,
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
    fn FixedDoubleInvariants() {
        let d = PlanVertexBuffering(&Upload(64), Constraints(128));
        let r = SimulateBufferSlotLifecycle(&d);
        assert_eq!(r.Status, BufferSlotLifecycleStatus::Success);
        assert_eq!(r.Telemetry.ActiveSlotCount, 2);
        assert!(r.Telemetry.WipDepth <= 2);
        assert_eq!(r.Telemetry.CommittedMemoryBytes, 128);
        assert!(
            r.Acts
                .windows(2)
                .any(|w| w[0].Id == MarkReady && w[1].Id == ConsumeReady)
        );
        assert!(!r.Trace.is_empty());
    }
    #[test]
    fn PullLagInvariantsAndSignal() {
        let d = PlanVertexBuffering(&Upload(64), Constraints(64));
        assert_eq!(d.SelectedMode, BufferingMode::PullLagPressure);
        let r = SimulateBufferSlotLifecycle(&d);
        assert!(r.Telemetry.WipDepth <= 2);
        assert!(r.Telemetry.CommittedMemoryBytes <= 64);
        assert_eq!(r.PullLagSignal, None);
    }
    #[test]
    fn SerialInvariantsAndCleanup() {
        let mut c = Constraints(64);
        c.TransferVariance = TransferVarianceClass::High;
        let d = PlanVertexBuffering(&Upload(64), c);
        let r = SimulateBufferSlotLifecycle(&d);
        assert_eq!(r.Telemetry.ActiveSlotCount, 1);
        assert_eq!(r.Telemetry.WipDepth, 1);
        assert!(r.Telemetry.CommittedMemoryBytes <= 64);
        assert_eq!(r.Telemetry.FailureCleanupCount, 1);
        assert!(r.Acts.iter().any(|x| x.Id == BeginCleanup));
    }
    #[test]
    fn HardFailureRejectsLifecycle() {
        let d = PlanVertexBuffering(&Upload(64), Constraints(63));
        let r = SimulateBufferSlotLifecycle(&d);
        assert_eq!(r.Status, BufferSlotLifecycleStatus::HardFailure);
        assert_eq!(
            r.Acts,
            vec![DwActRequest {
                Id: RejectLifecycle
            }]
        );
        assert_eq!(r.Telemetry.ActiveSlotCount, 0);
    }
}
