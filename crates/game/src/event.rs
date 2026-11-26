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
                trap_speed += self.traps_crossed(furthest, racer.distance) as f64 * racer.physics.speed() * MPH;
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
            let s = trap_speed.floor();
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
                    let mph = (self.racers[0].physics.speed() * MPH).round();
                    self.score += mph * crossed as f64;
                    self.notes.push(format!("📸 {mph} mph"));
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
            EventKind::SpeedTrap => format!("{} mph", value.round()),
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

    pub fn hud(&self) -> EventHud {
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
                stats.push(("SCORE".into(), self.format_score(self.score)));
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
                warning = "RETURN TO THE TRACK · R to reset".into();
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
