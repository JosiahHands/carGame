//! The realistic driving model: four tires on a car body that rides on real
//! suspension. It takes the place of the game's arcade model for every car,
//! the player's and the rivals'.
//!
//! Each step (four times per physics step: 480 times a second):
//!   1. The suspension: the body rises, dips and leans on four springs and
//!      dampers, with anti-roll bars. The springs' push is the weight on each tire.
//!   2. The engine and gearbox: a torque curve for each kind of engine, turbo
//!      lag, a slipping clutch when pulling away, a moment with the power cut
//!      while changing gear, and engine braking. The differential shares the
//!      push between the driven wheels.
//!   3. Each tire: how fast its contact patch slides along the road and
//!      across it, and the force from the "Magic Formula" curve real tire
//!      models use, shared between braking and cornering, growing with the
//!      weight on the tire, and depending on its temperature and wear.
//!   4. Air drag and downforce, then the driving aids, and the car moves.
//!
//! Numbers are per kilogram of car (forces are accelerations), so a toy car
//! and a 100 ft monster drive by the same rules.
use std::sync::RwLock;

use game::physics::{CarDynamics, CarInput, CarPhysics, GEAR_TOPS, MAX_REVERSE};
use game::venues::Weather;
use game::world::World;

use crate::powertrain::{Engine, auto_gear, differential, engine_for, spool_turbo, torque_at, turbo_share};
use crate::suspension::{FRONT, LEFT, SuspensionSpecs, SuspensionState, relax_suspension, roughness, step_suspension, suspension_specs};
use crate::tires::{PEAK, PEAK_SLIP, ambient_temperature, best_temperature, heat_tire, magic, temperature_grip, wear_grip};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assists {
    Full,
    Some,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Realism {
    Friendly,
    Real,
    Sim,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelOptions {
    /// Tire grip: 0.95 road, 1.2 sporty, 1.45 a racing slick.
    pub grip: f64,
    pub assists: Assists,
    pub realism: Realism,
}

#[derive(Clone, Copy, Debug)]
struct Rules {
    tire_heat: bool,
    start_below_best: Option<f64>,
    temp_floor: f64,
    wear: f64,
    shift: f64,
    turbo_lag: bool,
    damage: f64,
    brake_fade: bool,
    road: f64,
}

const FRIENDLY: Rules = Rules {
    tire_heat: false,
    start_below_best: Some(0.0),
    temp_floor: 1.0,
    wear: 0.0,
    shift: 0.0,
    turbo_lag: false,
    damage: 0.0,
    brake_fade: false,
    road: 0.5,
};
const REAL: Rules = Rules {
    tire_heat: true,
    start_below_best: Some(20.0),
    temp_floor: 0.88,
    wear: 0.5,
    shift: 0.15,
    turbo_lag: true,
    damage: 1.0,
    brake_fade: true,
    road: 1.0,
};
const SIM: Rules = Rules {
    tire_heat: true,
    start_below_best: None,
    temp_floor: 0.8,
    wear: 1.0,
    shift: 0.2,
    turbo_lag: true,
    damage: 1.6,
    brake_fade: true,
    road: 1.25,
};
const RIVAL: Rules = Rules {
    tire_heat: false,
    start_below_best: Some(0.0),
    temp_floor: 1.0,
    wear: 0.0,
    shift: 0.15,
    turbo_lag: true,
    damage: 0.0,
    brake_fade: false,
    road: 1.0,
};

/// The weight the tires feel: low-gravity cars keep Earth grip, or they could hardly pull away.
fn tire_gravity(car: &CarPhysics) -> f64 {
    car.gravity.max(9.81)
}

#[derive(Clone, Debug, Default)]
pub struct Damage {
    pub engine: f64,
    pub toe: f64,
    pub wheel: [f64; 4],
}

/// What each car keeps between steps.
#[derive(Clone, Debug)]
pub struct CarState {
    pub yaw_rate: f64,
    pub spin: [f64; 4],
    pub slip_angle: [f64; 4],
    pub ax: f64,
    pub ay: f64,
    pub susp: SuspensionState,
    pub temp: [f64; 4],
    pub wear: [f64; 4],
    pub grip_factor: f64,
    pub boost: f64,
    pub engine_revs: f64,
    pub shift_cut: f64,
    pub last_gear: usize,
    pub since_shift: f64,
    pub brake_temp: f64,
    pub fade: bool,
    pub damage: Damage,
    pub air_vy: f64,
    pub was_airborne: bool,
    pub jolt: f64,
    pub load: [f64; 4],
    pub usage: [f64; 4],
    pub slip_ratio: [f64; 4],
    pub traction_control: bool,
    pub abs: bool,
    pub stability: bool,
    pub rpm: f64,
    /// Dents waiting to be pressed into the player's car: (along, across, nx, nz, lost).
    pub dents: Vec<[f64; 5]>,
    specs: Option<(String, Specs)>,
}

impl Default for CarState {
    fn default() -> Self {
        CarState {
            yaw_rate: 0.0,
            spin: [0.0; 4],
            slip_angle: [0.0; 4],
            ax: 0.0,
            ay: 0.0,
            susp: SuspensionState::default(),
            temp: [f64::NAN; 4],
            wear: [0.0; 4],
            grip_factor: 1.0,
            boost: 0.0,
            engine_revs: 0.0,
            shift_cut: 0.0,
            last_gear: 1,
            since_shift: 9.0,
            brake_temp: f64::NAN,
            fade: false,
            damage: Damage::default(),
            air_vy: 0.0,
            was_airborne: false,
            jolt: 0.0,
            load: [0.0; 4],
            usage: [0.0; 4],
            slip_ratio: [0.0; 4],
            traction_control: false,
            abs: false,
            stability: false,
            rpm: 0.0,
            dents: Vec::new(),
            specs: None,
        }
    }
}

/// The model's state for a car (made on first use).
pub fn state_of(car: &mut CarPhysics) -> &mut CarState {
    if car.dynamics_state.as_ref().is_none_or(|s| !s.is::<CarState>()) {
        car.dynamics_state = Some(Box::new(CarState::default()));
    }
    car.dynamics_state
        .as_mut()
        .and_then(|s| s.downcast_mut::<CarState>())
        .expect("car state")
}

pub fn peek_state(car: &CarPhysics) -> Option<&CarState> {
    car.dynamics_state.as_ref().and_then(|s| s.downcast_ref::<CarState>())
}

/// The car's fixed numbers, from its garage settings and size.
#[derive(Clone, Debug)]
pub struct Specs {
    pub radius: f64,
    pub front: f64,
    pub rear: f64,
    pub half_track: f64,
    pub com_height: f64,
    pub yaw_inertia: f64,
    pub wheel_inertia: f64,
    pub power: f64,
    pub drag: f64,
    pub front_drive: f64,
    pub lsd_front: f64,
    pub lsd_rear: f64,
    pub brake: f64,
    pub max_steer: f64,
    pub engine: Engine,
    pub suspension: SuspensionSpecs,
    pub size: f64,
    pub wheel_x: [f64; 4],
    pub wheel_y: [f64; 4],
}

const ROLLING: f64 = 0.013;
const GRASS_ROLLING: f64 = 0.09;
const DRIVELINE: f64 = 0.88;
const AERO_FRONT: f64 = 0.45;
const SHOW_LEAN: f64 = 2.0;
pub const CORNER_MARGIN: f64 = 0.82;

pub fn specs_of(car: &CarPhysics) -> Specs {
    let size = ((car.length * car.height) / (4.4 * 1.3)).sqrt();
    let wheel_size = car.config.as_ref().map_or(1.0, |c| c.wheel_size);
    let radius = 0.33 * size.max(0.15) * wheel_size.sqrt();
    let half = car.wheel_base / 2.0;
    let power = car.accel * 20.0;
    let top = car.top_speed;
    let at_top = power * DRIVELINE * 0.95;
    let drag = ((at_top / top - ROLLING * car.gravity) / (top * top)).max(1e-7);
    let body = car.config.as_ref().map_or("", |c| c.body.as_str());
    let four_wheel = body == "pickup" || body == "van" || car.all_terrain;
    let front_drive = if body == "hatch" {
        1.0
    } else if four_wheel {
        0.4
    } else {
        0.0
    };
    let drift = car.config.as_ref().map_or(5.0, |c| c.tuning.drift);
    let lsd_rear = (if four_wheel { 0.6 } else { 0.4 } + (drift - 5.0).max(0.0) * 0.08).min(1.0);
    let lsd_front = if four_wheel { 0.3 } else { 0.0 };
    let frequency = (if car.all_terrain {
        1.3
    } else {
        match body {
            "wedge" => 2.4,
            "sports" => 2.1,
            "hatch" => 1.9,
            "muscle" => 1.7,
            "pickup" => 1.4,
            "van" => 1.3,
            _ => 1.8,
        }
    }) + car.downforce * 0.08;
    let half_track = (car.width / 2.0) * 0.86;
    Specs {
        radius,
        front: half,
        rear: half,
        half_track,
        com_height: car.height * 0.38,
        yaw_inertia: (car.length.powi(2) + car.width.powi(2)) / 12.0,
        wheel_inertia: 0.004 * radius * radius * 8.0,
        power,
        drag,
        front_drive,
        lsd_front,
        lsd_rear,
        brake: car.brake_decel * 1.4,
        max_steer: car.max_steer,
        engine: engine_for(body),
        suspension: suspension_specs(
            frequency,
            size,
            tire_gravity(car),
            car.wheel_base,
            half_track,
            (drift - 5.0).max(0.0) * 0.06,
        ),
        size,
        wheel_x: FRONT.map(|f| if f { half } else { -half }),
        wheel_y: LEFT.map(|l| if l { half_track } else { -half_track }),
    }
}

fn specs_key(car: &CarPhysics) -> String {
    let c = car.config.as_ref();
    format!(
        "{}/{}/{}/{}/{}/{}/{}/{}/{:?}/{}/{:?}/{}/{}/{:?}",
        car.accel,
        car.top_speed,
        car.length,
        car.width,
        car.height,
        car.wheel_base,
        car.brake_decel,
        car.max_steer,
        c.map(|c| &c.body),
        car.all_terrain,
        c.map(|c| c.wheel_size),
        car.gravity,
        car.downforce,
        c.map(|c| c.tuning.drift)
    )
}

fn specs_for(car: &mut CarPhysics) -> Specs {
    let key = specs_key(car);
    if let Some((k, s)) = &state_of(car).specs
        && *k == key
    {
        return s.clone();
    }
    let specs = specs_of(car);
    state_of(car).specs = Some((key, specs.clone()));
    specs
}

/// The realistic model, shared by every car.
pub struct RealisticDynamics {
    pub options: RwLock<ModelOptions>,
}

impl RealisticDynamics {
    pub fn new(options: ModelOptions) -> Self {
        RealisticDynamics {
            options: RwLock::new(options),
        }
    }

    fn opts(&self) -> ModelOptions {
        *self.options.read().expect("options")
    }

    fn rules_for(&self, car: &CarPhysics) -> Rules {
        if !car.player {
            return RIVAL;
        }
        match self.opts().realism {
            Realism::Friendly => FRIENDLY,
            Realism::Real => REAL,
            Realism::Sim => SIM,
        }
    }

    /// Back from the garage: crash damage fixed, fresh tires, cool brakes.
    pub fn repair(car: &mut CarPhysics) {
        let s = state_of(car);
        s.damage = Damage::default();
        s.wear = [0.0; 4];
        s.temp = [f64::NAN; 4];
        s.brake_temp = f64::NAN;
    }

    fn step(&self, car: &mut CarPhysics, dt: f64, input: &CarInput, world: &World) {
        let sp = specs_for(car);
        let rules = self.rules_for(car);
        let opts = self.opts();
        let assists = if car.player { opts.assists } else { Assists::Full };
        let help = match assists {
            Assists::Full => 1.0,
            Assists::Some => 0.5,
            Assists::Off => 0.0,
        };
        car.feel_surface(world);
        let (fx, fz) = (car.heading.sin(), car.heading.cos());
        let mut u = car.vx * fx + car.vz * fz;
        let mut v = car.vx * fz - car.vz * fx;
        let mut r = state_of(car).yaw_rate;
        let speed = u.hypot(v);
        {
            let s = state_of(car);
            if u.abs() > 2.0 && s.spin.iter().all(|w| *w == 0.0) {
                s.spin = [u / sp.radius; 4];
            }
        }
        let weather = world.weather_kind();
        let ambient = ambient_temperature(weather, world.is_night());
        let best = best_temperature(opts.grip);
        {
            let s = state_of(car);
            if s.temp[0].is_nan() {
                s.temp = [rules.start_below_best.map_or(ambient, |b| best - b); 4];
            }
            if s.brake_temp.is_nan() {
                s.brake_temp = ambient + 40.0;
            }
        }

        // ---------- steering ----------
        let sideslip = if speed > 3.0 { v.atan2(u.abs()) } else { 0.0 };
        let lock = sp.max_steer / (1.0 + u.abs() / 28.0);
        let mut target = input.steer * lock;
        let counter = input.steer.signum() == sideslip.signum() && input.steer != 0.0 && sideslip.abs() > 0.05;
        if counter {
            target = input.steer * lock.max(sp.max_steer.min(sideslip.abs() * 1.2));
        }
        target += sideslip * 0.5 * help * if u > 0.0 { 1.0 } else { 0.0 };
        target = target.clamp(-sp.max_steer, sp.max_steer);
        car.steer_angle += (target - car.steer_angle) * (dt * 12.0).min(1.0);
        let delta = car.steer_angle + state_of(car).damage.toe;

        if car.airborne {
            let control = car.air_control;
            relax_suspension(&mut state_of(car).susp, dt);
            r += (input.steer * 2.5 * control - r) * (dt * 2.0).min(1.0);
            if !car.manual {
                car.gear = 1;
                while car.gear < GEAR_TOPS.len() && u.abs() > GEAR_TOPS[car.gear - 1] * car.top_speed {
                    car.gear += 1;
                }
            }
            car.revs = if u < 0.0 {
                -u / MAX_REVERSE
            } else {
                u / (GEAR_TOPS[car.gear - 1] * car.top_speed)
            };
            let revs = car.revs;
            let gear = car.gear;
            {
                let s = state_of(car);
                s.last_gear = gear;
                s.rpm = revs.abs().min(1.2);
                s.engine_revs = revs.abs();
            }
            let mut pull = 1.0;
            if car.manual {
                if car.revs >= 1.0 {
                    pull = 0.0;
                } else if car.gear > 1 {
                    pull = 0.12 + 0.88 * (car.revs / 0.53).min(1.0).powi(2);
                }
            }
            car.limiter = car.manual && car.revs >= 1.0 && input.throttle > 0.0;
            car.braking = false;
            if input.throttle > 0.0 {
                if u < -0.5 {
                    u = (u + car.brake_decel * input.throttle * dt * control).min(0.0);
                    car.braking = true;
                } else {
                    u += car.accel
                        * input.throttle
                        * (1.0 - (u / (car.top_speed * (1.0 + 0.08 * car.draft))).powi(2)).max(0.0)
                        * pull
                        * dt
                        * control;
                }
            }
            let power = car.use_boost(dt, input, u);
            if power > 0.0 {
                u +=
                    car.accel * 1.6 * power * (1.0 - (u / (car.top_speed * (1.0 + 0.35 * power))).powi(2)).max(0.0) * dt * control.max(0.4);
            }
            if input.brake > 0.0 {
                if u > 0.5 {
                    u = (u - car.brake_decel * input.brake * dt * control).max(0.0);
                    car.braking = true;
                } else if u > -MAX_REVERSE {
                    u -= car.accel * 0.6 * input.brake * dt * control;
                }
            }
            let s = state_of(car);
            s.yaw_rate = r;
            for w in &mut s.spin {
                *w += (u / sp.radius - *w) * (dt * 2.0).min(1.0);
            }
            s.load = [0.0; 4];
            set_velocity(car, u, v, fx, fz, dt);
            return;
        }

        {
            let vy = car.vy;
            let s = state_of(car);
            if s.was_airborne {
                s.was_airborne = false;
                let fall = s.air_vy.min(vy).min(0.0);
                s.susp.heave_rate += (fall * 0.85).max(-4.0);
                s.jolt = s.jolt.max((-fall / 12.0).min(1.0));
                s.air_vy = 0.0;
            }
        }

        // ---------- the gearbox and engine ----------
        let driven: Vec<usize> = (0..4)
            .filter(|&i| if FRONT[i] { sp.front_drive > 0.0 } else { sp.front_drive < 1.0 })
            .collect();
        let wheel_speed = driven.iter().map(|&i| state_of(car).spin[i]).sum::<f64>() / driven.len() as f64 * sp.radius;
        let tops: Vec<f64> = GEAR_TOPS.iter().map(|t| t * car.top_speed).collect();
        state_of(car).since_shift += dt;
        if !car.manual && state_of(car).since_shift > 0.5 {
            car.gear = auto_gear(car.gear, u.abs() * 1.06, &tops);
        }
        let (gear, manual) = (car.gear, car.manual);
        {
            let s = state_of(car);
            if gear != s.last_gear {
                s.since_shift = 0.0;
                if gear > s.last_gear && rules.shift > 0.0 {
                    s.shift_cut = rules.shift * if manual { 0.7 } else { 1.0 };
                }
                s.last_gear = gear;
            }
            s.shift_cut = (s.shift_cut - dt).max(0.0);
        }
        let reversing = input.brake > 0.0 && u < 0.5 && input.throttle == 0.0;
        let gear_top = tops[car.gear - 1];
        let boost = car.use_boost(dt, input, u);
        let redline_speed = gear_top * if boost > 0.0 { 1.0 + 0.35 * boost } else { 1.0 };
        let wheel_revs = wheel_speed.abs() / gear_top;
        let slipping = car.gear == 1 && wheel_revs < 0.35 && input.throttle > 0.0 && !reversing;
        let engine_target = if slipping {
            wheel_revs.max(0.22 + 0.33 * input.throttle)
        } else {
            wheel_revs
        };
        let (engine_revs, turbo_boost, damage_engine, shift_cut) = {
            let s = state_of(car);
            s.engine_revs += (engine_target - s.engine_revs) * (dt * if slipping { 8.0 } else { 25.0 }).min(1.0);
            s.rpm = s.engine_revs.min(1.2);
            s.boost = spool_turbo(s.boost, input.throttle, s.engine_revs, dt, rules.turbo_lag);
            (s.engine_revs, s.boost, s.damage.engine, s.shift_cut)
        };
        car.revs = if reversing { u.abs() / 12.0 } else { engine_revs };
        car.limiter = car.manual && car.revs >= 1.0 && input.throttle > 0.0;
        let max_force = ((sp.power * DRIVELINE) / gear_top.max(4.0) / 0.95) * (1.0 - damage_engine);
        let torque = torque_at(&sp.engine, engine_revs.max(0.12)) * turbo_share(&sp.engine, turbo_boost);
        let mut throttle = input.throttle;
        if wheel_speed.abs() >= redline_speed && !reversing {
            throttle = 0.0;
        }
        if car.manual && car.gear > 1 && car.revs < 0.53 {
            throttle *= 0.12 + 0.88 * (car.revs / 0.53).powi(2);
        }
        let mut drive_force = throttle * max_force * torque * (1.0 + 0.6 * boost);
        if boost > 0.0 && throttle == 0.0 {
            drive_force = max_force * 0.6 * boost;
        }
        if shift_cut > 0.0 {
            drive_force = 0.0;
        } else if input.throttle == 0.0 && boost == 0.0 && !reversing && u > 1.0 && input.brake == 0.0 {
            drive_force = -max_force * (0.06 + 0.12 * wheel_revs.min(1.0));
        }
        if reversing {
            drive_force = -input.brake * max_force * 0.6 * if u.abs() < 12.0 { 1.0 } else { 0.0 };
        }
        let throttle_brakes = input.throttle > 0.0 && u < -0.5;
        if throttle_brakes {
            drive_force = 0.0;
        }

        // ---------- the suspension: weight on each wheel ----------
        let g0 = tire_gravity(car);
        let down = 1.0 + (car.downforce / 10.0) * (u / 44.7).powi(2);
        let g = g0 * down;
        let wheel_base = sp.front + sp.rear;
        let rough_amount = rules.road
            * if car.on_grass {
                0.03
            } else if car.surface_grip < 0.85 {
                0.012
            } else {
                0.003
            };
        let mut road = [0.0; 4];
        let limit = sp.suspension.sag + sp.suspension.travel;
        for i in 0..4 {
            let (x, y) = (sp.wheel_x[i], sp.wheel_y[i]);
            let wx = car.x + fx * x + fz * y;
            let wz = car.z + fz * x - fx * y;
            let plane = car.y - car.pitch * x + car.ground_roll * y;
            let bump = world.height_at(wx, wz) - plane + roughness(wx, wz, rough_amount);
            road[i] = bump.clamp(-limit, limit * 1.5);
        }
        let extra = g - g0;
        let (ax, ay) = (state_of(car).ax, state_of(car).ay);
        step_suspension(
            &mut state_of(car).susp,
            &sp.suspension,
            dt,
            &road,
            &sp.wheel_x,
            &sp.wheel_y,
            g,
            extra * (AERO_FRONT * sp.front - (1.0 - AERO_FRONT) * sp.rear),
            ax,
            ay,
            sp.com_height,
        );
        let loads = state_of(car).susp.load;

        // ---------- surface ----------
        let grassy = car.on_grass;
        let mut mu = opts.grip * car.grip_multiplier * car.surface_grip;
        if weather == Weather::Rain {
            mu *= (1.0 - 0.35 * ((speed - 25.0) / 40.0).max(0.0).powi(2)).max(0.55);
        }
        let drift = car.config.as_ref().map_or(0.0, |c| c.tuning.drift);
        let rear_grip = 1.1 - drift * 0.02;

        // ---------- the brakes ----------
        let handbrake = input.handbrake;
        let braking = input.brake > 0.0 && u > 0.5;
        let throttle_brake = if throttle_brakes { input.throttle } else { 0.0 };
        car.braking = braking || throttle_brakes || handbrake;
        let s = state_of(car);
        let fade = if rules.brake_fade {
            1.0 - 0.6 * ((s.brake_temp - 450.0) / 250.0).clamp(0.0, 1.0)
        } else {
            1.0
        };
        s.fade = fade < 0.97;

        // ---------- the differentials ----------
        let mut push = [0.0; 4];
        for (l, rt, share, lsd) in [(0, 1, sp.front_drive, sp.lsd_front), (2, 3, 1.0 - sp.front_drive, sp.lsd_rear)] {
            if share <= 0.0 {
                continue;
            }
            let (a, b) = differential(drive_force * share, lsd, s.spin[l] * sp.radius, s.spin[rt] * sp.radius, u);
            push[l] = a;
            push[rt] = b;
        }

        // ---------- each tire ----------
        let (mut fx_sum, mut fy_sum, mut moment, mut brake_power, mut grip_sum) = (0.0, 0.0, 0.0, 0.0, 0.0);
        s.traction_control = false;
        s.abs = false;
        let floor = (sp.radius * 3.0).max(0.5);
        let static_load = g / 4.0;
        for i in 0..4 {
            let (x, y) = (sp.wheel_x[i], sp.wheel_y[i]);
            let pu = u - r * y;
            let pv = v + r * x;
            let steer = if FRONT[i] { delta } else { 0.0 };
            let (sin, cos) = steer.sin_cos();
            let wu = pu * cos + pv * sin;
            let wv = -pu * sin + pv * cos;
            let reference = wu.abs().max(floor);
            let angle_target = wv.atan2(reference);
            let relax = ((wu.abs().max(2.0) * dt) / sp.radius.max(0.2)).min(1.0);
            s.slip_angle[i] += (angle_target - s.slip_angle[i]) * relax;
            let n = loads[i];
            let sensitivity = (1.0 - 0.12 * (n / static_load.max(1e-6) - 1.0)).max(0.7);
            let condition = if rules.tire_heat {
                temperature_grip(s.temp[i], best, rules.temp_floor) * wear_grip(s.wear[i])
            } else {
                1.0
            };
            grip_sum += condition;
            let camber = 1.0 - (s.susp.roll.abs() * 0.6).min(0.05);
            let peak_force = mu
                * n
                * sensitivity
                * condition
                * camber
                * (1.0 - s.damage.wheel[i] * 0.5)
                * if grassy { 0.8 } else { 1.0 }
                * if FRONT[i] { 1.0 } else { rear_grip };
            let mut drive = push[i];
            let mut brake = 0.0;
            let share = if FRONT[i] { 0.3 } else { 0.2 };
            if braking {
                brake = sp.brake * input.brake * share * fade;
            }
            if throttle_brakes {
                brake = sp.brake * throttle_brake * share * fade;
            }
            if handbrake && !FRONT[i] {
                brake = brake.max(sp.brake * 0.6);
            }
            let angle = s.slip_angle[i];
            let tire = |spin: f64| {
                let ratio = ((spin * sp.radius - wu) / reference).clamp(-1.5, 1.5);
                let sx = ratio / PEAK_SLIP;
                let sy = angle.clamp(-1.4, 1.4).tan() / PEAK_SLIP;
                let combined = sx.hypot(sy);
                if combined < 1e-9 || peak_force <= 0.0 {
                    return (0.0, 0.0, ratio, 0.0);
                }
                let force = peak_force * magic(combined * PEAK_SLIP) / PEAK;
                (force * sx / combined, -force * sy / combined, ratio, combined)
            };
            let now = tire(s.spin[i]);
            if help > 0.0 && drive > 0.0 && (now.2 > 0.2 + (1.0 - help) * 0.25 || now.3 > 1.05 + (1.0 - help) * 0.3) {
                drive *= 0.5;
                s.traction_control = true;
            }
            if help > 0.0 && brake > 0.0 && !handbrake && now.2 < -0.12 - (1.0 - help) * 0.2 {
                brake *= 0.4;
                s.abs = true;
            }
            let inertia = sp.wheel_inertia;
            let h = 1e-3;
            let slope = (tire(s.spin[i] + h).0 - now.0) / h;
            let free = s.spin[i] + dt * (drive - now.0) * sp.radius / inertia;
            let mut spin = s.spin[i] + (free - s.spin[i]) / (1.0 + dt * sp.radius * slope.max(0.0) / inertia);
            if brake > 0.0 {
                let slowing = dt * brake * sp.radius / inertia;
                spin = if spin.abs() <= slowing {
                    0.0
                } else {
                    spin - spin.signum() * slowing
                };
                brake_power += brake * (spin * sp.radius).abs();
            }
            s.spin[i] = spin;
            let out = tire(spin);
            s.slip_ratio[i] = out.2;
            s.usage[i] = out.3;
            s.load[i] = n / static_load.max(1e-6);
            if rules.tire_heat {
                let sliding = (out.0 * (spin * sp.radius - wu)).abs() + (out.1 * wv).abs();
                let rolling = 0.012 * (n / static_load.max(1e-6)) * wu.abs();
                s.temp[i] = heat_tire(s.temp[i], sliding + rolling, speed, ambient, dt);
                s.wear[i] = (s.wear[i] + sliding * 2.5e-5 * rules.wear * dt).min(1.0);
            }
            let mut tfx = out.0 * cos - out.1 * sin;
            let mut tfy = out.0 * sin + out.1 * cos;
            let rolling = if grassy { GRASS_ROLLING } else { ROLLING } * n;
            tfx -= pu.signum() * (pu.abs() / dt / 4.0).min(rolling) * if pu == 0.0 { 0.0 } else { 1.0 };
            let max_lat = pv.abs() / dt / 4.0 + peak_force * 0.02;
            tfy = tfy.clamp(-max_lat, max_lat);
            fx_sum += tfx;
            fy_sum += tfy;
            moment += x * tfy - y * tfx;
        }
        s.grip_factor = grip_sum / 4.0;
        s.brake_temp += (0.1 * brake_power - 0.02 * (s.brake_temp - ambient) * (1.0 + speed / 30.0)) * dt;
        let air = sp.drag * speed * (1.0 - 0.22 * car.draft);
        fx_sum -= air * u;
        fy_sum -= air * v;

        // ---------- stability control ----------
        let s = state_of(car);
        s.stability = false;
        if help > 0.0 && !handbrake && u > 5.0 {
            let wanted = u * delta.tan() / wheel_base;
            let most = mu * g / u.max(1.0);
            let aim = wanted.clamp(-most, most);
            let spinning = r.abs() > aim.abs() + 0.1 * (1.6 - help) || (r.signum() != aim.signum() && r.abs() > 0.15);
            if spinning && sideslip.abs() > 0.06 {
                moment -= (r - aim) * sp.yaw_inertia * 5.0 * help;
                s.stability = true;
            }
        }

        // ---------- move ----------
        u += fx_sum * dt;
        v += fy_sum * dt;
        r += moment / sp.yaw_inertia * dt;
        if speed < 0.3 && input.throttle == 0.0 && input.brake == 0.0 && boost == 0.0 {
            u *= 0.9;
            v *= 0.9;
            r *= 0.9;
        }
        s.yaw_rate = r;
        let settle = (dt * 30.0).min(1.0);
        s.ax += (fx_sum - s.ax) * settle;
        s.ay += (fy_sum - s.ay) * settle;
        set_velocity(car, u, v, fx, fz, dt);
    }
}

fn set_velocity(car: &mut CarPhysics, u: f64, v: f64, fx: f64, fz: f64, dt: f64) {
    car.vx = fx * u + fz * v;
    car.vz = fz * u - fx * v;
    car.heading += state_of(car).yaw_rate * dt;
}

impl CarDynamics for RealisticDynamics {
    fn drive(&self, car: &mut CarPhysics, dt: f64, input: &CarInput, world: &World) -> f64 {
        let steps = 4;
        if car.airborne {
            let vy = car.vy;
            let s = state_of(car);
            s.was_airborne = true;
            s.air_vy = s.air_vy.min(vy);
        }
        for _ in 0..steps {
            self.step(car, dt / steps as f64, input, world);
        }
        let s = state_of(car);
        s.jolt *= (1.0 - dt * 4.0).max(0.0);
        s.yaw_rate
    }

    fn corner_grip(&self, car: &CarPhysics, speed: f64, surface_grip: f64) -> f64 {
        let mu = self.opts().grip * car.grip_multiplier * surface_grip;
        let down = 1.0 + (car.downforce / 10.0) * (speed / 44.7).powi(2);
        let tires = peek_state(car).map_or(1.0, |s| s.grip_factor);
        mu * tire_gravity(car) * down * tires * CORNER_MARGIN
    }

    fn impact(&self, car: &mut CarPhysics, nx: f64, nz: f64, along: f64, across: f64, lost: f64) {
        let sp = specs_for(car);
        let rules = self.rules_for(car);
        let (fx, fz) = (car.heading.sin(), car.heading.cos());
        let (length, width, player) = (car.length, car.width, car.player);
        let s = state_of(car);
        let pf = (nx * fx + nz * fz) * lost;
        let pl = (nx * fz - nz * fx) * lost;
        s.yaw_rate += ((along * pl - across * pf) / sp.yaw_inertia) * 0.5;
        s.yaw_rate = s.yaw_rate.clamp(-8.0, 8.0);
        s.susp.roll_rate -= pl * 0.04;
        s.susp.pitch_rate -= pf * 0.03;
        s.jolt = s.jolt.max((lost / 18.0).min(1.0));
        if player && lost > 4.0 {
            s.dents.push([along, across, nx, nz, lost]);
        }
        if rules.damage <= 0.0 || lost < 3.0 {
            return;
        }
        let hurt = (lost - 3.0) * rules.damage;
        let d = &mut s.damage;
        let front = along > length * 0.15;
        let left_side = across > 0.0;
        if front {
            d.engine = (d.engine + hurt / 160.0).min(0.6);
        }
        let wheel = (if front { 0 } else { 2 }) + usize::from(!left_side);
        if along.abs() > length * 0.15 || across.abs() > width * 0.3 {
            d.wheel[wheel] = (d.wheel[wheel] + hurt / 220.0).min(0.4);
            if front {
                d.toe = (d.toe + if left_side { -1.0 } else { 1.0 } * hurt * 0.0006).clamp(-0.035, 0.035);
            }
        }
    }

    fn reset(&self, car: &mut CarPhysics, fresh: bool) {
        let old = car.dynamics_state.take().and_then(|s| s.downcast::<CarState>().ok());
        let mut s = CarState::default();
        if let Some(old) = old
            && !fresh
        {
            s.damage = old.damage;
            s.temp = old.temp;
            s.wear = old.wear;
            s.brake_temp = old.brake_temp;
        }
        car.dynamics_state = Some(Box::new(s));
    }

    fn pose(&self, car: &mut CarPhysics, _dt: f64) {
        if car.airborne {
            return;
        }
        let (roll, pitch) = {
            let s = &state_of(car).susp;
            (s.roll, s.pitch)
        };
        car.roll = roll * SHOW_LEAN;
        car.body_pitch = pitch * SHOW_LEAN;
    }
}

pub mod tires {
    //! The tires: the "Magic Formula" grip curve, and how temperature and wear
    //! change a tire's grip. Cold rubber is hard and slides; overheated rubber
    //! goes greasy. Sliding heats it, the air rushing past cools it, and it
    //! wears away as it slides.
    use game::venues::Weather;

    const B: f64 = 10.0;
    const C: f64 = 1.9;
    const E: f64 = 0.97;
    /// Where the curve peaks (slip ratio or tan of slip angle).
    pub const PEAK_SLIP: f64 = 0.11;

    /// Tire force (as a share of the most it can give) for slip x.
    pub fn magic(x: f64) -> f64 {
        let bx = B * x;
        (C * (bx - E * (bx - bx.atan())).atan()).sin()
    }

    /// The curve's height at its peak, magic(PEAK_SLIP).
    pub const PEAK: f64 = 0.970_803_486_419_246_3;

    /// The temperature where a tire of this grip grips best, °C.
    pub fn best_temperature(grip: f64) -> f64 {
        40.0 + grip * 35.0
    }

    pub fn temperature_grip(t: f64, best: f64, floor: f64) -> f64 {
        let off = (t - best) / 45.0;
        (1.0 - 0.15 * off * off).max(floor)
    }

    pub fn wear_grip(wear: f64) -> f64 {
        1.0 - 0.15 * wear.min(1.0)
    }

    pub fn heat_tire(t: f64, slip_power: f64, speed: f64, ambient: f64, dt: f64) -> f64 {
        let heating = 1.5 * slip_power.max(0.0).powf(0.25);
        let cooling = 0.02 * (t - ambient) * (1.0 + speed / 20.0);
        t + (heating - cooling) * dt
    }

    pub fn ambient_temperature(weather: Weather, night: bool) -> f64 {
        let base = match weather {
            Weather::Snow => 0.0,
            Weather::Rain => 12.0,
            Weather::Clear => 24.0,
        };
        base - if night { 6.0 } else { 0.0 }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn the_curve_peaks_where_it_should() {
            let peak = super::magic(super::PEAK_SLIP);
            assert!((peak - super::PEAK).abs() < 1e-12, "peak {peak}");
            assert!(super::magic(0.5) < super::magic(0.2), "grip falls off past the top");
        }
    }
}

pub mod powertrain {
    //! The engine, turbo, gearbox and differentials: a torque curve for each kind
    //! of engine, turbo lag, an automatic that doesn't hunt between gears, and
    //! limited-slip differentials.

    /// How an engine's torque changes with revs, and whether it has a turbo.
    #[derive(Clone, Copy, Debug)]
    pub struct Engine {
        pub low: f64,
        pub peak: f64,
        pub redline: f64,
        pub turbo: bool,
    }

    pub fn engine_for(body: &str) -> Engine {
        match body {
            "muscle" => Engine {
                low: 0.8,
                peak: 0.5,
                redline: 0.95,
                turbo: false,
            },
            "pickup" | "van" => Engine {
                low: 0.85,
                peak: 0.35,
                redline: 0.95,
                turbo: false,
            },
            "hatch" => Engine {
                low: 0.6,
                peak: 0.45,
                redline: 0.95,
                turbo: true,
            },
            "wedge" => Engine {
                low: 0.6,
                peak: 0.8,
                redline: 0.96,
                turbo: true,
            },
            _ => Engine {
                low: 0.55,
                peak: 0.85,
                redline: 0.97,
                turbo: true,
            },
        }
    }

    pub fn torque_at(engine: &Engine, revs: f64) -> f64 {
        let r = revs.clamp(0.0, 1.05);
        if r <= engine.peak {
            return engine.low + (1.0 - engine.low) * (std::f64::consts::FRAC_PI_2 * (r / engine.peak)).sin();
        }
        let past = (r - engine.peak) / (1.0 - engine.peak).max(0.05);
        1.0 - (1.0 - engine.redline) * past * past
    }

    pub fn spool_turbo(boost: f64, throttle: f64, revs: f64, dt: f64, lag: bool) -> f64 {
        let ready = ((revs - 0.28) / 0.3).clamp(0.0, 1.0);
        let target = throttle * if lag { ready } else { 1.0 };
        if !lag {
            return target;
        }
        let rate = if target > boost { 1.6 } else { 6.0 };
        boost + (target - boost) * (dt * rate).min(1.0)
    }

    pub fn turbo_share(engine: &Engine, boost: f64) -> f64 {
        if engine.turbo { 0.68 + 0.32 * boost } else { 1.0 }
    }

    pub fn auto_gear(gear: usize, speed: f64, tops: &[f64]) -> usize {
        let mut g = gear.clamp(1, tops.len());
        while g < tops.len() && speed > tops[g - 1] {
            g += 1;
        }
        while g > 1 && speed < tops[g - 2] * 0.86 {
            g -= 1;
        }
        g
    }

    /// How the two driven wheels on an axle share the push: (left, right).
    pub fn differential(push: f64, lock: f64, left_speed: f64, right_speed: f64, road_speed: f64) -> (f64, f64) {
        let half = push / 2.0;
        if lock <= 0.0 {
            return (half, half);
        }
        let most = lock * half.abs() + lock * 0.6;
        let difference = (left_speed - right_speed) / (0.4 + 0.04 * road_speed.abs());
        let moved = most * difference.clamp(-1.0, 1.0);
        (half - moved, half + moved)
    }
}

pub mod suspension {
    //! The suspension: each wheel hangs on a spring and a damper, and an
    //! anti-roll bar links the two wheels on each axle. The body can rise and
    //! fall, dip its nose and lean. This is where the weight on each tire comes
    //! from. Numbers are per kilogram of car.

    /// The four wheels: front left, front right, rear left, rear right.
    pub const FRONT: [bool; 4] = [true, true, false, false];
    pub const LEFT: [bool; 4] = [true, false, true, false];

    #[derive(Clone, Copy, Debug)]
    pub struct SuspensionSpecs {
        pub spring: f64,
        pub bump: f64,
        pub rebound: f64,
        pub bar_front: f64,
        pub bar_rear: f64,
        pub sag: f64,
        pub travel: f64,
        pub knee: f64,
        pub pitch_inertia: f64,
        pub roll_inertia: f64,
    }

    #[derive(Clone, Debug, Default)]
    pub struct SuspensionState {
        pub heave: f64,
        pub heave_rate: f64,
        pub pitch: f64,
        pub pitch_rate: f64,
        pub roll: f64,
        pub roll_rate: f64,
        pub compression: [f64; 4],
        pub load: [f64; 4],
        pub shake: f64,
        pub road: [f64; 4],
    }

    pub fn suspension_specs(frequency: f64, size: f64, gravity: f64, wheel_base: f64, half_track: f64, rear_bar: f64) -> SuspensionSpecs {
        let f = frequency / size.max(0.15).sqrt();
        let omega = 2.0 * std::f64::consts::PI * f;
        let spring = omega * omega / 4.0;
        let critical = spring.sqrt();
        SuspensionSpecs {
            spring,
            bump: 0.45 * critical,
            rebound: 0.7 * critical,
            bar_front: 0.9 * spring,
            bar_rear: (0.35 + rear_bar) * spring,
            sag: gravity / (4.0 * spring),
            travel: 0.09 * size.max(0.15),
            knee: 0.15 * size.max(0.15).sqrt(),
            pitch_inertia: 0.95 * (wheel_base / 2.0).powi(2),
            roll_inertia: 0.8 * half_track.powi(2),
        }
    }

    /// One step of the body on its springs.
    #[allow(clippy::too_many_arguments)]
    pub fn step_suspension(
        s: &mut SuspensionState,
        sp: &SuspensionSpecs,
        dt: f64,
        road: &[f64; 4],
        x: &[f64; 4],
        y: &[f64; 4],
        gravity: f64,
        aero_pitch: f64,
        ax: f64,
        ay: f64,
        com_height: f64,
    ) {
        let mut forces = [0.0; 4];
        let mut squash = [0.0; 4];
        let mut shake = 0.0;
        let give = (dt / 0.012).min(1.0);
        for i in 0..4 {
            s.road[i] += (road[i] - s.road[i]) * give;
            let lift = s.heave - s.pitch * x[i] + s.roll * y[i];
            let c = sp.sag - lift + s.road[i];
            let rate = (c - s.compression[i]) / dt;
            s.compression[i] = c;
            squash[i] = c;
            if c <= 0.0 {
                continue;
            }
            let mut f = sp.spring * c;
            let over = c - (sp.sag + sp.travel);
            if over > 0.0 {
                f += sp.spring * 12.0 * over * if rate > 0.0 { 1.0 } else { 0.25 };
            }
            let fast = if rate.abs() > sp.knee {
                rate.signum() * (sp.knee + 0.3 * (rate.abs() - sp.knee))
            } else {
                rate
            };
            f += if rate > 0.0 { sp.bump } else { sp.rebound } * fast;
            forces[i] = f;
            shake += rate.abs();
        }
        for (l, r, bar) in [(0, 1, sp.bar_front), (2, 3, sp.bar_rear)] {
            let twist = bar * (squash[l].max(0.0) - squash[r].max(0.0));
            if squash[l] > 0.0 {
                forces[l] += twist;
            }
            if squash[r] > 0.0 {
                forces[r] -= twist;
            }
        }
        let (mut up, mut pitch, mut roll) = (0.0, 0.0, 0.0);
        for i in 0..4 {
            let f = forces[i].max(0.0);
            s.load[i] = f;
            up += f;
            pitch -= f * x[i];
            roll += f * y[i];
        }
        pitch -= ax * com_height;
        roll += ay * com_height;
        pitch += aero_pitch;
        s.heave_rate += (up - gravity) * dt;
        s.pitch_rate += pitch / sp.pitch_inertia * dt;
        s.roll_rate += roll / sp.roll_inertia * dt;
        s.heave += s.heave_rate * dt;
        s.pitch += s.pitch_rate * dt;
        s.roll += s.roll_rate * dt;
        let lowest = -(sp.travel * 2.5);
        let highest = sp.sag + sp.travel * 0.6;
        if s.heave < lowest {
            s.heave = lowest;
            s.heave_rate = s.heave_rate.max(0.0);
        } else if s.heave > highest {
            s.heave = highest;
            s.heave_rate = s.heave_rate.min(0.0);
        }
        (s.pitch, s.pitch_rate) = limit(s.pitch, s.pitch_rate, 0.25);
        (s.roll, s.roll_rate) = limit(s.roll, s.roll_rate, 0.25);
        s.shake += (shake / 4.0 - s.shake) * (dt * 20.0).min(1.0);
    }

    fn limit(angle: f64, rate: f64, most: f64) -> (f64, f64) {
        if angle > most {
            (most, rate.min(0.0))
        } else if angle < -most {
            (-most, rate.max(0.0))
        } else {
            (angle, rate)
        }
    }

    /// In the air: the body settles back, and no tire carries any weight.
    pub fn relax_suspension(s: &mut SuspensionState, dt: f64) {
        let k = (dt * 6.0).min(1.0);
        s.heave -= s.heave * k;
        s.pitch -= s.pitch * k;
        s.roll -= s.roll * k;
        s.heave_rate = 0.0;
        s.pitch_rate = 0.0;
        s.roll_rate = 0.0;
        s.load = [0.0; 4];
        s.road = [0.0; 4];
        s.compression = [0.0; 4];
        s.shake *= 1.0 - k;
    }

    /// Small bumps in the road surface under a point, m: the same every time over the same spot.
    pub fn roughness(x: f64, z: f64, amplitude: f64) -> f64 {
        if amplitude <= 0.0 {
            return 0.0;
        }
        let cell = 0.8;
        let (gx, gz) = (x / cell, z / cell);
        let (ix, iz) = (gx.floor(), gz.floor());
        let (fx, fz) = (smooth(gx - ix), smooth(gz - iz));
        let (ix, iz) = (ix as i64 as i32, iz as i64 as i32);
        let a = hash(ix, iz);
        let b = hash(ix.wrapping_add(1), iz);
        let c = hash(ix, iz.wrapping_add(1));
        let d = hash(ix.wrapping_add(1), iz.wrapping_add(1));
        let value = a + (b - a) * fx + (c - a) * fz + (a - b - c + d) * fx * fz;
        (value - 0.5) * 2.0 * amplitude
    }

    fn smooth(t: f64) -> f64 {
        t * t * (3.0 - 2.0 * t)
    }

    fn hash(x: i32, z: i32) -> f64 {
        let mut h = x.wrapping_mul(374_761_393).wrapping_add(z.wrapping_mul(668_265_263));
        h = (h ^ ((h as u32) >> 13) as i32).wrapping_mul(1_274_126_177);
        ((h ^ ((h as u32) >> 16) as i32) as u32) as f64 / 4_294_967_296.0
    }
}
