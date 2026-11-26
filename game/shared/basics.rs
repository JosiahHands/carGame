//! Small things the whole game uses: random numbers, the date, number and
//! time formatting, the packs list, and the helpers for checking the game
//! without anyone watching.

pub mod rng {
    //! Random numbers: a small seeded generator (mulberry32, the same one the
    //! 1.2.x game used, so the same seed makes the same race tracks), and quick
    //! helpers for the everyday kind.

    /// A seeded random number generator. Same seed, same sequence.
    #[derive(Clone, Debug)]
    pub struct Seeded(u32);

    impl Seeded {
        pub fn new(seed: f64) -> Self {
            // JavaScript's `seed >>> 0`: the number wrapped into 32 bits.
            Seeded(js_u32(seed))
        }

        /// The next number, 0 up to (not including) 1.
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_add(0x6d2b_79f5);
            let mut t = self.0;
            t = (t ^ (t >> 15)).wrapping_mul(t | 1);
            t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
            f64::from(t ^ (t >> 14)) / 4_294_967_296.0
        }

        pub fn between(&mut self, min: f64, max: f64) -> f64 {
            min + (max - min) * self.next()
        }
    }

    /// JavaScript's `x >>> 0` for any number.
    fn js_u32(x: f64) -> u32 {
        if !x.is_finite() {
            return 0;
        }
        let t = x.trunc();
        t.rem_euclid(4_294_967_296.0) as u32
    }

    /// A random number 0..1 from the thread's generator.
    pub fn random() -> f64 {
        rand::random::<f64>()
    }

    /// A random number between `min` and `max`.
    pub fn between(min: f64, max: f64) -> f64 {
        min + (max - min) * random()
    }

    /// -0.5..0.5
    pub fn centered() -> f64 {
        random() - 0.5
    }

    /// A random item from a list.
    pub fn pick<T: Clone>(list: &[T]) -> T {
        list[((random() * list.len() as f64) as usize).min(list.len() - 1)].clone()
    }

    /// A random whole number 0..n.
    pub fn index(n: usize) -> usize {
        ((random() * n as f64) as usize).min(n.saturating_sub(1))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn matches_the_old_game() {
            // seededRandom(1) in the 1.2.x game starts 0.6270739405881613, 0.002735721180215478.
            let mut r = Seeded::new(1.0);
            assert!((r.next() - 0.627_073_940_588_161_3).abs() < 1e-15);
            assert!((r.next() - 0.002_735_721_180_215_478).abs() < 1e-15);
        }
    }
}

pub mod clock {
    //! Today's date on this computer, for the daily challenge.

    /// The local date (year, month, day) `days_ago` days before today.
    pub fn local_date(days_ago: i64) -> (i32, u32, u32) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let days = (now + offset_seconds()).div_euclid(86_400) - days_ago;
        civil_from_days(days)
    }

    /// The days since 1970-01-01 as a date (Howard Hinnant's algorithm).
    fn civil_from_days(z: i64) -> (i32, u32, u32) {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
    }

    /// How far local time is ahead of UTC, in seconds.
    fn offset_seconds() -> i64 {
        #[cfg(unix)]
        {
            // SAFETY: localtime_r only writes into the struct we give it.
            unsafe {
                let now = libc_time();
                let mut tm: Tm = std::mem::zeroed();
                if localtime_r(&now, &mut tm).is_null() {
                    return 0;
                }
                tm.tm_gmtoff
            }
        }
        #[cfg(windows)]
        {
            windows_offset()
        }
    }

    #[cfg(unix)]
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }

    #[cfg(unix)]
    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
        fn time(out: *mut i64) -> i64;
    }

    #[cfg(unix)]
    unsafe fn libc_time() -> i64 {
        unsafe { time(std::ptr::null_mut()) }
    }

    #[cfg(windows)]
    fn windows_offset() -> i64 {
        #[repr(C)]
        struct TimeZoneInformation {
            bias: i32,
            standard_name: [u16; 32],
            standard_date: [u16; 8],
            standard_bias: i32,
            daylight_name: [u16; 32],
            daylight_date: [u16; 8],
            daylight_bias: i32,
        }
        unsafe extern "system" {
            fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
        }
        // SAFETY: the function fills in the struct we pass.
        unsafe {
            let mut info: TimeZoneInformation = std::mem::zeroed();
            let mode = GetTimeZoneInformation(&mut info);
            let daylight = if mode == 2 { info.daylight_bias } else { 0 };
            -((info.bias + daylight) as i64) * 60
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn dates() {
            assert_eq!(super::civil_from_days(0), (1970, 1, 1));
            assert_eq!(super::civil_from_days(20_000), (2024, 10, 4));
        }
    }
}

pub mod text {
    //! Writing numbers and times for people.

    /// A whole number with commas: 12345 -> "12,345".
    pub fn thousands(n: f64) -> String {
        let n = n.round();
        let negative = n < 0.0;
        let digits = format!("{}", n.abs() as u64);
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        if negative { format!("-{out}") } else { out }
    }

    /// 83.456 seconds -> "1:23.46".
    pub fn format_time(seconds: f64) -> String {
        let minutes = (seconds / 60.0).floor();
        let rest = seconds - minutes * 60.0;
        format!("{}:{:05.2}", minutes as i64, rest)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn numbers() {
            assert_eq!(super::thousands(1234567.0), "1,234,567");
            assert_eq!(super::thousands(12.0), "12");
            assert_eq!(super::format_time(83.456), "1:23.46");
            assert_eq!(super::format_time(5.0), "0:05.00");
        }
    }
}

pub mod packs {
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
                    && !matches!((pack, option), ("npc", "city" | "options" | "ai"))
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

    /// Something that happened in the game this frame, for packs to react to
    /// (in `Game.happened`, cleared at the start of every frame).
    #[derive(Clone, Debug)]
    pub enum Happening {
        /// The player's car hit something hard (impact in m/s).
        Crash {
            x: f64,
            z: f64,
            impact: f64,
        },
        /// Something in the world was destroyed.
        Destroyed {
            x: f64,
            z: f64,
            kind: crate::world::TargetKind,
        },
        /// The player's car landed a jump.
        Jump {
            x: f64,
            z: f64,
            air_time: f64,
        },
        /// A near miss with traffic.
        NearMiss {
            x: f64,
            z: f64,
        },
        /// A rocket blew up.
        Explosion {
            x: f64,
            z: f64,
        },
        Horn {
            x: f64,
            z: f64,
        },
        /// A drift was banked (points).
        Drift {
            x: f64,
            z: f64,
            points: f64,
        },
    }
}

pub mod dev {
    //! Helpers for checking the game without anyone watching (on a build
    //! machine, for example).
    //!
    //! `CAR_GAME_SCREENSHOT=shot.png car-game` saves a picture of the game once
    //! it has settled, then quits.

    use bevy::prelude::*;
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};

    /// Frames to wait before the picture, so the first shaders have compiled.
    const SETTLE_FRAMES: u32 = 120;
    /// Frames to keep running once the picture is on disk.
    const SAVE_FRAMES: u32 = 5;
    /// Frames to wait for the picture at most (software renderers can take a
    /// long time to compile the capture pipeline), before giving up with an error.
    const GIVE_UP_FRAMES: u32 = 3000;

    pub struct DevPlugin;

    impl Plugin for DevPlugin {
        fn build(&self, app: &mut App) {
            if let Some(path) = std::env::var_os("CAR_GAME_SCREENSHOT") {
                app.insert_resource(ScreenshotThenQuit {
                    path: path.into(),
                    frame: 0,
                    capture_at: std::env::var("CAR_GAME_SCREENSHOT_FRAMES")
                        .ok()
                        .and_then(|value| value.parse::<u32>().ok())
                        .unwrap_or(SETTLE_FRAMES)
                        .clamp(1, 3600),
                    saved_at: None,
                })
                .add_systems(Update, screenshot_then_quit);
            }
        }
    }

    #[derive(Resource)]
    struct ScreenshotThenQuit {
        path: std::path::PathBuf,
        frame: u32,
        capture_at: u32,
        /// The frame the picture was found on disk.
        saved_at: Option<u32>,
    }

    fn screenshot_then_quit(mut commands: Commands, mut shot: ResMut<ScreenshotThenQuit>, mut exit: MessageWriter<AppExit>) {
        shot.frame += 1;
        if shot.frame == 1 {
            // A picture left over from an earlier run mustn't count as this one.
            let _ = std::fs::remove_file(&shot.path);
        }
        // Progress in the log, so a slow machine can be told apart from a stuck one.
        if shot.frame % 30 == 0 && shot.saved_at.is_none() {
            info!(
                "Screenshot: frame {} (the picture is taken at frame {})",
                shot.frame, shot.capture_at
            );
        }
        if shot.frame == shot.capture_at {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(shot.path.clone()));
        } else if shot.frame > shot.capture_at {
            // Quit once the picture is really on disk, however long the capture takes.
            if shot.saved_at.is_none() && std::fs::metadata(&shot.path).is_ok_and(|m| m.len() > 0) {
                shot.saved_at = Some(shot.frame);
                info!("Screenshot saved at frame {}", shot.frame);
            }
            match shot.saved_at {
                Some(at) if shot.frame >= at + SAVE_FRAMES => {
                    exit.write(AppExit::Success);
                }
                None if shot.frame > shot.capture_at + GIVE_UP_FRAMES => {
                    error!("The screenshot was never saved to {}", shot.path.display());
                    exit.write(AppExit::error());
                }
                _ => {}
            }
        }
    }

    /// `CAR_GAME_AUTOPILOT=steer` holds the throttle down and the steering at
    /// `steer` (-1..1), to see the car moving in screenshots.
    pub fn autopilot() -> Option<f64> {
        std::env::var("CAR_GAME_AUTOPILOT").ok().and_then(|v| v.parse().ok())
    }
}
