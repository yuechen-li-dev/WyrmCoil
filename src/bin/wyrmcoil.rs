#![allow(non_snake_case)]
use std::io::{self, BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wyrmcoil::Demo::room::RoomInput;
use wyrmcoil::Engine::asset::gltf::AssetLibrary;
use wyrmcoil::Engine::shader::sdslv::spirv_cache::{InspectShaderCache, ShaderCacheDirectory};
use wyrmcoil::GameRuntime::{GAME_ARTIFACT, GameState, TacticalCommand};
use wyrmcoil::Playtest::{ContentIdentity, Input, Run, Runner};

#[path = "../../examples/games/guardhouse/playtest.rs"]
mod guardhouse;
#[path = "../../examples/games/last-watch/playtest.rs"]
mod last_watch;

const USAGE: &str = "wyrmcoil shader-cache stats|clear
wyrmcoil asset inspect <asset> [--root <project>] [--poses <count>]
wyrmcoil playtest --project <directory> [--game <executable>] --output <directory> (--scenario guardhouse|last-watch | --stdio)";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match Execute(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn Execute(args: &[String]) -> Result<(), String> {
    match args {
        [help] if help == "--help" || help == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [asset, inspect, path, flags @ ..] if asset == "asset" && inspect == "inspect" => {
            Inspect(Path::new(path), flags)
        }
        [playtest, flags @ ..] if playtest == "playtest" => Playtest(flags),
        [cache, action]
            if cache == "shader-cache" && matches!(action.as_str(), "stats" | "clear") =>
        {
            let path = ShaderCacheDirectory().ok_or("Shader cache is disabled")?;
            let stats = InspectShaderCache(&path, action == "clear")
                .map_err(|error| format!("{}: {error}", path.display()))?;
            println!("{} {}", action, path.display());
            println!(
                "{}",
                serde_json::to_string_pretty(&stats).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn Inspect(path: &Path, flags: &[String]) -> Result<(), String> {
    let mut root = None;
    let mut poses = None;
    let mut args = flags.iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--root" if root.is_none() => root = Some(PathBuf::from(value)),
            "--poses" if poses.is_none() => poses = Some(Number::<usize>(value)?),
            _ => return Err(format!("unknown or duplicate option {flag}")),
        }
    }
    if let Some(root) = root {
        println!(
            "{}",
            AssetLibrary::New(&root)?.Inspect(path, poses.unwrap_or(0))?
        );
    } else {
        let resolved = path
            .canonicalize()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let root = resolved.parent().ok_or("asset has no directory")?;
        let relative = Path::new(resolved.file_name().ok_or("asset has no file name")?);
        println!(
            "{}",
            AssetLibrary::New(root)?.Inspect(relative, poses.unwrap_or(0))?
        );
    }
    Ok(())
}

fn Playtest(flags: &[String]) -> Result<(), String> {
    let mut game = None;
    let mut project = None;
    let mut output = None;
    let mut scenario = None;
    let mut args = flags.iter();
    while let Some(flag) = args.next() {
        if flag == "--stdio" && scenario.is_none() {
            scenario = Some("stdio");
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--game" if game.is_none() => game = Some(Path::new(value)),
            "--project" if project.is_none() => project = Some(Path::new(value)),
            "--output" if output.is_none() => output = Some(Path::new(value)),
            "--scenario" if scenario.is_none() => scenario = Some(value.as_str()),
            _ => return Err(format!("unknown or duplicate option {flag}")),
        }
    }
    let project = project.ok_or("specify --project")?;
    let default_game = project.join(GAME_ARTIFACT);
    let game = game.unwrap_or(&default_game);
    let output = output.ok_or("specify --output")?;
    let (identity, source, run): (&str, &[u8], fn(&mut Runner) -> Result<(), String>) =
        match scenario {
            Some("guardhouse") => (
                "guardhouse/m9.1",
                include_bytes!("../../examples/games/guardhouse/playtest.rs"),
                guardhouse::Scenario,
            ),
            Some("last-watch") => (
                "last-watch/m9.1",
                include_bytes!("../../examples/games/last-watch/playtest.rs"),
                last_watch::Scenario,
            ),
            Some("stdio") => ("interactive/v1", b"", Stdio),
            _ => return Err("specify --scenario guardhouse or last-watch, or --stdio".into()),
        };
    let identity = if source.is_empty() {
        identity.to_owned()
    } else {
        format!("{identity}#{}", ContentIdentity(source))
    };
    Run(project, game, output, &identity, run)
}

fn Stdio(run: &mut Runner) -> Result<(), String> {
    let mut stdin = io::stdin().lock();
    let mut line = String::new();
    loop {
        line.clear();
        // Read at most one bounded line, including the delimiter.
        let bytes = (&mut stdin)
            .take(4097)
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if bytes == 0 {
            return Ok(());
        }
        if bytes > 4096 {
            return Err("interactive line is too long".into());
        }
        let words: Vec<_> = line.split_whitespace().collect();
        match words.as_slice() {
            [] => continue,
            ["quit"] => return Ok(()),
            ["observe"] => {}
            ["start"] | ["restart"] => run.Restart()?,
            ["replay"] => run.Rewind(run.Checkpoint())?,
            ["rewind", count] => run.Rewind(Number(count)?)?,
            ["advance", ticks] => run.Advance(Number(ticks)?, Input::Idle)?,
            ["navigate", ticks, strafe, forward, yaw, pitch] => run.Advance(
                Number(ticks)?,
                Input::Navigate(RoomInput {
                    Strafe: Number(strafe)?,
                    Forward: Number(forward)?,
                    Yaw: Number(yaw)?,
                    Pitch: Number(pitch)?,
                }),
            )?,
            ["pause"] | ["resume"] => {
                let desired = words[0] == "pause";
                if run.Observe().Tactical()?.Paused != desired {
                    run.Step(Input::Tactical(vec![TacticalCommand::TogglePause]))?;
                }
            }
            ["select", slot] => run.Step(Input::Tactical(vec![TacticalCommand::SelectParty(
                Number(slot)?,
            )]))?,
            ["move", x, z] => {
                let x: f32 = Number(x)?;
                let z: f32 = Number(z)?;
                if !x.is_finite() || !z.is_finite() {
                    return Err("movement coordinates must be finite".into());
                }
                run.Step(Input::Tactical(vec![TacticalCommand::MoveTo([x, z])]))?;
            }
            ["attack", id] => {
                run.Step(Input::Tactical(vec![TacticalCommand::Attack(Number(id)?)]))?
            }
            ["inspect", id] => {
                run.Step(Input::Tactical(vec![TacticalCommand::Inspect(Number(id)?)]))?
            }
            ["ability", slot] => run.Step(Input::Tactical(vec![TacticalCommand::Ability(
                Number(slot)?,
            )]))?,
            ["clear"] => run.Step(Input::Tactical(vec![TacticalCommand::Clear]))?,
            ["hold"] => run.Step(Input::Tactical(vec![TacticalCommand::Hold]))?,
            ["assert", "tick", value] => run.Equal("tick", run.Observe().Tick(), Number(value)?)?,
            ["assert", "state", value] => {
                let expected = match *value {
                    "playing" => GameState::Playing,
                    "won" => GameState::Won,
                    "lost" => GameState::Lost,
                    _ => return Err(format!("unknown game state '{value}'")),
                };
                run.Equal("state", run.Observe().State(), expected)?;
            }
            _ => {
                return Err(format!(
                    "unknown or malformed interactive command '{}'",
                    words[0]
                ));
            }
        }
        println!(
            "{}",
            serde_json::to_string(run.Observe()).map_err(|error| error.to_string())?
        );
    }
}

fn Number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse().map_err(|_| format!("invalid number '{text}'"))
}
