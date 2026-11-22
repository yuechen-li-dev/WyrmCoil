use std::path::Path;
use std::process::ExitCode;

use wyrmcoil_scene as scene_authoring;

mod scene;

fn main() -> ExitCode {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(scene_authoring::ARTIFACT_PATH);
    match scene_authoring::BuildArtifact(&scene::BuildScene(), &path) {
        Ok(compiled) => {
            println!(
                "Built {} ({}) → {}",
                compiled.Data().name,
                compiled.Identity(),
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Scene build failed: {error}");
            ExitCode::FAILURE
        }
    }
}
