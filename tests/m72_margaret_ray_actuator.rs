#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use margaret_core::camera::Camera;
use margaret_core::math::{Point3, Vec3};
use wyrmcoil::Engine::ray::margaret::MargaretCameraRayAdapter;
use wyrmcoil::Engine::ray::{CameraRayRequest, RayQueryId, RayQueryStore};
use wyrmcoil::{
    Dw, DwActId, DwActRequest, DwFrameCtx, DwFrameDef, DwFrameId, DwFrameRegistry, DwKey,
    DwMessage, DwPhase, DwSession,
};

mod Keys {
    use super::DwKey;
    pub const RayQueryScreenX: DwKey<f32> = DwKey::New("RayQueryScreenX", 200);
    pub const RayQueryScreenY: DwKey<f32> = DwKey::New("RayQueryScreenY", 201);
    pub const RayQueryId: DwKey<i32> = DwKey::New("RayQueryId", 202);
}

mod Acts {
    use super::DwActId;
    pub const Domain: u64 = 720;
    pub const BuildCameraRay: DwActId = DwActId { Domain, Local: 1 };
}

mod MailKinds {
    pub const RayQueryCompleted: u32 = 7200;
}

#[derive(Clone, Copy)]
enum RootPhase {
    Request,
    Finish,
}

impl DwPhase for RootPhase {
    fn ToPc(self) -> u32 {
        match self {
            RootPhase::Request => 0,
            RootPhase::Finish => 1,
        }
    }

    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(RootPhase::Request),
            1 => Some(RootPhase::Finish),
            _ => None,
        }
    }
}

fn Root(ctx: &mut DwFrameCtx) -> wyrmcoil::DwControl {
    match ctx.Phase::<RootPhase>() {
        Some(RootPhase::Request) => {
            ctx.Immediate(Acts::BuildCameraRay);
            Dw::Continue(RootPhase::Finish)
        }
        Some(RootPhase::Finish) => Dw::Steady(),
        None => Dw::Fail("invalid root phase"),
    }
}

fn BuildRegistry() -> DwFrameRegistry {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: DwFrameId {
                Domain: 721,
                Local: 1,
            },
            Step: Root,
            DebugName: "Root",
        })
        .expect("root registers once");
    registry
}

fn DispatchRayActs(
    query_id: i32,
    screen_x: f32,
    screen_y: f32,
    acts: &[DwActRequest],
    mailbox: &mut wyrmcoil::DwMailbox,
    store: &mut RayQueryStore,
    adapter: &MargaretCameraRayAdapter,
) {
    for act in acts {
        if act.Id == Acts::BuildCameraRay {
            if query_id < 0 {
                continue;
            }
            let request = CameraRayRequest {
                QueryId: RayQueryId(query_id as u32),
                ScreenX: screen_x,
                ScreenY: screen_y,
            };
            let result = adapter.BuildCameraRay(request, store);
            mailbox.Enqueue(DwMessage::I32(
                MailKinds::RayQueryCompleted,
                result.QueryId.0 as i32,
            ));
        }
    }
}

#[test]
fn ActuatorBoundaryStagesCompletionAndResolvesResultNextTick() {
    let registry = BuildRegistry();
    let mut session = DwSession::New(
        registry,
        DwFrameId {
            Domain: 721,
            Local: 1,
        },
        0,
    )
    .expect("session should construct");

    session
        .BoardMut()
        .Set(Keys::RayQueryId, 17)
        .expect("query id board write should succeed");
    session
        .BoardMut()
        .Set(Keys::RayQueryScreenX, 0.5)
        .expect("screen x board write should succeed");
    session
        .BoardMut()
        .Set(Keys::RayQueryScreenY, 0.5)
        .expect("screen y board write should succeed");

    let adapter = MargaretCameraRayAdapter {
        Camera: Camera::New("main", Point3::ORIGIN, -Vec3::Z, Vec3::Y, 90.0),
        Width: 64,
        Height: 64,
    };
    let mut store = RayQueryStore::New();

    let first = session.Tick().expect("request tick should succeed");
    let query_id = session.Board().GetOr(Keys::RayQueryId, -1);
    let screen_x = session.Board().GetOr(Keys::RayQueryScreenX, 0.5);
    let screen_y = session.Board().GetOr(Keys::RayQueryScreenY, 0.5);
    DispatchRayActs(
        query_id,
        screen_x,
        screen_y,
        &first.ImmediateActs,
        session.MailboxMut(),
        &mut store,
        &adapter,
    );

    assert_eq!(
        session.Mailbox().StagedSnapshot(),
        vec![DwMessage::I32(MailKinds::RayQueryCompleted, 17)],
        "actuator dispatch should stage completion message"
    );
    assert!(
        session.Mailbox().VisibleSnapshot().is_empty(),
        "staged completion should not be visible in same tick"
    );

    let second = session.Tick().expect("promotion tick should succeed");
    assert!(
        second
            .VisibleMailbox
            .contains(&DwMessage::I32(MailKinds::RayQueryCompleted, 17)),
        "completion message should be visible after BeginTick promotion"
    );

    let consumed = session
        .MailboxMut()
        .ConsumeFirstKind(MailKinds::RayQueryCompleted);
    assert_eq!(
        consumed,
        Some(DwMessage::I32(MailKinds::RayQueryCompleted, 17))
    );

    let result = store
        .GetCompleted(RayQueryId(17))
        .expect("ray result exists");
    assert!((result.Direction.Z + 1.0).abs() < 0.01);
}
