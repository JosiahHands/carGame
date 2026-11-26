//! Helpers for checking the game without anyone watching (on a build
//! machine, for example).
//!
//! `CAR_GAME_SCREENSHOT=shot.png car-game` saves a picture of the game once
//! it has settled, then quits.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

/// Frames to wait before the picture, so the first shaders have compiled.
const SETTLE_FRAMES: u32 = 120;
/// Frames to keep running once the picture is on disk.
const SAVE_FRAMES: u32 = 5;
/// Frames to wait for the picture at most (software renderers can take a
/// long time to compile the capture pipeline), before giving up with an error.
const GIVE_UP_FRAMES: u32 = 3000;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        if let Some(path) = std::env::var_os("CAR_GAME_SCREENSHOT") {
            app.insert_resource(ScreenshotThenQuit {
                path: path.into(),
                frame: 0,
                capture_at: std::env::var("CAR_GAME_SCREENSHOT_FRAMES")
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(SETTLE_FRAMES)
                    .clamp(1, 3600),
                saved_at: None,
            })
            .add_systems(Update, screenshot_then_quit);
        }
    }
}

#[derive(Resource)]
struct ScreenshotThenQuit {
    path: std::path::PathBuf,
    frame: u32,
    capture_at: u32,
    /// The frame the picture was found on disk.
    saved_at: Option<u32>,
}

fn screenshot_then_quit(mut commands: Commands, mut shot: ResMut<ScreenshotThenQuit>, mut exit: MessageWriter<AppExit>) {
    shot.frame += 1;
    if shot.frame == 1 {
        // A picture left over from an earlier run mustn't count as this one.
        let _ = std::fs::remove_file(&shot.path);
    }
    if shot.frame == shot.capture_at {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(shot.path.clone()));
    } else if shot.frame > shot.capture_at {
        // Quit once the picture is really on disk, however long the capture takes.
        if shot.saved_at.is_none() && std::fs::metadata(&shot.path).is_ok_and(|m| m.len() > 0) {
            shot.saved_at = Some(shot.frame);
        }
        match shot.saved_at {
            Some(at) if shot.frame >= at + SAVE_FRAMES => {
                exit.write(AppExit::Success);
            }
            None if shot.frame > shot.capture_at + GIVE_UP_FRAMES => {
                error!("The screenshot was never saved to {}", shot.path.display());
                exit.write(AppExit::error());
            }
            _ => {}
        }
    }
}

/// `CAR_GAME_AUTOPILOT=steer` holds the throttle down and the steering at
/// `steer` (-1..1), to see the car moving in screenshots.
pub fn autopilot() -> Option<f64> {
    std::env::var("CAR_GAME_AUTOPILOT").ok().and_then(|v| v.parse().ok())
}
