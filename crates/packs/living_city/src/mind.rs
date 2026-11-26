//! What goes on inside each cast member: what they remember, what they've
//! come to believe, how they feel about everyone (you included), what they
//! need right now, and how their personality is drifting.
//!
//! Memory works like the "generative agents" research: every experience is
//! written down with how important it was, and when they need to think, the
//! memories that are recent, important and about the people involved come
//! back first. Now and then they reflect on what they've been through, which
//! turns a pile of memories into a belief, a nudge to their personality, or
//! a new habit. They also learn which places they enjoy.
//!
//! Saved exactly as 1.2.x saved it (the same JSON), so cities carry over.
use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cast::{CastMember, TRAIT_NAMES, Traits, cast};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Memory {
    /// When, in city time (seconds the city has been running).
    #[serde(default)]
    pub t: f64,
    #[serde(default)]
    pub text: String,
    /// 1 (forgettable) to 10 (life-changing).
    #[serde(default)]
    pub importance: f64,
    /// Who it's about: cast ids, or "driver" for you.
    #[serde(default)]
    pub about: Vec<String>,
    /// saw, talk, heard, chat or insight.
    #[serde(default)]
    pub kind: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Needs {
    /// 1 is fully satisfied, 0 is desperate.
    pub social: f64,
    pub food: f64,
    pub rest: f64,
    pub fun: f64,
}

/// One step of a day plan: from this hour, be here, doing this.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DayStep {
    pub hour: f64,
    pub place: String,
    #[serde(default)]
    pub what: String,
}

/// One exchange with the driver.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ChatLine {
    #[serde(default)]
    pub t: f64,
    pub you: String,
    pub them: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Mind {
    pub id: String,
    pub traits: Traits,
    pub memories: Vec<Memory>,
    /// Beliefs they've come to, newest last.
    pub insights: Vec<String>,
    /// Little habits picked up along the way.
    pub habits: Vec<String>,
    /// How they feel about each person (and "driver"): -1 can't stand them .. 1 adore them.
    pub feelings: HashMap<String, f64>,
    /// How many times they've talked to each person.
    pub met: HashMap<String, f64>,
    /// How much they've learned they enjoy each kind of place, -1..1.
    pub likes: HashMap<String, f64>,
    pub needs: Needs,
    /// Overall mood, -1..1, and the feeling on their face.
    pub mood: f64,
    pub emotion: String,
    /// Importance piled up since they last reflected.
    pub pending: f64,
    /// The driver's name, once the driver has told them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_name: Option<String>,
    /// Their life so far, in their own words.
    pub story: String,
    /// Their talks with the driver, newest last.
    pub chats: Vec<ChatLine>,
    /// What they're aiming for, in their own words.
    pub goal: String,
    /// Their plan for the day, and which city day it's for.
    pub plan: Vec<DayStep>,
    pub plan_day: f64,
    /// Their latest private thoughts, newest last.
    pub thoughts: Vec<String>,
    /// Standing decisions, in their own words, kept until they change their mind.
    pub resolutions: Vec<String>,
}

const MAX_MEMORIES: usize = 200;
const MAX_CHATS: usize = 40;
const MAX_STORY: usize = 700;
const MAX_INSIGHTS: usize = 6;
const MAX_HABITS: usize = 4;
const MAX_THOUGHTS: usize = 8;
/// How many standing decisions someone can hold at once.
pub const MAX_RESOLUTIONS: usize = 5;

/// The first `n` characters of some text.
pub fn cut(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// Spaces squeezed to single spaces, and trimmed.
pub fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Mind {
    pub fn new(member: &CastMember) -> Mind {
        let mut feelings = HashMap::from([("driver".to_string(), 0.0)]);
        for other in cast() {
            if other.id != member.id {
                feelings.insert(other.id.clone(), if member.friends.contains(&other.id) { 0.45 } else { 0.05 });
            }
        }
        if let Some(bond) = member.bond {
            feelings.insert(bond.to_string(), 0.9);
        }
        for other in cast() {
            if other.bond == Some(member.id.as_str()) {
                feelings.insert(other.id.clone(), 0.9);
            }
        }
        let mut likes: HashMap<String, f64> = member.likes.iter().map(|k| (k.to_string(), 0.4)).collect();
        if let Some(job) = member.job {
            let v = likes.get(job).copied().unwrap_or(0.0).max(0.15);
            likes.insert(job.to_string(), v);
        }
        let r = game::rng::random;
        Mind {
            id: member.id.clone(),
            traits: member.traits,
            feelings,
            likes,
            needs: Needs {
                social: 0.6 + r() * 0.3,
                food: 0.5 + r() * 0.4,
                rest: 0.8,
                fun: 0.5 + r() * 0.4,
            },
            mood: 0.3,
            emotion: "calm".into(),
            plan_day: -1.0,
            ..Mind::default()
        }
    }

    /// Keeps a saved mind sound when loading (old saves, or a cast that changed).
    pub fn restore(member: &CastMember, saved: Option<&Value>) -> Mind {
        let mut fresh = Mind::new(member);
        let Some(saved) = saved.filter(|v| v.is_object()) else {
            return fresh;
        };
        // Field by field, so one odd value doesn't lose the rest.
        let get = |k: &str| saved.get(k);
        if let Some(t) = get("traits").and_then(Value::as_object) {
            for name in TRAIT_NAMES {
                let key = if name == "humor" && !t.contains_key("humor") {
                    "humour"
                } else {
                    name
                };
                if let Some(v) = t.get(key).and_then(Value::as_f64) {
                    *fresh.traits.get_mut(name) = v;
                }
            }
        }
        fn list<T: serde::de::DeserializeOwned>(v: Option<&Value>, max: usize) -> Vec<T> {
            let items: Vec<T> = v
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| serde_json::from_value(x.clone()).ok()).collect())
                .unwrap_or_default();
            let skip = items.len().saturating_sub(max);
            items.into_iter().skip(skip).collect()
        }
        fn map(v: Option<&Value>) -> HashMap<String, f64> {
            v.and_then(Value::as_object)
                .map(|o| o.iter().filter_map(|(k, v)| v.as_f64().map(|n| (k.clone(), n))).collect())
                .unwrap_or_default()
        }
        fresh.memories = list(get("memories"), MAX_MEMORIES);
        fresh.insights = list(get("insights"), MAX_INSIGHTS);
        fresh.habits = list(get("habits"), MAX_HABITS);
        fresh.feelings.extend(map(get("feelings")));
        fresh.met = map(get("met"));
        fresh.likes.extend(map(get("likes")));
        if let Some(m) = get("mood").and_then(Value::as_f64) {
            fresh.mood = m;
        }
        fresh.driver_name = get("driverName").and_then(Value::as_str).map(|s| cut(s, 24));
        fresh.story = get("story").and_then(Value::as_str).map(|s| cut(s, MAX_STORY)).unwrap_or_default();
        fresh.chats = list(get("chats"), MAX_CHATS);
        if let Some(n) = get("needs").and_then(Value::as_object) {
            let set = |k: &str, v: &mut f64| {
                if let Some(x) = n.get(k).and_then(Value::as_f64) {
                    *v = x;
                }
            };
            set("social", &mut fresh.needs.social);
            set("food", &mut fresh.needs.food);
            set("rest", &mut fresh.needs.rest);
            set("fun", &mut fresh.needs.fun);
        }
        fresh.goal = get("goal").and_then(Value::as_str).map(|s| cut(s, 140)).unwrap_or_default();
        let mut plan: Vec<DayStep> = list(get("plan"), 100);
        plan.truncate(8);
        fresh.plan = plan;
        fresh.plan_day = get("planDay").and_then(Value::as_f64).unwrap_or(-1.0);
        fresh.thoughts = list(get("thoughts"), MAX_THOUGHTS);
        let resolutions: Vec<String> = list(get("resolutions"), MAX_RESOLUTIONS);
        fresh.resolutions = resolutions.iter().map(|r| cut(r, 120)).collect();
        if let Some(p) = get("pending").and_then(Value::as_f64) {
            fresh.pending = p;
        }
        if let Some(e) = get("emotion").and_then(Value::as_str) {
            fresh.emotion = e.to_string();
        }
        fresh
    }

    pub fn feeling(&self, who: &str) -> f64 {
        self.feelings.get(who).copied().unwrap_or(0.0)
    }

    pub fn times_met(&self, who: &str) -> f64 {
        self.met.get(who).copied().unwrap_or(0.0)
    }

    pub fn like_of(&self, kind: &str) -> f64 {
        self.likes.get(kind).copied().unwrap_or(0.0)
    }

    /// Writes down an experience. Very similar recent ones are merged, so a long chase isn't fifty memories.
    pub fn remember(&mut self, now: f64, text: &str, importance: f64, about: &[&str], kind: &str) {
        let clean = cut(text.trim(), 160);
        if clean.is_empty() {
            return;
        }
        if let Some(last) = self.memories.last_mut()
            && last.text == clean
            && now - last.t < 60.0
        {
            last.importance = (last.importance + 1.0).min(10.0);
            last.t = now;
            return;
        }
        self.memories.push(Memory {
            t: now,
            text: clean,
            importance: importance.round().clamp(1.0, 10.0),
            about: about.iter().map(|s| s.to_string()).collect(),
            kind: kind.to_string(),
        });
        self.pending += importance;
        // Forget the least worth keeping: old and unimportant.
        if self.memories.len() > MAX_MEMORIES {
            let mut worst = 0;
            let mut worst_score = f64::INFINITY;
            for (i, m) in self.memories.iter().enumerate() {
                let score = m.importance - (now - m.t) / 600.0;
                if score < worst_score {
                    worst_score = score;
                    worst = i;
                }
            }
            self.memories.remove(worst);
        }
    }

    /// The memories that come to mind: recent ones, important ones, and ones
    /// about these people or mentioning these words, best first.
    pub fn recall(&self, now: f64, about: &[&str], words: &[&str], count: usize) -> Vec<&Memory> {
        let lowered: Vec<String> = words.iter().map(|w| w.to_lowercase()).filter(|w| w.chars().count() > 3).collect();
        let score = |m: &Memory| {
            let recency = 0.995f64.powf((now - m.t) / 60.0);
            let relevance = m.about.iter().filter(|a| about.contains(&a.as_str())).count() as f64 * 0.6
                + lowered.iter().filter(|w| m.text.to_lowercase().contains(w.as_str())).count() as f64 * 0.3;
            recency + m.importance / 10.0 + relevance
        };
        let mut all: Vec<(f64, &Memory)> = self.memories.iter().map(|m| (score(m), m)).collect();
        all.sort_by(|a, b| b.0.total_cmp(&a.0));
        all.into_iter().take(count).map(|(_, m)| m).collect()
    }

    /// Changes how they feel about someone.
    pub fn feel(&mut self, who: &str, change: f64) {
        let v = (self.feeling(who) + change).clamp(-1.0, 1.0);
        self.feelings.insert(who.to_string(), v);
    }

    /// Nudges a trait: personalities drift slowly with experience.
    pub fn nudge(&mut self, name: &str, up: bool, amount: f64) {
        let t = self.traits.get_mut(name);
        *t = (*t + if up { amount } else { -amount }).clamp(0.03, 0.98);
    }

    pub fn add_insight(&mut self, now: f64, insight: &str) {
        let text = cut(insight.trim(), 140);
        if text.is_empty() || self.insights.contains(&text) {
            return;
        }
        self.insights.push(text.clone());
        if self.insights.len() > MAX_INSIGHTS {
            self.insights.remove(0);
        }
        self.remember(now, &text, 7.0, &[], "insight");
    }

    pub fn add_habit(&mut self, habit: &str) {
        let text = cut(habit.trim(), 100);
        if text.is_empty() || self.habits.contains(&text) {
            return;
        }
        self.habits.push(text);
        if self.habits.len() > MAX_HABITS {
            self.habits.remove(0);
        }
    }

    /// Learns from a visit: a place that lifted their mood is liked a little more next time.
    pub fn learn_place(&mut self, kind: &str, reward: f64) {
        let now = self.like_of(kind);
        self.likes.insert(kind.to_string(), (now + 0.25 * (reward - now)).clamp(-1.0, 1.0));
    }

    /// Needs run down over time; mood follows how well they're doing.
    pub fn tick_needs(&mut self, dt: f64, social: bool, eating: bool, resting: bool, fun: bool) {
        let minutes = dt / 60.0;
        let t = self.traits;
        let n = &mut self.needs;
        n.social = (n.social + if social { 0.25 } else { -0.025 * (0.5 + t.energy) } * minutes).clamp(0.0, 1.0);
        n.food = (n.food + if eating { 0.35 } else { -0.02 } * minutes).clamp(0.0, 1.0);
        n.rest = (n.rest + if resting { 0.15 } else { -0.012 * (1.5 - t.energy) } * minutes).clamp(0.0, 1.0);
        n.fun = (n.fun + if fun { 0.25 } else { -0.02 * (0.5 + t.curiosity) } * minutes).clamp(0.0, 1.0);
        let wellbeing = (n.social + n.food + n.rest + n.fun) / 4.0 - 0.45;
        self.mood += (wellbeing * 1.6 - self.mood) * (minutes * 0.5).min(1.0);
        self.mood = self.mood.clamp(-1.0, 1.0);
    }

    /// A shock or a treat: moves the mood straight away (it settles back over time).
    pub fn jolt(&mut self, change: f64, emotion: &str) {
        self.mood = (self.mood + change).clamp(-1.0, 1.0);
        self.emotion = emotion.to_string();
    }

    /// Writes down a talk with the driver.
    pub fn log_chat(&mut self, now: f64, you: &str, them: &str) {
        self.chats.push(ChatLine {
            t: now,
            you: cut(you, 300),
            them: cut(them, 300),
        });
        if self.chats.len() > MAX_CHATS {
            self.chats.remove(0);
        }
        *self.met.entry("driver".into()).or_insert(0.0) += 1.0;
    }

    /// A private thought.
    pub fn think(&mut self, thought: &str) {
        let text = cut(&squeeze(thought), 200);
        if text.is_empty() || self.thoughts.last() == Some(&text) {
            return;
        }
        self.thoughts.push(text);
        if self.thoughts.len() > MAX_THOUGHTS {
            self.thoughts.remove(0);
        }
    }

    /// A new goal, in their words ("" keeps the old one).
    pub fn set_goal(&mut self, goal: &str) {
        let text = cut(&squeeze(goal), 140);
        if text.chars().count() > 3 {
            self.goal = text;
        }
    }

    /// A standing decision, made for themselves. Returns whether it's new.
    /// With no room left, a new one isn't taken until they drop one.
    pub fn resolve(&mut self, now: f64, decision: &str) -> bool {
        let text = cut(&squeeze(decision), 120);
        if text.chars().count() < 6 || self.resolutions.iter().any(|r| r.to_lowercase() == text.to_lowercase()) {
            return false;
        }
        if self.resolutions.len() >= MAX_RESOLUTIONS {
            return false;
        }
        self.resolutions.push(text.clone());
        self.remember(now, &format!("I made up my mind: {text}"), 8.0, &[], "insight");
        true
    }

    /// Changes their mind about a standing decision (by its number, from 1). Returns what they let go of.
    pub fn release(&mut self, now: f64, number: usize) -> String {
        if number == 0 || number > self.resolutions.len() {
            return String::new();
        }
        let text = self.resolutions.remove(number - 1);
        self.remember(now, &format!("I changed my mind about \"{text}\""), 7.0, &[], "insight");
        text
    }

    pub fn room_to_resolve(&self) -> bool {
        self.resolutions.len() < MAX_RESOLUTIONS
    }

    /// Their life story, rewritten after reflecting.
    pub fn set_story(&mut self, story: &str) {
        let text = cut(&squeeze(story), MAX_STORY);
        if text.chars().count() > 20 {
            self.story = text;
        }
    }

    /// Whether one of their standing decisions rules this kind of thing out
    /// right now. `context` is what's going on: a decision with a condition
    /// ("I won't chase the driver unless they're speeding") holds or not
    /// depending on it. Only the part before the condition says what's ruled out.
    pub fn forbids(&self, kind: Vow, context: &str) -> bool {
        let re = res();
        self.resolutions.iter().any(|decision| {
            let r = re.even_if.replace(decision, "").to_string();
            let unless = re.unless.find(&r);
            let only_if = if unless.is_some() { None } else { re.only_if.find(&r) };
            let cut_at = unless.or(only_if);
            let rule = cut_at.map_or(r.as_str(), |m| &r[..m.start()]);
            if !re.vow(kind).is_match(rule) || !re.not.is_match(rule) || re.never_stop.is_match(rule) {
                return false;
            }
            // Matching words can't be sure what a condition means, so when in
            // doubt the promise is kept.
            if let Some(m) = unless {
                let condition = &r[m.end()..];
                return re.negated.is_match(condition) || overlap(condition, context) != Overlap::All;
            }
            if let Some(m) = only_if {
                let condition = &r[m.end()..];
                return re.negated.is_match(condition) || overlap(condition, context) != Overlap::None;
            }
            true
        })
    }
}

/// The kinds of thing a standing decision can rule out, by the words it uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vow {
    Chase,
    Ticket,
    Work,
    Mischief,
    Text,
    Report,
    Drive,
    Ride,
}

struct Res {
    vows: Vec<Regex>,
    not: Regex,
    never_stop: Regex,
    unless: Regex,
    only_if: Regex,
    even_if: Regex,
    negated: Regex,
}

impl Res {
    fn vow(&self, kind: Vow) -> &Regex {
        &self.vows[kind as usize]
    }
}

fn res() -> &'static Res {
    static RES: OnceLock<Res> = OnceLock::new();
    RES.get_or_init(|| {
        let r = |s: &str| Regex::new(s).expect("a valid pattern");
        Res {
            vows: [
                r"(?i)\bchas|\bpursu",
                r"(?i)\bticket|\bfin(e|ing)\b",
                r"(?i)\bwork|\bjob\b|\bshifts?\b",
                r"(?i)mischief|prank|trouble|vandal",
                r"(?i)\btext|\bmessag|\bphone",
                r"(?i)\breport|snitch|call (the )?police|call buddy",
                r"(?i)\bdriv(e|es|ing)\b|\bcars?\b",
                r"(?i)\bride|\blift|police car",
            ]
            .into_iter()
            .map(r)
            .collect(),
            not: r(
                r"(?i)\b(no|never|not|won't|wont|don't|dont|stop|stopping|quit|quitting|refuse|swear off|no more|give up|giving up|avoid|without)\b|\b(done|finished|through) (with|\w+ing)\b|n't\b",
            ),
            never_stop: r(r"(?i)\b(never|won't|wont|not|don't|dont)\s+(stop|quit|give up)\b"),
            unless: r(r"(?i)\b(unless|except|apart from|but not if|but not when)\b"),
            only_if: r(r"(?i)\b(if|when|whenever|while|as long as|so long as)\b"),
            even_if: r(r"(?is)\beven (if|when|though)\b.*$"),
            negated: r(r"(?i)\b(not|no|never|without|nobody|nothing)\b|n't\b"),
        }
    })
}

const COMMON: [&str; 61] = [
    "the",
    "and",
    "you",
    "are",
    "not",
    "but",
    "for",
    "she",
    "her",
    "his",
    "him",
    "was",
    "who",
    "its",
    "any",
    "all",
    "get",
    "got",
    "one",
    "out",
    "way",
    "they",
    "them",
    "their",
    "theyre",
    "driver",
    "drivers",
    "again",
    "really",
    "very",
    "that",
    "this",
    "with",
    "what",
    "there",
    "would",
    "could",
    "should",
    "about",
    "being",
    "going",
    "does",
    "doing",
    "anyone",
    "someone",
    "something",
    "everyone",
    "actually",
    "start",
    "starts",
    "keep",
    "keeps",
    "while",
    "when",
    "into",
    "from",
    "then",
    "just",
    "the",
    "and",
    "you",
];

#[derive(PartialEq, Eq)]
enum Overlap {
    All,
    Some,
    None,
}

/// How much of a condition turns up in what's going on (by the first five
/// letters of each word of three or more).
fn overlap(condition: &str, context: &str) -> Overlap {
    let text = context.to_lowercase();
    let cleaned: String = condition
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_whitespace())
        .collect();
    let mut words: Vec<String> = Vec::new();
    for w in cleaned.split(|c: char| !c.is_ascii_lowercase()) {
        if w.len() >= 3 && !COMMON.contains(&w) {
            words.push(w.to_string());
        }
    }
    let found = words.iter().filter(|w| text.contains(&w[..w.len().min(5)])).count();
    if found == 0 {
        Overlap::None
    } else if found == words.len() {
        Overlap::All
    } else {
        Overlap::Some
    }
}

/// A trait in words, for prompts and the journal.
pub fn trait_words(traits: &Traits) -> String {
    let words = [
        ["prickly", "decent", "very kind"],
        ["timid", "steady", "fearless"],
        ["set in their ways", "interested", "endlessly curious"],
        ["serious", "good-humored", "a joker"],
        ["sleepy", "lively", "bursting with energy"],
    ];
    TRAIT_NAMES
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let v = traits.get(t);
            words[i][if v < 0.35 {
                0
            } else if v < 0.7 {
                1
            } else {
                2
            }]
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// How they feel about someone, in words.
pub fn feeling_word(value: f64) -> &'static str {
    if value > 0.75 {
        "adores"
    } else if value > 0.4 {
        "likes"
    } else if value > 0.12 {
        "is warming to"
    } else if value > -0.12 {
        "is neutral about"
    } else if value > -0.4 {
        "is wary of"
    } else if value > -0.75 {
        "dislikes"
    } else {
        "can't stand"
    }
}

pub fn mood_word(mind: &Mind) -> &'static str {
    let m = mind.mood;
    if m > 0.5 {
        "great"
    } else if m > 0.15 {
        "good"
    } else if m > -0.15 {
        "okay"
    } else if m > -0.5 {
        "low"
    } else {
        "awful"
    }
}

/// Where you stand with them, from strangers to best friends.
pub fn friendship(mind: &Mind) -> &'static str {
    let like = mind.feeling("driver");
    let talks = mind.times_met("driver");
    if like < -0.5 {
        "enemy"
    } else if like < -0.15 {
        "wary"
    } else if talks == 0.0 {
        "stranger"
    } else if like < 0.3 || talks < 3.0 {
        "acquaintance"
    } else if like < 0.55 || talks < 6.0 {
        "friend"
    } else if like < 0.8 || talks < 12.0 {
        "close friend"
    } else {
        "best friend"
    }
}

/// Friends and better.
pub fn is_friend(mind: &Mind) -> bool {
    matches!(friendship(mind), "friend" | "close friend" | "best friend")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(decisions: &[&str]) -> Mind {
        Mind {
            resolutions: decisions.iter().map(|s| s.to_string()).collect(),
            ..Mind::default()
        }
    }

    #[test]
    fn standing_decisions_are_kept() {
        assert!(with(&["I won't chase the driver any more"]).forbids(Vow::Chase, ""));
        assert!(!with(&["I will never stop chasing the driver"]).forbids(Vow::Chase, ""));
        assert!(!with(&["I'm going to chase the driver"]).forbids(Vow::Chase, ""));
        // An exception lifts it only when all of it is happening.
        let m = with(&["No more chases unless they hit a pedestrian"]);
        assert!(m.forbids(Vow::Chase, "speeding"));
        assert!(!m.forbids(Vow::Chase, "hitting a pedestrian"));
        // "Only if" holds only when it's happening.
        let m = with(&["I won't work if it rains"]);
        assert!(!m.forbids(Vow::Work, "sunny"));
        assert!(m.forbids(Vow::Work, "it rains hard"));
        assert!(with(&["Done with tickets, even if they speed"]).forbids(Vow::Ticket, "speeding"));
    }

    #[test]
    fn old_minds_load() {
        let member = crate::cast::member("guy").unwrap();
        let saved = serde_json::json!({
            "traits": { "kindness": 0.9, "humour": 0.2 },
            "memories": [{ "t": 5, "text": "Hi", "importance": 3, "about": ["driver"], "kind": "saw" }],
            "driverName": "Jo",
            "planDay": 3,
            "feelings": { "driver": 0.5 }
        });
        let m = Mind::restore(member, Some(&saved));
        assert_eq!(m.traits.humor, 0.2);
        assert_eq!(m.memories.len(), 1);
        assert_eq!(m.driver_name.as_deref(), Some("Jo"));
        assert_eq!(m.plan_day, 3.0);
        assert_eq!(m.feeling("driver"), 0.5);
        assert_eq!(m.feeling("buddy"), 0.9);
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["driverName"], "Jo");
        assert!(back.get("planDay").is_some());
    }
}
