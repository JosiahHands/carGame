//! The game itself: driving, the garage, the worlds, events and the career.

use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

pub mod audio;
pub mod camera;
pub mod canvas;
pub mod carmodel;
pub mod config;
pub mod dev;
pub mod effects;
pub mod game;
pub mod hud;
pub mod input;
pub mod meshes;
pub mod packs;
pub mod physics;
pub mod rng;
pub mod settings;
pub mod storage;
pub mod text;
pub mod textures;
pub mod track;
pub mod venues;
pub mod world;
pub mod world_build;
pub mod world_render;

/// The game's version, from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
            ))
            .add_systems(Startup, game::setup_game)
            .add_systems(Update, (game::load_and_build, game::frame, game::place_camera).chain())
            .add_systems(EguiPrimaryContextPass, hud::hud);
    }
}
