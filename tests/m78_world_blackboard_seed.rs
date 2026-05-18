use wyrmcoil::Engine::primitives::EntityId;
use wyrmcoil::Engine::ray::{CameraRayRequest, RayQueryId, RayQueryRequest, RayVec3};
use wyrmcoil::Engine::world::{
    PickableTriangle, RegisterTriangleError, WorldBlackboard, WorldGeometryRegistry,
};

fn Triangle(entity_id: usize, triangle_id: i32, offset_x: f32) -> PickableTriangle {
    PickableTriangle {
        EntityId: EntityId(entity_id),
        TriangleId: triangle_id,
        A: RayVec3 {
            X: offset_x,
            Y: 0.0,
            Z: -1.0,
        },
        B: RayVec3 {
            X: offset_x + 1.0,
            Y: 0.0,
            Z: -1.0,
        },
        C: RayVec3 {
            X: offset_x,
            Y: 1.0,
            Z: -1.0,
        },
    }
}

#[test]
fn WorldBlackboardNewStartsEmpty() {
    let board = WorldBlackboard::New();
    assert!(
        board.Geometry.IsEmpty(),
        "new world geometry should be empty"
    );
    assert!(
        board.RayRequests.Snapshot().is_empty(),
        "new world ray request store should be empty"
    );
    assert!(
        board.RayResults.OutcomeSnapshot().is_empty(),
        "new world ray result store should be empty"
    );
}

#[test]
fn WorldBlackboardClearResetsGeometryAndRayStores() {
    let mut board = WorldBlackboard::New();

    board
        .Geometry
        .RegisterTriangle(Triangle(7, 12, 5.0))
        .expect("triangle should register before clear");

    let query_id = board.RayRequests.AllocateQueryId();
    board
        .RayRequests
        .Insert(RayQueryRequest::CameraRay(CameraRayRequest {
            QueryId: query_id,
            ScreenX: 0.5,
            ScreenY: 0.5,
        }));

    board.Clear();

    assert!(board.Geometry.IsEmpty(), "clear should empty geometry");
    assert!(
        board.RayRequests.Snapshot().is_empty(),
        "clear should empty ray requests"
    );
    assert!(
        board.RayResults.OutcomeSnapshot().is_empty(),
        "clear should empty ray results"
    );

    let reset_query_id = board.RayRequests.AllocateQueryId();
    assert_eq!(
        reset_query_id,
        RayQueryId(0),
        "clear should reset ray request query id allocator"
    );
}

#[test]
fn RegisterAndGetTrianglePreservesEntityId() {
    let mut registry = WorldGeometryRegistry::New();
    let triangle = Triangle(55, 100, 0.0);

    registry
        .RegisterTriangle(triangle)
        .expect("register triangle should succeed for new id");

    let stored = registry
        .GetTriangle(100)
        .expect("triangle id should be retrievable after register");
    assert_eq!(stored.EntityId, EntityId(55), "entity id must be preserved");
    assert_eq!(stored.TriangleId, 100, "triangle id must be preserved");
}

#[test]
fn SnapshotOrderIsDeterministicByTriangleId() {
    let mut registry = WorldGeometryRegistry::New();
    registry
        .RegisterTriangle(Triangle(2, 20, 0.0))
        .expect("register id 20 should succeed");
    registry
        .RegisterTriangle(Triangle(1, 10, 1.0))
        .expect("register id 10 should succeed");
    registry
        .RegisterTriangle(Triangle(3, 30, 2.0))
        .expect("register id 30 should succeed");

    let snapshot = registry.Snapshot();
    let ids: Vec<i32> = snapshot
        .iter()
        .map(|triangle| triangle.TriangleId)
        .collect();
    assert_eq!(
        ids,
        vec![10, 20, 30],
        "snapshot order must be deterministic and sorted by triangle id"
    );
}

#[test]
fn DuplicateTriangleIdIsRejected() {
    let mut registry = WorldGeometryRegistry::New();
    registry
        .RegisterTriangle(Triangle(9, 7, 0.0))
        .expect("first insert should succeed");

    let duplicate = registry.RegisterTriangle(Triangle(10, 7, 2.0));
    assert_eq!(
        duplicate,
        Err(RegisterTriangleError::DuplicateTriangleId { TriangleId: 7 }),
        "duplicate triangle ids should be rejected"
    );

    let still_original = registry
        .GetTriangle(7)
        .expect("original triangle should remain after duplicate rejection");
    assert_eq!(
        still_original.EntityId,
        EntityId(9),
        "duplicate insert should not replace existing triangle"
    );
}

#[test]
fn ClearAndMissingTriangleBehavior() {
    let mut registry = WorldGeometryRegistry::New();
    registry
        .RegisterTriangle(Triangle(11, 1, 0.0))
        .expect("insert id 1 should succeed");
    registry
        .RegisterTriangle(Triangle(12, 2, 1.0))
        .expect("insert id 2 should succeed");
    assert_eq!(registry.Len(), 2, "registry length should reflect inserts");

    registry.Clear();

    assert!(registry.IsEmpty(), "clear should empty geometry registry");
    assert!(
        registry.GetTriangle(1).is_none(),
        "missing triangle should return none after clear"
    );
}

#[test]
fn RayStoresRemainUsableThroughWorldBlackboard() {
    let mut board = WorldBlackboard::New();
    let query_id = board.RayRequests.AllocateQueryId();

    board
        .RayRequests
        .Insert(RayQueryRequest::CameraRay(CameraRayRequest {
            QueryId: query_id,
            ScreenX: 0.25,
            ScreenY: 0.75,
        }));

    let stored_request = board
        .RayRequests
        .Get(query_id)
        .expect("request should be stored through world blackboard");
    match stored_request {
        RayQueryRequest::CameraRay(request) => {
            assert_eq!(request.QueryId, query_id, "query id should match");
        }
        _ => panic!("stored request kind should match inserted camera-ray request"),
    }

    board
        .RayResults
        .StoreMissResult(wyrmcoil::Engine::ray::RayMissResult { QueryId: query_id });
    let outcome = board
        .RayResults
        .GetOutcome(query_id)
        .expect("result should be retrievable through world blackboard");
    assert!(
        matches!(outcome, wyrmcoil::Engine::ray::RayQueryOutcome::Miss(_)),
        "stored outcome should be miss for inserted miss result"
    );
}
