//! Things to do in free roam, on every map: gold wrenches to find, and
//! challenges with three stars each (speed cameras, drift zones and jump
//! zones). Stars only count in your career car; wrenches count in any car.
//! Spots come from the same seeds as the 1.2.x game, so found wrenches carry over.
use std::collections::HashSet;

use bevy::math::DVec3;

use crate::rng::Seeded;
use crate::text::thousands;
use crate::world::{BlockKind, HALF_SIZE, ROAD_LINES, World};

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
    pub fn update(&mut self, dt: f64, car: DVec3, speed: f64, airborne: bool, drift: f64, air_time: f64, reach: f64) -> Vec<FreeRoamEvent> {
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
                        text: format!("{mph} mph"),
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
    let unit = match z.kind {
        ZoneKind::Speed => " mph",
        ZoneKind::Jump => " s",
        ZoneKind::Drift => "",
    };
    z.targets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "{} {}{unit}",
                "★".repeat(i + 1),
                if z.kind == ZoneKind::Jump { format!("{t}") } else { thousands(*t) }
            )
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
