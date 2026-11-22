mod game;
mod scene;

fn main() -> Result<(), String> {
    wyrmcoil_game::Serve(game::Guardhouse::default())
}
