#![allow(non_snake_case)]

use margaret_core::camera::Camera;
use margaret_core::math::{Point3, Vec3};
use wyrmcoil::Engine::primitives::EntityId;
use wyrmcoil::Engine::ray::margaret::MargaretCameraRayAdapter;
use wyrmcoil::Engine::ray::{
    BuildRayTriangleSceneFromWorldGeometryRegistry, PickWorldGeometryRegistry, RayHitResult,
    RayQueryId, RayQueryOutcome, RayVec3, ResolveWorldPickResult, WorldPickError, WorldPickResult,
};
use wyrmcoil::Engine::world::{PickableTriangle, WorldBlackboard, WorldGeometryRegistry};

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

fn CameraAdapter() -> MargaretCameraRayAdapter {
    MargaretCameraRayAdapter {
        Camera: Camera::New(
            "m79-test-camera",
            Point3::New(0.0, 0.0, 0.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::New(0.0, 1.0, 0.0),
            60.0,
        ),
        Width: 64,
        Height: 64,
    }
}

#[test]
fn BuildWorldGeometryRayScenePreservesTriangleAndEntityMetadata() {
    let mut registry = WorldGeometryRegistry::New();
    registry
        .RegisterTriangle(Triangle(77, 301, -1.0, 0.0))
        .expect("registering world triangle should succeed");

    let world_scene = BuildRayTriangleSceneFromWorldGeometryRegistry(&registry);

    assert_eq!(
        world_scene.Scene.Triangles.len(),
        1,
        "world scene should include exactly one triangle"
    );
    assert_eq!(
        world_scene.Scene.Triangles[0].Id, 301,
        "triangle id should be preserved from world registry"
    );
    assert_eq!(
        world_scene.Sources[0].EntityId,
        EntityId(77),
        "entity id metadata should be preserved from world registry"
    );
}

#[test]
fn PickWorldGeometryRegistryCenterHitReturnsWorldMetadata() {
    let mut registry = WorldGeometryRegistry::New();
    registry
        .RegisterTriangle(Triangle(11, 401, -1.0, 0.0))
        .expect("registering world triangle should succeed");
    let result = PickWorldGeometryRegistry(&registry, &CameraAdapter(), 0.5, 0.5, RayQueryId(900))
        .expect("world pick query should execute");

    match result {
        WorldPickResult::Hit(hit) => {
            assert_eq!(
                hit.QueryId,
                RayQueryId(900),
                "query id should round-trip through world pick hit"
            );
            assert_eq!(
                hit.EntityId,
                EntityId(11),
                "entity id should come from world source metadata"
            );
            assert_eq!(
                hit.TriangleId, 401,
                "triangle id should match world registry triangle"
            );
            assert!(
                hit.Distance > 0.9 && hit.Distance < 1.1,
                "center hit distance should be close to z=-1 plane"
            );
            assert!(
                hit.Position.Z < -0.9 && hit.Position.Z > -1.1,
                "hit position z should be near registered triangle plane"
            );
        }
        WorldPickResult::Miss(miss) => panic!(
            "expected world pick hit but received miss for query {:?}",
            miss.QueryId
        ),
    }
}

#[test]
fn PickWorldGeometryRegistryMissesOnEmptyAndOffTargetCases() {
    let empty_registry = WorldGeometryRegistry::New();
    let empty_result =
        PickWorldGeometryRegistry(&empty_registry, &CameraAdapter(), 0.5, 0.5, RayQueryId(901))
            .expect("empty-registry world pick should execute");
    assert!(
        matches!(empty_result, WorldPickResult::Miss(_)),
        "empty world registry should produce miss"
    );

    let mut populated_registry = WorldGeometryRegistry::New();
    populated_registry
        .RegisterTriangle(Triangle(12, 402, -1.0, 1.5))
        .expect("registering off-center world triangle should succeed");
    let off_target_result = PickWorldGeometryRegistry(
        &populated_registry,
        &CameraAdapter(),
        0.5,
        0.5,
        RayQueryId(902),
    )
    .expect("off-target world pick should execute");
    assert!(
        matches!(off_target_result, WorldPickResult::Miss(_)),
        "center pick should miss geometry translated off-screen"
    );
}

#[test]
fn PickWorldGeometryRegistryUsesDeterministicSourceMappingAcrossEntities() {
    let mut registry = WorldGeometryRegistry::New();
    registry
        .RegisterTriangle(Triangle(21, 501, -1.0, -0.9))
        .expect("registering left triangle should succeed");
    registry
        .RegisterTriangle(Triangle(22, 502, -1.0, 0.0))
        .expect("registering center triangle should succeed");

    let first = PickWorldGeometryRegistry(&registry, &CameraAdapter(), 0.5, 0.5, RayQueryId(903))
        .expect("first world pick should execute");
    let second = PickWorldGeometryRegistry(&registry, &CameraAdapter(), 0.5, 0.5, RayQueryId(904))
        .expect("second world pick should execute");

    match first {
        WorldPickResult::Hit(hit) => {
            assert_eq!(
                hit.EntityId,
                EntityId(22),
                "center pick should map to center-entity triangle source"
            );
            assert_eq!(
                hit.TriangleId, 502,
                "center pick should map to center triangle id"
            );
        }
        WorldPickResult::Miss(_) => panic!("expected hit for first deterministic pick"),
    }

    match second {
        WorldPickResult::Hit(hit) => {
            assert_eq!(
                hit.EntityId,
                EntityId(22),
                "repeated center pick should preserve source mapping deterministically"
            );
            assert_eq!(
                hit.TriangleId, 502,
                "repeated center pick should preserve triangle mapping deterministically"
            );
        }
        WorldPickResult::Miss(_) => panic!("expected hit for second deterministic pick"),
    }
}

#[test]
fn ResolveWorldPickResultErrorsWhenSourceMetadataIsMissing() {
    let outcome = RayQueryOutcome::Hit(RayHitResult {
        QueryId: RayQueryId(905),
        TriangleId: 999,
        Distance: 1.0,
        Position: RayVec3 {
            X: 0.0,
            Y: 0.0,
            Z: -1.0,
        },
        Normal: RayVec3 {
            X: 0.0,
            Y: 0.0,
            Z: 1.0,
        },
    });

    let error = ResolveWorldPickResult(outcome, &[])
        .expect_err("resolving hit without source metadata should fail");
    assert_eq!(
        error,
        WorldPickError::MissingTriangleSource { TriangleId: 999 },
        "missing world source metadata should return structured error"
    );
}

#[test]
fn WorldBlackboardGeometryWorksAsWorldPickSource() {
    let mut blackboard = WorldBlackboard::New();
    blackboard
        .Geometry
        .RegisterTriangle(Triangle(31, 601, -1.0, 0.0))
        .expect("registering blackboard triangle should succeed");

    let result = PickWorldGeometryRegistry(
        &blackboard.Geometry,
        &CameraAdapter(),
        0.5,
        0.5,
        RayQueryId(906),
    )
    .expect("blackboard-backed world pick should execute");

    match result {
        WorldPickResult::Hit(hit) => {
            assert_eq!(
                hit.EntityId,
                EntityId(31),
                "blackboard geometry hit should preserve registered entity id"
            );
            assert_eq!(
                hit.TriangleId, 601,
                "blackboard geometry hit should preserve registered triangle id"
            );
        }
        WorldPickResult::Miss(_) => panic!("expected hit from blackboard geometry center triangle"),
    }
}
