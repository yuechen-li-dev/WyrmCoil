#![allow(non_snake_case)]

use crate::Dunewyrm::{
    Dw, DwActId, DwActRequest, DwBoard, DwControl, DwFrameCtx, DwFrameDef, DwFrameId,
    DwFrameRegistry, DwKey, DwPhase, DwRunStatus, DwRuntimeChunk, DwSession, DwTickTraceEntry,
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

pub fn LifecycleUploadIntentActId() -> DwActId {
    BeginStage
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct BufferSlotLifecycleChunk {
    pub Decision: BufferingPlanDecision,
    pub Runtime: DwRuntimeChunk,
    pub Telemetry: BufferSlotLifecycleTelemetry,
    pub Acts: Vec<DwActRequest>,
    pub Status: BufferSlotLifecycleStatus,
    pub PullLagSignal: Option<BufferingFallbackReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferSlotLifecycleRestoreError {
    InvalidChunk { Message: String },
    RuntimeRestoreFailed { Message: String },
}

pub struct BufferSlotLifecycleSession {
    Decision: BufferingPlanDecision,
    Session: DwSession,
    Acts: Vec<DwActRequest>,
    Status: BufferSlotLifecycleStatus,
    PullLagSignal: Option<BufferingFallbackReason>,
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
    let mut lifecycle = BufferSlotLifecycleSession::New(decision.clone());
    while lifecycle.Tick() {}
    lifecycle.Result()
}

impl BufferSlotLifecycleSession {
    pub fn New(decision: BufferingPlanDecision) -> Self {
        let mut session = DwSession::New(LifecycleRegistry(), RootFrame, 0).unwrap();
        let (acts, signal, status) = PrimeLifecycle(&mut session, &decision);
        Self {
            Decision: decision,
            Session: session,
            Acts: acts,
            Status: status,
            PullLagSignal: signal,
        }
    }

    pub fn Tick(&mut self) -> bool {
        match self.Session.Tick() {
            Ok(result) => {
                result.Status == DwRunStatus::Running || result.Status == DwRunStatus::Waiting
            }
            Err(_) => false,
        }
    }

    pub fn ExportChunk(&self) -> BufferSlotLifecycleChunk {
        BufferSlotLifecycleChunk {
            Decision: self.Decision.clone(),
            Runtime: self.Session.ExportChunk(),
            Telemetry: ReadTelemetry(self.Session.Board()),
            Acts: self.Acts.clone(),
            Status: self.Status,
            PullLagSignal: self.PullLagSignal,
        }
    }

    pub fn FromChunk(
        chunk: BufferSlotLifecycleChunk,
    ) -> Result<Self, BufferSlotLifecycleRestoreError> {
        if chunk.Runtime.Stack.is_empty() {
            return Err(BufferSlotLifecycleRestoreError::InvalidChunk {
                Message: "runtime stack cannot be empty".to_string(),
            });
        }
        let session = DwSession::FromChunk(LifecycleRegistry(), chunk.Runtime).map_err(|x| {
            BufferSlotLifecycleRestoreError::RuntimeRestoreFailed {
                Message: x.to_string(),
            }
        })?;
        Ok(Self {
            Decision: chunk.Decision,
            Session: session,
            Acts: chunk.Acts,
            Status: chunk.Status,
            PullLagSignal: chunk.PullLagSignal,
        })
    }

    pub fn Trace(&self) -> &[DwTickTraceEntry] {
        self.Session.Trace()
    }

    pub fn Result(&self) -> BufferSlotLifecycleResult {
        BufferSlotLifecycleResult {
            Status: self.Status,
            Acts: self.Acts.clone(),
            Trace: self.Trace().to_vec(),
            Telemetry: ReadTelemetry(self.Session.Board()),
            PullLagSignal: self.PullLagSignal,
        }
    }
}

fn LifecycleRegistry() -> DwFrameRegistry {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: RootFrame,
            Step: Root,
            DebugName: "BufferLifecycleRoot",
        })
        .unwrap();
    registry
}

fn PrimeLifecycle(
    session: &mut DwSession,
    decision: &BufferingPlanDecision,
) -> (
    Vec<DwActRequest>,
    Option<BufferingFallbackReason>,
    BufferSlotLifecycleStatus,
) {
    let mut acts = Vec::new();
    let mut signal = None;
    let mut status = BufferSlotLifecycleStatus::Success;
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
            status = BufferSlotLifecycleStatus::HardFailure;
        }
    }
    (acts, signal, status)
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
    use crate::Dunewyrm::{CompareTrace, FormatComparison};
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

    fn ReplayResult(
        decision: BufferingPlanDecision,
        partial_ticks: usize,
    ) -> (
        BufferSlotLifecycleResult,
        BufferSlotLifecycleResult,
        Vec<DwTickTraceEntry>,
    ) {
        let mut uninterrupted = BufferSlotLifecycleSession::New(decision.clone());
        while uninterrupted.Tick() {}
        let full = uninterrupted.Result();

        let mut split = BufferSlotLifecycleSession::New(decision);
        for _ in 0..partial_ticks {
            let _ = split.Tick();
        }
        let mut combined_trace = split.Trace().to_vec();
        let chunk = split.ExportChunk();
        let mut restored =
            BufferSlotLifecycleSession::FromChunk(chunk).expect("restore should succeed");
        while restored.Tick() {}
        combined_trace.extend_from_slice(restored.Trace());
        (full, restored.Result(), combined_trace)
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
    fn FixedDoubleReplayMatchesUninterrupted() {
        let d = PlanVertexBuffering(&Upload(64), Constraints(128));
        let (full, resumed, combined_trace) = ReplayResult(d, 1);
        assert_eq!(full.Status, resumed.Status, "fixed status should match");
        assert_eq!(
            full.Telemetry, resumed.Telemetry,
            "fixed telemetry should match"
        );
        assert_eq!(full.Acts, resumed.Acts, "fixed acts should match");
        let trace = CompareTrace(&full.Trace, &combined_trace);
        assert!(
            trace.Matches,
            "fixed trace mismatch: {}",
            FormatComparison(&trace)
        );
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
    fn PullLagReplayMatchesUninterrupted() {
        let mut c = Constraints(64);
        c.TransferVariance = TransferVarianceClass::Moderate;
        let d = PlanVertexBuffering(&Upload(64), c);
        assert_eq!(d.SelectedMode, BufferingMode::PullLagPressure);
        let (full, resumed, _) = ReplayResult(d, 1);
        assert_eq!(full.Status, resumed.Status);
        assert_eq!(full.Telemetry, resumed.Telemetry);
        assert_eq!(full.Acts, resumed.Acts);
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
    fn SerialReplayMatchesUninterrupted() {
        let mut c = Constraints(64);
        c.TransferVariance = TransferVarianceClass::High;
        let d = PlanVertexBuffering(&Upload(64), c);
        assert_eq!(d.SelectedMode, BufferingMode::SerialJitSurvival);
        let (full, resumed, _) = ReplayResult(d, 1);
        assert_eq!(full.Status, resumed.Status);
        assert_eq!(full.Telemetry, resumed.Telemetry);
        assert_eq!(full.Acts, resumed.Acts);
        assert!(resumed.Telemetry.ActiveSlotCount <= 1);
        assert!(resumed.Telemetry.WipDepth <= 1);
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

    #[test]
    fn HardFailureChunkRestorePreservesFailure() {
        let d = PlanVertexBuffering(&Upload(64), Constraints(63));
        let s = BufferSlotLifecycleSession::New(d);
        let chunk = s.ExportChunk();
        let mut restored = BufferSlotLifecycleSession::FromChunk(chunk).unwrap();
        while restored.Tick() {}
        let result = restored.Result();
        assert_eq!(result.Status, BufferSlotLifecycleStatus::HardFailure);
        assert_eq!(
            result.Acts,
            vec![DwActRequest {
                Id: RejectLifecycle
            }]
        );
    }

    #[test]
    fn DecisionPersistsAcrossRestore() {
        let d = PlanVertexBuffering(&Upload(64), Constraints(128));
        assert_eq!(d.SelectedMode, BufferingMode::FixedDoubleDefault);
        let mut s = BufferSlotLifecycleSession::New(d.clone());
        let _ = s.Tick();
        let mut chunk = s.ExportChunk();
        chunk.Decision.SelectedMode = BufferingMode::SerialJitSurvival;
        let restored = BufferSlotLifecycleSession::FromChunk(chunk).unwrap();
        assert_eq!(
            restored.ExportChunk().Decision.SelectedMode,
            BufferingMode::SerialJitSurvival
        );
        assert_ne!(d.SelectedMode, BufferingMode::SerialJitSurvival);
    }

    #[test]
    fn RestoreRejectsInvalidChunk() {
        let d = PlanVertexBuffering(&Upload(64), Constraints(128));
        let s = BufferSlotLifecycleSession::New(d);
        let mut chunk = s.ExportChunk();
        chunk.Runtime.Stack.clear();
        let restored = BufferSlotLifecycleSession::FromChunk(chunk);
        assert!(matches!(
            restored,
            Err(BufferSlotLifecycleRestoreError::InvalidChunk { .. })
        ));
    }
}
