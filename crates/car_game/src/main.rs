//! Car Game: starts Bevy with the game and every pack. Packs can each be
//! turned on and off in the game.

// No console window behind the game on Windows (release builds only, so
// errors still show while developing).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Car Game {}", game::VERSION),
                resolution: WindowResolution::new(1280, 720),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(game::GamePlugin)
        .add_plugins((
            real_physics::RealPhysicsPack,
            photo_real::PhotoRealPack,
            living_city::LivingCityPack,
        ))
        .run()
}
