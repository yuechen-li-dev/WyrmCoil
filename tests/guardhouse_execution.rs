#![allow(non_snake_case)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wyrmcoil::Demo::room::{RoomInput, RoomSnapshot};
use wyrmcoil::GameRuntime::{GAME_ARTIFACT, GameSession, GameState, Observation};

fn NewProject() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("examples/games/guardhouse");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let project = root
        .join("target")
        .join(format!("m7-contract-{nonce}"))
        .join("guardhouse");
    fs::create_dir_all(project.join("src")).unwrap();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "src/main.rs",
        "src/scene.rs",
        "src/game.rs",
        "build.ps1",
    ] {
        fs::copy(source.join(name), project.join(name)).unwrap();
    }
    eprintln!("Guardhouse contract project: {}", project.display());
    project
}

fn Build(project: &Path) -> Output {
    Command::new("pwsh")
        .args(["-NoProfile", "-File", "build.ps1"])
        .current_dir(project)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap()
}

fn Publish(project: &Path) {
    let started = Instant::now();
    let output = Build(project);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("Published game in {:.1?}", started.elapsed());
}

fn Ready(game: &mut GameSession) -> Result<(), String> {
    loop {
        if game.Poll(Duration::from_millis(50))? {
            return Ok(());
        }
        assert!(game.Busy(), "there must be a pending request");
    }
}

fn Start(project: &Path) -> GameSession {
    let started = Instant::now();
    let mut game = GameSession::Start(project).unwrap();
    Ready(&mut game).unwrap();
    assert_eq!(game.Name(), "guardhouse");
    assert_eq!(game.Clock().unwrap().SimulationTick, 0);
    assert_eq!(game.Observation().unwrap().Remaining, 3);
    eprintln!("Started PID {} in {:.1?}", game.Pid(), started.elapsed());
    game
}

fn Step(game: &mut GameSession, input: RoomInput) {
    let before = game.Clock().unwrap();
    game.BeginTick(input).unwrap();
    assert!(
        game.BeginTick(input).is_err(),
        "only one request can be pending"
    );
    assert_eq!(game.Clock().unwrap().SimulationTick, before.SimulationTick);
    Ready(game).unwrap();
    let after = game.Clock().unwrap();
    assert_eq!(after.ControlTick, before.ControlTick + 1);
    assert_eq!(after.SimulationTick, before.SimulationTick + 1);
}

const FORWARD: RoomInput = RoomInput {
    Forward: 1,
    Strafe: 0,
    Yaw: 0,
    Pitch: 0,
};
const RIGHT: RoomInput = RoomInput {
    Strafe: 1,
    Forward: 0,
    Yaw: 0,
    Pitch: 0,
};

fn ReplayWin(game: &mut GameSession) -> Vec<(Observation, RoomSnapshot)> {
    let mut replay = Vec::new();
    let started = Instant::now();
    for (input, steps) in [(FORWARD, 90), (RIGHT, 113), (FORWARD, 135)] {
        for _ in 0..steps {
            if !game.Playing() {
                break;
            }
            Step(game, input);
            replay.push((
                game.Observation().unwrap().clone(),
                game.Snapshot().unwrap(),
            ));
        }
    }
    eprintln!(
        "{} real control/IPC/simulation ticks in {:.1?}",
        replay.len(),
        started.elapsed()
    );
    replay
}

fn CountStaged(project: &Path) -> usize {
    fs::read_dir(project.join("target"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("wyrmcoil-run-")
        })
        .count()
}

fn WaitForCleanup(project: &Path) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while CountStaged(project) != 0 {
        assert!(
            Instant::now() < deadline,
            "game session files were retained"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "independently builds and launches several real Cargo game binaries"]
fn guardhouse_runs_rebuilds_and_recovers_through_the_host_protocol() {
    let project = NewProject();
    assert!(
        GameSession::Start(&project)
            .err()
            .unwrap()
            .contains("Build the game first")
    );
    Publish(&project);
    let initial_game = fs::read(project.join(GAME_ARTIFACT)).unwrap();
    let mut first = Start(&project);
    let initial = first.Snapshot().unwrap();
    let mut replay = ReplayWin(&mut first);
    assert_eq!(first.Observation().unwrap().State, GameState::Won);
    assert_eq!(first.Observation().unwrap().Remaining, 0);
    assert!(
        first.BeginTick(FORWARD).is_err(),
        "terminal game accepts no input"
    );
    assert!(
        !first
            .Snapshot()
            .unwrap()
            .Boxes
            .iter()
            .any(|object| object.Id.starts_with("key-"))
    );
    assert!(replay.iter().any(|(obs, _)| obs.Remaining == 2));
    assert!(replay.iter().any(|(obs, _)| obs.Remaining == 1));
    assert!(
        replay
            .iter()
            .any(|(_, snapshot)| snapshot.Guard.Position != initial.Guard.Position)
    );
    let terminal_tick = first.Clock().unwrap().SimulationTick;
    assert!(!first.Poll(Duration::ZERO).unwrap());
    assert_eq!(first.Clock().unwrap().SimulationTick, terminal_tick);

    let mut restart = Start(&project);
    assert_ne!(restart.Pid(), first.Pid());
    assert_eq!(restart.Snapshot().unwrap(), initial);
    assert_eq!(ReplayWin(&mut restart), std::mem::take(&mut replay));
    drop(first);
    drop(restart);
    WaitForCleanup(&project);

    let mut running = Start(&project);
    let game_path = project.join("src/game.rs");
    let rules = fs::read_to_string(&game_path).unwrap();
    fs::write(&game_path, rules.replace("0.82;", "0.01;")).unwrap();
    Publish(&project);
    let rebuilt_game = fs::read(project.join(GAME_ARTIFACT)).unwrap();
    assert_ne!(rebuilt_game, initial_game);
    let mut new_code = Start(&project);
    for _ in 0..180 {
        if running.Playing() {
            Step(&mut running, FORWARD);
        }
        Step(&mut new_code, FORWARD);
    }
    assert_eq!(running.Observation().unwrap().State, GameState::Lost);
    assert_eq!(new_code.Observation().unwrap().State, GameState::Playing);
    let paused = new_code.Clock().unwrap();
    assert!(!new_code.Poll(Duration::ZERO).unwrap());
    assert_eq!(new_code.Clock().unwrap(), paused);

    let main_path = project.join("src/main.rs");
    let main = fs::read_to_string(&main_path).unwrap();
    fs::write(
        &main_path,
        "compile_error!(\"intentional bad game build\");",
    )
    .unwrap();
    assert!(!Build(&project).status.success());
    assert_eq!(fs::read(project.join(GAME_ARTIFACT)).unwrap(), rebuilt_game);
    let good = Start(&project);
    drop(good);
    drop(new_code);
    drop(running);
    WaitForCleanup(&project);

    fs::write(&main_path, &main).unwrap();
    let scene_path = project.join("src/scene.rs");
    let scene = fs::read_to_string(&scene_path).unwrap();
    fs::write(&scene_path, scene.replace("[14.0, 17.0]", "[0.0, 17.0]")).unwrap();
    Publish(&project);
    let mut invalid = GameSession::Start(&project).unwrap();
    assert!(Ready(&mut invalid).unwrap_err().contains("floor"));
    assert!(invalid.Snapshot().is_none());
    drop(invalid);
    WaitForCleanup(&project);

    fs::write(
        &main_path,
        r##"
        use std::io::{self, Write};
        fn main() {
            let bytes = br#"{"Version":2,"Data":{"Failed":"version probe"}}"#;
            let mut out = io::stdout().lock();
            out.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
            out.write_all(bytes).unwrap();
        }
    "##,
    )
    .unwrap();
    Publish(&project);
    let mut incompatible = GameSession::Start(&project).unwrap();
    assert!(
        Ready(&mut incompatible)
            .unwrap_err()
            .contains("game protocol 2 is incompatible")
    );
    drop(incompatible);
    WaitForCleanup(&project);

    fs::write(
        &main_path,
        "fn main() { eprintln!(\"intentional game crash\"); std::process::abort(); }",
    )
    .unwrap();
    Publish(&project);
    let mut crashed = GameSession::Start(&project).unwrap();
    let diagnostic = Ready(&mut crashed).unwrap_err();
    assert!(diagnostic.contains("game "), "{diagnostic}");
    drop(crashed);
    WaitForCleanup(&project);

    fs::write(
        &main_path,
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(60)); }",
    )
    .unwrap();
    Publish(&project);
    let mut stuck = GameSession::Start(&project).unwrap();
    assert!(Ready(&mut stuck).unwrap_err().contains("timed out"));
    drop(stuck);
    WaitForCleanup(&project);
}
