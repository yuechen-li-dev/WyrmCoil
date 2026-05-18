#![allow(non_snake_case)]

use margaret_core::camera::Camera;
use margaret_core::math::{Point3, Vec3};
use wyrmcoil::Engine::ray::margaret::{
    ExecutePickRayQueryRequestById, ExecutePickTriangleQuery, RayQueryExecutionError,
};
use wyrmcoil::Engine::ray::{
    PickRayQueryRequest, RayQueryId, RayQueryOutcome, RayQueryRequest, RayQueryRequestStore,
    RayQueryStore, RayTriangle, RayTriangleScene, RayVec3,
};
use wyrmcoil::{DwMailbox, DwMessage};

fn Adapter() -> wyrmcoil::Engine::ray::margaret::MargaretCameraRayAdapter {
    wyrmcoil::Engine::ray::margaret::MargaretCameraRayAdapter {
        Camera: Camera::New("main", Point3::ORIGIN, -Vec3::Z, Vec3::Y, 90.0),
        Width: 64,
        Height: 64,
    }
}

fn FrontTriangleScene() -> RayTriangleScene {
    RayTriangleScene {
        Triangles: vec![RayTriangle {
            Id: 501,
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

#[test]
fn DirectPickHitComposesCameraAndTriangleQuery() {
    let mut results = RayQueryStore::New();
    let outcome = ExecutePickTriangleQuery(
        PickRayQueryRequest {
            QueryId: RayQueryId(1),
            ScreenX: 0.5,
            ScreenY: 0.5,
            Scene: FrontTriangleScene(),
        },
        &Adapter(),
        &mut results,
    )
    .expect("pick composition should succeed for center coordinate");

    let hit = match outcome {
        RayQueryOutcome::Hit(hit) => hit,
        _ => panic!("center pick should hit triangle"),
    };
    assert_eq!(
        hit.TriangleId, 501,
        "hit triangle id should match source scene"
    );
    assert!(
        (hit.Distance - 5.0).abs() < 0.0001,
        "hit distance should be near 5"
    );
    assert!(
        hit.Position.Z < -4.9 && hit.Position.Z > -5.1,
        "hit position should be around z=-5 plane"
    );
    let normal_len =
        (hit.Normal.X * hit.Normal.X + hit.Normal.Y * hit.Normal.Y + hit.Normal.Z * hit.Normal.Z)
            .sqrt();
    assert!(
        (normal_len - 1.0).abs() < 0.0001,
        "normal must be unit length"
    );
}

#[test]
fn DirectPickMissReturnsMissOutcome() {
    let mut results = RayQueryStore::New();
    let outcome = ExecutePickTriangleQuery(
        PickRayQueryRequest {
            QueryId: RayQueryId(2),
            ScreenX: 0.05,
            ScreenY: 0.5,
            Scene: FrontTriangleScene(),
        },
        &Adapter(),
        &mut results,
    )
    .expect("pick composition should run for valid screen coordinate");
    assert!(
        matches!(outcome, RayQueryOutcome::Miss(_)),
        "off-center pick should miss triangle"
    );
}

#[test]
fn PickRequestStorePathStoresFinalOutcomeAndStagesCompletion() {
    let query_id = RayQueryId(3);
    let mut requests = RayQueryRequestStore::New();
    requests.Insert(RayQueryRequest::PickTriangle(PickRayQueryRequest {
        QueryId: query_id,
        ScreenX: 0.5,
        ScreenY: 0.5,
        Scene: FrontTriangleScene(),
    }));
    let mut results = RayQueryStore::New();
    let mut mailbox = DwMailbox::New();

    ExecutePickRayQueryRequestById(
        query_id,
        &mut requests,
        &mut results,
        &Adapter(),
        &mut mailbox,
        7500,
    )
    .expect("pick request should execute by id");

    assert!(
        !requests.Contains(query_id),
        "pick request should be consumed after execution"
    );
    assert!(
        matches!(results.GetOutcome(query_id), Some(RayQueryOutcome::Hit(_))),
        "final pick outcome should be stored as hit/miss"
    );
    assert_eq!(
        results.GetCompleted(query_id),
        None,
        "pick path should store final hit/miss only, not intermediate camera-ray payload"
    );
    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![DwMessage::I32(7500, 3)],
        "completion mailbox should contain only query id payload"
    );
}

#[test]
fn PickMailboxPromotionPathIsDeterministic() {
    let query_id = RayQueryId(4);
    let mut requests = RayQueryRequestStore::New();
    requests.Insert(RayQueryRequest::PickTriangle(PickRayQueryRequest {
        QueryId: query_id,
        ScreenX: 0.5,
        ScreenY: 0.5,
        Scene: FrontTriangleScene(),
    }));
    let mut results = RayQueryStore::New();
    let mut mailbox = DwMailbox::New();
    ExecutePickRayQueryRequestById(
        query_id,
        &mut requests,
        &mut results,
        &Adapter(),
        &mut mailbox,
        7501,
    )
    .expect("pick execution should stage completion");

    assert!(
        mailbox.VisibleSnapshot().is_empty(),
        "completion should not be visible before promotion"
    );
    mailbox.BeginTick();
    assert_eq!(
        mailbox.VisibleSnapshot(),
        vec![DwMessage::I32(7501, 4)],
        "completion should appear after BeginTick"
    );
    let consumed = mailbox.ConsumeFirstKind(7501);
    assert_eq!(
        consumed,
        Some(DwMessage::I32(7501, 4)),
        "query id completion should be consumable exactly once"
    );
    assert!(
        results.GetOutcome(query_id).is_some(),
        "result should remain retrievable from result store"
    );
}

#[test]
fn InvalidAndMissingPickRequestsReturnStructuredErrorsWithoutSideEffects() {
    let adapter = Adapter();
    let mut requests = RayQueryRequestStore::New();
    let mut results = RayQueryStore::New();
    let mut mailbox = DwMailbox::New();

    let missing_error = ExecutePickRayQueryRequestById(
        RayQueryId(999),
        &mut requests,
        &mut results,
        &adapter,
        &mut mailbox,
        7502,
    )
    .expect_err("missing query id should return structured error");
    assert_eq!(
        missing_error,
        RayQueryExecutionError::MissingRequest {
            QueryId: RayQueryId(999)
        },
        "missing id should preserve query id"
    );

    requests.Insert(RayQueryRequest::PickTriangle(PickRayQueryRequest {
        QueryId: RayQueryId(8),
        ScreenX: 1.5,
        ScreenY: 0.5,
        Scene: FrontTriangleScene(),
    }));
    let invalid_error = ExecutePickRayQueryRequestById(
        RayQueryId(8),
        &mut requests,
        &mut results,
        &adapter,
        &mut mailbox,
        7502,
    )
    .expect_err("out-of-range screen coordinate should be rejected");
    assert!(
        matches!(
            invalid_error,
            RayQueryExecutionError::InvalidPickScreenCoordinate { .. }
        ),
        "invalid coordinate should surface explicit error"
    );

    assert!(
        mailbox.StagedSnapshot().is_empty(),
        "error paths should not stage completion messages"
    );
    assert!(
        results.GetOutcome(RayQueryId(8)).is_none(),
        "error paths should not store outcomes"
    );
}
