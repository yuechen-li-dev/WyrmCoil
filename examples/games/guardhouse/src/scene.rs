#![allow(non_snake_case)]

use wyrmcoil_game::Scene::{Patrol, PlayerCamera, Scene, SceneDocument, WalkBounds};

pub const KEYS: [(&str, [f32; 3]); 3] = [
    ("key-west", [-2.0, 0.65, 4.0]),
    ("key-mid", [-2.0, 0.65, 1.8]),
    ("key-east", [4.0, 0.65, 1.8]),
];
pub const EXIT: [f32; 3] = [4.0, 0.0, -5.4];

pub fn BuildScene() -> SceneDocument {
    let mut nodes = vec![
        Scene::Floor("floor", [14.0, 17.0], [0.29, 0.34, 0.42]),
        Scene::Box("north-wall", [14.0, 2.7, 0.25], [0.31, 0.35, 0.5]).At(0.0, 1.35, -8.0),
        Scene::Box("west-wall", [0.25, 2.7, 16.0], [0.35, 0.40, 0.53]).At(-6.8, 1.35, 0.0),
        Scene::Box("east-wall", [0.25, 2.7, 16.0], [0.35, 0.40, 0.53]).At(6.8, 1.35, 0.0),
        Scene::Box("south-wall", [14.0, 2.7, 0.25], [0.31, 0.35, 0.5]).At(0.0, 1.35, 8.0),
        Scene::Box("exit", [1.6, 2.2, 0.32], [0.15, 0.82, 0.50]).At(EXIT[0], 1.1, EXIT[2]),
        Scene::Player(
            "player",
            PlayerCamera {
                eye_height: 1.45,
                pitch: -0.17,
                vertical_fov: std::f32::consts::FRAC_PI_3,
                near: 0.1,
                far: 50.0,
            },
        )
        .At(-2.0, 0.0, 6.5),
        Scene::Guard(
            "guard",
            [0.82, 1.25, 0.82],
            [0.92, 0.25, 0.18],
            Patrol {
                half_span: 2.0,
                half_period_ticks: 120,
                phase_ticks: 80,
            },
        )
        .At(0.0, 0.625, -2.0),
    ];
    for (id, [x, y, z]) in KEYS {
        nodes.push(
            Scene::Box(id, [0.55, 0.62, 0.48], [1.0, 0.76, 0.14])
                .At(x, y, z)
                .Yaw(0.42),
        );
    }
    Scene::World(
        "guardhouse",
        WalkBounds {
            x: [-6.4, 6.4],
            z: [-7.6, 7.6],
        },
        nodes,
    )
}
