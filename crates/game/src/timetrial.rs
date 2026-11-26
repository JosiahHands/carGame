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
