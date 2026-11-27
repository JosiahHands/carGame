//! The residents' daily lives and the driver's history with them.
use std::collections::{HashMap, VecDeque};

use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use game::game::Game;
use game::packs::Happening;
use game::rng::{centered, random};
use game::storage::Storage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai::{LocalAi, Request};
use crate::brain::{self, ChatResult, Contact, ConversationResult, DayPlanResult, Decision, Moment, Person, ReflectResult, Situation};
use crate::cast::{CastMember, cast, name_of};
use crate::map::{CityMap, Place};
use crate::mind::{self, DayStep, Mind};
use crate::protocol::ThinkKind;

pub const SAVE_KEY: &str = "carGame.pack.npc.city";
const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
pub const POLICE_OWNER: usize = usize::MAX - 3;

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Saved {
    version: u32,
    saved_at: u64,
    clock: f64,
    minds: HashMap<String, Value>,
    tickets: u64,
    meetups: Vec<Meetup>,
    news: Vec<String>,
    talks: Vec<Talk>,
    heat: f64,
    crimes: Vec<String>,
    record: Record,
    messages: Vec<Message>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meetup {
    pub ids: [String; 2],
    pub place_id: String,
    pub kind: String,
    pub at: f64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Record {
    pub chases: u64,
    pub escapes: u64,
    pub busts: u64,
    pub fines: u64,
    pub called_off: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Talk {
    pub t: f64,
    #[serde(rename = "where")]
    pub place: String,
    pub lines: Vec<TalkLine>,
    pub ai: bool,
}

#[derive(Serialize, Deserialize)]
pub struct TalkLine {
    pub name: String,
    pub text: String,
}

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub from: String,
    pub text: String,
    pub t: f64,
}

pub struct Resident {
    pub member: &'static CastMember,
    pub mind: Mind,
    pub at: DVec2,
    pub heading: f64,
    pub speed: f64,
    pub asleep: bool,
    pub in_car: bool,
    pub speech: String,
    pub speech_until: f64,
    pub entity: Option<Entity>,
    home: DVec2,
    path: VecDeque<DVec2>,
    place: Option<Place>,
    task: String,
    next: f64,
    last_talk: f64,
    last_reaction: f64,
    evade: Option<(DVec2, f64)>,
    /// How hurt they are (100 knocks them out).
    pub hurt: f64,
    /// Lying knocked out until then, and in the hospital until then.
    pub down_until: f64,
    pub hospital_until: f64,
    /// Fighting the driver back until then.
    fighting_until: f64,
    punch_due: f64,
    /// Visiting another town (an index into `City::towns`), until then.
    pub away: Option<usize>,
    away_until: f64,
}

impl Resident {
    fn new(member: &'static CastMember, saved: Option<&Value>) -> Self {
        Self {
            member,
            mind: Mind::restore(member, saved),
            at: DVec2::ZERO,
            heading: 0.0,
            speed: 0.0,
            asleep: false,
            in_car: false,
            speech: String::new(),
            speech_until: 0.0,
            entity: None,
            home: DVec2::ZERO,
            path: VecDeque::new(),
            place: None,
            task: String::new(),
            next: 0.0,
            last_talk: -100.0,
            last_reaction: -100.0,
            evade: None,
            hurt: 0.0,
            down_until: 0.0,
            hospital_until: 0.0,
            fighting_until: 0.0,
            punch_due: 0.0,
            away: None,
            away_until: 0.0,
        }
    }

    /// Out of sight: asleep, in a car, or in the hospital.
    pub fn hidden(&self, now: f64) -> bool {
        self.asleep || self.in_car || (self.hospital_until > now && self.down_until <= now)
    }

    fn say(&mut self, now: f64, text: String) {
        if !text.is_empty() {
            self.speech = text;
            self.speech_until = now + 6.0;
        }
    }

    fn go(&mut self, map: &CityMap, at: DVec2, place: Option<Place>, now: f64, task: impl Into<String>) {
        self.path = map.route(self.at, at);
        self.place = place;
        self.task = task.into();
        self.next = now + 15.0 + random() * 25.0;
        self.asleep = false;
    }

    fn go_place(&mut self, map: &CityMap, place: Place, now: f64, task: impl Into<String>) {
        let spread = if place.kind == "park" { 5.0 } else { 2.6 };
        let at = place.at + DVec2::new(centered(), centered()) * spread;
        self.go(map, at, Some(place), now, task);
    }

    fn person(&self, chat: Option<Contact>) -> Person<'_> {
        Person {
            member: self.member,
            mind: &self.mind,
            place: self.place.map_or("the sidewalk", |p| p.label).into(),
            doing: if self.task.is_empty() {
                "standing on the sidewalk".into()
            } else {
                self.task.clone()
            },
            contact: chat,
        }
    }
}

pub struct Chat {
    pub who: usize,
    pub contact: Contact,
    pub input: String,
    pub greeting: String,
    pub focus: bool,
}

struct Pending {
    work: Work,
    index: usize,
}

enum Work {
    Chat(String, ChatResult),
    Decide(Situation, Decision),
    Plan(DayPlanResult),
    Reflect(ReflectResult),
    Talk(usize, ConversationResult),
}

pub struct Chase {
    pub at: DVec2,
    pub heading: f64,
    pub speed: f64,
    pub hurt: f64,
    start: f64,
    lost: f64,
    stopped: f64,
    rethink: f64,
    next_siren: f64,
    next_shot: f64,
    next_sight: f64,
    sees_driver: bool,
    damage: f64,
    partner: Option<usize>,
    seen: Vec<usize>,
}

pub struct Trip {
    pub at: DVec2,
    pub heading: f64,
    pub driver: usize,
    rider: Option<usize>,
    destination: Place,
    roads: VecDeque<DVec2>,
    /// Out to another town (index into `City::towns`), or back from one.
    out: Option<usize>,
    back: bool,
}

/// Another town residents can drive out to (the open world, see the game's land.rs).
#[derive(Clone)]
pub struct OutOfTown {
    pub name: String,
    pub at: DVec2,
    pub half: f64,
    /// The highway from the city's edge to the town's middle.
    pub route: Vec<DVec2>,
}

#[derive(Default, Resource)]
pub struct City {
    pub map: CityMap,
    pub residents: Vec<Resident>,
    pub generation: u64,
    pub active: bool,
    pub journal: bool,
    pub chat: Option<Chat>,
    pub clock: f64,
    pub tickets: u64,
    pub meetups: Vec<Meetup>,
    pub news: Vec<String>,
    pub talks: Vec<Talk>,
    pub heat: f64,
    pub crimes: Vec<String>,
    pub record: Record,
    pub messages: Vec<Message>,
    pub chase: Option<Chase>,
    pub trips: Vec<Trip>,
    pub passenger: Option<(usize, Place)>,
    pub find: Option<(usize, f64)>,
    pub reset_prompt: bool,
    pub ai_choice: Option<crate::protocol::ModelChoice>,
    pub ai_memory: u64,
    /// Towns out on the highways (only in the open world).
    pub towns: Vec<OutOfTown>,
    save_due: f64,
    decision_due: f64,
    conversation_due: f64,
    police_due: f64,
    party_day: Option<usize>,
    last_crime: f64,
    cursor: usize,
    pending: Option<Pending>,
}

fn push_limited<T>(list: &mut Vec<T>, value: T, count: usize) {
    list.insert(0, value);
    list.truncate(count);
}

pub fn position(game: &Game) -> DVec2 {
    DVec2::new(game.physics.x, game.physics.z)
}

fn hour(clock: f64) -> f64 {
    (8.0 + clock / 60.0).rem_euclid(24.0)
}
fn day(clock: f64) -> usize {
    ((8.0 + clock / 60.0) / 24.0) as usize
}

impl City {
    pub fn restore(storage: &Storage) -> Self {
        let mut saved = storage.get_as::<Saved>(SAVE_KEY).unwrap_or_default();
        let residents = cast()
            .iter()
            .map(|member| Resident::new(member, saved.minds.get(&member.id)))
            .collect();
        saved.news.truncate(10);
        saved.messages.truncate(20);
        saved.talks.truncate(30);
        let clock = saved.clock.max(0.0);
        Self {
            residents,
            clock,
            tickets: saved.tickets,
            meetups: saved.meetups,
            news: saved.news,
            talks: saved.talks,
            heat: saved.heat.clamp(0.0, 5.0),
            crimes: saved.crimes,
            record: saved.record,
            messages: saved.messages,
            save_due: clock + 20.0,
            last_crime: clock,
            police_due: clock + 8.0,
            ..default()
        }
    }

    pub fn save(&mut self, storage: &mut Storage) {
        let minds = self
            .residents
            .iter()
            .filter_map(|n| serde_json::to_value(&n.mind).ok().map(|mind| (n.member.id.clone(), mind)))
            .collect();
        let saved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_millis() as u64);
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Snapshot<'a> {
            version: u32,
            saved_at: u64,
            clock: f64,
            minds: HashMap<String, Value>,
            tickets: u64,
            meetups: &'a [Meetup],
            news: &'a [String],
            talks: &'a [Talk],
            heat: f64,
            crimes: &'a [String],
            record: &'a Record,
            messages: &'a [Message],
        }
        storage.set_json(
            SAVE_KEY,
            &Snapshot {
                version: 3,
                saved_at,
                clock: self.clock,
                minds,
                tickets: self.tickets,
                meetups: &self.meetups,
                news: &self.news,
                talks: &self.talks,
                heat: self.heat,
                crimes: &self.crimes,
                record: &self.record,
                messages: &self.messages,
            },
        );
        self.save_due = self.clock + 20.0;
    }

    pub fn place_residents(&mut self) {
        let h = hour(self.clock);
        for (i, n) in self.residents.iter_mut().enumerate() {
            n.home = self.map.home(&n.member.id);
            let kind = n.member.job.or_else(|| n.member.likes.first().copied()).unwrap_or("park");
            n.place = self.map.place(kind, n.home).or_else(|| self.map.nearest_place(n.home).copied());
            n.asleep = !(6.0..23.0).contains(&h);
            n.at = if n.asleep {
                n.home
            } else {
                n.place.map_or(n.home, |p| p.at + DVec2::new(centered(), centered()) * 4.0)
            };
            n.task = if n.asleep { "asleep at home" } else { "starting the day" }.into();
            n.path.clear();
            n.in_car = false;
            n.next = self.clock + 3.0 + i as f64 * 0.45;
        }
    }

    pub fn clock_text(&self) -> String {
        let h = hour(self.clock);
        format!("{} {:02}:{:02}", DAYS[day(self.clock) % 7], h as u32, (h.fract() * 60.0) as u32)
    }

    fn moment(&self, game: &Game) -> Moment {
        Moment {
            now: self.clock,
            time: self.clock_text(),
            hour: hour(self.clock),
            car: format!("a {} with {} paint", game.drive_config.body, game.drive_config.paint_color),
            news: self.news.clone(),
        }
    }

    pub fn resident(&self, id: &str) -> Option<usize> {
        self.residents.iter().position(|n| n.member.id == id)
    }

    pub fn nearest(&self, at: DVec2, reach: f64) -> Option<usize> {
        self.residents
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.asleep && !n.in_car && n.at.distance_squared(at) <= reach * reach)
            .min_by(|(_, a), (_, b)| a.at.distance_squared(at).total_cmp(&b.at.distance_squared(at)))
            .map(|(i, _)| i)
    }

    pub fn open_chat(&mut self, who: usize, contact: Contact, game: &mut Game) {
        let n = &mut self.residents[who];
        if n.asleep {
            game.toast(format!("{} is asleep at home", n.member.name), 1800.0);
            return;
        }
        if contact == Contact::Beside && (n.in_car || n.at.distance(position(game)) > 60.0 || game.physics.vx.hypot(game.physics.vz) > 7.0)
        {
            game.toast("Pull up nearby and slow down to talk", 1800.0);
            return;
        }
        let greeting = match mind::friendship(&n.mind) {
            "enemy" => "Oh. You. What do you want?".into(),
            "wary" => "Can I help you?".into(),
            "stranger" => "Oh, hello there! Do I know you?".into(),
            _ => format!("Hey, {}!", n.mind.driver_name.as_deref().unwrap_or("driver")),
        };
        self.chat = Some(Chat {
            who,
            contact,
            greeting,
            input: String::new(),
            focus: true,
        });
    }

    pub fn chat_pending(&self, who: usize) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.index == who && matches!(p.work, Work::Chat(..)))
    }

    pub fn send_chat(&mut self, game: &mut Game, ai: &mut LocalAi, ai_chat: bool) {
        let Some(chat) = &mut self.chat else { return };
        let who = chat.who;
        let contact = chat.contact;
        let text = mind::cut(std::mem::take(&mut chat.input).trim(), 220);
        chat.focus = true;
        if text.is_empty() || self.chat_pending(who) {
            return;
        }
        // Player messages take precedence over a resident's background request.
        if ai_chat && self.pending.as_ref().is_some_and(|p| !matches!(p.work, Work::Chat(..))) {
            ai.cancel_request();
            self.receive(None, game);
        }
        let moment = self.moment(game);
        let n = &self.residents[who];
        let p = n.person(Some(contact));
        let fallback = brain::rule_chat(&p, &text);
        if ai_chat
            && ai.request(Request {
                kind: ThinkKind::Chat,
                ids: vec![n.member.id.clone()],
                prompt: brain::chat_prompt(&p, &text, &moment),
            })
        {
            self.pending = Some(Pending {
                index: who,
                work: Work::Chat(text, fallback),
            });
        } else {
            self.apply_chat(who, &text, fallback, game);
        }
    }

    fn apply_chat(&mut self, i: usize, text: &str, result: ChatResult, game: &mut Game) {
        let now = self.clock;
        let n = &mut self.residents[i];
        let previous = mind::friendship(&n.mind);
        if !result.driver_name.is_empty() {
            n.mind.driver_name = Some(result.driver_name);
        }
        n.mind.feel("driver", result.opinion * 0.09);
        n.mind.remember(now, &result.memory, result.importance, &["driver"], "chat");
        n.mind.log_chat(now, text, &result.reply);
        n.mind.jolt(0.08 * result.opinion, &result.emotion);
        if let Some(drop) = result.release {
            n.mind.release(now, drop);
        }
        n.mind.resolve(now, &result.resolve);
        n.say(now, result.reply);
        let bond = mind::friendship(&n.mind);
        if previous != bond {
            game.toast(format!("{} now sees you as a {bond}", n.member.name), 2200.0);
        }
    }

    pub fn receive(&mut self, data: Option<Value>, game: &mut Game) {
        let Some(Pending { index: i, work }) = self.pending.take() else {
            return;
        };
        let n = &self.residents[i];
        match work {
            Work::Chat(text, fallback) => {
                let result = data.as_ref().and_then(|d| brain::read_chat(d, &n.mind)).unwrap_or(fallback);
                self.apply_chat(i, &text, result, game);
            }
            Work::Decide(situation, fallback) => {
                let result = data
                    .as_ref()
                    .and_then(|d| brain::read_decision(d, n.member, &n.mind, &situation))
                    .unwrap_or(fallback);
                self.act(i, result, game);
            }
            Work::Plan(fallback) => {
                let result = data.as_ref().and_then(brain::read_plan).unwrap_or(fallback);
                self.set_plan(i, result);
            }
            Work::Reflect(fallback) => {
                let result = data.as_ref().and_then(brain::read_reflect).unwrap_or(fallback);
                self.reflect(i, result);
            }
            Work::Talk(other, fallback) => {
                let ids = [n.member.id.clone(), self.residents[other].member.id.clone()];
                let result = data.as_ref().and_then(|d| brain::read_conversation(d, &ids)).unwrap_or(fallback);
                self.apply_talk(i, other, result);
            }
        }
    }

    pub fn abandon(&mut self, game: &mut Game) {
        self.receive(None, game);
        self.chat = None;
        self.journal = false;
        self.chase = None;
        self.trips.clear();
        self.passenger = None;
        self.find = None;
    }

    fn set_plan(&mut self, i: usize, plan: DayPlanResult) {
        let n = &mut self.residents[i];
        n.mind.set_goal(&plan.goal);
        n.mind.plan = plan
            .plan
            .into_iter()
            .map(|(hour, place, what)| DayStep { hour, place, what })
            .collect();
        n.mind.plan_day = day(self.clock) as f64;
    }

    fn reflect(&mut self, i: usize, result: ReflectResult) {
        let mind = &mut self.residents[i].mind;
        mind.pending = 0.0;
        for insight in result.insights {
            mind.add_insight(self.clock, &insight);
        }
        if let Some((trait_name, up)) = result.nudge {
            mind.nudge(trait_name, up, 0.03);
        }
        mind.add_habit(&result.habit);
        mind.set_story(&result.story);
    }

    fn decide(&mut self, i: usize, situation: Situation, game: &mut Game, ai: &mut LocalAi, use_ai: bool) {
        let moment = self.moment(game);
        let p = self.residents[i].person(None);
        let fallback = brain::rule_decide(&p, &moment, &situation);
        if use_ai
            && !ai.busy()
            && ai.request(Request {
                kind: ThinkKind::Decide,
                ids: vec![p.member.id.clone()],
                prompt: brain::decide_prompt(&p, &moment, &situation),
            })
        {
            self.pending = Some(Pending {
                index: i,
                work: Work::Decide(situation, fallback),
            });
        } else {
            self.act(i, fallback, game);
        }
    }

    fn act(&mut self, i: usize, decision: Decision, game: &mut Game) {
        let now = self.clock;
        // Away in another town, most things wait until they're back.
        if let Some(t) = self.residents[i].away
            && !["text the driver", "report the driver to the police"].contains(&decision.action.as_str())
        {
            let n = &mut self.residents[i];
            n.mind.think(&decision.thought);
            n.say(now, decision.say.clone());
            match decision.action.as_str() {
                "go home" => self.head_back(i),
                "something else" if !decision.doing.is_empty() => n.task = decision.doing,
                _ => n.task = format!("looking around {}", self.towns[t].name),
            }
            return;
        }
        let n = &mut self.residents[i];
        n.mind.think(&decision.thought);
        n.mind.set_goal(&decision.goal);
        if let Some(drop) = decision.release {
            n.mind.release(now, drop);
        }
        n.mind.resolve(now, &decision.resolve);
        n.say(now, decision.say.clone());
        let at = n.at;
        let person = decision.person.as_deref().and_then(|id| self.resident(id));
        let seek = person.map(|j| self.residents[j].at);
        match decision.action.as_str() {
            "chase the driver down" => self.start_chase(i, game),
            "call off the chase" => self.finish_chase("called off", game),
            "ticket the driver" if self.chase.is_none() => self.ticket(game),
            "let it go" => self.heat = (self.heat - 0.65).max(0.0),
            "go for a drive with GUY" | "drive somewhere" | "give someone a lift" => {
                let place = decision
                    .place
                    .as_deref()
                    .and_then(|k| self.map.place(k, at))
                    .or_else(|| self.map.place("park", at));
                if let Some(place) = place {
                    let rider = person.or_else(|| {
                        (decision.action == "go for a drive with GUY")
                            .then(|| self.resident("guy"))
                            .flatten()
                    });
                    if !self.start_trip(i, rider, place) {
                        self.residents[i].go_place(&self.map, place, now, "heading out");
                    }
                }
            }
            "go home" => {
                let n = &mut self.residents[i];
                n.go(&self.map, n.home, None, now, "going home");
            }
            "find someone" | "follow someone" | "avoid someone" if seek.is_some() => {
                let target = seek.unwrap_or(at);
                let to = if decision.action == "avoid someone" {
                    self.map.wander(at + (at - target).normalize_or_zero() * 90.0)
                } else {
                    target
                };
                self.residents[i].go(&self.map, to, None, now, decision.action);
            }
            "text the driver" if !decision.say.is_empty() => {
                let from = self.residents[i].member.id.clone();
                push_limited(
                    &mut self.messages,
                    Message {
                        from,
                        t: now,
                        text: decision.say,
                    },
                    20,
                );
                game.toast(format!("Message from {} · L to read", self.residents[i].member.name), 2400.0);
            }
            "take a trip out of town" => {
                if !self.leave_town(i) {
                    let n = &mut self.residents[i];
                    n.go(&self.map, self.map.wander(at), None, now, "wandering");
                }
            }
            "report the driver to the police" => {
                self.heat = (self.heat + 0.65).min(5.0);
                self.police_due = now;
            }
            "get up to mischief" => {
                let text = format!(
                    "{} pulled a prank near {}.",
                    self.residents[i].member.name,
                    self.residents[i].person(None).place
                );
                push_limited(&mut self.news, text, 10);
                self.residents[i].mind.jolt(0.08, "amused");
                self.residents[i].task = "getting up to mischief".into();
            }
            "wander" => self.residents[i].go(&self.map, self.map.wander(at), None, now, "wandering"),
            _ => {
                let n = &mut self.residents[i];
                if decision.action == "something else" && !decision.doing.is_empty() {
                    n.task = decision.doing;
                }
                let step = brain::current_step(&n.mind, hour(now));
                let kind = decision
                    .place
                    .as_deref()
                    .or(match decision.action.as_str() {
                        "work" => n.member.job,
                        "eat" => Some("diner"),
                        "rest" => Some("park"),
                        _ => None,
                    })
                    .or(step.map(|s| s.place.as_str()));
                let task = match decision.action.as_str() {
                    "eat" => "getting something to eat",
                    "rest" => "resting",
                    "work" => "working",
                    _ => step.map_or("out for a walk", |s| s.what.as_str()),
                }
                .to_string();
                if kind == Some("home") {
                    n.go(&self.map, n.home, None, now, "going home");
                } else if let Some(place) = kind.and_then(|k| self.map.place(k, at)) {
                    if n.place.is_none_or(|p| p.id != place.id) || n.at.distance(place.at) > 8.0 {
                        n.go_place(&self.map, place, now, task);
                    } else {
                        n.task = task;
                    }
                } else if decision.action != "something else" {
                    n.go(&self.map, self.map.wander(at), None, now, "wandering");
                }
            }
        }
    }

    fn apply_talk(&mut self, a: usize, b: usize, result: ConversationResult) {
        let now = self.clock;
        let where_at = self.residents[a].person(None).place;
        let ids = [self.residents[a].member.id.clone(), self.residents[b].member.id.clone()];
        for (i, other) in [(a, &ids[1]), (b, &ids[0])] {
            let mind = &mut self.residents[i].mind;
            *mind.met.entry(other.clone()).or_default() += 1.0;
            mind.needs.social = (mind.needs.social + 0.15).min(1.0);
        }
        for (speaker, text, importance) in result.memories {
            if let Some(i) = ids.iter().position(|id| id == &speaker).map(|i| [a, b][i]) {
                self.residents[i]
                    .mind
                    .remember(now, &text, importance, &[ids[usize::from(i == a)].as_str()], "talk");
            }
        }
        for (from, about, change) in result.feelings {
            if let Some(i) = self.resident(&from) {
                self.residents[i].mind.feel(&about, change * 0.065);
            }
        }
        if let Some(plan) = result.plan
            && let Some(place) = self.map.place(&plan.place, self.residents[a].at)
        {
            let delay = match plan.when.as_str() {
                "tomorrow" => 24.0 * 60.0,
                "later" => 180.0,
                _ => 60.0,
            };
            self.meetups.retain(|m| !m.ids.iter().all(|id| ids.contains(id)));
            self.meetups.push(Meetup {
                ids,
                place_id: place.id.into(),
                kind: place.kind.into(),
                at: now + delay,
            });
        }
        let mut lines = Vec::new();
        for line in result.lines {
            if let Some(i) = self.resident(&line.speaker) {
                self.residents[i].mind.jolt(0.03, &line.emotion);
                if !lines.iter().any(|l: &TalkLine| l.name == self.residents[i].member.name) {
                    self.residents[i].say(now, line.text.clone());
                }
            }
            lines.push(TalkLine {
                name: name_of(&line.speaker),
                text: line.text,
            });
        }
        if !lines.is_empty() {
            push_limited(
                &mut self.talks,
                Talk {
                    t: now,
                    place: where_at,
                    lines,
                    ai: result.ai,
                },
                30,
            );
        }
    }

    fn conversation(&mut self, game: &Game, ai: &mut LocalAi, use_ai: bool) {
        let now = self.clock;
        if now < self.conversation_due {
            return;
        }
        self.conversation_due = now + 2.0;
        let chatting = self.chat.as_ref().map(|c| c.who);
        let ready = |i: usize, n: &Resident| !n.asleep && !n.in_car && Some(i) != chatting && n.path.is_empty() && now - n.last_talk > 35.0;
        let pair = self.residents.iter().enumerate().find_map(|(i, n)| {
            if !ready(i, n) {
                return None;
            }
            self.residents
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(j, b)| ready(*j, b) && b.at.distance_squared(n.at) < 11.0 * 11.0)
                .map(|(j, _)| (i, j))
        });
        let Some((a, b)) = pair else { return };
        self.residents[a].last_talk = now;
        self.residents[b].last_talk = now;
        let moment = self.moment(game);
        let first = self.residents[a].person(None);
        let second = self.residents[b].person(None);
        let fallback = brain::rule_conversation(&first, &second, &moment);
        if use_ai
            && !ai.busy()
            && self.residents[a].at.distance_squared(position(game)) < 220.0_f64.powi(2)
            && ai.request(Request {
                kind: ThinkKind::Conversation,
                ids: vec![first.member.id.clone(), second.member.id.clone()],
                prompt: brain::conversation_prompt(&first, &second, &moment),
            })
        {
            self.pending = Some(Pending {
                index: a,
                work: Work::Talk(b, fallback),
            });
        } else {
            self.apply_talk(a, b, fallback);
        }
    }

    fn plan_next(&mut self, game: &mut Game, ai: &mut LocalAi, use_ai: bool) {
        let now = self.clock;
        if now < self.decision_due {
            return;
        }
        self.decision_due = now + 0.12;
        let len = self.residents.len();
        let Some(i) = (0..len).map(|n| (n + self.cursor) % len).find(|&i| {
            let n = &self.residents[i];
            !n.asleep
                && !n.in_car
                && n.path.is_empty()
                && n.next <= now
                && self.chat.as_ref().is_none_or(|c| c.who != i)
                && self.pending.as_ref().is_none_or(|p| p.index != i)
        }) else {
            return;
        };
        self.cursor = (i + 1) % len;
        self.residents[i].next = now + 20.0 + random() * 24.0;
        let n = &self.residents[i];
        let local = use_ai && !ai.busy() && n.at.distance_squared(position(game)) < 140.0_f64.powi(2);
        let moment = self.moment(game);
        let p = n.person(None);
        if n.mind.plan_day != day(now) as f64 {
            let fallback = brain::rule_plan(&p);
            if local
                && ai.request(Request {
                    kind: ThinkKind::Plan,
                    ids: vec![n.member.id.clone()],
                    prompt: brain::plan_prompt(&p, &moment),
                })
            {
                self.pending = Some(Pending {
                    index: i,
                    work: Work::Plan(fallback),
                });
            } else {
                self.set_plan(i, fallback);
            }
        } else if n.mind.pending >= 28.0 {
            let fallback = brain::rule_reflect(&p, &moment);
            if local
                && ai.request(Request {
                    kind: ThinkKind::Reflect,
                    ids: vec![n.member.id.clone()],
                    prompt: brain::reflect_prompt(&p, &moment),
                })
            {
                self.pending = Some(Pending {
                    index: i,
                    work: Work::Reflect(fallback),
                });
            } else {
                self.reflect(i, fallback);
            }
        } else {
            let nearby = self
                .residents
                .iter()
                .filter(|b| b.member.id != n.member.id && !b.asleep && !b.in_car && b.at.distance_squared(n.at) < 625.0)
                .map(|b| b.member.name.clone())
                .collect();
            self.decide(i, Situation::new("deciding what to do next", nearby), game, ai, local);
        }
    }

    fn happenings(&mut self, game: &Game) {
        for event in &game.happened {
            let (x, z, what, emotion, importance, reach, heat) = match *event {
                Happening::Crash { x, z, impact } if impact > 6.0 => (x, z, "crashed the car", "worried", 5.0, 45.0, 0.55),
                Happening::Destroyed { x, z, kind } => (x, z, kind.name(), "scared", 8.0, 70.0, 1.45),
                Happening::Explosion { x, z } => (x, z, "set off an explosion", "scared", 8.0, 85.0, 1.3),
                Happening::Jump { x, z, air_time } if air_time > 0.6 => (x, z, "landed a big jump", "excited", 6.0, 90.0, 0.0),
                Happening::NearMiss { x, z } => (x, z, "skimmed past a car", "worried", 4.0, 34.0, 0.35),
                Happening::Horn { x, z } => (x, z, "honked the horn", "annoyed", 2.0, 25.0, 0.0),
                Happening::Drift { x, z, points } if points > 100.0 => (x, z, "pulled off a drift", "excited", 5.0, 65.0, 0.0),
                Happening::Assault { x, z, knocked_out } => (
                    x,
                    z,
                    if knocked_out { "knocked someone out" } else { "attacked someone" },
                    "scared",
                    9.0,
                    60.0,
                    if knocked_out { 1.0 } else { 0.6 },
                ),
                Happening::Gunshot { x, z } => (x, z, "fired a gun", "scared", 9.0, 90.0, 0.7),
                Happening::CarTaken { x, z, occupied: true, .. } => (x, z, "pulled someone out of their car", "scared", 8.0, 50.0, 1.0),
                _ => continue,
            };
            let at = DVec2::new(x, z);
            let text = if matches!(event, Happening::Destroyed { .. }) {
                format!("The driver destroyed a {what}.")
            } else {
                format!("The driver {what}.")
            };
            let mut witnesses = 0;
            for n in &mut self.residents {
                if n.hidden(self.clock)
                    || n.down_until > self.clock
                    || n.at.distance_squared(at) > reach * reach
                    || self.clock - n.last_reaction < 2.0
                {
                    continue;
                }
                witnesses += 1;
                n.last_reaction = self.clock;
                n.mind.remember(
                    self.clock,
                    &format!("I saw: {}", text.trim_end_matches('.')),
                    importance,
                    &["driver"],
                    "saw",
                );
                n.mind.jolt(if heat > 0.0 { -0.16 } else { 0.12 }, emotion);
                n.mind.feel("driver", if heat > 0.0 { -0.06 } else { 0.025 });
                if emotion == "scared" {
                    n.evade = Some(((n.at - at).normalize_or_zero(), self.clock + 1.8));
                }
                if random() < 0.24 {
                    let speech = match emotion {
                        "scared" => "Look out!",
                        "excited" => "Did you see that?!",
                        _ => "Hey, careful!",
                    };
                    n.say(self.clock, speech.into());
                }
            }
            if witnesses > 0 {
                if self.news.first() != Some(&text) {
                    push_limited(&mut self.news, text.clone(), 10);
                }
                if heat > 0.0 {
                    self.last_crime = self.clock;
                    self.heat = (self.heat + heat).min(5.0);
                    if self.crimes.first() != Some(&text) {
                        push_limited(&mut self.crimes, text, 6);
                    }
                }
            }
        }
    }

    fn start_chase(&mut self, i: usize, game: &mut Game) {
        if self.chase.is_some() {
            return;
        }
        let officer = &self.residents[i];
        let (at, heading) = crate::roads::curb(officer.at);
        let partner = self.resident("guy").filter(|&j| {
            let n = &self.residents[j];
            !n.asleep
                && !n.in_car
                && n.at.distance_squared(officer.at) < 18.0_f64.powi(2)
                && !n.mind.forbids(mind::Vow::Chase, "ride with BUDDY on a chase")
        });
        for j in std::iter::once(i).chain(partner) {
            self.residents[j].in_car = true;
            self.residents[j].path.clear();
        }
        self.record.chases += 1;
        self.chase = Some(Chase {
            at,
            heading,
            speed: 0.0,
            hurt: 0.0,
            start: self.clock,
            lost: 0.0,
            stopped: 0.0,
            rethink: self.clock + 20.0,
            next_siren: self.clock,
            next_shot: self.clock + if self.heat >= 4.5 { 5.0 } else { 14.0 },
            next_sight: 0.0,
            sees_driver: false,
            damage: 0.0,
            partner,
            seen: Vec::new(),
        });
        game.toast("BUDDY is in pursuit", 3000.0);
    }

    fn ticket(&mut self, game: &mut Game) {
        self.tickets += 1;
        let fine = (self.heat * 35.0).ceil().min(game.career.coins).max(0.0);
        game.career.coins -= fine;
        self.record.fines += fine as u64;
        game.toast(format!("BUDDY wrote you a ticket · {fine:.0} coins"), 3000.0);
        self.heat = (self.heat - 2.0).max(0.0);
        self.police_due = self.clock + 14.0;
    }

    pub fn finish_chase(&mut self, outcome: &str, game: &mut Game) {
        let Some(chase) = self.chase.take() else { return };
        if let Some(i) = self.resident("buddy") {
            let n = &mut self.residents[i];
            n.at = chase.at;
            n.in_car = false;
            n.mind.remember(
                self.clock,
                &format!("I chased the driver. The chase ended: {outcome}."),
                8.0,
                &["driver"],
                "saw",
            );
        }
        if let Some(i) = chase.partner {
            let n = &mut self.residents[i];
            n.at = chase.at + DVec2::new(chase.heading.cos(), -chase.heading.sin()) * 2.0;
            n.in_car = false;
            n.mind.remember(
                self.clock,
                &format!("I rode with BUDDY. The chase ended: {outcome}."),
                6.0,
                &["buddy", "driver"],
                "saw",
            );
        }
        match outcome {
            "busted" => {
                self.record.busts += 1;
                self.ticket(game);
            }
            "escaped" | "wrecked" => {
                self.record.escapes += 1;
                game.toast("You lost BUDDY", 2600.0);
            }
            "called off" => {
                self.record.called_off += 1;
                game.toast("BUDDY called off the chase", 2600.0);
            }
            _ => {}
        }
        if outcome == "escaped" {
            self.heat = self.heat.min(3.0);
        } else {
            self.heat = 0.0;
            self.crimes.clear();
        }
        if outcome != "off" {
            push_limited(&mut self.news, format!("BUDDY's pursuit ended: {outcome}."), 10);
        }
        self.police_due = self.clock + 16.0;
    }

    pub fn police(
        &mut self,
        dt: f64,
        game: &mut Game,
        sound: &game::audio::Sound,
        world: &game::world::World,
        ai: &mut LocalAi,
        use_ai: bool,
        enabled: bool,
    ) {
        let now = self.clock;
        let player = position(game);
        let speed = game.physics.vx.hypot(game.physics.vz);
        let Some(officer) = self.resident("buddy") else { return };
        let mut situation = None;
        if let Some(c) = &mut self.chase {
            let distance = c.at.distance(player);
            let target = if distance > 32.0 {
                crate::roads::pursuit_target(c.at, player)
            } else {
                player
            };
            let to = target - c.at;
            let goal = to.x.atan2(to.y);
            let angle = (goal - c.heading + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
            c.heading += angle.clamp(-2.7 * dt, 2.7 * dt);
            let desired = if distance < 6.0 {
                2.0
            } else {
                (distance * 1.1).clamp(8.0, 43.0) * (1.0 - angle.abs().min(1.3) * 0.5)
            };
            c.speed += (desired - c.speed).clamp(-17.0 * dt, 10.0 * dt);
            let direction = DVec2::new(c.heading.sin(), c.heading.cos());
            c.at += direction * c.speed * dt;
            for i in std::iter::once(officer).chain(c.partner) {
                self.residents[i].at = c.at;
            }
            if now >= c.next_sight {
                c.next_sight = now + 0.25;
                let start = DVec3::new(c.at.x, 1.35, c.at.y);
                let end = DVec3::new(player.x, game.physics.y + 0.75, player.y);
                let ray = end - start;
                c.sees_driver = distance < 150.0 && world.raycast(start, ray.normalize_or_zero(), ray.length()).is_none();
            }
            c.lost = if !c.sees_driver {
                c.lost + dt
            } else {
                (c.lost - dt * 0.5).max(0.0)
            };
            c.stopped = if c.sees_driver && speed < 2.0 && distance < 16.0 {
                c.stopped + dt
            } else {
                (c.stopped - dt).max(0.0)
            };
            if c.sees_driver && (3.0..45.0).contains(&distance) && now >= c.next_shot && game.settings.weapons {
                c.next_shot = now + 1.4;
                c.damage += 5.0;
                game.shake = game.shake.max(0.18);
                sound.gunshot();
                if game.on_foot.is_some() {
                    game.player_hurt += 6.0;
                }
            }
            if c.sees_driver && distance < 2.7 && speed > 3.0 {
                let velocity = DVec2::new(game.physics.vx, game.physics.vz) - direction * c.speed;
                let impact = velocity.length().min(100.0);
                c.damage += impact * dt * 3.0;
                c.hurt += impact * dt * 0.014;
                let resistance = 1.0 - 1.8 * dt;
                game.physics.vx *= resistance;
                game.physics.vz *= resistance;
            }
            if now >= c.next_siren && distance < 110.0 {
                c.next_siren = now + 1.5;
                sound.horn("air");
            }
            for (i, n) in self.residents.iter_mut().enumerate() {
                if !n.asleep && !n.in_car && c.at.distance_squared(n.at) < 38.0_f64.powi(2) && !c.seen.contains(&i) {
                    c.seen.push(i);
                    n.mind.remember(
                        now,
                        "I saw BUDDY chase the driver through the city.",
                        7.0,
                        &["buddy", "driver"],
                        "saw",
                    );
                }
            }
            let outcome = if !enabled {
                Some("off")
            } else if c.hurt >= 1.0 {
                Some("wrecked")
            } else if c.lost >= 8.0 || distance > 320.0 {
                Some("escaped")
            } else if c.stopped >= 3.0 || c.damage >= 100.0 {
                Some("busted")
            } else {
                None
            };
            if let Some(outcome) = outcome {
                self.finish_chase(outcome, game);
            } else if now >= c.rethink {
                c.rethink = now + 20.0;
                situation = Some(Situation {
                    why: "reconsidering the chase".into(),
                    crimes: Some((self.crimes.clone(), self.heat)),
                    chase: Some(brain::ChaseState {
                        seconds: now - c.start,
                        distance,
                        hurt: c.hurt,
                        quiet: now - self.last_crime,
                        partner: c
                            .partner
                            .and_then(|i| {
                                let mind = &self.residents[i].mind;
                                if mind.feeling("driver") > 0.35 || mind.forbids(mind::Vow::Chase, "this pursuit") {
                                    Some("Come on, BUDDY, you can let them go.")
                                } else if mind.feeling("driver") < -0.35 {
                                    Some("Don't let them get away!")
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_default()
                            .into(),
                    }),
                    ..default()
                });
            }
        } else if enabled && self.heat >= 2.0 && now >= self.police_due && !self.residents[officer].asleep {
            self.police_due = now + 12.0;
            situation = Some(Situation {
                why: "considering the driver's recent offenses".into(),
                crimes: Some((self.crimes.clone(), self.heat)),
                ..default()
            });
        }
        if let Some(situation) = situation {
            self.decide(officer, situation, game, ai, use_ai);
        }
        if now - self.last_crime > 22.0 && self.chase.is_none() {
            self.heat = (self.heat - dt * 0.052).max(0.0);
        }
    }

    fn start_trip(&mut self, driver: usize, rider: Option<usize>, place: Place) -> bool {
        let n = &self.residents[driver];
        if self.trips.len() >= 3 || n.in_car || n.at.distance(place.at) < 35.0 {
            return false;
        }
        let (at, heading) = crate::roads::curb(n.at);
        let (end, _) = crate::roads::curb(place.at);
        let rider = rider.filter(|&i| {
            i != driver && !self.residents[i].in_car && !self.residents[i].asleep && self.residents[i].at.distance(n.at) < 15.0
        });
        self.trips.push(Trip {
            at,
            heading,
            driver,
            rider,
            destination: place,
            roads: crate::roads::route(at, end),
            out: None,
            back: false,
        });
        self.residents[driver].in_car = true;
        if let Some(i) = rider {
            self.residents[i].in_car = true;
        }
        true
    }

    fn transport(&mut self, dt: f64, game: &mut Game) {
        let mut finished = Vec::new();
        for (j, t) in self.trips.iter_mut().enumerate() {
            if let Some(&next) = t.roads.front() {
                let delta = next - t.at;
                // Out on the highway, they put their foot down.
                let speed = if t.at.x.abs().max(t.at.y.abs()) > 300.0 { 24.0 } else { 12.0 };
                if delta.length() < 0.7 {
                    t.roads.pop_front();
                } else {
                    t.heading = delta.x.atan2(delta.y);
                    t.at += delta.clamp_length_max(speed * dt);
                }
            } else {
                finished.push(j);
            }
        }
        for j in finished.into_iter().rev() {
            let t = self.trips.remove(j);
            let now = self.clock;
            for i in std::iter::once(t.driver).chain(t.rider) {
                let n = &mut self.residents[i];
                n.at = t.at;
                n.in_car = false;
                match (t.out, t.back) {
                    (Some(town), false) => {
                        let name = self.towns[town].name.clone();
                        n.away = Some(town);
                        n.away_until = now + 120.0 + random() * 150.0;
                        n.task = format!("visiting {name}");
                        n.path.clear();
                        n.next = now + 5.0;
                        n.mind
                            .remember(now, &format!("I drove out to {name} for the day."), 5.0, &[], "did");
                        n.mind.jolt(0.12, "excited");
                    }
                    (_, true) => {
                        n.away = None;
                        n.go(&self.map, n.home, None, now, "home after a day out");
                    }
                    _ => n.go_place(&self.map, t.destination, now, "arriving after a drive"),
                }
            }
        }
        if let Some((i, place)) = self.passenger {
            let at = position(game);
            self.residents[i].at = at;
            if at.distance(place.at) < 15.0 && game.physics.vx.hypot(game.physics.vz) < 3.0 {
                self.drop_passenger(game, true);
            }
        }
    }

    /// Drives out to another town for a few hours.
    fn leave_town(&mut self, i: usize) -> bool {
        let n = &self.residents[i];
        if self.towns.is_empty() || self.trips.len() >= 3 || n.in_car || n.away.is_some() {
            return false;
        }
        let t = (random() * self.towns.len() as f64) as usize % self.towns.len();
        let town = &self.towns[t];
        let Some(&first) = town.route.first() else { return false };
        let (at, heading) = crate::roads::curb(n.at);
        let mut roads = crate::roads::route(at, first);
        roads.extend(town.route.iter().copied());
        let name = town.name.clone();
        self.trips.push(Trip {
            at,
            heading,
            driver: i,
            rider: None,
            destination: Place {
                id: "home",
                kind: "home",
                label: "home",
                at: n.home,
            },
            roads,
            out: Some(t),
            back: false,
        });
        let now = self.clock;
        let n = &mut self.residents[i];
        n.in_car = true;
        n.task = format!("driving out to {name}");
        n.say(now, format!("Off to {name} for the day!"));
        push_limited(&mut self.news, format!("{} drove out to {name}.", n.member.name), 10);
        true
    }

    /// Drives back to the city from another town.
    fn head_back(&mut self, i: usize) {
        let Some(t) = self.residents[i].away else { return };
        if self.residents[i].in_car {
            return;
        }
        let town = &self.towns[t];
        let mut roads: VecDeque<DVec2> = town.route.iter().rev().copied().collect();
        let edge = *town.route.first().unwrap_or(&DVec2::ZERO);
        let home = self.residents[i].home;
        let (end, _) = crate::roads::curb(home);
        roads.extend(crate::roads::route(edge, end));
        let at = self.residents[i].at;
        self.trips.push(Trip {
            at,
            heading: 0.0,
            driver: i,
            rider: None,
            destination: Place {
                id: "home",
                kind: "home",
                label: "home",
                at: home,
            },
            roads,
            out: Some(t),
            back: true,
        });
        let n = &mut self.residents[i];
        n.in_car = true;
        n.task = "driving home".into();
    }

    /// The driver, on foot, punched or shot resident `i`.
    pub fn struck(&mut self, i: usize, damage: f64, gun: bool, game: &mut Game) {
        let now = self.clock;
        let n = &mut self.residents[i];
        if n.down_until > now || n.hospital_until > now || n.hidden(now) {
            return;
        }
        n.hurt += damage;
        n.path.clear();
        n.evade = None;
        let (name, id) = (n.member.name.clone(), n.member.id.clone());
        self.last_crime = now;
        if n.hurt >= 100.0 {
            n.down_until = now + 8.0;
            n.fighting_until = 0.0;
            n.task = "knocked out".into();
            n.mind.remember(
                now,
                "The driver knocked me out. I woke up in the hospital.",
                10.0,
                &["driver"],
                "saw",
            );
            n.mind.feel("driver", -0.7);
            n.mind.jolt(-0.5, "scared");
            let text = format!("The driver knocked out {name}.");
            push_limited(&mut self.news, text.clone(), 10);
            push_limited(&mut self.crimes, text, 6);
            self.heat = (self.heat + 2.2).min(5.0);
            // Word gets round to their friends.
            for (j, other) in self.residents.iter_mut().enumerate() {
                if j != i && other.mind.feeling(&id) > 0.3 {
                    other.mind.remember(
                        now,
                        &format!("{name} got knocked out by the driver."),
                        7.0,
                        &["driver", id.as_str()],
                        "heard",
                    );
                    other.mind.feel("driver", -0.25);
                }
            }
            game.toast(format!("{name} is knocked out"), 1600.0);
        } else {
            n.mind.remember(
                now,
                if gun { "The driver shot me!" } else { "The driver hit me!" },
                9.0,
                &["driver"],
                "saw",
            );
            n.mind.feel("driver", -0.35);
            n.mind.jolt(-0.3, "angry");
            if !gun && n.mind.traits.courage > 0.6 {
                n.fighting_until = now + 12.0;
                n.say(now, "You want a fight?!".into());
            } else {
                let away = (n.at - position(game)).normalize_or(DVec2::X);
                n.evade = Some((away, now + 4.0));
                n.say(now, "Help! Somebody help!".into());
            }
            self.heat = (self.heat + 1.2).min(5.0);
            push_limited(&mut self.crimes, format!("The driver attacked {name}."), 6);
        }
        self.police_due = self.police_due.min(now + 2.0);
    }

    /// The driver took the car resident `driver` was driving.
    pub fn car_taken(&mut self, driver: usize, game: &mut Game) {
        let Some(k) = self.trips.iter().position(|t| t.driver == driver) else {
            return;
        };
        let t = self.trips.remove(k);
        let now = self.clock;
        let side = DVec2::new(t.heading.cos(), -t.heading.sin());
        for (j, who) in std::iter::once(t.driver).chain(t.rider).enumerate() {
            let n = &mut self.residents[who];
            n.in_car = false;
            n.at = t.at + side * (2.0 + j as f64);
            n.away = None;
            n.mind
                .remember(now, "The driver took my car! Pulled me right out of it.", 9.0, &["driver"], "saw");
            n.mind.feel("driver", -0.5);
            n.mind.jolt(-0.35, "angry");
            n.say(now, "Hey! That's my car!".into());
            n.go(&self.map, n.home, None, now, "walking home without a car");
        }
        let name = self.residents[t.driver].member.name.clone();
        let text = format!("The driver took {name}'s car.");
        push_limited(&mut self.news, text.clone(), 10);
        push_limited(&mut self.crimes, text, 6);
        self.heat = (self.heat + 1.6).min(5.0);
        self.last_crime = now;
        game.toast(format!("You took {name}'s car"), 1800.0);
    }

    /// The driver asked resident `i` to borrow a car.
    pub fn lend_car(&mut self, i: usize, game: &mut Game) {
        let now = self.clock;
        let n = &mut self.residents[i];
        if n.mind.feeling("driver") > 0.2 && !n.mind.forbids(mind::Vow::Ride, "lend the driver my car") {
            n.mind.remember(now, "I lent the driver my car.", 6.0, &["driver"], "did");
            n.mind.feel("driver", 0.03);
            n.say(now, "Sure, take mine. Bring it back in one piece!".into());
            game.loan = Some(n.member.name.clone());
        } else {
            n.say(now, "Lend YOU my car? Not a chance.".into());
            n.mind
                .remember(now, "The driver asked to borrow my car. I said no.", 4.0, &["driver"], "did");
            game.toast(format!("{} won't lend you a car", n.member.name), 2000.0);
        }
    }

    pub fn offer_ride(&mut self, i: usize, game: &mut Game) {
        if self.passenger.is_some()
            || self.residents[i].in_car
            || !mind::is_friend(&self.residents[i].mind)
            || position(game).distance(self.residents[i].at) > 15.0
        {
            return;
        }
        let n = &self.residents[i];
        if n.mind.forbids(mind::Vow::Ride, "a ride with the driver") {
            game.toast(format!("{} passes on the ride", n.member.name), 1800.0);
            return;
        }
        let dest = n
            .member
            .likes
            .iter()
            .filter_map(|k| self.map.place(k, n.at))
            .find(|p| p.at.distance(n.at) > 45.0)
            .or_else(|| self.map.place("park", n.at));
        if let Some(place) = dest {
            self.passenger = Some((i, place));
            self.residents[i].in_car = true;
            game.toast(format!("Drive {} to {}", self.residents[i].member.name, place.label), 3000.0);
            if let Some(chat) = &mut self.chat {
                chat.contact = Contact::Passenger;
            }
        }
    }

    pub fn drop_passenger(&mut self, game: &mut Game, arrived: bool) {
        let Some((i, place)) = self.passenger.take() else { return };
        let n = &mut self.residents[i];
        n.in_car = false;
        n.at += DVec2::new(game.physics.heading.cos(), -game.physics.heading.sin()) * 2.6;
        if arrived {
            n.mind.remember(
                self.clock,
                &format!("The driver gave me a lift to {}.", place.label),
                7.0,
                &["driver"],
                "saw",
            );
            n.mind.feel("driver", 0.14);
            n.go_place(&self.map, place, self.clock, "arriving after a ride");
            game.toast(format!("{} thanks you for the ride", n.member.name), 2300.0);
        } else {
            n.mind
                .remember(self.clock, "The driver dropped me off along the way.", 4.0, &["driver"], "saw");
        }
        self.chat = None;
    }

    pub fn tick(&mut self, dt: f64, game: &mut Game, storage: &mut Storage, ai: &mut LocalAi, use_ai: bool) {
        self.clock += dt;
        let now = self.clock;
        let h = hour(now);
        let player = position(game);
        let moving = game.physics.vx.hypot(game.physics.vz) > 4.0;
        let chatting = self.chat.as_ref().filter(|c| c.contact == Contact::Beside).map(|c| c.who);
        let today = day(now);
        if today % 7 == 5
            && (19.0..22.5).contains(&h)
            && self.party_day != Some(today)
            && let Some(park) = self.map.place("park0", player)
        {
            self.party_day = Some(today);
            push_limited(&mut self.news, format!("The Saturday street party is on at {}.", park.label), 10);
            game.toast(format!("Saturday street party · {}", park.label), 3500.0);
            for (i, n) in self.residents.iter_mut().enumerate() {
                if !n.asleep && !n.in_car && Some(i) != chatting && n.mind.traits.energy >= 0.45 {
                    n.go_place(&self.map, park, now, "heading to the Saturday street party");
                    n.next = now + 120.0;
                }
            }
        }
        self.happenings(game);
        let meeting_now: Vec<_> = self.meetups.iter().filter(|m| m.at <= now && m.at > now - dt).cloned().collect();
        self.meetups.retain(|m| m.at + 600.0 > now);
        for m in meeting_now {
            for id in &m.ids {
                if let Some(i) = self.resident(id)
                    && let Some(place) = self.map.place(&m.place_id, self.residents[i].at)
                    && !self.residents[i].asleep
                    && !self.residents[i].in_car
                {
                    self.residents[i].go_place(&self.map, place, now, "keeping a meetup");
                }
            }
        }
        let mut away_done = Vec::new();
        for (i, n) in self.residents.iter_mut().enumerate() {
            let still = n.path.is_empty();
            let kind = n.place.map_or("", |p| p.kind);
            n.mind.tick_needs(
                dt,
                Some(i) == chatting || now - n.last_talk < 8.0,
                still && ["diner", "bakery", "coffee"].contains(&kind),
                n.asleep || still && kind == "park",
                still && ["park", "stunts", "music"].contains(&kind),
            );
            n.speed = 0.0;
            // Knocked out: lying there, then off to the hospital until the morning.
            if n.down_until > 0.0 && now >= n.down_until && n.hurt >= 100.0 {
                n.down_until = 0.0;
                n.hurt = 0.0;
                n.hospital_until = (day(now) + 1) as f64 * 24.0 * 60.0;
                n.path.clear();
                n.away = None;
                n.task = "in the hospital".into();
            }
            if n.down_until > now {
                continue;
            }
            if n.hospital_until > 0.0 {
                if now < n.hospital_until {
                    continue;
                }
                n.hospital_until = 0.0;
                if let Some(h) = self.map.place("hospital", n.home) {
                    n.at = h.at;
                }
                n.go(&self.map, n.home, None, now, "home from the hospital");
                n.mind
                    .remember(now, "I got out of the hospital this morning.", 6.0, &["driver"], "did");
            }
            if n.in_car || Some(i) == chatting {
                continue;
            }
            n.hurt = (n.hurt - dt * 0.5).max(0.0);
            // Fighting back, if the driver is on foot.
            if n.fighting_until > now {
                if game.on_foot.is_some() {
                    let to = player - n.at;
                    let d = to.length();
                    n.heading = to.x.atan2(to.y);
                    if d > 1.1 {
                        n.at += to / d * 3.6 * dt;
                        n.speed = 3.6;
                    } else if now >= n.punch_due {
                        n.punch_due = now + 1.2;
                        game.player_hurt += 7.0;
                    }
                    if d > 40.0 {
                        n.fighting_until = 0.0;
                    }
                    continue;
                }
                n.fighting_until = 0.0;
            }
            // Away in another town: wander about it, then head home.
            if let Some(t) = n.away {
                let town = &self.towns[t];
                if now >= n.away_until && n.path.is_empty() {
                    away_done.push(i);
                    continue;
                }
                if n.path.is_empty() && now >= n.next {
                    let spread = (town.half - 20.0).max(10.0);
                    let to = town.at + DVec2::new(centered(), centered()) * 2.0 * spread;
                    n.path = VecDeque::from([to]);
                    n.next = now + 20.0 + random() * 30.0;
                }
            }
            let awake = h >= 5.5 + (1.0 - n.mind.traits.energy) * 2.0 && h < 22.5 + n.mind.traits.energy * 1.5;
            if n.asleep {
                if awake {
                    n.asleep = false;
                    n.next = now + random() * 12.0;
                    n.task = "waking up".into();
                }
                continue;
            }
            if !awake && n.task != "going home" && n.away.is_none() {
                n.go(&self.map, n.home, None, now, "going home");
            }
            if n.task == "going home" && n.at.distance_squared(n.home) < 1.0 {
                if !awake {
                    n.asleep = true;
                    n.task = "asleep at home".into();
                } else {
                    n.task = "resting at home".into();
                }
            }
            let delta = n.at - player;
            if moving && delta.length_squared() < (game.car_radius() + 2.0).powi(2) {
                n.evade = Some((delta.normalize_or(DVec2::X), now + 0.9));
            }
            if let Some((away, until)) = n.evade {
                if now < until {
                    n.at += away * 4.5 * dt;
                    n.heading = away.x.atan2(away.y);
                    n.speed = 4.5;
                    continue;
                }
                n.evade = None;
            }
            if let Some(&next) = n.path.front() {
                let delta = next - n.at;
                if delta.length_squared() < 0.3 {
                    n.path.pop_front();
                    if n.path.is_empty()
                        && let Some(place) = n.place
                    {
                        n.mind.learn_place(place.kind, 0.03 + n.mind.mood * 0.15);
                    }
                } else {
                    n.speed = 1.55 + n.mind.traits.energy * 1.2;
                    n.heading = delta.x.atan2(delta.y);
                    n.at += delta.clamp_length_max(n.speed * dt);
                }
            }
        }
        for i in away_done {
            self.head_back(i);
        }
        self.transport(dt, game);
        self.plan_next(game, ai, use_ai);
        self.conversation(game, ai, use_ai);
        if self.clock >= self.save_due {
            self.save(storage);
            game::career::save_career(storage, &game.career);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_city_round_trips_through_native_storage() {
        let mut storage = Storage::in_memory();
        storage.set_json(
            SAVE_KEY,
            &json!({
                "version": 3, "clock": 500, "tickets": 2, "heat": 2.5,
                "record": {"calledOff": 1},
                "minds": {"ava": {
                    "driverName": "Sam", "feelings": {"driver":0.5},
                    "memories": [{"t":40,"importance":7,"text":"Sam gave me a ride.","about":["driver"],"kind":"saw"}],
                    "resolutions": ["I'll keep the coffee cart open until dusk."]
                }},
                "messages":[{"from":"ava","text":"Thanks for the ride!","t":40}],
                "meetups":[{"ids":["ava","guy"],"placeId":"coffee","kind":"coffee","at":540}]
            }),
        );
        let mut city = City::restore(&storage);
        city.save(&mut storage);
        let restored = City::restore(&storage);
        let ava = &restored.residents[restored.resident("ava").unwrap()].mind;
        assert_eq!(restored.tickets, 2);
        assert_eq!(restored.record.called_off, 1);
        assert_eq!(restored.meetups[0].at, 540.0);
        assert_eq!(restored.messages[0].text, "Thanks for the ride!");
        assert_eq!(ava.driver_name.as_deref(), Some("Sam"));
        assert_eq!(ava.feeling("driver"), 0.5);
        assert_eq!(ava.memories[0].text, "Sam gave me a ride.");
        assert_eq!(ava.resolutions, ["I'll keep the coffee cart open until dusk."]);
    }
}

pub mod map {
    //! Sidewalk routes, storefronts and residents' front doors.
    use std::collections::{HashMap, VecDeque};

    use bevy::math::DVec2;
    use game::world::{BlockKind, ROAD_LINES, World};

    const LOOP: f64 = 32.0;
    const SIDES: [DVec2; 4] = [DVec2::NEG_Y, DVec2::X, DVec2::Y, DVec2::NEG_X];

    #[derive(Clone, Copy)]
    pub struct Place {
        pub id: &'static str,
        pub kind: &'static str,
        pub label: &'static str,
        pub at: DVec2,
    }

    #[derive(Default)]
    pub struct CityMap {
        pub places: Vec<Place>,
        homes: Vec<DVec2>,
        corners: Vec<DVec2>,
        edges: Vec<Vec<(usize, f64)>>,
    }

    impl CityMap {
        pub fn build(world: &World) -> Self {
            let blocks = |kind| {
                let mut list: Vec<_> = world
                    .blocks
                    .iter()
                    .filter(|b| b.kind == kind)
                    .map(|b| DVec2::new(b.x, b.z))
                    .collect();
                list.sort_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
                list
            };
            let homes = blocks(BlockKind::City);
            let shops = [
                ("coffee", "the coffee cart"),
                ("bank", "the bank"),
                ("diner", "the diner"),
                ("gym", "the gym"),
                ("music", "the music corner"),
                ("news", "the newsstand"),
                ("bakery", "the bakery"),
                ("library", "the library"),
                ("office", "the office"),
                ("market", "the market"),
            ];
            let mut places = Vec::new();
            if !homes.is_empty() {
                for (i, (kind, label)) in shops.into_iter().enumerate() {
                    let block = homes[(i / 2) % homes.len()];
                    places.push(Place {
                        id: kind,
                        kind,
                        label,
                        at: block + SIDES[(i * 3 + i / 2) % 4] * LOOP,
                    });
                }
            }
            if let Some(block) = homes.get(7).or(homes.last()) {
                places.push(Place {
                    id: "hospital",
                    kind: "hospital",
                    label: "the hospital",
                    at: *block + DVec2::new(0.0, -LOOP),
                });
            }
            if let Some(block) = blocks(BlockKind::Lot).first().or_else(|| homes.get(4)) {
                places.push(Place {
                    id: "garage",
                    kind: "garage",
                    label: "the garage",
                    at: *block + DVec2::new(18.0, -28.0),
                });
            }
            for (i, block) in blocks(BlockKind::Park).into_iter().take(2).enumerate() {
                let (id, label, x) = if i == 0 {
                    ("park0", "the park", -6.0)
                } else {
                    ("park1", "the north park", 6.0)
                };
                places.push(Place {
                    id,
                    kind: "park",
                    label,
                    at: block + DVec2::new(x, 4.0),
                });
            }
            if let Some(block) = blocks(BlockKind::Stunt).first() {
                places.push(Place {
                    id: "stunts",
                    kind: "stunts",
                    label: "the stunt park",
                    at: *block + DVec2::new(-29.0, 0.0),
                });
            }

            let mut corners = Vec::new();
            let mut index = HashMap::new();
            for x in ROAD_LINES[..6].iter().map(|x| x + 40.0) {
                for z in ROAD_LINES[..6].iter().map(|z| z + 40.0) {
                    for (dx, dz) in [(-LOOP, -LOOP), (LOOP, -LOOP), (LOOP, LOOP), (-LOOP, LOOP)] {
                        let at = DVec2::new(x + dx, z + dz);
                        index.insert((at.x as i32, at.y as i32), corners.len());
                        corners.push(at);
                    }
                }
            }
            let edges = corners
                .iter()
                .map(|at| {
                    let mut edges = Vec::new();
                    for direction in SIDES {
                        for distance in [16.0, 64.0] {
                            let to = *at + direction * distance;
                            if let Some(i) = index.get(&(to.x as i32, to.y as i32)) {
                                edges.push((*i, distance));
                            }
                        }
                    }
                    edges
                })
                .collect();
            Self {
                places,
                homes,
                corners,
                edges,
            }
        }

        pub fn home(&self, id: &str) -> DVec2 {
            let hash = id.bytes().fold(2166136261_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
            if self.homes.is_empty() {
                return DVec2::ZERO;
            }
            let block = self.homes[hash as usize % self.homes.len()];
            let side = SIDES[((hash >> 8) % 4) as usize];
            let along = (f64::from((hash >> 12) % 1000) / 1000.0 - 0.5) * 52.0;
            block + side * LOOP + DVec2::new(side.y.abs(), side.x.abs()) * along
        }

        pub fn nearest_place(&self, at: DVec2) -> Option<&Place> {
            self.places
                .iter()
                .min_by(|a, b| a.at.distance_squared(at).total_cmp(&b.at.distance_squared(at)))
        }

        pub fn place(&self, kind: &str, at: DVec2) -> Option<Place> {
            self.places
                .iter()
                .filter(|p| p.id == kind || p.kind == kind)
                .min_by(|a, b| a.at.distance_squared(at).total_cmp(&b.at.distance_squared(at)))
                .copied()
        }

        fn nearest_corner(&self, at: DVec2) -> usize {
            self.corners
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
                .map_or(0, |(i, _)| i)
        }

        pub fn wander(&self, at: DVec2) -> DVec2 {
            let offset = DVec2::new(game::rng::centered(), game::rng::centered()) * 160.0;
            self.corners.get(self.nearest_corner(at + offset)).copied().unwrap_or(at)
        }

        pub fn route(&self, from: DVec2, to: DVec2) -> VecDeque<DVec2> {
            if from.distance_squared(to) < 40.0_f64.powi(2) || self.corners.is_empty() {
                return VecDeque::from([to]);
            }
            let start = self.nearest_corner(from);
            let goal = self.nearest_corner(to);
            let mut dist = vec![f64::INFINITY; self.corners.len()];
            let mut previous = vec![None; self.corners.len()];
            let mut done = vec![false; self.corners.len()];
            dist[start] = 0.0;
            while let Some(i) = (0..dist.len())
                .filter(|&i| !done[i] && dist[i].is_finite())
                .min_by(|&a, &b| dist[a].total_cmp(&dist[b]))
            {
                if i == goal {
                    break;
                }
                done[i] = true;
                for &(next, cost) in &self.edges[i] {
                    if dist[i] + cost < dist[next] {
                        dist[next] = dist[i] + cost;
                        previous[next] = Some(i);
                    }
                }
            }
            let mut path = VecDeque::from([to]);
            let mut node = Some(goal);
            while let Some(i) = node {
                path.push_front(self.corners[i]);
                node = previous[i];
            }
            path
        }
    }
}

pub mod roads {
    //! Routes along the city's streets and parking beside the sidewalk.
    use std::collections::VecDeque;

    use bevy::math::DVec2;
    use game::world::ROAD_LINES;

    fn road(at: f64) -> f64 {
        ROAD_LINES
            .into_iter()
            .min_by(|a, b| (*a - at).abs().total_cmp(&(*b - at).abs()))
            .unwrap_or(0.0)
    }

    fn vertical(at: DVec2) -> bool {
        (road(at.x) - at.x).abs() <= (road(at.y) - at.y).abs()
    }

    pub fn curb(at: DVec2) -> (DVec2, f64) {
        if vertical(at) {
            let line = road(at.x);
            let side = if at.x >= line { 1.0 } else { -1.0 };
            (
                DVec2::new(line + 4.6 * side, at.y.clamp(-231.0, 231.0)),
                if side > 0.0 { 0.0 } else { std::f64::consts::PI },
            )
        } else {
            let line = road(at.y);
            let side = if at.y >= line { 1.0 } else { -1.0 };
            (
                DVec2::new(at.x.clamp(-231.0, 231.0), line + 4.6 * side),
                -side * std::f64::consts::FRAC_PI_2,
            )
        }
    }

    pub fn route(from: DVec2, to: DVec2) -> VecDeque<DVec2> {
        let a = DVec2::new(road(from.x), road(from.y));
        let b = DVec2::new(road(to.x), road(to.y));
        let corner = if vertical(from) {
            DVec2::new(a.x, b.y)
        } else {
            DVec2::new(b.x, a.y)
        };
        let mut path = VecDeque::new();
        for point in [a, corner, b, to] {
            if path.back() != Some(&point) && from.distance_squared(point) > 1.0 {
                path.push_back(point);
            }
        }
        path
    }

    pub fn pursuit_target(from: DVec2, to: DVec2) -> DVec2 {
        let a = DVec2::new(road(from.x), road(from.y));
        let b = DVec2::new(road(to.x), road(to.y));
        if a.distance_squared(from) < 11.0_f64.powi(2) {
            if (a.x - b.x).abs() > (a.y - b.y).abs() {
                DVec2::new(b.x, a.y)
            } else if a.y != b.y {
                DVec2::new(a.x, b.y)
            } else {
                to
            }
        } else if vertical(from) {
            let z = if (b.x - a.x).abs() < 0.01 { to.y } else { a.y };
            DVec2::new(a.x, z)
        } else {
            let x = if (b.y - a.y).abs() < 0.01 { to.x } else { a.x };
            DVec2::new(x, a.y)
        }
    }
}

pub mod visual {
    //! Shared geometry for the residents, shops and cars.
    use std::collections::HashMap;

    use bevy::prelude::*;
    use game::camera::CameraRig;
    use game::world::World;

    use crate::cast::{CastMember, Extra, HairStyle, Hat};
    use crate::city::City;

    #[derive(Component)]
    pub struct CityRoot;

    #[derive(Component)]
    pub struct Body {
        index: usize,
        arms: [Entity; 2],
        legs: [Entity; 2],
    }

    #[derive(Component)]
    pub struct CityCar(pub Option<usize>);

    #[derive(Component)]
    pub struct Finder;

    #[derive(Resource)]
    pub struct Visuals {
        cube: Handle<Mesh>,
        sphere: Handle<Mesh>,
        cylinder: Handle<Mesh>,
        colors: HashMap<u32, Handle<StandardMaterial>>,
    }

    impl Visuals {
        pub fn new(meshes: &mut Assets<Mesh>) -> Self {
            Self {
                cube: meshes.add(Cuboid::from_length(1.0)),
                sphere: meshes.add(Sphere::new(0.5).mesh().uv(12, 8)),
                cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(12)),
                colors: HashMap::new(),
            }
        }

        fn color(&mut self, materials: &mut Assets<StandardMaterial>, color: u32) -> Handle<StandardMaterial> {
            self.colors
                .entry(color)
                .or_insert_with(|| {
                    materials.add(StandardMaterial {
                        base_color: Color::srgb_u8((color >> 16) as u8, (color >> 8) as u8, color as u8),
                        perceptual_roughness: 0.84,
                        ..default()
                    })
                })
                .clone()
        }
    }

    fn part(
        commands: &mut Commands,
        parent: Entity,
        mesh: &Handle<Mesh>,
        material: &Handle<StandardMaterial>,
        pos: [f32; 3],
        size: [f32; 3],
    ) -> Entity {
        commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(Vec3::from_array(pos)).with_scale(Vec3::from_array(size)),
                ChildOf(parent),
            ))
            .id()
    }

    pub fn spawn_resident(
        commands: &mut Commands,
        visuals: &mut Visuals,
        materials: &mut Assets<StandardMaterial>,
        member: &CastMember,
        index: usize,
    ) -> Entity {
        let look = member.look;
        let skin = visuals.color(materials, look.skin);
        let shirt = visuals.color(materials, look.shirt);
        let hair = visuals.color(materials, look.hair);
        let trousers = visuals.color(materials, look.trousers);
        let shoes = visuals.color(materials, look.shoes);
        let black = visuals.color(materials, 0x211f23);
        let root = commands.spawn((CityRoot, Transform::default(), Visibility::Hidden)).id();
        let width = look.build as f32;
        let cube = &visuals.cube;
        let sphere = &visuals.sphere;
        part(commands, root, cube, &shirt, [0.0, 1.05, 0.0], [0.39 * width, 0.58, 0.24]);
        part(commands, root, sphere, &skin, [0.0, 1.48, 0.0], [0.32, 0.39, 0.31]);
        if look.hair_style != HairStyle::Bald {
            let size = match look.hair_style {
                HairStyle::Long => [0.36, 0.32, 0.27],
                HairStyle::Curly => [0.38, 0.21, 0.35],
                HairStyle::Spiky => [0.32, 0.25, 0.28],
                _ => [0.34, 0.16, 0.34],
            };
            part(commands, root, sphere, &hair, [0.0, 1.62, -0.025], size);
            if look.hair_style == HairStyle::Bun {
                part(commands, root, sphere, &hair, [0.0, 1.63, -0.17], [0.14; 3]);
            }
        }
        for x in [-0.07, 0.07] {
            let size = if look.glasses { [0.12, 0.052, 0.035] } else { [0.024, 0.03, 0.022] };
            part(commands, root, cube, &black, [x, 1.51, 0.14], size);
        }
        let hat_color = match look.hat {
            Hat::None => None,
            Hat::Police => Some(0x182d49),
            Hat::Beanie => Some(0xa43255),
            Hat::Cap => Some(look.shirt),
        };
        if let Some(color) = hat_color {
            let mat = visuals.color(materials, color);
            part(commands, root, &visuals.cylinder, &mat, [0.0, 1.65, 0.0], [0.36, 0.12, 0.35]);
            if look.hat != Hat::Beanie {
                part(commands, root, &visuals.cube, &mat, [0.0, 1.61, 0.15], [0.28, 0.035, 0.22]);
            }
        }
        let accessory = match look.extra {
            Extra::None => None,
            Extra::Badge => Some((0xffcf65, [0.065, 0.078, 0.02], [0.11, 1.17, 0.13])),
            Extra::Apron => Some((0xf5e7d4, [0.29, 0.48, 0.02], [0.0, 0.99, 0.13])),
            Extra::Tie => Some((0x8c284c, [0.065, 0.32, 0.02], [0.0, 1.14, 0.13])),
            Extra::Headband => Some((0xdd3548, [0.33, 0.05, 0.06], [0.0, 1.6, 0.11])),
            Extra::Guitar => Some((0xbc783b, [0.25, 0.54, 0.12], [0.32, 0.95, -0.21])),
            Extra::Skateboard => Some((0x7ca759, [0.13, 0.62, 0.06], [0.33, 0.67, 0.09])),
            Extra::Camera => Some((0x252529, [0.16, 0.12, 0.09], [0.19, 0.94, 0.16])),
            Extra::Wrench | Extra::Spatula => Some((0xadb5bd, [0.062, 0.25, 0.04], [0.32, 0.75, 0.04])),
            Extra::Newspaper => Some((0xf5efdd, [0.12, 0.23, 0.04], [0.33, 0.74, 0.05])),
            Extra::Paint => Some((0xf5bc40, [0.10, 0.26, 0.04], [0.33, 0.75, 0.07])),
        };
        if let Some((color, size, pos)) = accessory {
            let mat = visuals.color(materials, color);
            part(commands, root, &visuals.cube, &mat, pos, size);
        }
        let arms = [-1.0, 1.0].map(|side| {
            let hinge = commands
                .spawn((
                    Transform::from_xyz(side * (0.19 * width + 0.07), 1.25, 0.0),
                    Visibility::Inherited,
                    ChildOf(root),
                ))
                .id();
            part(commands, hinge, &visuals.cube, &shirt, [0.0, -0.12, 0.0], [0.13, 0.25, 0.14]);
            part(commands, hinge, &visuals.cube, &skin, [0.0, -0.32, 0.0], [0.105, 0.18, 0.105]);
            hinge
        });
        let legs = [-1.0, 1.0].map(|side| {
            let hinge = commands
                .spawn((
                    Transform::from_xyz(side * 0.106 * width, 0.83, 0.0),
                    Visibility::Inherited,
                    ChildOf(root),
                ))
                .id();
            part(
                commands,
                hinge,
                &visuals.cube,
                &trousers,
                [0.0, -0.34, 0.0],
                [0.16 * width, 0.68, 0.17],
            );
            part(
                commands,
                hinge,
                &visuals.cube,
                &shoes,
                [0.0, -0.765, 0.035],
                [0.17 * width, 0.13, 0.25],
            );
            hinge
        });
        commands.entity(root).insert(Body { index, arms, legs });
        root
    }

    pub fn show_residents(
        city: Res<City>,
        world: Res<World>,
        camera: Res<CameraRig>,
        mut transforms: Query<&mut Transform>,
        mut bodies: Query<(Entity, &Body, &mut Visibility)>,
    ) {
        for (entity, body, mut visibility) in &mut bodies {
            let Some(n) = city.residents.get(body.index) else { continue };
            let ground = world.height_at(n.at.x, n.at.y) as f32;
            let at = Vec3::new(n.at.x as f32, ground, n.at.y as f32);
            let show = city.active && !n.hidden(city.clock) && at.distance_squared(camera.position) < 150.0_f32.powi(2);
            *visibility = if show { Visibility::Inherited } else { Visibility::Hidden };
            if !show {
                continue;
            }
            let lying = n.down_until > city.clock;
            let phase = (city.clock * n.speed * 3.0) as f32;
            let stride = if lying {
                0.0
            } else {
                phase.sin() * (n.speed as f32 * 0.28).min(1.15)
            };
            if let Ok(mut t) = transforms.get_mut(entity) {
                t.translation = at + Vec3::Y * if lying { 0.15 } else { stride.abs() * 0.035 };
                t.rotation = Quat::from_rotation_y(n.heading as f32)
                    * if lying {
                        Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
                    } else {
                        Quat::IDENTITY
                    };
                t.scale = Vec3::splat((n.member.look.height / 1.7) as f32);
            }
            for i in 0..2 {
                let sign = if i == 0 { 1.0 } else { -1.0 };
                if let Ok(mut arm) = transforms.get_mut(body.arms[i]) {
                    arm.rotation = Quat::from_rotation_x(stride * sign);
                }
                if let Ok(mut leg) = transforms.get_mut(body.legs[i]) {
                    leg.rotation = Quat::from_rotation_x(-stride * sign);
                }
            }
        }
    }

    pub fn spawn_place(commands: &mut Commands, visuals: &mut Visuals, materials: &mut Assets<StandardMaterial>, at: Vec3, kind: &str) {
        let color = match kind {
            "coffee" => 0xbb5838,
            "bank" => 0x43836d,
            "diner" => 0xb1344f,
            "gym" => 0x376e9e,
            "park" => 0x397444,
            "stunts" => 0xb78932,
            "garage" => 0x687581,
            "music" => 0x915ec3,
            _ => 0x3d8097,
        };
        let paint = visuals.color(materials, color);
        let wood = visuals.color(materials, 0xb99461);
        let pole = visuals.color(materials, 0x545456);
        let root = commands
            .spawn((CityRoot, Transform::from_translation(at), Visibility::Inherited))
            .id();
        if kind == "park" {
            part(commands, root, &visuals.cube, &wood, [0.0, 0.48, 0.0], [2.1, 0.12, 0.62]);
            part(commands, root, &visuals.cube, &wood, [0.0, 0.85, -0.27], [2.1, 0.38, 0.10]);
            for x in [-0.8, 0.8] {
                part(commands, root, &visuals.cube, &pole, [x, 0.23, 0.0], [0.13, 0.46, 0.53]);
            }
        } else {
            part(commands, root, &visuals.cube, &pole, [0.0, 0.91, 0.0], [0.13, 1.82, 0.13]);
            part(commands, root, &visuals.cube, &paint, [0.0, 2.03, 0.0], [1.35, 0.53, 0.14]);
            if ["coffee", "market", "news", "bakery"].contains(&kind) {
                part(commands, root, &visuals.cube, &wood, [0.0, 0.49, 0.0], [1.4, 0.98, 0.74]);
                part(commands, root, &visuals.cube, &paint, [0.0, 1.1, 0.0], [1.5, 0.17, 0.84]);
            }
        }
    }

    pub fn spawn_car(commands: &mut Commands, visuals: &mut Visuals, materials: &mut Assets<StandardMaterial>, driver: Option<usize>) {
        let paint = visuals.color(
            materials,
            driver.map_or(0xf4f3f3, |i| [0x8f334e, 0xc1692e, 0x378995, 0x6f7166][i % 4]),
        );
        let dark = visuals.color(materials, 0x1c2732);
        let lights = visuals.color(materials, 0xf8dfaf);
        let red = visuals.color(materials, 0xf3333d);
        let blue = visuals.color(materials, 0x368aff);
        let root = commands
            .spawn((CityRoot, CityCar(driver), Transform::default(), Visibility::Inherited))
            .id();
        part(commands, root, &visuals.cube, &paint, [0.0, 0.63, 0.0], [1.84, 0.70, 4.40]);
        part(commands, root, &visuals.cube, &dark, [0.0, 1.17, -0.24], [1.55, 0.55, 2.30]);
        for x in [-0.98, 0.98] {
            for z in [-1.4, 1.4] {
                let tire = part(commands, root, &visuals.cylinder, &dark, [x, 0.38, z], [0.69, 0.23, 0.69]);
                commands.entity(tire).insert(
                    Transform::from_xyz(x, 0.38, z)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::new(0.69, 0.23, 0.69)),
                );
            }
            part(commands, root, &visuals.cube, &lights, [x * 0.73, 0.68, 2.21], [0.3, 0.2, 0.05]);
            part(commands, root, &visuals.cube, &red, [x * 0.73, 0.68, -2.21], [0.3, 0.2, 0.05]);
        }
        if driver.is_none() {
            part(commands, root, &visuals.cube, &red, [-0.40, 1.54, -0.23], [0.50, 0.17, 0.25]);
            part(commands, root, &visuals.cube, &blue, [0.40, 1.54, -0.23], [0.50, 0.17, 0.25]);
        }
    }

    pub fn spawn_finder(commands: &mut Commands, visuals: &Visuals, materials: &mut Assets<StandardMaterial>) {
        let glow = materials.add(StandardMaterial {
            base_color: Color::srgba(0.46, 0.74, 1.0, 0.38),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.spawn((
            CityRoot,
            Finder,
            Mesh3d(visuals.cylinder.clone()),
            MeshMaterial3d(glow),
            Transform::from_scale(Vec3::new(1.25, 57.0, 1.25)),
            Visibility::Hidden,
        ));
    }
}
