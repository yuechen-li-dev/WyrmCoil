use wyrmcoil_game::Scene::{Patrol, PlayerCamera, Scene, SceneDocument, WalkBounds};
use wyrmcoil_game::{ActorArt, ClipArt, EquipmentArt, PropArt, TacticalArt};

pub const RELIC: [i32; 2] = [0, -6];
pub const PIERS: [[i32; 2]; 6] = [[-3, 0], [3, 0], [-3, -1], [3, -1], [-3, -2], [3, -2]];

pub fn BuildScene() -> SceneDocument {
    let mut nodes = vec![
        Scene::Floor("flagstones", [18.0, 16.0], [0.19, 0.22, 0.27]),
        Scene::Box("north", [18.0, 2.0, 0.4], [0.34, 0.37, 0.44]).At(0.0, 1.0, -7.8),
        Scene::Box("west", [0.4, 2.0, 16.0], [0.30, 0.33, 0.42]).At(-8.8, 1.0, 0.0),
        Scene::Box("east", [0.4, 2.0, 16.0], [0.30, 0.33, 0.42]).At(8.8, 1.0, 0.0),
        Scene::Box("south", [18.0, 0.35, 0.4], [0.30, 0.33, 0.42]).At(0.0, 0.175, 7.8),
        Scene::Box("relic-plinth", [0.8, 0.24, 0.8], [0.85, 0.61, 0.20]).At(
            RELIC[0] as f32,
            0.12,
            RELIC[1] as f32,
        ),
        Scene::Player(
            "camera",
            PlayerCamera {
                eye_height: 13.0,
                pitch: -0.94,
                vertical_fov: 0.88,
                near: 0.1,
                far: 90.0,
            },
        )
        .At(0.0, 0.0, 9.5),
        // The room artifact needs an anchor for legacy world construction.
        // Tactical snapshots publish their own complete actor list.
        Scene::Guard(
            "room-anchor",
            [0.2; 3],
            [0.2; 3],
            Patrol {
                half_span: 0.1,
                half_period_ticks: 120,
                phase_ticks: 0,
            },
        )
        .At(0.0, 0.1, -7.0),
    ];
    for (index, [x, z]) in PIERS.into_iter().enumerate() {
        nodes.push(
            Scene::Box(
                &format!("pier-{index}"),
                [0.95, 1.9, 0.95],
                [0.32, 0.35, 0.42],
            )
            .At(x as f32, 0.95, z as f32),
        );
    }
    // The pitched camera anchor is behind the battlefield. Actor navigation
    // uses its own 17-by-15 grid and never reaches the anchor.
    Scene::World(
        "the-last-watch",
        WalkBounds {
            x: [-8.0, 8.0],
            z: [-7.0, 10.0],
        },
        nodes,
    )
}

pub fn BuildArt() -> TacticalArt {
    let mut props = Vec::new();
    let mut place = |mesh: &str, position, scale, yaw| {
        props.push(PropArt {
            Mesh: format!("environment/{mesh}.gltf"),
            Position: position,
            Scale: scale,
            Yaw: yaw,
        })
    };
    for z in [-6.0, -2.0, 2.0, 6.0] {
        for x in [-7.2, -3.6, 0.0, 3.6, 7.2] {
            place("floor_tile_large", [x, -0.1, z], [0.90, 1.0, 1.0], 0.0);
        }
    }
    for x in [-7.2, -3.6, 0.0, 3.6, 7.2] {
        place("wall", [x, 0.0, -7.8], [0.90, 0.63, 0.47], 0.0);
        place("wall", [x, 0.0, 7.8], [0.90, 0.12, 0.47], 0.0);
    }
    for z in [-6.0, -2.0, 2.0, 6.0] {
        for x in [-8.8, 8.8] {
            place(
                "wall",
                [x, 0.0, z],
                [1.0, 0.51, 0.47],
                std::f32::consts::FRAC_PI_2,
            );
        }
    }
    for [x, z] in PIERS {
        place("pillar", [x as f32, 0.0, z as f32], [0.59, 0.49, 0.59], 0.0);
    }
    place(
        "chest_gold",
        [RELIC[0] as f32, 0.0, RELIC[1] as f32],
        [0.62; 3],
        0.0,
    );
    let actor = |id, model, basic, ability, equipment: &[(&str, &str)]| {
        let clip = |file: &str, name: &str| ClipArt {
            File: format!("animations/Rig_Medium_{file}.glb"),
            Name: name.into(),
        };
        let (combat, attack) = basic;
        let (ability_file, ability_name) = ability;
        ActorArt {
            Id: id,
            Mesh: format!("characters/{model}.glb"),
            Clips: [
                clip("General", "Idle_A"),
                clip("MovementBasic", "Walking_A"),
                clip(combat, attack),
                clip(ability_file, ability_name),
                clip("General", "Hit_A"),
                clip("General", "Death_A"),
            ],
            Equipment: equipment
                .iter()
                .map(|(name, socket)| EquipmentArt {
                    Mesh: format!("equipment/{name}.gltf"),
                    Socket: (*socket).into(),
                })
                .collect(),
        }
    };
    let melee = ("CombatMelee", "Melee_1H_Attack_Slice_Diagonal");
    let guard = |id| {
        actor(
            id,
            "Ranger",
            melee,
            melee,
            &[("sword_1handed", "handslot.r")],
        )
    };
    let actors = vec![
        actor(
            0,
            "Knight",
            melee,
            ("CombatMelee", "Melee_Block_Attack"),
            &[
                ("sword_1handed", "handslot.r"),
                ("shield_badge", "handslot.l"),
            ],
        ),
        actor(
            1,
            "Rogue",
            melee,
            ("CombatMelee", "Melee_Dualwield_Attack_Stab"),
            &[("dagger", "handslot.r"), ("dagger", "handslot.l")],
        ),
        actor(
            2,
            "Mage",
            ("CombatRanged", "Ranged_Magic_Shoot"),
            ("CombatRanged", "Ranged_Magic_Raise"),
            &[("staff", "handslot.r")],
        ),
        guard(3),
        guard(4),
        actor(
            5,
            "Barbarian",
            melee,
            melee,
            &[("sword_1handed", "handslot.r")],
        ),
    ];
    TacticalArt {
        Props: props,
        Actors: actors,
    }
}
