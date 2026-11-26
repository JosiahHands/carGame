//! The game's saves: named entries of JSON text, like the browser storage the
//! 1.2.x game used (with the same names, so old progress can be brought over
//! unchanged). Kept in one file in the player's data folder, written safely:
//! to a new file first, which then replaces the old one.
use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy::prelude::Resource;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Resource, Default)]
pub struct Storage {
    entries: BTreeMap<String, String>,
    file: Option<PathBuf>,
    /// Set by `set`; the game writes the file at most a few times a second.
    pub dirty: bool,
}

/// The folder for the game's saves, packs' data and downloaded AI models.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CAR_GAME_DATA") {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("io.github", "JosiahHands", "Car Game").map(|d| d.data_dir().to_path_buf())
}

impl Storage {
    /// Opens the saves (none yet is fine).
    pub fn open() -> Self {
        let file = data_dir().map(|d| d.join("saves.json"));
        let entries = file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Storage {
            entries,
            file,
            dirty: false,
        }
    }

    /// Saves kept only in memory (tests, or no data folder).
    pub fn in_memory() -> Self {
        Storage::default()
    }

    pub fn is_new(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if self.entries.get(key) != Some(&value) {
            self.entries.insert(key.to_string(), value);
            self.dirty = true;
        }
    }

    pub fn remove(&mut self, key: &str) {
        if self.entries.remove(key).is_some() {
            self.dirty = true;
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    pub fn get_json(&self, key: &str) -> Option<Value> {
        self.get(key).and_then(|t| serde_json::from_str(t).ok())
    }

    pub fn get_as<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get(key).and_then(|t| serde_json::from_str(t).ok())
    }

    pub fn set_json<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) {
        if let Ok(text) = serde_json::to_string(value) {
            self.set(key, text);
        }
    }

    /// Writes the saves to disk now. Saving is a convenience: failures are only logged.
    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let Some(file) = &self.file else { return };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let text = serde_json::to_string_pretty(&self.entries).map_err(std::io::Error::other)?;
            let temp = file.with_extension("json.new");
            std::fs::write(&temp, text)?;
            std::fs::rename(&temp, file)
        };
        if let Err(e) = write() {
            bevy::log::warn!("Couldn't save the game: {e}");
        }
    }
}
