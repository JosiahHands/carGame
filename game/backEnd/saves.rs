//! Everything that is kept between games: the save file, the settings, the
//! car builds, bringing progress over from the old 1.2.x app, and updating
//! the game to a new version.

pub mod storage {
    //! The game's saves: named entries of JSON text, like the browser storage the
    //! 1.2.x game used (with the same names, so old progress can be brought over
    //! unchanged). Kept in one file in the player's data folder, written safely:
    //! to a new file first, which then replaces the old one.
    use std::collections::BTreeMap;
    use std::fs::OpenOptions;
    use std::io::{self, ErrorKind, Write};
    use std::path::{Path, PathBuf};

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
            Self::open_file(data_dir().map(|d| d.join("saves.json")))
        }

        /// Preserves damaged saves before allowing the game to write a new save.
        fn open_file(file: Option<PathBuf>) -> Self {
            let mut storage = Self { file, ..Self::default() };
            let Some(path) = storage.file.as_deref() else { return storage };
            match std::fs::read(path) {
                Ok(bytes) => match serde_json::from_slice(&bytes) {
                    Ok(entries) => storage.entries = entries,
                    Err(error) => match backup_damaged_save(path, &bytes) {
                        Ok(backup) => bevy::log::warn!("Couldn't parse the saves ({error}). Backed them up to {}", backup.display()),
                        Err(e) => {
                            bevy::log::warn!("Couldn't parse or back up the saves ({error}, {e}). Saves are disabled for this session.");
                            storage.file = None;
                        }
                    },
                },
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => {
                    bevy::log::warn!("Couldn't read the saves ({e}). Saves are disabled for this session.");
                    storage.file = None;
                }
            }
            storage
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

    fn backup_damaged_save(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let backup = path.with_file_name(format!("saves-unreadable-{time}-{}.json", std::process::id()));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&backup)?;
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(&backup);
            return Err(error);
        }
        Ok(backup)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn damaged_save_is_preserved_byte_for_byte() {
            let dir = std::env::temp_dir().join(format!("car-game-damaged-save-test-{}", std::process::id()));
            std::fs::create_dir(&dir).unwrap();
            let file = dir.join("saves.json");
            let damaged = [b"{\"carGame.career.v1\": \"\xff".as_slice(), b"{\"missing-end\":".as_slice()];
            for bytes in damaged {
                std::fs::write(&file, bytes).unwrap();
                let mut storage = Storage::open_file(Some(file.clone()));
                assert!(storage.is_new());
                storage.set("carGame.muted", "1");
                storage.flush();
            }

            let kept: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap())
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("saves-unreadable-"))
                .map(|entry| std::fs::read(entry.path()).unwrap())
                .collect();
            assert_eq!(kept.len(), damaged.len());
            for bytes in damaged {
                assert!(kept.iter().any(|copy| copy == bytes));
            }
            assert_eq!(Storage::open_file(Some(file)).get("carGame.muted"), Some("1"));
            std::fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn io_failure_leaves_the_save_path_alone() {
            let dir = std::env::temp_dir().join(format!("car-game-save-io-test-{}", std::process::id()));
            let path = dir.join("saves.json");
            std::fs::create_dir_all(&path).unwrap();
            let marker = path.join("keep");
            std::fs::write(&marker, b"unchanged").unwrap();

            let mut storage = Storage::open_file(Some(path));
            storage.set("carGame.muted", "1");
            storage.flush();

            let actual = std::fs::read(marker);
            std::fs::remove_dir_all(&dir).unwrap();
            assert_eq!(actual.unwrap(), b"unchanged");
        }
    }
}

pub mod settings {
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
        /// New in 1.3.2: check for a new version each time the game starts and
        /// install it. Off unless the player turns it on (Garage → Updates).
        #[serde(default)]
        pub auto_update: bool,
        /// New in 1.4.0: each map's own options (Garage → Maps), by map name.
        #[serde(default)]
        pub maps: std::collections::BTreeMap<String, MapOptions>,
        /// New in 1.4.0: keys changed on the Controls tab (control id -> key ids).
        #[serde(default)]
        pub controls: std::collections::BTreeMap<String, Vec<String>>,
    }

    /// A map's own options, on the Maps tab.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(default, rename_all = "camelCase")]
    pub struct MapOptions {
        /// Start the map at night.
        pub night: bool,
        /// "auto" (the venue's own) | "clear" | "rain" | "snow"
        pub weather: String,
        /// City: past the made map, more land is made as you drive, forever.
        pub generative: bool,
        /// City: start where you left off (otherwise at the start).
        pub resume: bool,
        /// City: you can fight people on foot.
        pub fighting: bool,
    }

    impl Default for MapOptions {
        fn default() -> Self {
            MapOptions {
                night: false,
                weather: "auto".into(),
                generative: false,
                resume: true,
                fighting: true,
            }
        }
    }

    impl MapOptions {
        /// The weather this picks, if not the venue's own.
        pub fn weather_choice(&self) -> Option<crate::venues::Weather> {
            use crate::venues::Weather;
            match self.weather.as_str() {
                "clear" => Some(Weather::Clear),
                "rain" => Some(Weather::Rain),
                "snow" => Some(Weather::Snow),
                _ => None,
            }
        }
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
                auto_update: false,
                maps: std::collections::BTreeMap::new(),
                controls: std::collections::BTreeMap::new(),
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
                self.antialias = name != "Low";
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

        /// A map's own options (the defaults until changed).
        pub fn map_options(&self, map: &str) -> MapOptions {
            self.maps.get(map).cloned().unwrap_or_default()
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
        let saved = storage.get_json(STORAGE_KEY);
        let mut maps = std::collections::BTreeMap::new();
        // Each map's options are read on their own, so one broken entry can't spoil the rest.
        if let Some(entries) = saved.as_ref().and_then(|s| s.get("maps")).and_then(Value::as_object) {
            for (map, options) in entries {
                if MAP_NAMES.contains(&map.as_str())
                    && let Ok(o) = serde_json::from_value::<MapOptions>(options.clone())
                {
                    maps.insert(map.clone(), o);
                }
            }
        }
        if let (Some(saved), Some(target)) = (saved, merged.as_object_mut()) {
            for (key, value) in saved.as_object().into_iter().flatten() {
                if key == "maps" || key == "controls" {
                    continue;
                }
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
        for o in maps.values_mut() {
            if !["auto", "clear", "rain", "snow"].contains(&o.weather.as_str()) {
                o.weather = "auto".into();
            }
        }
        s.maps = maps;
        // Changed keys: lists of key names (unknown ones are dropped when used).
        if let Some(entries) = storage
            .get_json(STORAGE_KEY)
            .and_then(|v| v.get("controls").cloned())
            .and_then(|v| v.as_object().cloned())
        {
            for (id, keys) in entries {
                if let Some(list) = keys.as_array() {
                    s.controls
                        .insert(id, list.iter().filter_map(Value::as_str).map(str::to_string).collect());
                }
            }
        }
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn each_maps_options_are_kept_and_a_broken_one_is_dropped() {
            let mut storage = Storage::in_memory();
            storage.set(
                STORAGE_KEY,
                r#"{"map":"city","maps":{"city":{"generative":true,"weather":"rain"},"oval":{"night":"yes"},"nowhere":{"night":true},"canyon":{"weather":"hail"}}}"#,
            );
            let s = load_settings(&storage);
            let city = s.map_options("city");
            assert!(city.generative && city.resume && city.fighting);
            assert_eq!(city.weather_choice(), Some(crate::venues::Weather::Rain));
            assert_eq!(s.map_options("oval"), MapOptions::default());
            assert!(!s.maps.contains_key("nowhere"));
            assert_eq!(s.map_options("canyon").weather, "auto");
            let mut again = Storage::in_memory();
            save_settings(&mut again, &s);
            assert_eq!(load_settings(&again).maps, s.maps);
        }
    }
}

pub mod config {
    //! Everything the player can customize about their car, plus saving, loading,
    //! share codes, saved builds and the ready-made showcase builds.
    //!
    //! Saved as the same JSON the 1.2.x game used, so old saves load unchanged.
    use base64::Engine;
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value, json};

    use crate::rng;
    use crate::storage::Storage;

    /// A list of choices for a text setting: (value, label).
    pub type Options = &'static [(&'static str, &'static str)];

    pub const BODY_OPTIONS: Options = &[
        ("sports", "Sports"),
        ("muscle", "Muscle"),
        ("hatch", "Hatchback"),
        ("pickup", "Pickup"),
        ("van", "Van"),
        ("wedge", "Supercar"),
    ];
    pub const FINISH_OPTIONS: Options = &[
        ("gloss", "Gloss"),
        ("metallic", "Metallic"),
        ("matte", "Matte"),
        ("pearl", "Pearl"),
        ("chrome", "Chrome"),
        ("carbon", "Carbon"),
        ("glow", "Glow"),
        ("rainbow", "Rainbow"),
    ];
    pub const PATTERN_OPTIONS: Options = &[
        ("none", "None"),
        ("twoTone", "Two-tone"),
        ("flames", "Flames"),
        ("checker", "Checker"),
        ("camo", "Camo"),
        ("tiger", "Tiger"),
        ("dots", "Dots"),
    ];
    pub const STRIPE_OPTIONS: Options = &[("none", "None"), ("racing", "Racing"), ("side", "Side")];
    pub const WHEEL_OPTIONS: Options = &[
        ("fiveSpoke", "5-spoke"),
        ("multiSpoke", "Mesh"),
        ("dish", "Dish"),
        ("offroad", "Off-road"),
    ];
    pub const SPOILER_OPTIONS: Options = &[
        ("none", "None"),
        ("lip", "Lip"),
        ("wing", "Wing"),
        ("gt", "GT wing"),
        ("drag", "Dragster"),
    ];
    pub const PART_COLOR_OPTIONS: Options = &[("body", "Body"), ("accent", "Accent"), ("black", "Black"), ("carbon", "Carbon")];
    pub const EXHAUST_OPTIONS: Options = &[("single", "Single"), ("dual", "Dual"), ("quad", "Quad"), ("stacks", "Truck stacks")];
    pub const TOPPER_OPTIONS: Options = &[
        ("none", "None"),
        ("crown", "Crown"),
        ("topHat", "Top hat"),
        ("cone", "Traffic cone"),
        ("horns", "Horns"),
        ("sharkFin", "Shark fin"),
        ("halo", "Halo"),
        ("duck", "Rubber duck"),
    ];
    pub const ANTENNA_OPTIONS: Options = &[("none", "None"), ("whip", "Whip"), ("flag", "Flag"), ("ball", "Ball")];
    pub const UNDERGLOW_OPTIONS: Options = &[("off", "Off"), ("on", "On"), ("rainbow", "Rainbow")];
    pub const TRAIL_OPTIONS: Options = &[
        ("flames", "Flames"),
        ("rainbow", "Rainbow"),
        ("sparkles", "Sparkles"),
        ("smoke", "Smoke"),
    ];
    pub const HORN_OPTIONS: Options = &[
        ("classic", "Classic"),
        ("truck", "Air horn"),
        ("train", "Train"),
        ("clown", "Clown"),
        ("tune", "Tune"),
    ];
    pub const ENGINE_OPTIONS: Options = &[
        ("v8", "V8"),
        ("v12", "V12"),
        ("diesel", "Diesel"),
        ("electric", "Electric"),
        ("jet", "Jet"),
        ("moped", "Moped"),
    ];

    pub const PAINT_SWATCHES: &[&str] = &[
        "#d7263d", "#f46036", "#ffd23f", "#3bb273", "#1b98e0", "#2e294e", "#7b2cbf", "#ff70a6", "#f2f2f2", "#9aa0a6", "#2b2d42", "#0b0b0b",
    ];
    pub const LIGHT_SWATCHES: &[&str] = &["#fff6e0", "#ffffff", "#cfe6ff", "#ffd27a", "#7af0ff", "#c58bff"];

    /// How the car drives. Real units where there are some.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Tuning {
        pub horsepower: f64,
        /// mph
        pub top_speed: f64,
        pub grip: f64,
        pub brakes: f64,
        /// Degrees of steering lock.
        pub steering: f64,
        pub drift: f64,
        pub downforce: f64,
        pub nitro_power: f64,
        pub nitro_seconds: f64,
        pub infinite_nitro: bool,
        /// Meters (0 = no jumping).
        pub jump: f64,
        pub air_control: f64,
        pub gravity: f64,
        pub bounce: f64,
    }

    /// Which roof weapons the car has right now (set by the game, never saved).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
    pub struct Armed {
        pub gun: bool,
        pub rockets: bool,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CarConfig {
        pub body: String,
        pub size: f64,
        pub length: f64,
        pub width: f64,
        pub height: f64,
        pub plate: String,
        pub race_number: f64,
        pub paint_color: String,
        pub finish: String,
        pub accent_color: String,
        pub pattern: String,
        pub stripes: String,
        pub window_tint: f64,
        pub wheel_style: String,
        pub rim_color: String,
        pub tire_color: String,
        pub wheel_size: f64,
        pub tire_width: f64,
        pub track: f64,
        pub camber: f64,
        pub lift: f64,
        pub spoiler: String,
        pub spoiler_color: String,
        pub exhaust: String,
        pub topper: String,
        pub antenna: String,
        pub bull_bar: bool,
        pub light_bar: bool,
        pub roof_guns: bool,
        #[serde(skip)]
        pub armed: Armed,
        pub headlight_color: String,
        pub underglow: String,
        pub underglow_color: String,
        pub nitro_color: String,
        pub boost_trail: String,
        pub tire_smoke: bool,
        pub tire_smoke_color: String,
        pub horn: String,
        pub engine_sound: String,
        pub tuning: Tuning,
    }

    impl Default for CarConfig {
        fn default() -> Self {
            CarConfig {
                body: "sports".into(),
                size: 1.0,
                length: 1.0,
                width: 1.0,
                height: 1.0,
                plate: "DRIVE".into(),
                race_number: 0.0,
                paint_color: "#d7263d".into(),
                finish: "metallic".into(),
                accent_color: "#f2f2f2".into(),
                pattern: "none".into(),
                stripes: "racing".into(),
                window_tint: 0.6,
                wheel_style: "fiveSpoke".into(),
                rim_color: "#c9ccd1".into(),
                tire_color: "#1a1a1a".into(),
                wheel_size: 1.0,
                tire_width: 1.0,
                track: 1.0,
                camber: 0.0,
                lift: 0.0,
                spoiler: "wing".into(),
                spoiler_color: "carbon".into(),
                exhaust: "dual".into(),
                topper: "none".into(),
                antenna: "none".into(),
                bull_bar: false,
                light_bar: false,
                roof_guns: true,
                armed: Armed::default(),
                headlight_color: "#fff6e0".into(),
                underglow: "off".into(),
                underglow_color: "#7af0ff".into(),
                nitro_color: "#4fb3ff".into(),
                boost_trail: "flames".into(),
                tire_smoke: true,
                tire_smoke_color: "#e8e8e8".into(),
                horn: "classic".into(),
                engine_sound: "v8".into(),
                tuning: Tuning {
                    horsepower: 400.0,
                    top_speed: 110.0,
                    grip: 1.0,
                    brakes: 1.0,
                    steering: 30.0,
                    drift: 5.0,
                    downforce: 0.0,
                    nitro_power: 1.0,
                    nitro_seconds: 3.5,
                    infinite_nitro: false,
                    jump: 0.0,
                    air_control: 0.15,
                    gravity: 1.0,
                    bounce: 0.35,
                },
            }
        }
    }

    /// The allowed range of every number setting. The garage sliders use these too.
    pub fn range(key: &str) -> (f64, f64) {
        match key {
            "size" => (0.25, 25.0),
            "length" => (0.5, 3.0),
            "width" => (0.6, 2.5),
            "height" => (0.5, 3.0),
            "raceNumber" => (0.0, 99.0),
            "windowTint" => (0.0, 1.0),
            "wheelSize" => (0.5, 4.0),
            "tireWidth" => (0.5, 3.0),
            "track" => (0.8, 2.0),
            "camber" => (-20.0, 20.0),
            "lift" => (-0.08, 3.0),
            "horsepower" => (50.0, 10000.0),
            "topSpeed" => (20.0, 1000.0),
            "grip" => (0.2, 3.0),
            "brakes" => (0.3, 5.0),
            "steering" => (10.0, 60.0),
            "drift" => (1.0, 10.0),
            "downforce" => (0.0, 10.0),
            "nitroPower" => (0.5, 5.0),
            "nitroSeconds" => (1.0, 30.0),
            "jump" => (0.0, 30.0),
            "airControl" => (0.0, 1.0),
            "gravity" => (0.1, 3.0),
            "bounce" => (0.0, 1.5),
            _ => (f64::MIN, f64::MAX),
        }
    }

    fn choices(key: &str) -> Option<Options> {
        Some(match key {
            "body" => BODY_OPTIONS,
            "finish" => FINISH_OPTIONS,
            "pattern" => PATTERN_OPTIONS,
            "stripes" => STRIPE_OPTIONS,
            "wheelStyle" => WHEEL_OPTIONS,
            "spoiler" => SPOILER_OPTIONS,
            "spoilerColor" => PART_COLOR_OPTIONS,
            "exhaust" => EXHAUST_OPTIONS,
            "topper" => TOPPER_OPTIONS,
            "antenna" => ANTENNA_OPTIONS,
            "underglow" => UNDERGLOW_OPTIONS,
            "boostTrail" => TRAIL_OPTIONS,
            "horn" => HORN_OPTIONS,
            "engineSound" => ENGINE_OPTIONS,
            _ => return None,
        })
    }

    /// The label for a choice.
    pub fn label(options: Options, value: &str) -> &'static str {
        options.iter().find(|o| o.0 == value).map_or("?", |o| o.1)
    }

    pub fn clean_plate(text: &str) -> String {
        text.to_uppercase()
            .chars()
            .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == ' ' || *c == '-')
            .take(8)
            .collect()
    }

    fn is_hex_color(s: &str) -> bool {
        s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Copies each setting from `raw` onto the defaults if it is valid: numbers
    /// kept in range, choices must be known ones and colors must be #rrggbb.
    fn clean_fields(raw: &Value, defaults: &Value) -> Value {
        let mut result = defaults.as_object().cloned().unwrap_or_default();
        let empty = Map::new();
        let r = raw.as_object().unwrap_or(&empty);
        for (key, fallback) in defaults.as_object().unwrap_or(&empty) {
            let Some(value) = r.get(key) else { continue };
            match (fallback, value) {
                (Value::Number(_), Value::Number(n)) => {
                    if let Some(v) = n.as_f64().filter(|v| v.is_finite()) {
                        let (min, max) = range(key);
                        result.insert(key.clone(), json!(v.clamp(min, max)));
                    }
                }
                (Value::Bool(_), Value::Bool(b)) => {
                    result.insert(key.clone(), json!(b));
                }
                (Value::String(_), Value::String(s)) => {
                    if key == "plate" {
                        result.insert(key.clone(), json!(clean_plate(s)));
                    } else if let Some(options) = choices(key) {
                        if options.iter().any(|o| o.0 == s) {
                            result.insert(key.clone(), json!(s));
                        }
                    } else if is_hex_color(s) {
                        result.insert(key.clone(), json!(s.to_lowercase()));
                    }
                }
                _ => {}
            }
        }
        Value::Object(result)
    }

    /// Older saves scored the tuning from 1 to 10 and had a ride height from -1 to 1.
    fn upgrade_old_save(r: &mut Map<String, Value>) {
        let round2 = |n: f64| (n * 100.0).round() / 100.0;
        if let Some(ride) = r.get("rideHeight").and_then(Value::as_f64)
            && !r.contains_key("lift")
        {
            r.insert("lift".into(), json!(ride * 0.07));
        }
        let Some(old) = r.get("tuning").and_then(Value::as_object).cloned() else {
            return;
        };
        if !old.contains_key("power") || old.contains_key("horsepower") {
            return;
        }
        let score = |key: &str| {
            old.get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .map_or(5.0, |v| v.clamp(1.0, 10.0))
        };
        let normal = CarConfig::default().tuning;
        let accel = (5.0 + 0.9 * score("power")) / 9.5;
        r.insert(
            "tuning".into(),
            json!({
                "horsepower": (normal.horsepower * accel.powf(1.0 / 0.6)).round(),
                "topSpeed": ((normal.top_speed * (28.0 + 5.0 * score("topSpeed"))) / 53.0).round(),
                "grip": round2((2.5 + 0.9 * score("grip")) / 7.0),
                "brakes": round2((12.0 + 2.0 * score("brakes")) / 22.0),
                "steering": (((0.38 + 0.03 * score("steering")) * 180.0) / std::f64::consts::PI).round(),
                "drift": score("drift"),
                "nitroPower": round2((0.6 + 0.12 * score("nitro")) / 1.2),
                "nitroSeconds": round2((normal.nitro_seconds * 0.305) / (0.18 + 0.025 * score("nitro"))),
            }),
        );
    }

    /// Turns anything (old saves, share codes, hand-edited data) into a valid config.
    pub fn sanitize_config(raw: &Value) -> CarConfig {
        let mut r = raw.as_object().cloned().unwrap_or_default();
        upgrade_old_save(&mut r);
        if let Some(Value::Bool(b)) = r.get("underglow").cloned() {
            r.insert("underglow".into(), json!(if b { "on" } else { "off" }));
        }
        let defaults = serde_json::to_value(CarConfig::default()).expect("config to JSON");
        let mut config = clean_fields(&Value::Object(r.clone()), &defaults);
        let tuning = clean_fields(r.get("tuning").unwrap_or(&Value::Null), &defaults["tuning"]);
        config["tuning"] = tuning;
        serde_json::from_value(config).unwrap_or_default()
    }

    // ---------- saving ----------

    const STORAGE_KEY: &str = "carGame.config.v1";
    const BUILDS_KEY: &str = "carGame.builds.v1";

    pub fn load_config(storage: &Storage) -> CarConfig {
        storage.get_json(STORAGE_KEY).map(|v| sanitize_config(&v)).unwrap_or_default()
    }

    pub fn save_config(storage: &mut Storage, config: &CarConfig) {
        storage.set_json(STORAGE_KEY, config);
    }

    /// A car the player saved under a name, in the Builds tab.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct SavedBuild {
        pub name: String,
        pub config: CarConfig,
    }

    pub fn load_builds(storage: &Storage) -> Vec<SavedBuild> {
        let Some(Value::Array(list)) = storage.get_json(BUILDS_KEY) else {
            return Vec::new();
        };
        list.iter()
            .map(|b| SavedBuild {
                name: b.get("name").and_then(Value::as_str).unwrap_or("My car").chars().take(30).collect(),
                config: sanitize_config(b.get("config").unwrap_or(&Value::Null)),
            })
            .collect()
    }

    pub fn save_builds(storage: &mut Storage, builds: &[SavedBuild]) {
        storage.set_json(BUILDS_KEY, &builds);
    }

    // ---------- share codes ----------
    // A share code is the car's settings that differ from the default car, as
    // JSON, in base64, after "CAR1-".

    pub fn share_code(config: &CarConfig) -> String {
        let defaults = serde_json::to_value(CarConfig::default()).expect("JSON");
        let all = serde_json::to_value(config).expect("JSON");
        let mut changed = Map::new();
        for (key, value) in all.as_object().into_iter().flatten() {
            if key != "tuning" && defaults.get(key) != Some(value) {
                changed.insert(key.clone(), value.clone());
            }
        }
        let mut tuning = Map::new();
        for (key, value) in all["tuning"].as_object().into_iter().flatten() {
            if defaults["tuning"].get(key) != Some(value) {
                tuning.insert(key.clone(), value.clone());
            }
        }
        if !tuning.is_empty() {
            changed.insert("tuning".into(), Value::Object(tuning));
        }
        format!(
            "CAR1-{}",
            base64::engine::general_purpose::STANDARD.encode(Value::Object(changed).to_string())
        )
    }

    /// Reads a share code back into a car, or None if it isn't one.
    pub fn read_share_code(code: &str) -> Option<CarConfig> {
        let body = code.trim().strip_prefix("CAR1-")?;
        if body.is_empty() || !body.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=') {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD.decode(body).ok()?;
        let data: Value = serde_json::from_slice(&bytes).ok()?;
        let obj = data.as_object()?;
        let defaults = serde_json::to_value(CarConfig::default()).ok()?;
        if obj.keys().any(|k| defaults.get(k).is_none()) {
            return None;
        }
        Some(sanitize_config(&data))
    }

    // ---------- showcase builds ----------

    pub struct Showcase {
        pub name: &'static str,
        pub about: &'static str,
        pub changes: fn() -> Value,
    }

    /// Ready-made cars that show off what the garage can do.
    pub const SHOWCASE_BUILDS: &[Showcase] = &[
        Showcase {
            name: "Kaiju",
            about: "A 100 ft tall muscle car that flattens buildings.",
            changes: || {
                json!({"body":"muscle","size":22,"paintColor":"#2f4f2f","finish":"matte","pattern":"tiger","accentColor":"#0b0b0b","stripes":"none",
            "topper":"horns","underglow":"on","underglowColor":"#39ff88","horn":"train","engineSound":"diesel","exhaust":"stacks",
            "tuning":{"horsepower":3000,"topSpeed":140}})
            },
        },
        Showcase {
            name: "500 mph",
            about: "A low, long land-speed record car with a jet engine.",
            changes: || {
                json!({"body":"wedge","length":1.6,"width":0.9,"height":0.8,"paintColor":"#f2f2f2","finish":"gloss","pattern":"flames","accentColor":"#f46036",
            "stripes":"none","spoiler":"none","engineSound":"jet","raceNumber":1,"wheelStyle":"dish",
            "tuning":{"horsepower":10000,"topSpeed":500,"grip":2,"downforce":8,"steering":18,"nitroPower":2}})
            },
        },
        Showcase {
            name: "Monster truck",
            about: "Giant wheels, a big lift, and bouncy landings.",
            changes: || {
                json!({"body":"pickup","wheelSize":3.2,"tireWidth":2.2,"track":1.4,"lift":0.5,"wheelStyle":"offroad","rimColor":"#ffd23f",
            "paintColor":"#1b98e0","pattern":"flames","accentColor":"#ffd23f","stripes":"none","spoiler":"none","exhaust":"stacks",
            "bullBar":true,"lightBar":true,"horn":"truck","tuning":{"horsepower":900,"topSpeed":90,"jump":4,"bounce":0.8,"grip":1.3}})
            },
        },
        Showcase {
            name: "Pocket rocket",
            about: "A toy-sized car with a rubber duck on the roof.",
            changes: || {
                json!({"body":"hatch","size":0.3,"paintColor":"#ffd23f","pattern":"dots","accentColor":"#1b98e0","stripes":"none","topper":"duck",
            "antenna":"ball","horn":"clown","engineSound":"moped","roofGuns":false,"tuning":{"horsepower":120,"topSpeed":45,"grip":1.5}})
            },
        },
        Showcase {
            name: "Moon buggy",
            about: "Moon gravity, huge jumps and full control in the air.",
            changes: || {
                json!({"body":"hatch","lift":0.4,"wheelSize":1.6,"tireWidth":1.5,"wheelStyle":"offroad","paintColor":"#f2f2f2","finish":"matte",
            "stripes":"side","accentColor":"#1b98e0","antenna":"flag","engineSound":"electric","spoiler":"none",
            "tuning":{"gravity":0.17,"jump":6,"airControl":0.9,"bounce":0.9}})
            },
        },
        Showcase {
            name: "Stretch limo",
            about: "Three times longer, with blacked-out windows.",
            changes: || {
                json!({"body":"muscle","length":2.8,"paintColor":"#0b0b0b","finish":"gloss","stripes":"none","windowTint":1,"rimColor":"#f2f2f2",
            "wheelStyle":"multiSpoke","spoiler":"none","antenna":"whip","topper":"topHat","horn":"tune","engineSound":"v12","roofGuns":false})
            },
        },
        Showcase {
            name: "Aerial ace",
            about: "Jump, boost and fly: steer and boost freely in the air.",
            changes: || {
                json!({"body":"wedge","paintColor":"#2f6bff","finish":"pearl","accentColor":"#ff70a6","stripes":"racing","topper":"halo",
            "underglow":"rainbow","boostTrail":"rainbow","roofGuns":false,
            "tuning":{"jump":3,"airControl":1,"nitroPower":2.5,"nitroSeconds":8,"horsepower":800}})
            },
        },
        Showcase {
            name: "Low rider",
            about: "Slammed, wide and tilted, with rainbow neon.",
            changes: || {
                json!({"body":"muscle","lift":-0.08,"camber":-14,"track":1.2,"wheelSize":0.9,"paintColor":"#7b2cbf","finish":"pearl","stripes":"none",
            "underglow":"rainbow","spoiler":"none","exhaust":"quad","horn":"tune","rimColor":"#d4af37","wheelStyle":"multiSpoke",
            "tireSmokeColor":"#ff70a6"})
            },
        },
        Showcase {
            name: "Drag racer",
            about: "A huge wing, 3,000 hp and almost no steering.",
            changes: || {
                json!({"body":"sports","spoiler":"drag","spoilerColor":"accent","paintColor":"#ffd23f","accentColor":"#0b0b0b","pattern":"checker",
            "stripes":"none","raceNumber":7,"tireWidth":1.6,"engineSound":"v12",
            "tuning":{"horsepower":3000,"topSpeed":260,"grip":1.8,"steering":14,"nitroPower":3}})
            },
        },
        Showcase {
            name: "Rainbow",
            about: "Everything that can be a rainbow, is.",
            changes: || {
                json!({"body":"sports","finish":"rainbow","underglow":"rainbow","boostTrail":"rainbow","topper":"crown","stripes":"none",
            "tireSmokeColor":"#ff70a6","rimColor":"#f2f2f2","horn":"clown"})
            },
        },
    ];

    /// Applies changes (like a showcase build's) on top of a config.
    pub fn with_changes(base: &CarConfig, changes: &Value) -> CarConfig {
        let mut v = serde_json::to_value(base).expect("JSON");
        if let (Some(target), Some(src)) = (v.as_object_mut(), changes.as_object()) {
            for (k, val) in src {
                if k == "tuning" {
                    if let (Some(t), Some(s)) = (target.get_mut("tuning").and_then(Value::as_object_mut), val.as_object()) {
                        for (tk, tv) in s {
                            t.insert(tk.clone(), tv.clone());
                        }
                    }
                } else {
                    target.insert(k.clone(), val.clone());
                }
            }
        }
        let mut out: CarConfig = serde_json::from_value(v).unwrap_or_else(|_| base.clone());
        out.armed = base.armed;
        out
    }

    /// The default car with a showcase build's changes on top.
    pub fn showcase_config(build: &Showcase) -> CarConfig {
        with_changes(&CarConfig::default(), &(build.changes)())
    }

    // ---------- random cars ----------

    pub fn random_color() -> String {
        let h = rng::random();
        let s = 0.5 + rng::random() * 0.5;
        let l = 0.25 + rng::random() * 0.45;
        hsl_hex(h, s, l)
    }

    /// HSL (0..1 each) as "#rrggbb".
    pub fn hsl_hex(h: f64, s: f64, l: f64) -> String {
        let f = |n: f64| {
            let k = (n + h * 12.0) % 12.0;
            let a = s * l.min(1.0 - l);
            let c = l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
            (c * 255.0).round() as u8
        };
        format!("#{:02x}{:02x}{:02x}", f(0.0), f(8.0), f(4.0))
    }

    fn pick_value(options: Options) -> String {
        rng::pick(options).0.to_string()
    }

    /// A new look for the car: body, paint, wheels and parts. Size and tuning are kept.
    pub fn random_config(current: &CarConfig) -> CarConfig {
        let sometimes = |chance: f64, value: String, otherwise: &str| if rng::random() < chance { value } else { otherwise.to_string() };
        let finishes: Vec<_> = FINISH_OPTIONS.iter().filter(|o| o.0 != "rainbow").copied().collect();
        let spoilers: Vec<_> = SPOILER_OPTIONS.iter().filter(|o| o.0 != "drag").copied().collect();
        let paint = |_: ()| {
            if rng::random() < 0.5 {
                rng::pick(PAINT_SWATCHES).to_string()
            } else {
                random_color()
            }
        };
        CarConfig {
            body: pick_value(BODY_OPTIONS),
            paint_color: paint(()),
            finish: rng::pick(&finishes).0.to_string(),
            accent_color: paint(()),
            pattern: sometimes(0.35, pick_value(PATTERN_OPTIONS), "none"),
            stripes: pick_value(STRIPE_OPTIONS),
            window_tint: (rng::random() * 10.0).round() / 10.0,
            wheel_style: pick_value(WHEEL_OPTIONS),
            rim_color: rng::pick(&[
                "#c9ccd1".to_string(),
                "#1a1a1a".into(),
                "#d4af37".into(),
                "#ffffff".into(),
                random_color(),
            ]),
            tire_color: sometimes(0.1, random_color(), "#1a1a1a"),
            wheel_size: 0.85 + (rng::random() * 8.0).round() * 0.05,
            tire_width: 0.8 + (rng::random() * 6.0).round() * 0.1,
            spoiler: rng::pick(&spoilers).0.to_string(),
            spoiler_color: pick_value(PART_COLOR_OPTIONS),
            exhaust: pick_value(EXHAUST_OPTIONS),
            topper: sometimes(0.15, pick_value(TOPPER_OPTIONS), "none"),
            antenna: sometimes(0.15, pick_value(ANTENNA_OPTIONS), "none"),
            race_number: if rng::random() < 0.2 {
                1.0 + (rng::random() * 99.0).floor()
            } else {
                0.0
            },
            headlight_color: rng::pick(LIGHT_SWATCHES).to_string(),
            underglow: sometimes(0.4, "on".into(), "off"),
            underglow_color: random_color(),
            nitro_color: random_color(),
            ..current.clone()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn share_codes_round_trip() {
            let car = showcase_config(&SHOWCASE_BUILDS[2]);
            let code = share_code(&car);
            assert_eq!(read_share_code(&code), Some(car));
            assert_eq!(read_share_code("CAR1-e30="), Some(CarConfig::default()));
            assert_eq!(read_share_code("CAR1-bnVsbA=="), None);
        }

        #[test]
        fn old_saves_are_upgraded() {
            let old = json!({"tuning": {"power": 5, "topSpeed": 5}, "rideHeight": 1, "underglow": true, "size": 99});
            let c = sanitize_config(&old);
            assert_eq!(c.tuning.horsepower, 400.0);
            assert_eq!(c.tuning.top_speed, 110.0);
            assert_eq!(c.underglow, "on");
            assert_eq!(c.size, 25.0);
            assert!((c.lift - 0.07).abs() < 1e-9);
        }
    }
}

pub mod import {
    //! Brings progress over from the old (1.2.x) app: career, garage builds,
    //! settings, best times, ghosts and the Living City's memories.
    //!
    //! The old app was built with Electron and kept everything in its browser
    //! storage (Chromium's LevelDB), in its folder under the system's app data.
    //! This only ever reads the old data: the folder is copied somewhere
    //! temporary first, and the copy is what gets opened.
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use rusty_leveldb::{DB, LdbIterator, Options};

    /// Where the old app may have kept its data on this system. The folder was
    /// named after the app ("Car Game") or its package ("car-game").
    pub fn old_app_folders() -> Vec<PathBuf> {
        let base = if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        };
        base.map(|b| vec![b.join("Car Game"), b.join("car-game")]).unwrap_or_default()
    }

    /// The old game's saves (name -> JSON text, like its browser storage), or
    /// None if there's no old game on this computer.
    pub fn read_old_saves() -> Option<BTreeMap<String, String>> {
        const CITY_KEY: &str = "carGame.pack.npc.city";
        let mut found = BTreeMap::new();
        let mut newest_city = None;
        for folder in old_app_folders() {
            let db = folder.join("Local Storage").join("leveldb");
            if db.is_dir()
                && let Ok(mut entries) = read_local_storage(&db)
            {
                if let Some(text) = entries.remove(CITY_KEY) {
                    pick_city(&mut newest_city, text);
                }
                found.extend(entries);
            }
            let npc = folder.join("packs/npc");
            // The backup predates the main file. Equal timestamps prefer the main file.
            for name in ["city.json.backup", "city.json"] {
                let file = npc.join(name);
                if file.metadata().is_ok_and(|m| m.len() <= 40 * 1024 * 1024)
                    && let Ok(text) = std::fs::read_to_string(file)
                {
                    pick_city(&mut newest_city, text);
                }
            }
            if let Ok(text) = std::fs::read_to_string(npc.join("ai.json"))
                && serde_json::from_str::<serde_json::Value>(&text).is_ok_and(|v| v.is_object())
            {
                found.insert("carGame.pack.npc.ai".into(), text);
            }
        }
        if let Some((_, text)) = newest_city {
            found.insert(CITY_KEY.into(), text);
        }
        if found.is_empty() { None } else { Some(found) }
    }

    fn pick_city(newest: &mut Option<(u64, String)>, text: String) {
        let Ok(city) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        if !city.is_object() {
            return;
        }
        let timestamp = city["savedAt"].as_u64().unwrap_or(0);
        if newest.as_ref().is_none_or(|(current, _)| timestamp >= *current) {
            *newest = Some((timestamp, text));
        }
    }

    /// Reads the game's entries from a copy of a Chromium "Local Storage/leveldb" folder.
    pub fn read_local_storage(dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
        let copy = std::env::temp_dir().join(format!("car-game-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&copy);
        std::fs::create_dir_all(&copy)?;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            // The lock file belongs to the old app, if it's open.
            if name != "LOCK" && entry.file_type()?.is_file() {
                std::fs::copy(entry.path(), copy.join(&name))?;
            }
        }
        let result = read_copy(&copy);
        let _ = std::fs::remove_dir_all(&copy);
        result
    }

    fn read_copy(dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
        let options = Options {
            create_if_missing: false,
            ..Options::default()
        };
        let mut db = DB::open(dir, options).map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut iter = db.new_iter().map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut out = BTreeMap::new();
        while iter.advance() {
            let Some((key, value)) = iter.current() else { continue };
            if let Some(name) = storage_key(&key)
                && name.starts_with("carGame.")
                && let Some(text) = decode(&value)
            {
                out.insert(name, text);
            }
        }
        Ok(out)
    }

    /// A Chromium local storage key is "_" + the page's origin + a zero byte +
    /// the name, which starts with a byte saying how it's written.
    fn storage_key(key: &[u8]) -> Option<String> {
        if key.first() != Some(&b'_') {
            return None;
        }
        let zero = key.iter().position(|b| *b == 0)?;
        decode(&key[zero + 1..])
    }

    /// Chromium writes strings as 0x01 + Latin-1, or 0x00 + UTF-16 (little-endian).
    fn decode(bytes: &[u8]) -> Option<String> {
        let (&kind, rest) = bytes.split_first()?;
        match kind {
            1 => Some(rest.iter().map(|b| *b as char).collect()),
            0 => {
                let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                String::from_utf16(&units).ok()
            }
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn looks_in_both_old_folder_names() {
            let folders = old_app_folders();
            assert_eq!(folders.len(), 2);
            assert!(folders[0].ends_with("Car Game") && folders[1].ends_with("car-game"));
        }

        #[test]
        fn chooses_newest_readable_city() {
            let mut city = None;
            pick_city(&mut city, r#"{"savedAt":42,"clock":1}"#.into());
            pick_city(&mut city, r#"{"savedAt":10,"clock":0}"#.into());
            pick_city(&mut city, r#"{"savedAt":100,"#.into());
            pick_city(&mut city, "null".into());
            assert_eq!(city, Some((42, r#"{"savedAt":42,"clock":1}"#.into())));
        }

        #[test]
        fn reads_chromium_strings() {
            assert_eq!(
                storage_key(b"_file://\x00\x01carGame.config.v1").as_deref(),
                Some("carGame.config.v1")
            );
            assert_eq!(decode(b"\x01{\"a\":1}").as_deref(), Some("{\"a\":1}"));
            assert_eq!(decode(&[0, b'h', 0, b'i', 0]).as_deref(), Some("hi"));
            assert_eq!(storage_key(b"META:file://"), None);
        }

        /// A database written the way Chromium writes it can be read back.
        #[test]
        fn reads_a_local_storage_database() {
            let dir = std::env::temp_dir().join(format!("car-game-import-test-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            {
                let mut db = DB::open(
                    &dir,
                    Options {
                        create_if_missing: true,
                        ..Options::default()
                    },
                )
                .unwrap();
                db.put(b"VERSION", b"1").unwrap();
                db.put(b"_file://\x00\x01carGame.career.v1", b"\x01{\"coins\":1234}").unwrap();
                db.put(b"_file://\x00\x01other.thing", b"\x01x").unwrap();
                db.flush().unwrap();
            }
            let saves = read_local_storage(&dir).unwrap();
            assert_eq!(saves.get("carGame.career.v1").map(String::as_str), Some("{\"coins\":1234}"));
            assert_eq!(saves.len(), 1);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

pub mod updates {
    //! Updating the game to a new version from its GitHub releases: checking
    //! whether there's a newer one, downloading it, putting it in place of this
    //! copy, then starting it once this one has closed. Garage → Updates does
    //! this when asked, or (if the player turns it on) each time the game starts.
    //!
    //! The downloads are made with curl, which macOS, Windows 10 and later and
    //! Linux systems all come with. Saves are kept in the player's data folder,
    //! not next to the game, so a new version carries on with the same progress.
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};

    use bevy::prelude::*;

    const RELEASES: &str = "https://github.com/JosiahHands/carGame/releases";

    #[derive(Clone, Debug, PartialEq)]
    pub enum Status {
        /// Nothing checked yet.
        Idle,
        Checking,
        UpToDate,
        /// A newer version (like "1.3.3") can be installed.
        Available(String),
        /// Downloading a version into `file`, `size` bytes in all (if known).
        Downloading {
            version: String,
            file: PathBuf,
            size: Option<u64>,
        },
        Installing(String),
        /// Installed: the game closes, and the new version starts.
        Restarting(String),
        Failed(String),
    }

    /// How this copy of the game was installed, which says how to replace it.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Install {
        /// macOS: the Car Game.app bundle.
        MacApp(PathBuf),
        /// Windows: the folder car-game.exe is in. The installer installs over it.
        Windows(PathBuf),
        /// Linux: the "Car Game" folder from the zip.
        LinuxFolder(PathBuf),
        /// Linux: the AppImage file.
        AppImage(PathBuf),
    }

    /// What to start once this copy of the game has closed.
    #[derive(Debug)]
    enum Launch {
        Program(PathBuf),
        MacApp(PathBuf),
        /// The new version's installer, which starts the game when it's done.
        WindowsSetup {
            setup: PathBuf,
            dir: PathBuf,
        },
    }

    static LAUNCH: Mutex<Option<Launch>> = Mutex::new(None);

    /// Checks for and installs updates in the background; the menus show its status.
    #[derive(Resource, Clone)]
    pub struct Updater {
        status: Arc<Mutex<Status>>,
    }

    impl Default for Updater {
        fn default() -> Self {
            Updater {
                status: Arc::new(Mutex::new(Status::Idle)),
            }
        }
    }

    impl Updater {
        pub fn status(&self) -> Status {
            self.status.lock().map(|s| s.clone()).unwrap_or(Status::Idle)
        }

        fn set(&self, status: Status) {
            if let Ok(mut s) = self.status.lock() {
                *s = status;
            }
        }

        /// Checking, downloading or installing.
        pub fn busy(&self) -> bool {
            matches!(
                self.status(),
                Status::Checking | Status::Downloading { .. } | Status::Installing(_) | Status::Restarting(_)
            )
        }

        /// Looks for a newer version, and with `then_install` installs it if there is one.
        pub fn check(&self, then_install: bool) {
            if self.busy() {
                return;
            }
            self.set(Status::Checking);
            let me = self.clone();
            std::thread::spawn(move || match latest_version() {
                Ok(v) if newer(&v, crate::VERSION) && then_install && installation().is_some() => me.run_install(v),
                Ok(v) if newer(&v, crate::VERSION) => me.set(Status::Available(v)),
                Ok(_) => me.set(Status::UpToDate),
                Err(e) => me.set(Status::Failed(e)),
            });
        }

        /// Downloads and installs `version`; the game then restarts on it.
        pub fn install(&self, version: String) {
            if self.busy() {
                return;
            }
            self.set(Status::Installing(version.clone()));
            let me = self.clone();
            std::thread::spawn(move || me.run_install(version));
        }

        fn run_install(&self, version: String) {
            match install(&version, self) {
                Ok(launch) => {
                    if let Ok(mut l) = LAUNCH.lock() {
                        *l = Some(launch);
                    }
                    self.set(Status::Restarting(version));
                }
                Err(e) => self.set(Status::Failed(e)),
            }
        }

        /// While downloading: the bytes downloaded so far, and how many there are in all.
        pub fn progress(&self) -> Option<(u64, Option<u64>)> {
            match self.status() {
                Status::Downloading { file, size, .. } => Some((std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0), size)),
                _ => None,
            }
        }
    }

    /// True if version `a` ("1.3.10") is newer than `b` ("1.3.9").
    pub fn newer(a: &str, b: &str) -> bool {
        let parts = |v: &str| -> Vec<u64> {
            v.trim_start_matches('v')
                .split(['-', '+'])
                .next()
                .unwrap_or("")
                .split('.')
                .map(|n| n.parse().unwrap_or(0))
                .collect()
        };
        let (a, b) = (parts(a), parts(b));
        for i in 0..a.len().max(b.len()) {
            let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
            if x != y {
                return x > y;
            }
        }
        false
    }

    /// The version a releases/latest redirect points to, from its headers.
    pub fn version_from_headers(headers: &str) -> Option<String> {
        let location = headers
            .lines()
            .filter_map(|l| l.split_once(':'))
            .filter(|(name, _)| name.trim().eq_ignore_ascii_case("location"))
            .map(|(_, value)| value.trim())
            .next_back()?;
        let tag = location.rsplit_once("/releases/tag/")?.1;
        let version = tag.trim_start_matches('v').trim_end_matches('/');
        let valid = !version.is_empty() && version.split('.').all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        valid.then(|| version.to_string())
    }

    fn curl() -> Command {
        let mut command = Command::new("curl");
        command.stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // The game has no console, so don't open one for curl.
            command.creation_flags(0x0800_0000);
        }
        command
    }

    /// Runs a command to the end; the error says what couldn't be done.
    fn run(command: &mut Command, what: &str) -> Result<String, String> {
        match command.stdout(Stdio::piped()).stderr(Stdio::piped()).output() {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            Ok(out) => {
                let error = String::from_utf8_lossy(&out.stderr);
                let error = error.trim().lines().last().unwrap_or("").trim_start_matches("curl: ");
                Err(if error.is_empty() {
                    format!("Couldn't {what}.")
                } else {
                    format!("Couldn't {what}: {error}")
                })
            }
            Err(e) => Err(format!("Couldn't {what}: {e}")),
        }
    }

    /// The newest released version, like "1.3.3".
    fn latest_version() -> Result<String, String> {
        let headers = run(
            curl()
                .args(["-sSfI", "--max-time", "20", "--proto", "=https"])
                .arg(format!("{RELEASES}/latest")),
            "check for updates (is the computer online?)",
        )?;
        version_from_headers(&headers).ok_or_else(|| "Couldn't find the latest version.".to_string())
    }

    /// The size of a download, if the server says.
    fn download_size(url: &str) -> Option<u64> {
        let headers = run(curl().args(["-sSfIL", "--max-time", "20", "--proto-redir", "=https"]).arg(url), "").ok()?;
        headers
            .lines()
            .filter_map(|l| l.split_once(':'))
            .filter(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
            .filter_map(|(_, value)| value.trim().parse().ok())
            .next_back()
    }

    /// How this copy of the game was installed, or None if it wasn't installed
    /// from a download (it was built from the source code), so it can't update itself.
    pub fn installation() -> Option<Install> {
        let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
        let dir = exe.parent()?;
        if cfg!(target_os = "macos") {
            let app = dir.parent()?.parent()?;
            let bundled = dir.ends_with("Contents/MacOS") && app.extension().is_some_and(|e| e == "app");
            (bundled && dir.join("../Resources/assets").is_dir()).then(|| Install::MacApp(app.to_path_buf()))
        } else if cfg!(windows) {
            (dir.join("assets").is_dir() && dir.join("icon.ico").is_file()).then(|| Install::Windows(dir.to_path_buf()))
        } else if let Some(image) = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file()) {
            Some(Install::AppImage(image))
        } else {
            let zip = exe.file_name().is_some_and(|n| n == "car-game-bin") && dir.join("car-game").is_file();
            (zip && dir.join("assets").is_dir()).then(|| Install::LinuxFolder(dir.to_path_buf()))
        }
    }

    /// A folder for unpacking an update next to the installed game (so it can
    /// be moved into place), removed afterwards.
    struct Stage(PathBuf);

    impl Stage {
        fn next_to(installed: &Path) -> Result<Stage, String> {
            let parent = installed.parent().ok_or("The game's folder couldn't be found.")?;
            let stage = parent.join(format!(".car-game-update-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&stage);
            std::fs::create_dir_all(&stage).map_err(|e| {
                format!(
                    "Couldn't write to {} ({e}). Try updating with the install command.",
                    parent.display()
                )
            })?;
            Ok(Stage(stage))
        }
    }

    impl Drop for Stage {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Puts `new` in place of `old`, keeping `old` if that fails.
    fn replace(old: &Path, new: &Path, stage: &Stage) -> Result<(), String> {
        let previous = stage.0.join("previous");
        std::fs::rename(old, &previous).map_err(|e| format!("Couldn't replace the game ({e})."))?;
        if let Err(e) = std::fs::rename(new, old) {
            let _ = std::fs::rename(&previous, old);
            return Err(format!("Couldn't replace the game ({e})."));
        }
        Ok(())
    }

    #[cfg(unix)]
    fn make_runnable(file: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755));
    }

    #[cfg(not(unix))]
    fn make_runnable(_file: &Path) {}

    fn mac_download() -> &'static str {
        let arm = cfg!(target_arch = "aarch64")
            || Command::new("sysctl")
                .args(["-n", "hw.optional.arm64"])
                .output()
                .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1");
        if arm {
            "Car-Game-macOS-Apple-Silicon.zip"
        } else {
            "Car-Game-macOS-Intel.zip"
        }
    }

    /// Downloads `version` and puts it in place of this copy (on Windows, gets
    /// its installer ready); then says what to start.
    fn install(version: &str, updater: &Updater) -> Result<Launch, String> {
        let installed = installation()
            .ok_or("This copy of the game was built from the source code, so it can't update itself. Download the new version instead.")?;
        let download = |name: &str, file: &Path| -> Result<(), String> {
            let url = format!("{RELEASES}/download/v{version}/{name}");
            updater.set(Status::Downloading {
                version: version.into(),
                file: file.into(),
                size: download_size(&url),
            });
            let mut command = curl();
            command
                .args(["-fsSL", "--retry", "3", "--proto", "=https", "--proto-redir", "=https", "-o"])
                .arg(file)
                .arg(&url);
            run(&mut command, "download the update")?;
            updater.set(Status::Installing(version.into()));
            Ok(())
        };
        let missing = "The download was missing the game.".to_string();
        match installed {
            Install::MacApp(app) => {
                let stage = Stage::next_to(&app)?;
                let zip = stage.0.join("update.zip");
                download(mac_download(), &zip)?;
                run(
                    Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&stage.0),
                    "unpack the update",
                )?;
                let new = stage.0.join("Car Game.app");
                if !new.join("Contents/MacOS/car-game").is_file() {
                    return Err(missing);
                }
                let _ = Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&new).output();
                replace(&app, &new, &stage)?;
                // Launchpad, Spotlight and the Dock see the new version straight away.
                let _ = Command::new(
                    "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
                )
                .arg("-f")
                .arg(&app)
                .output();
                Ok(Launch::MacApp(app))
            }
            Install::LinuxFolder(folder) => {
                let stage = Stage::next_to(&folder)?;
                let zip = stage.0.join("update.zip");
                download("Car-Game-Linux.zip", &zip)?;
                run(
                    Command::new("unzip").arg("-q").arg(&zip).arg("-d").arg(&stage.0),
                    "unpack the update (is unzip installed?)",
                )?;
                let new = stage.0.join("Car Game");
                if !new.join("car-game-bin").is_file() {
                    return Err(missing);
                }
                make_runnable(&new.join("car-game"));
                make_runnable(&new.join("car-game-bin"));
                replace(&folder, &new, &stage)?;
                Ok(Launch::Program(folder.join("car-game")))
            }
            Install::AppImage(image) => {
                let new = image.with_file_name(".car-game-update.AppImage");
                let result = download("Car-Game-Linux.AppImage", &new).and_then(|()| {
                    make_runnable(&new);
                    std::fs::rename(&new, &image).map_err(|e| format!("Couldn't replace the game ({e})."))
                });
                if result.is_err() {
                    let _ = std::fs::remove_file(&new);
                }
                result.map(|()| Launch::Program(image))
            }
            Install::Windows(dir) => {
                let folder = std::env::temp_dir().join("car-game-update");
                std::fs::create_dir_all(&folder).map_err(|e| format!("Couldn't download the update ({e})."))?;
                let setup = folder.join("Car-Game-Windows-Setup.exe");
                download("Car-Game-Windows-Setup.exe", &setup)?;
                Ok(Launch::WindowsSetup { setup, dir })
            }
        }
    }

    /// Checks for an update as the game starts, when the player has asked for that.
    pub fn check_at_start(updater: Res<Updater>, storage: Res<crate::storage::Storage>) {
        let unattended = std::env::var_os("CAR_GAME_SCREENSHOT").is_some();
        if crate::settings::load_settings(&storage).auto_update && !unattended && installation().is_some() {
            updater.check(true);
        }
    }

    /// Closes the game once an update is installed; [`start_new_version`] then starts it.
    pub fn close_when_installed(updater: Res<Updater>, mut exit: MessageWriter<AppExit>, mut sent: Local<bool>) {
        if !*sent && matches!(updater.status(), Status::Restarting(_)) {
            *sent = true;
            exit.write(AppExit::Success);
        }
    }

    /// Starts the newly installed version, after the game has closed (and saved).
    pub fn start_new_version() {
        let Some(launch) = LAUNCH.lock().ok().and_then(|mut l| l.take()) else {
            return;
        };
        let mut command = match &launch {
            Launch::Program(program) => Command::new(program),
            Launch::MacApp(app) => {
                let mut c = Command::new("open");
                c.arg("-n").arg(app);
                c
            }
            Launch::WindowsSetup { setup, dir } => {
                let mut c = Command::new(setup);
                c.args([
                    "/SP-",
                    "/VERYSILENT",
                    "/SUPPRESSMSGBOXES",
                    "/NORESTART",
                    "/CLOSEAPPLICATIONS",
                    "/LAUNCH=1",
                ])
                .arg(format!("/DIR={}", dir.display()));
                c
            }
        };
        if let Err(e) = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            eprintln!("Couldn't start the new version ({launch:?}): {e}");
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn compares_versions_by_number() {
            assert!(newer("1.3.3", "1.3.2"));
            assert!(newer("1.3.10", "1.3.9"));
            assert!(newer("2.0", "1.9.9"));
            assert!(newer("v1.4.0", "1.3.2"));
            assert!(!newer("1.3.2", "1.3.2"));
            assert!(!newer("1.3.2", "1.3.10"));
            assert!(!newer("1.3.2-beta", "1.3.2"));
        }

        #[test]
        fn finds_the_version_in_the_latest_release_redirect() {
            let headers = "HTTP/2 302\r\ndate: x\r\nLocation: https://github.com/JosiahHands/carGame/releases/tag/v1.3.3\r\n\r\n";
            assert_eq!(version_from_headers(headers).as_deref(), Some("1.3.3"));
            // No releases yet: the redirect goes to the releases page.
            assert_eq!(
                version_from_headers("HTTP/2 302\r\nlocation: https://github.com/JosiahHands/carGame/releases\r\n"),
                None
            );
            assert_eq!(
                version_from_headers("location: https://github.com/x/releases/tag/nightly\r\n"),
                None
            );
        }

        #[test]
        fn a_copy_built_from_source_cant_update_itself() {
            // Tests run from builds/, not an installed download.
            assert_eq!(installation(), None);
        }
    }
}
