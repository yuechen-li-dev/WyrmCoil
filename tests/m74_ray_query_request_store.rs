#![allow(non_snake_case)]

use margaret_core::camera::Camera;
use margaret_core::math::{Point3, Vec3};
use wyrmcoil::Engine::ray::margaret::{
    ExecuteRayQueryRequestById, MargaretCameraRayAdapter, RayQueryExecutionError,
};
use wyrmcoil::Engine::ray::{
    CameraRayRequest, Ray3, RayQueryId, RayQueryOutcome, RayQueryRequest, RayQueryRequestStore,
    RayQueryStore, RayTriangle, RayTriangleScene, RayVec3, TriangleRayQueryRequest,
};
use wyrmcoil::{DwMailbox, DwMessage};

fn SampleScene() -> RayTriangleScene {
    RayTriangleScene {
        Triangles: vec![RayTriangle {
            Id: 91,
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

fn Adapter() -> MargaretCameraRayAdapter {
    MargaretCameraRayAdapter {
        Camera: Camera::New("main", Point3::ORIGIN, -Vec3::Z, Vec3::Y, 90.0),
        Width: 64,
        Height: 64,
    }
}

#[test]
fn RequestStoreAllocatesAndSnapshotsDeterministically() {
    let mut requests = RayQueryRequestStore::New();
    assert_eq!(
        requests.AllocateQueryId(),
        RayQueryId(0),
        "first request id must be 0"
    );
    assert_eq!(
        requests.AllocateQueryId(),
        RayQueryId(1),
        "second request id must be 1"
    );

    let replace_previous = requests.Insert(RayQueryRequest::CameraRay(CameraRayRequest {
        QueryId: RayQueryId(7),
        ScreenX: 0.25,
        ScreenY: 0.25,
    }));
    assert!(
        replace_previous.is_none(),
        "first insert should not replace existing request"
    );
    requests.Insert(RayQueryRequest::TriangleRay {
        Request: TriangleRayQueryRequest {
            QueryId: RayQueryId(2),
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
        },
        Scene: SampleScene(),
    });

    let snapshot = requests.Snapshot();
    assert_eq!(
        snapshot.len(),
        2,
        "snapshot should contain both inserted requests"
    );
    assert_eq!(
        snapshot[0].QueryId(),
        RayQueryId(2),
        "snapshot order should sort by query id"
    );
    assert_eq!(
        snapshot[1].QueryId(),
        RayQueryId(7),
        "snapshot order should sort by query id"
    );
}

#[test]
fn RequestStoreDuplicateIdReplaceRuleIsExplicit() {
    let mut requests = RayQueryRequestStore::New();
    requests.Insert(RayQueryRequest::CameraRay(CameraRayRequest {
        QueryId: RayQueryId(4),
        ScreenX: 0.1,
        ScreenY: 0.1,
    }));
    let replaced = requests.Insert(RayQueryRequest::CameraRay(CameraRayRequest {
        QueryId: RayQueryId(4),
        ScreenX: 0.9,
        ScreenY: 0.9,
    }));
    assert!(
        replaced.is_some(),
        "duplicate query id should replace previous request"
    );
}

#[test]
fn ExecuteCameraRequestByIdStoresResultStagesCompletionAndConsumesRequest() {
    let mut requests = RayQueryRequestStore::New();
    let query_id = RayQueryId(17);
    requests.Insert(RayQueryRequest::CameraRay(CameraRayRequest {
        QueryId: query_id,
        ScreenX: 0.5,
        ScreenY: 0.5,
    }));
    let mut results = RayQueryStore::New();
    let mut mailbox = DwMailbox::New();

    ExecuteRayQueryRequestById(
        query_id,
        &mut requests,
        &mut results,
        &Adapter(),
        &mut mailbox,
        7400,
    )
    .expect("camera request execution should succeed");

    assert!(
        results.GetCompleted(query_id).is_some(),
        "camera execution should store camera-ray result"
    );
    assert!(
        !requests.Contains(query_id),
        "executed request should be consumed from request store"
    );
    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![DwMessage::I32(7400, 17)],
        "completion should stage query-id only payload"
    );
    assert!(
        mailbox.VisibleSnapshot().is_empty(),
        "staged completion should not be visible before BeginTick"
    );
    mailbox.BeginTick();
    assert_eq!(
        mailbox.VisibleSnapshot(),
        vec![DwMessage::I32(7400, 17)],
        "completion should be visible after mailbox promotion"
    );
}

#[test]
fn ExecuteTriangleRequestByIdStoresHitAndStagesCompletion() {
    let query_id = RayQueryId(18);
    let mut requests = RayQueryRequestStore::New();
    requests.Insert(RayQueryRequest::TriangleRay {
        Request: TriangleRayQueryRequest {
            QueryId: query_id,
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
        },
        Scene: SampleScene(),
    });
    let mut results = RayQueryStore::New();
    let mut mailbox = DwMailbox::New();

    ExecuteRayQueryRequestById(
        query_id,
        &mut requests,
        &mut results,
        &Adapter(),
        &mut mailbox,
        7401,
    )
    .expect("triangle request execution should succeed");

    assert!(
        matches!(results.GetOutcome(query_id), Some(RayQueryOutcome::Hit(_))),
        "triangle execution should produce hit outcome"
    );
    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![DwMessage::I32(7401, 18)],
        "completion mailbox should only carry query id"
    );
}

#[test]
fn ExecuteByIdMissingRequestReturnsStructuredErrorAndNoSideEffects() {
    let mut requests = RayQueryRequestStore::New();
    let mut results = RayQueryStore::New();
    let mut mailbox = DwMailbox::New();
    let missing = RayQueryId(999);

    let error = ExecuteRayQueryRequestById(
        missing,
        &mut requests,
        &mut results,
        &Adapter(),
        &mut mailbox,
        7402,
    )
    .expect_err("missing request should return execution error");
    assert_eq!(
        error,
        RayQueryExecutionError::MissingRequest { QueryId: missing },
        "error should preserve missing query id"
    );
    assert!(
        results.GetOutcome(missing).is_none(),
        "missing execution should not insert result"
    );
    assert!(
        mailbox.StagedSnapshot().is_empty(),
        "missing execution should not stage completion"
    );
}
