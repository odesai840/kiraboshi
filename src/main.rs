#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod player;
mod audio;

fn main() -> Result<(), eframe::Error> {
    player::run()
}
