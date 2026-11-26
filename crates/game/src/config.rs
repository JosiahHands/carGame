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
