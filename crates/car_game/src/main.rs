//! Car Game: starts Bevy with the game and every pack. Packs can each be
//! turned on and off in the game.

// No console window behind the game on Windows (release builds only, so
// errors still show while developing).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

/// The game's files (the car models): next to the program in a download, or
/// the repository's `assets` folder while developing.
fn assets_folder() -> String {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from));
    let candidates = [
        exe.as_ref().map(|d| d.join("assets")),
        // macOS app bundle: Contents/MacOS/car-game, files in Contents/Resources/assets.
        exe.as_ref().map(|d| d.join("../Resources/assets")),
        // AppImage and Linux packages: usr/bin/car-game, files in usr/share/car-game/assets.
        exe.as_ref().map(|d| d.join("../share/car-game/assets")),
        Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| p.is_dir())
        .map_or_else(|| "assets".into(), |p| p.to_string_lossy().into_owned())
}

fn main() -> AppExit {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("Car Game {}", game::VERSION),
                        resolution: WindowResolution::new(1280, 720),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: assets_folder(),
                    ..default()
                }),
        )
        .add_plugins(game::GamePlugin)
        .add_plugins((
            real_physics::RealPhysicsPack,
            photo_real::PhotoRealPack,
            living_city::LivingCityPack,
        ))
        .run()
}
