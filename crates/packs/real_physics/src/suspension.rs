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
