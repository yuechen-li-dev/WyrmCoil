use wyrmcoil::Demo::room::RoomInput;
use wyrmcoil::GameRuntime::GameState;
use wyrmcoil::Playtest::{Facts, Input, Runner};

const FORWARD: RoomInput = RoomInput {
    Forward: 1,
    Strafe: 0,
    Yaw: 0,
    Pitch: 0,
};
const RIGHT: RoomInput = RoomInput {
    Forward: 0,
    Strafe: 1,
    Yaw: 0,
    Pitch: 0,
};

pub fn Scenario(run: &mut Runner) -> Result<(), String> {
    let initial = run.Observe().clone();
    run.Equal("fresh room tick", initial.Tick(), 0)?;
    run.Equal("fresh game is running", initial.State(), GameState::Playing)?;
    let Facts::Room {
        player,
        observation,
        ..
    } = initial
    else {
        return Err("expected Guardhouse room facts".into());
    };
    run.Equal("three keys at spawn", observation.Remaining, 3)?;
    run.Near("initial player X", player[0], -2.0, 0.001)?;
    run.Advance(90, Input::Navigate(FORWARD))?;
    let checkpoint = run.Checkpoint();
    let Facts::Room { observation, .. } = run.Observe() else {
        unreachable!()
    };
    run.Equal(
        "two keys gathered along the western lane",
        observation.Remaining,
        1,
    )?;
    run.Rewind(checkpoint)?;

    for (input, limit) in [(RIGHT, 113), (FORWARD, 135)] {
        for _ in 0..limit {
            if run.Observe().State() != GameState::Playing {
                break;
            }
            run.Step(Input::Navigate(input))?;
        }
    }
    run.Equal(
        "escape after collecting the keys",
        run.Observe().State(),
        GameState::Won,
    )?;
    run.Rewind(run.Checkpoint())?;
    run.Equal(
        "victory survives full replay",
        run.Observe().State(),
        GameState::Won,
    )?;

    run.Restart()?;
    run.Equal("restart resets simulation", run.Observe().Tick(), 0)?;
    for _ in 0..180 {
        if run.Observe().State() != GameState::Playing {
            break;
        }
        run.Step(Input::Navigate(FORWARD))?;
    }
    run.Equal(
        "running into the guard loses",
        run.Observe().State(),
        GameState::Lost,
    )?;
    run.Rewind(run.Checkpoint())?;
    Ok(())
}
