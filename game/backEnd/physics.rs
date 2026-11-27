//! Arcade car physics: acceleration, a gearbox (automatic or manual), braking,
//! steering, grip, handbrake drifts, jumps off ramps and bumping into things.
//! Hand-written, no physics engine. A pack (Real Physics) can replace the
//! driving part with its own model through `CarDynamics`.
use std::any::Any;
use std::sync::Arc;

use bevy::math::DVec3;

use crate::config::CarConfig;
use crate::track::WALL_THICKNESS;
use crate::world::World;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarInput {
    pub throttle: f64,
    pub brake: f64,
    /// -1 (right) .. 1 (left)
    pub steer: f64,
    pub handbrake: bool,
    pub boost: bool,
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StepEvents {
    /// Speed lost hitting something, m/s.
    pub impact: f64,
    /// What the car hit hardest, if it can be destroyed (a target index).
    pub hit_target: Option<usize>,
    /// Lamps and trees the car drove straight through.
    pub smashed: u32,
    pub cones: u32,
    /// Vertical speed on landing, m/s.
    pub landed: f64,
}

const EARTH_GRAVITY: f64 = 16.0;
pub const MAX_REVERSE: f64 = 12.0;
pub const MPH: f64 = 0.44704;
/// Most sideways acceleration the tires can give, in m/s², at normal grip on dry asphalt.
pub const TIRE_GRIP: f64 = 22.0;
const DRIFT_TURN: f64 = 3.0;
/// A car this tall (meters) or taller flattens anything up to twice its height.
pub const GIANT_HEIGHT: f64 = 3.0;
/// Where each forward gear runs out of revs, as a share of the car's top speed.
pub const GEAR_TOPS: [f64; 5] = [0.18, 0.34, 0.52, 0.72, 1.0];

/// Another way to drive the cars, from a pack. It replaces the arcade
/// model's engine, brakes, steering and tires; the game still does crashes,
/// jumps, ramps and slopes.
pub trait CarDynamics: Send + Sync {
    /// One step of driving. Updates vx, vz, heading, steer_angle, gear, revs,
    /// limiter and braking, and returns the turn rate (rad/s, positive left).
    fn drive(&self, car: &mut CarPhysics, dt: f64, input: &CarInput, world: &World) -> f64;
    /// Most sideways acceleration the tires can give at this speed, m/s².
    fn corner_grip(&self, car: &CarPhysics, speed: f64, surface_grip: f64) -> f64;
    /// The car hit something: (nx, nz) points away from it, at a point `along` forward and `across` left of the center.
    fn impact(&self, _car: &mut CarPhysics, _nx: f64, _nz: f64, _along: f64, _across: f64, _lost: f64) {}
    /// The car was put somewhere new, standing still.
    fn reset(&self, _car: &mut CarPhysics, _fresh: bool) {}
    /// After each step: may set the body's lean from the model's own suspension.
    fn pose(&self, _car: &mut CarPhysics, _dt: f64) {}
}

/// The pack's driving model, shared by every car, or none for the arcade model.
pub type Dynamics = Option<Arc<dyn CarDynamics>>;

#[derive(Default)]
pub struct CarPhysics {
    /// The player's car (a pack's model may treat it differently from the rivals').
    pub player: bool,
    /// Each car's own state for the pack's driving model.
    pub dynamics_state: Option<Box<dyn Any + Send + Sync>>,
    pub x: f64,
    pub z: f64,
    pub y: f64,
    /// Radians; 0 faces +z, positive turns left.
    pub heading: f64,
    pub vx: f64,
    pub vz: f64,
    pub vy: f64,
    pub steer_angle: f64,
    pub airborne: bool,
    pub roll: f64,
    pub body_pitch: f64,
    /// Ground slope pitch, nose-up negative.
    pub pitch: f64,
    /// Ground slope sideways; positive lifts the left side.
    pub ground_roll: f64,
    pub on_grass: bool,
    pub forward_speed: f64,
    pub lateral_speed: f64,
    pub braking: bool,
    /// Nitro left in the bottle, 0..1.
    pub nitro: f64,
    pub boosting: bool,
    pub manual: bool,
    /// Forward gear, 1 to 5.
    pub gear: usize,
    /// 0 at a standstill, 1 at the redline of the current gear.
    pub revs: f64,
    pub limiter: bool,
    pub accel: f64,
    /// m/s
    pub top_speed: f64,
    pub grip_multiplier: f64,
    pub grip: f64,
    pub brake_decel: f64,
    pub max_steer: f64,
    pub drift_grip: f64,
    pub downforce: f64,
    pub nitro_power: f64,
    pub nitro_burn: f64,
    pub infinite_nitro: bool,
    pub jump_speed: f64,
    pub air_control: f64,
    pub gravity: f64,
    pub bounce: f64,
    pub wheel_base: f64,
    pub length: f64,
    pub width: f64,
    pub height: f64,
    pub surface_grip: f64,
    pub all_terrain: bool,
    /// Slipstream, 0..1.
    pub draft: f64,
    /// Seconds of free boost left (drift mini-turbo, rocket start).
    pub turbo: f64,
    pub turbo_power: f64,
    pub config: Option<CarConfig>,
    last_forward: f64,
    jump_held: bool,
}

/// The car's size, from its model.
#[derive(Clone, Copy, Debug)]
pub struct Dims {
    pub length: f64,
    pub width: f64,
    pub height: f64,
    pub wheel_base: f64,
}

fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

impl CarPhysics {
    pub fn new() -> Self {
        CarPhysics {
            nitro: 1.0,
            gear: 1,
            accel: 9.0,
            top_speed: 50.0,
            grip_multiplier: 1.0,
            grip: 7.0,
            brake_decel: 20.0,
            max_steer: 0.5,
            drift_grip: 0.25,
            nitro_power: 1.0,
            nitro_burn: 0.3,
            air_control: 0.15,
            gravity: EARTH_GRAVITY,
            bounce: 0.35,
            wheel_base: 2.6,
            length: 4.4,
            width: 1.9,
            height: 1.3,
            surface_grip: 1.0,
            turbo_power: 1.0,
            ..Default::default()
        }
    }

    pub fn configure(&mut self, config: &CarConfig, dims: Dims) {
        let t = &config.tuning;
        self.config = Some(config.clone());
        self.accel = 10.0 * (t.horsepower / 400.0).powf(0.6);
        self.top_speed = t.top_speed * MPH;
        self.grip_multiplier = t.grip;
        self.grip = 7.0 * t.grip;
        self.brake_decel = 22.0 * t.brakes;
        self.max_steer = t.steering * std::f64::consts::PI / 180.0;
        self.drift_grip = (0.45 - t.drift * 0.04).max(0.06);
        self.downforce = t.downforce;
        self.nitro_power = 1.2 * t.nitro_power;
        self.nitro_burn = 1.0 / t.nitro_seconds;
        self.infinite_nitro = t.infinite_nitro;
        self.gravity = EARTH_GRAVITY * t.gravity;
        self.jump_speed = (2.0 * self.gravity * t.jump).sqrt();
        self.air_control = t.air_control;
        self.bounce = t.bounce;
        self.wheel_base = dims.wheel_base;
        self.length = dims.length;
        self.width = dims.width;
        self.height = dims.height;
    }

    /// Puts the car somewhere, standing still. `fresh` is a new start (an event's grid).
    pub fn reset(&mut self, x: f64, z: f64, heading: f64, fresh: bool, dynamics: &Dynamics) {
        self.x = x;
        self.z = z;
        self.y = 0.0;
        self.heading = heading;
        self.vx = 0.0;
        self.vz = 0.0;
        self.vy = 0.0;
        self.steer_angle = 0.0;
        self.roll = 0.0;
        self.body_pitch = 0.0;
        self.pitch = 0.0;
        self.ground_roll = 0.0;
        self.airborne = false;
        self.nitro = 1.0;
        self.boosting = false;
        self.turbo = 0.0;
        self.draft = 0.0;
        self.gear = 1;
        self.revs = 0.0;
        self.limiter = false;
        if let Some(d) = dynamics {
            d.reset(self, fresh);
        }
    }

    /// Changes up a gear (manual gearbox). False in top gear.
    pub fn shift_up(&mut self) -> bool {
        if self.gear >= GEAR_TOPS.len() {
            return false;
        }
        self.gear += 1;
        true
    }

    /// Changes down a gear. False in first, or too fast for the lower gear.
    pub fn shift_down(&mut self) -> bool {
        if self.gear <= 1 || self.forward_speed > GEAR_TOPS[self.gear - 2] * self.top_speed * 1.01 {
            return false;
        }
        self.gear -= 1;
        true
    }

    /// A free boost for `seconds`, as strong as nitro × `power`.
    pub fn give_turbo(&mut self, seconds: f64, power: f64) {
        if self.turbo <= 0.0 || power >= self.turbo_power {
            self.turbo_power = power;
        }
        self.turbo = self.turbo.max(seconds);
    }

    /// Shoves the car, for example away from an explosion.
    pub fn push(&mut self, vx: f64, vy: f64, vz: f64) {
        self.vx += vx;
        self.vz += vz;
        if vy > 0.0 {
            self.vy = self.vy.max(0.0) + vy;
            self.y += 0.05;
            self.airborne = true;
        }
    }

    pub fn speed(&self) -> f64 {
        self.vx.hypot(self.vz)
    }

    /// Most sideways acceleration the tires can give at this speed, with downforce and the surface.
    pub fn corner_grip(&self, speed: f64, dynamics: &Dynamics) -> f64 {
        if let Some(d) = dynamics {
            return d.corner_grip(self, speed, self.surface_grip);
        }
        TIRE_GRIP * self.grip_multiplier * self.surface_grip * (1.0 + (self.downforce / 10.0) * (speed / (100.0 * MPH)).powi(2))
    }

    /// The fastest the car can take a bend of this curvature, in m/s (infinity if it always can).
    pub fn corner_speed(&self, curve: f64, surface_grip: f64, dynamics: &Dynamics) -> f64 {
        if let Some(d) = dynamics {
            let (mut lo, mut hi) = (0.0, 2000.0);
            if d.corner_grip(self, hi, surface_grip) >= hi * hi * curve.abs() {
                return f64::INFINITY;
            }
            for _ in 0..40 {
                let v = (lo + hi) / 2.0;
                if d.corner_grip(self, v, surface_grip) >= v * v * curve.abs() {
                    lo = v;
                } else {
                    hi = v;
                }
            }
            return lo;
        }
        let g = TIRE_GRIP * self.grip_multiplier * surface_grip;
        let k = (self.downforce / 10.0) / (100.0 * MPH).powi(2);
        let room = curve.abs() - g * k;
        if room <= 0.0 { f64::INFINITY } else { (g / room).sqrt() }
    }

    /// The steering input (-1..1) that turns along a bend of this curvature at the current speed.
    pub fn steer_for(&self, curve: f64) -> f64 {
        let lock = self.max_steer / (1.0 + self.forward_speed.abs() / 28.0);
        ((curve * self.wheel_base).atan() / lock).clamp(-1.0, 1.0)
    }

    /// 0..1, how close to top speed.
    pub fn speed_fraction(&self) -> f64 {
        (self.forward_speed.abs() / self.top_speed).min(1.0)
    }

    pub fn step(&mut self, dt: f64, input: &CarInput, world: &mut World, dynamics: &Dynamics) -> StepEvents {
        let mut events = StepEvents::default();
        let mut fx = self.heading.sin();
        let mut fz = self.heading.cos();
        let mut vf = self.vx * fx + self.vz * fz;
        let mut vl = self.vx * fz - self.vz * fx;
        let control = if self.airborne { self.air_control } else { 1.0 };

        let yaw_rate = match dynamics {
            Some(d) => d.drive(self, dt, input, world),
            None => self.arcade(dt, input, world, vf, vl, fx, fz, control),
        };

        let pieces = ((self.speed() * dt) / (self.width.min(self.length) * 0.4)).ceil().clamp(1.0, 12.0) as usize;
        for _ in 0..pieces {
            self.x += self.vx * dt / pieces as f64;
            self.z += self.vz * dt / pieces as f64;
            let hit = self.collide(world, &mut events, dynamics);
            events.impact = events.impact.max(hit);
        }

        if input.jump && !self.jump_held && !self.airborne && self.jump_speed > 0.0 {
            self.vy = self.jump_speed;
            self.y += 0.05;
        }
        self.jump_held = input.jump;

        let ground = world.height_at(self.x, self.z);
        let drop = self.y - ground;
        if !self.airborne && self.vy <= 0.0 && drop > 0.0 && drop < self.speed() * dt * 0.2 + 0.02 {
            self.y = ground;
        }
        if self.y > ground + 0.02 {
            self.vy -= self.gravity * dt;
            self.y += self.vy * dt;
            if self.y <= ground {
                events.landed = (-self.vy).max(0.0);
                self.y = ground;
                self.vy = 0.0;
            }
        } else {
            self.vy = (ground - self.y) / dt;
            self.y = ground;
        }
        self.airborne = self.y > ground + 0.15;

        fx = self.heading.sin();
        fz = self.heading.cos();
        if !self.airborne {
            let half = self.wheel_base / 2.0;
            let h_front = world.height_at(self.x + fx * half, self.z + fz * half);
            let h_rear = world.height_at(self.x - fx * half, self.z - fz * half);
            let target = -(h_front - h_rear).atan2(self.wheel_base);
            self.pitch += (target - self.pitch) * (dt * 20.0).min(1.0);
            let side = self.width / 2.0;
            let h_left = world.height_at(self.x + fz * side, self.z - fx * side);
            let h_right = world.height_at(self.x - fz * side, self.z + fx * side);
            let roll_target = (h_left - h_right).atan2(self.width);
            self.ground_roll += (roll_target - self.ground_roll) * (dt * 20.0).min(1.0);
        } else {
            let target = -self.vy.atan2(vf.abs().max(4.0)) * 0.6;
            self.pitch += (target - self.pitch) * (dt * 2.0).min(1.0);
            self.ground_roll -= self.ground_roll * (dt * 2.0).min(1.0);
        }

        events.cones = world.hit_cones(self.x, self.z, self.y, self.width / 2.0 + 0.2, self.vx, self.vz);

        vf = self.vx * fx + self.vz * fz;
        vl = self.vx * fz - self.vz * fx;
        let long_accel = (vf - self.last_forward) / dt;
        self.last_forward = vf;
        self.forward_speed = vf;
        self.lateral_speed = vl;
        let lat_accel = if self.airborne { 0.0 } else { vf * yaw_rate };
        let roll_target = (lat_accel * 0.008).clamp(-0.1, 0.1);
        let pitch_target = if self.airborne {
            0.0
        } else {
            (-long_accel * 0.003).clamp(-0.06, 0.06)
        };
        self.roll += (roll_target - self.roll) * (dt * 6.0).min(1.0);
        self.body_pitch += (pitch_target - self.body_pitch) * (dt * 6.0).min(1.0);
        if let Some(d) = dynamics {
            d.pose(self, dt);
        }
        events
    }

    /// The arcade driving model: one step of engine, brakes, steering and grip.
    #[allow(clippy::too_many_arguments)]
    fn arcade(&mut self, dt: f64, input: &CarInput, world: &World, mut vf: f64, mut vl: f64, fx: f64, fz: f64, control: f64) -> f64 {
        let steer_target = input.steer * self.max_steer / (1.0 + vf.abs() / 28.0);
        self.steer_angle += (steer_target - self.steer_angle) * (dt * 10.0).min(1.0);

        if !self.manual {
            self.gear = 1;
            while self.gear < GEAR_TOPS.len() && vf.abs() > GEAR_TOPS[self.gear - 1] * self.top_speed {
                self.gear += 1;
            }
        }
        self.revs = if vf < 0.0 {
            -vf / MAX_REVERSE
        } else {
            vf / (GEAR_TOPS[self.gear - 1] * self.top_speed)
        };
        let mut pull = 1.0;
        if self.manual {
            if self.revs >= 1.0 {
                pull = 0.0;
            } else if self.gear > 1 {
                pull = 0.12 + 0.88 * (self.revs / 0.53).min(1.0).powi(2);
            }
        }
        self.limiter = self.manual && self.revs >= 1.0 && input.throttle > 0.0;

        self.braking = false;
        if input.throttle > 0.0 {
            if vf < -0.5 {
                vf = (vf + self.brake_decel * input.throttle * dt * control).min(0.0);
                self.braking = true;
            } else {
                let top = self.top_speed * (1.0 + 0.08 * self.draft);
                let headroom = (1.0 - (vf / top).powi(2)).max(0.0);
                vf += self.accel * input.throttle * headroom * pull * dt * control;
            }
        }
        let power = self.use_boost(dt, input, vf);
        if power > 0.0 {
            let top = self.top_speed * (1.0 + 0.35 * power);
            let headroom = (1.0 - (vf / top).powi(2)).max(0.0);
            vf += self.accel * 1.6 * power * headroom * dt * if self.airborne { self.air_control.max(0.4) } else { 1.0 };
        }
        if input.brake > 0.0 {
            if vf > 0.5 {
                vf = (vf - self.brake_decel * input.brake * dt * control).max(0.0);
                self.braking = true;
            } else if vf > -MAX_REVERSE {
                vf -= self.accel * 0.6 * input.brake * dt * control;
            }
        }
        if input.handbrake && !self.airborne {
            let s = sign(vf);
            vf -= s * vf.abs().min(7.0 * dt);
            self.braking = true;
        }
        if !self.airborne {
            let coast = if input.throttle == 0.0 && input.brake == 0.0 { 1.2 } else { 0.3 };
            let air = if input.throttle > 0.0 { 0.0 } else { vf.abs() * 0.012 };
            let s = sign(vf);
            vf -= s * vf.abs().min((coast + air) * dt);
        }
        self.feel_surface(world);
        if self.on_grass {
            let s = sign(vf);
            let drag = (0.5 + vf.abs() * 0.08) * if self.all_terrain { 0.5 } else { 1.0 };
            vf -= s * vf.abs().min(drag * dt);
        }

        let downforce_grip = 1.0 + (self.downforce / 10.0) * (vf / (100.0 * MPH)).powi(2);
        if !self.airborne {
            let g = if input.handbrake { self.grip * self.drift_grip } else { self.grip } * downforce_grip * self.surface_grip;
            vl *= (-g * dt).exp();
        }

        let mut yaw_rate = (vf / self.wheel_base) * self.steer_angle.tan() * control;
        if input.handbrake && vf.abs() > 3.0 {
            yaw_rate *= 1.5;
        }
        let max_yaw = (self.corner_grip(vf, &None) * if input.handbrake { DRIFT_TURN } else { 1.0 }) / vf.abs().max(1.0);
        yaw_rate = yaw_rate.clamp(-max_yaw, max_yaw);
        let d_yaw = yaw_rate * dt;
        self.heading += d_yaw;

        let carry = if input.handbrake { 0.25 } else { 0.85 };
        let mut vx = fx * vf + fz * vl;
        let mut vz = fz * vf - fx * vl;
        if !self.airborne {
            let a = d_yaw * carry;
            let (s, c) = a.sin_cos();
            (vx, vz) = (vx * c + vz * s, -vx * s + vz * c);
        }
        self.vx = vx;
        self.vz = vz;
        yaw_rate
    }

    /// Nitro and turbo: uses up the bottle (or the free turbo) and returns how strong the boost is.
    pub fn use_boost(&mut self, dt: f64, input: &CarInput, forward_speed: f64) -> f64 {
        let nitro_on = input.boost && self.nitro > 0.0;
        let turbo_on = self.turbo > 0.0;
        self.turbo = (self.turbo - dt).max(0.0);
        self.boosting = (nitro_on || turbo_on) && forward_speed > -0.5;
        if !self.boosting {
            return 0.0;
        }
        if nitro_on && !self.infinite_nitro {
            self.nitro = (self.nitro - self.nitro_burn * dt).max(0.0);
        }
        (if nitro_on { self.nitro_power } else { 0.0 }).max(if turbo_on { self.turbo_power } else { 0.0 })
    }

    /// Looks at the ground under the car: grass beside a track, and how grippy it is.
    pub fn feel_surface(&mut self, world: &World) {
        self.on_grass = !self.airborne && world.off_road(self.x, self.z);
        self.surface_grip = world.grip_at(self.x, self.z);
        if self.all_terrain {
            self.surface_grip = 1.0 - (1.0 - self.surface_grip) * 0.4;
        }
    }

    /// Pushes the car out of walls, buildings and posts. Returns speed lost.
    fn collide(&mut self, world: &mut World, events: &mut StepEvents, dynamics: &Dynamics) -> f64 {
        let fx = self.heading.sin();
        let fz = self.heading.cos();
        let r = self.length.min(self.width) / 2.0;
        let along_room = self.length / 2.0 - r;
        let across_room = self.width / 2.0 - r;
        let along_count = if along_room > 0.01 {
            ((self.length / (2.0 * r)).ceil() as usize + 1).max(3)
        } else {
            1
        };
        let across_count = if across_room > 0.01 {
            ((self.width / (2.0 * r)).ceil() as usize + 1).max(3)
        } else {
            1
        };
        let giant = self.height >= GIANT_HEIGHT;
        let direction = DVec3::new(self.vx, 0.0, self.vz).normalize_or_zero();
        let reach = self.length.max(self.width) / 2.0 + 1.0;
        let (near_circles, near_walls) = world.nearby(self.x, self.z, reach);
        let near_boxes = world.boxes_near(self.x, self.z, reach);
        let mut impact = 0.0;
        let mut hit = |lost: f64, target: Option<usize>, events: &mut StepEvents| {
            if lost > impact {
                impact = lost;
                events.hit_target = target;
            }
        };
        let spread = |k: usize, count: usize, room: f64| {
            if count == 1 {
                0.0
            } else {
                room * (-1.0 + (2.0 * k as f64) / (count - 1) as f64)
            }
        };
        for k in 0..along_count * across_count {
            let along = spread(k % along_count, along_count, along_room);
            let across = spread(k / along_count, across_count, across_room);
            let cx = self.x + fx * along + fz * across;
            let cz = self.z + fz * along - fx * across;
            for &bi in &near_boxes {
                let b = world.boxes[bi];
                if !b.solid {
                    continue;
                }
                if (b.target.is_some() || b.base != 0.0) && (self.y > b.base + b.height || self.y + self.height < b.base) {
                    continue;
                }
                let px = cx.clamp(b.min_x, b.max_x);
                let pz = cz.clamp(b.min_z, b.max_z);
                if (cx - px).powi(2) + (cz - pz).powi(2) >= r * r {
                    continue;
                }
                if let Some(t) = b.target
                    && giant
                    && self.height * 2.0 >= b.height
                {
                    world.damage(t, 1e6, direction);
                    events.smashed += 1;
                    continue;
                }
                // A wreck gets shoved along rather than stopping the car dead.
                if let Some(t) = b.target
                    && world.is_wreck(t)
                {
                    let (dx, dz) = (px - cx, pz - cz);
                    let d = dx.hypot(dz).max(1e-3);
                    let (nx, nz) = (dx / d, dz / d);
                    let toward = self.vx * nx + self.vz * nz;
                    if toward > 0.5 {
                        world.shove(t, DVec3::new(nx, 0.0, nz) * toward * 1.15);
                        self.vx -= nx * toward * 0.35;
                        self.vz -= nz * toward * 0.35;
                        // Out of the box, without the bounce of a wall.
                        self.x -= nx * (r - d).max(0.0);
                        self.z -= nz * (r - d).max(0.0);
                        continue;
                    }
                }
                let lost = self.resolve(cx, cz, px, pz, r, dynamics);
                hit(lost, b.target, events);
            }
            let reach = r + WALL_THICKNESS / 2.0;
            for &wi in &near_walls {
                let w = world.walls[wi];
                if self.y > w.top || (giant && w.top - self.y < self.height / 2.0) {
                    continue;
                }
                if cx + reach < w.min_x || cx - reach > w.max_x || cz + reach < w.min_z || cz - reach > w.max_z {
                    continue;
                }
                let sx = w.bx - w.ax;
                let sz = w.bz - w.az;
                let len2 = sx * sx + sz * sz;
                let t = (((cx - w.ax) * sx + (cz - w.az) * sz) / if len2 == 0.0 { 1.0 } else { len2 }).clamp(0.0, 1.0);
                let px = w.ax + sx * t;
                let pz = w.az + sz * t;
                if (cx - px).powi(2) + (cz - pz).powi(2) >= reach * reach {
                    continue;
                }
                let lost = self.resolve(cx, cz, px, pz, reach, dynamics);
                hit(lost, None, events);
            }
            for &ci in &near_circles {
                let c = world.circles[ci];
                if !c.solid || self.y > c.base + c.height || self.y + self.height.max(1.0) < c.base {
                    continue;
                }
                let dx = cx - c.x;
                let dz = cz - c.z;
                let reach = r + c.r;
                if dx * dx + dz * dz >= reach * reach {
                    continue;
                }
                if let Some(t) = c.target
                    && world.ram(t, if giant { f64::INFINITY } else { self.speed() }, direction)
                {
                    if !giant {
                        self.vx *= 0.9;
                        self.vz *= 0.9;
                    }
                    events.smashed += 1;
                    continue;
                }
                if giant && c.height < self.height / 2.0 {
                    continue;
                }
                let d = dx.hypot(dz);
                let d = if d == 0.0 { 0.001 } else { d };
                let lost = self.resolve(cx, cz, c.x + (dx / d) * c.r, c.z + (dz / d) * c.r, r, dynamics);
                hit(lost, c.target, events);
            }
        }
        impact
    }

    fn resolve(&mut self, cx: f64, cz: f64, px: f64, pz: f64, r: f64, dynamics: &Dynamics) -> f64 {
        let mut dx = cx - px;
        let mut dz = cz - pz;
        let dist_sq = dx * dx + dz * dz;
        if dist_sq >= r * r {
            return 0.0;
        }
        let mut d = dist_sq.sqrt();
        if d < 1e-4 {
            let s = self.vx.hypot(self.vz);
            let s = if s == 0.0 { 1.0 } else { s };
            dx = -self.vx / s;
            dz = -self.vz / s;
            d = 0.0;
        } else {
            dx /= d;
            dz /= d;
        }
        let push = r - d;
        self.x += dx * push;
        self.z += dz * push;
        let vn = self.vx * dx + self.vz * dz;
        if vn >= 0.0 {
            return 0.0;
        }
        self.vx -= vn * dx * (1.0 + self.bounce);
        self.vz -= vn * dz * (1.0 + self.bounce);
        self.vx *= 0.85;
        self.vz *= 0.85;
        if let Some(dyn_model) = dynamics {
            let fx = self.heading.sin();
            let fz = self.heading.cos();
            let along = (cx - self.x) * fx + (cz - self.z) * fz;
            let across = (cx - self.x) * fz - (cz - self.z) * fx;
            dyn_model.impact(self, dx, dz, along, across, -vn);
        }
        -vn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CarConfig;

    fn car() -> CarPhysics {
        let mut p = CarPhysics::new();
        p.configure(
            &CarConfig::default(),
            Dims {
                length: 4.4,
                width: 1.9,
                height: 1.3,
                wheel_base: 2.6,
            },
        );
        p
    }

    /// The stock car on flat ground, driven the same way as the 1.2.x game
    /// was measured: flat out, a full-lock turn, braking, then a boosted drift.
    #[test]
    fn drives_exactly_like_the_old_game() {
        let mut world = World::empty();
        let mut p = car();
        let dt = 1.0 / 120.0;
        let full = CarInput {
            throttle: 1.0,
            ..Default::default()
        };
        let mut t = 0.0;
        while p.speed() < 60.0 * MPH {
            p.step(dt, &full, &mut world, &None);
            t += dt;
        }
        assert!((t - 3.116_666_666_666_658).abs() < 1e-9, "0-60 took {t}");
        for _ in 0..(120 * 60) {
            p.step(dt, &full, &mut world, &None);
        }
        assert!(
            (p.speed() / MPH - 108.337_435_817_578_96).abs() < 1e-6,
            "top speed {}",
            p.speed() / MPH
        );
        for _ in 0..360 {
            p.step(dt, &CarInput { steer: 1.0, ..full }, &mut world, &None);
        }
        let close = |a: f64, b: f64| (a - b).abs() < 1e-5;
        assert!(close(p.x, 83.634_987) && close(p.z, 2994.549_154) && close(p.heading, 1.362_321) && close(p.speed(), 48.378_898));
        let mut steps = 0;
        while p.speed() > 0.5 {
            p.step(
                dt,
                &CarInput {
                    brake: 1.0,
                    ..Default::default()
                },
                &mut world,
                &None,
            );
            steps += 1;
        }
        assert_eq!(steps, 255);
        assert!(close(p.x, 134.707_551) && close(p.z, 2999.679_605));
        for i in 0..240 {
            p.step(
                dt,
                &CarInput {
                    boost: true,
                    handbrake: i > 120,
                    steer: -1.0,
                    ..full
                },
                &mut world,
                &None,
            );
        }
        assert!(close(p.x, 137.695_510) && close(p.z, 3035.058_133) && close(p.heading, -2.211_295) && close(p.nitro, 0.428_571));
    }
}
