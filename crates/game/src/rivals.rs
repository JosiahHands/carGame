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
