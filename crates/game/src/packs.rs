//! Packs: optional parts of the game (Real Physics, Photo Real, Living City).
//! Each pack is a Bevy plugin that registers itself here when the game
//! starts. Packs can be switched on and off in the garage's Packs tab, and
//! show their own options there.
use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::egui::{self, RichText};
use serde_json::{Value, json};

use crate::storage::Storage;

/// What a pack is, for the packs screen.
#[derive(Clone, Debug)]
pub struct PackInfo {
    /// Short name used in saved settings, like "physics".
    pub id: &'static str,
    /// Name shown in the game, like "Real Physics".
    pub name: &'static str,
    pub icon: &'static str,
    /// One line about what the pack does.
    pub about: &'static str,
}

/// One of a pack's options on the Packs tab.
#[derive(Clone, Debug)]
pub enum PackOption {
    Heading(String),
    Hint(String),
    /// key, label, (value, label) choices, default
    Choice(&'static str, &'static str, Vec<(String, String)>, String),
    /// key, label, min, max, step, default
    Slider(&'static str, &'static str, f64, f64, f64, f64),
    /// key, label: a button the pack reacts to.
    Button(&'static str, &'static str),
}

#[derive(Resource, Default)]
pub struct Packs {
    pub list: Vec<PackInfo>,
    /// Packs switched on (packs start off, as in 1.2.x where each was an extra install).
    pub on: Vec<String>,
    /// Each pack's options, set by the pack (it can change them, say to show progress).
    pub options: HashMap<&'static str, Vec<PackOption>>,
    /// Option values: (pack, key) -> value.
    pub values: HashMap<(String, String), Value>,
    /// Buttons pressed since the pack last looked: (pack, key).
    pub pressed: Vec<(String, String)>,
    /// Something to save.
    pub dirty: bool,
}

/// Which packs are on (1.2.x kept a different list under "carGame.packs.v1").
const STATE_KEY: &str = "carGame.packs.v2";

impl Packs {
    pub fn is_on(&self, id: &str) -> bool {
        self.list.iter().any(|p| p.id == id) && self.on.iter().any(|o| o == id)
    }

    pub fn set_on(&mut self, id: &str, on: bool) {
        self.on.retain(|o| o != id);
        if on {
            self.on.push(id.to_string());
        }
        self.dirty = true;
    }

    pub fn value(&self, pack: &str, key: &str) -> Option<&Value> {
        self.values.get(&(pack.to_string(), key.to_string()))
    }

    pub fn string(&self, pack: &str, key: &str, fallback: &str) -> String {
        self.value(pack, key)
            .and_then(Value::as_str)
            .map_or(fallback.to_string(), str::to_string)
    }

    pub fn number(&self, pack: &str, key: &str, fallback: f64) -> f64 {
        self.value(pack, key).and_then(Value::as_f64).unwrap_or(fallback)
    }

    pub fn set(&mut self, pack: &str, key: &str, value: Value) {
        self.values.insert((pack.to_string(), key.to_string()), value);
        self.dirty = true;
    }

    /// Takes a button press, if there was one.
    pub fn take_press(&mut self, pack: &str, key: &str) -> bool {
        let before = self.pressed.len();
        self.pressed.retain(|(p, k)| !(p == pack && k == key));
        self.pressed.len() != before
    }

    /// Reads the on/off state and the pack's saved option values.
    pub fn load(&mut self, storage: &Storage) {
        if let Some(state) = storage.get_json(STATE_KEY) {
            self.on = state
                .get("on")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        for key in storage.keys() {
            if let Some(rest) = key.strip_prefix("carGame.pack.")
                && let Some((pack, option)) = rest.split_once('.')
                && let Some(v) = storage.get_json(key)
            {
                self.values.insert((pack.to_string(), option.to_string()), v);
            }
        }
    }

    pub fn save(&mut self, storage: &mut Storage) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        storage.set_json(STATE_KEY, &json!({ "on": self.on }));
        for ((pack, key), v) in &self.values {
            storage.set_json(&format!("carGame.pack.{pack}.{key}"), v);
        }
    }
}

/// Lets a pack add itself to the list.
pub trait RegisterPack {
    fn register_pack(&mut self, info: PackInfo) -> &mut Self;
}

impl RegisterPack for App {
    fn register_pack(&mut self, info: PackInfo) -> &mut Self {
        self.world_mut().get_resource_or_init::<Packs>().list.push(info);
        self
    }
}

/// The Packs tab in the garage. True if something changed.
pub fn packs_tab(ui: &mut egui::Ui, packs: &mut Packs) -> bool {
    let muted = crate::menu::MUTED;
    let mut changed = false;
    if packs.list.is_empty() {
        ui.label(RichText::new("No packs are in this copy of the game.").color(muted));
        return false;
    }
    ui.label(
        RichText::new("Packs add to the game. Switch one off to try the game without it.")
            .size(12.0)
            .color(muted),
    );
    for pack in packs.list.clone() {
        ui.add_space(8.0);
        ui.label(RichText::new(format!("{} {}", pack.icon, pack.name)).strong().size(17.0));
        ui.label(RichText::new(pack.about).size(12.0).color(muted));
        let mut on = packs.is_on(pack.id);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Pack").size(13.0).color(muted));
            for (v, l) in [(true, "On"), (false, "Off")] {
                if ui.selectable_label(on == v, l).clicked() && on != v {
                    on = v;
                    packs.set_on(pack.id, v);
                    changed = true;
                }
            }
        });
        if !on {
            continue;
        }
        for option in packs.options.get(pack.id).cloned().unwrap_or_default() {
            match option {
                PackOption::Heading(t) => {
                    ui.label(RichText::new(t).strong());
                }
                PackOption::Hint(t) => {
                    ui.label(RichText::new(t).size(12.0).color(muted));
                }
                PackOption::Choice(key, label, choices, default) => {
                    let current = packs.string(pack.id, key, &default);
                    ui.label(RichText::new(label).size(13.0).color(muted));
                    ui.horizontal_wrapped(|ui| {
                        for (v, l) in &choices {
                            if ui.selectable_label(&current == v, l).clicked() && &current != v {
                                packs.set(pack.id, key, json!(v));
                                changed = true;
                            }
                        }
                    });
                }
                PackOption::Slider(key, label, min, max, step, default) => {
                    let mut v = packs.number(pack.id, key, default);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(label).size(13.0).color(muted));
                        if ui.add(egui::Slider::new(&mut v, min..=max).step_by(step)).changed() {
                            packs.set(pack.id, key, json!(v));
                            changed = true;
                        }
                    });
                }
                PackOption::Button(key, label) => {
                    if crate::menu::button(ui, label, false, true) {
                        packs.pressed.push((pack.id.to_string(), key.to_string()));
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}
