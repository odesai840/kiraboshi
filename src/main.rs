mod player;
mod audio;

fn main() -> Result<(), eframe::Error> {
    player::run()
}
