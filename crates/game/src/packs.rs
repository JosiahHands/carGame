//! Packs: optional parts of the game (Real Physics, Photo Real, Living City).
//! Each pack is a Bevy plugin that adds itself to this list when it starts.

use bevy::prelude::*;

/// What a pack is, for the packs screen.
#[derive(Clone, Debug)]
pub struct PackInfo {
    /// Short name used in saved settings, like "physics".
    pub id: &'static str,
    /// Name shown in the game, like "Real Physics".
    pub name: &'static str,
    /// One line about what the pack does.
    pub about: &'static str,
}

/// Every pack in this build of the game.
#[derive(Resource, Default, Debug)]
pub struct Packs {
    pub list: Vec<PackInfo>,
}

/// Lets a pack plugin add itself to the list.
pub trait RegisterPack {
    fn register_pack(&mut self, info: PackInfo) -> &mut Self;
}

impl RegisterPack for App {
    fn register_pack(&mut self, info: PackInfo) -> &mut Self {
        self.world_mut().get_resource_or_init::<Packs>().list.push(info);
        self
    }
}
