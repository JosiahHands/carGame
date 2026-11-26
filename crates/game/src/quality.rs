//! Picks graphics settings that suit this computer on the first start:
//! Low on small or old machines (the target is a steady 60 frames a second
//! at 720p on an Intel UHD 620 laptop with 4 GB of memory), up to High on
//! big ones. The player can change it any time in Garage → Settings.
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;

use crate::game::Game;

/// A first guess from the memory and processor.
pub fn pick_preset() -> &'static str {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let gb = sys.total_memory() as f64 / 1024f64.powi(3);
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    if gb > 0.0 && gb < 6.0 || cores <= 2 {
        "Low"
    } else if gb < 12.0 || cores <= 4 {
        "Medium"
    } else {
        "High"
    }
}

/// Once the graphics chip is known: integrated graphics get at most Medium,
/// and software drawing gets Low.
pub fn refine(
    adapter: Option<Res<RenderAdapterInfo>>,
    mut game: ResMut<Game>,
    mut done: Local<bool>,
    mut storage: ResMut<crate::storage::Storage>,
    mut graphics: ResMut<crate::world_render::Graphics>,
) {
    if *done {
        return;
    }
    let Some(adapter) = adapter else { return };
    *done = true;
    if !game.first_start {
        return;
    }
    let kind = format!("{:?}", adapter.device_type);
    let cap = match kind.as_str() {
        "Cpu" | "VirtualGpu" => "Low",
        "IntegratedGpu" => {
            if game.settings.quality == "Low" {
                "Low"
            } else {
                "Medium"
            }
        }
        _ => return,
    };
    let order = ["Low", "Medium", "High", "Ultra"];
    let now = order.iter().position(|p| *p == game.settings.quality).unwrap_or(2);
    let limit = order.iter().position(|p| *p == cap).unwrap_or(0);
    if now > limit && storage.get("carGame.imported").is_none() {
        game.settings.apply_preset(cap);
        let s = game.settings.clone();
        crate::settings::save_settings(&mut storage, &s);
        graphics.shadows = s.shadows.clone();
        graphics.view_distance = s.view_distance as f32;
        if let Some(p) = game.popups.last_mut() {
            p.title = format!("Graphics: {cap}");
        }
    }
}
