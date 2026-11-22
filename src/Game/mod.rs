#![allow(non_snake_case)]
//! The external game authoring and wire contract.

use std::io::{self, BufReader, BufWriter, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
pub use wyrmcoil_scene as Scene;
mod tactical;
pub use tactical::*;

pub const PROTOCOL: u32 = 1;
pub const TICK_HZ: u32 = 60;
pub const MAX_FRAME: usize = 2 * 1024 * 1024;
pub const GAME_ARTIFACT: &str = if cfg!(windows) {
    "target/wyrmcoil-game.exe"
} else {
    "target/wyrmcoil-game"
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameInput {
    pub Strafe: i32,
    pub Forward: i32,
    pub Yaw: i32,
    pub Pitch: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTick {
    pub Index: u64,
    pub Input: GameInput,
    /// World coordinates sampled after control and before simulation.
    pub Player: [f32; 3],
    pub Guard: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameState {
    Playing,
    Won,
    Lost,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub State: GameState,
    pub Remaining: u32,
    pub Objective: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameUpdate {
    pub HideBoxes: Vec<String>,
    pub Observation: Observation,
}

pub trait Game {
    fn Start(&mut self) -> Result<(Scene::SceneDocument, Observation), String>;
    fn FixedUpdate(&mut self, tick: GameTick) -> Result<GameUpdate, String>;
}

#[doc(hidden)]
pub mod Wire {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Packet<T> {
        pub Version: u32,
        pub Data: T,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum Request {
        Start,
        Tick(GameTick),
        TacticalTick {
            Index: u64,
            Commands: Vec<TacticalCommand>,
        },
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum Reply {
        Started {
            Scene: String,
            Observation: Observation,
        },
        Updated {
            Index: u64,
            Update: GameUpdate,
        },
        TacticalStarted {
            Scene: String,
            View: TacticalView,
            Art: Option<TacticalArt>,
        },
        TacticalUpdated {
            Index: u64,
            View: TacticalView,
        },
        Failed(String),
    }

    pub fn WriteFrame(writer: &mut impl Write, data: impl Serialize) -> Result<(), String> {
        let packet = Packet {
            Version: PROTOCOL,
            Data: data,
        };
        let bytes = serde_json::to_vec(&packet).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_FRAME {
            return Err("game packet exceeds its size limit".into());
        }
        writer
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .and_then(|()| writer.write_all(&bytes))
            .and_then(|()| writer.flush())
            .map_err(|error| error.to_string())
    }

    pub fn ReadFrame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, String> {
        let mut header = [0; 4];
        if reader
            .read(&mut header[..1])
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(None);
        }
        reader
            .read_exact(&mut header[1..])
            .map_err(|error| format!("truncated game header: {error}"))?;
        let length = u32::from_le_bytes(header) as usize;
        if !(1..=MAX_FRAME).contains(&length) {
            return Err("invalid game packet length".into());
        }
        let mut data = vec![0; length];
        reader
            .read_exact(&mut data)
            .map_err(|error| format!("truncated game packet: {error}"))?;
        let packet: Packet<T> = serde_json::from_slice(&data)
            .map_err(|error| format!("invalid game packet: {error}"))?;
        if packet.Version != PROTOCOL {
            return Err(format!(
                "game protocol {} is incompatible with {PROTOCOL}",
                packet.Version
            ));
        }
        Ok(Some(packet.Data))
    }
}

/// Serve requests from WyrmCoil. Applications should send diagnostics to stderr.
pub fn Serve(mut game: impl Game) -> Result<(), String> {
    use Wire::{Reply, Request};

    let mut next_tick = 0;
    ServeFrames(|request| match request {
        Request::Start if next_tick == 0 => {
            next_tick = 1;
            let (document, observation) = game.Start()?;
            let scene = Scene::CompileScene(&document)?;
            Ok(Reply::Started {
                Scene: Scene::EncodeArtifact(&scene)?,
                Observation: observation,
            })
        }
        Request::Tick(tick) if next_tick > 0 && tick.Index == next_tick => {
            next_tick = next_tick.checked_add(1).ok_or("game tick overflow")?;
            Ok(Reply::Updated {
                Index: tick.Index,
                Update: game.FixedUpdate(tick)?,
            })
        }
        _ => Err("game received an unexpected request or tick".into()),
    })
}

fn ServeFrames(
    mut handle: impl FnMut(Wire::Request) -> Result<Wire::Reply, String>,
) -> Result<(), String> {
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = BufWriter::new(io::stdout().lock());
    while let Some(request) = Wire::ReadFrame(&mut input)? {
        match handle(request) {
            Ok(reply) => Wire::WriteFrame(&mut output, reply)?,
            Err(error) => {
                Wire::WriteFrame(&mut output, Wire::Reply::Failed(error.clone()))?;
                return Err(error);
            }
        }
    }
    Ok(())
}
