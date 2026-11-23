use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use wyrmcoil::Engine::asset::gltf::{AssetLibrary, PaletteMesh};

const MODELS: [&str; 13] = [
    "environment/floor_tile_large.gltf",
    "environment/wall.gltf",
    "environment/pillar.gltf",
    "environment/chest_gold.gltf",
    "characters/Knight.glb",
    "equipment/sword_1handed.gltf",
    "equipment/shield_badge.gltf",
    "characters/Rogue.glb",
    "equipment/dagger.gltf",
    "characters/Mage.glb",
    "equipment/staff.gltf",
    "characters/Ranger.glb",
    "characters/Barbarian.glb",
];

#[test]
fn last_watch_assets_share_only_identical_pixel_sources() {
    let root = Path::new("examples/games/last-watch/assets");
    let mut library = AssetLibrary::New(root).unwrap();
    let knight = library.Load(Path::new("characters/Knight.glb")).unwrap();
    let shield = library
        .Load(Path::new("equipment/shield_badge.gltf"))
        .unwrap();
    let rogue = library.Load(Path::new("characters/Rogue.glb")).unwrap();
    assert!(Arc::ptr_eq(&knight.Textures[0], &shield.Textures[0]));
    assert!(!Arc::ptr_eq(&knight.Textures[0], &rogue.Textures[0]));
    assert_eq!(library.Timings().Images.Hits, 1);
    assert_eq!(library.Timings().Images.Misses, 2);
    for path in MODELS {
        library.Load(Path::new(path)).unwrap();
    }
    let images = library.Timings().Images;
    assert_eq!((images.Hits, images.Misses), (7, 6));
    assert_eq!(images.DecodedBytes, 6 * 1_024 * 1_024 * 4);
}

#[test]
#[ignore = "profiling sample: cargo test --test m12_asset_profile -- --ignored --nocapture"]
fn measure_last_watch_material_and_mesh_preparation() {
    let root = Path::new("examples/games/last-watch/assets");
    for run in 0..6 {
        let start = Instant::now();
        let mut library = AssetLibrary::New(root).unwrap();
        let mut colors = Duration::ZERO;
        for path in MODELS {
            let model = library.Load(Path::new(path)).unwrap();
            colors += PaletteMesh::New(model).unwrap().ColorPreparation;
        }
        let total = start.elapsed();
        let hit_start = Instant::now();
        for path in MODELS {
            library.Load(Path::new(path)).unwrap();
        }
        println!(
            "run={run} models={} total={total:?} color={colors:?} warm_models={:?} {:?}",
            MODELS.len(),
            hit_start.elapsed(),
            library.Timings()
        );
    }
}
