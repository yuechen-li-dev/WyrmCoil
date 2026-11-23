//! Host-side execution of a trusted, independently built game.

use std::path::Path;
use std::time::{Duration, Instant};

pub use wyrmcoil_game::TacticalArt;
use wyrmcoil_game::Wire::{Reply, Request};
pub use wyrmcoil_game::{
    ActorMotion, MAX_TACTICAL_COMMANDS, TacticalActor, TacticalCommand, TacticalOrder, TacticalView,
};
pub use wyrmcoil_game::{GAME_ARTIFACT, GameState, Observation, PROTOCOL};
use wyrmcoil_game::{GameInput, GameTick};

use crate::Demo::room::{RoomInput, RoomSnapshot, RoomWorld, TryNewRoomEngineFromScene};
use crate::Engine::startup::StartupStage;
use crate::Engine::{Engine, EngineClock, World};
use crate::Scene::DecodeArtifact;

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

    pub fn BeginTacticalTick(&mut self, commands: Vec<TacticalCommand>) -> Result<(), String> {
        if self.tactical.is_none() || self.Busy() || !self.Playing() {
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
    Ok(())
}
