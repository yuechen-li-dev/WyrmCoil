#![allow(non_snake_case)]

use margaret_core::camera::Camera;
use margaret_core::math::{Point3, Vec3};
use wyrmcoil::Engine::primitives::{EntityId, RenderItem, RenderSnapshot, Vec2};
use wyrmcoil::Engine::ray::margaret::ExecutePickTriangleQuery;
use wyrmcoil::Engine::ray::{
    BuildVisiblePrimitiveRaySceneFromRenderSnapshot, PickRayQueryRequest, RayQueryId,
    RayQueryOutcome, RayQueryStore, RenderSnapshotRaySceneOptions,
};

fn DemoAdapter() -> wyrmcoil::Engine::ray::margaret::MargaretCameraRayAdapter {
    wyrmcoil::Engine::ray::margaret::MargaretCameraRayAdapter {
        Camera: Camera::New("main", Point3::ORIGIN, -Vec3::Z, Vec3::Y, 90.0),
        Width: 100,
        Height: 100,
    }
}

#[test]
fn BridgeBuildsDeterministicTrianglesAndSourcesOnFixedPlane() {
    let snapshot = RenderSnapshot {
        Frame: 76,
        Items: vec![RenderItem {
            Entity: EntityId(19),
            Position: Vec2 { X: 0.0, Y: 0.0 },
            SpriteId: 5,
        }],
    };

    let first = BuildVisiblePrimitiveRaySceneFromRenderSnapshot(
        &snapshot,
        RenderSnapshotRaySceneOptions { PlaneZ: -1.0 },
    );
    let second = BuildVisiblePrimitiveRaySceneFromRenderSnapshot(
        &snapshot,
        RenderSnapshotRaySceneOptions { PlaneZ: -1.0 },
    );

    assert_eq!(
        first, second,
        "bridge should be deterministic for same input"
    );
    assert_eq!(
        first.Scene.Triangles.len(),
        2,
        "one visible primitive item should expand to two triangles"
    );
    assert_eq!(
        first.TriangleSources.len(),
        2,
        "source mapping should include one mapping per triangle"
    );
    assert_eq!(
        first.Scene.Triangles[0].Id, 0,
        "first item first triangle id should start at zero"
    );
    assert_eq!(
        first.Scene.Triangles[1].Id, 1,
        "first item second triangle id should be one"
    );

    for triangle in &first.Scene.Triangles {
        assert_eq!(
            triangle.A.Z, -1.0,
            "triangle vertices should lie on bridge plane"
        );
        assert_eq!(
            triangle.B.Z, -1.0,
            "triangle vertices should lie on bridge plane"
        );
        assert_eq!(
            triangle.C.Z, -1.0,
            "triangle vertices should lie on bridge plane"
        );
    }

    assert_eq!(first.TriangleSources[0].RenderItemIndex, 0);
    assert_eq!(first.TriangleSources[1].RenderItemIndex, 0);
    assert_eq!(first.TriangleSources[0].EntityId, 19);
    assert_eq!(first.TriangleSources[1].EntityId, 19);
}

#[test]
fn PickCenterHitsBridgedRenderSnapshotGeometry() {
    let snapshot = RenderSnapshot {
        Frame: 76,
        Items: vec![RenderItem {
            Entity: EntityId(77),
            Position: Vec2 { X: 0.0, Y: 0.0 },
            SpriteId: 0,
        }],
    };
    let bridge = BuildVisiblePrimitiveRaySceneFromRenderSnapshot(
        &snapshot,
        RenderSnapshotRaySceneOptions { PlaneZ: -1.0 },
    );
    let mut store = RayQueryStore::New();

    let outcome = ExecutePickTriangleQuery(
        PickRayQueryRequest {
            QueryId: RayQueryId(7600),
            ScreenX: 0.5,
            ScreenY: 0.5,
            Scene: bridge.Scene.clone(),
        },
        &DemoAdapter(),
        &mut store,
    )
    .expect("center pick should execute without screen coordinate error");

    let hit = match outcome {
        RayQueryOutcome::Hit(hit) => hit,
        _ => panic!("center pick should hit bridged visible-primitive triangles"),
    };
    let source = bridge
        .TriangleSources
        .iter()
        .find(|source| source.TriangleId == hit.TriangleId)
        .expect("hit triangle id should map back to a render snapshot source item");

    assert_eq!(
        source.EntityId, 77,
        "hit triangle source should point at expected render item/entity"
    );
    assert!(
        (hit.Position.Z + 1.0).abs() < 0.0001,
        "hit position should lie near bridge plane z=-1"
    );
    assert!(
        (hit.Distance - 1.0).abs() < 0.0001,
        "origin camera at z=0 and bridge plane z=-1 should hit at distance near 1"
    );
}

#[test]
fn PickMissesWhenRenderItemIsOffCenter() {
    let snapshot = RenderSnapshot {
        Frame: 76,
        Items: vec![RenderItem {
            Entity: EntityId(88),
            Position: Vec2 { X: 0.9, Y: 0.0 },
            SpriteId: 0,
        }],
    };
    let bridge = BuildVisiblePrimitiveRaySceneFromRenderSnapshot(
        &snapshot,
        RenderSnapshotRaySceneOptions { PlaneZ: -1.0 },
    );
    let mut store = RayQueryStore::New();

    let outcome = ExecutePickTriangleQuery(
        PickRayQueryRequest {
            QueryId: RayQueryId(7601),
            ScreenX: 0.5,
            ScreenY: 0.5,
            Scene: bridge.Scene,
        },
        &DemoAdapter(),
        &mut store,
    )
    .expect("valid screen coordinate should run pick query");

    assert!(
        matches!(outcome, RayQueryOutcome::Miss(_)),
        "center pick should miss off-center bridged geometry"
    );
}
