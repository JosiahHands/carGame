//! Career mode: 100 levels in 10 chapters, coins, cars and upgrades. Also the
//! sandbox (endless levels after 100), the daily challenge, and quick races.
//!
//! The economy, roughly: a perfect first run through all 100 levels pays about
//! 230,000 coins. Every car and every upgrade costs about 270,000, so maxing
//! everything means replaying levels for 3 stars and doing dailies. Each time
//! you finish the career and start again, coins pay 25% more.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{CarConfig, PAINT_SWATCHES, Tuning, range};
use crate::rng::Seeded;
use crate::spec::{EventKind, EventSpec, RivalSpec};
use crate::storage::Storage;
use crate::venues::{self, MAP_NAMES};

// ---------- cars ----------

pub struct Chassis {
    pub name: &'static str,
    pub body: &'static str,
    pub price: f64,
    pub perk: &'static str,
    pub spoiler: &'static str,
    pub all_terrain: bool,
    /// horsepower, top speed, grip, brakes, steering, drift, downforce, nitro power, nitro seconds
    pub tuning: [f64; 9],
}

pub const CHASSIS: &[Chassis] = &[
    Chassis {
        name: "Hot Hatch",
        body: "hatch",
        price: 0.0,
        perk: "Nimble: the sharpest steering of any car.",
        spoiler: "lip",
        all_terrain: false,
        tuning: [150.0, 92.0, 0.95, 0.9, 36.0, 5.0, 0.0, 0.9, 3.0],
    },
    Chassis {
        name: "Muscle Car",
        body: "muscle",
        price: 3000.0,
        perk: "Big torque: lots of power, and it loves to slide.",
        spoiler: "none",
        all_terrain: false,
        tuning: [340.0, 110.0, 0.92, 0.9, 28.0, 7.0, 0.0, 1.1, 3.5],
    },
    Chassis {
        name: "Rally Truck",
        body: "pickup",
        price: 8000.0,
        perk: "All-terrain: grass, rain and snow barely slow it down.",
        spoiler: "none",
        all_terrain: true,
        tuning: [380.0, 114.0, 1.0, 1.0, 32.0, 6.0, 0.5, 1.0, 3.5],
    },
    Chassis {
        name: "Sports GT",
        body: "sports",
        price: 16000.0,
        perk: "Balanced: quick everywhere, with a little downforce.",
        spoiler: "wing",
        all_terrain: false,
        tuning: [470.0, 128.0, 1.05, 1.1, 30.0, 5.0, 2.0, 1.1, 4.0],
    },
    Chassis {
        name: "Supercar",
        body: "wedge",
        price: 40000.0,
        perk: "Aero: big downforce, so it flies through fast corners.",
        spoiler: "gt",
        all_terrain: false,
        tuning: [660.0, 150.0, 1.12, 1.25, 28.0, 4.0, 4.5, 1.2, 4.0],
    },
    Chassis {
        name: "Hypercar",
        body: "wedge",
        price: 100000.0,
        perk: "Hyperdrive: monster nitro, and the fastest thing on wheels.",
        spoiler: "drag",
        all_terrain: false,
        tuning: [920.0, 178.0, 1.2, 1.4, 28.0, 4.0, 7.0, 1.5, 5.0],
    },
];

pub const UPGRADE_IDS: [&str; 7] = ["engine", "gearbox", "tires", "brakes", "nitro", "aero", "armor"];

/// (id, name, about, icon)
pub const UPGRADES: &[(&str, &str, &str, &str)] = &[
    ("engine", "Engine", "+8% power: faster acceleration", "🔧"),
    ("gearbox", "Gearbox", "+3.5% top speed", "⚙"),
    ("tires", "Tires", "+3% grip: faster through corners", "🛞"),
    ("brakes", "Brakes", "+6% braking: brake later", "🛑"),
    ("nitro", "Nitro", "+5% boost and a longer-lasting bottle", "🔥"),
    ("aero", "Aero", "More downforce: more grip at speed", "🪽"),
    ("armor", "Armor", "+12% toughness in takedowns", "🛡"),
];

pub const MAX_UPGRADE: u32 = 10;
pub const SANDBOX_MAX_UPGRADE: u32 = 20;

pub fn upgrade_cost(level: u32) -> f64 {
    ((200.0 * 1.4f64.powi(level as i32)) / 10.0).round() * 10.0
}

fn steps(level: u32) -> f64 {
    level.min(10) as f64 + level.saturating_sub(10) as f64 * 0.5
}

pub type Upgrades = BTreeMap<String, u32>;

pub fn no_upgrades() -> Upgrades {
    UPGRADE_IDS.iter().map(|k| (k.to_string(), 0)).collect()
}

fn all_upgrades(level: u32) -> Upgrades {
    UPGRADE_IDS.iter().map(|k| (k.to_string(), level)).collect()
}

fn up(u: &Upgrades, k: &str) -> u32 {
    u.get(k).copied().unwrap_or(0)
}

// ---------- weapons ----------

/// The most upgrade levels a weapon can have.
pub const MAX_WEAPON_LEVEL: u32 = 5;
pub type Loadout = [u32; 3];
pub const NO_WEAPONS: Loadout = [0, 0, 0];
pub const STOCK_WEAPONS: Loadout = [1, 1, 1];

pub const WEAPON_IDS: [&str; 3] = ["gun", "rockets", "shockwave"];

/// (id, name, about, icon, price, unlock level)
pub const WEAPON_SHOP: &[(&str, &str, &str, &str, f64, usize)] = &[
    (
        "gun",
        "Machine gun",
        "Fast shots from the roof. Overheats if you hold the trigger too long.",
        "🔫",
        800.0,
        6,
    ),
    (
        "rockets",
        "Rocket pod",
        "Rockets that blow up everything near where they land.",
        "🚀",
        3500.0,
        11,
    ),
    (
        "shockwave",
        "Shockwave",
        "A blast all around the car, then a few seconds to recharge.",
        "💥",
        10000.0,
        31,
    ),
];

pub const WEAPON_UPGRADE_ABOUT: &str = "Each upgrade: +15% damage, and the gun runs cooler or it reloads faster.";

pub type Arsenal = BTreeMap<String, u32>;

pub fn no_weapons() -> Arsenal {
    WEAPON_IDS.iter().map(|k| (k.to_string(), 0)).collect()
}

pub fn loadout_of(arsenal: &Arsenal) -> Loadout {
    [up(arsenal, "gun"), up(arsenal, "rockets"), up(arsenal, "shockwave")]
}

/// The weapons a car has: none when weapons are off; the career car carries what's been bought; any other car has all three.
pub fn car_loadout(weapons_on: bool, arsenal: Option<&Arsenal>) -> Loadout {
    if !weapons_on {
        return NO_WEAPONS;
    }
    arsenal.map_or(STOCK_WEAPONS, loadout_of)
}

pub fn weapon_cost(id: &str, level: u32) -> f64 {
    let price = WEAPON_SHOP.iter().find(|w| w.0 == id).map_or(0.0, |w| w.4);
    if level == 0 {
        price
    } else {
        ((price * 0.35 * 1.4f64.powi(level as i32 - 1)) / 10.0).round() * 10.0
    }
}

pub fn weapon_locked_reason(save: &CareerSave, id: &str) -> String {
    let unlock = WEAPON_SHOP.iter().find(|w| w.0 == id).map_or(1, |w| w.5);
    if save.mode == "sandbox" || save.stars[unlock - 2] > 0 {
        String::new()
    } else {
        format!("Unlocks at level {unlock}")
    }
}

pub fn buy_weapon(save: &mut CareerSave, id: &str) -> bool {
    let level = up(&save.arsenal, id);
    let cost = weapon_cost(id, level);
    if !weapon_locked_reason(save, id).is_empty() || level >= MAX_WEAPON_LEVEL || save.coins < cost {
        return false;
    }
    save.coins -= cost;
    save.arsenal.insert(id.into(), level + 1);
    true
}

/// A car's tuning: the chassis, with the upgrades on top.
pub fn career_tuning(chassis: usize, u: &Upgrades) -> Tuning {
    let c = &CHASSIS[chassis.min(CHASSIS.len() - 1)];
    let [hp, top, grip, brakes, steering, drift, downforce, nitro_power, nitro_seconds] = c.tuning;
    let mut t = Tuning {
        horsepower: hp,
        top_speed: top,
        grip,
        brakes,
        steering,
        drift,
        downforce,
        nitro_power,
        nitro_seconds,
        jump: 0.0,
        gravity: 1.0,
        bounce: 0.35,
        air_control: 0.15,
        infinite_nitro: false,
    };
    t.horsepower *= 1.0 + 0.08 * steps(up(u, "engine"));
    t.top_speed *= 1.0 + 0.035 * steps(up(u, "gearbox"));
    t.grip += 0.03 * steps(up(u, "tires"));
    t.brakes *= 1.0 + 0.06 * steps(up(u, "brakes"));
    t.nitro_power *= 1.0 + 0.05 * steps(up(u, "nitro"));
    t.nitro_seconds += 0.3 * steps(up(u, "nitro"));
    t.downforce = (t.downforce + 0.4 * steps(up(u, "aero"))).min(10.0);
    t
}

/// How much damage a car with this armor level can take.
pub fn armor_for(level: u32) -> f64 {
    100.0 * (1.0 + 0.12 * steps(level))
}

/// The player's career car: the chassis and upgrades, wearing the paint and parts from the garage.
pub fn career_car(looks: &CarConfig, chassis: usize, u: &Upgrades) -> CarConfig {
    let c = &CHASSIS[chassis.min(CHASSIS.len() - 1)];
    CarConfig {
        body: c.body.into(),
        size: 1.0,
        length: 1.0,
        width: 1.0,
        height: 1.0,
        wheel_size: looks.wheel_size.min(1.2),
        tire_width: looks.tire_width.min(1.5),
        track: looks.track.min(1.2),
        lift: looks.lift.min(0.15),
        tuning: career_tuning(chassis, u),
        ..looks.clone()
    }
}

fn pick<T: Clone>(list: &[T], rand: &mut Seeded) -> T {
    list[((rand.next() * list.len() as f64) as usize).min(list.len() - 1)].clone()
}

/// A rival's car: a chassis with upgrades and a random paint job.
pub fn rival_car(chassis: usize, upgrade: u32, rand: &mut Seeded, boss: bool) -> CarConfig {
    let c = &CHASSIS[chassis];
    let mut config = CarConfig {
        body: c.body.into(),
        spoiler: c.spoiler.into(),
        ..CarConfig::default()
    };
    config.paint_color = pick(PAINT_SWATCHES, rand).into();
    config.accent_color = pick(PAINT_SWATCHES, rand).into();
    config.finish = pick(&["gloss", "metallic", "pearl", "matte"], rand).into();
    config.stripes = pick(&["none", "racing", "side"], rand).into();
    config.pattern = if rand.next() < 0.2 {
        pick(&["twoTone", "flames", "checker"], rand).into()
    } else {
        "none".into()
    };
    config.wheel_style = pick(&["fiveSpoke", "multiSpoke", "dish", "offroad"], rand).into();
    config.race_number = 1.0 + (rand.next() * 99.0).floor();
    config.roof_guns = false;
    config.plate = "RIVAL".into();
    if boss {
        config.paint_color = "#0b0b0b".into();
        config.accent_color = "#d4af37".into();
        config.finish = "chrome".into();
        config.stripes = "racing".into();
        config.rim_color = "#d4af37".into();
        config.topper = if rand.next() < 0.5 { "crown".into() } else { "horns".into() };
        config.underglow = "on".into();
        config.underglow_color = "#ffd23f".into();
        config.race_number = 1.0;
        config.plate = "BOSS".into();
    }
    config.tuning = career_tuning(chassis, &all_upgrades(upgrade));
    config
}

// ---------- the save ----------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CareerSave {
    pub coins: f64,
    pub stars: Vec<u32>,
    pub best: Vec<String>,
    pub best_score: Vec<f64>,
    pub upgrades: Upgrades,
    pub arsenal: Arsenal,
    pub owned: Vec<usize>,
    pub car: usize,
    /// "career" or "sandbox"
    pub mode: String,
    pub sandbox_level: usize,
    pub sandbox_best: usize,
    pub completions: u32,
    pub finished: bool,
    pub daily: String,
    pub streak: u32,
    pub events: u32,
    pub wins: u32,
    pub earned: f64,
    pub xp: f64,
    pub spins: u32,
    pub coin_rush: u32,
    pub stats: BTreeMap<String, f64>,
    pub trophies: Vec<String>,
    pub found: Vec<String>,
    pub zones: BTreeMap<String, f64>,
    pub nemesis: BTreeMap<String, f64>,
}

const SAVE_KEY: &str = "carGame.career.v1";
pub const LEVELS: usize = 100;

pub fn new_career(completions: u32) -> CareerSave {
    CareerSave {
        coins: 0.0,
        stars: vec![0; LEVELS],
        best: vec![String::new(); LEVELS],
        best_score: vec![0.0; LEVELS],
        upgrades: no_upgrades(),
        arsenal: no_weapons(),
        owned: vec![0],
        car: 0,
        mode: "career".into(),
        sandbox_level: LEVELS + 1,
        sandbox_best: 0,
        completions,
        finished: false,
        daily: String::new(),
        streak: 0,
        events: 0,
        wins: 0,
        earned: 0.0,
        xp: 0.0,
        spins: 0,
        coin_rush: 0,
        stats: BTreeMap::new(),
        trophies: Vec::new(),
        found: Vec::new(),
        zones: BTreeMap::new(),
        nemesis: BTreeMap::new(),
    }
}

/// Reads a career save, keeping only sensible values (like the 1.2.x game did).
pub fn read_career(raw: &Value) -> CareerSave {
    let mut save = new_career(0);
    let Some(r) = raw.as_object() else { return save };
    let num = |v: Option<&Value>, fallback: f64, max: f64| {
        v.and_then(Value::as_f64)
            .filter(|x| x.is_finite())
            .map_or(fallback, |x| x.floor().clamp(0.0, max))
    };
    save.coins = num(r.get("coins"), 0.0, 1e12);
    if let Some(Value::Array(a)) = r.get("stars") {
        for (i, s) in save.stars.iter_mut().enumerate() {
            *s = num(a.get(i), 0.0, 3.0) as u32;
        }
    }
    if let Some(Value::Array(a)) = r.get("bestScore") {
        for (i, s) in save.best_score.iter_mut().enumerate() {
            *s = a
                .get(i)
                .and_then(Value::as_f64)
                .filter(|x| x.is_finite())
                .map_or(0.0, |x| x.max(0.0));
        }
    }
    if let Some(Value::Array(a)) = r.get("best") {
        for (i, s) in save.best.iter_mut().enumerate() {
            *s = a
                .get(i)
                .and_then(Value::as_str)
                .map_or(String::new(), |t| t.chars().take(20).collect());
        }
    }
    // Saves from before 1.2.3 spelled two upgrades the British way.
    let ups = r.get("upgrades").and_then(Value::as_object).cloned().unwrap_or_default();
    for k in UPGRADE_IDS {
        let old = match k {
            "tires" => "tyres",
            "armor" => "armour",
            _ => k,
        };
        let v = ups.get(k).or_else(|| ups.get(old));
        save.upgrades.insert(k.into(), num(v, 0.0, SANDBOX_MAX_UPGRADE as f64) as u32);
    }
    for k in WEAPON_IDS {
        let v = r.get("arsenal").and_then(|a| a.get(k));
        save.arsenal.insert(k.into(), num(v, 0.0, MAX_WEAPON_LEVEL as f64) as u32);
    }
    if let Some(Value::Array(a)) = r.get("owned") {
        let mut owned = vec![0usize];
        for i in a.iter().filter_map(Value::as_f64) {
            let i = i as usize;
            if i < CHASSIS.len() && !owned.contains(&i) {
                owned.push(i);
            }
        }
        save.owned = owned;
    }
    let car = r.get("car").and_then(Value::as_f64).map_or(0, |c| c as usize);
    save.car = if save.owned.contains(&car) { car } else { 0 };
    save.mode = if r.get("mode").and_then(Value::as_str) == Some("sandbox") {
        "sandbox".into()
    } else {
        "career".into()
    };
    // A save from before weapons were bought (1.2.0 and older) keeps the ones its progress has unlocked, at level 1.
    if !r.contains_key("arsenal") {
        for w in WEAPON_SHOP {
            if weapon_locked_reason(&save, w.0).is_empty() {
                save.arsenal.insert(w.0.into(), 1);
            }
        }
    }
    save.sandbox_level = (num(r.get("sandboxLevel"), (LEVELS + 1) as f64, 1e12) as usize).max(LEVELS + 1);
    save.sandbox_best = num(r.get("sandboxBest"), 0.0, 1e12) as usize;
    save.completions = num(r.get("completions"), 0.0, 1000.0) as u32;
    save.finished = r.get("finished") == Some(&Value::Bool(true));
    save.daily = r
        .get("daily")
        .and_then(Value::as_str)
        .map_or(String::new(), |t| t.chars().take(10).collect());
    save.streak = num(r.get("streak"), 0.0, 1e9) as u32;
    save.events = num(r.get("events"), 0.0, 1e9) as u32;
    save.wins = num(r.get("wins"), 0.0, 1e9) as u32;
    save.earned = num(r.get("earned"), 0.0, 1e12);
    save.xp = num(r.get("xp"), 0.0, 1e12);
    save.spins = num(r.get("spins"), 0.0, 1000.0) as u32;
    save.coin_rush = num(r.get("coinRush"), 0.0, 1000.0) as u32;
    let numbers = |v: Option<&Value>, max: f64| -> BTreeMap<String, f64> {
        let mut out = BTreeMap::new();
        for (k, x) in v.and_then(Value::as_object).into_iter().flatten() {
            if let Some(x) = x.as_f64().filter(|x| x.is_finite()) {
                out.insert(k.chars().take(40).collect(), x.clamp(0.0, max));
            }
        }
        out
    };
    let strings = |v: Option<&Value>| -> Vec<String> {
        let mut seen = BTreeSet::new();
        v.and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|s| s.chars().take(40).collect::<String>())
            .filter(|s| seen.insert(s.clone()))
            .collect()
    };
    save.stats = numbers(r.get("stats"), 1e12);
    save.trophies = strings(r.get("trophies"));
    save.found = strings(r.get("found"));
    save.zones = numbers(r.get("zones"), 3.0);
    save.nemesis = numbers(r.get("nemesis"), 1e6);
    for (old, now) in crate::rivals::RENAMED_RIVALS {
        if let Some(wins) = save.nemesis.remove(*old) {
            let total = (save.nemesis.get(*now).copied().unwrap_or(0.0) + wins).min(1e6);
            save.nemesis.insert(now.to_string(), total);
        }
    }
    save
}

pub fn load_career(storage: &Storage) -> CareerSave {
    storage.get_json(SAVE_KEY).map_or_else(|| new_career(0), |v| read_career(&v))
}

pub fn save_career(storage: &mut Storage, save: &CareerSave) {
    storage.set_json(SAVE_KEY, save);
}

pub fn awaiting_choice(save: &CareerSave) -> bool {
    save.stars[LEVELS - 1] > 0 && !save.finished
}

pub fn total_stars(save: &CareerSave) -> u32 {
    save.stars.iter().sum()
}

pub fn upgrade_cap(save: &CareerSave) -> u32 {
    if save.mode == "sandbox" { SANDBOX_MAX_UPGRADE } else { MAX_UPGRADE }
}

pub fn prestige(save: &CareerSave) -> f64 {
    1.0 + 0.25 * save.completions as f64
}

// ---------- the levels ----------

pub struct Chapter {
    pub name: &'static str,
    pub about: &'static str,
    pub venues: &'static [&'static str],
    pub kinds: [EventKind; 9],
    pub boss: &'static str,
    pub cars: [usize; 2],
}

use EventKind::*;

pub const CHAPTERS: &[Chapter] = &[
    Chapter {
        name: "Rookie Cup",
        about: "Your first races, on the Grand Prix circuit and the kart track.",
        venues: &["circuit", "kart"],
        kinds: [Race, TimeAttack, Race, Drift, Race, SpeedTrap, Elimination, Race, Race],
        boss: "Big Earl",
        cars: [0, 0],
    },
    Chapter {
        name: "Street Kings",
        about: "Smash up the city, then race the neon streets at night.",
        venues: &["neon", "city"],
        kinds: [Destruction, Race, Stunt, Race, Takedown, Drift, Race, Destruction, Elimination],
        boss: "Vinnie Vega",
        cars: [0, 1],
    },
    Chapter {
        name: "Speedway Showdown",
        about: "Flat out on banked ovals. Nitro and nerve.",
        venues: &["oval"],
        kinds: [Race, SpeedTrap, Race, Elimination, TimeAttack, Race, Takedown, Race, Elimination],
        boss: "Dale \"Thunder\" Rhodes",
        cars: [1, 1],
    },
    Chapter {
        name: "Desert Heat",
        about: "Long, fast canyon roads between red rock mesas.",
        venues: &["canyon"],
        kinds: [Race, TimeAttack, Drift, Race, Takedown, SpeedTrap, Race, Elimination, Race],
        boss: "Sandstorm Sam",
        cars: [1, 2],
    },
    Chapter {
        name: "Frozen Peaks",
        about: "Snow, ice and mountain passes. Brake early.",
        venues: &["alpine"],
        kinds: [Race, Drift, TimeAttack, Race, Elimination, Race, Drift, Takedown, Race],
        boss: "Ice Queen Ingrid",
        cars: [2, 3],
    },
    Chapter {
        name: "Night Riders",
        about: "The city after dark. Walls close, rivals closer.",
        venues: &["neon", "city"],
        kinds: [Race, Takedown, Destruction, Race, Drift, Elimination, Stunt, Race, Takedown],
        boss: "Neon Nyx",
        cars: [3, 3],
    },
    Chapter {
        name: "Into the Woods",
        about: "Rally stages in the rain. No barriers, just trees.",
        venues: &["forest"],
        kinds: [Race, TimeAttack, Race, Elimination, Drift, Race, SpeedTrap, Race, TimeAttack],
        boss: "Grizzly Gus",
        cars: [3, 3],
    },
    Chapter {
        name: "Kart Mayhem",
        about: "Tight, twisty and packed with rivals.",
        venues: &["kart"],
        kinds: [Race, Elimination, Drift, Race, Takedown, TimeAttack, Race, Elimination, Race],
        boss: "Tiny Tina",
        cars: [3, 4],
    },
    Chapter {
        name: "World Tour",
        about: "Every venue, every kind of event. Prove you can do it all.",
        venues: &["circuit", "oval", "alpine", "canyon", "neon", "forest", "kart", "city"],
        kinds: [Race, Elimination, Drift, Takedown, Race, TimeAttack, SpeedTrap, Race, Destruction],
        boss: "The Baron",
        cars: [4, 4],
    },
    Chapter {
        name: "Legends",
        about: "The best drivers in the world, in the best cars. Then: The Phantom.",
        venues: &["circuit", "canyon", "alpine", "neon", "forest", "oval"],
        kinds: [Race, Elimination, Takedown, TimeAttack, Race, Drift, Race, SpeedTrap, Elimination],
        boss: "The Phantom",
        cars: [5, 5],
    },
];

pub const BOSS_STARS: u32 = 18;

#[derive(Clone, Debug)]
pub struct Level {
    pub number: usize,
    pub chapter: usize,
    pub spec: EventSpec,
    pub reward: f64,
    pub is_boss: bool,
}

fn is_city_kind(kind: EventKind) -> bool {
    matches!(kind, Destruction | Stunt)
}

fn base_reward(level: usize) -> f64 {
    100.0 + 20.0 * level as f64
}

/// Builds level `n` (1 to 100 for the career; above 100 for the sandbox). The same number always gives the same event.
pub fn make_level(n: usize) -> Level {
    if n > LEVELS {
        return sandbox_level(n);
    }
    let chapter = n.div_ceil(10);
    let ch = &CHAPTERS[chapter - 1];
    let index = (n - 1) % 10;
    let is_boss = index == 9;
    let kind = if is_boss { Duel } else { ch.kinds[index] };
    let mut rand = Seeded::new(n as f64 * 7919.0);
    let map = pick_venue(kind, ch.venues, index);
    let layout = if index.is_multiple_of(3) || map == "city" { 0 } else { n as u32 };
    let progress = (n - 1) as f64 / (LEVELS - 1) as f64;
    let skill = 0.8 + 0.16 * progress + if is_boss { 0.04 } else { 0.0 };
    let chassis = if index < 5 { ch.cars[0] } else { ch.cars[1] };
    let upgrade = MAX_UPGRADE.min(((chapter - 1) as f64 + index as f64 * 0.15).round() as u32);
    let boss_car = |rand: &mut Seeded| {
        rival_car(
            (ch.cars[1] + if chapter >= 9 { 0 } else { 1 }).min(5),
            (upgrade + 1).min(MAX_UPGRADE),
            rand,
            true,
        )
    };
    // The order of the random draws matters (it matches the 1.2.x game): the
    // night roll comes after the boss car is drawn, like the old code's argument order.
    let boss = if is_boss {
        Some((ch.boss.to_string(), boss_car(&mut rand)))
    } else {
        None
    };
    let night = map == "neon" || chapter == 6 || (chapter >= 3 && rand.next() < 0.15);
    let mut spec = build_spec(SpecOptions {
        kind,
        map,
        layout,
        rand: &mut rand,
        skill,
        level: n,
        rivals: rival_count(kind, chapter),
        rival_car: &mut |r| rival_car(chassis, upgrade, r, false),
        boss,
        night,
        modifiers: chapter >= 4,
        laps: None,
        catch_up: None,
    });
    spec.title = if is_boss {
        format!("Boss: {}", ch.boss)
    } else {
        format!("{} {}", ch.name, index + 1)
    };
    Level {
        number: n,
        chapter,
        spec,
        reward: base_reward(n),
        is_boss,
    }
}

fn pick_venue(kind: EventKind, venues: &[&'static str], index: usize) -> &'static str {
    if is_city_kind(kind) {
        return "city";
    }
    let tracks: Vec<&str> = venues.iter().copied().filter(|v| *v != "city").collect();
    if tracks.is_empty() {
        "circuit"
    } else {
        tracks[index % tracks.len()]
    }
}

fn rival_count(kind: EventKind, chapter: usize) -> usize {
    match kind {
        Duel => 1,
        Takedown => 4 + chapter / 4,
        Elimination => 5,
        Race => 7.min(4 + chapter / 3),
        _ => 0,
    }
}

struct SpecOptions<'a> {
    kind: EventKind,
    map: &'a str,
    layout: u32,
    rand: &'a mut Seeded,
    skill: f64,
    level: usize,
    rivals: usize,
    rival_car: &'a mut dyn FnMut(&mut Seeded) -> CarConfig,
    boss: Option<(String, CarConfig)>,
    night: bool,
    modifiers: bool,
    laps: Option<u32>,
    catch_up: Option<f64>,
}

fn build_spec(o: SpecOptions) -> EventSpec {
    let v = venues::venue(o.map);
    let length = if o.map == "city" {
        0.0
    } else {
        v.generate.as_ref().map_or(2000.0, |g| g.length)
    };
    let laps_for = |k: f64| o.laps.unwrap_or_else(|| ((k / length.max(900.0)).round() as u32).max(1));
    let names = shuffled(crate::rivals::RIVAL_NAMES, o.rand);
    let mut rivals = Vec::new();
    if let Some((name, car)) = &o.boss {
        rivals.push(RivalSpec {
            name: name.clone(),
            config: car.clone(),
            skill: o.skill.min(1.08),
            steady: false,
        });
    }
    let others = o.rivals - usize::from(o.boss.is_some());
    for (i, name) in names.iter().enumerate().take(others) {
        let config = (o.rival_car)(o.rand);
        rivals.push(RivalSpec {
            name: name.to_string(),
            config,
            skill: o.skill * (0.94 + 0.08 * (i as f64 / (o.rivals.max(2) - 1) as f64)),
            steady: false,
        });
    }
    let twist = if o.modifiers { o.rand.next() } else { 0.5 };
    let mut spec = EventSpec {
        kind: o.kind,
        title: String::new(),
        map: o.map.to_string(),
        layout: o.layout,
        laps: 1,
        time_limit: 0.0,
        rivals,
        pace: None,
        targets: [3.0, 2.0, 1.0],
        night: o.night,
        weapons: false,
        no_nitro: twist < 0.12,
        catch_up: o.catch_up.unwrap_or(0.6),
        armor: 100.0,
        rival_armor: 150.0 + 2.0 * o.level as f64,
        rival_damage: 1.2 + o.level as f64 * 0.03,
        boss: o.boss.as_ref().map(|b| b.0.clone()),
    };
    let grow = 1.0 + o.level as f64 * 0.006;
    let scale3 = |t: [f64; 3], f: &dyn Fn(f64) -> f64| t.map(f);
    match o.kind {
        Race => {
            spec.laps = laps_for(6000.0);
            spec.weapons = twist > 0.8;
        }
        Duel => {
            spec.laps = laps_for(6000.0);
            spec.targets = [1.0, 1.0, 1.0];
        }
        Elimination => {
            spec.laps = 99;
            spec.time_limit = (length / 75.0).clamp(18.0, 32.0).round();
        }
        TimeAttack => {
            spec.laps = laps_for(4000.0);
            spec.pace = Some(((o.rival_car)(o.rand), (o.skill + 0.02).min(1.0)));
        }
        SpeedTrap => {
            spec.laps = o.laps.unwrap_or(2);
            spec.pace = Some(((o.rival_car)(o.rand), (o.skill + 0.02).min(1.0)));
        }
        Takedown => {
            spec.weapons = true;
            spec.time_limit = 120.0;
            spec.catch_up = 0.0;
            spec.rival_armor = 200.0 + 4.0 * o.level as f64;
            spec.targets = scale3([4.0, 8.0, 13.0], &|t| (t * grow.min(1.5)).round());
        }
        Drift => {
            spec.time_limit = 90.0;
            spec.targets = scale3([2000.0, 4000.0, 6500.0], &|t| ((t * grow) / 100.0).round() * 100.0);
        }
        Destruction => {
            spec.weapons = true;
            spec.time_limit = 75.0;
            spec.targets = scale3([300000.0, 700000.0, 1200000.0], &|t| ((t * grow) / 10000.0).round() * 10000.0);
        }
        Stunt => {
            spec.time_limit = 90.0;
            spec.targets = scale3([6.0, 12.0, 20.0], &|t| (t * grow.min(1.5)).round());
        }
    }
    spec
}

fn shuffled(list: &[&'static str], rand: &mut Seeded) -> Vec<&'static str> {
    let mut copy = list.to_vec();
    for i in (1..copy.len()).rev() {
        let j = (rand.next() * (i + 1) as f64).floor() as usize;
        copy.swap(i, j.min(i));
    }
    copy
}

// ---------- the sandbox ----------

const ALL_KINDS: [EventKind; 11] = [
    Race,
    Race,
    Race,
    Elimination,
    Elimination,
    TimeAttack,
    Takedown,
    Drift,
    SpeedTrap,
    Destruction,
    Stunt,
];
const SANDBOX_BOSSES: [&str; 8] = [
    "The Phantom",
    "Nova",
    "Ghost Rider",
    "Mach Morgan",
    "Velvet Viper",
    "Captain Clutch",
    "Zero",
    "The Duke",
];

fn sandbox_level(n: usize) -> Level {
    let mut rand = Seeded::new(n as f64 * 104729.0 + 17.0);
    let past = n - LEVELS;
    let is_boss = past.is_multiple_of(10);
    let kind = if is_boss {
        Duel
    } else {
        ALL_KINDS[((rand.next() * ALL_KINDS.len() as f64) as usize).min(10)]
    };
    let tracks: Vec<&str> = MAP_NAMES.iter().copied().filter(|m| *m != "city").collect();
    let map = if is_city_kind(kind) {
        "city"
    } else {
        tracks[((rand.next() * tracks.len() as f64) as usize).min(tracks.len() - 1)]
    };
    let skill = (0.96 + past as f64 * 0.003).min(1.1);
    let upgrade = SANDBOX_MAX_UPGRADE.min(10 + (past / 4) as u32);
    let extra = if matches!(kind, Race | Elimination | Takedown) {
        2.min(past / 25)
    } else {
        0
    };
    let boss = if is_boss {
        Some((
            SANDBOX_BOSSES[(past / 10) % SANDBOX_BOSSES.len()].to_string(),
            rival_car(5, (upgrade + 2).min(SANDBOX_MAX_UPGRADE), &mut rand, true),
        ))
    } else {
        None
    };
    let night = map == "neon" || rand.next() < 0.25;
    let mut spec = build_spec(SpecOptions {
        kind,
        map,
        layout: if map == "city" { 0 } else { n as u32 },
        rand: &mut rand,
        skill,
        level: n,
        rivals: rival_count(kind, 10) + extra,
        rival_car: &mut |r| rival_car(5, upgrade, r, false),
        boss,
        night,
        modifiers: true,
        laps: None,
        catch_up: Some(0.4),
    });
    spec.title = if is_boss {
        format!("Sandbox boss: {}", spec.boss.clone().unwrap_or_default())
    } else {
        format!("Sandbox {n}")
    };
    Level {
        number: n,
        chapter: 11,
        spec,
        reward: base_reward(n),
        is_boss,
    }
}

// ---------- unlocking and rewards ----------

pub fn chapter_stars(save: &CareerSave, chapter: usize) -> u32 {
    (0..9).map(|i| save.stars[(chapter - 1) * 10 + i]).sum()
}

/// Why a level is locked, or "" if it can be played.
pub fn locked_reason(save: &CareerSave, n: usize) -> String {
    if n == 1 {
        return String::new();
    }
    if save.stars[n - 2] == 0 {
        return format!("Finish level {} first", n - 1);
    }
    if n.is_multiple_of(10) {
        let have = chapter_stars(save, n / 10);
        if have < BOSS_STARS {
            return format!("Needs {BOSS_STARS} ★ from this chapter (you have {have})");
        }
    }
    String::new()
}

pub fn next_level(save: &CareerSave) -> usize {
    save.stars.iter().position(|s| *s == 0).map_or(LEVELS, |i| i + 1)
}

#[derive(Clone, Debug, Default)]
pub struct Payout {
    pub coins: f64,
    pub lines: Vec<String>,
    pub new_stars: u32,
    pub first_clear: bool,
}

#[derive(Clone, Debug)]
pub struct LevelResult {
    pub stars: u32,
    pub score: f64,
    pub score_text: String,
    pub position: usize,
    pub lower_is_better: bool,
}

pub fn pay_level(save: &mut CareerSave, level: &Level, result: &LevelResult) -> Payout {
    let n = level.number;
    let base = level.reward;
    let stars = result.stars;
    let mut lines = Vec::new();
    let mut coins = (base * [0.15, 0.6, 0.8, 1.0][stars as usize]).round();
    lines.push(if stars > 0 {
        format!("{} finish: {coins}", "★".repeat(stars as usize))
    } else {
        format!("Entry: {coins}")
    });
    let mut new_stars = 0;
    let mut first_clear = false;
    if n <= LEVELS {
        let had = save.stars[n - 1];
        if stars > had {
            new_stars = stars - had;
            let bonus = (base * 0.2 * new_stars as f64).round();
            coins += bonus;
            lines.push(format!("New stars: +{bonus}"));
            if had == 0 {
                first_clear = true;
                let clear = (base * 0.5).round();
                coins += clear;
                lines.push(format!("First clear: +{clear}"));
            }
            save.stars[n - 1] = stars;
            save.best[n - 1] = result.score_text.clone();
            save.best_score[n - 1] = result.score;
        } else if stars == had && stars > 0 {
            let old = save.best_score[n - 1];
            let better = save.best[n - 1].is_empty()
                || old <= 0.0
                || if result.lower_is_better {
                    result.score < old
                } else {
                    result.score > old
                };
            if better {
                save.best[n - 1] = result.score_text.clone();
                save.best_score[n - 1] = result.score;
            }
        }
    } else if stars > 0 && n == save.sandbox_level {
        first_clear = true;
        let clear = (base * 0.5).round();
        coins += clear;
        lines.push(format!("Level cleared: +{clear}"));
        save.sandbox_level += 1;
        save.sandbox_best = save.sandbox_best.max(n);
    }
    if save.completions > 0 {
        let extra = (coins * (prestige(save) - 1.0)).round();
        coins += extra;
        lines.push(format!("Prestige ×{:.2}: +{extra}", prestige(save)));
    }
    coins = use_coin_rush(save, coins, &mut lines);
    save.coins += coins;
    save.earned += coins;
    save.events += 1;
    if result.position == 1 {
        save.wins += 1;
    }
    Payout {
        coins,
        lines,
        new_stars,
        first_clear,
    }
}

fn use_coin_rush(save: &mut CareerSave, coins: f64, lines: &mut Vec<String>) -> f64 {
    if save.coin_rush == 0 || coins <= 0.0 {
        return coins;
    }
    save.coin_rush -= 1;
    lines.push(format!("🎰 Coin rush ×2: +{coins}"));
    coins * 2.0
}

/// Starts the career again from level 1. `completed` counts it as a finished career.
pub fn reset_career(save: &CareerSave, completed: bool) -> CareerSave {
    let mut fresh = new_career(save.completions + u32::from(completed));
    fresh.events = save.events;
    fresh.wins = save.wins;
    fresh.earned = save.earned;
    fresh.sandbox_best = save.sandbox_best;
    fresh.xp = save.xp;
    fresh.spins = save.spins;
    fresh.stats = save.stats.clone();
    fresh.trophies = save.trophies.clone();
    fresh.found = save.found.clone();
    fresh.zones = save.zones.clone();
    fresh.nemesis = save.nemesis.clone();
    fresh
}

// ---------- the daily challenge ----------

/// Today's date as YYYY-MM-DD, local time.
pub fn today() -> String {
    date_string(0)
}

/// A local date `days_ago` days back, as YYYY-MM-DD.
fn date_string(days_ago: i64) -> String {
    let (y, m, d) = crate::clock::local_date(days_ago);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn daily_level(save: &CareerSave) -> Level {
    let date = today();
    let seed: f64 = date.replace('-', "").parse().unwrap_or(20260101.0);
    let mut rand = Seeded::new(seed);
    let reached = if save.mode == "sandbox" {
        save.sandbox_level
    } else {
        next_level(save)
    };
    let kinds = [Race, Elimination, TimeAttack, Takedown, Drift, SpeedTrap, Destruction, Stunt];
    let kind = kinds[((rand.next() * kinds.len() as f64) as usize).min(7)];
    let tracks: Vec<&str> = MAP_NAMES.iter().copied().filter(|m| *m != "city").collect();
    let map = if is_city_kind(kind) {
        "city"
    } else {
        tracks[((rand.next() * tracks.len() as f64) as usize).min(tracks.len() - 1)]
    };
    let chapter = 10.min(reached.div_ceil(10)).max(1);
    let ch = &CHAPTERS[chapter - 1];
    let progress = ((reached as f64 - 1.0) / (LEVELS - 1) as f64).min(1.0);
    let car = ch.cars[1];
    let up_level = MAX_UPGRADE.min(chapter as u32);
    let night = map == "neon" || rand.next() < 0.3;
    let mut spec = build_spec(SpecOptions {
        kind,
        map,
        layout: if map == "city" { 0 } else { 500 + (seed as u64 % 997) as u32 },
        rand: &mut rand,
        skill: 0.8 + 0.16 * progress,
        level: reached,
        rivals: rival_count(kind, chapter),
        rival_car: &mut |r| rival_car(car, up_level, r, false),
        boss: None,
        night,
        modifiers: true,
        laps: None,
        catch_up: None,
    });
    spec.title = format!("Daily challenge · {date}");
    Level {
        number: 0,
        chapter,
        spec,
        reward: base_reward(reached) * 2.0,
        is_boss: false,
    }
}

pub fn pay_daily(save: &mut CareerSave, level: &Level, stars: u32) -> Payout {
    let first = save.daily != today();
    let mut lines = Vec::new();
    let mut coins = (level.reward * if first { [0.1, 0.6, 0.8, 1.0][stars as usize] } else { 0.1 }).round();
    lines.push(if first {
        let s = "★".repeat(stars as usize);
        format!("Daily {}: {coins}", if s.is_empty() { "entry".to_string() } else { s })
    } else {
        format!("Already done today: {coins}")
    });
    if first && stars > 0 {
        save.streak = if save.daily == date_string(1) { save.streak + 1 } else { 1 };
        save.daily = today();
        let bonus = (coins * ((save.streak as f64 - 1.0) / 7.0).min(1.0)).round();
        if bonus > 0.0 {
            coins += bonus;
            lines.push(format!("{}-day streak: +{bonus}", save.streak));
        }
    }
    coins = use_coin_rush(save, coins, &mut lines);
    save.coins += coins;
    save.earned += coins;
    save.events += 1;
    Payout {
        coins,
        lines,
        new_stars: 0,
        first_clear: first && stars > 0,
    }
}

// ---------- quick race ----------

/// (label, skill, power, catch-up)
pub const DIFFICULTIES: [(&str, f64, f64, f64); 5] = [
    ("Easy", 0.8, 0.9, 1.0),
    ("Medium", 0.88, 0.97, 0.7),
    ("Hard", 0.95, 1.0, 0.4),
    ("Pro", 1.0, 1.04, 0.2),
    ("Insane", 1.05, 1.1, 0.0),
];

pub struct QuickRaceOptions {
    pub kind: EventKind,
    pub map: String,
    pub layout: u32,
    pub laps: u32,
    pub rivals: usize,
    pub difficulty: usize,
    pub car: CarConfig,
    /// The rivals' top speed in mph, or 0 to match the player's car.
    pub rival_top_speed: f64,
    pub night: bool,
    pub weapons: bool,
}

pub const RIVAL_TOP_SPEEDS: [f64; 9] = [0.0, 60.0, 90.0, 120.0, 150.0, 200.0, 300.0, 500.0, 1000.0];
pub const RIVAL_SPEED_KINDS: [EventKind; 4] = [Race, Duel, Elimination, Takedown];

pub fn quick_race_spec(o: &QuickRaceOptions) -> EventSpec {
    let d = DIFFICULTIES[o.difficulty.min(4)];
    let seed = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |t| t.as_millis())
        % 1_000_000_000) as f64;
    let mut rand = Seeded::new(seed);
    let mine = o.car.tuning.top_speed;
    let chosen = if RIVAL_SPEED_KINDS.contains(&o.kind) {
        o.rival_top_speed
    } else {
        0.0
    };
    let top = if chosen > 0.0 { chosen } else { mine * d.2 };
    let (hp_min, hp_max) = range("horsepower");
    let rival_tuning = Tuning {
        horsepower: if chosen > 0.0 {
            (o.car.tuning.horsepower * top / mine).clamp(hp_min, hp_max)
        } else {
            o.car.tuning.horsepower * d.2
        },
        top_speed: top,
        jump: 0.0,
        gravity: 1.0,
        infinite_nitro: false,
        ..o.car.tuning.clone()
    };
    let looks = |rand: &mut Seeded| {
        let mut c = rival_car(0, 0, rand, false);
        c.body = pick(&["sports", "muscle", "hatch", "pickup", "wedge"], rand).into();
        c.spoiler = pick(&["none", "lip", "wing", "gt"], rand).into();
        c.tuning = rival_tuning.clone();
        c
    };
    let rivals = if o.kind == Duel {
        1
    } else if matches!(o.kind, Race | Elimination | Takedown) {
        o.rivals
    } else {
        0
    };
    let level = (10.0 + o.difficulty as f64 * 25.0).round() as usize;
    let boss = if o.kind == Duel {
        let mut c = rival_car(0, 0, &mut rand, true);
        c.body = "wedge".into();
        c.tuning = rival_tuning.clone();
        Some(("The Champion".to_string(), c))
    } else {
        None
    };
    let mut spec = build_spec(SpecOptions {
        kind: o.kind,
        map: &o.map,
        layout: o.layout,
        rand: &mut rand,
        skill: d.1,
        level,
        rivals,
        rival_car: &mut |r| looks(r),
        boss,
        night: o.night,
        modifiers: false,
        laps: Some(o.laps),
        catch_up: Some(d.3),
    });
    if o.weapons {
        spec.weapons = true;
    }
    if o.kind == Elimination {
        spec.laps = 99;
    }
    spec.title = format!(
        "Quick {}",
        match o.kind {
            TimeAttack => "time attack".to_string(),
            SpeedTrap => "speed trap".to_string(),
            k => k.id().to_string(),
        }
    );
    spec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_the_same_every_time() {
        let a = make_level(37);
        let b = make_level(37);
        assert_eq!(a.spec.title, b.spec.title);
        assert_eq!(
            a.spec.rivals.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            b.spec.rivals.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
        );
        assert_eq!(make_level(10).spec.kind, EventKind::Duel);
        assert_eq!(make_level(150).chapter, 11);
    }

    #[test]
    fn old_saves_are_read() {
        let raw = serde_json::json!({"coins": 1234.7, "stars": [3, 2, 1, 1, 1], "upgrades": {"tyres": 4, "armour": 2}, "nemesis": {"Kiki Kerb": 2}, "owned": [0, 3, 99], "car": 3});
        let s = read_career(&raw);
        assert_eq!(s.coins, 1234.0);
        assert_eq!(s.stars[0], 3);
        assert_eq!(s.upgrades["tires"], 4);
        assert_eq!(s.upgrades["armor"], 2);
        assert_eq!(s.nemesis.get("Kiki Curb"), Some(&2.0));
        assert_eq!(s.owned, vec![0, 3]);
        assert_eq!(s.car, 3);
        assert_eq!(s.arsenal["gun"], 1); // from before weapons were bought
    }
}
