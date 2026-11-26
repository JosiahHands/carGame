//! Real Physics pack: Suspension, tire grip that warms up and wears, engines, gearboxes, brake fade and crash damage.
//!
//! Rebuilt in stage 5 of 1.3.0. For now it only adds itself to the packs list.

use bevy::prelude::*;
use game::packs::{PackInfo, RegisterPack};

pub struct RealPhysicsPack;

impl Plugin for RealPhysicsPack {
    fn build(&self, app: &mut App) {
        app.register_pack(PackInfo {
            id: "physics",
            name: "Real Physics",
            about: "Suspension, tire grip that warms up and wears, engines, gearboxes, brake fade and crash damage.",
        });
    }
}
