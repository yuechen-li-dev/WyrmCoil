#![allow(non_snake_case)]

use crate::{
    Dw, DwActId, DwControl, DwFrameCtx, DwFrameDef, DwFrameId, DwFrameRegistry, DwKey, DwMessage,
    DwMessagePayload, DwPhase, DwRootPolicy, DwSession,
};

pub mod Frames {
    use super::DwFrameId;
    pub const Domain: u64 = 320;
    pub const ControllerRoot: DwFrameId = DwFrameId { Domain, Local: 1 };
    pub const HandleAlert: DwFrameId = DwFrameId { Domain, Local: 2 };
}

pub mod Acts {
    use super::DwActId;
    pub const Domain: u64 = 321;
    pub const BeginHandleAlert: DwActId = DwActId { Domain, Local: 1 };
    pub const CompleteHandleAlert: DwActId = DwActId { Domain, Local: 2 };
}

pub mod Keys {
    use super::DwKey;
    pub const AlertActive: DwKey<bool> = DwKey::New("AlertActive", 40);
    pub const AlertLevel: DwKey<i32> = DwKey::New("AlertLevel", 41);
}

pub mod MailKinds {
    pub const Alert: u32 = 40;
}

const AlertTtlTicks: u32 = 2;

#[derive(Clone, Copy)]
enum ControllerPhase {
    Poll,
    WaitForChild,
}

impl DwPhase for ControllerPhase {
    fn ToPc(self) -> u32 {
        match self {
            ControllerPhase::Poll => 0,
            ControllerPhase::WaitForChild => 1,
        }
    }
    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(ControllerPhase::Poll),
            1 => Some(ControllerPhase::WaitForChild),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum HandleAlertPhase {
    Start,
    Done,
}

impl DwPhase for HandleAlertPhase {
    fn ToPc(self) -> u32 {
        match self {
            HandleAlertPhase::Start => 0,
            HandleAlertPhase::Done => 1,
        }
    }
    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(HandleAlertPhase::Start),
            1 => Some(HandleAlertPhase::Done),
            _ => None,
        }
    }
}

fn ControllerRoot(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<ControllerPhase>() {
        Some(ControllerPhase::Poll) => {
            if let Some(message) = ctx.MailboxMut().ConsumeFirstKind(MailKinds::Alert) {
                let level = match message.Payload {
                    DwMessagePayload::I32(value) => value,
                    _ => return Dw::Fail("persistent controller expected i32 alert payload"),
                };
                ctx.BoardMut()
                    .SetBoolWithTtl(Keys::AlertActive, true, AlertTtlTicks)
                    .expect("alert active TTL write should succeed");
                ctx.BoardMut()
                    .SetI32WithTtl(Keys::AlertLevel, level, AlertTtlTicks, 0)
                    .expect("alert level TTL write should succeed");
                return Dw::Push(Frames::HandleAlert, ControllerPhase::WaitForChild);
            }
            Dw::Steady()
        }
        Some(ControllerPhase::WaitForChild) => Dw::Continue(ControllerPhase::Poll),
        None => Dw::Fail("persistent controller root phase invalid"),
    }
}

fn HandleAlert(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<HandleAlertPhase>() {
        Some(HandleAlertPhase::Start) => {
            ctx.Immediate(Acts::BeginHandleAlert);
            Dw::Continue(HandleAlertPhase::Done)
        }
        Some(HandleAlertPhase::Done) => {
            ctx.Immediate(Acts::CompleteHandleAlert);
            Dw::Pop()
        }
        None => Dw::Fail("persistent controller child phase invalid"),
    }
}

pub fn AlertMessage(level: i32) -> DwMessage {
    DwMessage::I32(MailKinds::Alert, level)
}

pub fn BuildPersistentControllerRegistry() -> DwFrameRegistry {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: Frames::ControllerRoot,
            Step: ControllerRoot,
            DebugName: "ControllerRoot",
        })
        .expect("controller root should register once");
    registry
        .Register(DwFrameDef {
            Id: Frames::HandleAlert,
            Step: HandleAlert,
            DebugName: "HandleAlert",
        })
        .expect("handle alert should register once");
    registry
}

pub fn NewPersistentControllerSession() -> DwSession {
    DwSession::NewWithRootPolicy(
        BuildPersistentControllerRegistry(),
        Frames::ControllerRoot,
        ControllerPhase::Poll.ToPc(),
        DwRootPolicy::KeepRootFrame,
    )
    .expect("persistent controller session should construct")
}
