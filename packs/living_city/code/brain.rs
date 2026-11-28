//! How the city's people think. Two brains with the same job:
//!  - the local AI (an open-source model running on this computer, see ai.rs):
//!    here are the prompts it's given and how its answers are read.
//!  - the rule brain needs nothing: it fills in phrases from what they
//!    remember and how they feel. It keeps the city going while the model
//!    downloads or loads, when the AI is busy, and when it's switched off.
//!
//! Both give the same results, and everything the AI writes is checked and
//! trimmed before the city uses it.
use serde_json::Value;

use crate::cast::{CastMember, EMOTIONS, PLACE_KINDS, TRAIT_NAMES, member, name_of};
use crate::mind::{Mind, Vow, cut, feeling_word, friendship, mood_word, squeeze, trait_words};
use crate::protocol::{ACTIONS, POLICE_ACTIONS};
use game::rng::random;

/// A person as the brain sees them.
pub struct Person<'a> {
    pub member: &'a CastMember,
    pub mind: &'a Mind,
    /// Where they are ("the coffee cart") and what they're up to.
    pub place: String,
    pub doing: String,
    /// How the driver is talking to them: beside them, on the phone, or giving them a ride.
    pub contact: Option<Contact>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contact {
    Beside,
    Phone,
    Passenger,
}

/// The moment: time of day, what's been happening, who the driver is.
#[derive(Clone, Debug)]
pub struct Moment {
    pub now: f64,
    /// The time in the city, in words ("Tuesday, late afternoon").
    pub time: String,
    /// The hour of the day, 0..24.
    pub hour: f64,
    /// The player's car, in words ("a red sports car").
    pub car: String,
    /// Things that happened in the city lately, newest first.
    pub news: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub speaker: String,
    pub text: String,
    pub emotion: String,
}

/// A plan two people make to meet again.
#[derive(Clone, Debug)]
pub struct MeetPlan {
    pub place: String,
    /// soon, later or tomorrow
    pub when: String,
}

#[derive(Clone, Debug, Default)]
pub struct ConversationResult {
    pub lines: Vec<Line>,
    pub memories: Vec<(String, String, f64)>,
    /// from, about, change (+1 warmer, -1 cooler)
    pub feelings: Vec<(String, String, f64)>,
    pub plan: Option<MeetPlan>,
    /// Whether the AI wrote it (shown in the journal).
    pub ai: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ChatResult {
    pub reply: String,
    pub emotion: String,
    /// How their opinion of the driver moved: -1, 0 or +1.
    pub opinion: f64,
    pub memory: String,
    pub importance: f64,
    pub driver_name: String,
    /// A lasting decision the talk led them to ("" for none), and the number of one they dropped.
    pub resolve: String,
    pub release: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct ReflectResult {
    pub insights: Vec<String>,
    pub nudge: Option<(&'static str, bool)>,
    pub habit: String,
    /// Their life story so far, rewritten ("" to keep the old one).
    pub story: String,
}

/// How a chase is going, as BUDDY sees it when he thinks it over.
#[derive(Clone, Debug, Default)]
pub struct ChaseState {
    pub seconds: f64,
    pub distance: f64,
    /// How battered the police car is, 0..1.
    pub hurt: f64,
    /// Seconds since the driver last did anything wrong.
    pub quiet: f64,
    /// What GUY, riding along, says about it ("" if he's not there or keeps quiet).
    pub partner: String,
}

/// What's going on when someone decides what to do next.
#[derive(Clone, Debug, Default)]
pub struct Situation {
    pub why: String,
    /// Who's around (names).
    pub nearby: Vec<String>,
    /// For the police officer: what the driver has done lately, and how bad it is (0..5).
    pub crimes: Option<(Vec<String>, f64)>,
    pub chase: Option<ChaseState>,
}

impl Situation {
    pub fn new(why: impl Into<String>, nearby: Vec<String>) -> Self {
        Situation {
            why: why.into(),
            nearby,
            ..Default::default()
        }
    }

    /// What's going on, in words, for standing decisions with conditions.
    pub fn context(&self) -> String {
        let mut parts = vec![self.why.clone()];
        if let Some((list, _)) = &self.crimes {
            parts.extend(list.iter().cloned());
        }
        parts.extend(self.nearby.iter().cloned());
        parts.join(" ")
    }
}

/// What someone chose to do.
#[derive(Clone, Debug, Default)]
pub struct Decision {
    pub thought: String,
    pub goal: String,
    pub action: String,
    /// A place kind, "home", or None.
    pub place: Option<String>,
    /// A cast id, "driver", or None.
    pub person: Option<String>,
    pub say: String,
    /// For "something else": what they do, in their own words.
    pub doing: String,
    pub resolve: String,
    pub release: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct DayPlanResult {
    pub goal: String,
    pub plan: Vec<(f64, String, String)>,
}

fn importance(v: &Value, fallback: f64) -> f64 {
    match v.as_str() {
        Some("low") => 3.0,
        Some("medium") => 5.0,
        Some("high") => 8.0,
        _ => fallback,
    }
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

pub fn pick<T: Copy>(list: &[T]) -> T {
    list[((random() * list.len() as f64) as usize).min(list.len() - 1)]
}

fn pick_s(list: &[&str]) -> String {
    pick(list).to_string()
}

// ---------- the local AI: prompts ----------

/// The time and place, where the driver is, and the latest news.
fn scene(moment: &Moment, place: &str, driver_here: bool) -> String {
    let driver = if driver_here {
        format!("The driver has pulled up here in {}.", moment.car)
    } else {
        format!(
            "The driver (in {}) is not here: they are out driving somewhere in the city.",
            moment.car
        )
    };
    let mut out = format!("{}, at {place}. {driver}", moment.time);
    if !moment.news.is_empty() {
        out.push_str("\nLately in the city:");
        for n in moment.news.iter().take(4) {
            out.push_str(&format!("\n- {n}"));
        }
    }
    out
}

/// How someone's needs feel, in words.
fn needs_words(mind: &Mind) -> String {
    let n = &mind.needs;
    let words: Vec<&str> = [
        (n.food < 0.3, "hungry"),
        (n.rest < 0.3, "tired"),
        (n.social < 0.3, "lonely"),
        (n.fun < 0.3, "bored"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, w)| w)
    .collect();
    if words.is_empty() {
        String::new()
    } else {
        format!(" and {}", words.join(", "))
    }
}

/// One person as they are now, with what they remember about these people.
fn describe(p: &Person, moment: &Moment, about: &[&str]) -> String {
    let (m, mind) = (p.member, p.mind);
    let mut feelings: Vec<(&String, f64)> = mind
        .feelings
        .iter()
        .filter(|(id, v)| v.abs() > 0.2 || about.contains(&id.as_str()))
        .map(|(id, v)| (id, *v))
        .collect();
    feelings.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()).then_with(|| a.0.cmp(b.0)));
    let feelings: Vec<String> = feelings
        .iter()
        .take(5)
        .map(|(id, v)| {
            let who = if id.as_str() == "driver" {
                format!(
                    "the driver{}",
                    mind.driver_name.as_ref().map(|n| format!(" ({n})")).unwrap_or_default()
                )
            } else {
                name_of(id)
            };
            format!("{} {who}", feeling_word(*v))
        })
        .collect();
    // The same thing remembered twice is only worth saying once.
    let mut memories: Vec<&str> = Vec::new();
    for mem in mind.recall(moment.now, about, &[], 12) {
        if !memories.contains(&mem.text.as_str()) {
            memories.push(&mem.text);
        }
    }
    memories.truncate(6);
    let mut parts = vec![
        format!("{}, {}. {} Talks: {}.", m.name, m.role, m.bio, m.voice),
        format!("Personality now: {}.", trait_words(&mind.traits)),
    ];
    if !mind.story.is_empty() {
        parts.push(format!("Their life so far: {}", mind.story));
    }
    if !mind.insights.is_empty() {
        let start = mind.insights.len().saturating_sub(3);
        parts.push(format!("Believes: {}", mind.insights[start..].join(" ")));
    }
    if !mind.habits.is_empty() {
        parts.push(format!("Habits: {}.", mind.habits.join("; ")));
    }
    if !mind.resolutions.is_empty() {
        let list: Vec<String> = mind
            .resolutions
            .iter()
            .enumerate()
            .map(|(i, r)| format!("{}. {r}", i + 1))
            .collect();
        let mut t = format!(
            "Standing decisions {} has made (their own: they stick to them, unless they change their mind):\n{}",
            m.name,
            list.join("\n")
        );
        if !mind.room_to_resolve() {
            t.push_str("\nThat's as many as they can hold: to make a new one, they'd drop one of these first.");
        }
        parts.push(t);
    }
    if !mind.thoughts.is_empty() {
        let start = mind.thoughts.len().saturating_sub(2);
        parts.push(format!("Lately thinking: {}", mind.thoughts[start..].join(" ")));
    }
    if !feelings.is_empty() {
        parts.push(format!("{} {}.", m.name, feelings.join(", ")));
    }
    let emotion = if mind.emotion != "calm" {
        format!(", {}", mind.emotion)
    } else {
        String::new()
    };
    parts.push(format!(
        "Right now: {}, feeling {}{emotion}{}.",
        p.doing,
        mood_word(mind),
        needs_words(mind)
    ));
    if !memories.is_empty() {
        let list: Vec<String> = memories.iter().map(|t| format!("- {t}")).collect();
        parts.push(format!("{} remembers:\n{}", m.name, list.join("\n")));
    }
    parts.join("\n")
}

/// The instruction for making (or dropping) a standing decision.
const RESOLVE_WORDS: &str = "If they make a lasting decision or a promise right now about how they will live from now on (like \"I won't ever...\" or \"From now on I'll...\"), give it in their own words as resolve (otherwise \"\"). Their standing decisions stay (\"keep them all\") unless they truly change their mind about one (\"drop\" and its number).";

pub fn conversation_prompt(a: &Person, b: &Person, moment: &Moment) -> String {
    let (am, bm) = (a.member, b.member);
    let together = a.mind.times_met(&bm.id);
    let history = if together == 0.0 {
        "They have never really talked before.".to_string()
    } else {
        format!("They have talked {together} time{} before.", if together == 1.0 { "" } else { "s" })
    };
    [
        scene(moment, &a.place, false),
        describe(a, moment, &[&bm.id]),
        describe(b, moment, &[&am.id]),
        format!("{} and {} run into each other at {}. {history}", am.name, bm.name, a.place),
        format!(
            "Write their conversation: 2 to 4 short lines, taking turns, starting with {}. Make it real: they might catch up, ask about each other's lives, share gossip about other people, talk about the driver, tease, comfort, flirt a little, argue, or make a plan to meet again. Then: one new memory for each of them (first person), any change in how either feels about the other (or about someone they discussed), and a plan if they agreed to meet somewhere (place \"none\" if not).",
            am.name
        ),
    ]
    .join("\n\n")
}

pub fn chat_prompt(p: &Person, said: &str, moment: &Moment) -> String {
    let (m, mind) = (p.member, p.mind);
    let start = mind.chats.len().saturating_sub(6);
    let history: Vec<String> = mind.chats[start..]
        .iter()
        .map(|c| format!("The driver: \"{}\"\n{}: \"{}\"", c.you, m.name, c.them))
        .collect();
    let contact = p.contact.unwrap_or(Contact::Beside);
    let opening = match contact {
        Contact::Phone => format!("The driver calls {} on the phone and says", m.name),
        Contact::Passenger => format!("{} is riding in the driver's car, and the driver says", m.name),
        Contact::Beside => format!("The driver pulls up next to {} and says", m.name),
    };
    let talks = mind.times_met("driver");
    [
        scene(moment, &p.place, contact != Contact::Phone),
        describe(p, moment, &["driver"]),
        format!(
            "How {} sees the driver: {}{}. They have talked {talks} time{}.",
            m.name,
            friendship(mind),
            mind.driver_name.as_ref().map(|n| format!(", knows them as {n}")).unwrap_or_default(),
            if talks == 1.0 { "" } else { "s" }
        ),
        if history.is_empty() {
            "They have never talked before.".to_string()
        } else {
            format!("Their last talks:\n{}", history.join("\n"))
        },
        format!(
            "{opening}: \"{}\"\n\nWrite {}'s reply: at most 2 short sentences, in their own voice, shaped by their mood, their feelings about the driver and what they remember (they may bring up something they remember, ask the driver something back, or invite the driver somewhere). Say how their opinion of the driver changes, and what they will remember about this talk. If the driver said their own name, give it as driverName; otherwise \"\". {RESOLVE_WORDS}",
            cut(said, 300),
            m.name
        ),
    ]
    .join("\n\n")
}

pub fn reflect_prompt(p: &Person, moment: &Moment) -> String {
    let memories: Vec<String> = p
        .mind
        .recall(moment.now, &[], &[], 16)
        .iter()
        .map(|m| format!("- {}", m.text))
        .collect();
    [
        describe(p, moment, &[]),
        format!(
            "{} has a quiet moment and thinks over lately:\n{}",
            p.member.name,
            if memories.is_empty() { "- nothing much".to_string() } else { memories.join("\n") }
        ),
        "What one or two new beliefs do they form (about themselves, a friend, the city or the driver), each a short sentence in their own words? Does this nudge one personality trait up or down (\"none\" if not)? Is there a small new habit they pick up (\"\" for none)? Finally, rewrite their life story so far in the first person, at most 90 words: keep what mattered from their old story, add what matters now (people, the driver, hopes, worries).".to_string(),
    ]
    .join("\n\n")
}

/// The actions someone can choose from now (the police officer's own choices
/// first when the driver has been up to something).
pub fn actions_for(member: &CastMember, situation: &Situation) -> Vec<&'static str> {
    if member.id == "buddy" {
        if situation.crimes.is_some() || situation.chase.is_some() {
            POLICE_ACTIONS.iter().chain(ACTIONS.iter()).copied().collect()
        } else {
            ACTIONS.iter().chain(POLICE_ACTIONS.iter()).copied().collect()
        }
    } else {
        ACTIONS.to_vec()
    }
}

/// How a chase is going, as a short list (small models weigh a list better than a long sentence).
fn chase_words(name: &str, mind: &Mind, chase: &ChaseState) -> String {
    let mut lines = vec![
        format!("{name} chose to go after the driver, and is in the middle of the police chase:"),
        format!(
            "- {} seconds so far; the driver is {} m away",
            chase.seconds.round(),
            chase.distance.round()
        ),
        format!(
            "- the police car is {}% damaged{}",
            (chase.hurt * 100.0).round(),
            if chase.hurt > 0.5 { " (badly hurt)" } else { "" }
        ),
        if chase.quiet > 25.0 {
            format!("- the driver hasn't done anything wrong for {} seconds", chase.quiet.round())
        } else {
            "- the driver is still causing trouble".to_string()
        },
    ];
    if !chase.partner.is_empty() {
        lines.push(format!("- GUY, riding along, says: \"{}\"", chase.partner));
    }
    for v in &mind.resolutions {
        let l = v.to_lowercase();
        if l.contains("chas") || l.contains("pursu") || l.contains("driver") {
            lines.push(format!("- {name} has decided: \"{v}\""));
        }
    }
    lines.join("\n")
}

pub fn decide_prompt(p: &Person, moment: &Moment, situation: &Situation) -> String {
    let (m, mind) = (p.member, p.mind);
    let police = m.id == "buddy";
    let actions = actions_for(m, situation);
    let step = current_step(mind, moment.hour);
    let mut parts = vec![
        scene(moment, &p.place, false),
        describe(p, moment, &[]),
        if mind.goal.is_empty() {
            format!("{} has no particular goal yet.", m.name)
        } else {
            format!("{}'s goal: {}", m.name, mind.goal)
        },
    ];
    if !mind.plan.is_empty() {
        let steps: Vec<String> = mind
            .plan
            .iter()
            .map(|s| format!("{}:00 {} ({})", s.hour, s.what, s.place))
            .collect();
        parts.push(format!(
            "Their plan for today: {}.{}",
            steps.join("; "),
            step.map(|s| format!(" Right now the plan says: {}.", s.what)).unwrap_or_default()
        ));
    }
    parts.push(if situation.nearby.is_empty() {
        "Nobody they know is nearby.".to_string()
    } else {
        format!(
            "Nearby: {}.",
            situation.nearby.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
        )
    });
    if let Some((list, heat)) = &situation.crimes {
        let list: Vec<String> = list.iter().take(5).cloned().collect();
        parts.push(format!(
            "What the driver has done lately: {}. How serious it is: {heat} out of 5.",
            if list.is_empty() {
                "nothing much".to_string()
            } else {
                list.join("; ")
            }
        ));
    }
    parts.push(format!("Now: {}", situation.why));
    if let Some(chase) = &situation.chase {
        parts.push(chase_words(&m.name, mind, chase));
    }
    let police_words = if police && situation.chase.is_some() {
        format!(
            " Keep going, or give up? It is entirely {}'s call, weighing everything above: {}.",
            m.name,
            if random() < 0.5 {
                "\"chase the driver down\" to keep after them, or \"call off the chase\" to give up"
            } else {
                "\"call off the chase\" to give up, or \"chase the driver down\" to keep after them"
            }
        )
    } else if police && situation.crimes.is_some() {
        " As the police officer, they choose for themselves what the driver deserves: \"chase the driver down\" (siren on, after them in the police car), \"ticket the driver\", or \"let it go\".".to_string()
    } else {
        String::new()
    };
    parts.push(format!(
        "{} decides freely what to do next: whatever they truly want, from these actions: {}. \"something else\" is anything else at all that a person could do in a city: then say what, in a few words of their own, as doing (otherwise doing is \"\"). Give their honest private thought, their goal (or \"\" to keep it), the action, the place and person it needs (or \"none\"), and anything they say out loud (\"\" for nothing). {RESOLVE_WORDS}{police_words}",
        m.name,
        actions.join(", ")
    ));
    parts.join("\n\n")
}

pub fn plan_prompt(p: &Person, moment: &Moment) -> String {
    let (m, mind) = (p.member, p.mind);
    let mut parts = vec![describe(p, moment, &[])];
    if !mind.goal.is_empty() {
        parts.push(format!("Their goal so far: {}", mind.goal));
    }
    parts.push(format!(
        "It's {}. {} plans their day, freely: what they want from today, given who they are, their job ({}), their friends, their feelings and their memories. Give their goal (a sentence: what they are working towards these days, kept or changed) and 3 to 6 steps, each from an hour of the day: where, and what they will do there (a few words).",
        moment.time,
        m.name,
        m.job.unwrap_or("none")
    ));
    parts.join("\n\n")
}

// ---------- the local AI: reading its answers ----------

/// At most 20 words, so a speech bubble stays readable.
pub fn short(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= 20 {
        words.join(" ")
    } else {
        let mut t = words[..20].join(" ");
        if t.ends_with([',', ';', ':']) {
            t.pop();
        }
        t + "…"
    }
}

/// Takes off quote marks wrapped around a whole sentence.
pub fn unquote(text: &str) -> String {
    let t = text.trim();
    let mut chars = t.chars();
    let (first, last) = (chars.next(), t.chars().last());
    if t.chars().count() >= 2 && matches!(first, Some('"' | '“' | '\'')) && matches!(last, Some('"' | '”' | '\'')) {
        let inner: String = t.chars().skip(1).take(t.chars().count() - 2).collect();
        return inner.trim().to_string();
    }
    t.to_string()
}

fn emotion(v: &Value) -> String {
    let e = v.as_str().unwrap_or("");
    if EMOTIONS.contains(&e) { e.to_string() } else { "calm".to_string() }
}

/// The number of the standing decision they drop ("drop 2"), if it's one they have.
fn released(mind: &Mind, v: &Value) -> Option<usize> {
    let t = v.as_str()?;
    let n: usize = t.strip_prefix("drop")?.trim().parse().ok()?;
    (n >= 1 && n <= mind.resolutions.len()).then_some(n)
}

pub fn read_conversation(data: &Value, ids: &[String]) -> Option<ConversationResult> {
    let lines = data.get("lines")?.as_array()?;
    if lines.is_empty() {
        return None;
    }
    let ok = |id: &str| ids.iter().any(|i| i == id);
    let lines: Vec<Line> = lines
        .iter()
        .filter(|l| ok(&s(l, "speaker")) && l.get("text").is_some_and(Value::is_string))
        .take(5)
        .map(|l| Line {
            speaker: s(l, "speaker"),
            text: short(&s(l, "text")),
            emotion: emotion(l.get("emotion").unwrap_or(&Value::Null)),
        })
        .collect();
    let empty = Vec::new();
    let memories = data
        .get("memories")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter(|m| ok(&s(m, "who")))
        .take(4)
        .map(|m| {
            (
                s(m, "who"),
                cut(&s(m, "text"), 160),
                importance(m.get("importance").unwrap_or(&Value::Null), 4.0),
            )
        })
        .collect();
    let feelings = data
        .get("feelings")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter(|f| ok(&s(f, "from")) && s(f, "from") != s(f, "about"))
        .take(6)
        .map(|f| (s(f, "from"), s(f, "about"), if s(f, "change") == "warmer" { 1.0 } else { -1.0 }))
        .collect();
    let plan = data.get("plan").and_then(|p| {
        let place = s(p, "place");
        if place == "none" || !PLACE_KINDS.contains(&place.as_str()) {
            return None;
        }
        let when = match s(p, "when").as_str() {
            "tomorrow" => "tomorrow",
            "later" => "later",
            _ => "soon",
        };
        Some(MeetPlan { place, when: when.into() })
    });
    Some(ConversationResult {
        lines,
        memories,
        feelings,
        plan,
        ai: true,
    })
}

pub fn read_chat(data: &Value, mind: &Mind) -> Option<ChatResult> {
    let reply = s(data, "reply");
    if reply.is_empty() {
        return None;
    }
    let name: String = s(data, "driverName")
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '-'))
        .take(24)
        .collect();
    Some(ChatResult {
        reply: cut(&squeeze(&reply), 220),
        emotion: emotion(data.get("emotion").unwrap_or(&Value::Null)),
        opinion: match s(data, "opinion").as_str() {
            "warmer" => 1.0,
            "cooler" => -1.0,
            _ => 0.0,
        },
        memory: cut(&s(data, "memory"), 160),
        importance: importance(data.get("importance").unwrap_or(&Value::Null), 5.0),
        driver_name: name.trim().to_string(),
        resolve: cut(&unquote(&s(data, "resolve")), 120),
        release: released(mind, data.get("release").unwrap_or(&Value::Null)),
    })
}

pub fn read_reflect(data: &Value) -> Option<ReflectResult> {
    let insights = data.get("insights")?.as_array()?;
    let mut t = s(data, "trait");
    if t == "humour" {
        t = "humor".into();
    }
    Some(ReflectResult {
        insights: insights
            .iter()
            .take(2)
            .map(|v| cut(&unquote(v.as_str().unwrap_or("")), 140))
            .collect(),
        nudge: TRAIT_NAMES.iter().find(|n| **n == t).map(|n| (*n, s(data, "direction") == "up")),
        habit: cut(&s(data, "habit"), 100),
        story: cut(&s(data, "story"), 700),
    })
}

pub fn read_decision(data: &Value, member: &CastMember, mind: &Mind, situation: &Situation) -> Option<Decision> {
    let action = s(data, "action");
    if !actions_for(member, situation).contains(&action.as_str()) {
        return None;
    }
    let doing: String = cut(unquote(&s(data, "doing")).trim_end_matches(['.', '!']), 100);
    let place = s(data, "place");
    let place = if place == "home" || PLACE_KINDS.contains(&place.as_str()) {
        Some(place)
    } else {
        None
    };
    let who = s(data, "person");
    let person = (who != "none" && who != member.id && (who == "driver" || crate::cast::member(&who).is_some())).then_some(who);
    Some(Decision {
        thought: cut(&unquote(&s(data, "thought")), 200),
        goal: cut(&s(data, "goal"), 140),
        // "something else" with nothing said about what: they carry on as they were.
        action: if action == "something else" && doing.chars().count() < 3 {
            "carry on".into()
        } else {
            action
        },
        place,
        person,
        say: short(&unquote(&s(data, "say"))),
        doing,
        resolve: cut(&unquote(&s(data, "resolve")), 120),
        release: released(mind, data.get("release").unwrap_or(&Value::Null)),
    })
}

pub fn read_plan(data: &Value) -> Option<DayPlanResult> {
    let plan = data.get("plan")?.as_array()?;
    if plan.is_empty() {
        return None;
    }
    let mut steps: Vec<(f64, String, String)> = plan
        .iter()
        .filter_map(|p| {
            let hour: f64 = s(p, "hour").parse().ok()?;
            let place = s(p, "place");
            (place == "home" || PLACE_KINDS.contains(&place.as_str())).then(|| (hour, place, cut(&s(p, "what"), 60)))
        })
        .collect();
    steps.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(DayPlanResult {
        goal: cut(&s(data, "goal"), 140),
        plan: steps,
    })
}

/// The step of their day plan they're on now, if any.
pub fn current_step(mind: &Mind, hour: f64) -> Option<&crate::mind::DayStep> {
    mind.plan.iter().rfind(|p| p.hour <= hour)
}

// ---------- the rule brain ----------

pub fn rule_conversation(a: &Person, b: &Person, moment: &Moment) -> ConversationResult {
    let (am, bm) = (a.member, b.member);
    let fondness = a.mind.feeling(&bm.id);
    let greet = if am.bond == Some(bm.id.as_str()) || bm.bond == Some(am.id.as_str()) {
        pick(
            &[
                format!("There's my favorite person! Hey, {}.", bm.name),
                format!("{}! Right where I left you.", bm.name),
                "Partner! Ready for whatever today brings?".to_string(),
            ]
            .each_ref(),
        )
        .clone()
    } else if fondness > 0.4 {
        pick(
            &[
                format!("Hey {}! Good to see you.", bm.name),
                format!("{}! How's your day going?", bm.name),
                format!("Look who it is! {}!", bm.name),
            ]
            .each_ref(),
        )
        .clone()
    } else if fondness < -0.2 {
        pick(&[format!("Oh. Hello, {}.", bm.name), format!("{}. Didn't see you there.", bm.name)].each_ref()).clone()
    } else {
        pick(
            &[
                format!("Afternoon, {}.", bm.name),
                format!("Hi there, {}.", bm.name),
                format!("Oh, hello {}.", bm.name),
            ]
            .each_ref(),
        )
        .clone()
    };
    let about_driver = a
        .mind
        .recall(moment.now, &["driver"], &[], 3)
        .into_iter()
        .find(|m| m.about.iter().any(|x| x == "driver"));
    let gossip = a
        .mind
        .recall(moment.now, &[], &[], 8)
        .into_iter()
        .find(|m| m.about.iter().any(|id| id != "driver" && *id != bm.id && *id != am.id) && m.kind != "heard");
    let line = |who: &str, text: String, emotion: &str| Line {
        speaker: who.to_string(),
        text,
        emotion: emotion.to_string(),
    };
    let mut lines = vec![line(&am.id, greet, if fondness > 0.3 { "happy" } else { "calm" })];
    let mut memories = Vec::new();
    let mut feelings = Vec::new();
    let driver_like = a.mind.feeling("driver");
    let car = &moment.car;
    if about_driver.is_some() && random() < 0.6 {
        let (text, emo) = if driver_like > 0.2 {
            (
                pick(
                    &[
                        format!("Did you see the driver in {car}? Incredible!"),
                        format!("That driver in {car} is something else."),
                    ]
                    .each_ref(),
                )
                .clone(),
                "excited",
            )
        } else if driver_like < -0.2 {
            let bare = car.strip_prefix("an ").or_else(|| car.strip_prefix("a ")).unwrap_or(car);
            (
                pick(
                    &[
                        format!("That driver in {car} nearly gave me a heart attack."),
                        format!("Someone should slow that {bare} down."),
                    ]
                    .each_ref(),
                )
                .clone(),
                "annoyed",
            )
        } else {
            (
                pick(
                    &[
                        format!("Have you seen the driver in {car}?"),
                        format!("What do you make of that driver in {car}?"),
                    ]
                    .each_ref(),
                )
                .clone(),
                "curious",
            )
        };
        lines.push(line(&am.id, text, emo));
        let agrees = b.mind.feeling("driver") * driver_like >= 0.0;
        lines.push(line(
            &bm.id,
            if agrees {
                pick_s(&["Tell me about it!", "I saw that too!", "You are not wrong."])
            } else {
                pick_s(&[
                    "Really? I kind of like them.",
                    "Oh, they're harmless.",
                    "I don't know, I think they're fun.",
                ])
            },
            if agrees { "amused" } else { "curious" },
        ));
        memories.push((bm.id.clone(), format!("{} told me about the driver in {car}.", am.name), 3.0));
        feelings.push((bm.id.clone(), "driver".to_string(), if driver_like > 0.0 { 1.0 } else { -1.0 }));
    } else if let Some(g) = gossip.filter(|_| random() < 0.6) {
        let who = g
            .about
            .iter()
            .find(|id| id.as_str() != "driver" && **id != bm.id && **id != am.id)
            .and_then(|id| member(id))
            .map_or("someone".to_string(), |m| m.name.clone());
        lines.push(line(
            &am.id,
            pick(
                &[
                    format!("Did you hear about {who}?"),
                    format!("Between you and me, {who} has been up to something."),
                    format!("So I was talking to {who}..."),
                ]
                .each_ref(),
            )
            .clone(),
            "curious",
        ));
        lines.push(line(
            &bm.id,
            pick_s(&[
                "No way! Tell me everything.",
                "I always wondered about that.",
                "Ha! That sounds about right.",
            ]),
            "amused",
        ));
        memories.push((
            bm.id.clone(),
            format!("{} told me: {}", am.name, g.text),
            (g.importance * 0.7).round().max(2.0),
        ));
    } else if !a.mind.insights.is_empty() && random() < 0.5 {
        lines.push(line(
            &am.id,
            short(&format!("You know what I've realized? {}", a.mind.insights.last().unwrap())),
            "proud",
        ));
        lines.push(line(
            &bm.id,
            pick_s(&[
                "Huh. I never thought of it like that.",
                "That is very deep.",
                "You might be onto something.",
            ]),
            "curious",
        ));
    } else {
        let n = &a.mind.needs;
        let text = if n.food < 0.35 {
            "I'm starving. Want to grab something to eat?".to_string()
        } else if n.rest < 0.3 {
            "I'm running on empty today.".to_string()
        } else if n.fun < 0.35 {
            "I need some excitement in my life.".to_string()
        } else {
            pick(
                &[
                    "Beautiful day, isn't it?".to_string(),
                    "Same old, same old.".to_string(),
                    "Busy day! You know how it is.".to_string(),
                    format!("Nice spot, {}.", a.place),
                ]
                .each_ref(),
            )
            .clone()
        };
        lines.push(line(&am.id, text, if n.fun < 0.35 { "sad" } else { "happy" }));
        lines.push(line(
            &bm.id,
            pick_s(&["I hear you.", "Let's make it a good one.", "Hang in there!", "Same here, honestly."]),
            "calm",
        ));
    }
    // Friendly chats bring people closer; prickly people less so.
    if (am.traits.kindness + bm.traits.kindness) / 2.0 > 0.45 {
        feelings.push((am.id.clone(), bm.id.clone(), 1.0));
        feelings.push((bm.id.clone(), am.id.clone(), 1.0));
    }
    memories.push((am.id.clone(), format!("Chatted with {} at {}.", bm.name, a.place), 2.0));
    // Friends sometimes agree to meet again, somewhere one of them likes.
    let mut plan = None;
    let liked = am.likes.iter().find(|k| bm.likes.contains(k)).or(am.likes.first());
    if let Some(liked) = liked
        && fondness > 0.35
        && random() < 0.3
        && lines.len() < 4
    {
        let when = pick(&["soon", "later", "tomorrow"]);
        lines.push(line(
            &am.id,
            format!("Meet me at the {liked} {}?", if when == "tomorrow" { "tomorrow" } else { "later" }),
            "happy",
        ));
        lines.push(line(
            &bm.id,
            pick_s(&["It's a date!", "I'll be there.", "Wouldn't miss it."]),
            "happy",
        ));
        plan = Some(MeetPlan {
            place: liked.to_string(),
            when: when.into(),
        });
    }
    lines.truncate(5);
    ConversationResult {
        lines,
        memories,
        feelings,
        plan,
        ai: false,
    }
}

/// A name the driver gave ("my name is Sam", "I'm Sam", "call me Sam").
fn name_in(said: &str) -> String {
    // Only ASCII letters change case here, so every position in `lower` is the
    // same in `said` (full lowercasing can change a letter's length, like İ).
    let lower = said.to_ascii_lowercase();
    for lead in ["my name is ", "i'm ", "i am ", "call me "] {
        if let Some(i) = lower.find(lead) {
            let rest = &said[i + lead.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphabetic() || *c == '\'' || *c == '-')
                .take(21)
                .collect();
            if name.chars().count() >= 2 && name.chars().next().is_some_and(char::is_alphabetic) {
                return name;
            }
        }
    }
    String::new()
}

fn has_word(text: &str, words: &[&str]) -> bool {
    text.split(|c: char| !c.is_alphanumeric()).any(|w| words.contains(&w))
}

pub fn rule_chat(p: &Person, said: &str) -> ChatResult {
    let opinion = p.mind.feeling("driver");
    let text = said.to_lowercase();
    let named = name_in(said);
    let name = if named.is_empty() {
        p.mind.driver_name.clone().unwrap_or_default()
    } else {
        named.clone()
    };
    let m = p.member;
    let reply = if !named.is_empty() {
        pick(
            &[
                format!("Nice to meet you, {named}! I'm {}.", m.name),
                format!("{named}, huh? I'll remember that."),
            ]
            .each_ref(),
        )
        .clone()
    } else if has_word(&text, &["hi", "hello", "hey", "morning", "evening"]) {
        if opinion < -0.3 {
            pick_s(&["Oh. It's you.", "You again."])
        } else {
            let with = |sep: &str| if name.is_empty() { String::new() } else { format!("{sep}{name}") };
            pick(
                &[
                    format!("Hello{}! Lovely day for a drive.", with(" ")),
                    format!("Hey there{}!", with(", ")),
                ]
                .each_ref(),
            )
            .clone()
        }
    } else if text.contains("how are you") || text.contains("how's it going") || text.contains("you ok") {
        format!("Feeling {}, thanks for asking!", mood_word(p.mind))
    } else if text.contains("sorry") {
        if opinion < 0.0 {
            "Apology accepted. Drive safe, okay?".into()
        } else {
            "No harm done!".into()
        }
    } else if ["car", "drive", "jump", "fast", "speed"].iter().any(|w| text.contains(w)) {
        if opinion > 0.2 {
            "Your driving is the best show in town!".into()
        } else {
            "Maybe ease off the pedal a little?".into()
        }
    } else if text.contains("job") || text.contains("work") {
        match m.job {
            Some(job) => format!("I work at the {job}. Come by sometime!"),
            None => format!("Work? I'm a {}. It's a calling.", m.role),
        }
    } else {
        pick_s(&[
            "Ha! You are a funny one.",
            "Interesting. I'll have to think about that.",
            "You know, I like talking to you.",
            "That's not something you hear every day!",
        ])
    };
    let warmer = ["sorry", "thank", "nice", "love", "great", "friend"]
        .iter()
        .any(|w| text.contains(w));
    ChatResult {
        reply: short(&reply),
        emotion: if opinion < -0.3 { "annoyed" } else { "happy" }.into(),
        opinion: if warmer { 1.0 } else { 0.0 },
        memory: format!(
            "The driver{} stopped to chat with me: \"{}\"",
            if name.is_empty() { String::new() } else { format!(" ({name})") },
            cut(said, 60)
        ),
        importance: 5.0,
        driver_name: named,
        resolve: String::new(),
        release: None,
    }
}

pub fn rule_reflect(p: &Person, moment: &Moment) -> ReflectResult {
    let mind = p.mind;
    let mut insights = Vec::new();
    let driver = mind.feeling("driver");
    let driver_memories = mind.memories.iter().filter(|m| m.about.iter().any(|a| a == "driver")).count();
    if driver_memories >= 3 {
        insights.push(if driver > 0.2 {
            format!("The driver in {} makes this city exciting.", moment.car)
        } else if driver < -0.2 {
            format!("The driver in {} is a menace.", moment.car)
        } else {
            "I can't make my mind up about that driver.".to_string()
        });
    }
    let best = mind
        .feelings
        .iter()
        .filter(|(id, _)| id.as_str() != "driver")
        .max_by(|a, b| a.1.total_cmp(b.1));
    if let Some((id, v)) = best
        && *v > 0.5
    {
        insights.push(format!("{} is one of the good ones.", name_of(id)));
    }
    let favorite = mind.likes.iter().max_by(|a, b| a.1.total_cmp(b.1));
    let start = mind.memories.len().saturating_sub(12);
    let scared = mind.memories[start..]
        .iter()
        .filter(|m| m.importance >= 7.0 && m.kind == "saw")
        .count();
    insights.truncate(2);
    ReflectResult {
        insights,
        nudge: if scared >= 3 {
            Some(("courage", p.member.traits.courage > 0.6))
        } else if mind.mood > 0.4 {
            Some(("humor", true))
        } else {
            None
        },
        habit: match favorite {
            Some((kind, v)) if *v > 0.5 => format!("heads to the {kind} whenever there's a spare moment"),
            _ => String::new(),
        },
        story: String::new(),
    }
}

fn rule_decision(action: &str, thought: &str) -> Decision {
    Decision {
        thought: thought.to_string(),
        action: action.to_string(),
        ..Default::default()
    }
}

/// Which kind of standing decision would rule out each action (for the built-in brain).
fn vow_for(action: &str) -> Option<Vow> {
    Some(match action {
        "chase the driver down" => Vow::Chase,
        "ticket the driver" => Vow::Ticket,
        "work" => Vow::Work,
        "get up to mischief" => Vow::Mischief,
        "text the driver" => Vow::Text,
        "report the driver to the police" => Vow::Report,
        "drive somewhere" | "give someone a lift" | "go for a drive with GUY" => Vow::Drive,
        _ => return None,
    })
}

/// The built-in way of choosing. It keeps the standing decisions it
/// understands: someone who's sworn off something doesn't do it.
pub fn rule_decide(p: &Person, moment: &Moment, situation: &Situation) -> Decision {
    let d = rule_choose(p, moment, situation);
    match vow_for(&d.action) {
        Some(kind) if p.mind.forbids(kind, &situation.context()) => rule_decision("carry on", "No. I made my mind up about that."),
        _ => d,
    }
}

fn rule_choose(p: &Person, moment: &Moment, situation: &Situation) -> Decision {
    let mind = p.mind;
    let n = &mind.needs;
    let context = situation.context();
    // The police officer, mid-chase: keep going, or give it up?
    if let Some(chase) = &situation.chase {
        let fond = mind.feeling("driver") > 0.3;
        let heat = situation.crimes.as_ref().map_or(0.0, |c| c.1);
        let why = if mind.forbids(Vow::Chase, &context) {
            "I said I was done with chases. I meant it."
        } else if chase.hurt > 0.6 {
            "The car can't take much more of this."
        } else if !chase.partner.is_empty() && fond {
            "GUY's right. Let them go."
        } else if chase.quiet > 45.0 && heat < 3.0 {
            "They've calmed down. That'll do."
        } else if chase.seconds > 150.0 && random() < 0.5 {
            "This is going nowhere."
        } else {
            ""
        };
        if !why.is_empty() {
            let mut d = rule_decision("call off the chase", why);
            d.say = pick_s(&[
                "Alright, alright. Siren off.",
                "That's enough for today.",
                "Let them go. I've made my point.",
            ]);
            return d;
        }
        return rule_decision(
            "chase the driver down",
            pick(&["Not letting them get away.", "Stay on them!", "Almost got them."]),
        );
    }
    // The police officer: how bad has the driver been? (And what has he promised himself?)
    if let Some((_, heat)) = &situation.crimes {
        let soft = mind.feeling("driver") > 0.55;
        let chases = !mind.forbids(Vow::Chase, &context);
        let tickets = !mind.forbids(Vow::Ticket, &context);
        if *heat >= if soft { 4.0 } else { 3.0 } && chases {
            let mut d = rule_decision("chase the driver down", "That driver has gone too far this time.");
            d.say = pick_s(&["Pull over! Now!", "That's it, you're nicked!", "Stop right there!"]);
            return d;
        }
        if *heat >= 2.0 && tickets {
            return rule_decision("ticket the driver", "Time somebody wrote that driver a ticket.");
        }
        return rule_decision(
            "let it go",
            if chases && tickets {
                "Not worth the paperwork. Yet."
            } else {
                "I made my mind up about that. I'm sticking to it."
            },
        );
    }
    if n.food < 0.3 {
        let mut d = rule_decision("eat", "I'm starving.");
        d.place = Some(pick(&["diner", "bakery", "coffee"]).to_string());
        return d;
    }
    if n.rest < 0.25 {
        let late = moment.hour >= 20.0;
        let mut d = rule_decision(if late { "go home" } else { "rest" }, "I can barely keep my eyes open.");
        d.place = Some(if late { "home" } else { "park" }.to_string());
        return d;
    }
    if n.social < 0.3 {
        let friend = mind
            .feelings
            .iter()
            .filter(|(id, v)| id.as_str() != "driver" && **v > 0.4)
            .max_by(|a, b| a.1.total_cmp(b.1));
        if let Some((id, _)) = friend {
            let mut d = rule_decision("find someone", &format!("I miss {}.", name_of(id)));
            d.person = Some(id.clone());
            return d;
        }
    }
    // The curious go off to see another town now and then (in the daytime).
    if mind.traits.curiosity > 0.5 && (8.0..16.0).contains(&moment.hour) && random() < 0.05 {
        return rule_decision("take a trip out of town", "I fancy seeing somewhere new today.");
    }
    // Mischief, now and then, for the prickly and the bored.
    if mind.traits.kindness < 0.4 && n.fun < 0.5 && random() < 0.15 {
        return rule_decision("get up to mischief", "Nobody is watching. Probably.");
    }
    // Friends of the driver send a message sometimes.
    if mind.feeling("driver") > 0.45 && random() < 0.08 {
        let mut d = rule_decision("text the driver", "I wonder what the driver is up to.");
        d.say = pick_s(&[
            "Hey! Fancy meeting at the diner later?",
            "Saw you fly past earlier. Show-off!",
            "Come by and say hi sometime!",
        ]);
        return d;
    }
    let thought = format!("I wonder what's going on at {}.", p.place);
    rule_decision("carry on", pick(&["Same as always.", "Not a bad day.", thought.as_str()]))
}

pub fn rule_plan(p: &Person) -> DayPlanResult {
    let (m, mind) = (p.member, p.mind);
    let mut likes: Vec<(&String, &f64)> = mind.likes.iter().collect();
    likes.sort_by(|a, b| b.1.total_cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let fun = likes
        .iter()
        .map(|(k, _)| k.as_str())
        .find(|k| Some(*k) != m.job)
        .unwrap_or("park")
        .to_string();
    let step = |h: f64, place: &str, what: &str| (h, place.to_string(), what.to_string());
    let mut plan = vec![step(7.0, "coffee", "grab a coffee")];
    // Work, unless they've decided they're done with it.
    match m.job {
        Some(job) if !mind.forbids(Vow::Work, "") => {
            plan.push(step(9.0, job, "work"));
            plan.push(step(12.0, "diner", "lunch"));
            plan.push(step(13.0, job, "back to work"));
        }
        _ => {
            plan.push(step(10.0, &fun, "spend the morning"));
            plan.push(step(12.0, "bakery", "lunch"));
        }
    }
    plan.push(step(18.0, &fun, "unwind"));
    plan.push(step(22.0, "home", "head home"));
    DayPlanResult {
        goal: if mind.goal.is_empty() {
            format!("Make the most of {}the city.", if m.job.is_some() { "work and " } else { "" })
        } else {
            mind.goal.clone()
        },
        plan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_tidied() {
        assert_eq!(unquote("\"Hello there\""), "Hello there");
        assert_eq!(unquote("Hi"), "Hi");
        let long = (0..30).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        assert!(short(&long).ends_with('…'));
        assert_eq!(name_in("Hi, my name is Sam!"), "Sam");
    }

    /// Letters that change length when lowercased (Turkish İ) mustn't throw
    /// the name finder off the end of what was said, or into the middle of a letter.
    #[test]
    fn names_are_found_after_any_letters() {
        assert_eq!(name_in("İİ i am x"), "");
        assert_eq!(name_in("İstanbul! My name is Ayşe"), "Ayşe");
        assert_eq!(name_in("ÉMILIE here. Call me Em"), "Em");
        for said in ["İİİİİİ call me", "İ my name is", "ΣΣ i'm Bo"] {
            let _ = name_in(said);
        }
    }

    #[test]
    fn the_rule_brain_answers() {
        let guy = crate::cast::member("guy").unwrap();
        let buddy = crate::cast::member("buddy").unwrap();
        let (gm, bm) = (Mind::new(guy), Mind::new(buddy));
        let person = |m: &'static CastMember, mind: &'static Mind| Person {
            member: m,
            mind,
            place: "the park".into(),
            doing: "standing".into(),
            contact: None,
        };
        let gm: &'static Mind = Box::leak(Box::new(gm));
        let bm: &'static Mind = Box::leak(Box::new(bm));
        let moment = Moment {
            now: 0.0,
            time: "Monday, morning".into(),
            hour: 9.0,
            car: "a red sports car".into(),
            news: vec![],
        };
        let c = rule_conversation(&person(guy, gm), &person(buddy, bm), &moment);
        assert!(c.lines.len() >= 2);
        let d = rule_decide(
            &person(buddy, bm),
            &moment,
            &Situation {
                crimes: Some((vec!["speeding".into()], 4.0)),
                ..Situation::new("saw it", vec![])
            },
        );
        assert_eq!(d.action, "chase the driver down");
        assert!(!rule_plan(&person(guy, gm)).plan.is_empty());
        assert!(chat_prompt(&person(guy, gm), "hi", &moment).contains("GUY"));
    }
}

pub mod protocol {
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
    pub const ACTIONS: [&str; 17] = [
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
        "take a trip out of town",
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
}

pub mod ai {
    //! Optional GGUF inference on a worker, off the game and render threads.
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    #[cfg(feature = "local-ai")]
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::sync::{Mutex, mpsc};
    use std::time::{Duration, Instant};

    use bevy::prelude::Resource;
    use serde_json::Value;

    use crate::protocol::{ModelChoice, ThinkKind};

    const REQUEST_LIFETIME: Duration = Duration::from_secs(45);

    pub struct Request {
        pub kind: ThinkKind,
        pub ids: Vec<String>,
        pub prompt: String,
    }

    type Answer = Result<Value, String>;

    #[derive(Default, Resource)]
    pub struct LocalAi {
        path: Option<PathBuf>,
        #[cfg(feature = "local-ai")]
        gpu: bool,
        answer: Option<Mutex<mpsc::Receiver<Answer>>>,
        started: Option<Instant>,
        #[cfg(feature = "local-ai")]
        worker: Option<mpsc::SyncSender<Job>>,
        #[cfg(feature = "local-ai")]
        cancel: Option<Arc<AtomicBool>>,
        download: Option<Download>,
        pub status: String,
    }

    #[cfg(feature = "local-ai")]
    struct Job {
        path: PathBuf,
        gpu: bool,
        request: Request,
        started: Instant,
        answer: mpsc::Sender<Answer>,
        cancel: Arc<AtomicBool>,
    }

    struct Download {
        child: Child,
        partial: PathBuf,
        path: PathBuf,
    }

    impl Drop for Download {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_file(&self.partial);
        }
    }

    fn model_dir() -> Option<PathBuf> {
        game::storage::data_dir().map(|p| p.join("packs/npc/models"))
    }

    fn gguf(path: &Path) -> bool {
        use std::io::Read;
        let mut magic = [0; 4];
        std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut magic)).is_ok() && &magic == b"GGUF"
    }

    impl LocalAi {
        pub fn model(choice: ModelChoice) -> Option<PathBuf> {
            let file = choice.info().file;
            Self::owned_model(choice).or_else(|| {
                game::import::old_app_folders()
                    .into_iter()
                    .map(|p| p.join("packs/npc/models").join(file))
                    .find(|p| gguf(p) && p.with_extension("gguf.done").is_file())
            })
        }

        pub fn owned_model(choice: ModelChoice) -> Option<PathBuf> {
            let path = model_dir()?.join(choice.info().file);
            gguf(&path).then_some(path)
        }

        pub fn select(&mut self, choice: ModelChoice, enabled: bool, gpu: bool) {
            self.path = enabled.then(|| Self::model(choice)).flatten();
            #[cfg(feature = "local-ai")]
            {
                self.gpu = gpu;
            }
            #[cfg(not(feature = "local-ai"))]
            let _ = gpu;
        }

        pub fn stop(&mut self) {
            self.path = None;
            self.cancel_request();
            #[cfg(feature = "local-ai")]
            {
                self.worker = None;
            }
        }

        pub fn cancel_request(&mut self) {
            #[cfg(feature = "local-ai")]
            if let Some(cancel) = self.cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.answer = None;
            self.started = None;
        }

        pub fn busy(&self) -> bool {
            self.answer.is_some()
        }

        pub fn request(&mut self, request: Request) -> bool {
            #[cfg(not(feature = "local-ai"))]
            {
                let Request { kind, ids, prompt } = request;
                drop((kind, ids, prompt));
                false
            }
            #[cfg(feature = "local-ai")]
            {
                let Some(path) = self.path.clone() else { return false };
                if self.busy() {
                    return false;
                }
                if self.worker.is_none() {
                    let (tx, rx) = mpsc::sync_channel(1);
                    if std::thread::Builder::new()
                        .name("living-city-ai".into())
                        .spawn(move || run(rx))
                        .is_err()
                    {
                        return false;
                    }
                    self.worker = Some(tx);
                }
                let (answer, receive) = mpsc::channel();
                let started = Instant::now();
                let cancel = Arc::new(AtomicBool::new(false));
                let job = Job {
                    path,
                    gpu: self.gpu,
                    request,
                    started,
                    answer,
                    cancel: cancel.clone(),
                };
                let Some(worker) = &self.worker else { return false };
                match worker.try_send(job) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(_)) => return false,
                    Err(mpsc::TrySendError::Disconnected(_)) => {
                        self.worker = None;
                        return false;
                    }
                }
                self.cancel = Some(cancel);
                self.answer = Some(Mutex::new(receive));
                self.started = Some(started);
                self.status = "Thinking locally…".into();
                true
            }
        }

        pub fn poll(&mut self) -> Option<Answer> {
            use mpsc::TryRecvError::{Disconnected, Empty};
            let result = match self.answer.as_mut()?.get_mut().ok()?.try_recv() {
                Ok(result) => result,
                Err(Disconnected) => Err("The AI worker stopped.".into()),
                Err(Empty) if self.started.is_some_and(|t| t.elapsed() >= REQUEST_LIFETIME) => {
                    Err("The model took too long. Using the built-in brain.".into())
                }
                Err(Empty) => return None,
            };
            self.cancel_request();
            self.status = match &result {
                Ok(_) => "Local AI ready".into(),
                Err(error) => error.clone(),
            };
            Some(result)
        }

        pub fn start_download(&mut self, choice: ModelChoice) {
            if self.download.is_some() || Self::model(choice).is_some() {
                return;
            }
            let Some(dir) = model_dir() else {
                self.status = "The model folder is unavailable.".into();
                return;
            };
            let info = choice.info();
            let path = dir.join(info.file);
            let partial = path.with_extension("gguf.part");
            let url = format!("https://huggingface.co/{}/resolve/main/{}", info.repo, info.remote_file);
            let result = std::fs::create_dir_all(dir).and_then(|()| {
                Command::new("curl")
                    .args([
                        "--fail",
                        "--location",
                        "--silent",
                        "--show-error",
                        "--retry",
                        "2",
                        "--proto",
                        "=https",
                        "--proto-redir",
                        "=https",
                        "--output",
                    ])
                    .arg(&partial)
                    .arg(url)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            });
            match result {
                Ok(child) => self.download = Some(Download { child, partial, path }),
                Err(error) => self.status = format!("Could not start model download: {error}"),
            }
        }

        pub fn downloading(&self) -> bool {
            self.download.is_some()
        }

        pub fn cancel_download(&mut self) {
            if self.download.take().is_some() {
                self.status = "Download canceled.".into();
            }
        }

        pub fn poll_download(&mut self) {
            let Some(download) = &mut self.download else { return };
            let result = match download.child.try_wait() {
                Ok(None) => {
                    let bytes = std::fs::metadata(&download.partial).map_or(0, |m| m.len());
                    self.status = format!("Downloading model… {:.0} MB", bytes as f64 / 1_000_000.0);
                    return;
                }
                Ok(Some(exit)) if exit.success() && gguf(&download.partial) => {
                    std::fs::rename(&download.partial, &download.path).map_err(|e| e.to_string())
                }
                Ok(Some(exit)) => Err(format!("The model download failed ({exit}).")),
                Err(error) => Err(error.to_string()),
            };
            self.download = None;
            self.status = result.err().unwrap_or_else(|| "Model downloaded".into());
        }

        pub fn remove_model(&mut self, choice: ModelChoice) {
            self.stop();
            let result = model_dir().map(|dir| std::fs::remove_file(dir.join(choice.info().file)));
            self.status = match result {
                Some(Ok(())) => "Model removed.".into(),
                Some(Err(error)) => format!("Could not remove model: {error}"),
                None => "The model folder is unavailable.".into(),
            };
        }
    }

    #[cfg(feature = "local-ai")]
    fn run(rx: mpsc::Receiver<Job>) {
        use llama_cpp_2::{
            llama_backend::LlamaBackend,
            model::{LlamaModel, params::LlamaModelParams},
        };
        let mut backend = None;
        let mut model = None;
        while let Ok(job) = rx.recv() {
            if job.cancel.load(Ordering::Relaxed) {
                continue;
            }
            let result = (|| {
                if backend.is_none() {
                    backend = Some(LlamaBackend::init().map_err(|e| e.to_string())?);
                }
                let backend = backend.as_ref().expect("initialized above");
                if !model.as_ref().is_some_and(|(path, gpu, _)| path == &job.path && *gpu == job.gpu) {
                    model = None;
                    let layers = if job.gpu && backend.supports_gpu_offload() { u32::MAX } else { 0 };
                    let params = LlamaModelParams::default().with_n_gpu_layers(layers);
                    let loaded = LlamaModel::load_from_file(backend, &job.path, &params).map_err(|e| e.to_string())?;
                    model = Some((job.path.clone(), job.gpu, loaded));
                }
                generate(backend, &model.as_ref().expect("loaded above").2, &job)
            })();
            let _ = job.answer.send(result);
        }
    }

    #[cfg(feature = "local-ai")]
    fn generate(backend: &llama_cpp_2::llama_backend::LlamaBackend, model: &llama_cpp_2::model::LlamaModel, job: &Job) -> Answer {
        use llama_cpp_2::{context::params::LlamaContextParams, llama_batch::LlamaBatch, sampling::LlamaSampler};
        use std::num::NonZeroU32;
        let request = &job.request;
        let grammar = llama_cpp_2::json_schema_to_grammar(&crate::protocol::schema_for(request.kind, &request.ids).to_string())
            .map_err(|e| e.to_string())?;
        let guarded_prompt = request.prompt.replace("<|", "< |");
        // All three choices are Qwen3.5 models with the same chat markers. An empty
        // thinking block starts the assistant directly in its structured reply.
        let prompt = format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n",
            crate::protocol::SYSTEM_PROMPT,
            guarded_prompt
        );
        let vocab = model.vocab();
        let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
        let max = request.kind.max_tokens();
        let size = (tokens.len() + max + 1).next_multiple_of(256).clamp(1024, 8192);
        if tokens.is_empty() || tokens.len() + max >= size {
            return Err("The resident's prompt was too long.".into());
        }
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(2).clamp(1, 4)) as i32;
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(size as u32))
            .with_n_batch(512)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let mut context = model.new_context(backend, params).map_err(|e| e.to_string())?;
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::grammar(model, &grammar, "root").map_err(|e| e.to_string())?,
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::temp(request.kind.temperature()),
            LlamaSampler::dist(u32::MAX),
        ]);
        let mut batch = LlamaBatch::new(512, 1);
        for (part, chunk) in tokens.chunks(512).enumerate() {
            if job.cancel.load(Ordering::Relaxed) {
                return Err("Canceled".into());
            }
            batch.clear();
            for (i, token) in chunk.iter().enumerate() {
                let pos = part * 512 + i;
                batch
                    .add(*token, pos as i32, &[0], pos + 1 == tokens.len())
                    .map_err(|e| e.to_string())?;
            }
            context.decode(&mut batch).map_err(|e| e.to_string())?;
        }
        let mut out = Vec::new();
        for i in 0..max {
            if job.cancel.load(Ordering::Relaxed) {
                return Err("Canceled".into());
            }
            if job.started.elapsed() >= REQUEST_LIFETIME {
                return Err("The model took too long.".into());
            }
            let token = sampler.sample(&context, -1);
            if vocab.is_eog(token) {
                break;
            }
            vocab.token_to_piece_into(token, &mut out, false, None);
            if out.last() == Some(&b'}')
                && let Ok(data) = serde_json::from_slice(&out)
            {
                return Ok(data);
            }
            batch.clear();
            batch.add(token, (tokens.len() + i) as i32, &[0], true).map_err(|e| e.to_string())?;
            context.decode(&mut batch).map_err(|e| e.to_string())?;
        }
        serde_json::from_slice(&out).map_err(|_| "The model did not finish its reply. Using the built-in brain.".into())
    }

    #[cfg(all(test, feature = "local-ai"))]
    mod tests {
        use super::*;

        #[test]
        #[ignore = "requires a downloaded Qwen3.5 GGUF model"]
        fn local_model_can_generate_a_structured_chat_reply() {
            let path = std::env::var_os("CAR_GAME_TEST_MODEL")
                .map(PathBuf::from)
                .or_else(|| LocalAi::model(ModelChoice::Light))
                .expect("set CAR_GAME_TEST_MODEL to a Qwen3.5 GGUF file");
            let mut ai = LocalAi {
                path: Some(path),
                gpu: true,
                ..Default::default()
            };
            assert!(ai.request(Request {
                kind: ThinkKind::Chat,
                ids: vec!["ava".into()],
                prompt: "You are Ava, an enthusiastic barista. The driver says hello. Say a short greeting.".into(),
            }));
            let reply = loop {
                if let Some(result) = ai.poll() {
                    break result.expect("the local model generated JSON");
                }
                std::thread::sleep(Duration::from_millis(50));
            };
            let mind = crate::mind::Mind::new(crate::cast::member("ava").unwrap());
            assert!(crate::brain::read_chat(&reply, &mind).is_some(), "{reply}");
        }
    }
}
