//! Car Game: driving, the garage, the worlds, events and the career.
//!
//! The code is in three parts:
//! - backEnd/: the game's rules and data. The main loop, car physics, the
//!   worlds, events and rivals, the city's traffic and people, the career,
//!   saves and settings.
//! - frontEnd/: what you see, hear and touch. Drawing the worlds and cars,
//!   effects, the menus, garage and on-screen displays, sound, the controls
//!   and the camera.
//! - shared/: small things both use. Random numbers, the date, number and
//!   time formatting, the packs list, and helpers for unattended checks.
//!
//! Each file holds a few closely related parts. They're all listed here
//! under their own names (`game::world`, `game::hud`...), so the rest of the
//! game and the packs don't need to know which file a part is in.

// Game code passes a lot of state around; these lints fight that more than they help.
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::inconsistent_digit_grouping)]

use bevy::prelude::*;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};

// ---------- shared by both ----------
#[path = "shared/basics.rs"]
pub mod basics;

pub use basics::{clock, dev, packs, rng, text};

// ---------- the back end ----------
#[path = "backEnd/career.rs"]
pub mod career;
#[path = "backEnd/city.rs"]
pub mod city;
#[path = "backEnd/events.rs"]
pub mod event;
#[path = "backEnd/game.rs"]
pub mod game;
#[path = "backEnd/physics.rs"]
pub mod physics;
#[path = "backEnd/saves.rs"]
pub mod saves;
#[path = "backEnd/world.rs"]
pub mod world;

pub use career::rewards;
pub use city::{pedestrians, traffic};
pub use event::{freeroam, rivals, skills, spec, timetrial, weapons};
pub use saves::{config, import, settings, storage};
pub use world::{track, venues};

// ---------- the front end ----------
#[path = "frontEnd/sound.rs"]
pub mod audio;
#[path = "frontEnd/car.rs"]
pub mod carmodel;
#[path = "frontEnd/controls.rs"]
pub mod controls;
#[path = "frontEnd/graphics.rs"]
pub mod graphics;
#[path = "frontEnd/hud.rs"]
pub mod hud;
#[path = "frontEnd/menus.rs"]
pub mod menu;
#[path = "frontEnd/render.rs"]
pub mod render;

pub use audio::speak;
pub use controls::{camera, input};
pub use graphics::{canvas, effects, meshes, quality, scene, textures};
pub use hud::minimap;
pub use menu::garage;
pub use render::{world_build, world_render};

/// The game's version, from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Systems that run the game each frame, in order. Packs can run theirs before or after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    Load,
    Frame,
    Camera,
}

/// Everything in the game. Packs add their own plugins next to it.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(storage::Storage::open())
            .insert_resource(world::World::empty())
            .init_resource::<camera::CameraRig>()
            .init_resource::<packs::Packs>()
            .insert_resource(EguiGlobalSettings {
                auto_create_primary_context: false,
                ..default()
            })
            .insert_resource(ClearColor(Color::srgb(0.55, 0.75, 0.95)))
            .add_plugins((
                EguiPlugin::default(),
                dev::DevPlugin,
                audio::AudioPlugin,
                world_render::WorldRenderPlugin,
                scene::ScenePlugin,
            ))
            .configure_sets(Update, (GameSet::Load, GameSet::Frame, GameSet::Camera).chain())
            .add_systems(Startup, game::setup_game.after(audio_start_marker))
            .add_systems(
                Update,
                (
                    game::load_and_build.in_set(GameSet::Load),
                    game::frame.in_set(GameSet::Frame),
                    game::place_camera.in_set(GameSet::Camera),
                    quality::refine.before(GameSet::Load),
                ),
            )
            .add_systems(EguiPrimaryContextPass, (game::menus, hud::hud).chain());
    }
}

/// Startup systems that must run before the game is set up.
fn audio_start_marker() {}

/// The folder the game's files (the packs' car models) are read from: next to
/// the program in a download, or the top of the repository while developing.
pub fn assets_folder() -> &'static std::path::Path {
    static FOLDER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    FOLDER.get_or_init(|| {
        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(std::path::PathBuf::from));
        let candidates = [
            exe.as_ref().map(|d| d.join("assets")),
            // macOS app bundle: Contents/MacOS/car-game, files in Contents/Resources/assets.
            exe.as_ref().map(|d| d.join("../Resources/assets")),
            // AppImage and Linux packages: usr/bin/car-game, files in usr/share/car-game/assets.
            exe.as_ref().map(|d| d.join("../share/car-game/assets")),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|p| p.is_dir())
            .unwrap_or_else(|| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/..")))
    })
}
