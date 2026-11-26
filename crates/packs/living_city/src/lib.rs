//! Living City pack: People in the city who live their own lives, remember you and talk, with a local AI.
//!
//! Rebuilt in stage 5 of 1.3.0. For now it only adds itself to the packs list.

use bevy::prelude::*;
use game::packs::{PackInfo, RegisterPack};

pub struct LivingCityPack;

impl Plugin for LivingCityPack {
    fn build(&self, app: &mut App) {
        app.register_pack(PackInfo {
            id: "npc",
            name: "Living City",
            about: "People in the city who live their own lives, remember you and talk, with a local AI.",
        });
    }
}
