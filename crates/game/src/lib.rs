//! The game itself: driving, the garage, the worlds, events and the career.

// Game code passes a lot of state around; these lints fight that more than they help.
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::inconsistent_digit_grouping)]

use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

pub mod audio;
pub mod camera;
pub mod canvas;
pub mod career;
pub mod carmodel;
pub mod clock;
pub mod config;
pub mod dev;
pub mod effects;
pub mod event;
pub mod freeroam;
pub mod game;
pub mod garage;
pub mod hud;
pub mod input;
pub mod menu;
pub mod meshes;
pub mod minimap;
pub mod packs;
pub mod pedestrians;
pub mod physics;
pub mod quality;
pub mod rewards;
pub mod rivals;
pub mod rng;
pub mod scene;
pub mod settings;
pub mod skills;
pub mod speak;
pub mod spec;
pub mod storage;
pub mod text;
pub mod textures;
pub mod timetrial;
pub mod track;
pub mod traffic;
pub mod venues;
pub mod weapons;
pub mod world;
pub mod world_build;
pub mod world_render;

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
                    quality::refine,
                ),
            )
            .add_systems(EguiPrimaryContextPass, (game::menus, hud::hud).chain());
    }
}

/// Startup systems that must run before the game is set up.
fn audio_start_marker() {}
