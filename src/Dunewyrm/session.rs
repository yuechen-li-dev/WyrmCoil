#![allow(non_snake_case)]

use std::fmt::Write;

use crate::{
    DwActId, DwActRequest, DwBoard, DwBoardChunk, DwControl, DwControlSummary, DwDeferredAct,
    DwFrameId, DwFrameRegistry, DwMailbox, DwMailboxChunk, DwMessage, DwPhase,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwDecisionKey {
    pub Frame: DwFrameId,
    pub Pc: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwDecisionCommitState {
    pub Frame: DwFrameId,
    pub Pc: u32,
    pub Target: DwFrameId,
    pub Age: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwDecisionTraceEntry {
    pub Tick: u64,
    pub Frame: DwFrameId,
    pub Pc: u32,
    pub Candidates: Vec<(DwFrameId, f32)>,
    pub RawWinner: DwFrameId,
    pub Selected: DwFrameId,
    pub TieBreakApplied: bool,
    pub MinCommitApplied: bool,
    pub HysteresisApplied: bool,
    pub CommitAge: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DwRunStatus {
    Running,
    Waiting,
    Steady,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwRuntimeFrame {
    pub Id: DwFrameId,
    pub Pc: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwTickResult {
    pub DirtySlots: Vec<u32>,
    pub VisibleMailbox: Vec<DwMessage>,
    pub StagedMailbox: Vec<DwMessage>,
    pub Tick: u64,
    pub Status: DwRunStatus,
    pub Frame: Option<DwFrameId>,
    pub Pc: Option<u32>,
    pub Stack: [Option<DwRuntimeFrame>; 8],
    pub StackDepth: usize,
    pub Control: Option<DwControlSummary>,
    pub FailureReason: Option<&'static str>,
    pub Decisions: Vec<DwDecisionTraceEntry>,
    pub ImmediateActs: Vec<DwActRequest>,
    pub MaturedDeferredActs: Vec<DwActRequest>,
    pub PendingDeferredActs: Vec<DwDeferredAct>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwTickTraceEntry {
    pub Tick: u64,
    pub Status: DwRunStatus,
    pub Frame: Option<DwFrameId>,
    pub Pc: Option<u32>,
    pub Stack: Vec<DwRuntimeFrame>,
    pub Control: Option<DwControlSummary>,
    pub DirtySlots: Vec<u32>,
    pub VisibleMailbox: Vec<DwMessage>,
    pub StagedMailbox: Vec<DwMessage>,
    pub FailureReason: Option<&'static str>,
    pub Decisions: Vec<DwDecisionTraceEntry>,
    pub ImmediateActs: Vec<DwActRequest>,
    pub MaturedDeferredActs: Vec<DwActRequest>,
    pub PendingDeferredActs: Vec<DwDeferredAct>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwTraceComparison {
    pub Matches: bool,
    pub FirstMismatchIndex: Option<usize>,
    pub Reason: Option<String>,
    pub Expected: Option<DwTickTraceEntry>,
    pub Actual: Option<DwTickTraceEntry>,
}

pub fn CompareTrace(
    expected: &[DwTickTraceEntry],
    actual: &[DwTickTraceEntry],
) -> DwTraceComparison {
    let shared_len = expected.len().min(actual.len());
    for index in 0..shared_len {
        if expected[index] != actual[index] {
            return DwTraceComparison {
                Matches: false,
                FirstMismatchIndex: Some(index),
                Reason: Some(format!("trace entry mismatch at tick index {index}")),
                Expected: Some(expected[index].clone()),
                Actual: Some(actual[index].clone()),
            };
        }
    }

    if expected.len() != actual.len() {
        return DwTraceComparison {
            Matches: false,
            FirstMismatchIndex: Some(shared_len),
            Reason: Some(format!(
                "trace length mismatch expected={} actual={}",
                expected.len(),
                actual.len()
            )),
            Expected: expected.get(shared_len).cloned(),
            Actual: actual.get(shared_len).cloned(),
        };
    }

    DwTraceComparison {
        Matches: true,
        FirstMismatchIndex: None,
        Reason: None,
        Expected: None,
        Actual: None,
    }
}

pub fn FormatFrameId(frame: DwFrameId) -> String {
    format!("{}:{}", frame.Domain, frame.Local)
}

pub fn FormatActId(act: DwActId) -> String {
    format!("{}:{}", act.Domain, act.Local)
}

pub fn FormatTraceEntry(entry: &DwTickTraceEntry) -> String {
    let mut output = String::new();
    let _ = write!(
        output,
        "tick={} status={:?} frame={} pc={} control={:?} dirty={:?}",
        entry.Tick,
        entry.Status,
        entry
            .Frame
            .map(FormatFrameId)
            .unwrap_or_else(|| "none".to_string()),
        entry
            .Pc
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        entry.Control,
        entry.DirtySlots
    );

    let stack = entry
        .Stack
        .iter()
        .map(|frame| format!("{}@{}", FormatFrameId(frame.Id), frame.Pc))
        .collect::<Vec<_>>()
        .join(" -> ");
    let _ = write!(output, " stack=[{}]", stack);
    let _ = write!(
        output,
        " visible={:?} staged={:?}",
        entry.VisibleMailbox, entry.StagedMailbox
    );
    if !entry.Decisions.is_empty() {
        let decisions = entry
            .Decisions
            .iter()
            .map(|decision| {
                format!(
                    "{}@{} raw={} selected={} age={}",
                    FormatFrameId(decision.Frame),
                    decision.Pc,
                    FormatFrameId(decision.RawWinner),
                    FormatFrameId(decision.Selected),
                    decision.CommitAge
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(output, " decisions=[{}]", decisions);
    }
    if !entry.ImmediateActs.is_empty() || !entry.MaturedDeferredActs.is_empty() {
        let immediate = entry
            .ImmediateActs
            .iter()
            .map(|request| FormatActId(request.Id))
            .collect::<Vec<_>>()
            .join(", ");
        let matured = entry
            .MaturedDeferredActs
            .iter()
            .map(|request| FormatActId(request.Id))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            output,
            " immediate_acts=[{}] matured_acts=[{}]",
            immediate, matured
        );
    }

    if let Some(reason) = entry.FailureReason {
        let _ = write!(output, " failure={reason}");
    }

    output
}

pub fn FormatTrace(trace: &[DwTickTraceEntry]) -> String {
    trace
        .iter()
        .map(FormatTraceEntry)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn FormatComparison(comparison: &DwTraceComparison) -> String {
    if comparison.Matches {
        return "trace comparison matches".to_string();
    }

    let mut output = String::new();
    let _ = write!(
        output,
        "trace comparison mismatch index={:?} reason={}",
        comparison.FirstMismatchIndex,
        comparison.Reason.clone().unwrap_or_default()
    );
    if let Some(expected) = &comparison.Expected {
        let _ = write!(output, "\nexpected: {}", FormatTraceEntry(expected));
    }
    if let Some(actual) = &comparison.Actual {
        let _ = write!(output, "\nactual: {}", FormatTraceEntry(actual));
    }
    output
}

pub struct DwFrameCtx<'a> {
    /* unchanged */
    Frame: DwFrameId,
    Pc: u32,
    Tick: u64,
    Board: &'a mut DwBoard,
    Mailbox: &'a mut DwMailbox,
    DecisionMemory: &'a mut Vec<DwDecisionCommitState>,
    Decisions: &'a mut Vec<DwDecisionTraceEntry>,
    ImmediateActs: &'a mut Vec<DwActRequest>,
    DeferredActs: &'a mut Vec<DwDeferredAct>,
}
impl<'a> DwFrameCtx<'a> {
    /* methods */
    pub fn New(
        frame: DwFrameId,
        pc: u32,
        tick: u64,
        board: &'a mut DwBoard,
        mailbox: &'a mut DwMailbox,
        decision_memory: &'a mut Vec<DwDecisionCommitState>,
        decisions: &'a mut Vec<DwDecisionTraceEntry>,
        immediate_acts: &'a mut Vec<DwActRequest>,
        deferred_acts: &'a mut Vec<DwDeferredAct>,
    ) -> Self {
        Self {
            Frame: frame,
            Pc: pc,
            Tick: tick,
            Board: board,
            Mailbox: mailbox,
            DecisionMemory: decision_memory,
            Decisions: decisions,
            ImmediateActs: immediate_acts,
            DeferredActs: deferred_acts,
        }
    }
    pub fn Frame(&self) -> DwFrameId {
        self.Frame
    }
    pub fn Pc(&self) -> u32 {
        self.Pc
    }
    pub fn Phase<P: DwPhase>(&self) -> Option<P> {
        P::FromPc(self.Pc)
    }
    pub fn Tick(&self) -> u64 {
        self.Tick
    }
    pub fn Board(&self) -> &DwBoard {
        self.Board
    }
    pub fn BoardMut(&mut self) -> &mut DwBoard {
        self.Board
    }
    pub fn Mailbox(&self) -> &DwMailbox {
        self.Mailbox
    }
    pub fn MailboxMut(&mut self) -> &mut DwMailbox {
        self.Mailbox
    }
    pub fn DecisionKey(&self) -> DwDecisionKey {
        DwDecisionKey {
            Frame: self.Frame,
            Pc: self.Pc,
        }
    }
    pub fn FindDecisionMemoryIndex(&self, key: DwDecisionKey) -> Option<usize> {
        self.DecisionMemory
            .iter()
            .position(|state| state.Frame == key.Frame && state.Pc == key.Pc)
    }
    pub fn DecisionMemoryAt(&self, index: usize) -> DwDecisionCommitState {
        self.DecisionMemory[index]
    }
    pub fn UpsertDecisionMemory(&mut self, next: DwDecisionCommitState) {
        if let Some(index) = self.FindDecisionMemoryIndex(DwDecisionKey {
            Frame: next.Frame,
            Pc: next.Pc,
        }) {
            self.DecisionMemory[index] = next;
        } else {
            self.DecisionMemory.push(next);
        }
    }
    pub fn RecordDecision(&mut self, decision: DwDecisionTraceEntry) {
        self.Decisions.push(decision);
    }
    pub fn Immediate(&mut self, id: DwActId) {
        self.ImmediateActs.push(DwActRequest { Id: id });
    }
    pub fn Deferred(&mut self, id: DwActId, delay_ticks: u32) {
        self.DeferredActs.push(DwDeferredAct {
            Request: DwActRequest { Id: id },
            DueTick: self.Tick + u64::from(delay_ticks) + 1,
        });
    }
}

pub struct DwSession {
    Registry: DwFrameRegistry,
    Stack: Vec<DwRuntimeFrame>,
    Tick: u64,
    WaitRemaining: u32,
    WaitResumePc: Option<u32>,
    Status: DwRunStatus,
    FailureReason: Option<&'static str>,
    Board: DwBoard,
    Mailbox: DwMailbox,
    Trace: Vec<DwTickTraceEntry>,
    DecisionMemory: Vec<DwDecisionCommitState>,
    ImmediateActs: Vec<DwActRequest>,
    PendingDeferredActs: Vec<DwDeferredAct>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DwWaitChunk {
    pub WaitRemaining: u32,
    pub WaitResumePc: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DwRuntimeChunk {
    pub Tick: u64,
    pub Status: DwRunStatus,
    pub FailureReason: Option<&'static str>,
    pub Wait: DwWaitChunk,
    pub Stack: Vec<DwRuntimeFrame>,
    pub Board: DwBoardChunk,
    pub Mailbox: DwMailboxChunk,
    pub DecisionMemory: Vec<DwDecisionCommitState>,
    pub PendingDeferredActs: Vec<DwDeferredAct>,
}

impl DwSession {
    pub fn New(
        registry: DwFrameRegistry,
        root: DwFrameId,
        initial_pc: u32,
    ) -> Result<Self, &'static str> {
        if registry.Find(root).is_none() {
            return Err("root frame not found");
        }
        Ok(Self {
            Registry: registry,
            Stack: vec![DwRuntimeFrame {
                Id: root,
                Pc: initial_pc,
            }],
            Tick: 0,
            WaitRemaining: 0,
            WaitResumePc: None,
            Status: DwRunStatus::Running,
            FailureReason: None,
            Board: DwBoard::New(),
            Mailbox: DwMailbox::New(),
            Trace: Vec::new(),
            DecisionMemory: Vec::new(),
            ImmediateActs: Vec::new(),
            PendingDeferredActs: Vec::new(),
        })
    }
    pub fn Tick(&mut self) -> Result<DwTickResult, &'static str> {
        let tick_now = self.Tick;
        let mut decisions = Vec::new();
        self.ImmediateActs.clear();
        let matured_acts = self.FlushMaturedDeferredActs(tick_now);
        let result = if self.Status == DwRunStatus::Completed || self.Status == DwRunStatus::Failed
        {
            self.BuildResult(tick_now, None, decisions, matured_acts)
        } else {
            if self.Status == DwRunStatus::Steady {
                self.Status = DwRunStatus::Running;
            }
            self.Mailbox.BeginTick();
            self.Board.ClearDirty();
            self.Board.TickTtl();
            if self.WaitRemaining > 0 {
                self.WaitRemaining -= 1;
                if self.WaitRemaining == 0 {
                    if let Some(top) = self.Stack.last_mut() {
                        top.Pc = self.WaitResumePc.expect("wait resume pc should be set");
                    }
                    self.WaitResumePc = None;
                    self.BuildResult(
                        tick_now,
                        Some(DwControlSummary::WaitTicks { Ticks: 0 }),
                        decisions,
                        matured_acts,
                    )
                } else {
                    self.BuildResult(
                        tick_now,
                        Some(DwControlSummary::WaitTicks {
                            Ticks: self.WaitRemaining,
                        }),
                        decisions,
                        matured_acts,
                    )
                }
            } else {
                let active = self.Stack.last().copied().ok_or("runtime stack empty")?;
                let frame = self
                    .Registry
                    .Find(active.Id)
                    .ok_or("active frame missing")?;
                let mut ctx = DwFrameCtx::New(
                    active.Id,
                    active.Pc,
                    tick_now,
                    &mut self.Board,
                    &mut self.Mailbox,
                    &mut self.DecisionMemory,
                    &mut decisions,
                    &mut self.ImmediateActs,
                    &mut self.PendingDeferredActs,
                );
                let control = (frame.Step)(&mut ctx);
                self.ApplyControl(control);
                self.BuildResult(
                    tick_now,
                    Some(Self::Summarize(control)),
                    decisions,
                    matured_acts,
                )
            }
        };
        self.Trace.push(Self::TraceFromResult(&result));
        self.Tick += 1;
        Ok(result)
    }

    pub fn RunUntilBlocked(&mut self, max_ticks: u32) -> Result<DwTickResult, &'static str> {
        if max_ticks == 0 {
            return Err("max_ticks must be greater than zero");
        }
        let mut last = self.Tick()?;
        for _ in 1..max_ticks {
            if Self::IsBlockedStatus(last.Status) {
                return Ok(last);
            }
            last = self.Tick()?;
        }
        Ok(last)
    }
    fn TraceFromResult(result: &DwTickResult) -> DwTickTraceEntry {
        DwTickTraceEntry {
            Tick: result.Tick,
            Status: result.Status,
            Frame: result.Frame,
            Pc: result.Pc,
            Stack: result.Stack.iter().flatten().copied().collect(),
            Control: result.Control,
            DirtySlots: result.DirtySlots.clone(),
            VisibleMailbox: result.VisibleMailbox.clone(),
            StagedMailbox: result.StagedMailbox.clone(),
            FailureReason: result.FailureReason,
            Decisions: result.Decisions.clone(),
            ImmediateActs: result.ImmediateActs.clone(),
            MaturedDeferredActs: result.MaturedDeferredActs.clone(),
            PendingDeferredActs: result.PendingDeferredActs.clone(),
        }
    }
    pub fn Trace(&self) -> &[DwTickTraceEntry] {
        &self.Trace
    }
    pub fn Board(&self) -> &DwBoard {
        &self.Board
    }
    pub fn BoardMut(&mut self) -> &mut DwBoard {
        &mut self.Board
    }
    pub fn Mailbox(&self) -> &DwMailbox {
        &self.Mailbox
    }
    pub fn MailboxMut(&mut self) -> &mut DwMailbox {
        &mut self.Mailbox
    }
    pub fn ExportChunk(&self) -> DwRuntimeChunk {
        DwRuntimeChunk {
            Tick: self.Tick,
            Status: self.Status,
            FailureReason: self.FailureReason,
            Wait: DwWaitChunk {
                WaitRemaining: self.WaitRemaining,
                WaitResumePc: self.WaitResumePc,
            },
            Stack: self.Stack.clone(),
            Board: self.Board.ExportChunk(),
            Mailbox: self.Mailbox.ExportChunk(),
            DecisionMemory: self.DecisionMemory.clone(),
            PendingDeferredActs: self.PendingDeferredActs.clone(),
        }
    }
    pub fn FromChunk(
        registry: DwFrameRegistry,
        chunk: DwRuntimeChunk,
    ) -> Result<Self, &'static str> {
        for frame in &chunk.Stack {
            if registry.Find(frame.Id).is_none() {
                return Err("chunk stack frame not found in registry");
            }
        }
        Ok(Self {
            Registry: registry,
            Stack: chunk.Stack,
            Tick: chunk.Tick,
            WaitRemaining: chunk.Wait.WaitRemaining,
            WaitResumePc: chunk.Wait.WaitResumePc,
            Status: chunk.Status,
            FailureReason: chunk.FailureReason,
            Board: DwBoard::FromChunk(chunk.Board),
            Mailbox: DwMailbox::FromChunk(chunk.Mailbox),
            Trace: Vec::new(),
            DecisionMemory: chunk.DecisionMemory,
            ImmediateActs: Vec::new(),
            PendingDeferredActs: chunk.PendingDeferredActs,
        })
    }
    fn ApplyControl(&mut self, control: DwControl) {
        match control {
            DwControl::Continue { Pc } => {
                if let Some(top) = self.Stack.last_mut() {
                    top.Pc = Pc;
                }
            }
            DwControl::WaitTicks { Ticks, Pc } => {
                if Ticks == 0 {
                    if let Some(top) = self.Stack.last_mut() {
                        top.Pc = Pc;
                    }
                } else {
                    self.WaitRemaining = Ticks;
                    self.WaitResumePc = Some(Pc);
                    self.Status = DwRunStatus::Waiting;
                }
            }
            DwControl::Steady => {
                self.Status = DwRunStatus::Steady;
            }
            DwControl::Push { Target, ResumePc } => {
                if self.Registry.Find(Target).is_none() {
                    self.FailNow("push target frame not found");
                    return;
                }
                if let Some(top) = self.Stack.last_mut() {
                    top.Pc = ResumePc;
                }
                self.Stack.push(DwRuntimeFrame { Id: Target, Pc: 0 });
            }
            DwControl::Pop => {
                if self.Stack.len() == 1 {
                    self.FailNow("cannot pop root frame");
                    return;
                }
                self.Stack.pop();
            }
            DwControl::Replace { Target } => {
                if self.Registry.Find(Target).is_none() {
                    self.FailNow("replace target frame not found");
                    return;
                }
                self.Stack.pop();
                self.Stack.push(DwRuntimeFrame { Id: Target, Pc: 0 });
            }
            DwControl::Stay => {}
            DwControl::Complete => {
                if self.Stack.len() == 1 {
                    self.Status = DwRunStatus::Completed;
                } else {
                    self.Stack.pop();
                }
            }
            DwControl::Fail { Reason } => self.FailNow(Reason),
        }
    }
    fn FailNow(&mut self, reason: &'static str) {
        self.Status = DwRunStatus::Failed;
        self.FailureReason = Some(reason);
    }
    fn BuildResult(
        &mut self,
        tick_now: u64,
        control: Option<DwControlSummary>,
        decisions: Vec<DwDecisionTraceEntry>,
        matured_acts: Vec<DwActRequest>,
    ) -> DwTickResult {
        if self.Status == DwRunStatus::Waiting && self.WaitRemaining == 0 {
            self.Status = DwRunStatus::Running;
        }
        let mut snap = [None; 8];
        for (i, frame) in self.Stack.iter().take(8).enumerate() {
            snap[i] = Some(*frame);
        }
        let active = self.Stack.last().copied();
        DwTickResult {
            Tick: tick_now,
            Status: self.Status,
            Frame: active.map(|f| f.Id),
            Pc: active.map(|f| f.Pc),
            Stack: snap,
            StackDepth: self.Stack.len(),
            Control: control,
            FailureReason: self.FailureReason,
            DirtySlots: self.Board.DirtySlots(),
            VisibleMailbox: self.Mailbox.VisibleSnapshot(),
            StagedMailbox: self.Mailbox.StagedSnapshot(),
            Decisions: decisions,
            ImmediateActs: self.ImmediateActs.clone(),
            MaturedDeferredActs: matured_acts,
            PendingDeferredActs: self.PendingDeferredActs.clone(),
        }
    }
    fn FlushMaturedDeferredActs(&mut self, tick_now: u64) -> Vec<DwActRequest> {
        let mut matured = Vec::new();
        let mut remaining = Vec::new();
        for deferred in &self.PendingDeferredActs {
            if deferred.DueTick <= tick_now {
                matured.push(deferred.Request);
            } else {
                remaining.push(*deferred);
            }
        }
        self.PendingDeferredActs = remaining;
        matured
    }
    fn Summarize(control: DwControl) -> DwControlSummary {
        match control {
            DwControl::Continue { .. } => DwControlSummary::Continue,
            DwControl::WaitTicks { Ticks, .. } => DwControlSummary::WaitTicks { Ticks },
            DwControl::Steady => DwControlSummary::Steady,
            DwControl::Push { .. } => DwControlSummary::Push,
            DwControl::Pop => DwControlSummary::Pop,
            DwControl::Replace { .. } => DwControlSummary::Replace,
            DwControl::Stay => DwControlSummary::Stay,
            DwControl::Complete => DwControlSummary::Complete,
            DwControl::Fail { .. } => DwControlSummary::Fail,
        }
    }

    fn IsBlockedStatus(status: DwRunStatus) -> bool {
        matches!(
            status,
            DwRunStatus::Waiting
                | DwRunStatus::Steady
                | DwRunStatus::Completed
                | DwRunStatus::Failed
        )
    }
}
