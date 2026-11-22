use wyrmcoil::GameRuntime::{GameState, TacticalCommand::*, TacticalOrder};
use wyrmcoil::Playtest::{Input, Runner};

pub fn Scenario(run: &mut Runner) -> Result<(), String> {
    let initial = run.Observe().Tactical()?.clone();
    run.Check("starts paused", initial.Paused)?;
    run.Equal("six actors", initial.Actors.len(), 6)?;
    run.Step(Input::Tactical(vec![
        SelectParty(1),
        MoveTo([-2.0, 4.0]),
        MoveTo([-2.0, 3.0]),
    ]))?;
    for _ in 0..20 {
        run.Step(Input::Idle)?;
    }
    let frozen = run.Observe().Tactical()?.clone();
    run.Equal("paused commands do not advance time", frozen.Frame, 0)?;
    run.Equal(
        "paused patrol and animation stay frozen",
        &frozen.Actors,
        &initial.Actors,
    )?;
    run.Equal(
        "paused orders remain in FIFO order",
        frozen.Orders,
        vec![
            TacticalOrder::MoveTo([-2.0, 4.0]),
            TacticalOrder::MoveTo([-2.0, 3.0]),
        ],
    )?;
    let checkpoint = run.Checkpoint();
    run.Step(Input::Tactical(vec![TogglePause]))?;
    run.Advance(13, Input::Idle)?;
    let moved = run.Observe().Tactical()?;
    let [x, z] = moved.Actors[1].Position;
    run.Near("first waypoint X", x, -2.0, 0.001)?;
    run.Near("first waypoint Z", z, 4.0, 0.001)?;
    run.Rewind(checkpoint)?;
    run.Check(
        "rewound to tactical pause",
        run.Observe().Tactical()?.Paused,
    )?;

    for target in 3..6 {
        for slot in 0..3 {
            run.Step(Input::Tactical(vec![
                SelectParty(slot),
                Clear,
                Inspect(target),
                Ability(slot),
                Attack(target),
            ]))?;
        }
        run.Step(Input::Tactical(vec![TogglePause]))?;
        for _ in 0..1_500 {
            let view = run.Observe().Tactical()?;
            if view.Actors[target as usize].Health == 0 || view.State != GameState::Playing {
                break;
            }
            run.Step(Input::Idle)?;
        }
        let health = run.Observe().Tactical()?.Actors[target as usize].Health;
        run.Equal("focused target defeated", health, 0)?;
        run.Step(Input::Tactical(vec![TogglePause]))?;
    }
    let survivor = run
        .Observe()
        .Tactical()?
        .Actors
        .iter()
        .find(|actor| actor.Health > 0 && actor.PartySlot.is_some())
        .and_then(|actor| actor.PartySlot)
        .ok_or("the party failed to survive")?;
    run.Step(Input::Tactical(vec![
        SelectParty(survivor),
        Clear,
        MoveTo([0.0, -6.0]),
        TogglePause,
    ]))?;
    for _ in 0..700 {
        if run.Observe().State() != GameState::Playing {
            break;
        }
        run.Step(Input::Idle)?;
    }
    run.Equal("relic victory", run.Observe().State(), GameState::Won)?;
    run.Rewind(run.Checkpoint())?;

    run.Restart()?;
    let restored = run.Observe().Tactical()?.clone();
    run.Equal(
        "restart restores all initial tactical facts",
        restored,
        initial,
    )?;
    run.Step(Input::Tactical(vec![
        MoveTo([0.0, -3.0]),
        SelectParty(1),
        MoveTo([-1.0, -3.0]),
        SelectParty(2),
        MoveTo([1.0, -3.0]),
        TogglePause,
    ]))?;
    for _ in 0..3_000 {
        if run.Observe().State() != GameState::Playing {
            break;
        }
        run.Step(Input::Idle)?;
    }
    run.Equal(
        "undefended approach loses",
        run.Observe().State(),
        GameState::Lost,
    )?;
    run.Rewind(run.Checkpoint())?;
    Ok(())
}
