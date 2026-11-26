//! Graphics and game settings, chosen on the Settings tab of the garage (and
//! the map, on the Maps tab). Saved between sessions, like the car.
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::storage::Storage;
use crate::venues::MAP_NAMES;

#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// How sharp the picture is: 1 draws every pixel, 0.5 draws a quarter as many.
    pub resolution: f64,
    /// "off" | "low" | "high" | "ultra"
    pub shadows: String,
    /// How far you can see, in meters, before everything fades into the fog.
    pub view_distance: f64,
    /// "low" | "medium" | "high": how much smoke, fire and debris.
    pub effects: String,
    /// Highest frames per second to draw; 0 means no limit.
    pub fps_limit: f64,
    pub antialias: bool,
    pub fov: f64,
    pub show_fps: bool,
    pub minimap: bool,
    pub weapons: bool,
    /// "mph" | "kmh"
    pub units: String,
    pub map: String,
    /// "auto" | "manual"
    pub transmission: String,
    pub music: bool,
    pub announcer: bool,
    pub speed_lines: bool,
    /// "off" | "light" | "normal" | "busy"
    pub traffic: String,
    pub pedestrians: bool,
    /// New in 1.3.0: the quality preset picked for this computer on the first start ("" until then).
    #[serde(default)]
    pub quality: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            resolution: 1.0,
            shadows: "high".into(),
            view_distance: 430.0,
            effects: "high".into(),
            fps_limit: 0.0,
            antialias: true,
            fov: 62.0,
            show_fps: false,
            minimap: true,
            weapons: true,
            units: "mph".into(),
            map: "city".into(),
            transmission: "auto".into(),
            music: true,
            announcer: true,
            speed_lines: true,
            traffic: "normal".into(),
            pedestrians: true,
            quality: String::new(),
        }
    }
}

/// Traffic cars for each Traffic setting.
pub fn traffic_count(setting: &str) -> usize {
    match setting {
        "off" => 0,
        "light" => 16,
        "busy" => 56,
        _ => 32,
    }
}

/// The quality presets. Each one sets the graphics settings at once.
pub const PRESETS: &[(&str, f64, &str, f64, &str)] = &[
    ("Low", 0.5, "off", 200.0, "low"),
    ("Medium", 0.75, "low", 300.0, "medium"),
    ("High", 1.0, "high", 430.0, "high"),
    ("Ultra", 1.0, "ultra", 600.0, "high"),
];

impl Settings {
    pub fn apply_preset(&mut self, name: &str) {
        if let Some(p) = PRESETS.iter().find(|p| p.0 == name) {
            self.resolution = p.1;
            self.shadows = p.2.into();
            self.view_distance = p.3;
            self.effects = p.4.into();
            self.quality = name.into();
        }
    }

    /// The preset these settings match, if any.
    pub fn preset(&self) -> Option<&'static str> {
        PRESETS
            .iter()
            .find(|p| p.1 == self.resolution && p.2 == self.shadows && p.3 == self.view_distance && p.4 == self.effects)
            .map(|p| p.0)
    }

    pub fn is_mph(&self) -> bool {
        self.units == "mph"
    }
}

/// Shadow map size, and how many meters around the car get shadows.
pub fn shadow_size(setting: &str) -> (u32, f64) {
    match setting {
        "low" => (1024, 35.0),
        "ultra" => (4096, 70.0),
        _ => (2048, 45.0),
    }
}

/// How many particles and pieces of debris each effects level makes, compared to High.
pub fn effects_amount(setting: &str) -> f64 {
    match setting {
        "low" => 0.35,
        "medium" => 0.65,
        _ => 1.0,
    }
}

const STORAGE_KEY: &str = "carGame.settings.v1";

pub fn load_settings(storage: &Storage) -> Settings {
    let defaults = Settings::default();
    let mut merged = serde_json::to_value(&defaults).expect("JSON");
    if let (Some(saved), Some(target)) = (storage.get_json(STORAGE_KEY), merged.as_object_mut()) {
        for (key, value) in saved.as_object().into_iter().flatten() {
            // Only keep saved values of the right type, so a broken save can't break the game.
            let same_type = matches!(
                (target.get(key), value),
                (Some(Value::Number(_)), Value::Number(_))
                    | (Some(Value::Bool(_)), Value::Bool(_))
                    | (Some(Value::String(_)), Value::String(_))
            );
            if same_type {
                target.insert(key.clone(), value.clone());
            }
        }
    }
    let mut s: Settings = serde_json::from_value(merged).unwrap_or_default();
    let clamp = |v: f64, min: f64, max: f64| if v == 0.0 || v.is_nan() { min } else { v.clamp(min, max) };
    s.resolution = clamp(s.resolution, 0.5, 1.0);
    s.view_distance = clamp(s.view_distance, 150.0, 600.0);
    s.fov = clamp(s.fov, 50.0, 90.0);
    if ![0.0, 30.0, 60.0, 120.0].contains(&s.fps_limit) {
        s.fps_limit = 0.0;
    }
    if !["off", "low", "high", "ultra"].contains(&s.shadows.as_str()) {
        s.shadows = defaults.shadows.clone();
    }
    if !["low", "medium", "high"].contains(&s.effects.as_str()) {
        s.effects = defaults.effects.clone();
    }
    if !["mph", "kmh"].contains(&s.units.as_str()) {
        s.units = defaults.units.clone();
    }
    if !MAP_NAMES.contains(&s.map.as_str()) {
        s.map = defaults.map.clone();
    }
    if !["auto", "manual"].contains(&s.transmission.as_str()) {
        s.transmission = defaults.transmission.clone();
    }
    if !["off", "light", "normal", "busy"].contains(&s.traffic.as_str()) {
        s.traffic = defaults.traffic.clone();
    }
    s
}

pub fn save_settings(storage: &mut Storage, settings: &Settings) {
    storage.set_json(STORAGE_KEY, settings);
}
