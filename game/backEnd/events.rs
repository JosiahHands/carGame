//! One event: a race against rivals, or a challenge against the clock. It
//! places the cars, counts down, runs the rivals, keeps track of everyone's
//! progress around the lap, and works out the result and stars at the end.
//!
//! The game runs the player's car as usual; while the event runs, the
//! player's physics is lent to it (`with_player`) so every car is handled the
//! same way.
use std::sync::Arc;

use bevy::math::DVec3;

use crate::audio::Sound;
use crate::carmodel::CarRig;
use crate::physics::{CarPhysics, Dynamics};
use crate::rivals::{RacerPosition, Rival, racing_line};
use crate::rng;
use crate::skills::SkillKind;
use crate::spec::{EventKind, EventSpec, RivalSpec};
use crate::text::{format_time, thousands};
use crate::track::RaceTrack;
use crate::world::{HitBox, World};

#[derive(Clone, Debug)]
pub struct Standing {
    pub name: String,
    pub text: String,
    pub me: bool,
}

#[derive(Clone, Debug)]
pub struct EventResult {
    pub stars: u32,
    pub position: usize,
    pub score_text: String,
    pub score: f64,
    pub passed: bool,
    pub standings: Vec<Standing>,
    pub best_lap: f64,
}

#[derive(Clone, Debug, Default)]
pub struct EventHud {
    pub stats: Vec<(String, String)>,
    /// (name, gap, me, out)
    pub standings: Vec<(String, String, bool, bool)>,
    pub warning: String,
    pub health: Option<f64>,
    pub stars: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventState {
    Countdown,
    Running,
    Done,
}

const COUNTDOWN: f64 = 3.5;
const MAX_JUMP: f64 = 40.0;
const OFF_COURSE: f64 = 6.0;
const RESPAWN_TIME: f64 = 5.0;
const MPH: f64 = 2.237;

pub struct Racer {
    pub name: String,
    pub physics: CarPhysics,
    pub rival: Option<Rival>,
    pub along: f64,
    pub offset: f64,
    pub distance: f64,
    pub finished: Option<f64>,
    pub out_order: usize,
    pub lost: f64,
    pub wrong_way: f64,
    pub hp: f64,
    pub has_hit_box: bool,
    gun_wait: f64,
    burst: u32,
    lap_start: f64,
    near_miss_at: f64,
}

impl Racer {
    fn position(&self) -> RacerPosition {
        RacerPosition {
            along: self.along,
            offset: self.offset,
            speed: self.physics.speed(),
        }
    }

    fn active(&self) -> bool {
        self.rival.as_ref().is_none_or(|r| r.active)
    }
}

/// Things that happened that the screen needs to show.
#[derive(Clone, Debug)]
pub enum EventVisual {
    Wreck(usize, bool),
    Hide(usize),
}

pub struct EventRun {
    pub spec: EventSpec,
    pub state: EventState,
    pub countdown: f64,
    pub clock: f64,
    pub score: f64,
    pub result: Option<EventResult>,
    pub notes: Vec<String>,
    pub skills: Vec<(SkillKind, f64, Option<String>)>,
    pub callouts: Vec<String>,
    pub visuals: Vec<EventVisual>,
    last_place: usize,
    final_lap_called: bool,
    pub player_wrecked: f64,
    pub unbanked_drift: f64,
    pub unbanked_air: f64,
    trap_mark: f64,
    /// The player's speeds through the speed cameras so far (mph), added up, and how many.
    trap_total: f64,
    trap_hits: usize,
    pub track: Option<Arc<RaceTrack>>,
    line: Vec<f64>,
    pub racers: Vec<Racer>,
    out_count: usize,
    next_out: f64,
    best_lap: f64,
    pub traps: Vec<f64>,
    takedowns: u32,
    first_finish: Option<f64>,
    dynamics: Dynamics,
}

pub fn ordinal(n: usize) -> String {
    let tens = n % 100;
    let suffix = if (11..=13).contains(&tens) {
        "th"
    } else {
        match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{n}{suffix}")
}

impl EventRun {
    /// Sets up an event. `rigs` are the rivals' car models, in the order of `spec.rivals`.
    pub fn new(spec: EventSpec, world: &mut World, player: &mut CarPhysics, rigs: Vec<Option<CarRig>>, dynamics: Dynamics) -> Self {
        let track = world.track.clone();
        let line = track.as_deref().map(racing_line).unwrap_or_default();
        let mut run = EventRun {
            next_out: spec.time_limit,
            spec,
            state: EventState::Countdown,
            countdown: COUNTDOWN,
            clock: 0.0,
            score: 0.0,
            result: None,
            notes: Vec::new(),
            skills: Vec::new(),
            callouts: Vec::new(),
            visuals: Vec::new(),
            last_place: 0,
            final_lap_called: false,
            player_wrecked: 0.0,
            unbanked_drift: 0.0,
            unbanked_air: 0.0,
            trap_mark: f64::NEG_INFINITY,
            trap_total: 0.0,
            trap_hits: 0,
            track,
            line,
            racers: Vec::new(),
            out_count: 0,
            best_lap: 0.0,
            traps: Vec::new(),
            takedowns: 0,
            first_finish: None,
            dynamics,
        };
        let me = run.make_racer("You", CarPhysics::new(), None);
        run.racers.push(me);
        let specs = run.spec.rivals.clone();
        for (spec, rig) in specs.iter().zip(rigs) {
            let (mut rival, physics) = Rival::new(spec, rig);
            rival.use_nitro = !run.spec.no_nitro;
            let r = run.make_racer(&spec.name, physics, Some(rival));
            run.racers.push(r);
        }
        run.with_player(player, |run| {
            run.place_all(world);
            if run.track.is_some() && matches!(run.spec.kind, EventKind::TimeAttack | EventKind::SpeedTrap) {
                run.set_targets_from_pace_car(world);
            }
        });
        run.sync_hit_boxes(world);
        run
    }

    /// Lends the player's physics to the event for `f`.
    pub fn with_player<R>(&mut self, player: &mut CarPhysics, f: impl FnOnce(&mut Self) -> R) -> R {
        std::mem::swap(&mut self.racers[0].physics, player);
        let out = f(self);
        std::mem::swap(&mut self.racers[0].physics, player);
        out
    }

    fn make_racer(&self, name: &str, physics: CarPhysics, rival: Option<Rival>) -> Racer {
        Racer {
            name: name.into(),
            has_hit_box: rival.is_some() && self.spec.weapons,
            physics,
            rival,
            along: 0.0,
            offset: 0.0,
            distance: 0.0,
            finished: None,
            out_order: 0,
            lost: 0.0,
            wrong_way: 0.0,
            hp: 100.0,
            gun_wait: 2.0 + rng::random() * 3.0,
            burst: 0,
            lap_start: 0.0,
            near_miss_at: f64::NEG_INFINITY,
        }
    }

    /// The weapons' targets: every rival that can be hit.
    fn sync_hit_boxes(&self, world: &mut World) {
        world.hit_boxes = self
            .racers
            .iter()
            .enumerate()
            .filter(|(_, r)| r.has_hit_box)
            .map(|(i, r)| {
                let p = &r.physics;
                let alive = r.rival.as_ref().is_some_and(|v| v.wrecked <= 0.0 && v.active) && self.state != EventState::Done;
                HitBox {
                    x: p.x,
                    y: p.y,
                    z: p.z,
                    half: p.length.max(p.width) * 0.4,
                    height: p.height,
                    alive,
                    owner: i,
                }
            })
            .collect();
    }

    pub fn is_race(&self) -> bool {
        matches!(self.spec.kind, EventKind::Race | EventKind::Elimination | EventKind::Duel)
    }

    fn uses_laps(&self) -> bool {
        (self.is_race() && self.spec.kind != EventKind::Elimination)
            || matches!(self.spec.kind, EventKind::TimeAttack | EventKind::SpeedTrap)
    }

    // ---------- the start ----------

    fn place_all(&mut self, world: &World) {
        let Some(track) = self.track.clone() else {
            let s = world.spawn;
            let d = self.dynamics.clone();
            self.racers[0].physics.reset(s.x, s.z, s.heading, true, &d);
            return;
        };
        let edge = track.style.width / 2.0;
        // Rivals first (the fastest at the front), then the player.
        let mut order: Vec<usize> = (1..self.racers.len()).collect();
        order.push(0);
        let count = order.len();
        for (i, &r) in order.iter().enumerate() {
            let along = -10.0 - (i / 2) as f64 * 9.0 - (i % 2) as f64 * 4.0;
            let offset = if count == 1 {
                0.0
            } else if i % 2 == 0 {
                edge * 0.4
            } else {
                -edge * 0.4
            };
            self.place_on_track(r, world, along, offset, true);
            let racer = &mut self.racers[r];
            racer.distance = along;
            racer.along = along.rem_euclid(track.length);
        }
        if self.spec.kind == EventKind::SpeedTrap {
            self.build_traps();
            self.trap_mark = self.racers[0].distance;
        }
    }

    fn place_on_track(&mut self, r: usize, world: &World, along: f64, offset: f64, fresh: bool) {
        let track = self.track.clone().expect("track");
        let p = track.pose(along, offset);
        let d = self.dynamics.clone();
        let racer = &mut self.racers[r];
        racer.physics.reset(p.x, p.z, p.heading, fresh, &d);
        racer.physics.y = world.height_at(p.x, p.z);
        racer.offset = offset;
    }

    /// How many of the five start lights are on.
    pub fn start_lights(&self) -> usize {
        if self.state != EventState::Countdown || self.track.is_none() {
            return 0;
        }
        (((COUNTDOWN - self.countdown) / COUNTDOWN * 6.0).floor() as usize).min(5)
    }

    /// Drives a steady pace car around the track, unseen, to set the targets.
    fn set_targets_from_pace_car(&mut self, world: &mut World) {
        let track = self.track.clone().expect("track");
        let Some((config, skill)) = self.spec.pace.clone() else { return };
        let (mut car, physics) = Rival::new(
            &RivalSpec {
                name: "Pace car".into(),
                config,
                skill,
                steady: true,
            },
            None,
        );
        car.use_nitro = !self.spec.no_nitro;
        let mut racer = self.make_racer("Pace car", physics, None);
        let p = track.pose(-10.0, 0.0);
        racer.physics.reset(p.x, p.z, p.heading, false, &self.dynamics);
        racer.physics.y = world.height_at(p.x, p.z);
        racer.distance = -10.0;
        racer.along = track.length - 10.0;
        let goal = self.spec.laps as f64 * track.length;
        let step = 1.0 / 120.0;
        let mut time = 0.0;
        let mut trap_speed = 0.0;
        let mut trap_hits = 0;
        let mut furthest = racer.distance;
        while racer.distance < goal && time < 900.0 {
            let me = racer.position();
            car.drive(&mut racer.physics, step, world, &track, &self.line, me, &[], true, &self.dynamics);
            Self::progress(
                &track,
                &mut racer,
                step,
                false,
                0.0,
                self.uses_laps(),
                self.spec.laps,
                &mut None,
                &mut Vec::new(),
                &mut 0.0,
            );
            if self.spec.kind == EventKind::SpeedTrap && racer.distance > furthest {
                let crossed = self.traps_crossed(furthest, racer.distance);
                trap_speed += crossed as f64 * (racer.physics.speed() * MPH).round();
                trap_hits += crossed;
                furthest = racer.distance;
            }
            time += step;
        }
        // The pace car may have knocked over a tree or lamp; put everything back.
        let (x, z) = (self.racers[0].physics.x, self.racers[0].physics.z);
        world.rebuild_all(x, z, 3.0);
        world.destroyed.clear();
        if self.spec.kind == EventKind::TimeAttack {
            let t = (time * 10.0).ceil() / 10.0;
            self.spec.targets = [(t * 1.14).ceil(), (t * 1.06).ceil(), t];
        } else {
            let s = (trap_speed / trap_hits.max(1) as f64).floor();
            self.spec.targets = [(s * 0.86).floor(), (s * 0.94).floor(), s];
        }
    }

    /// Speed cameras at the end of the three longest straights.
    fn build_traps(&mut self) {
        let track = self.track.clone().expect("track");
        let n = track.points.len();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut start: Option<usize> = None;
        let mut first = 0;
        while first < n && track.points[first].curve.abs() < 1.0 / 300.0 {
            first += 1;
        }
        for k in 0..=n {
            let i = (first + k) % n;
            let straight = k < n && track.points[i].curve.abs() < 1.0 / 300.0;
            if straight && start.is_none() {
                start = Some(k);
            }
            if !straight && let Some(s) = start {
                runs.push((first + s, k - s));
                start = None;
            }
        }
        runs.sort_by_key(|r| std::cmp::Reverse(r.1));
        for (start, length) in runs.into_iter().take(3) {
            let i = (start + (length as f64 * 0.8).floor() as usize) % n;
            self.traps.push(track.points[i].along);
        }
    }

    fn traps_crossed(&self, from: f64, to: f64) -> usize {
        let Some(track) = &self.track else { return 0 };
        if to <= from {
            return 0;
        }
        let l = track.length;
        self.traps
            .iter()
            .filter(|&&t| {
                let k = ((from - t) / l + 1e-9).ceil();
                let at = t + k * l;
                at <= to && at > from && at >= 0.0
            })
            .count()
    }

    // ---------- each step ----------

    /// One physics step for the rivals (after the player's own step), then
    /// car-to-car crashes and progress. Returns how hard the player was hit.
    pub fn step(&mut self, dt: f64, world: &mut World, sound: &Sound) -> f64 {
        let track = self.track.clone();
        if self.state == EventState::Done {
            if let Some(track) = &track {
                for i in 1..self.racers.len() {
                    let r = &mut self.racers[i];
                    let me = r.position();
                    if let Some(rival) = r.rival.as_mut() {
                        rival.drive(&mut r.physics, dt, world, track, &self.line, me, &[], true, &self.dynamics);
                    }
                }
            }
            return 0.0;
        }
        let racing = self.state == EventState::Running;
        if let Some(track) = &track {
            let positions: Vec<(usize, RacerPosition)> = self
                .racers
                .iter()
                .enumerate()
                .filter(|(_, r)| r.active())
                .map(|(i, r)| (i, r.position()))
                .collect();
            for i in 1..self.racers.len() {
                let pace = self.pace_for(i);
                let others: Vec<RacerPosition> = positions.iter().filter(|(j, _)| *j != i).map(|(_, p)| *p).collect();
                let r = &mut self.racers[i];
                let me = r.position();
                let Some(rival) = r.rival.as_mut() else { continue };
                rival.pace = pace;
                rival.drive(&mut r.physics, dt, world, track, &self.line, me, &others, racing, &self.dynamics);
                if rival.wrecked > 0.0 {
                    rival.wrecked -= dt;
                    if rival.wrecked <= 0.0 {
                        self.respawn_rival(i, world);
                    }
                }
            }
        }
        let impact = self.collide_cars(world, sound);
        self.update_drafts(dt);
        if !racing {
            return impact;
        }
        if impact == 0.0 {
            self.near_misses();
        }
        let uses_laps = self.uses_laps();
        let laps = self.spec.laps;
        for i in 0..self.racers.len() {
            if let Some(track) = &track {
                let clock = self.clock;
                let r = &mut self.racers[i];
                Self::progress(
                    track,
                    r,
                    dt,
                    i == 0,
                    clock,
                    uses_laps,
                    laps,
                    &mut self.first_finish,
                    &mut self.notes,
                    &mut self.best_lap,
                );
            }
            if i == 0 && self.spec.kind == EventKind::SpeedTrap && self.racers[0].distance > self.trap_mark {
                let crossed = self.traps_crossed(self.trap_mark, self.racers[0].distance);
                self.trap_mark = self.racers[0].distance;
                if crossed > 0 {
                    // The score is the average speed through the cameras.
                    let mph = (self.racers[0].physics.speed() * MPH).round();
                    self.trap_total += mph * crossed as f64;
                    self.trap_hits += crossed;
                    self.score = (self.trap_total / self.trap_hits as f64).round();
                    self.notes.push(format!("📸 {}", crate::text::speed(mph)));
                    sound.chime(false);
                }
            }
        }
        self.sync_hit_boxes(world);
        impact
    }

    fn update_drafts(&mut self, dt: f64) {
        let cars: Vec<usize> = (0..self.racers.len())
            .filter(|&i| self.racers[i].rival.as_ref().is_none_or(|r| r.active && r.wrecked <= 0.0))
            .collect();
        let states: Vec<(f64, f64, f64)> = cars
            .iter()
            .map(|&i| (self.racers[i].physics.x, self.racers[i].physics.z, self.racers[i].physics.heading))
            .collect();
        for (k, &a) in cars.iter().enumerate() {
            let p = &self.racers[a].physics;
            let mut target: f64 = 0.0;
            if p.speed() > 15.0 && !p.airborne {
                let (fx, fz) = (p.heading.sin(), p.heading.cos());
                for (j, &(qx, qz, qh)) in states.iter().enumerate() {
                    if j == k {
                        continue;
                    }
                    let (dx, dz) = (qx - p.x, qz - p.z);
                    let ahead = dx * fx + dz * fz;
                    let side = (dx * fz - dz * fx).abs();
                    let same_way = qh.sin() * fx + qh.cos() * fz;
                    if ahead > 3.0 && ahead < 32.0 && side < 2.4 && same_way > 0.9 {
                        target = target.max(1.0 - (ahead - 3.0) / 29.0);
                    }
                }
            }
            let p = &mut self.racers[a].physics;
            p.draft += (target - p.draft) * (dt * 4.0).min(1.0);
        }
    }

    fn near_misses(&mut self) {
        let clock = self.clock;
        let (mx, mz, my, mw, mvx, mvz, mspeed) = {
            let m = &self.racers[0].physics;
            (m.x, m.z, m.y, m.width, m.vx, m.vz, m.speed())
        };
        for r in self.racers.iter_mut().skip(1) {
            let Some(rival) = &r.rival else { continue };
            if !rival.active || rival.wrecked > 0.0 {
                continue;
            }
            let q = &r.physics;
            let gap = (q.x - mx).hypot(q.z - mz) - (mw + q.width) / 2.0;
            let closing = (mvx - q.vx).hypot(mvz - q.vz);
            if gap > 0.15 && gap < 1.4 && mspeed > 18.0 && closing > 4.0 && (q.y - my).abs() < 1.5 {
                if clock - r.near_miss_at > 2.5 {
                    self.skills.push((SkillKind::NearMiss, 100.0 + mspeed * 3.0, None));
                }
                r.near_miss_at = clock;
            }
        }
    }

    fn pace_for(&self, i: usize) -> f64 {
        if !self.is_race() || self.spec.catch_up <= 0.0 {
            return 1.0;
        }
        let gap = self.racers[i].distance - self.racers[0].distance;
        let amount = (gap / 300.0).clamp(-1.0, 1.0);
        1.0 - amount * if amount > 0.0 { 0.1 } else { 0.05 } * self.spec.catch_up
    }

    /// Moves a car's place around the lap on (only on or near the road).
    #[allow(clippy::too_many_arguments)]
    fn progress(
        track: &RaceTrack,
        r: &mut Racer,
        dt: f64,
        is_me: bool,
        clock: f64,
        uses_laps: bool,
        laps: u32,
        first_finish: &mut Option<f64>,
        notes: &mut Vec<String>,
        best_lap: &mut f64,
    ) {
        let p = &r.physics;
        let at = track.locate(p.x, p.z);
        r.offset = at.map_or(999.0, |a| a.1);
        let mut moved = f64::INFINITY;
        if let Some((along, _)) = at {
            moved = along - r.along;
            if moved > track.length / 2.0 {
                moved -= track.length;
            }
            if moved < -track.length / 2.0 {
                moved += track.length;
            }
        }
        let st = &track.style;
        let room = OFF_COURSE
            .max(st.outside_runoff + 2.0)
            .max(st.inside_runoff.unwrap_or(0.0) + 2.0)
            .max(st.apron + 2.0);
        let on_course = at.is_some_and(|a| a.1.abs() <= st.width / 2.0 + room);
        if let Some((along, _)) = at
            && on_course
            && moved.abs() < MAX_JUMP
        {
            r.along = along;
            let before = r.distance;
            r.distance += moved;
            r.lost = 0.0;
            if is_me && (before / track.length).floor() < (r.distance / track.length).floor() && r.distance > 1.0 {
                let lap_time = clock - r.lap_start;
                r.lap_start = clock;
                if r.distance > track.length {
                    if *best_lap == 0.0 || lap_time < *best_lap {
                        *best_lap = lap_time;
                    }
                    if uses_laps && r.distance < laps as f64 * track.length {
                        notes.push(format!("Lap {}", format_time(lap_time)));
                    }
                }
            }
        } else {
            r.lost += dt;
        }
        let point = track.point_at(r.along);
        let facing = r.physics.heading.sin() * point.dx + r.physics.heading.cos() * point.dz;
        r.wrong_way = if facing < -0.3 && r.physics.speed() > 3.0 {
            r.wrong_way + dt
        } else {
            0.0
        };
        if uses_laps && r.finished.is_none() && r.distance >= laps as f64 * track.length {
            r.finished = Some(clock);
            first_finish.get_or_insert(clock);
        }
    }

    /// Car-to-car crashes: each car is two circles; touching cars push apart and swap some speed.
    fn collide_cars(&mut self, world: &mut World, sound: &Sound) -> f64 {
        let mut player_impact: f64 = 0.0;
        let cars: Vec<usize> = (0..self.racers.len()).filter(|&i| self.racers[i].active()).collect();
        let circles = |p: &CarPhysics| {
            let (fx, fz) = (p.heading.sin(), p.heading.cos());
            let reach = (p.length / 2.0 - p.width / 2.0).max(0.0);
            [(p.x + fx * reach, p.z + fz * reach), (p.x - fx * reach, p.z - fz * reach)]
        };
        let mut hurts: Vec<(usize, f64, DVec3, f64)> = Vec::new();
        for ii in 0..cars.len() {
            for jj in ii + 1..cars.len() {
                let (i, j) = (cars[ii], cars[jj]);
                let (left, right) = self.racers.split_at_mut(j);
                let a = &mut left[i].physics;
                let b = &mut right[0].physics;
                if (a.y - b.y).abs() > a.height.min(b.height) {
                    continue;
                }
                if (a.x - b.x).abs() > a.length + b.length || (a.z - b.z).abs() > a.length + b.length {
                    continue;
                }
                let reach = (a.width + b.width) / 2.0;
                for ca in circles(a) {
                    for cb in circles(b) {
                        let (mut nx, mut nz) = (ca.0 - cb.0, ca.1 - cb.1);
                        let d = nx.hypot(nz);
                        if d >= reach {
                            continue;
                        }
                        if d < 1e-4 {
                            nx = 1.0;
                            nz = 0.0;
                        } else {
                            nx /= d;
                            nz /= d;
                        }
                        let push = (reach - d) / 2.0;
                        a.x += nx * push;
                        a.z += nz * push;
                        b.x -= nx * push;
                        b.z -= nz * push;
                        let closing = (a.vx - b.vx) * nx + (a.vz - b.vz) * nz;
                        if closing >= 0.0 {
                            continue;
                        }
                        let kick = (-closing * 1.3) / 2.0;
                        a.vx += nx * kick;
                        a.vz += nz * kick;
                        b.vx -= nx * kick;
                        b.vz -= nz * kick;
                        let impact = -closing;
                        if i == 0 || j == 0 {
                            player_impact = player_impact.max(impact);
                            let other = if i == 0 { j } else { i };
                            if self.spec.kind == EventKind::Takedown && impact > 4.0 && self.state == EventState::Running {
                                hurts.push((other, impact * 7.0, DVec3::new(-nx, 0.0, -nz), impact * 1.2));
                            }
                        }
                    }
                }
            }
        }
        for (other, damage, dir, mine) in hurts {
            self.hurt_rival(other, damage, dir, world, sound);
            self.hurt_player(mine, world, sound);
        }
        player_impact
    }

    // ---------- weapons and wrecks ----------

    /// The player's weapons hit rivals (from the world's hit boxes).
    pub fn take_hits(&mut self, world: &mut World, sound: &Sound) {
        for (owner, damage, dir) in std::mem::take(&mut world.hit_box_hits) {
            self.hurt_rival(owner, damage, dir, world, sound);
        }
    }

    fn hurt_rival(&mut self, i: usize, damage: f64, direction: DVec3, world: &mut World, sound: &Sound) {
        let running = self.state == EventState::Running;
        let me = (self.racers[0].physics.x, self.racers[0].physics.z);
        let armor = self.spec.rival_armor;
        let r = &mut self.racers[i];
        let Some(rival) = r.rival.as_mut() else { return };
        if rival.wrecked > 0.0 || !rival.active || !running {
            return;
        }
        r.hp -= damage * 100.0 / armor;
        if r.hp > 0.0 {
            return;
        }
        rival.wrecked = RESPAWN_TIME;
        self.visuals.push(EventVisual::Wreck(i, true));
        let p = &mut r.physics;
        p.push(direction.x * 6.0, 9.0, direction.z * 6.0);
        world.effects.explosion(DVec3::new(p.x, p.y + 1.0, p.z), 2.0);
        sound.explosion(1.8, (p.x - me.0).hypot(p.z - me.1));
        let name = r.name.clone();
        if self.spec.kind == EventKind::Takedown {
            self.takedowns += 1;
            self.score = self.takedowns as f64;
            self.notes.push(format!("💥 TAKEDOWN! {name}"));
        } else {
            self.notes.push(format!("💥 {name} wrecked"));
        }
        self.skills.push((SkillKind::Takedown, 400.0, Some(format!("Takedown: {name}"))));
        if rng::random() < 0.5 {
            self.callouts.push("Takedown!".into());
        }
        let me = &mut self.racers[0].physics;
        me.nitro = (me.nitro + 0.35).min(1.0);
    }

    fn hurt_player(&mut self, damage: f64, world: &mut World, sound: &Sound) {
        if self.player_wrecked > 0.0 || self.state != EventState::Running {
            return;
        }
        let me = &mut self.racers[0];
        me.hp -= damage * 100.0 / self.spec.armor;
        if me.hp > 0.0 {
            return;
        }
        me.hp = 0.0;
        self.player_wrecked = 3.0;
        let p = &mut me.physics;
        world.effects.explosion(DVec3::new(p.x, p.y + 1.0, p.z), 2.2);
        sound.explosion(2.0, 0.0);
        p.push(0.0, 8.0, 0.0);
        self.notes.push("💀 WRECKED! Back in 3 seconds".into());
    }

    fn respawn_rival(&mut self, i: usize, world: &World) {
        let Some(track) = self.track.clone() else { return };
        self.visuals.push(EventVisual::Wreck(i, false));
        let takedown = self.spec.kind == EventKind::Takedown;
        let (my_along, my_distance) = (self.racers[0].along, self.racers[0].distance);
        let r = &mut self.racers[i];
        r.hp = 100.0;
        let along = if takedown {
            my_along + if rng::random() < 0.5 { 150.0 } else { -110.0 }
        } else {
            r.along
        };
        let offset = rng::centered() * track.style.width * 0.5;
        if let Some(rival) = r.rival.as_mut() {
            rival.place(&mut r.physics, &track, world, along, offset, &self.dynamics);
        }
        if takedown {
            r.along = along.rem_euclid(track.length);
            r.distance = my_distance + (r.along - my_along);
        }
    }

    fn rival_guns(&mut self, dt: f64, world: &mut World, sound: &Sound) {
        let (mx, my, mz) = (self.racers[0].physics.x, self.racers[0].physics.y, self.racers[0].physics.z);
        let mut hits = 0;
        for r in self.racers.iter_mut().skip(1) {
            let Some(rival) = &r.rival else { continue };
            if rival.wrecked > 0.0 || !rival.active {
                continue;
            }
            r.gun_wait -= dt;
            if r.gun_wait > 0.0 {
                continue;
            }
            let p = &r.physics;
            let (dx, dz) = (mx - p.x, mz - p.z);
            let distance = dx.hypot(dz);
            let facing = (p.heading.sin() * dx + p.heading.cos() * dz) / if distance == 0.0 { 1.0 } else { distance };
            if distance > 90.0 || facing < 0.97 || self.player_wrecked > 0.0 {
                r.gun_wait = 0.3;
                continue;
            }
            if r.burst == 0 {
                r.burst = 8;
            }
            r.burst -= 1;
            r.gun_wait = if r.burst > 0 { 0.11 } else { 2.5 + rng::random() * 2.5 };
            let from = DVec3::new(
                p.x + p.heading.sin() * p.length * 0.5,
                p.y + p.height * 0.8,
                p.z + p.heading.cos() * p.length * 0.5,
            );
            let hit = rng::random() < rival.skill * (0.75 - distance / 300.0);
            let miss = if hit { 0.0 } else { 2.0 + rng::random() * 3.0 };
            let to = DVec3::new(mx + rng::centered() * miss, my + 0.7, mz + rng::centered() * miss);
            world.effects.muzzle_flash(from, (to - from).normalize_or_zero());
            world.effects.tracer(from, to);
            if distance < 60.0 {
                sound.gunshot();
            }
            if hit {
                world.effects.bullet_hit(to, false);
                hits += 1;
            }
        }
        for _ in 0..hits {
            let damage = self.spec.rival_damage;
            self.hurt_player(damage, world, sound);
        }
    }

    // ---------- each frame ----------

    /// The countdown, timers, and the end of the event. `dt` is real time.
    pub fn update(&mut self, dt: f64, world: &mut World, sound: &Sound) {
        if self.state == EventState::Countdown {
            let before = self.countdown.ceil();
            self.countdown -= dt;
            if self.countdown.ceil() < before && self.countdown > 0.0 && before <= 4.0 {
                sound.chime(false);
                self.callouts.push(format!("{}", self.countdown.ceil()));
            }
            if self.countdown <= 0.0 {
                self.state = EventState::Running;
                sound.chime(true);
                self.notes.push("GO!".into());
                self.callouts.push("Go!".into());
            }
            return;
        }
        if self.state != EventState::Running {
            return;
        }
        self.clock += dt;
        self.take_hits(world, sound);
        if self.spec.weapons && self.track.is_some() {
            self.rival_guns(dt, world, sound);
        }
        if self.player_wrecked > 0.0 {
            self.player_wrecked -= dt;
            if self.player_wrecked <= 0.0 {
                self.racers[0].hp = 100.0;
                self.recover(world);
            }
        }
        if self.spec.kind == EventKind::Elimination {
            self.eliminate(sound, world);
        }
        self.watch_places();
        self.check_finished(world);
    }

    fn watch_places(&mut self) {
        if self.is_race() {
            let place = self.order().iter().position(|&i| i == 0).unwrap_or(0) + 1;
            if self.last_place > 0 && place < self.last_place && self.racers[0].out_order == 0 {
                self.skills.push((
                    SkillKind::Overtake,
                    250.0 * (self.last_place - place) as f64,
                    Some(format!("Overtake: P{place}")),
                ));
                if place == 1 {
                    self.callouts.push("You take the lead!".into());
                }
            }
            self.last_place = place;
        }
        if let Some(track) = &self.track
            && self.uses_laps()
            && self.spec.laps > 1
            && !self.final_lap_called
            && self.racers[0].distance > (self.spec.laps - 1) as f64 * track.length
        {
            self.final_lap_called = true;
            self.callouts.push("Final lap!".into());
            self.notes.push("🏁 FINAL LAP".into());
        }
    }

    fn eliminate(&mut self, sound: &Sound, world: &mut World) {
        if self.clock < self.next_out {
            return;
        }
        self.next_out += self.spec.time_limit;
        let order: Vec<usize> = self.order().into_iter().filter(|&i| self.racers[i].out_order == 0).collect();
        let Some(&last) = order.last() else { return };
        self.out_count += 1;
        self.racers[last].out_order = self.out_count;
        if last == 0 {
            return;
        }
        let name = self.racers[last].name.clone();
        self.notes.push(format!("❌ {name} is out!"));
        self.callouts.push(format!("{name} is out!"));
        sound.crash(4.0);
        if let Some(rival) = self.racers[last].rival.as_mut() {
            rival.active = false;
        }
        self.visuals.push(EventVisual::Hide(last));
        self.sync_hit_boxes(world);
    }

    fn check_finished(&mut self, world: &mut World) {
        let me = &self.racers[0];
        if self.is_race() {
            let out = me.out_order > 0;
            let won = self.spec.kind == EventKind::Elimination && self.racers.iter().skip(1).all(|r| r.out_order > 0);
            let too_slow = self.first_finish.is_some_and(|f| me.finished.is_none() && self.clock - f > 60.0);
            if me.finished.is_some() || out || won || too_slow {
                self.finish(world);
            }
            return;
        }
        if self.uses_laps() {
            if me.finished.is_some() || self.clock > self.spec.laps as f64 * 300.0 {
                self.finish(world);
            }
            return;
        }
        if self.clock >= self.spec.time_limit {
            self.finish(world);
        }
    }

    /// The running order (racer indices): finished first, then by distance; knocked-out cars last.
    pub fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.racers.len()).collect();
        order.sort_by(|&a, &b| {
            let (ra, rb) = (&self.racers[a], &self.racers[b]);
            if ra.out_order > 0 || rb.out_order > 0 {
                let key = |o: usize| if o == 0 { f64::INFINITY } else { o as f64 };
                return key(rb.out_order).total_cmp(&key(ra.out_order));
            }
            if ra.finished.is_some() || rb.finished.is_some() {
                return ra
                    .finished
                    .unwrap_or(f64::INFINITY)
                    .total_cmp(&rb.finished.unwrap_or(f64::INFINITY));
            }
            rb.distance.total_cmp(&ra.distance)
        });
        order
    }

    pub fn lower_is_better(&self) -> bool {
        self.is_race() || self.spec.kind == EventKind::TimeAttack
    }

    pub fn stars_for(&self, score: f64) -> u32 {
        let t = self.spec.targets;
        let mut stars = 0;
        for (k, target) in t.iter().enumerate() {
            let ok = if self.lower_is_better() {
                score <= *target && score > 0.0
            } else {
                score >= *target
            };
            if ok {
                stars = k as u32 + 1;
            }
        }
        stars
    }

    fn finish(&mut self, world: &mut World) {
        self.state = EventState::Done;
        world.hit_boxes.clear();
        let kind = self.spec.kind;
        let mut score = self.score;
        let mut position = 0;
        if kind == EventKind::Drift {
            score += self.unbanked_drift;
        }
        if kind == EventKind::Stunt {
            score += self.unbanked_air;
        }
        let order = self.order();
        let standings = order
            .iter()
            .map(|&i| {
                let r = &self.racers[i];
                let text = match r.finished {
                    Some(t) => format_time(t),
                    None if r.out_order > 0 => "Out".into(),
                    None if kind == EventKind::Elimination => "Survivor".into(),
                    None if r.rival.is_some() => "Running".into(),
                    None => "Did not finish".into(),
                };
                Standing {
                    name: r.name.clone(),
                    text,
                    me: i == 0,
                }
            })
            .collect();
        let me = &self.racers[0];
        let mut finished = true;
        let score_text;
        if self.is_race() {
            position = order.iter().position(|&i| i == 0).unwrap_or(0) + 1;
            if me.finished.is_none() && me.out_order == 0 && kind != EventKind::Elimination {
                position = self.racers.len();
                finished = false;
            }
            score = position as f64;
            score_text = if finished { ordinal(position) } else { "Did not finish".into() };
        } else if kind == EventKind::TimeAttack {
            score = me.finished.unwrap_or(0.0);
            score_text = me.finished.map_or("Did not finish".into(), format_time);
        } else {
            score_text = self.format_score(score);
        }
        let stars = if finished { self.stars_for(score) } else { 0 };
        self.result = Some(EventResult {
            stars,
            position,
            score,
            score_text,
            passed: stars > 0,
            standings,
            best_lap: self.best_lap,
        });
        self.callouts.push(
            if position == 1 {
                "Winner!"
            } else if stars == 3 {
                "Perfect!"
            } else if stars > 0 {
                "Finished!"
            } else {
                "Better luck next time."
            }
            .into(),
        );
    }

    pub fn format_score(&self, value: f64) -> String {
        match self.spec.kind {
            EventKind::Race | EventKind::Elimination | EventKind::Duel => ordinal(value as usize),
            EventKind::TimeAttack => format_time(value),
            EventKind::Takedown => format!("{value} takedown{}", if value == 1.0 { "" } else { "s" }),
            EventKind::Drift => format!("{} pts", thousands(value)),
            EventKind::SpeedTrap => format!("{} avg", crate::text::speed(value.round())),
            EventKind::Destruction => format!("${}", thousands(value)),
            EventKind::Stunt => format!("{value:.1} s air"),
        }
    }

    pub fn add_drift(&mut self, points: f64) {
        if self.spec.kind == EventKind::Drift && self.state == EventState::Running {
            self.score += points;
        }
    }

    pub fn add_air(&mut self, seconds: f64) {
        if self.spec.kind == EventKind::Stunt && self.state == EventState::Running {
            self.score += seconds;
        }
    }

    pub fn add_damage(&mut self, dollars: f64) {
        if self.spec.kind == EventKind::Destruction && self.state == EventState::Running {
            self.score += dollars;
        }
    }

    /// R: puts the player's car back on the track where it left it, or at the start in the city.
    pub fn recover(&mut self, world: &World) {
        if self.track.is_none() {
            let s = world.spawn;
            let d = self.dynamics.clone();
            let p = &mut self.racers[0].physics;
            p.reset(s.x, s.z, s.heading, false, &d);
            p.y = world.height_at(s.x, s.z);
            return;
        }
        let nitro = self.racers[0].physics.nitro;
        let along = self.racers[0].along;
        self.place_on_track(0, world, along, 0.0, false);
        let me = &mut self.racers[0];
        me.physics.nitro = nitro;
        me.lost = 0.0;
        me.wrong_way = 0.0;
    }

    pub fn hud(&self, bindings: &crate::input::Bindings) -> EventHud {
        let s = &self.spec;
        let me = &self.racers[0];
        let mut stats = Vec::new();
        let lap = self
            .track
            .as_ref()
            .map_or(0, |t| s.laps.min(((me.distance / t.length).floor() as i64 + 1).max(1) as u32));
        let order = self.order();
        if self.is_race() {
            let pos = order.iter().position(|&i| i == 0).unwrap_or(0) + 1;
            let count = order.iter().filter(|&&i| self.racers[i].out_order == 0 || i == 0).count();
            stats.push(("POS".into(), format!("{pos}/{count}")));
            if s.kind == EventKind::Elimination {
                stats.push(("NEXT OUT".into(), format!("{}s", (self.next_out - self.clock).ceil().max(0.0))));
            } else {
                stats.push(("LAP".into(), format!("{lap}/{}", s.laps)));
            }
            stats.push(("TIME".into(), format_time(self.clock)));
        } else if self.uses_laps() {
            stats.push(("LAP".into(), format!("{lap}/{}", s.laps)));
            stats.push(("TIME".into(), format_time(self.clock)));
            if s.kind == EventKind::SpeedTrap {
                let average = if self.trap_hits == 0 {
                    "–".into()
                } else {
                    crate::text::speed(self.score)
                };
                stats.push(("AVG SPEED".into(), average));
            } else {
                stats.push(("★★★".into(), format_time(s.targets[2])));
            }
        } else {
            stats.push(("TIME LEFT".into(), format_time((s.time_limit - self.clock).max(0.0))));
            stats.push(("SCORE".into(), self.format_score(self.score)));
            match s.targets.iter().find(|t| self.score < **t) {
                Some(next) => stats.push(("NEXT ★".into(), self.format_score(*next))),
                None => stats.push(("ALL STARS!".into(), "★★★".into())),
            }
        }
        let mut standings = Vec::new();
        if self.is_race() {
            let leader = &self.racers[order[0]];
            for (k, &i) in order.iter().enumerate() {
                let r = &self.racers[i];
                let gap = if k == 0 {
                    "Leader".to_string()
                } else if r.out_order > 0 {
                    "OUT".into()
                } else if r.finished.is_some() {
                    "Finished".into()
                } else {
                    format!("{} m", (r.distance - leader.distance).round())
                };
                standings.push((r.name.clone(), gap, i == 0, r.out_order > 0));
            }
        }
        let mut warning = String::new();
        if self.state == EventState::Running && self.track.is_some() {
            if self.player_wrecked > 0.0 {
                warning = "WRECKED".into();
            } else if me.lost > 1.0 {
                warning = format!("RETURN TO THE TRACK · {} to reset", bindings.label("reset"));
            } else if me.wrong_way > 1.0 {
                warning = "WRONG WAY".into();
            }
        }
        if self.state == EventState::Countdown {
            warning = if self.countdown > 3.0 {
                match &s.boss {
                    Some(b) => format!("👑 {b}"),
                    None => s.title.to_uppercase(),
                }
            } else {
                format!("{}", self.countdown.ceil())
            };
        }
        let stars = if self.is_race() {
            self.stars_for((order.iter().position(|&i| i == 0).unwrap_or(0) + 1) as f64)
        } else if s.kind == EventKind::TimeAttack {
            let left = self
                .track
                .as_ref()
                .map_or(0.0, |t| (s.laps as f64 * t.length - me.distance).max(0.0) / 60.0);
            self.stars_for(self.clock.max(0.01) + left)
        } else {
            self.stars_for(self.score)
        };
        EventHud {
            stats,
            standings,
            warning,
            health: if s.weapons && self.track.is_some() {
                Some(me.hp.max(0.0) / 100.0)
            } else {
                None
            },
            stars,
        }
    }

    /// Dots for the rivals and cameras on the minimap: (x, z, color, size).
    pub fn map_markers(&self) -> Vec<(f64, f64, u32, f32)> {
        let mut out: Vec<(f64, f64, u32, f32)> = self
            .racers
            .iter()
            .filter(|r| r.rival.as_ref().is_some_and(|v| v.active))
            .map(|r| {
                let wrecked = r.rival.as_ref().is_some_and(|v| v.wrecked > 0.0);
                (
                    r.physics.x,
                    r.physics.z,
                    if wrecked {
                        0x555555
                    } else if self.spec.boss.is_some() {
                        0xffd23f
                    } else {
                        0xff4d4d
                    },
                    4.0,
                )
            })
            .collect();
        if let Some(track) = &self.track {
            for t in &self.traps {
                let p = track.pose(*t, 0.0);
                out.push((p.x, p.z, 0xffb300, 5.0));
            }
        }
        out
    }

    /// Whether this rival is a known nemesis (for the app).
    pub fn rival_names(&self) -> Vec<String> {
        self.racers
            .iter()
            .skip(1)
            .filter(|r| r.rival.is_some())
            .map(|r| r.name.clone())
            .collect()
    }
}

pub mod rivals {
    //! Computer-driven rival cars. Each one uses the same physics as the
    //! player's car and drives it with the same controls, worked out every step:
    //! aim at a point on the racing line ahead, brake in time for the corners
    //! coming up, pass slower cars, and (the less skilled) now and then run into
    //! a corner too fast. A rival that gets stuck or lost is put back on track.
    use std::collections::HashMap;

    use crate::carmodel::CarRig;
    use crate::physics::{CarInput, CarPhysics, Dynamics};
    use crate::rng;
    use crate::spec::RivalSpec;
    use crate::track::RaceTrack;
    use crate::world::World;

    /// Rivals who were renamed, old name to new, so saved records follow them.
    pub const RENAMED_RIVALS: &[(&str, &str)] = &[("Kiki Kerb", "Kiki Curb")];

    pub const RIVAL_NAMES: &[&str] = &[
        "Max Power",
        "Lola Lightning",
        "Rico Revs",
        "Dash Delgado",
        "Penny Piston",
        "Turbo Tim",
        "Gina Gearbox",
        "Axel Grease",
        "Skid Marx",
        "Nitro Nina",
        "Bruno Burnout",
        "Holly Hairpin",
        "Vince Vroom",
        "Zoe Zoom",
        "Duke Drift",
        "Cleo Clutch",
        "Rusty Bolts",
        "Speedy Gonzo",
        "Ivy Apex",
        "Chuck Chicane",
        "Mona Mph",
        "Felix Flatout",
        "Kiki Curb",
        "Otto Overtake",
    ];

    /// Where a car is in the race, for the rivals to see each other.
    #[derive(Clone, Copy, Debug)]
    pub struct RacerPosition {
        pub along: f64,
        pub offset: f64,
        pub speed: f64,
    }

    /// The racing line: towards the inside of each corner, eased in and out.
    pub fn racing_line(track: &RaceTrack) -> Vec<f64> {
        let n = track.points.len();
        let room = (track.style.width / 2.0 - 2.0).max(0.0);
        let raw: Vec<f64> = track.points.iter().map(|p| (p.curve * 60.0).clamp(-1.0, 1.0) * room).collect();
        (0..n)
            .map(|i| {
                let mut sum = 0.0;
                for k in -10i64..=10 {
                    sum += raw[((i as i64 + k).rem_euclid(n as i64)) as usize];
                }
                sum / 21.0
            })
            .collect()
    }

    /// Racing lines, worked out once per track.
    #[derive(Default)]
    pub struct Lines(pub HashMap<usize, Vec<f64>>);

    pub struct Rival {
        pub rig: Option<CarRig>,
        pub name: String,
        pub skill: f64,
        lane: f64,
        dodge: f64,
        mistake: f64,
        next_mistake: f64,
        stuck: f64,
        /// Seconds left before a wrecked rival is put back on the track.
        pub wrecked: f64,
        pub hp: f64,
        /// Catch-up: above 1 when far behind the player, below when far ahead.
        pub pace: f64,
        steady: bool,
        pub active: bool,
        pub use_nitro: bool,
    }

    impl Rival {
        /// A rival and its car's physics.
        pub fn new(spec: &RivalSpec, rig: Option<CarRig>) -> (Self, CarPhysics) {
            let mut physics = CarPhysics::new();
            if let Some(r) = &rig {
                physics.configure(&spec.config, r.dims());
            } else {
                physics.configure(
                    &spec.config,
                    crate::physics::Dims {
                        length: 4.5,
                        width: 1.9,
                        height: 1.3,
                        wheel_base: 2.7,
                    },
                );
            }
            let rival = Rival {
                rig,
                name: spec.name.clone(),
                skill: spec.skill,
                lane: rng::centered() * 2.0,
                dodge: 0.0,
                mistake: 0.0,
                next_mistake: 6.0 + rng::random() * 10.0,
                stuck: 0.0,
                wrecked: 0.0,
                hp: 100.0,
                pace: 1.0,
                steady: spec.steady,
                active: true,
                use_nitro: true,
            };
            (rival, physics)
        }

        /// Puts the car on the track, standing still.
        pub fn place(&mut self, physics: &mut CarPhysics, track: &RaceTrack, world: &World, along: f64, offset: f64, dynamics: &Dynamics) {
            let p = track.pose(along, offset);
            physics.reset(p.x, p.z, p.heading, false, dynamics);
            physics.y = world.height_at(p.x, p.z);
            self.stuck = 0.0;
        }

        /// Works out this step's controls and runs the physics.
        #[allow(clippy::too_many_arguments)]
        pub fn drive(
            &mut self,
            physics: &mut CarPhysics,
            dt: f64,
            world: &mut World,
            track: &RaceTrack,
            line: &[f64],
            me: RacerPosition,
            others: &[RacerPosition],
            racing: bool,
            dynamics: &Dynamics,
        ) {
            let mut input = CarInput::default();
            if self.wrecked > 0.0 || !self.active {
                input.brake = 1.0;
                physics.step(dt, &input, world, dynamics);
                return;
            }
            if !racing {
                physics.step(dt, &input, world, dynamics);
                return;
            }
            let p = &*physics;
            let speed = p.forward_speed.max(0.0);
            let half = track.style.width / 2.0;
            self.next_mistake -= dt;
            if self.next_mistake <= 0.0 && !self.steady {
                self.next_mistake = 5.0 + rng::random() * 12.0;
                if rng::random() < (1.02 - self.skill) * 2.5 {
                    self.mistake = 1.2 + rng::random();
                }
            }
            self.mistake = (self.mistake - dt).max(0.0);

            let mut blocker: Option<RacerPosition> = None;
            let mut gap = f64::INFINITY;
            for o in others {
                let mut ahead = o.along - me.along;
                if ahead < -track.length / 2.0 {
                    ahead += track.length;
                }
                if ahead > track.length / 2.0 {
                    ahead -= track.length;
                }
                if ahead > 0.0 && ahead < 10.0 + speed * 0.6 && (o.offset - me.offset).abs() < 3.0 && ahead < gap {
                    gap = ahead;
                    blocker = Some(*o);
                }
            }
            let look = 7.0 + speed * 0.42;
            let target_along = me.along + look;
            let li = ((target_along.rem_euclid(track.length) / track.length * line.len() as f64) as usize) % line.len();
            let line_here = line[li];
            match blocker {
                Some(b) if b.speed < speed + 3.0 => {
                    let left = half - b.offset;
                    let right = b.offset + half;
                    let side = if left > right { 1.0 } else { -1.0 };
                    self.dodge += (b.offset + side * 3.6 - (line_here + self.lane) - self.dodge) * (dt * 3.0).min(1.0);
                }
                _ => self.dodge -= self.dodge * (dt * 0.8).min(1.0),
            }
            let offset = (line_here + self.lane + self.dodge).clamp(-half + 1.6, half - 1.6);
            let target = track.pose(target_along, offset);
            let (dx, dz) = (target.x - p.x, target.z - p.z);
            let raw = dx.atan2(dz) - p.heading;
            let angle = raw.sin().atan2(raw.cos());
            let distance = dx.hypot(dz).max(1e-9);
            input.steer = p.steer_for(2.0 * angle.sin() / distance);

            let limit = self.skill * self.pace * if self.mistake > 0.0 { 1.18 } else { 1.0 };
            let brake = p.brake_decel * 0.8;
            let mut wanted = p.top_speed * if self.pace < 1.0 { self.pace } else { 1.2 };
            let scan = (30.0 + speed * speed / (2.0 * brake)).min(400.0);
            let mut s = 0.0;
            while s < scan {
                let corner = p.corner_speed(track.point_at(me.along + s).curve * 0.85, p.surface_grip, dynamics) * limit;
                wanted = wanted.min((corner * corner + 2.0 * brake * s).sqrt());
                s += 4.0;
            }
            if let Some(b) = blocker
                && gap < 8.0
                && self.dodge.abs() < 1.5
            {
                wanted = wanted.min(b.speed);
            }
            if speed < wanted - 0.5 {
                input.throttle = 1.0;
            } else if speed < wanted + 0.5 {
                input.throttle = 0.3;
            }
            if speed > wanted + 1.5 {
                input.brake = ((speed - wanted) / 6.0 + 0.3).min(1.0);
            }
            input.boost = self.use_nitro && p.nitro > 0.25 && speed > p.top_speed * 0.6 && wanted > p.top_speed * 1.15 && self.skill > 0.8;
            physics.step(dt, &input, world, dynamics);
            if !input.boost {
                physics.nitro = (physics.nitro + dt * 0.04).min(1.0);
            }
            let lost = me.offset.abs() > half + 14.0;
            self.stuck = if physics.speed() < 2.0 || lost { self.stuck + dt } else { 0.0 };
            if self.stuck > if lost { 1.0 } else { 2.5 } {
                self.place(physics, track, world, me.along, line_here, dynamics);
            }
        }
    }
}

pub mod spec {
    //! What an event is: its kind, venue, rivals, targets and rules. Career
    //! levels, the daily challenge and quick races all make these (career.rs);
    //! event.rs runs them.
    use crate::config::CarConfig;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum EventKind {
        Race,
        Elimination,
        Duel,
        TimeAttack,
        Takedown,
        Drift,
        SpeedTrap,
        Destruction,
        Stunt,
    }

    impl EventKind {
        pub const ALL: [EventKind; 9] = [
            EventKind::Race,
            EventKind::Elimination,
            EventKind::Duel,
            EventKind::TimeAttack,
            EventKind::Takedown,
            EventKind::Drift,
            EventKind::SpeedTrap,
            EventKind::Destruction,
            EventKind::Stunt,
        ];

        pub fn label(self) -> &'static str {
            match self {
                EventKind::Race => "Race",
                EventKind::Elimination => "Elimination",
                EventKind::Duel => "Boss duel",
                EventKind::TimeAttack => "Time attack",
                EventKind::Takedown => "Takedown",
                EventKind::Drift => "Drift",
                EventKind::SpeedTrap => "Speed trap",
                EventKind::Destruction => "Destruction",
                EventKind::Stunt => "Stunt show",
            }
        }

        pub fn rules(self) -> &'static str {
            match self {
                EventKind::Race => "Finish in the top 3. 1st place for 3 stars.",
                EventKind::Elimination => "The car in last place is knocked out every few seconds. Be the last one left.",
                EventKind::Duel => "One on one against the boss. Beat them to move on.",
                EventKind::TimeAttack => "Just you and the clock. Beat the target times.",
                EventKind::Takedown => "Weapons on. Wreck rivals with guns, rockets and rams. They shoot back!",
                EventKind::Drift => "Score drift points before the time runs out. Crashing loses the drift.",
                EventKind::SpeedTrap => "Hit the speed cameras as fast as you can. You're scored on your average speed through them.",
                EventKind::Destruction => "Weapons on. Do as much damage to the city as you can.",
                EventKind::Stunt => "Use the ramps. Every second in the air counts.",
            }
        }

        pub fn icon(self) -> &'static str {
            match self {
                EventKind::Race => "🏁",
                EventKind::Elimination => "⏳",
                EventKind::Duel => "👑",
                EventKind::TimeAttack => "⏱",
                EventKind::Takedown => "💥",
                EventKind::Drift => "🌀",
                EventKind::SpeedTrap => "📸",
                EventKind::Destruction => "💥",
                EventKind::Stunt => "🚀",
            }
        }

        /// The name the 1.2.x game used (for old quick race settings).
        pub fn id(self) -> &'static str {
            match self {
                EventKind::Race => "race",
                EventKind::Elimination => "elimination",
                EventKind::Duel => "duel",
                EventKind::TimeAttack => "timeAttack",
                EventKind::Takedown => "takedown",
                EventKind::Drift => "drift",
                EventKind::SpeedTrap => "speedTrap",
                EventKind::Destruction => "destruction",
                EventKind::Stunt => "stunt",
            }
        }

        pub fn from_id(id: &str) -> Option<EventKind> {
            EventKind::ALL.iter().copied().find(|k| k.id() == id)
        }
    }

    /// Who a rival is, and what they drive.
    #[derive(Clone, Debug)]
    pub struct RivalSpec {
        pub name: String,
        pub config: CarConfig,
        /// How close to the limit they drive: 0.75 (gentle) to 1.05 (flat out, no mistakes).
        pub skill: f64,
        /// Never makes mistakes: the pace car that sets target times.
        pub steady: bool,
    }

    #[derive(Clone, Debug)]
    pub struct EventSpec {
        pub kind: EventKind,
        pub title: String,
        pub map: String,
        pub layout: u32,
        pub laps: u32,
        /// Seconds: the time limit, or for an elimination, how often someone is knocked out.
        pub time_limit: f64,
        pub rivals: Vec<RivalSpec>,
        /// The car and skill that set the target times and speeds for time attacks and speed traps.
        pub pace: Option<(CarConfig, f64)>,
        /// What 1, 2 and 3 stars need.
        pub targets: [f64; 3],
        pub night: bool,
        pub weapons: bool,
        pub no_nitro: bool,
        pub catch_up: f64,
        pub armor: f64,
        pub rival_armor: f64,
        pub rival_damage: f64,
        pub boss: Option<String>,
    }
}

pub mod timetrial {
    //! Time trial (T): a lap through glowing arches, against the clock. Each map
    //! has its own lap and best lap; the best lap is recorded, so next time a
    //! see-through "ghost" car races you around it.
    use crate::storage::Storage;
    use crate::world::{Course, Spot};

    const COUNTDOWN: f64 = 3.0;
    const GHOST_RATE: f64 = 20.0;
    const GHOST_VALUES: usize = 6;

    /// Where the car is and which way it points: what the ghost records.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CarPose {
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub heading: f64,
        pub pitch: f64,
        pub ground_roll: f64,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TrialEvent {
        Go,
        Checkpoint,
        Finish,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ArchLook {
        Hidden,
        Next,
        Finish,
        Later,
    }

    // The city kept the key it had before there were other maps.
    fn best_key(map: &str) -> String {
        if map == "city" {
            "carGame.bestLap".into()
        } else {
            format!("carGame.bestLap.{map}")
        }
    }

    fn ghost_key(map: &str) -> String {
        format!("carGame.ghost.{map}")
    }

    pub fn best_lap_for(storage: &Storage, map: &str) -> f64 {
        storage.get(&best_key(map)).and_then(|t| t.parse().ok()).unwrap_or(0.0)
    }

    #[derive(Default)]
    pub struct TimeTrial {
        pub active: bool,
        pub countdown: f64,
        pub time: f64,
        pub best: f64,
        pub new_best: bool,
        pub next: usize,
        pub total: usize,
        pub start: Spot,
        pub course: Option<Course>,
        map: String,
        last: Option<(f64, f64)>,
        recording: Vec<f64>,
        ghost: Option<Vec<f64>>,
        record_timer: f64,
        last_pose: Option<CarPose>,
        /// How each arch should look now.
        pub arches: Vec<ArchLook>,
        /// Where the arrow over the car is and which way it points.
        pub arrow: Option<(CarPose, f64)>,
        /// Counts up when the course changes (so the arches are built again).
        pub generation: u64,
    }

    impl TimeTrial {
        pub fn set_course(&mut self, course: &Course, map: &str, storage: &Storage) {
            self.stop();
            self.total = course.checkpoints.len();
            self.start = course.start;
            self.course = Some(course.clone());
            self.map = map.into();
            self.best = best_lap_for(storage, map);
            self.ghost = storage.get_as::<Vec<f64>>(&ghost_key(map));
            self.arches = vec![ArchLook::Hidden; self.total];
            self.generation += 1;
        }

        /// Each arch's position and turn (heading, bank), for drawing.
        pub fn arch_poses(&self) -> Vec<(f64, f64, f64, f64, f64)> {
            let Some(c) = &self.course else { return Vec::new() };
            let p = &c.checkpoints;
            let n = p.len();
            (0..n)
                .map(|i| {
                    let (before, after) = (p[(i + n - 1) % n], p[(i + 1) % n]);
                    let here = p[i];
                    (
                        here.x,
                        here.y,
                        here.z,
                        here.heading.unwrap_or_else(|| (after.x - before.x).atan2(after.z - before.z)),
                        here.bank.unwrap_or(0.0),
                    )
                })
                .collect()
        }

        pub fn begin(&mut self) {
            self.active = true;
            self.countdown = COUNTDOWN;
            self.time = 0.0;
            self.next = 0;
            self.last = None;
            self.recording.clear();
            self.record_timer = 0.0;
            self.last_pose = None;
            self.show_arches();
        }

        pub fn stop(&mut self) {
            self.active = false;
            self.countdown = 0.0;
            for a in &mut self.arches {
                *a = ArchLook::Hidden;
            }
            self.arrow = None;
        }

        pub fn start_lights(&self) -> usize {
            if !self.active || self.countdown <= 0.0 {
                return 0;
            }
            (((COUNTDOWN - self.countdown) / (COUNTDOWN / 5.0)).floor() as usize + 1).min(5)
        }

        pub fn has_ghost(&self) -> bool {
            self.ghost.as_ref().is_some_and(|g| g.len() >= GHOST_VALUES * 2)
        }

        pub fn ghost_pose(&self) -> Option<CarPose> {
            let g = self.ghost.as_ref()?;
            if !self.active || self.countdown > 0.0 {
                return None;
            }
            let f = self.time * GHOST_RATE;
            let i = f.floor() as usize;
            if (i + 1) * GHOST_VALUES >= g.len() {
                return None;
            }
            let t = f - i as f64;
            let value = |k: usize| g[i * GHOST_VALUES + k] + (g[(i + 1) * GHOST_VALUES + k] - g[i * GHOST_VALUES + k]) * t;
            let h0 = g[i * GHOST_VALUES + 3];
            let d = g[(i + 1) * GHOST_VALUES + 3] - h0;
            let turn = d.sin().atan2(d.cos());
            Some(CarPose {
                x: value(0),
                y: value(1),
                z: value(2),
                heading: h0 + turn * t,
                pitch: value(4),
                ground_roll: value(5),
            })
        }

        pub fn update(&mut self, dt: f64, car: CarPose, clock: f64, storage: &mut Storage) -> Option<TrialEvent> {
            if !self.active {
                return None;
            }
            let course = self.course.clone()?;
            let mut event = None;
            let points = &course.checkpoints;
            if self.countdown > 0.0 {
                self.countdown -= dt;
                if self.countdown <= 0.0 {
                    self.countdown = 0.0;
                    event = Some(TrialEvent::Go);
                    self.record(car, car, 1.0);
                }
            } else {
                self.time += dt;
                let last = self.last_pose.unwrap_or(car);
                self.record_timer += dt;
                while self.record_timer >= 1.0 / GHOST_RATE {
                    self.record_timer -= 1.0 / GHOST_RATE;
                    self.record(last, car, 1.0 - self.record_timer / dt);
                }
                let (fx, fz) = self.last.unwrap_or((car.x, car.z));
                let target = points[self.next];
                if distance_to_path(target.x, target.z, fx, fz, car.x, car.z) < course.radius + 0.5 {
                    self.next += 1;
                    if self.next == points.len() {
                        let t = if dt > 0.0 {
                            (1.0 + (1.0 / GHOST_RATE - self.record_timer) / dt).min(3.0)
                        } else {
                            1.0
                        };
                        self.record(last, car, t);
                        self.finish(storage);
                        return Some(TrialEvent::Finish);
                    }
                    event = Some(TrialEvent::Checkpoint);
                    self.show_arches();
                }
            }
            self.last = Some((car.x, car.z));
            self.last_pose = Some(car);
            let target = points[self.next];
            let mut arrow = car;
            arrow.y = car.y + 1.9 + (clock * 1000.0 / 300.0).sin() * 0.1;
            self.arrow = Some((arrow, (target.x - car.x).atan2(target.z - car.z)));
            event
        }

        fn record(&mut self, from: CarPose, to: CarPose, t: f64) {
            let at = |a: f64, b: f64| ((a + (b - a) * t) * 100.0).round() / 100.0;
            let d = to.heading - from.heading;
            let turn = d.sin().atan2(d.cos());
            self.recording.extend([
                at(from.x, to.x),
                at(from.y, to.y),
                at(from.z, to.z),
                at(from.heading, from.heading + turn),
                at(from.pitch, to.pitch),
                at(from.ground_roll, to.ground_roll),
            ]);
        }

        /// Map dots: (x, z, color, size).
        pub fn map_markers(&self) -> Vec<(f64, f64, u32, f32)> {
            let Some(c) = &self.course else { return Vec::new() };
            if !self.active {
                return Vec::new();
            }
            let p = &c.checkpoints;
            let mut out = vec![(p[self.next].x, p[self.next].z, 0xffd23f, 5.0)];
            if self.next + 1 < p.len() {
                out.push((p[self.next + 1].x, p[self.next + 1].z, 0xbbbbbb, 3.5));
            }
            if let Some(g) = self.ghost_pose() {
                out.push((g.x, g.z, 0x9fd3ff, 3.5));
            }
            out
        }

        fn finish(&mut self, storage: &mut Storage) {
            self.new_best = self.best == 0.0 || self.time < self.best;
            if self.new_best {
                self.best = self.time;
                self.ghost = Some(self.recording.clone());
                storage.set(&best_key(&self.map), self.time.to_string());
                storage.set_json(&ghost_key(&self.map), &self.recording);
            }
            self.stop();
        }

        fn show_arches(&mut self) {
            let last = self.arches.len().saturating_sub(1);
            for (i, a) in self.arches.iter_mut().enumerate() {
                *a = if i == self.next {
                    if self.next == last { ArchLook::Finish } else { ArchLook::Next }
                } else if i == self.next + 1 {
                    ArchLook::Later
                } else {
                    ArchLook::Hidden
                };
            }
        }
    }

    fn distance_to_path(px: f64, pz: f64, ax: f64, az: f64, bx: f64, bz: f64) -> f64 {
        let (dx, dz) = (bx - ax, bz - az);
        let len2 = dx * dx + dz * dz;
        let t = if len2 > 0.0 {
            (((px - ax) * dx + (pz - az) * dz) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (px - (ax + dx * t)).hypot(pz - (az + dz * t))
    }
}

pub mod freeroam {
    //! Things to do in free roam, on every map: gold wrenches to find, and
    //! challenges with three stars each (speed cameras, drift zones and jump
    //! zones). Stars only count in your career car; wrenches count in any car.
    //! Spots come from the same seeds as the 1.2.x game, so found wrenches carry over.
    use std::collections::HashSet;

    use bevy::math::DVec3;

    use crate::rng::Seeded;
    use crate::text::thousands;
    use crate::world::{BlockKind, HALF_SIZE, ROAD_LINES, World};

    /// Where you were in the open world when you left it, for "Start where
    /// I left off" (Garage → Maps).
    #[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
    #[serde(default, rename_all = "camelCase")]
    pub struct Resume {
        pub x: f64,
        pub z: f64,
        pub heading: f64,
        pub on_foot: bool,
        /// The car you were in, if it wasn't your own (checked by sanitize_config when used).
        pub car: Option<serde_json::Value>,
        pub night: bool,
    }

    const RESUME_KEY: &str = "carGame.cityResume.v1";

    impl Resume {
        pub fn load(storage: &crate::storage::Storage) -> Option<Resume> {
            storage
                .get_json(RESUME_KEY)
                .and_then(|v| serde_json::from_value::<Resume>(v).ok())
                .filter(|r| r.x.is_finite() && r.z.is_finite() && r.heading.is_finite())
        }

        pub fn save(&self, storage: &mut crate::storage::Storage) {
            storage.set_json(RESUME_KEY, self);
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ZoneKind {
        Speed,
        Drift,
        Jump,
    }

    impl ZoneKind {
        pub fn color(self) -> u32 {
            match self {
                ZoneKind::Speed => 0xff3b3b,
                ZoneKind::Drift => 0xffd23f,
                ZoneKind::Jump => 0x9b8cff,
            }
        }
        pub fn icon(self) -> &'static str {
            match self {
                ZoneKind::Speed => "📸",
                ZoneKind::Drift => "🌀",
                ZoneKind::Jump => "🚀",
            }
        }
        pub fn label(self) -> &'static str {
            match self {
                ZoneKind::Speed => "SPEED CAMERA",
                ZoneKind::Drift => "DRIFT ZONE",
                ZoneKind::Jump => "JUMP ZONE",
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct Zone {
        pub id: String,
        pub kind: ZoneKind,
        pub name: String,
        pub x: f64,
        pub z: f64,
        pub radius: f64,
        pub targets: [f64; 3],
    }

    #[derive(Clone, Debug)]
    pub struct Wrench {
        pub id: String,
        pub pos: DVec3,
    }

    #[derive(Clone, Debug)]
    pub enum FreeRoamEvent {
        Wrench(String),
        Zone { zone: Zone, score: f64, text: String },
    }

    #[derive(Default)]
    pub struct FreeRoam {
        pub zones: Vec<Zone>,
        pub wrenches: Vec<Wrench>,
        inside: Option<(usize, f64)>,
        carried: f64,
        cooldown: HashSet<String>,
        pub clock: f64,
        /// Counts up when the spots change (so the screen rebuilds them).
        pub generation: u64,
        pub visible: bool,
    }

    impl FreeRoam {
        pub fn load(&mut self, world: &World, found: &[String]) {
            self.clear();
            let map = &world.map;
            let first = map.chars().next().map_or(0, |c| c as u32);
            let mut rand = Seeded::new(9001.0 + map.len() as f64 * 31.0 + first as f64);
            let (wrenches, zones) = if world.track.is_some() {
                track_spots(world, &mut rand)
            } else {
                city_spots(world, &mut rand)
            };
            self.zones = zones;
            for (i, p) in wrenches.into_iter().enumerate() {
                let id = format!("{map}:{i}");
                if found.contains(&id) {
                    continue;
                }
                self.wrenches.push(Wrench { id, pos: p });
            }
            self.generation += 1;
        }

        /// Checks the car against the wrenches and challenges.
        pub fn update(
            &mut self,
            dt: f64,
            car: DVec3,
            speed: f64,
            airborne: bool,
            drift: f64,
            air_time: f64,
            reach: f64,
        ) -> Vec<FreeRoamEvent> {
            self.clock += dt;
            let mut events = Vec::new();
            let before = self.wrenches.len();
            self.wrenches.retain(|w| {
                let got = (car.x - w.pos.x).hypot(car.z - w.pos.z) < reach + 1.0 && (car.y + 0.6 - w.pos.y).abs() < 1.2 + reach * 0.3;
                if got {
                    events.push(FreeRoamEvent::Wrench(w.id.clone()));
                }
                !got
            });
            if self.wrenches.len() != before {
                self.generation += 1;
            }
            for (zi, z) in self.zones.iter().enumerate() {
                let distance = (car.x - z.x).hypot(car.z - z.z);
                if z.kind == ZoneKind::Speed {
                    if distance < z.radius && !self.cooldown.contains(&z.id) {
                        self.cooldown.insert(z.id.clone());
                        let mph = (speed * 2.237).round();
                        events.push(FreeRoamEvent::Zone {
                            zone: z.clone(),
                            score: mph,
                            text: crate::text::speed(mph),
                        });
                    } else if distance > z.radius + 20.0 {
                        self.cooldown.remove(&z.id);
                    }
                    continue;
                }
                let inside = distance < z.radius;
                if inside && self.inside.is_none() {
                    self.inside = Some((zi, 0.0));
                    self.carried = drift;
                }
                let Some((current, mut score)) = self.inside else { continue };
                if current != zi {
                    continue;
                }
                if drift < self.carried {
                    self.carried = 0.0;
                }
                if z.kind == ZoneKind::Jump && airborne {
                    score = score.max(air_time);
                    self.inside = Some((zi, score));
                }
                if !inside && z.kind == ZoneKind::Jump && airborne && score > 0.0 {
                    continue;
                }
                if !inside {
                    let total = if z.kind == ZoneKind::Drift {
                        score + (drift - self.carried).max(0.0)
                    } else {
                        score
                    };
                    self.inside = None;
                    if total > 0.0 {
                        let text = if z.kind == ZoneKind::Drift {
                            format!("{} pts", thousands(total))
                        } else {
                            format!("{total:.1} s air")
                        };
                        events.push(FreeRoamEvent::Zone {
                            zone: z.clone(),
                            score: total,
                            text,
                        });
                    }
                }
            }
            events
        }

        /// A finished drift inside a drift zone counts towards it.
        pub fn add_drift(&mut self, points: f64) {
            if let Some((zi, score)) = self.inside
                && self.zones[zi].kind == ZoneKind::Drift
            {
                self.inside = Some((zi, score + (points - self.carried).max(0.0)));
                self.carried = 0.0;
            }
        }

        pub fn hud(&self, x: f64, z: f64, drift: f64) -> String {
            if let Some((zi, score)) = self.inside {
                let zone = &self.zones[zi];
                let s = if zone.kind == ZoneKind::Drift {
                    format!("{} pts", thousands(score + (drift - self.carried).max(0.0)))
                } else {
                    format!("{score:.1} s")
                };
                return format!("{} {}: {s} · {}", zone.kind.icon(), zone.name, targets_text(zone));
            }
            for zone in &self.zones {
                if (x - zone.x).hypot(z - zone.z) < zone.radius + 60.0 {
                    return format!("{} {} ahead · {}", zone.kind.icon(), zone.name, targets_text(zone));
                }
            }
            String::new()
        }

        pub fn map_markers(&self, x: f64, z: f64) -> Vec<(f64, f64, u32, f32)> {
            let mut out: Vec<(f64, f64, u32, f32)> = self.zones.iter().map(|zn| (zn.x, zn.z, zn.kind.color(), 5.0)).collect();
            for w in &self.wrenches {
                if (x - w.pos.x).hypot(z - w.pos.z) < 90.0 {
                    out.push((w.pos.x, w.pos.z, 0xffc83d, 3.0));
                }
            }
            out
        }

        pub fn clear(&mut self) {
            self.zones.clear();
            self.wrenches.clear();
            self.inside = None;
            self.cooldown.clear();
            self.generation += 1;
        }
    }

    pub fn targets_text(z: &Zone) -> String {
        z.targets
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let target = match z.kind {
                    ZoneKind::Speed => crate::text::speed(*t),
                    ZoneKind::Jump => format!("{t} s"),
                    ZoneKind::Drift => thousands(*t),
                };
                format!("{} {target}", "★".repeat(i + 1))
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    pub fn zone_stars(z: &Zone, score: f64) -> u32 {
        z.targets.iter().filter(|t| score >= **t).count() as u32
    }

    fn track_spots(world: &World, rand: &mut Seeded) -> (Vec<DVec3>, Vec<Zone>) {
        let track = world.track.as_ref().expect("track");
        let half = track.style.width / 2.0;
        let mut wrenches = Vec::new();
        for i in 0..8 {
            let along = ((i as f64 + 0.3 + rand.next() * 0.4) / 8.0) * track.length;
            let offset = if i % 2 == 0 {
                (rand.next() - 0.5) * track.style.width * 0.7
            } else {
                (if rand.next() < 0.5 { -1.0 } else { 1.0 }) * (half + 2.5)
            };
            let p = track.pose(along, offset);
            wrenches.push(DVec3::new(p.x, world.height_at(p.x, p.z) + 1.3, p.z));
        }
        let n = track.points.len();
        let mut straights: Vec<(usize, usize)> = Vec::new();
        let mut run = 0;
        for k in 0..n * 2 {
            let i = k % n;
            if track.points[i].curve.abs() < 1.0 / 300.0 {
                run += 1;
            } else {
                if run > 10 && k >= n {
                    straights.push(((i + n - 3) % n, run));
                }
                run = 0;
            }
        }
        straights.sort_by_key(|s| std::cmp::Reverse(s.1));
        let mut zones = Vec::new();
        let fast = world.map == "oval";
        for (k, &(end, length)) in straights.iter().take(2).enumerate() {
            let p = track.points[end];
            let top = if fast {
                1.0
            } else {
                (0.8 + (length as f64 * 4.0) / 1200.0).min(1.25)
            };
            let base = if fast { [110.0, 145.0, 180.0] } else { [70.0, 95.0, 125.0] };
            zones.push(Zone {
                id: format!("{}:speed{k}", world.map),
                kind: ZoneKind::Speed,
                name: format!("{} straight camera", if k == 0 { "Main" } else { "Back" }),
                x: p.x,
                z: p.z,
                radius: half + 3.0,
                targets: base.map(|v| ((v * top) / 5.0).round() * 5.0),
            });
        }
        let (mut best, mut best_at) = (0.0, 0);
        for i in 0..n {
            let sum: f64 = (0..40).map(|k| track.points[(i + k) % n].curve.abs()).sum();
            if sum > best {
                best = sum;
                best_at = (i + 20) % n;
            }
        }
        let d = track.points[best_at];
        zones.push(Zone {
            id: format!("{}:drift", world.map),
            kind: ZoneKind::Drift,
            name: "Drift zone".into(),
            x: d.x,
            z: d.z,
            radius: 70.0,
            targets: [1500.0, 4000.0, 8000.0],
        });
        if !world.ramps.is_empty() {
            let r = world.ramps[world.ramps.len() / 2];
            zones.push(Zone {
                id: format!("{}:jump", world.map),
                kind: ZoneKind::Jump,
                name: "Infield jumps".into(),
                x: r.x,
                z: r.z,
                radius: 60.0,
                targets: [1.5, 2.5, 3.5],
            });
        }
        (wrenches, zones)
    }

    fn city_spots(world: &World, rand: &mut Seeded) -> (Vec<DVec3>, Vec<Zone>) {
        let mut wrenches: Vec<DVec3> = Vec::new();
        for r in world.ramps.iter().take(3) {
            let (fx, fz) = (r.angle.sin(), r.angle.cos());
            let lip = -r.length / 2.0 + r.length * 0.8;
            wrenches.push(DVec3::new(r.x + fx * (lip + 7.0), r.height + 2.6, r.z + fz * (lip + 7.0)));
        }
        for b in world.blocks.iter().filter(|b| b.kind == BlockKind::Park).take(2) {
            for tries in 0..50 {
                let p = DVec3::new(b.x + (rand.next() - 0.5) * 30.0, 1.3, b.z + (rand.next() - 0.5) * 30.0);
                if world.circles.iter().all(|c| (p.x - c.x).hypot(p.z - c.z) > c.r + 2.0) || tries == 49 {
                    wrenches.push(p);
                    break;
                }
            }
        }
        while wrenches.len() < 12 {
            let along = (rand.next() - 0.5) * (HALF_SIZE * 2.0 - 40.0);
            let across = ROAD_LINES[1 + ((rand.next() * (ROAD_LINES.len() - 2) as f64) as usize).min(ROAD_LINES.len() - 3)];
            let p = if rand.next() < 0.5 {
                DVec3::new(along, 1.3, across)
            } else {
                DVec3::new(across, 1.3, along)
            };
            if wrenches.iter().all(|w| w.distance(p) > 60.0) {
                wrenches.push(p);
            }
        }
        let stunt = world.blocks.iter().find(|b| b.kind == BlockKind::Stunt);
        let lot = world.blocks.iter().find(|b| b.kind == BlockKind::Lot);
        let mut zones = vec![
            Zone {
                id: "city:speed0".into(),
                kind: ZoneKind::Speed,
                name: "Downtown camera".into(),
                x: ROAD_LINES[3],
                z: 200.0,
                radius: 7.0,
                targets: [60.0, 80.0, 100.0],
            },
            Zone {
                id: "city:speed1".into(),
                kind: ZoneKind::Speed,
                name: "Crosstown camera".into(),
                x: -200.0,
                z: ROAD_LINES[3],
                radius: 7.0,
                targets: [60.0, 80.0, 100.0],
            },
            Zone {
                id: "city:drift".into(),
                kind: ZoneKind::Drift,
                name: "Parking lot drift zone".into(),
                x: lot.map_or(40.0, |b| b.x),
                z: lot.map_or(-40.0, |b| b.z),
                radius: 45.0,
                targets: [1500.0, 4000.0, 8000.0],
            },
        ];
        if let Some(s) = stunt {
            zones.push(Zone {
                id: "city:jump".into(),
                kind: ZoneKind::Jump,
                name: "Stunt park".into(),
                x: s.x,
                z: s.z,
                radius: 42.0,
                targets: [1.2, 2.0, 3.0],
            });
        }
        (wrenches, zones)
    }
}

pub mod skills {
    //! Skill chains: stylish driving scores points. Drifts, near misses,
    //! slipstreaming, jumps, overtakes, takedowns and boosts all add to a chain,
    //! and the more different skills in it, the bigger its multiplier (up to
    //! ×10). The chain is banked a few seconds after the last skill; a hard crash
    //! before then loses it all.
    use std::collections::BTreeSet;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum SkillKind {
        Drift,
        NearMiss,
        Draft,
        Air,
        Overtake,
        Takedown,
        PerfectStart,
        DriftBoost,
        Smash,
        Speed,
        Revenge,
        Collect,
    }

    impl SkillKind {
        pub fn label(self) -> &'static str {
            match self {
                SkillKind::Drift => "Drift",
                SkillKind::NearMiss => "Near miss",
                SkillKind::Draft => "Slipstream",
                SkillKind::Air => "Air",
                SkillKind::Overtake => "Overtake",
                SkillKind::Takedown => "Takedown",
                SkillKind::PerfectStart => "Perfect start",
                SkillKind::DriftBoost => "Drift boost",
                SkillKind::Smash => "Smash",
                SkillKind::Speed => "Speed demon",
                SkillKind::Revenge => "Revenge",
                SkillKind::Collect => "Collectible",
            }
        }

        /// The color it's shown in (sRGB hex).
        pub fn color(self) -> u32 {
            match self {
                SkillKind::Drift => 0xffd23f,
                SkillKind::NearMiss => 0xff70a6,
                SkillKind::Draft => 0x7af0ff,
                SkillKind::Air => 0x9b8cff,
                SkillKind::Overtake => 0x3bb273,
                SkillKind::Takedown => 0xff5a36,
                SkillKind::PerfectStart => 0xffffff,
                SkillKind::DriftBoost => 0x4fb3ff,
                SkillKind::Smash => 0xff9f40,
                SkillKind::Speed => 0xff3b3b,
                SkillKind::Revenge | SkillKind::Collect => 0xd4af37,
            }
        }

        /// The name the 1.2.x game saved its stats under.
        pub fn stat(self) -> &'static str {
            match self {
                SkillKind::NearMiss => "nearMiss",
                SkillKind::Takedown => "takedown",
                SkillKind::Overtake => "overtake",
                SkillKind::PerfectStart => "perfectStart",
                SkillKind::DriftBoost => "driftBoost",
                _ => "",
            }
        }
    }

    /// Seconds after the last skill before the chain is banked.
    pub const CHAIN_TIME: f64 = 4.0;
    const MAX_MULTIPLIER: f64 = 10.0;

    #[derive(Clone, Debug)]
    pub struct FeedLine {
        pub kind: SkillKind,
        pub points: f64,
        pub label: String,
    }

    /// What happened to a chain.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum ChainEnd {
        Banked(f64),
        Broken(f64),
    }

    #[derive(Default, Clone, Debug)]
    pub struct SkillChain {
        pub points: f64,
        pub count: usize,
        kinds: BTreeSet<SkillKind>,
        pub timer: f64,
        pub feed: Vec<FeedLine>,
    }

    impl SkillChain {
        /// ×1 for one kind of skill, +1 for each other kind, +0.5 for every 5 skills, up to ×10.
        pub fn multiplier(&self) -> f64 {
            if self.count == 0 {
                return 1.0;
            }
            (self.kinds.len() as f64 + (self.count / 5) as f64 * 0.5).min(MAX_MULTIPLIER)
        }

        pub fn total(&self) -> f64 {
            (self.points * self.multiplier()).round()
        }

        pub fn add(&mut self, kind: SkillKind, points: f64, label: Option<&str>) {
            let points = points.round();
            if points <= 0.0 {
                return;
            }
            let label = label.unwrap_or(kind.label()).to_string();
            self.points += points;
            self.count += 1;
            self.kinds.insert(kind);
            self.timer = CHAIN_TIME;
            match self.feed.first_mut() {
                Some(top) if top.kind == kind && top.label == label => top.points += points,
                _ => self.feed.insert(0, FeedLine { kind, points, label }),
            }
            self.feed.truncate(5);
        }

        pub fn update(&mut self, dt: f64) -> Option<ChainEnd> {
            if self.count == 0 {
                return None;
            }
            self.timer -= dt;
            if self.timer <= 0.0 { self.bank() } else { None }
        }

        pub fn bank(&mut self) -> Option<ChainEnd> {
            if self.count == 0 {
                return None;
            }
            let total = self.total();
            self.clear();
            Some(ChainEnd::Banked(total))
        }

        pub fn break_chain(&mut self) -> Option<ChainEnd> {
            if self.count == 0 {
                return None;
            }
            let lost = self.total();
            self.clear();
            Some(ChainEnd::Broken(lost))
        }

        pub fn clear(&mut self) {
            *self = SkillChain::default();
        }
    }
}

pub mod weapons {
    //! The car's three weapons:
    //!   1. Machine gun: fast, weak, overheats if you hold the trigger too long.
    //!   2. Rockets: fly forward and explode, wrecking everything nearby.
    //!   3. Shockwave: a blast all around the car, then a few seconds to recharge.
    //!
    //! Which of them the car has, and how upgraded each one is, is its loadout.
    use bevy::math::{DQuat, DVec3};
    use bevy::prelude::*;

    use crate::audio::Sound;
    use crate::career::{Loadout, STOCK_WEAPONS};
    use crate::rng;
    use crate::world::{TargetKind, World};

    pub const WEAPON_NAMES: [&str; 3] = ["Machine gun", "Rockets", "Shockwave"];

    const GUN_SHOTS_PER_SECOND: f64 = 14.0;
    const GUN_DAMAGE: f64 = 9.0;
    const GUN_RANGE: f64 = 300.0;
    const GUN_HEAT_PER_SHOT: f64 = 0.045;
    const GUN_COOLING: f64 = 0.45;
    const ROCKET_RELOAD: f64 = 0.45;
    const ROCKET_SPEED: f64 = 85.0;
    const ROCKET_LIFETIME: f64 = 3.0;
    const ROCKET_RADIUS: f64 = 8.0;
    const ROCKET_DAMAGE: f64 = 170.0;
    const SHOCKWAVE_RELOAD: f64 = 6.0;
    const SHOCKWAVE_RADIUS: f64 = 16.0;
    const SHOCKWAVE_DAMAGE: f64 = 150.0;
    const DAMAGE_PER_LEVEL: f64 = 0.15;
    const QUICKER_PER_LEVEL: f64 = 0.08;

    /// An explosion near the player, so the game can shake the camera and push the car.
    #[derive(Clone, Copy, Debug)]
    pub struct Blast {
        pub at: DVec3,
        pub radius: f64,
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct Rocket {
        pub pos: DVec3,
        pub velocity: DVec3,
        pub age: f64,
        pub flying: bool,
    }

    /// Where the car is: its root's position, turn and size, and where its weapons fire from (in car space).
    pub struct CarFrame {
        pub position: DVec3,
        pub rotation: DQuat,
        pub scale: f64,
        pub gun_muzzle: DVec3,
        pub rocket_muzzle: DVec3,
    }

    impl CarFrame {
        fn to_world(&self, local: DVec3) -> DVec3 {
            self.position + self.rotation * (local * self.scale)
        }
    }

    #[derive(Resource)]
    pub struct Weapons {
        pub selected: usize,
        pub heat: f64,
        pub overheated: bool,
        gun_wait: f64,
        rocket_wait: f64,
        shockwave_wait: f64,
        next_tube: usize,
        pub rockets: Vec<Rocket>,
        levels: Loadout,
    }

    impl Default for Weapons {
        fn default() -> Self {
            Weapons {
                selected: 0,
                heat: 0.0,
                overheated: false,
                gun_wait: 0.0,
                rocket_wait: 0.0,
                shockwave_wait: 0.0,
                next_tube: 0,
                rockets: vec![Rocket::default(); 12],
                levels: STOCK_WEAPONS,
            }
        }
    }

    impl Weapons {
        pub fn name(&self) -> &'static str {
            WEAPON_NAMES[self.selected]
        }

        /// 0..1: 1 means ready to fire.
        pub fn readiness(&self) -> f64 {
            match self.selected {
                0 => 1.0 - self.heat,
                1 => 1.0 - self.rocket_wait.max(0.0) / self.quicker(ROCKET_RELOAD, 1),
                _ => 1.0 - self.shockwave_wait.max(0.0) / self.quicker(SHOCKWAVE_RELOAD, 2),
            }
        }

        pub fn armed(&self) -> bool {
            self.levels.iter().any(|l| *l > 0)
        }

        pub fn loadout(&self) -> Loadout {
            self.levels
        }

        pub fn set_loadout(&mut self, levels: Loadout) {
            self.levels = levels;
            if self.levels[self.selected] == 0 {
                self.selected = self.levels.iter().position(|l| *l > 0).unwrap_or(0);
            }
        }

        pub fn has(&self, index: usize) -> bool {
            self.levels.get(index).copied().unwrap_or(0) > 0
        }

        pub fn select(&mut self, index: usize) -> bool {
            let i = index % 3;
            if !self.has(i) {
                return false;
            }
            self.selected = i;
            true
        }

        pub fn select_next(&mut self) {
            for step in 1..=3 {
                if self.select(self.selected + step) {
                    return;
                }
            }
        }

        fn damage(&self, base: f64, index: usize) -> f64 {
            base * (1.0 + DAMAGE_PER_LEVEL * self.levels[index].saturating_sub(1) as f64)
        }

        fn quicker(&self, base: f64, index: usize) -> f64 {
            base * (1.0 - QUICKER_PER_LEVEL * self.levels[index].saturating_sub(1) as f64)
        }

        pub fn clear(&mut self) {
            for r in &mut self.rockets {
                r.flying = false;
            }
        }

        /// Fires the selected weapon while `firing`, and moves rockets along. Returns the explosions.
        pub fn update(
            &mut self,
            dt: f64,
            mut firing: bool,
            car: &CarFrame,
            car_velocity: DVec3,
            world: &mut World,
            sound: &Sound,
        ) -> Vec<Blast> {
            let mut blasts = Vec::new();
            if !self.has(self.selected) {
                firing = false;
            }
            self.gun_wait -= dt;
            self.rocket_wait -= dt;
            self.shockwave_wait -= dt;
            let gun_firing = firing && self.selected == 0 && !self.overheated;
            self.heat = (self.heat - GUN_COOLING * dt * if gun_firing { 0.3 } else { 1.0 }).max(0.0);
            if self.overheated && self.heat < 0.35 {
                self.overheated = false;
            }
            if firing && self.selected == 0 {
                while self.gun_wait <= 0.0 && !self.overheated {
                    self.gun_wait += 1.0 / GUN_SHOTS_PER_SECOND;
                    self.fire_bullet(car, world, sound);
                }
            }
            if self.gun_wait < 0.0 {
                self.gun_wait = 0.0;
            }
            if firing && self.selected == 1 && self.rocket_wait <= 0.0 {
                self.rocket_wait = self.quicker(ROCKET_RELOAD, 1);
                self.fire_rocket(car, car_velocity, world, sound);
            }
            if firing && self.selected == 2 && self.shockwave_wait <= 0.0 {
                self.shockwave_wait = self.quicker(SHOCKWAVE_RELOAD, 2);
                let at = car.position;
                world.effects.shockwave(at, SHOCKWAVE_RADIUS);
                let damage = self.damage(SHOCKWAVE_DAMAGE, 2);
                world.explode(at, SHOCKWAVE_RADIUS, damage);
                sound.shockwave();
                blasts.push(Blast { at, radius: 0.0 });
            }
            self.move_rockets(dt, &mut blasts, world);
            blasts
        }

        fn fire_bullet(&mut self, car: &CarFrame, world: &mut World, sound: &Sound) {
            let from = car.to_world(car.gun_muzzle);
            let direction = aim(car, from, false, world);
            self.heat += self.quicker(GUN_HEAT_PER_SHOT, 0);
            if self.heat >= 1.0 {
                self.heat = 1.0;
                self.overheated = true;
            }
            world.effects.muzzle_flash(from, direction);
            sound.gunshot();
            let hit = world.raycast(from, direction, GUN_RANGE);
            let to = hit.map_or(from + direction * GUN_RANGE, |h| h.point);
            world.effects.tracer(from, to);
            if let Some(h) = hit {
                let damage = self.damage(GUN_DAMAGE, 0);
                world.apply_hit(&h, damage, direction);
                let dusty = h.ground || h.target.is_some_and(|t| world.targets[t].kind == TargetKind::Building);
                world.effects.bullet_hit(h.point, dusty);
            }
        }

        fn fire_rocket(&mut self, car: &CarFrame, car_velocity: DVec3, world: &mut World, sound: &Sound) {
            let i = self.rockets.iter().position(|r| !r.flying).unwrap_or(0);
            let tube = self.next_tube % 4;
            self.next_tube += 1;
            let mut muzzle = car.rocket_muzzle;
            muzzle.x += if tube.is_multiple_of(2) { -0.075 } else { 0.075 };
            muzzle.y += if tube < 2 { -0.07 } else { 0.07 };
            let from = car.to_world(muzzle);
            let direction = aim(car, from, true, world);
            self.rockets[i] = Rocket {
                pos: from,
                velocity: direction * ROCKET_SPEED + car_velocity,
                age: 0.0,
                flying: true,
            };
            world.effects.muzzle_flash(from, direction);
            sound.rocket_launch();
        }

        fn move_rockets(&mut self, dt: f64, blasts: &mut Vec<Blast>, world: &mut World) {
            let damage = self.damage(ROCKET_DAMAGE, 1);
            for r in &mut self.rockets {
                if !r.flying {
                    continue;
                }
                r.age += dt;
                let step = r.velocity * dt;
                let direction = step.normalize_or_zero();
                let hit = world.raycast(r.pos, direction, step.length());
                let too_old = r.age > ROCKET_LIFETIME;
                let in_ground = r.pos.y < world.height_at(r.pos.x, r.pos.z);
                if hit.is_some() || too_old || in_ground {
                    let at = hit.map_or(r.pos, |h| h.point);
                    if let Some(h) = &hit {
                        world.apply_hit(h, damage / 2.0, direction);
                    }
                    world.effects.explosion(at, 2.0);
                    world.explode(at, ROCKET_RADIUS, damage);
                    blasts.push(Blast { at, radius: ROCKET_RADIUS });
                    r.flying = false;
                    continue;
                }
                r.pos += step;
                r.velocity.y -= 1.5 * dt;
                let tail = r.pos - direction * 0.7;
                use crate::effects::{ParticleOptions, rgb};
                world.effects.add_smoke(ParticleOptions {
                    at: tail,
                    velocity: DVec3::new(0.0, 0.4, 0.0),
                    life: 1.5,
                    size: [0.35, 1.6],
                    color: rgb(0xd8d8d8),
                    opacity: 0.55,
                    drag: 1.0,
                    ..Default::default()
                });
                world.effects.add_glow(ParticleOptions {
                    at: tail,
                    life: 0.08,
                    size: [0.6, 0.2],
                    color: rgb(0xffb347),
                    ..Default::default()
                });
            }
        }
    }

    /// Which way to shoot: straight ahead, unless something is almost in line; then right at it.
    fn aim(car: &CarFrame, from: DVec3, level: bool, world: &World) -> DVec3 {
        let forward = car.rotation * DVec3::Z;
        let flat_forward = DVec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let mut best: Option<DVec3> = None;
        let mut best_distance = 120.0;
        let (circles, _) = world.nearby(from.x, from.z, best_distance);
        let mut targets: Vec<_> = world
            .car_targets()
            .chain(circles.into_iter().filter_map(|i| world.circles[i].target))
            .collect();
        targets.sort_unstable();
        for i in targets {
            let t = &world.targets[i];
            if !t.alive || (t.kind == TargetKind::Car && t.rig.is_none()) {
                continue;
            }
            let middle = match t.kind {
                TargetKind::Car => world.boxes[t.shape].base + 0.7,
                TargetKind::Tree => world.circles[t.shape].base + 2.5,
                TargetKind::Lamp => world.circles[t.shape].base + 4.0,
                TargetKind::Building => continue,
            };
            let to = DVec3::new(t.x - from.x, middle - from.y, t.z - from.z);
            let distance = to.length();
            let flat = DVec3::new(to.x, 0.0, to.z).normalize_or_zero();
            if flat.dot(flat_forward) > 0.992 && distance > 3.0 && distance < best_distance {
                best_distance = distance;
                best = Some(to.normalize());
            }
        }
        for h in &world.hit_boxes {
            if !h.alive {
                continue;
            }
            let to = DVec3::new(h.x - from.x, h.y + h.height / 2.0 - from.y, h.z - from.z);
            let distance = to.length();
            let flat = DVec3::new(to.x, 0.0, to.z).normalize_or_zero();
            if flat.dot(flat_forward) > 0.985 && distance > 3.0 && distance < best_distance {
                best_distance = distance;
                best = Some(to.normalize());
            }
        }
        let mut direction =
            best.unwrap_or_else(|| DVec3::new(forward.x, forward.y - if level { 0.0 } else { from.y / 60.0 }, forward.z).normalize());
        let spread = if level { 0.004 } else { 0.012 };
        direction.x += rng::centered() * spread;
        direction.y += rng::centered() * spread;
        direction.z += rng::centered() * spread;
        direction.normalize()
    }
}
