#![allow(non_snake_case)]

use margaret_core::camera::Camera;
use margaret_core::math::{Point3, Vec3};
use wyrmcoil::Engine::primitives::{EntityId, RenderItem, RenderSnapshot, Vec2};
use wyrmcoil::Engine::ray::margaret::MargaretCameraRayAdapter;
use wyrmcoil::Engine::ray::{
    PickVisibleRenderSnapshot, RayHitResult, RayMissResult, RayQueryId, RayQueryOutcome, RayVec3,
    RenderSnapshotRaySceneOptions, ResolveVisiblePickResult, VisiblePickError, VisiblePickResult,
};

fn DemoAdapter() -> MargaretCameraRayAdapter {
    MargaretCameraRayAdapter {
        Camera: Camera::New("main", Point3::ORIGIN, -Vec3::Z, Vec3::Y, 90.0),
        Width: 100,
        Height: 100,
    }
}

#[test]
fn PickVisibleCenterHitMapsToEntityAndRenderItem() {
    let snapshot = RenderSnapshot {
        Frame: 77,
        Items: vec![RenderItem {
            Entity: EntityId(77),
            Position: Vec2 { X: 0.0, Y: 0.0 },
            SpriteId: 0,
        }],
    };

    let result = PickVisibleRenderSnapshot(
        &snapshot,
        &DemoAdapter(),
        0.5,
        0.5,
        RayQueryId(7700),
        RenderSnapshotRaySceneOptions { PlaneZ: -1.0 },
    )
    .expect("center pick should execute and resolve source metadata");

    let hit = match result {
        VisiblePickResult::Hit(hit) => hit,
        _ => panic!("center pick should hit visible primitive"),
    };
    assert_eq!(hit.QueryId, RayQueryId(7700), "query id should round-trip");
    assert_eq!(
        hit.EntityId,
        EntityId(77),
        "entity id should map from source"
    );
    assert_eq!(hit.RenderItemIndex, 0, "single item should map to index 0");
    assert!(
        hit.TriangleIndexInItem <= 1,
        "triangle index within item should be 0 or 1"
    );
    assert!(
        (hit.Position.Z + 1.0).abs() < 0.0001,
        "hit position should lie on configured z plane"
    );
    assert!(
        (hit.Distance - 1.0).abs() < 0.0001,
        "camera origin z=0 to plane z=-1 should be distance ~1"
    );
}

#[test]
fn PickVisibleMissReturnsMiss() {
    let snapshot = RenderSnapshot {
        Frame: 77,
        Items: vec![RenderItem {
            Entity: EntityId(88),
            Position: Vec2 { X: 0.9, Y: 0.0 },
            SpriteId: 0,
        }],
    };

    let result = PickVisibleRenderSnapshot(
        &snapshot,
        &DemoAdapter(),
        0.5,
        0.5,
        RayQueryId(7701),
        RenderSnapshotRaySceneOptions::default(),
    )
    .expect("valid pick coordinate should execute");

    assert!(
        matches!(result, VisiblePickResult::Miss(_)),
        "off-center geometry should miss center pick"
    );
}

#[test]
fn PickVisibleWithTwoItemsResolvesDeterministicItemMapping() {
    let snapshot = RenderSnapshot {
        Frame: 77,
        Items: vec![
            RenderItem {
                Entity: EntityId(31),
                Position: Vec2 { X: -0.5, Y: 0.0 },
                SpriteId: 0,
            },
            RenderItem {
                Entity: EntityId(32),
                Position: Vec2 { X: 0.5, Y: 0.0 },
                SpriteId: 0,
            },
        ],
    };

    let left = PickVisibleRenderSnapshot(
        &snapshot,
        &DemoAdapter(),
        0.25,
        0.5,
        RayQueryId(7702),
        RenderSnapshotRaySceneOptions::default(),
    )
    .expect("left pick should execute");
    let right = PickVisibleRenderSnapshot(
        &snapshot,
        &DemoAdapter(),
        0.75,
        0.5,
        RayQueryId(7703),
        RenderSnapshotRaySceneOptions::default(),
    )
    .expect("right pick should execute");

    match left {
        VisiblePickResult::Hit(hit) => {
            assert_eq!(
                hit.EntityId,
                EntityId(31),
                "left pick should hit left entity"
            );
            assert_eq!(hit.RenderItemIndex, 0, "left pick should map to first item");
        }
        _ => panic!("left pick should hit"),
    }

    match right {
        VisiblePickResult::Hit(hit) => {
            assert_eq!(
                hit.EntityId,
                EntityId(32),
                "right pick should hit right entity"
            );
            assert_eq!(
                hit.RenderItemIndex, 1,
                "right pick should map to second item"
            );
        }
        _ => panic!("right pick should hit"),
    }
}

#[test]
fn ResolveVisiblePickResultErrorsOnMissingTriangleSource() {
    let outcome = RayQueryOutcome::Hit(RayHitResult {
        QueryId: RayQueryId(7704),
        TriangleId: 999,
        Distance: 2.0,
        Position: RayVec3 {
            X: 0.0,
            Y: 0.0,
            Z: -2.0,
        },
        Normal: RayVec3 {
            X: 0.0,
            Y: 0.0,
            Z: 1.0,
        },
    });

    let error = ResolveVisiblePickResult(outcome, &[])
        .expect_err("missing source metadata should return structured error");
    assert_eq!(
        error,
        VisiblePickError::MissingTriangleSource { TriangleId: 999 },
        "missing source should include triangle id"
    );
}

#[test]
fn PickVisibleIsDeterministicForSameInputs() {
    let snapshot = RenderSnapshot {
        Frame: 77,
        Items: vec![RenderItem {
            Entity: EntityId(90),
            Position: Vec2 { X: 0.0, Y: 0.0 },
            SpriteId: 0,
        }],
    };

    let first = PickVisibleRenderSnapshot(
        &snapshot,
        &DemoAdapter(),
        0.5,
        0.5,
        RayQueryId(7705),
        RenderSnapshotRaySceneOptions::default(),
    )
    .expect("first pick should execute");
    let second = PickVisibleRenderSnapshot(
        &snapshot,
        &DemoAdapter(),
        0.5,
        0.5,
        RayQueryId(7705),
        RenderSnapshotRaySceneOptions::default(),
    )
    .expect("second pick should execute");

    assert_eq!(
        first, second,
        "pick API should be deterministic for same inputs"
    );

    let miss = ResolveVisiblePickResult(
        RayQueryOutcome::Miss(RayMissResult {
            QueryId: RayQueryId(7706),
        }),
        &[],
    )
    .expect("miss mapping should succeed");
    assert_eq!(
        miss,
        VisiblePickResult::Miss(wyrmcoil::Engine::ray::VisiblePickMiss {
            QueryId: RayQueryId(7706),
        }),
        "miss mapping should preserve query id"
    );
}
