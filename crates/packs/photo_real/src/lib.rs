//! Photo Real pack: Real car bodies, lifelike lighting and materials, and camera effects.
//!
//! Rebuilt in stage 5 of 1.3.0. For now it only adds itself to the packs list.

use bevy::prelude::*;
use game::packs::{PackInfo, RegisterPack};

pub struct PhotoRealPack;

impl Plugin for PhotoRealPack {
    fn build(&self, app: &mut App) {
        app.register_pack(PackInfo {
            id: "graphics",
            name: "Photo Real",
            about: "Real car bodies, lifelike lighting and materials, and camera effects.",
        });
    }
}
