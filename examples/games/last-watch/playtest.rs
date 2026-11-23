use wyrmcoil::GameRuntime::{
    ControlMode, GameState, PlayerAction, PlayerError, PlayerRequest, PlayerResult,
    TacticalCommand::*, TacticalOrder,
};
use wyrmcoil::Playtest::{Input, Runner};

pub fn Scenario(run: &mut Runner) -> Result<(), String> {
    Players(run)?;
    run.Restart()?;
    ThreatScenario(run)?;
    run.Restart()?;
    AgentScenario(run)?;
    run.Restart()?;
    let initial = run.Observe().Tactical()?.clone();
    run.Check("starts paused", initial.Paused)?;
    run.Equal("six actors", initial.Actors.len(), 6)?;
    run.Step(Input::Tactical(vec![
        SetPartyAutonomy(false),
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
        SetPartyAutonomy(false),
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

fn Players(run: &mut Runner) -> Result<(), String> {
    use PlayerAction::{Ability as UseAbility, Claim, Leave, Move, Possess, Release};
    PlayerStep(run, 1, 1, Claim, PlayerResult::Accepted)?;
    PlayerStep(run, 2, 2, Claim, PlayerResult::Accepted)?;
    PlayerStep(run, 1, 3, Possess { character: 1 }, PlayerResult::Accepted)?;
    PlayerStep(
        run,
        2,
        4,
        Possess { character: 1 },
        PlayerResult::Rejected(PlayerError::Occupied),
    )?;
    PlayerStep(run, 2, 5, Possess { character: 2 }, PlayerResult::Accepted)?;
    let view = run.Observe().Tactical()?.clone();
    run.Equal(
        "human and two guests possess independently",
        view.Players
            .unwrap()
            .Seats
            .into_iter()
            .map(|s| s.Character)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2)],
    )?;
    run.Step(Input::Tactical(vec![SelectParty(1)]))?;
    run.Equal(
        "human input cannot steal a guest's character",
        run.Observe().Tactical()?.Selected,
        0,
    )?;
    PlayerStep(run, 2, 6, Leave, PlayerResult::Accepted)?;
    PlayerStep(
        run,
        1,
        7,
        Move {
            position: [-2., 4.],
        },
        PlayerResult::Queued,
    )?;
    run.Step(Input::Tactical(vec![MoveTo([0., 4.]), TogglePause]))?;
    run.Advance(20, Input::Idle)?;
    let view = run.Observe().Tactical()?.clone();
    run.Equal(
        "human movement reaches its own destination",
        view.Actors[0].Position,
        [0., 4.],
    )?;
    run.Equal(
        "guest movement reaches its own destination",
        view.Actors[1].Position,
        [-2., 4.],
    )?;
    run.Equal(
        "guest character brain stays suspended",
        view.Agents[1].Clock,
        0,
    )?;
    run.Check("unowned Vey runs independently", view.Agents[2].Clock > 0)?;
    run.Step(Input::Tactical(vec![TogglePause, Attack(3)]))?;
    PlayerStep(run, 1, 8, UseAbility { target: 3 }, PlayerResult::Queued)?;
    run.Step(Input::Tactical(vec![TogglePause]))?;
    for _ in 0..250 {
        if run
            .Observe()
            .Tactical()?
            .Players
            .as_ref()
            .unwrap()
            .Events
            .iter()
            .any(|event| event.Receipt == 8 && event.Result == PlayerResult::Completed)
        {
            break;
        }
        run.Step(Input::Idle)?;
    }
    let view = run.Observe().Tactical()?.clone();
    run.Check(
        "guest Backstab completes in the real encounter",
        view.Players
            .as_ref()
            .unwrap()
            .Events
            .iter()
            .any(|event| event.Receipt == 8 && event.Result == PlayerResult::Completed),
    )?;
    run.Check(
        "Backstab produces authoritative health and cooldown changes",
        view.Actors[3].Health < 94 && view.Actors[1].Cooldown > 0,
    )?;
    PlayerStep(run, 1, 9, Release, PlayerResult::Accepted)?;
    let released = run.Observe().Tactical()?.Agents[1].clone();
    run.Equal(
        "released Merrit immediately resumes arbitration",
        released.Decision.unwrap().Tick,
        released.Clock,
    )?;
    run.CaptureAgents("M15 guest returns Merrit to Dunewyrm")?;
    run.Rewind(run.Checkpoint())
}

fn PlayerStep(
    run: &mut Runner,
    seat: u8,
    receipt: u64,
    action: PlayerAction,
    result: PlayerResult,
) -> Result<(), String> {
    let revision = run.Observe().Tactical()?.Players.as_ref().unwrap().Revision;
    run.Step(Input::Tactical(vec![Player(PlayerRequest {
        Seat: seat,
        Receipt: receipt,
        Observed: revision,
        Action: action,
    })]))?;
    let actual = run
        .Observe()
        .Tactical()?
        .Players
        .as_ref()
        .unwrap()
        .Events
        .iter()
        .find(|event| event.Receipt == receipt)
        .map(|event| event.Result);
    run.Equal(&format!("seat {seat} {action:?}"), actual, Some(result))
}

fn AgentScenario(run: &mut Runner) -> Result<(), String> {
    let initial = run.Observe().Tactical()?.clone();
    run.Equal("three independent party brains", initial.Agents.len(), 3)?;
    run.Equal(
        "starts in control of Alda",
        initial.Agents[0].Mode,
        ControlMode::PlayerControlled,
    )?;
    run.Step(Input::Tactical(vec![Attack(3), TogglePause]))?;
    run.Advance(24, Input::Idle)?;
    let underway = run.Observe().Tactical()?.clone();
    run.Equal(
        "possessed Alda brain stays at tick zero",
        underway.Agents[0].Clock,
        0,
    )?;
    for agent in &underway.Agents[1..] {
        run.Equal(
            "autonomous brain follows eligible world ticks",
            agent.Clock,
            underway.Frame,
        )?;
        run.Check(
            "autonomous agent records a nested stack",
            agent.Transitions.iter().any(|t| t.After.len() > 2),
        )?;
        run.Check(
            "autonomous agent records utility candidates",
            agent
                .Decision
                .as_ref()
                .is_some_and(|d| d.Candidates.len() >= 4),
        )?;
    }
    run.Check(
        "automated orders reach world authority",
        underway.Agents[1..].iter().all(|a| !a.Commands.is_empty()),
    )?;
    run.CaptureAgents("Alda under player control")?;
    run.Step(Input::Tactical(vec![TogglePause, SelectParty(1)]))?;
    let paused = run.Observe().Tactical()?.clone();
    for _ in 0..20 {
        run.Step(Input::Idle)?;
    }
    let after_pause = run.Observe().Tactical()?.clone();
    run.Equal(
        "tactical pause freezes the whole decision trace",
        after_pause,
        paused.clone(),
    )?;
    run.Step(Input::Tactical(vec![TogglePause]))?;
    let reassigned = run.Observe().Tactical()?.clone();
    run.Equal(
        "Merrit preserves her brain while possessed",
        &reassigned.Agents[1],
        &paused.Agents[1],
    )?;
    run.Equal(
        "released Alda evaluates on her first eligible tick",
        reassigned.Agents[0].Decision.as_ref().map(|d| d.Tick),
        Some(reassigned.Agents[0].Clock),
    )?;
    run.Step(Input::Tactical(vec![ReleaseParty]))?;
    let released = run.Observe().Tactical()?.clone();
    run.Check(
        "all party members become autonomous",
        released
            .Agents
            .iter()
            .all(|a| a.Mode == ControlMode::Autonomous),
    )?;
    run.Equal(
        "Merrit resumes with immediate arbitration",
        released.Agents[1].Decision.as_ref().map(|d| d.Tick),
        Some(paused.Agents[1].Clock + 1),
    )?;
    run.CaptureAgents("all party members released")?;
    let mut evidence = [false; 3];
    for _ in 0..850 {
        let view = run.Observe().Tactical()?;
        if view.State != GameState::Playing || view.Actors[3..].iter().all(|a| a.Health == 0) {
            break;
        }
        run.Step(Input::Idle)?;
        let view = run.Observe().Tactical()?;
        let witnessed = [
            view.Agents[1]
                .Decision
                .as_ref()
                .and_then(|d| d.Selected.as_deref())
                == Some("Flank"),
            view.Agents[0].Transitions.iter().any(|t| {
                t.Before.ends_with(&["ShieldBash".into()])
                    && t.After.ends_with(&["AttackCycle".into()])
                    && t.Reason == "Succeed"
            }),
            view.Agents[0].Transitions.iter().any(|t| {
                t.Interrupt
                    && t.Before.contains(&"AttackCycle".into())
                    && t.After
                        .ends_with(&["AttackCycle".into(), "BlockThreat".into()])
            }),
        ];
        for (index, label) in [
            "Merrit flanks a distracted target",
            "Shield Bash returns to AttackCycle",
            "Alda interrupts AttackCycle to block",
        ]
        .into_iter()
        .enumerate()
        {
            if witnessed[index] && !evidence[index] {
                run.CaptureAgents(label)?;
                evidence[index] = true;
            }
        }
    }
    run.Check(
        "Merrit flanks a distracted target during combat",
        evidence[0],
    )?;
    run.Check("Alda returns from nested Shield Bash", evidence[1])?;
    run.Check("Alda interrupts AttackCycle to block", evidence[2])?;
    let view = run.Observe().Tactical()?;
    run.Check(
        "the hybrid party defeats all three guards",
        view.Actors[3..].iter().all(|a| a.Health == 0),
    )?;
    run.Step(Input::Tactical(vec![
        SelectParty(0),
        Clear,
        MoveTo([0.0, -6.0]),
    ]))?;
    for _ in 0..350 {
        if run.Observe().State() != GameState::Playing {
            break;
        }
        run.Step(Input::Idle)?;
    }
    run.Equal(
        "possessing Alda recovers the relic",
        run.Observe().State(),
        GameState::Won,
    )?;
    let checkpoint = run.Checkpoint();
    run.Rewind(checkpoint)?;
    Ok(())
}

fn ThreatScenario(run: &mut Runner) -> Result<(), String> {
    run.Step(Input::Tactical(vec![
        SelectParty(2),
        MoveTo([4.0, 2.0]),
        TogglePause,
    ]))?;
    for _ in 0..160 {
        let view = run.Observe().Tactical()?;
        if view.Agents[0]
            .Decision
            .as_ref()
            .and_then(|d| d.Selected.as_deref())
            == Some("ProtectParty")
            && view.Actors[3..].iter().any(|a| {
                a.Health > 0
                    && (a.Position[0] - view.Actors[2].Position[0]).abs()
                        + (a.Position[1] - view.Actors[2].Position[1]).abs()
                        <= 1.1
            })
        {
            break;
        }
        run.Step(Input::Idle)?;
    }
    let view = run.Observe().Tactical()?;
    run.Equal(
        "Alda protects the threatened mage",
        view.Agents[0]
            .Decision
            .as_ref()
            .and_then(|d| d.Selected.clone())
            .as_deref(),
        Some("ProtectParty"),
    )?;
    run.CaptureAgents("Alda protects the threatened mage")?;
    run.Step(Input::Tactical(vec![ReleaseParty]))?;
    let view = run.Observe().Tactical()?;
    run.Equal(
        "Vey keeps range on possession release",
        view.Agents[2]
            .Decision
            .as_ref()
            .and_then(|d| d.Selected.clone())
            .as_deref(),
        Some("MaintainDistance"),
    )?;
    run.CaptureAgents("Vey keeps her distance")?;
    run.Rewind(run.Checkpoint())?;
    Ok(())
}
