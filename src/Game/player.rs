use serde::{Deserialize, Serialize};

use super::TacticalOrder;

pub const HUMAN_SEAT: u8 = 0;
pub const PLAYER_SEATS: usize = 3;
pub const MAX_PLAYER_EVENTS: usize = 32;
pub const PLAYER_REVISION_AGE: u64 = 360;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlayerKind {
    Human,
    Llm,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlayerAction {
    Claim,
    Leave,
    Possess { character: u16 },
    Release,
    Move { position: [f32; 2] },
    Attack { target: u16 },
    Ability { target: u16 },
    Target { target: u16 },
    Hold,
    Clear,
    Pause,
    Resume,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerRequest {
    pub Seat: u8,
    pub Receipt: u64,
    pub Observed: u64,
    pub Action: PlayerAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlayerResult {
    Accepted,
    Queued,
    Executed,
    Completed,
    Canceled,
    Rejected(PlayerError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlayerError {
    Unavailable,
    Unclaimed,
    AlreadyClaimed,
    Occupied,
    NoCharacter,
    InvalidCharacter,
    InvalidTarget,
    InvalidPosition,
    Cooldown,
    QueueFull,
    Stale,
    NotPlayable,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerEvent {
    pub Sequence: u64,
    pub Tick: u64,
    pub Seat: u8,
    pub Receipt: u64,
    pub Actor: Option<u16>,
    pub Action: PlayerAction,
    pub Result: PlayerResult,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerSeat {
    pub Id: u8,
    pub Kind: PlayerKind,
    pub Claimed: bool,
    pub Character: Option<u16>,
    pub Target: Option<u16>,
    pub Last: Option<PlayerEvent>,
    pub Orders: Vec<TacticalOrder>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BattleActor {
    pub Id: u16,
    pub Target: Option<u16>,
    pub Facing: [i32; 2],
    pub Stunned: bool,
    pub Slowed: bool,
    pub Blocking: bool,
    pub HeavyThreat: bool,
    pub Advantage: bool,
    pub Ability: Option<u8>,
    pub Range: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerView {
    pub Revision: u64,
    pub Seats: Vec<PlayerSeat>,
    pub Events: Vec<PlayerEvent>,
    pub Obstacles: Vec<[i32; 2]>,
    pub Battle: Vec<BattleActor>,
}
