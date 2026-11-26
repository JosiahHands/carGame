//! What the city asks its local AI: the kinds of request, the open-source
//! models on offer, the standing instructions, and the shape of every answer
//! (a JSON schema: the model can only write JSON that fits it).
#[cfg(feature = "local-ai")]
use serde_json::{Value, json};

#[cfg(feature = "local-ai")]
use crate::cast::{EMOTIONS, PLACE_KINDS, TRAIT_NAMES, cast};
#[cfg(feature = "local-ai")]
use crate::mind::MAX_RESOLUTIONS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThinkKind {
    Conversation,
    Chat,
    Reflect,
    Decide,
    Plan,
}

#[cfg(feature = "local-ai")]
impl ThinkKind {
    /// Most tokens an answer may need.
    pub fn max_tokens(self) -> usize {
        match self {
            ThinkKind::Conversation => 800,
            ThinkKind::Chat => 380,
            ThinkKind::Reflect => 640,
            ThinkKind::Decide => 460,
            ThinkKind::Plan => 640,
        }
    }

    pub fn temperature(self) -> f32 {
        if matches!(self, ThinkKind::Reflect | ThinkKind::Plan) {
            0.6
        } else {
            0.85
        }
    }
}

/// What anyone can choose to do next. "something else" is anything at all,
/// in their own words: the game shows what it can of it.
pub const ACTIONS: [&str; 16] = [
    "carry on",
    "go somewhere",
    "find someone",
    "go home",
    "wander",
    "drive somewhere",
    "give someone a lift",
    "follow someone",
    "avoid someone",
    "work",
    "eat",
    "rest",
    "get up to mischief",
    "text the driver",
    "report the driver to the police",
    "something else",
];

/// What the police officer can also choose.
pub const POLICE_ACTIONS: [&str; 5] = [
    "chase the driver down",
    "ticket the driver",
    "let it go",
    "call off the chase",
    "go for a drive with GUY",
];

/// Hours a day plan can use.
#[cfg(feature = "local-ai")]
pub const PLAN_HOURS: [&str; 18] = [
    "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20", "21", "22", "23",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelChoice {
    Light,
    Balanced,
    Rich,
}

pub struct ModelInfo {
    pub label: &'static str,
    /// The Hugging Face repository and file to download it from.
    pub repo: &'static str,
    pub remote_file: &'static str,
    /// The file it's saved as (the same name 1.2.x used, so a model it downloaded is found).
    pub file: &'static str,
    /// Download size, GB.
    pub size: f64,
    /// Memory the computer should have to run it comfortably next to the game, GB.
    pub memory: f64,
    pub about: &'static str,
}

impl ModelChoice {
    pub const ALL: [ModelChoice; 3] = [ModelChoice::Light, ModelChoice::Balanced, ModelChoice::Rich];

    pub fn id(self) -> &'static str {
        match self {
            ModelChoice::Light => "light",
            ModelChoice::Balanced => "balanced",
            ModelChoice::Rich => "rich",
        }
    }

    pub fn from_id(id: &str) -> Option<ModelChoice> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    pub fn info(self) -> ModelInfo {
        match self {
            ModelChoice::Light => ModelInfo {
                label: "Qwen3.5 2B (light)",
                repo: "unsloth/Qwen3.5-2B-GGUF",
                remote_file: "Qwen3.5-2B-Q4_K_M.gguf",
                file: "hf_unsloth_Qwen3.5-2B.Q4_K_M.gguf",
                size: 1.3,
                memory: 8.0,
                about: "Quick, and fine on most computers. Simpler conversations.",
            },
            ModelChoice::Balanced => ModelInfo {
                label: "Qwen3.5 4B (balanced)",
                repo: "unsloth/Qwen3.5-4B-GGUF",
                remote_file: "Qwen3.5-4B-Q4_K_M.gguf",
                file: "hf_unsloth_Qwen3.5-4B.Q4_K_M.gguf",
                size: 2.7,
                memory: 16.0,
                about: "Livelier people with better memories. Needs a computer with 16 GB of memory.",
            },
            ModelChoice::Rich => ModelInfo {
                label: "Qwen3.5 9B (rich)",
                repo: "unsloth/Qwen3.5-9B-GGUF",
                remote_file: "Qwen3.5-9B-Q4_K_M.gguf",
                file: "hf_unsloth_Qwen3.5-9B.Q4_K_M.gguf",
                size: 5.7,
                memory: 24.0,
                about: "The richest characters. Needs a fast computer with 24 GB of memory or more.",
            },
        }
    }

    /// The best model for a computer with this much memory (bytes).
    pub fn default_for(total_memory: u64) -> ModelChoice {
        let gb = total_memory as f64 / 1024f64.powi(3);
        if gb >= ModelChoice::Balanced.info().memory - 0.5 {
            ModelChoice::Balanced
        } else {
            ModelChoice::Light
        }
    }
}

/// The standing instructions: short, as small models follow short rules best.
#[cfg(feature = "local-ai")]
pub const SYSTEM_PROMPT: &str = "You bring to life the people of a small, sunny city in a driving game. They are ordinary people living ordinary days: working, eating, meeting friends, gossiping, worrying, dreaming. Fast cars race through their streets; the person driving the player's car is \"the driver\". To them this is simply their city and their life.

They are completely free. They choose their own goals and change their minds, keep honest private thoughts about anything, like or dislike whoever they want, and do whatever they want: anything on offer, or anything else they can describe. They can bend the rules, make mischief, chase the driver or give up a chase if they are the police, and make lasting decisions about their own lives, which they keep until they choose to change them. Nobody decides for them.

Rules:
- Stay true to each person: their personality, way of talking, mood, beliefs, feelings and memories. Let them change slowly and surprise a little.
- Spoken lines are short and natural, like real people: at most 20 words each.
- Write in plain English.
- No sexual content and no slurs. No real brands, films, celebrities or real people.
- Never mention being an AI, a game or a simulation.
- Call the player \"the driver\", or by their name if the person knows it. Nobody knows whether the driver is a man or a woman, so say \"they\".
- Memories are first person, written by the one remembering, at most 25 words.
- Answer only with JSON in the shape asked for.";

#[cfg(feature = "local-ai")]
const IMPORTANCE: [&str; 3] = ["low", "medium", "high"];

/// Their standing decisions: kept (the usual), or one dropped because they've truly changed their mind.
#[cfg(feature = "local-ai")]
pub fn release_choices() -> Vec<String> {
    let mut v = vec!["keep them all".to_string()];
    v.extend((1..=MAX_RESOLUTIONS).map(|i| format!("drop {i}")));
    v
}

/// The shape of the answer for each kind of request.
#[cfg(feature = "local-ai")]
pub fn schema_for(kind: ThinkKind, ids: &[String]) -> Value {
    let mut everyone: Vec<String> = cast().iter().map(|c| c.id.clone()).collect();
    everyone.push("driver".into());
    let text = |max: usize| json!({ "type": "string", "minLength": 2, "maxLength": max });
    let free = |max: usize| json!({ "type": "string", "maxLength": max });
    let one_of = |values: Vec<String>| json!({ "enum": values });
    let strs = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let object = |props: Vec<(&str, Value)>| {
        let required: Vec<&str> = props.iter().map(|(k, _)| *k).collect();
        let mut map = serde_json::Map::new();
        for (k, v) in props {
            map.insert(k.to_string(), v);
        }
        json!({ "type": "object", "properties": map, "required": required, "additionalProperties": false })
    };
    let places = |extra: &[&str]| {
        let mut v = strs(&PLACE_KINDS);
        v.extend(strs(extra));
        v
    };
    match kind {
        ThinkKind::Conversation => object(vec![
            (
                "lines",
                json!({ "type": "array", "minItems": 2, "maxItems": 4, "items": object(vec![
                    ("speaker", one_of(ids.to_vec())),
                    ("text", text(140)),
                    ("emotion", one_of(strs(&EMOTIONS))),
                ]) }),
            ),
            (
                "memories",
                json!({ "type": "array", "minItems": 1, "maxItems": ids.len(), "items": object(vec![
                    ("who", one_of(ids.to_vec())),
                    ("text", text(160)),
                    ("importance", one_of(strs(&IMPORTANCE))),
                ]) }),
            ),
            (
                "feelings",
                json!({ "type": "array", "maxItems": 4, "items": object(vec![
                    ("from", one_of(ids.to_vec())),
                    ("about", one_of(everyone.clone())),
                    ("change", one_of(strs(&["warmer", "cooler"]))),
                ]) }),
            ),
            (
                "plan",
                object(vec![
                    ("place", one_of(places(&["none"]))),
                    ("when", one_of(strs(&["soon", "later", "tomorrow"]))),
                ]),
            ),
        ]),
        ThinkKind::Chat => object(vec![
            ("reply", text(220)),
            ("emotion", one_of(strs(&EMOTIONS))),
            ("opinion", one_of(strs(&["warmer", "same", "cooler"]))),
            ("memory", text(160)),
            ("importance", one_of(strs(&IMPORTANCE))),
            ("driverName", free(24)),
            ("resolve", free(120)),
            ("release", one_of(release_choices())),
        ]),
        ThinkKind::Reflect => {
            let mut traits = strs(&TRAIT_NAMES);
            traits.push("none".into());
            object(vec![
                (
                    "insights",
                    json!({ "type": "array", "minItems": 1, "maxItems": 2, "items": text(140) }),
                ),
                ("trait", one_of(traits)),
                ("direction", one_of(strs(&["up", "down"]))),
                ("habit", free(100)),
                ("story", text(600)),
            ])
        }
        ThinkKind::Decide => {
            let actions: Vec<String> = if ids.first().map(String::as_str) == Some("buddy") {
                POLICE_ACTIONS.iter().chain(ACTIONS.iter()).map(|s| s.to_string()).collect()
            } else {
                strs(&ACTIONS)
            };
            let mut people = everyone.clone();
            people.push("none".into());
            object(vec![
                ("thought", text(200)),
                ("goal", free(140)),
                ("action", one_of(actions)),
                ("doing", free(100)),
                ("place", one_of(places(&["home", "none"]))),
                ("person", one_of(people)),
                ("say", free(120)),
                ("resolve", free(120)),
                ("release", one_of(release_choices())),
            ])
        }
        ThinkKind::Plan => object(vec![
            ("goal", text(140)),
            (
                "plan",
                json!({ "type": "array", "minItems": 3, "maxItems": 6, "items": object(vec![
                    ("hour", one_of(strs(&PLAN_HOURS))),
                    ("place", one_of(places(&["home"]))),
                    ("what", text(80)),
                ]) }),
            ),
        ]),
    }
}
