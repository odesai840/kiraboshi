#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod player;
mod audio;

use std::path::PathBuf;

fn main() -> Result<(), eframe::Error> {
    let file_arg = std::env::args().nth(1).map(PathBuf::from);
    player::run(file_arg)
}
