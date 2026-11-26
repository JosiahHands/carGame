//! Helpers for checking the game without anyone watching (on a build
//! machine, for example).
//!
//! `CAR_GAME_SCREENSHOT=shot.png car-game` saves a picture of the game once
//! it has settled, then quits.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

/// Frames to wait before the picture, so the first shaders have compiled.
const SETTLE_FRAMES: u32 = 120;
/// Frames to wait after asking for it, so it's on disk before quitting.
const SAVE_FRAMES: u32 = 30;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        if let Some(path) = std::env::var_os("CAR_GAME_SCREENSHOT") {
            app.insert_resource(ScreenshotThenQuit {
                path: path.into(),
                frame: 0,
            })
            .add_systems(Update, screenshot_then_quit);
        }
    }
}

#[derive(Resource)]
struct ScreenshotThenQuit {
    path: std::path::PathBuf,
    frame: u32,
}

fn screenshot_then_quit(mut commands: Commands, mut shot: ResMut<ScreenshotThenQuit>, mut exit: MessageWriter<AppExit>) {
    shot.frame += 1;
    if shot.frame == SETTLE_FRAMES {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(shot.path.clone()));
    } else if shot.frame == SETTLE_FRAMES + SAVE_FRAMES {
        exit.write(AppExit::Success);
    }
}

/// `CAR_GAME_AUTOPILOT=steer` holds the throttle down and the steering at
/// `steer` (-1..1), to see the car moving in screenshots.
pub fn autopilot() -> Option<f64> {
    std::env::var("CAR_GAME_AUTOPILOT").ok().and_then(|v| v.parse().ok())
}
