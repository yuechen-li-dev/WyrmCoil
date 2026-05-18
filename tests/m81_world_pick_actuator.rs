#![allow(non_snake_case)]

use wyrmcoil::Engine::primitives::EntityId;
use wyrmcoil::Engine::ray::{
    ExecuteWorldPickRequestById, RayQueryId, RayQueryOutcome, RayQueryRequest, RayVec3,
    WorldPickFailureKind, WorldPickRayQueryRequest,
};
use wyrmcoil::Engine::world::{PickableTriangle, WorldBlackboard, WorldCameraResource};
use wyrmcoil::{DwMailbox, DwMessage};

fn CameraResource() -> WorldCameraResource {
    WorldCameraResource {
        Position: RayVec3 {
            X: 0.0,
            Y: 0.0,
            Z: 0.0,
        },
        Forward: RayVec3 {
            X: 0.0,
            Y: 0.0,
            Z: -1.0,
        },
        Up: RayVec3 {
            X: 0.0,
            Y: 1.0,
            Z: 0.0,
        },
        VerticalFovDegrees: 60.0,
        Width: 64,
        Height: 64,
    }
}

fn Triangle(entity_id: usize, triangle_id: i32) -> PickableTriangle {
    PickableTriangle {
        EntityId: EntityId(entity_id),
        TriangleId: triangle_id,
        A: RayVec3 {
            X: -0.5,
            Y: -0.5,
            Z: -1.0,
        },
        B: RayVec3 {
            X: 0.5,
            Y: -0.5,
            Z: -1.0,
        },
        C: RayVec3 {
            X: 0.0,
            Y: 0.5,
            Z: -1.0,
        },
    }
}

#[test]
fn ActuatorizedWorldPickHitStoresOutcomeAndStagesCompletion() {
    let query_id = RayQueryId(8100);
    let mut board = WorldBlackboard::New();
    board.Camera = Some(CameraResource());
    board
        .Input
        .SetCursorScreen(0.5, 0.5)
        .expect("cursor should be valid");
    board
        .Geometry
        .RegisterTriangle(Triangle(71, 1701))
        .expect("triangle registration should succeed");
    board
        .RayRequests
        .Insert(RayQueryRequest::WorldPick(WorldPickRayQueryRequest {
            QueryId: query_id,
        }));
    let mut mailbox = DwMailbox::New();

    ExecuteWorldPickRequestById(query_id, &mut board, 8101, &mut mailbox)
        .expect("world pick act should execute");

    assert!(
        !board.RayRequests.Contains(query_id),
        "executed world pick request should be consumed"
    );
    match board.RayResults.GetOutcome(query_id) {
        Some(RayQueryOutcome::Hit(hit)) => {
            assert_eq!(
                hit.TriangleId, 1701,
                "hit triangle id should match world geometry source"
            );
        }
        _ => panic!("expected world pick hit outcome in shared ray result store"),
    }
    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![DwMessage::I32(8101, 8100)],
        "completion should stage query-id payload only"
    );
    mailbox.BeginTick();
    assert_eq!(
        mailbox.ConsumeFirstKind(8101),
        Some(DwMessage::I32(8101, 8100)),
        "completion should promote and consume deterministically"
    );
}

#[test]
fn ActuatorizedWorldPickMissStagesCompletionAndStoresMiss() {
    let query_id = RayQueryId(8102);
    let mut board = WorldBlackboard::New();
    board.Camera = Some(CameraResource());
    board
        .Input
        .SetCursorScreen(0.05, 0.5)
        .expect("cursor should be valid");
    board
        .RayRequests
        .Insert(RayQueryRequest::WorldPick(WorldPickRayQueryRequest {
            QueryId: query_id,
        }));
    let mut mailbox = DwMailbox::New();

    ExecuteWorldPickRequestById(query_id, &mut board, 8103, &mut mailbox)
        .expect("world pick miss should still execute");

    assert!(
        matches!(
            board.RayResults.GetOutcome(query_id),
            Some(RayQueryOutcome::Miss(_))
        ),
        "miss should be stored in ray results"
    );
    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![DwMessage::I32(8103, 8102)],
        "miss completion should still stage query id only"
    );
}

#[test]
fn MissingCameraAndCursorBecomeStoredFailuresAndStillComplete() {
    let mut board = WorldBlackboard::New();
    let mut mailbox = DwMailbox::New();

    let missing_camera_id = RayQueryId(8104);
    board
        .RayRequests
        .Insert(RayQueryRequest::WorldPick(WorldPickRayQueryRequest {
            QueryId: missing_camera_id,
        }));
    ExecuteWorldPickRequestById(missing_camera_id, &mut board, 8105, &mut mailbox)
        .expect("missing camera should become stored failure, not execution error");
    assert!(
        matches!(
            board.RayResults.GetOutcome(missing_camera_id),
            Some(RayQueryOutcome::WorldPickFailure(f)) if f.Error == WorldPickFailureKind::MissingCamera
        ),
        "missing camera should persist structured world-blackboard failure"
    );

    board.Camera = Some(CameraResource());
    let missing_cursor_id = RayQueryId(8106);
    board
        .RayRequests
        .Insert(RayQueryRequest::WorldPick(WorldPickRayQueryRequest {
            QueryId: missing_cursor_id,
        }));
    ExecuteWorldPickRequestById(missing_cursor_id, &mut board, 8107, &mut mailbox)
        .expect("missing cursor should become stored failure, not execution error");
    assert!(
        matches!(
            board.RayResults.GetOutcome(missing_cursor_id),
            Some(RayQueryOutcome::WorldPickFailure(f)) if f.Error == WorldPickFailureKind::MissingCursor
        ),
        "missing cursor should persist structured world-blackboard failure"
    );

    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![DwMessage::I32(8105, 8104), DwMessage::I32(8107, 8106)],
        "stored failures should still stage completion for control routing"
    );
}

#[test]
fn MissingRequestAndWrongKindReturnExecutionErrorsWithoutSideEffects() {
    let mut board = WorldBlackboard::New();
    let mut mailbox = DwMailbox::New();

    let missing = ExecuteWorldPickRequestById(RayQueryId(8999), &mut board, 8108, &mut mailbox)
        .expect_err("missing world pick request should return structured execution error");
    assert!(
        format!("{missing:?}").contains("MissingRequest"),
        "missing request should be surfaced as MissingRequest execution error"
    );
    assert!(
        mailbox.StagedSnapshot().is_empty(),
        "missing request should not stage completion"
    );

    board.RayRequests.Insert(RayQueryRequest::CameraRay(
        wyrmcoil::Engine::ray::CameraRayRequest {
            QueryId: RayQueryId(8110),
            ScreenX: 0.5,
            ScreenY: 0.5,
        },
    ));
    let wrong_kind = ExecuteWorldPickRequestById(RayQueryId(8110), &mut board, 8109, &mut mailbox)
        .expect_err("wrong request kind should return structured execution error");
    assert!(
        format!("{wrong_kind:?}").contains("WrongRequestKind"),
        "wrong kind should be surfaced as WrongRequestKind execution error"
    );
    assert!(
        board.RayResults.GetOutcome(RayQueryId(8110)).is_none(),
        "wrong kind should not store world pick result"
    );
    assert!(
        mailbox.StagedSnapshot().is_empty(),
        "wrong kind should not stage completion"
    );
}
