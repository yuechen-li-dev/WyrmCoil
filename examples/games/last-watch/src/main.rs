#![allow(non_snake_case)]

mod encounter;
mod scene;

fn main() -> Result<(), String> {
    wyrmcoil_game::ServeTactical(encounter::Encounter::New())
}
