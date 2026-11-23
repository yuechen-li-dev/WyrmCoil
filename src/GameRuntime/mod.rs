//! Host-side execution of a trusted, independently built game.

use std::path::Path;
use std::time::{Duration, Instant};

pub use wyrmcoil_game::TacticalArt;
use wyrmcoil_game::Wire::{Reply, Request};
pub use wyrmcoil_game::{
    ActorMotion, AgentInspection, AgentOrder, ControlMode, MAX_TACTICAL_COMMANDS, TacticalActor,
    TacticalCommand, TacticalOrder, TacticalView,
};
pub use wyrmcoil_game::{GAME_ARTIFACT, GameState, Observation, PROTOCOL};
use wyrmcoil_game::{GameInput, GameTick};
pub use wyrmcoil_game::{
    PlayerAction, PlayerError, PlayerEvent, PlayerKind, PlayerRequest, PlayerResult, PlayerView,
};

use crate::Demo::room::{RoomInput, RoomSnapshot, RoomWorld, TryNewRoomEngineFromScene};
use crate::Engine::startup::StartupStage;
use crate::Engine::{Engine, EngineClock, World};
use crate::Scene::DecodeArtifact;

pub mod collaboration;
mod process;
use process::GameProcess;

const START_LIMIT: Duration = Duration::from_secs(3);
const TICK_LIMIT: Duration = Duration::from_millis(500);

enum Pending {
    Start(Instant),
    Tick(u64, Instant),
}

pub struct GameSession {
    process: GameProcess,
    engine: Option<Engine<RoomWorld, RoomInput>>,
    observation: Option<Observation>,
    name: String,
    pending: Option<Pending>,
    applied: RoomInput,
    tactical: Option<TacticalView>,
    tactical_tick: u64,
    art: Option<TacticalArt>,
    collaboration: Option<collaboration::Bridge>,
}

impl GameSession {
    /// The caller must obtain explicit authorization to execute this local project's code.
    pub fn Start(project: &Path) -> Result<Self, String> {
        Self::StartExecutable(project, &project.join(GAME_ARTIFACT))
    }

    /// The caller must obtain explicit authorization to execute this local binary.
    pub fn StartExecutable(project: &Path, executable: &Path) -> Result<Self, String> {
        let mut process = GameProcess::Start(project, executable)?;
        if let Err(error) = process.Send(Request::Start) {
            process.Close();
            return Err(error);
        }
        Ok(Self {
            process,
            engine: None,
            observation: None,
            name: String::new(),
            pending: Some(Pending::Start(Instant::now() + START_LIMIT)),
            applied: RoomInput::default(),
            tactical: None,
            tactical_tick: 0,
            art: None,
            collaboration: None,
        })
    }

    pub fn Project(&self) -> &Path {
        self.process.Project()
    }

    pub fn Pid(&self) -> u32 {
        self.process.Pid()
    }

    pub fn Name(&self) -> &str {
        &self.name
    }

    pub fn Observation(&self) -> Option<&Observation> {
        self.observation.as_ref()
    }

    pub fn Clock(&self) -> Option<EngineClock> {
        self.engine.as_ref().map(Engine::Clock)
    }

    pub fn Snapshot(&mut self) -> Option<RoomSnapshot> {
        self.engine.as_mut().map(Engine::RenderSnapshot)
    }

    pub fn ObserveRoom(&self) -> Option<RoomSnapshot> {
        let engine = self.engine.as_ref()?;
        Some(
            engine
                .World
                .ExtractRenderSnapshot(engine.Clock().RenderFrame),
        )
    }

    pub fn StagedExecutable(&self) -> &Path {
        self.process.StagedExecutable()
    }

    /// Wait for the direct child to be reaped before a command-line host exits.
    pub fn Close(mut self) {
        self.process.Close();
    }

    pub fn Busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn Playing(&self) -> bool {
        if let Some(view) = &self.tactical {
            return view.State == GameState::Playing;
        }
        self.observation
            .as_ref()
            .is_some_and(|view| view.State == GameState::Playing)
    }

    pub fn Tactical(&self) -> Option<&TacticalView> {
        self.tactical.as_ref()
    }

    pub fn TacticalArt(&self) -> Option<&TacticalArt> {
        self.art.as_ref()
    }

    pub fn EnableLocalPlayers(&mut self) -> Result<(), String> {
        if self
            .tactical
            .as_ref()
            .is_some_and(|view| view.Players.is_some())
        {
            self.collaboration = collaboration::Bridge::FromEnvironment(&self.name)?;
        }
        Ok(())
    }

    pub fn HasLocalPlayers(&self) -> bool {
        self.collaboration.is_some()
    }

    /// Explicit opt-in for a caller-owned local tactical host.
    pub fn OpenLocalPlayers(&mut self, label: &str, timeout: Duration) -> Result<(), String> {
        if self.collaboration.is_some()
            || !self
                .tactical
                .as_ref()
                .is_some_and(|view| view.Players.is_some())
            || !(Duration::from_secs(5)..=Duration::from_secs(1800)).contains(&timeout)
        {
            return Err("local players require a ready tactical game and a bounded lease".into());
        }
        self.collaboration = Some(collaboration::Bridge::ForLabel(&self.name, label, timeout)?);
        Ok(())
    }

    pub fn PlayerLiveness(&self, now: Instant) -> Vec<collaboration::LeaseStatus> {
        self.collaboration
            .as_ref()
            .map_or_else(Vec::new, |bridge| bridge.Status(now))
    }

    pub fn PollLocalPlayers(&mut self, now: Instant) -> Vec<TacticalCommand> {
        match (&mut self.collaboration, &self.tactical) {
            (Some(bridge), Some(view)) => bridge.Poll(view, now),
            _ => Vec::new(),
        }
    }

    pub fn BeginTacticalTick(&mut self, commands: Vec<TacticalCommand>) -> Result<(), String> {
        if self.tactical.is_none()
            || self.Busy()
            || (!self.Playing()
                && (commands.is_empty()
                    || !commands
                        .iter()
                        .all(|command| matches!(command, TacticalCommand::Player(_)))))
        {
            return Err("game cannot accept a tactical tick".into());
        }
        let index = self
            .tactical_tick
            .checked_add(1)
            .ok_or("game tick overflow")?;
        self.process.Send(Request::TacticalTick {
            Index: index,
            Commands: commands,
        })?;
        self.pending = Some(Pending::Tick(index, Instant::now() + TICK_LIMIT));
        Ok(())
    }

    pub fn BeginTick(&mut self, input: RoomInput) -> Result<(), String> {
        if self.Busy() || !self.Playing() {
            return Err("game cannot accept another tick".into());
        }
        let engine = self.engine.as_mut().ok_or("game room is missing")?;
        let input = RoomInput {
            Strafe: input.Strafe.signum(),
            Forward: input.Forward.signum(),
            Yaw: input.Yaw.signum(),
            Pitch: input.Pitch.signum(),
        };
        if self.applied != input {
            engine.EnqueueInput(input);
            self.applied = input;
        }
        engine.TickControl();
        let world = engine.World.ExtractRenderSnapshot(0);
        let index = engine.Clock().SimulationTick + 1;
        self.process.Send(Request::Tick(GameTick {
            Index: index,
            Input: GameInput {
                Strafe: input.Strafe,
                Forward: input.Forward,
                Yaw: input.Yaw,
                Pitch: input.Pitch,
            },
            Player: [world.Camera.Eye.x, world.Camera.Eye.y, world.Camera.Eye.z],
            Guard: [
                world.Guard.Position.x,
                world.Guard.Position.y,
                world.Guard.Position.z,
            ],
        }))?;
        self.pending = Some(Pending::Tick(index, Instant::now() + TICK_LIMIT));
        Ok(())
    }

    /// Complete the single pending request. Callers own the timing budget.
    pub fn Poll(&mut self, wait: Duration) -> Result<bool, String> {
        let Some(pending) = &self.pending else {
            return Ok(false);
        };
        let deadline = match pending {
            Pending::Start(at) | Pending::Tick(_, at) => *at,
        };
        let now = Instant::now();
        let wait = wait.min(deadline.saturating_duration_since(now));
        let reply = match self.process.Receive(wait)? {
            Some(reply) => reply,
            None if Instant::now() >= deadline => return Err("game response timed out".into()),
            None => return Ok(false),
        };
        if let Pending::Start(deadline) = pending {
            StartupStage::Mark(
                "game_handshake_observed",
                (Instant::now() + START_LIMIT).saturating_duration_since(*deadline),
            );
        }
        match (pending, reply) {
            (_, Reply::Failed(message)) => return Err(message),
            (Pending::Start(_), Reply::Started { Scene, Observation }) => {
                ValidateObservation(&Observation)?;
                let scene =
                    StartupStage::Call("game_scene_artifact", || DecodeArtifact(Scene.as_bytes()))?;
                self.engine = Some(TryNewRoomEngineFromScene(&scene)?);
                self.name = scene.Data().name.clone();
                self.observation = Some(Observation);
            }
            (Pending::Start(_), Reply::TacticalStarted { Scene, View, Art }) => {
                ValidateTactical(&View)?;
                if View.Frame != 0 {
                    return Err("initial tactical frame must be zero".into());
                }
                let scene =
                    StartupStage::Call("game_scene_artifact", || DecodeArtifact(Scene.as_bytes()))?;
                self.engine = Some(TryNewRoomEngineFromScene(&scene)?);
                self.name = scene.Data().name.clone();
                self.tactical = Some(View);
                self.art = Art;
            }
            (Pending::Tick(expected, _), Reply::TacticalUpdated { Index, View })
                if Index == *expected && self.tactical.is_some() =>
            {
                ValidateTactical(&View)?;
                let previous = self.tactical.as_ref().unwrap().Frame;
                if View.Frame != previous && View.Frame != previous.saturating_add(1) {
                    return Err("unexpected tactical frame index".into());
                }
                self.tactical_tick = Index;
                self.tactical = Some(View);
            }
            (Pending::Tick(expected, _), Reply::Updated { Index, Update })
                if Index == *expected && self.tactical.is_none() =>
            {
                ValidateObservation(&Update.Observation)?;
                if Update.HideBoxes.len() > 32 || Update.HideBoxes.iter().any(|id| id.len() > 256) {
                    return Err("game exceeded its per-tick action limits".into());
                }
                let engine = self.engine.as_mut().ok_or("game room is missing")?;
                engine.World.HideBoxes(&Update.HideBoxes)?;
                engine.TickSimulation();
                self.observation = Some(Update.Observation);
            }
            _ => return Err("unexpected game response kind or tick index".into()),
        }
        self.pending = None;
        Ok(true)
    }
}

fn ValidateObservation(observation: &Observation) -> Result<(), String> {
    if observation.Objective.len() > 240 || observation.Objective.chars().any(char::is_control) {
        return Err("game objective must be short single-line text".into());
    }
    Ok(())
}

fn ValidateTactical(view: &TacticalView) -> Result<(), String> {
    if view.Actors.is_empty()
        || view.Actors.len() > 32
        || view.Orders.len() > 16
        || view.Objective.len() > 240
        || view.Objective.chars().any(char::is_control)
        || !view.Relic.into_iter().all(f32::is_finite)
    {
        return Err("invalid tactical view".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for actor in &view.Actors {
        if !ids.insert(actor.Id)
            || actor.Name.len() > 48
            || actor.Name.chars().any(char::is_control)
            || actor.Health > actor.MaxHealth
            || actor.MaxHealth == 0
            || !actor.Yaw.is_finite()
            || !actor.Position.into_iter().all(f32::is_finite)
        {
            return Err("invalid tactical actor".into());
        }
    }
    if !ids.contains(&view.Selected) || view.Target.is_some_and(|id| !ids.contains(&id)) {
        return Err("invalid tactical selection".into());
    }
    for order in &view.Orders {
        if let TacticalOrder::MoveTo(position) = order
            && !position.iter().all(|axis| axis.is_finite())
        {
            return Err("invalid tactical destination".into());
        }
    }
    ValidateAgents(view, &ids)?;
    if let Some(players) = &view.Players {
        ValidatePlayers(view, players, &ids)?;
    }
    Ok(())
}

fn ValidatePlayers(
    view: &TacticalView,
    players: &PlayerView,
    ids: &std::collections::BTreeSet<u16>,
) -> Result<(), String> {
    use wyrmcoil_game::{MAX_PLAYER_EVENTS, PLAYER_SEATS};
    if players.Revision < view.Frame
        || players.Seats.len() != PLAYER_SEATS
        || players.Events.len() > MAX_PLAYER_EVENTS
        || players.Obstacles.len() > 64
        || players.Battle.len() != view.Actors.len()
    {
        return Err("invalid player snapshot bounds".into());
    }
    let mut owners = std::collections::BTreeSet::new();
    for (index, seat) in players.Seats.iter().enumerate() {
        if usize::from(seat.Id) != index
            || (index == 0 && (!seat.Claimed || seat.Kind != PlayerKind::Human))
            || (index > 0 && seat.Kind != PlayerKind::Llm)
            || seat.Orders.len() > 16
            || seat.Target.is_some_and(|id| !ids.contains(&id))
            || seat.Character.is_some_and(|id| {
                !seat.Claimed
                    || !owners.insert(id)
                    || !view
                        .Actors
                        .iter()
                        .any(|a| a.Id == id && a.PartySlot.is_some() && a.Health > 0)
            })
        {
            return Err("invalid player seat".into());
        }
    }
    if players
        .Events
        .windows(2)
        .any(|events| events[0].Sequence >= events[1].Sequence)
        || players
            .Events
            .iter()
            .any(|event| event.Tick > view.Frame || usize::from(event.Seat) >= PLAYER_SEATS)
    {
        return Err("invalid player event history".into());
    }
    let mut battle_ids = std::collections::BTreeSet::new();
    if players.Battle.iter().any(|fact| {
        !ids.contains(&fact.Id)
            || !battle_ids.insert(fact.Id)
            || fact.Target.is_some_and(|id| !ids.contains(&id))
    }) {
        return Err("invalid battle actor".into());
    }
    Ok(())
}

fn ValidateAgents(
    view: &TacticalView,
    ids: &std::collections::BTreeSet<u16>,
) -> Result<(), String> {
    let label = |s: &str| !s.is_empty() && s.len() <= 96 && !s.chars().any(char::is_control);
    let stack = |states: &[String]| states.len() <= 32 && states.iter().all(|s| label(s));
    let mut seen = std::collections::BTreeSet::new();
    let mut possessed = false;
    if view.Agents.len() > view.Actors.len() {
        return Err("too many tactical agent reports".into());
    }
    for agent in &view.Agents {
        if !seen.insert(agent.Actor)
            || !view
                .Actors
                .iter()
                .any(|a| a.Id == agent.Actor && a.PartySlot.is_some())
            || agent.Clock > view.Frame
            || agent.Stack.is_empty()
            || !stack(&agent.Stack)
            || agent.Target.is_some_and(|id| !ids.contains(&id))
            || (agent.Mode == ControlMode::PlayerControlled
                && (agent.Actor != view.Selected || std::mem::replace(&mut possessed, true)))
            || agent.Transitions.len() > 8
            || agent.Transitions.iter().any(|t| {
                t.Tick > agent.Clock
                    || t.WorldTick > view.Frame
                    || !label(&t.Reason)
                    || !stack(&t.Before)
                    || !stack(&t.After)
            })
            || agent.Commands.len() > 8
            || agent.Commands.iter().any(|c| {
                c.Tick > view.Frame
                    || match c.Order {
                        AgentOrder::Approach(id)
                        | AgentOrder::Attack(id)
                        | AgentOrder::Ability(id) => !ids.contains(&id),
                        _ => false,
                    }
            })
            || agent.Decisions.len() > 8
            || agent.Decision.iter().chain(&agent.Decisions).any(|d| {
                d.Tick > agent.Clock
                    || d.WorldTick > view.Frame
                    || !label(&d.Reason)
                    || !d.Hysteresis.is_finite()
                    || d.Hysteresis < 0.0
                    || d.Candidates.len() > 8
                    || d.Invalid.len() > 8
                    || d.Invalid.iter().any(|name| !label(name))
                    || d.Selected.as_ref().is_some_and(|name| !label(name))
                    || d.Current
                        .as_ref()
                        .is_some_and(|(name, score)| !label(name) || !score.is_finite())
                    || d.Candidates
                        .iter()
                        .any(|(name, score)| !label(name) || score.is_some_and(|s| !s.is_finite()))
            })
        {
            return Err("invalid tactical agent report".into());
        }
    }
    Ok(())
}
