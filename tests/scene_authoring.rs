#![allow(non_snake_case)]

use wyrmcoil::Demo::room::{
    BuiltinRoomScene, NewRoomEngine, NewRoomEngineFromScene, RoomInput, Vec3,
};
use wyrmcoil::Scene::{
    BuildArtifact, CompileScene, DecodeArtifact, EncodeArtifact, Patrol, PlayerCamera,
    ReadArtifact, Scene, SceneDocument, SceneNode, WalkBounds,
};

fn Camera() -> PlayerCamera {
    PlayerCamera {
        eye_height: 1.5,
        pitch: -0.1,
        vertical_fov: 1.0,
        near: 0.1,
        far: 60.0,
    }
}

fn Document(extra: impl IntoIterator<Item = SceneNode>) -> SceneDocument {
    let mut nodes = vec![
        Scene::Floor("floor", [20.0; 2], [0.3; 3]),
        Scene::Player("player", Camera()).At(1.0, 0.0, 4.0),
        Scene::Guard(
            "guard",
            [1.0; 3],
            [1.0, 0.5, 0.0],
            Patrol {
                half_span: 2.0,
                half_period_ticks: 60,
                phase_ticks: 30,
            },
        )
        .At(0.0, 0.5, -4.0),
    ];
    nodes.extend(extra);
    Scene::World(
        "test-room",
        WalkBounds {
            x: [-9.0, 9.0],
            z: [-9.0, 9.0],
        },
        nodes,
    )
}

fn Crate() -> SceneNode {
    Scene::Box("crate", [1.0; 3], [0.1, 0.5, 0.8]).At(1.0, 0.5, 0.0)
}

#[test]
fn fragments_resolve_scoped_ids_and_parent_relative_trs_in_canonical_order() {
    let fragment = Scene::Fragment([Scene::Group("inner", [Crate()]).At(0.0, 0.0, 1.0)]);
    let left = Scene::Instance("left", fragment.clone())
        .At(-4.0, 0.0, -3.0)
        .Yaw(std::f32::consts::FRAC_PI_2)
        .Scale(2.0, 3.0, 4.0);
    let right = Scene::Instance("right", fragment).At(4.0, 0.0, -3.0);
    let first = CompileScene(&Document([right.clone(), left.clone()])).unwrap();
    let second = CompileScene(&Document([left, right])).unwrap();
    assert_eq!(first, second);
    assert_eq!(EncodeArtifact(&first), EncodeArtifact(&second));
    let [left, right] = first.Data().boxes.as_slice() else {
        panic!("two resolved boxes")
    };
    assert_eq!(left.id, "left.inner.crate");
    assert_eq!(right.id, "right.inner.crate");
    for (actual, want) in left
        .model
        .Transform([0.0, 0.0, 0.0, 1.0])
        .into_iter()
        .zip([-8.0, 1.5, -1.0, 1.0])
    {
        assert!((actual - want).abs() < 1e-5, "{actual} != {want}");
    }
    assert_eq!(right.model.0[3], [5.0, 0.5, -2.0, 1.0]);
    assert!((left.model.0[0][2] - 2.0).abs() < 1e-5);
}

#[test]
fn compiler_rejects_invalid_nodes_with_scoped_diagnostics() {
    let cases = [
        (
            Scene::Group("duplicate", [Crate(), Crate()]),
            "duplicate.crate: duplicate identity",
        ),
        (
            Scene::Group("room", [Scene::Box("bad", [0.0, 1.0, 1.0], [0.4; 3])]),
            "room.bad: invalid dimensions",
        ),
        (
            Crate().At(f32::NAN, 0.0, 0.0),
            "crate: invalid local transform",
        ),
        (
            Crate().Scale(-1.0, 1.0, 1.0),
            "crate: invalid local transform",
        ),
        (
            Scene::Player("second", Camera()),
            "second: more than one player",
        ),
        (Scene::Group("empty", []), "empty: empty group"),
    ];
    for (node, want) in cases {
        assert!(
            CompileScene(&Document([node])).unwrap_err().contains(want),
            "{want}"
        );
    }
}

#[test]
fn artifacts_round_trip_and_reject_corrupt_incompatible_or_changed_content() {
    let scene = CompileScene(&Document([Crate()])).unwrap();
    let text = EncodeArtifact(&scene).unwrap();
    let decoded = DecodeArtifact(text.as_bytes()).unwrap();
    assert_eq!(decoded, scene);
    assert_eq!(EncodeArtifact(&decoded).unwrap(), text);
    assert_ne!(
        scene.Identity(),
        CompileScene(&Document([Crate().Yaw(0.1)]))
            .unwrap()
            .Identity()
    );
    assert!(
        DecodeArtifact(text.replacen("version = 1", "version = 2", 1).as_bytes())
            .unwrap_err()
            .contains("version 2")
    );
    assert!(
        DecodeArtifact(text.replace("test-room", "changed-room").as_bytes())
            .unwrap_err()
            .contains("identity mismatch")
    );
    assert!(DecodeArtifact(&text.as_bytes()[..19]).is_err());
    assert!(DecodeArtifact(&[0xff]).is_err());
    assert!(DecodeArtifact(&vec![0; 1024 * 1024 + 1]).is_err());
}

#[test]
fn authoring_replaces_an_artifact_only_after_validation() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("scene-build-test-{}.toml", std::process::id()));
    let first = BuildArtifact(&Document([]), &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(BuildArtifact(&Document([Crate(), Crate()]), &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let second = BuildArtifact(&Document([Crate()]), &path).unwrap();
    assert_ne!(first.Identity(), second.Identity());
    assert_eq!(ReadArtifact(&path).unwrap(), second);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn built_in_room_and_compiled_room_use_the_real_world_snapshot() {
    let mut baseline = NewRoomEngine().RenderSnapshot();
    assert_eq!(
        baseline,
        NewRoomEngineFromScene(BuiltinRoomScene()).RenderSnapshot()
    );
    assert_eq!(baseline.Camera.Eye, Vec3::New(0.0, 1.65, 8.8));
    assert_eq!(baseline.Boxes.len(), 6);
    assert_eq!(baseline.Boxes[0].Position, Vec3::New(-2.2, 0.82, 2.4));
    assert_eq!(baseline.Boxes[5].Id.as_ref(), "walls.right");
    assert_eq!(baseline.Guard.Position, Vec3::New(0.0, 0.47, -2.8));

    let compiled = CompileScene(&Document([Crate()])).unwrap();
    let mut engine = NewRoomEngineFromScene(&compiled);
    baseline = engine.RenderSnapshot();
    assert_eq!(baseline.Camera.Eye, Vec3::New(1.0, 1.5, 4.0));
    assert_eq!(baseline.Boxes[0].Position, Vec3::New(1.0, 0.5, 0.0));
    engine.EnqueueInput(RoomInput {
        Forward: 1,
        ..RoomInput::default()
    });
    for _ in 0..30 {
        engine.TickControl();
        engine.TickSimulation();
    }
    let moved = engine.RenderSnapshot();
    assert!((moved.Camera.Eye.z - 2.4).abs() < 1e-5);
    assert_eq!(moved.Guard.Position, Vec3::New(2.0, 0.5, -4.0));
    assert!(std::sync::Arc::ptr_eq(&baseline.Boxes, &moved.Boxes));
}
