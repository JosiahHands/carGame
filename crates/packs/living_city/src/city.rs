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
        }
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
                if n.asleep || n.in_car || n.at.distance_squared(at) > reach * reach || self.clock - n.last_reaction < 2.0 {
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
                if delta.length() < 0.7 {
                    t.roads.pop_front();
                } else {
                    t.heading = delta.x.atan2(delta.y);
                    t.at += delta.clamp_length_max(12.0 * dt);
                }
            } else {
                finished.push(j);
            }
        }
        for j in finished.into_iter().rev() {
            let t = self.trips.remove(j);
            for i in std::iter::once(t.driver).chain(t.rider) {
                let n = &mut self.residents[i];
                n.at = t.at;
                n.in_car = false;
                n.go_place(&self.map, t.destination, self.clock, "arriving after a drive");
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
            if n.in_car || Some(i) == chatting {
                continue;
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
            if !awake && n.task != "going home" {
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
