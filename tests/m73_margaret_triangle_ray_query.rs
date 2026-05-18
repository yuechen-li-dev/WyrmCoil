#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use wyrmcoil::Engine::ray::margaret::ExecuteTriangleRayQuery;
use wyrmcoil::Engine::ray::{
    Ray3, RayMissResult, RayQueryId, RayQueryOutcome, RayQueryStore, RayTriangle, RayTriangleScene,
    RayVec3, TriangleRayQueryRequest,
};
use wyrmcoil::{
    Dw, DwActId, DwActRequest, DwFrameCtx, DwFrameDef, DwFrameId, DwFrameRegistry, DwKey,
    DwMessage, DwPhase, DwSession,
};

mod Keys {
    use super::DwKey;
    pub const RayQueryId: DwKey<i32> = DwKey::New("RayQueryId", 302);
}
mod Acts {
    use super::DwActId;
    pub const Domain: u64 = 730;
    pub const ExecuteTriangleRayQuery: DwActId = DwActId { Domain, Local: 1 };
}
mod MailKinds {
    pub const RayQueryCompleted: u32 = 7300;
}

fn SampleScene() -> RayTriangleScene {
    RayTriangleScene {
        Triangles: vec![RayTriangle {
            Id: 44,
            A: RayVec3 {
                X: -1.0,
                Y: -1.0,
                Z: -5.0,
            },
            B: RayVec3 {
                X: 1.0,
                Y: -1.0,
                Z: -5.0,
            },
            C: RayVec3 {
                X: 0.0,
                Y: 1.0,
                Z: -5.0,
            },
        }],
    }
}

fn HitRay(query_id: u32) -> TriangleRayQueryRequest {
    TriangleRayQueryRequest {
        QueryId: RayQueryId(query_id),
        Ray: Ray3 {
            Origin: RayVec3 {
                X: 0.0,
                Y: 0.0,
                Z: 0.0,
            },
            Direction: RayVec3 {
                X: 0.0,
                Y: 0.0,
                Z: -1.0,
            },
        },
    }
}

#[test]
fn DirectTriangleHitStoresExpectedPayload() {
    let mut store = RayQueryStore::New();
    ExecuteTriangleRayQuery(HitRay(1), &SampleScene(), &mut store);
    let hit = store
        .GetHitResult(RayQueryId(1))
        .expect("expected hit payload for query id");
    assert_eq!(
        hit.TriangleId, 44,
        "triangle id should be preserved across bridge"
    );
    assert!(
        (hit.Distance - 5.0).abs() < 0.0001,
        "distance should be approximately 5.0"
    );
    assert!(
        (hit.Position.Z + 5.0).abs() < 0.0001,
        "hit point should be on z=-5 plane"
    );
    assert!(
        (hit.Normal.LengthSquared() - 1.0).abs() < 0.0001,
        "normal should be normalized"
    );
}

#[test]
fn DirectTriangleMissStoresMissOutcome() {
    let mut store = RayQueryStore::New();
    let request = TriangleRayQueryRequest {
        QueryId: RayQueryId(2),
        Ray: Ray3 {
            Origin: RayVec3 {
                X: 0.0,
                Y: 0.0,
                Z: 0.0,
            },
            Direction: RayVec3 {
                X: 1.0,
                Y: 0.0,
                Z: 0.0,
            },
        },
    };
    ExecuteTriangleRayQuery(request, &SampleScene(), &mut store);
    assert_eq!(
        store.GetHitResult(RayQueryId(2)),
        None,
        "miss should not expose hit payload"
    );
    assert_eq!(
        store.GetOutcome(RayQueryId(2)),
        Some(RayQueryOutcome::Miss(RayMissResult {
            QueryId: RayQueryId(2)
        })),
        "miss outcome should be retained"
    );
}

#[test]
fn TriangleQueryDeterminismReturnsStableHit() {
    let scene = SampleScene();
    let mut store = RayQueryStore::New();
    ExecuteTriangleRayQuery(HitRay(6), &scene, &mut store);
    let first = store
        .GetHitResult(RayQueryId(6))
        .expect("first result should be hit");
    ExecuteTriangleRayQuery(HitRay(6), &scene, &mut store);
    let second = store
        .GetHitResult(RayQueryId(6))
        .expect("second result should be hit");
    assert_eq!(
        first, second,
        "same scene/ray/query id should produce same stored hit"
    );
}

#[test]
fn StoreSnapshotOrderingIsDeterministicAcrossHitAndMiss() {
    let scene = SampleScene();
    let mut store = RayQueryStore::New();
    ExecuteTriangleRayQuery(HitRay(7), &scene, &mut store);
    ExecuteTriangleRayQuery(
        TriangleRayQueryRequest {
            QueryId: RayQueryId(3),
            Ray: Ray3 {
                Origin: RayVec3 {
                    X: 0.0,
                    Y: 0.0,
                    Z: 0.0,
                },
                Direction: RayVec3 {
                    X: 0.0,
                    Y: 1.0,
                    Z: 0.0,
                },
            },
        },
        &scene,
        &mut store,
    );
    let snapshot = store.OutcomeSnapshot();
    assert_eq!(
        snapshot.len(),
        2,
        "both hit and miss should be present in snapshot"
    );
    assert!(
        matches!(
            snapshot[0],
            RayQueryOutcome::Miss(RayMissResult {
                QueryId: RayQueryId(3)
            })
        ),
        "btree ordering should place query id 3 first"
    );
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
            ctx.Immediate(Acts::ExecuteTriangleRayQuery);
            Dw::Continue(RootPhase::Finish)
        }
        Some(RootPhase::Finish) => Dw::Steady(),
        None => Dw::Fail("invalid root phase"),
    }
}

#[test]
fn ActuatorBoundaryStagesCompletionAndResolvesTriangleHitNextTick() {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: DwFrameId {
                Domain: 731,
                Local: 1,
            },
            Step: Root,
            DebugName: "Root",
        })
        .expect("root registers once");
    let mut session = DwSession::New(
        registry,
        DwFrameId {
            Domain: 731,
            Local: 1,
        },
        0,
    )
    .expect("session should construct");
    session
        .BoardMut()
        .Set(Keys::RayQueryId, 17)
        .expect("query id write should succeed");

    let scene = SampleScene();
    let request = HitRay(17);
    let mut store = RayQueryStore::New();

    let first = session.Tick().expect("request tick should succeed");
    DispatchRayActs(
        session.Board().GetOr(Keys::RayQueryId, -1),
        &first.ImmediateActs,
        session.MailboxMut(),
        &mut store,
        &scene,
        request,
    );
    assert_eq!(
        session.Mailbox().StagedSnapshot(),
        vec![DwMessage::I32(MailKinds::RayQueryCompleted, 17)],
        "completion should be staged immediately"
    );
    assert!(
        session.Mailbox().VisibleSnapshot().is_empty(),
        "staged completion should remain hidden until next tick"
    );

    let second = session.Tick().expect("promotion tick should succeed");
    assert!(
        second
            .VisibleMailbox
            .contains(&DwMessage::I32(MailKinds::RayQueryCompleted, 17)),
        "completion should be visible after promotion"
    );
    let outcome = store
        .GetOutcome(RayQueryId(17))
        .expect("result should exist in store");
    assert!(
        matches!(outcome, RayQueryOutcome::Hit(_)),
        "query id should resolve to hit outcome"
    );
}

fn DispatchRayActs(
    query_id: i32,
    acts: &[DwActRequest],
    mailbox: &mut wyrmcoil::DwMailbox,
    store: &mut RayQueryStore,
    scene: &RayTriangleScene,
    request: TriangleRayQueryRequest,
) {
    for act in acts {
        if act.Id == Acts::ExecuteTriangleRayQuery && query_id >= 0 {
            ExecuteTriangleRayQuery(request, scene, store);
            mailbox.Enqueue(DwMessage::I32(MailKinds::RayQueryCompleted, query_id));
        }
    }
}

trait RayVec3Len {
    fn LengthSquared(&self) -> f32;
}
impl RayVec3Len for RayVec3 {
    fn LengthSquared(&self) -> f32 {
        self.X * self.X + self.Y * self.Y + self.Z * self.Z
    }
}
