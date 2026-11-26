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
