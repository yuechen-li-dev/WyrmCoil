use super::{
    GameState, Scene, ServeFrames,
    Wire::{Reply, Request},
};
use serde::{Deserialize, Serialize};

pub const MAX_TACTICAL_COMMANDS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TacticalCommand {
    TogglePause,
    SelectParty(u8),
    Inspect(u16),
    MoveTo([f32; 2]),
    Attack(u16),
    Ability(u8),
    Clear,
    Hold,
    ReleaseParty,
    SetPartyAutonomy(bool),
    Player(super::PlayerRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorMotion {
    Idle,
    Walk,
    Attack,
    Ability,
    Hit,
    Death,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalActor {
    pub Id: u16,
    pub Name: String,
    pub PartySlot: Option<u8>,
    pub Position: [f32; 2],
    pub Yaw: f32,
    pub Health: u16,
    pub MaxHealth: u16,
    pub Cooldown: u16,
    pub Motion: ActorMotion,
    pub MotionTicks: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TacticalOrder {
    MoveTo([f32; 2]),
    Attack(u16),
    Ability(u16),
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlMode {
    Autonomous,
    PlayerControlled,
    LlmControlled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentDecision {
    pub Tick: u64,
    pub WorldTick: u64,
    pub Candidates: Vec<(String, Option<f32>)>,
    pub Invalid: Vec<String>,
    pub Current: Option<(String, f32)>,
    pub Selected: Option<String>,
    pub Hysteresis: f32,
    pub CommitRemaining: u64,
    pub Reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentTransition {
    pub Tick: u64,
    pub WorldTick: u64,
    pub Before: Vec<String>,
    pub After: Vec<String>,
    pub Reason: String,
    pub Interrupt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOutcome {
    InProgress,
    Completed,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentCommand {
    pub Tick: u64,
    pub Order: AgentOrder,
    pub Outcome: CommandOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentOrder {
    Move([i32; 2]),
    Approach(u16),
    Attack(u16),
    Ability(u16),
    Hold,
    Block,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentInspection {
    pub Actor: u16,
    pub Mode: ControlMode,
    pub Clock: u64,
    pub Stack: Vec<String>,
    pub Target: Option<u16>,
    pub Blocking: bool,
    pub Decision: Option<AgentDecision>,
    pub Decisions: Vec<AgentDecision>,
    pub Transitions: Vec<AgentTransition>,
    pub Commands: Vec<AgentCommand>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalView {
    pub Frame: u64,
    pub Paused: bool,
    pub State: GameState,
    pub Objective: String,
    pub Selected: u16,
    pub Target: Option<u16>,
    pub Actors: Vec<TacticalActor>,
    pub Orders: Vec<TacticalOrder>,
    pub Relic: [f32; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub Agents: Vec<AgentInspection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub Players: Option<super::PlayerView>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TacticalArt {
    pub Props: Vec<PropArt>,
    pub Actors: Vec<ActorArt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PropArt {
    pub Mesh: String,
    pub Position: [f32; 3],
    pub Scale: [f32; 3],
    pub Yaw: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActorArt {
    pub Id: u16,
    pub Mesh: String,
    /// Idle, walk, attack, ability, hit, death.
    pub Clips: [ClipArt; 6],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub Equipment: Vec<EquipmentArt>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EquipmentArt {
    pub Mesh: String,
    pub Socket: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipArt {
    pub File: String,
    pub Name: String,
}

pub trait TacticalGame {
    fn Start(&mut self) -> Result<(Scene::SceneDocument, TacticalView), String>;
    fn FixedUpdate(&mut self, commands: &[TacticalCommand]) -> Result<TacticalView, String>;
    fn Art(&self) -> Option<TacticalArt> {
        None
    }
}

/// Runs an opt-in tactical game on the same framed transport as room games.
pub fn ServeTactical(mut game: impl TacticalGame) -> Result<(), String> {
    let mut next_tick = 0;
    ServeFrames(|request| match request {
        Request::Start if next_tick == 0 => {
            next_tick = 1;
            let (document, view) = game.Start()?;
            let scene = Scene::CompileScene(&document)?;
            Ok(Reply::TacticalStarted {
                Scene: Scene::EncodeArtifact(&scene)?,
                View: view,
                Art: game.Art(),
            })
        }
        Request::TacticalTick { Index, Commands }
            if next_tick > 0 && Index == next_tick && Commands.len() <= MAX_TACTICAL_COMMANDS =>
        {
            next_tick = next_tick.checked_add(1).ok_or("game tick overflow")?;
            Ok(Reply::TacticalUpdated {
                Index,
                View: game.FixedUpdate(&Commands)?,
            })
        }
        _ => Err("tactical game received an unexpected request or tick".into()),
    })
}
