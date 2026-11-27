//! Car Game: starts Bevy with the game and every pack. Packs can each be
//! turned on and off in the game.

// No console window behind the game on Windows (release builds only, so
// errors still show while developing).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::window::{PresentMode, PrimaryWindow, WindowResolution};

fn main() -> AppExit {
    let exit = App::new()
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
                    file_path: game::assets_folder().to_string_lossy().into_owned(),
                    ..default()
                })
                .set(RenderPlugin {
                    // Software renderers can leave pipelines queued during unattended captures.
                    synchronous_pipeline_compilation: std::env::var_os("CAR_GAME_SCREENSHOT").is_some(),
                    ..default()
                }),
        )
        .add_systems(Update, window_icon)
        .add_plugins(game::GamePlugin)
        .add_plugins((
            real_physics::RealPhysicsPack,
            photo_real::PhotoRealPack,
            living_city::LivingCityPack,
        ))
        .run();
    // After an update: the new version starts once this one has closed and saved.
    game::updates::start_new_version();
    exit
}

/// Gives the game's window its icon, once the window is open: Windows shows
/// it in the taskbar and Alt+Tab, Linux in its task switcher. (On macOS the
/// app bundle's icon is used instead.)
fn window_icon(_main_thread: NonSendMarker, window: Query<Entity, With<PrimaryWindow>>, mut done: Local<bool>) {
    if *done || cfg!(target_os = "macos") {
        return;
    }
    let Ok(entity) = window.single() else { return };
    bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
        let Some(window) = windows.get_window(entity) else { return };
        *done = true;
        let Ok(image) = image::load_from_memory_with_format(include_bytes!("frontEnd/icons/icon.png"), image::ImageFormat::Png) else {
            return;
        };
        let image = image.resize(256, 256, image::imageops::FilterType::Triangle).into_rgba8();
        let (width, height) = image.dimensions();
        if let Ok(icon) = winit::window::Icon::from_rgba(image.into_raw(), width, height) {
            window.set_window_icon(Some(icon));
        }
    });
}
