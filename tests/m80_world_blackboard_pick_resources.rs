#![allow(non_snake_case)]

use wyrmcoil::Engine::primitives::EntityId;
use wyrmcoil::Engine::ray::{
    PickWorldBlackboard, RayQueryId, RayVec3, WorldBlackboardPickError, WorldPickResult,
};
use wyrmcoil::Engine::world::{
    PickableTriangle, WorldBlackboard, WorldCameraResource, WorldInputError,
};

fn Triangle(entity_id: usize, triangle_id: i32, z: f32, x_offset: f32) -> PickableTriangle {
    PickableTriangle {
        EntityId: EntityId(entity_id),
        TriangleId: triangle_id,
        A: RayVec3 {
            X: -0.5 + x_offset,
            Y: -0.5,
            Z: z,
        },
        B: RayVec3 {
            X: 0.5 + x_offset,
            Y: -0.5,
            Z: z,
        },
        C: RayVec3 {
            X: 0.0 + x_offset,
            Y: 0.5,
            Z: z,
        },
    }
}

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

#[test]
fn WorldBlackboardCameraAndInputStartUnset() {
    let board = WorldBlackboard::New();
    assert!(
        board.Camera.is_none(),
        "new world blackboard should start without camera resource"
    );
    assert!(
        board.Input.CursorScreen().is_none(),
        "new world blackboard should start without cursor resource"
    );
}

#[test]
fn WorldInputSetterValidatesAndStoresCursor() {
    let mut board = WorldBlackboard::New();
    board
        .Input
        .SetCursorScreen(0.25, 0.75)
        .expect("valid normalized cursor should be accepted");
    assert_eq!(
        board.Input.CursorScreen(),
        Some((0.25, 0.75)),
        "cursor should round-trip from world input resource"
    );

    let error = board
        .Input
        .SetCursorScreen(1.25, 0.5)
        .expect_err("out-of-range cursor should be rejected");
    assert_eq!(
        error,
        WorldInputError::InvalidCursor {
            CursorScreenX: 1.25,
            CursorScreenY: 0.5
        },
        "invalid cursor should return structured error"
    );
}

#[test]
fn WorldBlackboardClearResetsCameraAndInputResources() {
    let mut board = WorldBlackboard::New();
    board.Camera = Some(CameraResource());
    board
        .Input
        .SetCursorScreen(0.5, 0.5)
        .expect("cursor setup should succeed before clear");

    board.Clear();

    assert!(
        board.Camera.is_none(),
        "clear should remove world camera resource"
    );
    assert!(
        board.Input.CursorScreen().is_none(),
        "clear should remove world cursor resource"
    );
}

#[test]
fn PickWorldBlackboardReturnsStructuredMissingResourceErrors() {
    let mut board = WorldBlackboard::New();

    let missing_camera = PickWorldBlackboard(&board, RayQueryId(100))
        .expect_err("missing camera should fail before pick execution");
    assert_eq!(
        missing_camera,
        WorldBlackboardPickError::MissingCamera,
        "missing camera should map to explicit world-blackboard pick error"
    );

    board.Camera = Some(CameraResource());
    let missing_cursor = PickWorldBlackboard(&board, RayQueryId(101))
        .expect_err("missing cursor should fail before pick execution");
    assert_eq!(
        missing_cursor,
        WorldBlackboardPickError::MissingCursor,
        "missing cursor should map to explicit world-blackboard pick error"
    );
}

#[test]
fn PickWorldBlackboardHitMissAndDeterminism() {
    let mut board = WorldBlackboard::New();
    board.Camera = Some(CameraResource());
    board
        .Input
        .SetCursorScreen(0.5, 0.5)
        .expect("center cursor should be valid");

    let empty = PickWorldBlackboard(&board, RayQueryId(102))
        .expect("empty geometry pick should still execute");
    assert!(
        matches!(empty, WorldPickResult::Miss(_)),
        "empty world geometry should produce miss"
    );

    board
        .Geometry
        .RegisterTriangle(Triangle(31, 900, -1.0, 0.0))
        .expect("registering center triangle should succeed");
    let hit_first =
        PickWorldBlackboard(&board, RayQueryId(103)).expect("center pick should execute");
    let hit_second = PickWorldBlackboard(&board, RayQueryId(104))
        .expect("repeated center pick should execute deterministically");

    match (hit_first, hit_second) {
        (WorldPickResult::Hit(first), WorldPickResult::Hit(second)) => {
            assert_eq!(
                first.EntityId,
                EntityId(31),
                "center hit should resolve world entity metadata"
            );
            assert_eq!(
                first.TriangleId, 900,
                "center hit should resolve world triangle id"
            );
            assert_eq!(
                second.EntityId,
                EntityId(31),
                "repeated pick should preserve deterministic entity mapping"
            );
            assert_eq!(
                second.TriangleId, 900,
                "repeated pick should preserve deterministic triangle mapping"
            );
        }
        _ => panic!("expected hit results for deterministic center picks"),
    }

    board
        .Input
        .SetCursorScreen(0.0, 0.5)
        .expect("off-target cursor should still be valid normalized input");
    let off_target =
        PickWorldBlackboard(&board, RayQueryId(105)).expect("off-target pick should execute");
    assert!(
        matches!(off_target, WorldPickResult::Miss(_)),
        "off-target cursor should miss centered geometry"
    );
}
