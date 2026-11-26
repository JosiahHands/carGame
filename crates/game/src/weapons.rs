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
    for t in &world.targets {
        if !t.alive || t.kind == TargetKind::Building || (t.kind == TargetKind::Car && t.rig.is_none()) {
            continue;
        }
        let middle = match t.kind {
            TargetKind::Car => 0.7,
            TargetKind::Tree => 2.5,
            _ => 4.0,
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
