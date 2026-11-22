use std::path::Path;
use std::time::{Duration, Instant};

use wyrmcoil::GameRuntime::{GameSession, TacticalCommand, TacticalOrder};

#[test]
#[ignore = "requires the independently published Last Watch artifact from build.ps1"]
fn last_watch_receives_orders_over_the_real_game_pipe_while_paused() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/games/last-watch");
    let start = Instant::now();
    let mut game = GameSession::Start(&root).unwrap();
    assert!(game.Poll(Duration::from_secs(3)).unwrap());
    eprintln!("Last Watch process and initial scene ready in {:.1?}", start.elapsed());
    let first = game.Tactical().unwrap().clone();
    assert_eq!(game.Name(), "the-last-watch");
    assert!(game.TacticalArt().is_some());
    assert!(first.Paused);
    let commands = vec![
        TacticalCommand::SelectParty(1),
        TacticalCommand::MoveTo([-2.0, 4.0]),
        TacticalCommand::Inspect(3),
        TacticalCommand::Attack(3),
    ];
    game.BeginTacticalTick(commands).unwrap();
    assert!(game.Poll(Duration::from_millis(500)).unwrap());
    let paused = game.Tactical().unwrap();
    assert_eq!(paused.Frame, 0);
    assert_eq!(paused.Actors, first.Actors);
    assert_eq!(
        paused.Orders,
        [TacticalOrder::MoveTo([-2.0, 4.0]), TacticalOrder::Attack(3)]
    );
    game.BeginTacticalTick(vec![TacticalCommand::TogglePause])
        .unwrap();
    assert!(game.Poll(Duration::from_millis(500)).unwrap());
    for _ in 0..18 {
        game.BeginTacticalTick(Vec::new()).unwrap();
        assert!(game.Poll(Duration::from_millis(500)).unwrap());
    }
    assert_eq!(game.Tactical().unwrap().Frame, 19);
    assert_ne!(
        game.Tactical().unwrap().Actors[1].Position,
        first.Actors[1].Position
    );
    drop(game);
    let restart = Instant::now();
    let mut fresh = GameSession::Start(&root).unwrap();
    assert!(fresh.Poll(Duration::from_secs(3)).unwrap());
    eprintln!("Last Watch process restart ready in {:.1?}", restart.elapsed());
    assert_eq!(fresh.Tactical().unwrap(), &first);
}
