#![allow(non_snake_case)]

use wyrmcoil_game::{Game, GameState, GameTick, GameUpdate, Observation, Scene::SceneDocument};

use crate::scene::{BuildScene, EXIT, KEYS};

const GUARD_RADIUS: f32 = 0.82;

pub struct Guardhouse {
    collected: [bool; 3],
    state: GameState,
    at_locked_exit: bool,
}

impl Default for Guardhouse {
    fn default() -> Self {
        Self {
            collected: [false; 3],
            state: GameState::Playing,
            at_locked_exit: false,
        }
    }
}

impl Guardhouse {
    fn Observe(&self) -> Observation {
        let remaining = self
            .collected
            .iter()
            .filter(|&&collected| !collected)
            .count() as u32;
        let objective = match self.state {
            GameState::Won => "Escaped. Restart Game to play again.",
            GameState::Lost => "Caught by the guard. Restart Game to try again.",
            GameState::Playing if remaining == 0 => "All keys found. Reach the green exit.",
            GameState::Playing if self.at_locked_exit => {
                "Exit locked. Find the remaining gold keys."
            }
            GameState::Playing => {
                "Collect the gold keys, avoid the red guard, then reach the green exit."
            }
        };
        Observation {
            State: self.state,
            Remaining: remaining,
            Objective: objective.into(),
        }
    }
}

impl Game for Guardhouse {
    fn Start(&mut self) -> Result<(SceneDocument, Observation), String> {
        *self = Self::default();
        Ok((BuildScene(), self.Observe()))
    }

    fn FixedUpdate(&mut self, tick: GameTick) -> Result<GameUpdate, String> {
        let mut hidden = Vec::new();
        if self.state == GameState::Playing {
            if Near(tick.Player, tick.Guard, GUARD_RADIUS) {
                self.state = GameState::Lost;
            } else {
                for (index, (id, position)) in KEYS.into_iter().enumerate() {
                    if !self.collected[index] && Near(tick.Player, position, 0.65) {
                        self.collected[index] = true;
                        hidden.push(id.into());
                    }
                }
                self.at_locked_exit = Near(tick.Player, EXIT, 0.8);
                if self.at_locked_exit && self.collected.iter().all(|&collected| collected) {
                    self.state = GameState::Won;
                }
            }
        }
        Ok(GameUpdate {
            HideBoxes: hidden,
            Observation: self.Observe(),
        })
    }
}

fn Near(player: [f32; 3], other: [f32; 3], radius: f32) -> bool {
    let dx = player[0] - other[0];
    let dz = player[2] - other[2];
    dx * dx + dz * dz <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    fn Tick(player: [f32; 3], guard: [f32; 3]) -> GameTick {
        GameTick {
            Index: 1,
            Input: Default::default(),
            Player: player,
            Guard: guard,
        }
    }

    #[test]
    fn exit_requires_all_keys_and_restart_changes_nothing_about_initial_state() {
        let mut game = Guardhouse::default();
        let initial = game.Start().unwrap().1;
        let far = [8.0, 0.0, 8.0];
        assert!(
            game.FixedUpdate(Tick(EXIT, far))
                .unwrap()
                .Observation
                .Objective
                .contains("locked")
        );
        for (index, (id, position)) in KEYS.into_iter().enumerate() {
            let update = game.FixedUpdate(Tick(position, far)).unwrap();
            assert_eq!(update.HideBoxes, [id]);
            assert_eq!(update.Observation.Remaining, 2 - index as u32);
            assert!(
                game.FixedUpdate(Tick(position, far))
                    .unwrap()
                    .HideBoxes
                    .is_empty()
            );
        }
        assert_eq!(
            game.FixedUpdate(Tick(EXIT, far)).unwrap().Observation.State,
            GameState::Won
        );
        assert_eq!(game.Start().unwrap().1, initial);
    }

    #[test]
    fn caught_is_terminal_until_restart() {
        let mut game = Guardhouse::default();
        assert_eq!(
            game.FixedUpdate(Tick([0.0, 1.45, 0.0], [0.3, 0.5, 0.2]))
                .unwrap()
                .Observation
                .State,
            GameState::Lost
        );
        assert_eq!(
            game.FixedUpdate(Tick(KEYS[0].1, [10.0; 3]))
                .unwrap()
                .Observation
                .Remaining,
            3
        );
        assert_eq!(game.Start().unwrap().1.State, GameState::Playing);
    }
}
