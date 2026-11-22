#![allow(non_snake_case)]

use super::scene_authoring::{Patrol, PlayerCamera, Scene, SceneDocument, WalkBounds};

pub fn BuildScene() -> SceneDocument {
    Scene::World(
        "training-room",
        WalkBounds {
            x: [-7.1, 7.1],
            z: [-9.2, 10.8],
        },
        [
            Scene::Floor("floor", [15.4, 21.4], [0.26, 0.33, 0.38]).At(0.0, 0.0, 0.5),
            Scene::Box("box-cyan", [1.8, 1.64, 1.6], [0.23, 0.74, 0.87]).At(-2.2, 0.82, 2.4),
            Scene::Box("box-rose", [1.7, 2.0, 1.7], [0.84, 0.40, 0.54]).At(2.1, 1.0, -0.7),
            Scene::Box("box-sage", [2.2, 2.2, 1.8], [0.46, 0.69, 0.43]).At(-0.9, 1.1, -4.7),
            Scene::Group(
                "walls",
                [
                    Scene::Box("back", [15.4, 3.3, 0.32], [0.29, 0.34, 0.45]).At(0.0, 1.65, -9.9),
                    Scene::Box("left", [0.32, 3.3, 21.0], [0.36, 0.40, 0.52]).At(-7.5, 1.65, 0.5),
                    Scene::Box("right", [0.32, 3.3, 21.0], [0.30, 0.35, 0.46]).At(7.5, 1.65, 0.5),
                ],
            ),
            Scene::Player(
                "player",
                PlayerCamera {
                    eye_height: 1.65,
                    pitch: -0.13,
                    vertical_fov: std::f32::consts::FRAC_PI_3,
                    near: 0.10,
                    far: 70.0,
                },
            )
            .At(0.0, 0.0, 8.8),
            Scene::Guard(
                "guard",
                [0.94; 3],
                [1.0, 0.65, 0.22],
                Patrol {
                    half_span: 3.6,
                    half_period_ticks: 240,
                    phase_ticks: 120,
                },
            )
            .At(0.0, 0.47, -2.8),
        ],
    )
}
