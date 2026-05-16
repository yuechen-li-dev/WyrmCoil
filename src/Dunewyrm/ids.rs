#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use dunewyrm::{
    Dw, DwControl, DwDecideOptions, DwFrameCtx, DwFrameDef, DwFrameRegistry, DwMessage, DwPhase,
    DwTieBreak,
};

pub mod GuardFrames {
    use dunewyrm::DwFrameId;

    pub const Domain: u64 = 100;
    pub const Root: DwFrameId = DwFrameId { Domain, Local: 1 };
    pub const Patrol: DwFrameId = DwFrameId { Domain, Local: 2 };
    pub const Recover: DwFrameId = DwFrameId { Domain, Local: 3 };
}

pub mod GuardActs {
    use dunewyrm::DwActId;

    pub const Domain: u64 = 200;
    pub const Look: DwActId = DwActId { Domain, Local: 1 };
    pub const Step: DwActId = DwActId { Domain, Local: 2 };
    pub const RecoverSweep: DwActId = DwActId { Domain, Local: 3 };
    pub const CallBackup: DwActId = DwActId { Domain, Local: 4 };
}

pub mod Keys {
    use dunewyrm::DwKey;

    pub const TargetLost: DwKey<bool> = DwKey::New("TargetLost", 1);
    pub const RecoverAttempts: DwKey<i32> = DwKey::New("RecoverAttempts", 2);
    pub const Pressure: DwKey<f32> = DwKey::New("Pressure", 3);
}

pub mod MailKinds {
    pub const TargetLost: u32 = 1;
}

#[derive(Clone, Copy)]
enum PatrolPhase {
    Enter,
    Finish,
}
impl DwPhase for PatrolPhase {
    fn ToPc(self) -> u32 {
        match self {
            PatrolPhase::Enter => 0,
            PatrolPhase::Finish => 1,
        }
    }
    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(PatrolPhase::Enter),
            1 => Some(PatrolPhase::Finish),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum RecoverPhase {
    Enter,
    Finish,
}
impl DwPhase for RecoverPhase {
    fn ToPc(self) -> u32 {
        match self {
            RecoverPhase::Enter => 0,
            RecoverPhase::Finish => 1,
        }
    }
    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(RecoverPhase::Enter),
            1 => Some(RecoverPhase::Finish),
            _ => None,
        }
    }
}

pub fn BuildRegistry() -> DwFrameRegistry {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: GuardFrames::Root,
            Step: Root,
            DebugName: "GuardRoot",
        })
        .expect("Guard root frame id must register exactly once");
    registry
        .Register(DwFrameDef {
            Id: GuardFrames::Patrol,
            Step: Patrol,
            DebugName: "GuardPatrol",
        })
        .expect("Guard patrol frame id must register exactly once");
    registry
        .Register(DwFrameDef {
            Id: GuardFrames::Recover,
            Step: Recover,
            DebugName: "GuardRecover",
        })
        .expect("Guard recover frame id must register exactly once");
    registry
}

fn ScorePatrol(ctx: &DwFrameCtx) -> f32 {
    let lost = ctx.Board().GetOr(Keys::TargetLost, false);
    if lost { 0.1 } else { 0.9 }
}
fn ScoreRecover(ctx: &DwFrameCtx) -> f32 {
    let lost = ctx.Board().GetOr(Keys::TargetLost, false);
    let pressure = ctx.Board().GetOr(Keys::Pressure, 0.0);
    if lost {
        (0.65 + pressure).clamp(0.0, 1.0)
    } else {
        0.2
    }
}

fn Root(ctx: &mut DwFrameCtx) -> DwControl {
    while let Some(message) = ctx.MailboxMut().ConsumeFront() {
        if message.Kind == MailKinds::TargetLost {
            ctx.BoardMut()
                .Set(Keys::TargetLost, true)
                .expect("TargetLost bool key write must succeed");
            ctx.BoardMut()
                .Set(Keys::Pressure, 0.75)
                .expect("Pressure f32 key write must succeed");
        }
    }

    Dw::Decide(
        ctx,
        &[
            Dw::When(GuardFrames::Patrol, ScorePatrol),
            Dw::When(GuardFrames::Recover, ScoreRecover),
        ],
        DwDecideOptions {
            Hysteresis: 0.05,
            MinCommitTicks: 1,
            TieBreak: DwTieBreak::KeepCurrent,
        },
    )
}

fn Patrol(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<PatrolPhase>() {
        Some(PatrolPhase::Enter) => {
            ctx.Immediate(GuardActs::Look);
            ctx.Immediate(GuardActs::Step);
            ctx.BoardMut()
                .Set(Keys::Pressure, 0.1)
                .expect("Pressure f32 key write must succeed");
            Dw::Continue(PatrolPhase::Finish)
        }
        Some(PatrolPhase::Finish) => Dw::Pop(),
        None => Dw::Fail("guard patrol phase invalid"),
    }
}

fn Recover(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<RecoverPhase>() {
        Some(RecoverPhase::Enter) => {
            let attempts = ctx.Board().GetOr(Keys::RecoverAttempts, 0);
            ctx.BoardMut()
                .Set(Keys::RecoverAttempts, attempts + 1)
                .expect("RecoverAttempts i32 key write must succeed");
            ctx.Immediate(GuardActs::RecoverSweep);
            ctx.Deferred(GuardActs::CallBackup, 1);
            Dw::Continue(RecoverPhase::Finish)
        }
        Some(RecoverPhase::Finish) => Dw::Pop(),
        None => Dw::Fail("guard recover phase invalid"),
    }
}

pub fn TargetLostMessage() -> DwMessage {
    DwMessage {
        Kind: MailKinds::TargetLost,
        Value: 1,
    }
}
