#![allow(non_snake_case)]

mod board;
mod control;
mod ids;
mod mailbox;
mod phase;
mod registry;
mod session;

pub use board::{
    DwBoard, DwBoardChunk, DwBoardKind, DwBoardSnapshot, DwBoardSnapshotEntry, DwBoardTtlEntry,
    DwBoardTtlSnapshot, DwBoardValue, DwBoardValueSnapshot, DwKey, DwSlotCollision,
};
pub use control::{
    Dw, DwControl, DwControlSummary, DwDecideOptions, DwScoreFn, DwTieBreak, DwUtilityCandidate,
    DwUtilityCandidateReport, DwUtilityDecisionReport, DwUtilitySelectionReason,
    SelectHighestUtilityTarget, SelectHighestUtilityTargetWithReport,
};
pub use ids::{DwActId, DwActRequest, DwDeferredAct, DwFrameId};
pub use mailbox::{DwMailbox, DwMailboxChunk, DwMessage, DwMessagePayload};
pub use phase::DwPhase;
pub use registry::{DwFrameDef, DwFrameFn, DwFrameRegistry};
pub use session::{
    CompareTrace, DwDecisionCommitState, DwDecisionKey, DwDecisionTraceEntry, DwFrameCtx,
    DwRootPolicy, DwRunStatus, DwRuntimeChunk, DwRuntimeFrame, DwSession, DwTickResult,
    DwTickTraceEntry, DwTraceComparison, DwWaitChunk, FormatComparison, FormatTrace,
    FormatTraceEntry,
};

pub fn ProjectName() -> &'static str {
    "Dunewyrm"
}

#[cfg(test)]
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum RootPhase {
        Start,
        Wait,
        Done,
    }
    impl DwPhase for RootPhase {
        fn ToPc(self) -> u32 {
            match self {
                RootPhase::Start => 0,
                RootPhase::Wait => 1,
                RootPhase::Done => 2,
            }
        }
        fn FromPc(pc: u32) -> Option<Self> {
            match pc {
                0 => Some(RootPhase::Start),
                1 => Some(RootPhase::Wait),
                2 => Some(RootPhase::Done),
                _ => None,
            }
        }
    }
    fn Root(ctx: &mut DwFrameCtx) -> DwControl {
        match ctx.Phase::<RootPhase>() {
            Some(RootPhase::Start) => Dw::Continue(RootPhase::Wait),
            Some(RootPhase::Wait) => Dw::WaitTicks(1, RootPhase::Done),
            Some(RootPhase::Done) => Dw::Complete(),
            None => Dw::Fail("bad phase"),
        }
    }

    #[test]
    fn ProjectNameMatches() {
        assert_eq!(ProjectName(), "Dunewyrm");
    }

    #[test]
    fn StackPushPopAndChildCompleteWorkDeterministically() {
        #[derive(Clone, Copy)]
        enum R {
            Start,
            After,
        }
        impl DwPhase for R {
            fn ToPc(self) -> u32 {
                match self {
                    R::Start => 0,
                    R::After => 1,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(R::Start),
                    1 => Some(R::After),
                    _ => None,
                }
            }
        }
        #[derive(Clone, Copy)]
        enum C {
            Begin,
        }
        impl DwPhase for C {
            fn ToPc(self) -> u32 {
                0
            }
            fn FromPc(pc: u32) -> Option<Self> {
                if pc == 0 { Some(C::Begin) } else { None }
            }
        }
        let root_id = DwFrameId {
            Domain: 1,
            Local: 1,
        };
        let child_id = DwFrameId {
            Domain: 1,
            Local: 2,
        };
        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Phase::<R>() {
                Some(R::Start) => Dw::Push(
                    DwFrameId {
                        Domain: 1,
                        Local: 2,
                    },
                    R::After,
                ),
                Some(R::After) => Dw::Complete(),
                None => Dw::Fail("root phase"),
            }
        }
        fn ChildF(_: &mut DwFrameCtx) -> DwControl {
            Dw::Complete()
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root_id,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: child_id,
            Step: ChildF,
            DebugName: "Child",
        })
        .unwrap();
        let mut s = DwSession::New(reg, root_id, 0).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(t0.Control, Some(DwControlSummary::Push));
        assert_eq!(t0.Frame, Some(child_id));
        assert_eq!(t0.Pc, Some(0));
        assert_eq!(t0.StackDepth, 2);
        let t1 = s.Tick().unwrap();
        assert_eq!(t1.Control, Some(DwControlSummary::Complete));
        assert_eq!(t1.Frame, Some(root_id));
        assert_eq!(t1.Pc, Some(1));
        assert_eq!(t1.StackDepth, 1);
        let t2 = s.Tick().unwrap();
        assert_eq!(t2.Status, DwRunStatus::Completed);
    }

    #[test]
    fn WaitInChildBlocksParentAndResumesChild() {
        #[derive(Clone, Copy)]
        enum R {
            Start,
            After,
        }
        impl DwPhase for R {
            fn ToPc(self) -> u32 {
                match self {
                    R::Start => 0,
                    R::After => 1,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(R::Start),
                    1 => Some(R::After),
                    _ => None,
                }
            }
        }
        #[derive(Clone, Copy)]
        enum C {
            Start,
            Done,
        }
        impl DwPhase for C {
            fn ToPc(self) -> u32 {
                match self {
                    C::Start => 0,
                    C::Done => 1,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(C::Start),
                    1 => Some(C::Done),
                    _ => None,
                }
            }
        }
        let root_id = DwFrameId {
            Domain: 2,
            Local: 1,
        };
        let child_id = DwFrameId {
            Domain: 2,
            Local: 2,
        };
        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Phase::<R>() {
                Some(R::Start) => Dw::Push(
                    DwFrameId {
                        Domain: 2,
                        Local: 2,
                    },
                    R::After,
                ),
                Some(R::After) => Dw::Complete(),
                None => Dw::Fail("root"),
            }
        }
        fn ChildF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Phase::<C>() {
                Some(C::Start) => Dw::WaitTicks(2, C::Done),
                Some(C::Done) => Dw::Pop(),
                None => Dw::Fail("child"),
            }
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root_id,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: child_id,
            Step: ChildF,
            DebugName: "Child",
        })
        .unwrap();
        let mut s = DwSession::New(reg, root_id, 0).unwrap();
        s.Tick().unwrap();
        let t1 = s.Tick().unwrap();
        assert_eq!(t1.Status, DwRunStatus::Waiting);
        assert_eq!(t1.Frame, Some(child_id));
        let t2 = s.Tick().unwrap();
        assert_eq!(t2.Status, DwRunStatus::Waiting);
        assert_eq!(t2.Frame, Some(child_id));
        let t3 = s.Tick().unwrap();
        assert_eq!(t3.Status, DwRunStatus::Running);
        assert_eq!(t3.Frame, Some(child_id));
        assert_eq!(t3.Pc, Some(1));
    }

    #[test]
    fn ReplaceAndFailureCasesCovered() {
        let root = DwFrameId {
            Domain: 3,
            Local: 1,
        };
        let repl = DwFrameId {
            Domain: 3,
            Local: 2,
        };
        fn RootF(_: &mut DwFrameCtx) -> DwControl {
            Dw::Replace(DwFrameId {
                Domain: 3,
                Local: 2,
            })
        }
        fn ReplF(_: &mut DwFrameCtx) -> DwControl {
            Dw::Complete()
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: repl,
            Step: ReplF,
            DebugName: "Repl",
        })
        .unwrap();
        let mut s = DwSession::New(reg, root, 0).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(t0.Control, Some(DwControlSummary::Replace));
        assert_eq!(t0.Frame, Some(repl));

        let mut reg2 = DwFrameRegistry::New();
        reg2.Register(DwFrameDef {
            Id: root,
            Step: |_| Dw::Pop(),
            DebugName: "RootPop",
        })
        .unwrap();
        let mut s2 = DwSession::New(reg2, root, 0).unwrap();
        let f = s2.Tick().unwrap();
        assert_eq!(f.Status, DwRunStatus::Failed);
    }

    mod Keys {
        use crate::DwKey;
        pub const Alerted: DwKey<bool> = DwKey::New("Alerted", 1);
        pub const Count: DwKey<i32> = DwKey::New("Count", 2);
        pub const Pressure: DwKey<f32> = DwKey::New("Pressure", 3);
    }

    #[test]
    fn BoardSetGetDirtyAndCollisionBehavior() {
        let mut board = DwBoard::New();
        assert_eq!(board.GetOr(Keys::Alerted, false), false);
        assert_eq!(board.GetOr(Keys::Count, -1), -1);
        assert!((board.GetOr(Keys::Pressure, 1.5) - 1.5).abs() < f32::EPSILON);

        board.Set(Keys::Alerted, true).unwrap();
        board.Set(Keys::Count, 7).unwrap();
        board.Set(Keys::Pressure, 2.0).unwrap();
        board.Set(Keys::Alerted, false).unwrap();

        assert_eq!(board.TryGet(Keys::Alerted), Some(false));
        assert_eq!(board.TryGet(Keys::Count), Some(7));
        assert_eq!(board.TryGet(Keys::Pressure), Some(2.0));
        assert!(board.IsDirty(Keys::Alerted));
        assert_eq!(board.DirtySlots(), vec![1, 2, 3]);

        board.ClearDirty();
        assert_eq!(board.DirtySlots(), Vec::<u32>::new());

        let alias_ok = DwKey::<bool>::New("Alerted", 1);
        board.Set(alias_ok, true).unwrap();

        let bad_name = DwKey::<bool>::New("Other", 1);
        assert!(board.Set(bad_name, true).is_err());
        assert!(board.LastSlotCollision().is_some());

        let bad_type = DwKey::<i32>::New("Alerted", 1);
        assert!(board.Set(bad_type, 1).is_err());
        assert_eq!(board.DirtySlots(), vec![1]);
    }

    #[test]
    fn BoardTtlExpiryRefreshSnapshotAndClearBehavior() {
        let mut board = DwBoard::New();
        board.SetBoolWithTtl(Keys::Alerted, true, 2).unwrap();
        assert_eq!(board.TryGet(Keys::Alerted), Some(true));
        assert_eq!(board.TtlSnapshot().Entries[0].RemainingTicks, 2);

        board.ClearDirty();
        board.TickTtl();
        assert_eq!(board.TryGet(Keys::Alerted), Some(true));
        assert_eq!(board.TtlSnapshot().Entries[0].RemainingTicks, 1);
        assert_eq!(board.DirtySlots(), Vec::<u32>::new());

        board.SetBoolWithTtl(Keys::Alerted, true, 2).unwrap();
        assert_eq!(board.TtlSnapshot().Entries[0].RemainingTicks, 2);
        assert_eq!(board.DirtySlots(), Vec::<u32>::new());

        board.TickTtl();
        board.TickTtl();
        assert_eq!(board.TryGet(Keys::Alerted), Some(false));
        assert_eq!(board.DirtySlots(), vec![1]);

        let snap = board.Snapshot();
        assert_eq!(snap.Entries.len(), 1);
        assert_eq!(snap.Entries[0].Slot, 1);
        assert_eq!(snap.Entries[0].Value, DwBoardValueSnapshot::Bool(false));

        board.SetBoolWithTtl(Keys::Alerted, true, 3).unwrap();
        board.ClearTtl(Keys::Alerted);
        board.ClearDirty();
        board.TickTtl();
        board.TickTtl();
        assert_eq!(board.TryGet(Keys::Alerted), Some(true));
    }

    #[test]
    fn BoardTtlI32F32ZeroAndChunkResumeBehavior() {
        let mut board = DwBoard::New();
        board.SetI32WithTtl(Keys::Count, 9, 2, -1).unwrap();
        board.SetF32WithTtl(Keys::Pressure, 6.0, 2, 0.5).unwrap();
        board.TickTtl();
        let chunk = board.ExportChunk();
        let mut restored = DwBoard::FromChunk(chunk);
        restored.ClearDirty();
        restored.TickTtl();
        assert_eq!(restored.TryGet(Keys::Count), Some(-1));
        assert_eq!(restored.TryGet(Keys::Pressure), Some(0.5));
        assert_eq!(restored.DirtySlots(), vec![2, 3]);

        restored.ClearDirty();
        restored.SetBoolWithTtl(Keys::Alerted, true, 0).unwrap();
        assert_eq!(restored.TryGet(Keys::Alerted), Some(false));
        assert_eq!(restored.DirtySlots(), vec![1]);
    }

    #[test]
    fn SessionTickAppliesTtlBeforeFrameExecution() {
        #[derive(Clone, Copy)]
        enum R {
            Check,
            Done,
        }
        impl DwPhase for R {
            fn ToPc(self) -> u32 {
                match self {
                    R::Check => 0,
                    R::Done => 1,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(R::Check),
                    1 => Some(R::Done),
                    _ => None,
                }
            }
        }
        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Phase::<R>() {
                Some(R::Check) => {
                    if ctx.Board().GetOr(Keys::Alerted, true) {
                        Dw::Fail("ttl should have expired before frame step")
                    } else {
                        Dw::Continue(R::Done)
                    }
                }
                Some(R::Done) => Dw::Complete(),
                None => Dw::Fail("phase"),
            }
        }

        let root = DwFrameId {
            Domain: 55,
            Local: 1,
        };
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        let mut s = DwSession::New(reg, root, 0).unwrap();
        s.BoardMut().SetBoolWithTtl(Keys::Alerted, true, 1).unwrap();
        s.BoardMut().ClearDirty();
        let t0 = s.Tick().unwrap();
        assert_eq!(t0.DirtySlots, vec![1]);
        assert_eq!(s.Board().TryGet(Keys::Alerted), Some(false));
    }

    #[test]
    fn BoardFlowsAcrossParentAndChildFramesAndDirtyResetsPerTick() {
        #[derive(Clone, Copy)]
        enum R {
            Start,
            Verify,
            Done,
        }
        impl DwPhase for R {
            fn ToPc(self) -> u32 {
                match self {
                    R::Start => 0,
                    R::Verify => 1,
                    R::Done => 2,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(R::Start),
                    1 => Some(R::Verify),
                    2 => Some(R::Done),
                    _ => None,
                }
            }
        }
        #[derive(Clone, Copy)]
        enum C {
            Start,
        }
        impl DwPhase for C {
            fn ToPc(self) -> u32 {
                0
            }
            fn FromPc(pc: u32) -> Option<Self> {
                if pc == 0 { Some(C::Start) } else { None }
            }
        }

        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Phase::<R>() {
                Some(R::Start) => {
                    ctx.BoardMut().Set(Keys::Count, 10).unwrap();
                    Dw::Push(
                        DwFrameId {
                            Domain: 4,
                            Local: 2,
                        },
                        R::Verify,
                    )
                }
                Some(R::Verify) => {
                    let count = ctx.Board().GetOr(Keys::Count, -1);
                    if count == 11 {
                        Dw::Continue(R::Done)
                    } else {
                        Dw::Fail("count")
                    }
                }
                Some(R::Done) => Dw::Complete(),
                None => Dw::Fail("root phase"),
            }
        }

        fn ChildF(ctx: &mut DwFrameCtx) -> DwControl {
            let count = ctx.Board().GetOr(Keys::Count, -1);
            if count != 10 {
                return Dw::Fail("missing parent value");
            }
            ctx.BoardMut().Set(Keys::Count, 11).unwrap();
            Dw::Pop()
        }

        let root = DwFrameId {
            Domain: 4,
            Local: 1,
        };
        let child = DwFrameId {
            Domain: 4,
            Local: 2,
        };
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: child,
            Step: ChildF,
            DebugName: "Child",
        })
        .unwrap();

        let mut s = DwSession::New(reg, root, 0).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(t0.DirtySlots, vec![2]);
        let t1 = s.Tick().unwrap();
        assert_eq!(t1.DirtySlots, vec![2]);
        let t2 = s.Tick().unwrap();
        assert_eq!(t2.DirtySlots, Vec::<u32>::new());
        assert_eq!(s.Board().TryGet(Keys::Count), Some(11));
    }

    #[test]
    fn MailboxEmptyPeekConsumeAndFifoBehavior() {
        let mut mailbox = DwMailbox::New();
        assert_eq!(
            mailbox.PeekFront(),
            None,
            "expected empty mailbox peek to return None before any seeding"
        );
        assert_eq!(
            mailbox.ConsumeFront(),
            None,
            "expected empty mailbox consume to return None before any seeding"
        );

        mailbox.EnqueueVisibleForTest(DwMessage::I32(1, 11));
        mailbox.EnqueueVisibleForTest(DwMessage::I32(2, 22));
        assert_eq!(
            mailbox.PeekFront(),
            Some(DwMessage::I32(1, 11)),
            "expected peek to show the front visible message without consuming it"
        );
        assert_eq!(
            mailbox.ConsumeFront(),
            Some(DwMessage::I32(1, 11)),
            "expected consume to remove the earliest visible message first (FIFO)"
        );
        assert_eq!(
            mailbox.ConsumeFront(),
            Some(DwMessage::I32(2, 22)),
            "expected consume to continue preserving FIFO order"
        );
    }

    #[test]
    fn MailboxStagingBoundaryAndWaitPromotionAreDeterministic() {
        #[derive(Clone, Copy)]
        enum P {
            Start,
            Check,
            Done,
        }
        impl DwPhase for P {
            fn ToPc(self) -> u32 {
                match self {
                    P::Start => 0,
                    P::Check => 1,
                    P::Done => 2,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(P::Start),
                    1 => Some(P::Check),
                    2 => Some(P::Done),
                    _ => None,
                }
            }
        }

        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Phase::<P>() {
                Some(P::Start) => {
                    let before = ctx.Mailbox().PeekFront();
                    assert_eq!(
                        before,
                        Some(DwMessage::I32(7, 70)),
                        "expected seeded visible message to be readable at start phase"
                    );
                    ctx.MailboxMut().Enqueue(DwMessage::I32(8, 80));
                    let same_tick = ctx.Mailbox().PeekFront();
                    assert_eq!(
                        same_tick,
                        Some(DwMessage::I32(7, 70)),
                        "expected staged message to remain invisible during same tick"
                    );
                    Dw::WaitTicks(1, P::Check)
                }
                Some(P::Check) => {
                    let consumed = ctx.MailboxMut().ConsumeFront();
                    assert_eq!(
                        consumed,
                        Some(DwMessage::I32(7, 70)),
                        "expected old visible message to stay available until explicitly consumed"
                    );
                    let promoted = ctx.Mailbox().PeekFront();
                    assert_eq!(
                        promoted,
                        Some(DwMessage::I32(8, 80)),
                        "expected staged message to promote at tick boundary while wait elapsed"
                    );
                    Dw::Continue(P::Done)
                }
                Some(P::Done) => Dw::Complete(),
                None => Dw::Fail("bad phase"),
            }
        }

        let root = DwFrameId {
            Domain: 5,
            Local: 1,
        };
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        let mut s = DwSession::New(reg, root, 0).unwrap();
        s.MailboxMut().EnqueueVisibleForTest(DwMessage::I32(7, 70));
        let t0 = s.Tick().unwrap();
        assert_eq!(
            t0.VisibleMailbox,
            vec![DwMessage::I32(7, 70)],
            "expected visible snapshot to keep unconsumed visible message after start tick"
        );
        assert_eq!(
            t0.StagedMailbox,
            vec![DwMessage::I32(8, 80)],
            "expected staged snapshot to include message enqueued during tick"
        );
        let t1 = s.Tick().unwrap();
        assert_eq!(
            t1.Status,
            DwRunStatus::Running,
            "expected wait countdown tick to return running after wait reaches zero"
        );
        assert_eq!(
            t1.VisibleMailbox,
            vec![DwMessage::I32(7, 70), DwMessage::I32(8, 80)],
            "expected staged message promotion at tick boundary to preserve FIFO behind existing visible messages"
        );
        assert_eq!(
            t1.StagedMailbox,
            Vec::<DwMessage>::new(),
            "expected staged queue to be empty immediately after promotion"
        );
    }

    #[test]
    fn RuntimeChunkRoundTripPreservesStateAndSupportsResume() {
        #[derive(Clone, Copy)]
        enum P {
            Start,
            Wait,
            Finish,
        }
        impl DwPhase for P {
            fn ToPc(self) -> u32 {
                match self {
                    P::Start => 0,
                    P::Wait => 1,
                    P::Finish => 2,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(P::Start),
                    1 => Some(P::Wait),
                    2 => Some(P::Finish),
                    _ => None,
                }
            }
        }

        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Phase::<P>() {
                Some(P::Start) => {
                    ctx.BoardMut().Set(Keys::Alerted, true).unwrap();
                    ctx.BoardMut().Set(Keys::Count, 5).unwrap();
                    ctx.BoardMut().Set(Keys::Pressure, 0.5).unwrap();
                    ctx.MailboxMut().Enqueue(DwMessage::I32(2, 20));
                    Dw::WaitTicks(2, P::Wait)
                }
                Some(P::Wait) => {
                    ctx.BoardMut().Set(Keys::Count, 8).unwrap();
                    Dw::Continue(P::Finish)
                }
                Some(P::Finish) => Dw::Complete(),
                None => Dw::Fail("phase"),
            }
        }

        let root = DwFrameId {
            Domain: 6,
            Local: 1,
        };
        let mut reg_a = DwFrameRegistry::New();
        reg_a
            .Register(DwFrameDef {
                Id: root,
                Step: RootF,
                DebugName: "Root",
            })
            .unwrap();
        let mut uninterrupted = DwSession::New(reg_a, root, 0).unwrap();
        let mut final_uninterrupted = uninterrupted.Tick().unwrap();
        for _ in 0..4 {
            final_uninterrupted = uninterrupted.Tick().unwrap();
        }

        let mut reg_b = DwFrameRegistry::New();
        reg_b
            .Register(DwFrameDef {
                Id: root,
                Step: RootF,
                DebugName: "Root",
            })
            .unwrap();
        let mut split = DwSession::New(reg_b, root, 0).unwrap();
        let first = split.Tick().unwrap();
        assert_eq!(
            first.Status,
            DwRunStatus::Waiting,
            "expected first tick to enter waiting before chunk export"
        );
        let chunk = split.ExportChunk();
        assert_eq!(
            chunk.Board.DirtySlots,
            vec![1, 2, 3],
            "expected chunk to preserve board dirty slots at export boundary"
        );
        assert_eq!(
            chunk.Mailbox.Staged,
            vec![DwMessage::I32(2, 20)],
            "expected staged mailbox queue to be preserved in exported chunk"
        );

        let mut reg_c = DwFrameRegistry::New();
        reg_c
            .Register(DwFrameDef {
                Id: root,
                Step: RootF,
                DebugName: "Root",
            })
            .unwrap();
        let mut restored = DwSession::FromChunk(reg_c, chunk.clone()).unwrap();
        assert_eq!(
            restored.ExportChunk(),
            chunk,
            "expected immediate re-export after restore to match original exported chunk"
        );

        let mut final_restored = restored.Tick().unwrap();
        for _ in 0..3 {
            final_restored = restored.Tick().unwrap();
        }

        assert_eq!(
            final_restored.Status, final_uninterrupted.Status,
            "restored session should preserve terminal status across chunk import"
        );
        assert_eq!(
            final_restored.Tick, final_uninterrupted.Tick,
            "restored session should preserve tick progression equivalence"
        );
        assert_eq!(
            restored.ExportChunk().Stack,
            uninterrupted.ExportChunk().Stack,
            "restored session should preserve final stack shape equivalence"
        );
        assert_eq!(
            restored.ExportChunk().Board,
            uninterrupted.ExportChunk().Board,
            "restored session should preserve final board state equivalence"
        );
        assert_eq!(
            restored.ExportChunk().Mailbox,
            uninterrupted.ExportChunk().Mailbox,
            "restored session should preserve final mailbox state equivalence"
        );
    }

    #[test]
    fn ChunkRestoreFailsWhenRegistryIsMissingFrame() {
        let root = DwFrameId {
            Domain: 7,
            Local: 1,
        };
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: |_| Dw::Complete(),
            DebugName: "Root",
        })
        .unwrap();
        let session = DwSession::New(reg, root, 0).unwrap();
        let chunk = session.ExportChunk();

        let empty_reg = DwFrameRegistry::New();
        let restored = DwSession::FromChunk(empty_reg, chunk);
        assert!(
            matches!(restored, Err("chunk stack frame not found in registry")),
            "expected clear restore error when chunk stack references missing frame IDs"
        );
    }

    #[test]
    fn TickTraceRecordsEntriesAndIncludesCoreFields() {
        let root = DwFrameId {
            Domain: 8,
            Local: 1,
        };
        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            if ctx.Pc() == 0 {
                ctx.BoardMut().Set(Keys::Count, 3).unwrap();
                ctx.MailboxMut().Enqueue(DwMessage::I32(9, 90));
                Dw::WaitTicks(1, RootPhase::Done)
            } else {
                Dw::Complete()
            }
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        let mut session = DwSession::New(reg, root, 0).unwrap();
        session.Tick().unwrap();
        session.Tick().unwrap();
        assert_eq!(
            session.Trace().len(),
            2,
            "expected one trace entry appended per tick"
        );
        let entry0 = &session.Trace()[0];
        assert_eq!(
            entry0.Tick, 0,
            "expected first trace entry tick index to start at zero"
        );
        assert_eq!(
            entry0.DirtySlots,
            vec![2],
            "expected trace entry to include dirty board slots for the tick"
        );
        assert_eq!(
            entry0.StagedMailbox,
            vec![DwMessage::I32(9, 90)],
            "expected trace entry to include staged mailbox snapshot after tick"
        );
        assert_eq!(
            entry0.Stack.len(),
            1,
            "expected trace entry stack snapshot to include active frame"
        );
    }

    #[test]
    fn TraceComparisonReportsMatchAndFirstMismatch() {
        let expected = vec![DwTickTraceEntry {
            Tick: 0,
            Status: DwRunStatus::Running,
            Frame: Some(DwFrameId {
                Domain: 1,
                Local: 1,
            }),
            Pc: Some(0),
            Stack: vec![DwRuntimeFrame {
                Id: DwFrameId {
                    Domain: 1,
                    Local: 1,
                },
                Pc: 0,
            }],
            Control: Some(DwControlSummary::Stay),
            DirtySlots: vec![],
            VisibleMailbox: vec![],
            StagedMailbox: vec![],
            FailureReason: None,
            Decisions: vec![],
            ImmediateActs: vec![],
            MaturedDeferredActs: vec![],
            PendingDeferredActs: vec![],
        }];
        let same = expected.clone();
        let matching = CompareTrace(&expected, &same);
        assert!(
            matching.Matches,
            "expected identical traces to compare as matching"
        );

        let mut mismatched = expected.clone();
        mismatched[0].Status = DwRunStatus::Failed;
        let mismatch = CompareTrace(&expected, &mismatched);
        assert!(
            !mismatch.Matches,
            "expected status mismatch to produce non-matching comparison"
        );
        assert_eq!(
            mismatch.FirstMismatchIndex,
            Some(0),
            "expected first mismatch index to identify first divergent entry"
        );
        assert!(
            FormatComparison(&mismatch).contains("mismatch"),
            "expected formatted comparison text to include mismatch summary"
        );

        let length_mismatch = CompareTrace(&expected, &[]);
        assert_eq!(
            length_mismatch.FirstMismatchIndex,
            Some(0),
            "expected length mismatch to report first missing index"
        );
    }

    #[test]
    fn SaveRestoreProducesTraceEquivalentSequence() {
        let root = DwFrameId {
            Domain: 9,
            Local: 1,
        };
        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            match ctx.Pc() {
                0 => {
                    ctx.BoardMut().Set(Keys::Count, 1).unwrap();
                    ctx.MailboxMut().Enqueue(DwMessage::I32(30, 300));
                    Dw::WaitTicks(1, RootPhase::Wait)
                }
                1 => {
                    let _ = ctx.MailboxMut().ConsumeFront();
                    ctx.BoardMut().Set(Keys::Count, 2).unwrap();
                    Dw::Continue(RootPhase::Done)
                }
                2 => Dw::Complete(),
                _ => Dw::Fail("bad"),
            }
        }
        let mut reg_u = DwFrameRegistry::New();
        reg_u
            .Register(DwFrameDef {
                Id: root,
                Step: RootF,
                DebugName: "Root",
            })
            .unwrap();
        let mut uninterrupted = DwSession::New(reg_u, root, 0).unwrap();
        for _ in 0..4 {
            uninterrupted.Tick().unwrap();
        }

        let mut reg_split = DwFrameRegistry::New();
        reg_split
            .Register(DwFrameDef {
                Id: root,
                Step: RootF,
                DebugName: "Root",
            })
            .unwrap();
        let mut split = DwSession::New(reg_split, root, 0).unwrap();
        split.Tick().unwrap();
        let pre = split.Trace().to_vec();
        let chunk = split.ExportChunk();

        let mut reg_restore = DwFrameRegistry::New();
        reg_restore
            .Register(DwFrameDef {
                Id: root,
                Step: RootF,
                DebugName: "Root",
            })
            .unwrap();
        let mut restored = DwSession::FromChunk(reg_restore, chunk).unwrap();
        for _ in 0..3 {
            restored.Tick().unwrap();
        }

        let mut combined = pre;
        combined.extend_from_slice(restored.Trace());
        let comparison = CompareTrace(uninterrupted.Trace(), &combined);
        assert!(
            comparison.Matches,
            "restored trace should match uninterrupted trace: {}",
            FormatComparison(&comparison)
        );
        assert!(
            FormatTrace(uninterrupted.Trace()).contains("9:1"),
            "formatted trace should contain deterministic domain:local frame IDs"
        );
    }

    #[test]
    fn SteadyTickKeepsRootFrameAliveAndQuiescent() {
        #[derive(Clone, Copy)]
        enum Phase {
            Observe,
        }
        impl DwPhase for Phase {
            fn ToPc(self) -> u32 {
                0
            }
            fn FromPc(pc: u32) -> Option<Self> {
                if pc == 0 { Some(Phase::Observe) } else { None }
            }
        }
        let root = DwFrameId {
            Domain: 50,
            Local: 1,
        };
        fn RootF(_: &mut DwFrameCtx) -> DwControl {
            Dw::Steady()
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "SteadyRoot",
        })
        .unwrap();
        let mut session = DwSession::New(reg, root, Phase::Observe.ToPc()).unwrap();
        let first = session.Tick().unwrap();
        assert_eq!(
            first.Status,
            DwRunStatus::Steady,
            "steady control should report steady run status"
        );
        assert_eq!(
            first.StackDepth, 1,
            "steady should keep root frame on stack"
        );
        assert_eq!(
            first.Frame,
            Some(root),
            "steady should keep the same active frame"
        );
        assert_eq!(first.Pc, Some(0), "steady should keep phase pc unchanged");
        assert_eq!(
            first.Control,
            Some(DwControlSummary::Steady),
            "steady control summary should be recorded"
        );
        assert_eq!(first.FailureReason, None, "steady should not mark failure");
    }

    #[test]
    fn SteadyRepeatsAndRunUntilBlockedStopsAtSteady() {
        let root = DwFrameId {
            Domain: 50,
            Local: 2,
        };
        fn RootF(_: &mut DwFrameCtx) -> DwControl {
            Dw::Steady()
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "SteadyLoop",
        })
        .unwrap();
        let mut session = DwSession::New(reg, root, 0).unwrap();
        let first = session.Tick().unwrap();
        let second = session.Tick().unwrap();
        assert_eq!(
            first.Status,
            DwRunStatus::Steady,
            "first steady tick should report steady status"
        );
        assert_eq!(
            second.Status,
            DwRunStatus::Steady,
            "second steady tick should also report steady status"
        );
        assert_eq!(
            second.Tick, 1,
            "tick index should advance deterministically across repeated steady ticks"
        );
        let blocked = session.RunUntilBlocked(3).unwrap();
        assert_eq!(
            blocked.Status,
            DwRunStatus::Steady,
            "run-until-blocked should stop on steady status"
        );
    }

    #[test]
    fn KeepRootFrameTransformsRootCompleteIntoSteady() {
        let root = DwFrameId {
            Domain: 51,
            Local: 1,
        };
        fn RootF(_: &mut DwFrameCtx) -> DwControl {
            Dw::Complete()
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "RootComplete",
        })
        .unwrap();
        let mut session =
            DwSession::NewWithRootPolicy(reg, root, 0, DwRootPolicy::KeepRootFrame).unwrap();
        let first = session.Tick().unwrap();
        assert_eq!(
            first.Status,
            DwRunStatus::Steady,
            "root complete under keep-root policy should become steady"
        );
        assert_eq!(first.StackDepth, 1, "root frame should remain on stack");
        assert_eq!(first.Frame, Some(root), "root frame should stay active");
        assert_eq!(
            first.Control,
            Some(DwControlSummary::Complete),
            "control summary should still indicate original complete control"
        );
        let second = session.Tick().unwrap();
        assert_eq!(
            second.Status,
            DwRunStatus::Steady,
            "repeated ticks should remain deterministic under keep-root complete behavior"
        );
    }

    #[test]
    fn KeepRootFrameRejectsRootPopAndRootReplace() {
        let root_pop = DwFrameId {
            Domain: 51,
            Local: 2,
        };
        fn RootPop(_: &mut DwFrameCtx) -> DwControl {
            Dw::Pop()
        }
        let mut reg_pop = DwFrameRegistry::New();
        reg_pop
            .Register(DwFrameDef {
                Id: root_pop,
                Step: RootPop,
                DebugName: "RootPop",
            })
            .unwrap();
        let mut session_pop =
            DwSession::NewWithRootPolicy(reg_pop, root_pop, 0, DwRootPolicy::KeepRootFrame)
                .unwrap();
        let pop_tick = session_pop.Tick().unwrap();
        assert_eq!(
            pop_tick.Status,
            DwRunStatus::Failed,
            "root pop under keep-root policy should fail loudly"
        );
        assert_eq!(
            pop_tick.FailureReason,
            Some("cannot pop root frame under keep-root policy"),
            "root pop failure reason should identify keep-root policy violation"
        );

        let root_replace = DwFrameId {
            Domain: 51,
            Local: 3,
        };
        let child = DwFrameId {
            Domain: 51,
            Local: 4,
        };
        fn RootReplace(_: &mut DwFrameCtx) -> DwControl {
            Dw::Replace(DwFrameId {
                Domain: 51,
                Local: 4,
            })
        }
        fn Child(_: &mut DwFrameCtx) -> DwControl {
            Dw::Complete()
        }
        let mut reg_replace = DwFrameRegistry::New();
        reg_replace
            .Register(DwFrameDef {
                Id: root_replace,
                Step: RootReplace,
                DebugName: "RootReplace",
            })
            .unwrap();
        reg_replace
            .Register(DwFrameDef {
                Id: child,
                Step: Child,
                DebugName: "Child",
            })
            .unwrap();
        let mut session_replace =
            DwSession::NewWithRootPolicy(reg_replace, root_replace, 0, DwRootPolicy::KeepRootFrame)
                .unwrap();
        let replace_tick = session_replace.Tick().unwrap();
        assert_eq!(
            replace_tick.Status,
            DwRunStatus::Failed,
            "root replace under keep-root policy should fail loudly"
        );
        assert_eq!(
            replace_tick.FailureReason,
            Some("cannot replace root frame under keep-root policy"),
            "root replace failure reason should identify keep-root policy violation"
        );
    }

    #[test]
    fn KeepRootFramePersistsAcrossChunkRestoreAndRunUntilBlocked() {
        let root = DwFrameId {
            Domain: 51,
            Local: 5,
        };
        fn RootF(_: &mut DwFrameCtx) -> DwControl {
            Dw::Complete()
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "RootChunk",
        })
        .unwrap();

        let mut session =
            DwSession::NewWithRootPolicy(reg, root, 0, DwRootPolicy::KeepRootFrame).unwrap();
        let first = session.Tick().unwrap();
        assert_eq!(
            first.Status,
            DwRunStatus::Steady,
            "root complete should steady"
        );
        let chunk = session.ExportChunk();
        assert_eq!(
            chunk.RootPolicy,
            DwRootPolicy::KeepRootFrame,
            "runtime chunk should persist keep-root policy"
        );

        let mut reg_restore = DwFrameRegistry::New();
        reg_restore
            .Register(DwFrameDef {
                Id: root,
                Step: RootF,
                DebugName: "RootChunk",
            })
            .unwrap();
        let mut restored = DwSession::FromChunk(reg_restore, chunk).unwrap();
        assert_eq!(
            restored.RootPolicy(),
            DwRootPolicy::KeepRootFrame,
            "restored session should keep keep-root policy"
        );

        let blocked = restored.RunUntilBlocked(2).unwrap();
        assert_eq!(
            blocked.Status,
            DwRunStatus::Steady,
            "run-until-blocked should stop on keep-root steady state"
        );
        assert_eq!(
            blocked.StackDepth, 1,
            "restored keep-root session should retain root frame"
        );
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
mod m7_tests {
    use super::*;

    #[derive(Clone, Copy)]
    enum RootPhase {
        Decide,
    }
    impl DwPhase for RootPhase {
        fn ToPc(self) -> u32 {
            0
        }
        fn FromPc(pc: u32) -> Option<Self> {
            if pc == 0 { Some(Self::Decide) } else { None }
        }
    }

    mod Keys {
        use crate::DwKey;
        pub const Recover: DwKey<bool> = DwKey::New("Recover", 50);
    }

    fn RecoverScore(ctx: &DwFrameCtx) -> f32 {
        if ctx.Board().GetOr(Keys::Recover, false) {
            1.5
        } else {
            -0.1
        }
    }
    fn PatrolScore(_ctx: &DwFrameCtx) -> f32 {
        0.5
    }

    fn ChildPop(_ctx: &mut DwFrameCtx) -> DwControl {
        Dw::Pop()
    }

    fn BuildRegistry(root_step: DwFrameFn) -> (DwFrameRegistry, DwFrameId, DwFrameId, DwFrameId) {
        let root = DwFrameId {
            Domain: 10,
            Local: 1,
        };
        let recover = DwFrameId {
            Domain: 10,
            Local: 2,
        };
        let patrol = DwFrameId {
            Domain: 10,
            Local: 3,
        };
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: root_step,
            DebugName: "Root",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: recover,
            Step: ChildPop,
            DebugName: "Recover",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: patrol,
            Step: ChildPop,
            DebugName: "Patrol",
        })
        .unwrap();
        (reg, root, recover, patrol)
    }

    #[test]
    fn DecideClampsScoresAndPushesWinner() {
        fn Root(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 2,
                        },
                        RecoverScore,
                    ),
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 3,
                        },
                        PatrolScore,
                    ),
                ],
                DwDecideOptions {
                    Hysteresis: 0.1,
                    MinCommitTicks: 0,
                    TieBreak: DwTieBreak::First,
                },
            )
        }
        let root = DwFrameId {
            Domain: 10,
            Local: 1,
        };
        let recover = DwFrameId {
            Domain: 10,
            Local: 2,
        };
        let patrol = DwFrameId {
            Domain: 10,
            Local: 3,
        };
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: Root,
            DebugName: "Root",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: recover,
            Step: ChildPop,
            DebugName: "Recover",
        })
        .unwrap();
        reg.Register(DwFrameDef {
            Id: patrol,
            Step: ChildPop,
            DebugName: "Patrol",
        })
        .unwrap();
        let mut s = DwSession::New(reg, root, 0).unwrap();
        s.BoardMut().Set(Keys::Recover, true).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(
            t0.Control,
            Some(DwControlSummary::Push),
            "decide should push selected child on decision tick"
        );
        assert_eq!(
            t0.Frame,
            Some(recover),
            "highest clamped score should win when recover score clamps to 1.0"
        );
        assert_eq!(
            t0.Decisions[0].Candidates[0].1, 1.0,
            "recover score above 1.0 should clamp to 1.0"
        );
        assert_eq!(
            t0.Decisions[0].Candidates[1].1, 0.5,
            "patrol score should remain unchanged when already in range"
        );
        assert_eq!(
            t0.StackDepth, 2,
            "decision should push child and increase stack depth"
        );

        let root_pc = s.ExportChunk().Stack[0].Pc;
        assert_eq!(
            root_pc, 0,
            "decide should set resume PC to current parent PC"
        );

        s.BoardMut().Set(Keys::Recover, false).unwrap();
        let _ = s.Tick().unwrap();
        let t2 = s.Tick().unwrap();
        assert_eq!(
            t2.Decisions[0].Candidates[0].1, 0.0,
            "negative recover score should clamp to 0.0"
        );
    }

    #[test]
    fn DecideEmptyCandidatesFailsClearly() {
        fn Root(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[],
                DwDecideOptions {
                    Hysteresis: 0.1,
                    MinCommitTicks: 0,
                    TieBreak: DwTieBreak::First,
                },
            )
        }
        let (reg, root, _, _) = BuildRegistry(Root);
        let mut s = DwSession::New(reg, root, 0).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(
            t0.Status,
            DwRunStatus::Failed,
            "empty decide candidate list should fail the frame loudly"
        );
        assert_eq!(
            t0.FailureReason,
            Some("decide candidates cannot be empty"),
            "empty decide failure reason should be directly inspectable"
        );
    }

    #[test]
    fn TieBreakFirstPrefersAuthorOrder() {
        fn Half(_ctx: &DwFrameCtx) -> f32 {
            0.5
        }
        fn Root(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 3,
                        },
                        Half,
                    ),
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 2,
                        },
                        Half,
                    ),
                ],
                DwDecideOptions {
                    Hysteresis: 0.0,
                    MinCommitTicks: 0,
                    TieBreak: DwTieBreak::First,
                },
            )
        }
        let (reg, root, _, patrol) = BuildRegistry(Root);
        let mut s = DwSession::New(reg, root, 0).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(
            t0.Frame,
            Some(patrol),
            "tie-break First should pick the first candidate in authored order"
        );
    }

    #[test]
    fn TieBreakKeepCurrentRetainsCommittedWhenTied() {
        fn Half(_ctx: &DwFrameCtx) -> f32 {
            0.5
        }
        fn Root(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 3,
                        },
                        Half,
                    ),
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 2,
                        },
                        Half,
                    ),
                ],
                DwDecideOptions {
                    Hysteresis: 0.0,
                    MinCommitTicks: 0,
                    TieBreak: DwTieBreak::KeepCurrent,
                },
            )
        }
        let (reg, root, recover, patrol) = BuildRegistry(Root);
        let mut s = DwSession::New(reg, root, 0).unwrap();

        let first = s.Tick().unwrap();
        assert_eq!(
            first.Frame,
            Some(patrol),
            "first tie with no current should pick first candidate"
        );
        let _ = s.Tick().unwrap();
        let second = s.Tick().unwrap();
        assert_eq!(
            second.Frame,
            Some(patrol),
            "keep-current should retain current target when tied for best"
        );
        assert_ne!(
            second.Frame,
            Some(recover),
            "keep-current tie should not switch away from currently committed tied target"
        );
    }

    #[test]
    fn HysteresisRetainsAndThenAllowsSwitchOnMargin() {
        fn RecoverFromBoard(ctx: &DwFrameCtx) -> f32 {
            if ctx.Board().GetOr(Keys::Recover, false) {
                0.60
            } else {
                0.50
            }
        }
        fn PatrolFixed(_ctx: &DwFrameCtx) -> f32 {
            0.55
        }
        fn Root(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 2,
                        },
                        RecoverFromBoard,
                    ),
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 3,
                        },
                        PatrolFixed,
                    ),
                ],
                DwDecideOptions {
                    Hysteresis: 0.10,
                    MinCommitTicks: 0,
                    TieBreak: DwTieBreak::First,
                },
            )
        }
        let (reg, root, recover, patrol) = BuildRegistry(Root);
        let mut s = DwSession::New(reg, root, 0).unwrap();
        s.BoardMut().Set(Keys::Recover, true).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(
            t0.Frame,
            Some(recover),
            "recover should commit first when score is higher"
        );
        let _ = s.Tick().unwrap();

        s.BoardMut().Set(Keys::Recover, false).unwrap();
        let t2 = s.Tick().unwrap();
        assert_eq!(
            t2.Frame,
            Some(recover),
            "hysteresis should retain current target when challenger score does not exceed margin"
        );
        assert!(
            t2.Decisions[0].HysteresisApplied,
            "decision trace should flag hysteresis retention when margin blocks a switch"
        );

        fn PatrolStrong(_ctx: &DwFrameCtx) -> f32 {
            0.70
        }
        fn RootStrong(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 2,
                        },
                        RecoverFromBoard,
                    ),
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 3,
                        },
                        PatrolStrong,
                    ),
                ],
                DwDecideOptions {
                    Hysteresis: 0.10,
                    MinCommitTicks: 0,
                    TieBreak: DwTieBreak::First,
                },
            )
        }
        let (reg2, root2, _, patrol2) = BuildRegistry(RootStrong);
        let mut s2 = DwSession::New(reg2, root2, 0).unwrap();
        s2.BoardMut().Set(Keys::Recover, true).unwrap();
        let _ = s2.Tick().unwrap();
        let _ = s2.Tick().unwrap();
        s2.BoardMut().Set(Keys::Recover, false).unwrap();
        let t_switch = s2.Tick().unwrap();
        assert_eq!(
            t_switch.Frame,
            Some(patrol2),
            "challenger should win once score exceeds current by at least hysteresis margin"
        );
        assert_eq!(
            root, root2,
            "test setup should keep root identities consistent"
        );
        assert_eq!(
            patrol, patrol2,
            "test setup should keep patrol identities consistent"
        );
    }

    #[test]
    fn MinCommitRetainsUntilWindowExpiresAndAgeAdvances() {
        fn RecoverLow(_ctx: &DwFrameCtx) -> f32 {
            0.2
        }
        fn PatrolHigh(_ctx: &DwFrameCtx) -> f32 {
            0.9
        }
        fn Root(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 2,
                        },
                        RecoverLow,
                    ),
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 3,
                        },
                        PatrolHigh,
                    ),
                ],
                DwDecideOptions {
                    Hysteresis: 0.0,
                    MinCommitTicks: 2,
                    TieBreak: DwTieBreak::First,
                },
            )
        }

        let (reg, root, _recover, patrol) = BuildRegistry(Root);
        let mut s = DwSession::New(reg, root, 0).unwrap();
        let first = s.Tick().unwrap();
        assert_eq!(
            first.Frame,
            Some(patrol),
            "initial winner should commit to high score target"
        );
        assert_eq!(
            first.Decisions[0].CommitAge, 0,
            "initial commit age should start at zero"
        );
        let _ = s.Tick().unwrap();
        let second = s.Tick().unwrap();
        assert_eq!(
            second.Frame,
            Some(patrol),
            "same target should remain selected within min-commit window"
        );
        assert_eq!(
            second.Decisions[0].CommitAge, 1,
            "commit age should advance per decision tick"
        );
        assert!(
            second.Decisions[0].MinCommitApplied,
            "decision trace should mark min-commit retention while age is below threshold"
        );
    }

    #[test]
    fn PushResumeAndTracePersistenceAcrossRestore() {
        fn RecoverScoreLocal(ctx: &DwFrameCtx) -> f32 {
            if ctx.Tick() < 2 { 0.8 } else { 0.2 }
        }
        fn PatrolScoreLocal(_ctx: &DwFrameCtx) -> f32 {
            0.6
        }
        fn Root(ctx: &mut DwFrameCtx) -> DwControl {
            Dw::Decide(
                ctx,
                &[
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 2,
                        },
                        RecoverScoreLocal,
                    ),
                    Dw::When(
                        DwFrameId {
                            Domain: 10,
                            Local: 3,
                        },
                        PatrolScoreLocal,
                    ),
                ],
                DwDecideOptions {
                    Hysteresis: 0.0,
                    MinCommitTicks: 2,
                    TieBreak: DwTieBreak::KeepCurrent,
                },
            )
        }

        let (reg_u, root, recover, patrol) = BuildRegistry(Root);
        let mut uninterrupted = DwSession::New(reg_u, root, 0).unwrap();
        for _ in 0..6 {
            uninterrupted.Tick().unwrap();
        }

        let (reg_s, root_s, _, _) = BuildRegistry(Root);
        let mut split = DwSession::New(reg_s, root_s, 0).unwrap();
        let d0 = split.Tick().unwrap();
        assert_eq!(
            d0.Frame,
            Some(recover),
            "decide should push selected child and child should become active next"
        );
        let c1 = split.Tick().unwrap();
        assert_eq!(
            c1.Frame,
            Some(root),
            "child pop should return control to parent on later tick"
        );
        assert_eq!(
            c1.Pc,
            Some(0),
            "parent should resume same PC after child pop for re-evaluation"
        );

        let chunk = split.ExportChunk();
        assert!(
            !chunk.DecisionMemory.is_empty(),
            "runtime chunk should persist decision commitment memory for restore continuity"
        );

        let (reg_r, _, _, _) = BuildRegistry(Root);
        let mut restored = DwSession::FromChunk(reg_r, chunk).unwrap();
        for _ in 0..4 {
            restored.Tick().unwrap();
        }

        let mut combined = split.Trace().to_vec();
        combined.extend_from_slice(restored.Trace());
        let cmp = CompareTrace(uninterrupted.Trace(), &combined);
        assert!(
            cmp.Matches,
            "restored run should match uninterrupted trace for min-commit decision continuity: {}",
            FormatComparison(&cmp)
        );

        let formatted = FormatTraceEntry(&combined[0]);
        assert!(
            formatted.contains("decisions=["),
            "formatted trace entry should include deterministic decision summary text"
        );
        assert!(
            combined[0].Decisions[0]
                .Candidates
                .iter()
                .any(|(id, _)| *id == recover)
                && combined[0].Decisions[0]
                    .Candidates
                    .iter()
                    .any(|(id, _)| *id == patrol),
            "decision records should include candidate target IDs for inspectability"
        );
    }

    #[test]
    fn M8ActuationImmediateAndDeferredTimingAndPersistence() {
        #[derive(Clone, Copy)]
        enum P {
            Start,
            Wait,
            Done,
        }
        impl DwPhase for P {
            fn ToPc(self) -> u32 {
                match self {
                    P::Start => 0,
                    P::Wait => 1,
                    P::Done => 2,
                }
            }
            fn FromPc(pc: u32) -> Option<Self> {
                match pc {
                    0 => Some(P::Start),
                    1 => Some(P::Wait),
                    2 => Some(P::Done),
                    _ => None,
                }
            }
        }
        let root = DwFrameId {
            Domain: 20,
            Local: 1,
        };
        let act_a = DwActId {
            Domain: 20,
            Local: 10,
        };
        let act_b = DwActId {
            Domain: 20,
            Local: 11,
        };
        fn RootF(ctx: &mut DwFrameCtx) -> DwControl {
            let act_a = DwActId {
                Domain: 20,
                Local: 10,
            };
            let act_b = DwActId {
                Domain: 20,
                Local: 11,
            };
            match ctx.Phase::<P>() {
                Some(P::Start) => {
                    ctx.Immediate(act_a);
                    ctx.Immediate(act_b);
                    ctx.Deferred(act_b, 0);
                    Dw::WaitTicks(1, P::Wait)
                }
                Some(P::Wait) => Dw::Continue(P::Done),
                Some(P::Done) => Dw::Complete(),
                None => Dw::Fail("phase"),
            }
        }
        let mut reg = DwFrameRegistry::New();
        reg.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        let mut s = DwSession::New(reg, root, 0).unwrap();
        let t0 = s.Tick().unwrap();
        assert_eq!(
            t0.ImmediateActs,
            vec![DwActRequest { Id: act_a }, DwActRequest { Id: act_b }],
            "immediate acts should preserve in-frame emission order"
        );
        assert_eq!(
            t0.MaturedDeferredActs,
            Vec::<DwActRequest>::new(),
            "deferred acts should not mature in the same tick they are scheduled"
        );
        assert_eq!(
            t0.PendingDeferredActs.len(),
            1,
            "deferred scheduling should create pending deferred state"
        );
        let chunk = s.ExportChunk();
        let mut reg2 = DwFrameRegistry::New();
        reg2.Register(DwFrameDef {
            Id: root,
            Step: RootF,
            DebugName: "Root",
        })
        .unwrap();
        let mut restored = DwSession::FromChunk(reg2, chunk).unwrap();
        let t1 = restored.Tick().unwrap();
        assert_eq!(
            t1.MaturedDeferredActs,
            vec![DwActRequest { Id: act_b }],
            "deferred act scheduled at tick N with delay 0 should mature at start of tick N+1"
        );
        assert_eq!(
            t1.ImmediateActs,
            Vec::<DwActRequest>::new(),
            "waiting tick should not invent immediate acts"
        );
        assert_eq!(
            restored.Trace()[0].MaturedDeferredActs,
            vec![DwActRequest { Id: act_b }],
            "matured deferred acts should be included in trace entries"
        );
    }
}
