//! The drivable world: the city, or one of the race track venues (see
//! venues.rs). The city is a grid of streets with buildings, parks, a parking
//! lot and a stunt park. The race tracks come from track.rs; world_build.rs
//! puts them on the map and dresses them up.
//!
//! Buildings, parked cars, street lamps and trees can be destroyed
//! ("targets") on every map. Buildings sink into a pile of rubble you can
//! drive over; parked cars blow up into burning wrecks; lamps and trees fall
//! over. Everything comes back a while after the player has driven away.
//!
//! This file is the world's state and rules. It doesn't touch the screen
//! directly: it lists what changed (`changes`) and world_render.rs shows it.
use std::collections::HashMap;

use bevy::math::DVec3;
use bevy::prelude::*;

use crate::carmodel::CarRig;
use crate::effects::Effects;
use crate::rng::{self, Seeded};
use crate::track::{RaceTrack, Wall};
use crate::venues::{self, Theme, Venue};

/// A time trial lap: arches to drive through in order, and where the car waits for the start.
#[derive(Clone, Debug)]
pub struct Course {
    pub checkpoints: Vec<Checkpoint>,
    pub start: Spot,
    /// Radius of each arch; driving within it counts.
    pub radius: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Checkpoint {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub heading: Option<f64>,
    pub bank: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Spot {
    pub x: f64,
    pub z: f64,
    pub heading: f64,
}

pub fn city_course() -> Course {
    let pts = [
        (0.0, 120.0),
        (40.0, 160.0),
        (120.0, 160.0),
        (160.0, 100.0),
        (160.0, 20.0),
        (120.0, -80.0),
        (40.0, -80.0),
        (0.0, -40.0),
        (0.0, 20.0),
    ];
    Course {
        checkpoints: pts
            .iter()
            .map(|&(x, z)| Checkpoint {
                x,
                y: 0.0,
                z,
                heading: None,
                bank: None,
            })
            .collect(),
        start: Spot {
            x: -3.0,
            z: 5.0,
            heading: 0.0,
        },
        radius: 7.5,
    }
}

/// A solid box standing on the ground (building, parked car, wall).
#[derive(Clone, Copy, Debug)]
pub struct BoxShape {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
    pub height: f64,
    pub solid: bool,
    pub target: Option<usize>,
}

/// A post (lamp, tree trunk, rock): a circle on the ground, `height` tall.
#[derive(Clone, Copy, Debug)]
pub struct Circle {
    pub x: f64,
    pub z: f64,
    pub r: f64,
    pub height: f64,
    /// Bullets hit within this radius (a tree's leaves are wider than its trunk).
    pub hit_radius: f64,
    pub solid: bool,
    pub target: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Building,
    Car,
    Lamp,
    Tree,
}

impl TargetKind {
    pub fn name(self) -> &'static str {
        match self {
            TargetKind::Building => "building",
            TargetKind::Car => "car",
            TargetKind::Lamp => "lamp",
            TargetKind::Tree => "tree",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingState {
    Standing,
    Falling,
    Rubble,
    Rising,
}

/// Something that can be destroyed.
#[derive(Clone, Debug)]
pub struct Target {
    pub kind: TargetKind,
    pub x: f64,
    pub z: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub alive: bool,
    pub destroyed_at: f64,
    /// What it costs, in dollars, for the destruction score.
    pub value: f64,
    /// The collision box (buildings and cars) or post (lamps and trees).
    pub shape: usize,
    // Buildings
    pub entity: Option<Entity>,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub state: BuildingState,
    pub timer: f64,
    pub lean: (f64, f64),
    pub rubble: Option<Entity>,
    pub rubble_height: f64,
    pub color: u32,
    // Cars
    pub rig: Option<CarRig>,
    pub home: Spot,
    pub pos: DVec3,
    /// Turn (euler x, y, z, applied in that order).
    pub rot: DVec3,
    pub flying: bool,
    pub velocity: DVec3,
    pub spin: DVec3,
    pub burn_until: f64,
    pub traffic: bool,
    // Lamps and trees
    pub prop: usize,
    pub size: f64,
}

impl Target {
    pub(crate) fn new_public(kind: TargetKind, x: f64, z: f64, hp: f64, value: f64, shape: usize) -> Self {
        Target {
            kind,
            x,
            z,
            hp,
            max_hp: hp,
            alive: true,
            destroyed_at: 0.0,
            value,
            shape,
            entity: None,
            width: 0.0,
            depth: 0.0,
            height: 0.0,
            state: BuildingState::Standing,
            timer: 0.0,
            lean: (0.0, 0.0),
            rubble: None,
            rubble_height: 0.0,
            color: 0,
            rig: None,
            home: Spot::default(),
            pos: DVec3::new(x, 0.0, z),
            rot: DVec3::ZERO,
            flying: false,
            velocity: DVec3::ZERO,
            spin: DVec3::ZERO,
            burn_until: 0.0,
            traffic: false,
            prop: 0,
            size: 1.0,
        }
    }
}

/// A lamp or tree, drawn merged with its neighbors (see PropChunk).
#[derive(Clone, Debug)]
pub struct Prop {
    pub lamp: bool,
    pub transform: Transform,
    /// The leaves' color (linear), for trees.
    pub color: [f32; 4],
    pub visible: bool,
    pub chunk: (i32, i32),
}

pub const CHUNK: f64 = 64.0;

/// What a ray hit.
#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub distance: f64,
    pub point: DVec3,
    pub target: Option<usize>,
    pub cone: Option<usize>,
    pub ground: bool,
    pub hit_box: Option<usize>,
}

/// A box that weapons can hit, for rival cars in battle events.
#[derive(Clone, Copy, Debug)]
pub struct HitBox {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub half: f64,
    pub height: f64,
    pub alive: bool,
    /// Which rival it belongs to.
    pub owner: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Stunt,
    Park,
    Lot,
    City,
}

#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub x: f64,
    pub z: f64,
    pub kind: BlockKind,
}

#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    pub x: f64,
    pub z: f64,
    /// The direction the ramp faces (driving up it).
    pub angle: f64,
    pub width: f64,
    pub length: f64,
    pub height: f64,
}

#[derive(Clone, Debug)]
pub struct Cone {
    pub entity: Entity,
    pub home: DVec3,
    pub pos: DVec3,
    pub rot: DVec3,
    pub vel: DVec3,
    pub spin: DVec3,
    pub knocked: bool,
    pub settled: bool,
    pub knocked_at: f64,
}

/// Something the screen has to show differently.
#[derive(Clone, Debug)]
pub enum Change {
    Move(Entity, Transform),
    Show(Entity, bool),
    /// A parked or traffic car (target index) was wrecked or repaired.
    Wreck(usize, bool),
    /// A group of lamps and trees needs drawing again.
    Chunk((i32, i32)),
    /// A building's rubble pile appears (target index).
    MakeRubble(usize),
    /// Remove a car's model.
    Despawn(Entity),
}

pub const HALF_SIZE: f64 = 260.0;
pub const ROAD_LINES: [f64; 7] = [-240.0, -160.0, -80.0, 0.0, 80.0, 160.0, 240.0];
pub const ROAD_WIDTH: f64 = 12.0;
pub const RISE: f64 = 0.8;
pub const CONE_HEIGHT: f64 = 0.75;
const CONE_LYING_Y: f64 = 0.25;
const FALL_TIME: f64 = 2.4;
const RISE_TIME: f64 = 2.0;
pub const RUBBLE_HEIGHT: f64 = 1.6;
const COMES_BACK_AFTER: f64 = 45.0;
const COMES_BACK_DISTANCE: f64 = 60.0;
const CELL: f64 = 16.0;
pub const WEATHER_DROPS: usize = 2500;
pub const WEATHER_BOX: f64 = 70.0;
pub const WEATHER_HEIGHT: f64 = 30.0;

#[derive(Default)]
struct Cell {
    circles: Vec<usize>,
    walls: Vec<usize>,
}

#[derive(Resource)]
pub struct World {
    pub boxes: Vec<BoxShape>,
    pub circles: Vec<Circle>,
    pub walls: Vec<Wall>,
    pub targets: Vec<Target>,
    pub props: Vec<Prop>,
    pub blocks: Vec<Block>,
    pub ramps: Vec<Ramp>,
    pub cones: Vec<Cone>,
    pub hit_boxes: Vec<HitBox>,
    /// Hits on the hit boxes since last asked: (owner, damage, direction).
    pub hit_box_hits: Vec<(usize, f64, DVec3)>,
    pub map: String,
    pub layout: u32,
    pub half: f64,
    pub track: Option<RaceTrack>,
    pub spawn: Spot,
    pub course: Course,
    /// Targets destroyed since last asked (for the score and sounds).
    pub destroyed: Vec<usize>,
    /// A new map was built since last asked.
    pub loaded: bool,
    /// Counts up each time a map is built (so the screen knows to redraw everything).
    pub generation: u64,
    pub paved_areas: Vec<[f64; 4]>,
    /// Lights lit on the start gantry (0-5).
    pub start_lights: usize,
    pub clock: f64,
    pub night: bool,
    pub view_distance: f64,
    pub weather: Vec<[f32; 3]>,
    pub weather_speed: f64,
    pub effects: Effects,
    pub changes: Vec<Change>,
    /// The entity everything on the map hangs from.
    pub root: Option<Entity>,
    pub(crate) rand: Seeded,
    cells: HashMap<(i64, i64), Cell>,
}

impl World {
    /// A world with nothing in it (for tests and before the first map loads).
    pub fn empty() -> Self {
        World {
            boxes: Vec::new(),
            circles: Vec::new(),
            walls: Vec::new(),
            targets: Vec::new(),
            props: Vec::new(),
            blocks: Vec::new(),
            ramps: Vec::new(),
            cones: Vec::new(),
            hit_boxes: Vec::new(),
            hit_box_hits: Vec::new(),
            map: "city".into(),
            layout: 0,
            half: HALF_SIZE,
            track: None,
            spawn: Spot {
                x: -3.0,
                z: -20.0,
                heading: 0.0,
            },
            course: city_course(),
            destroyed: Vec::new(),
            loaded: false,
            generation: 0,
            paved_areas: Vec::new(),
            start_lights: 0,
            clock: 0.0,
            night: false,
            view_distance: 430.0,
            weather: Vec::new(),
            weather_speed: 0.0,
            effects: Effects::default(),
            changes: Vec::new(),
            root: None,
            rand: Seeded::new(42.0),
            cells: HashMap::new(),
        }
    }

    pub fn venue(&self) -> &'static Venue {
        venues::venue(&self.map)
    }

    pub fn theme(&self) -> &'static Theme {
        &self.venue().theme
    }

    /// Whether it's dark: the player switched to night, or the venue is always at night.
    pub fn is_night(&self) -> bool {
        self.night || self.theme().night
    }

    /// Fills the lookup grid of posts and walls. They never move, so this is done once per map.
    pub(crate) fn build_cells(&mut self) {
        self.cells.clear();
        let mut add = |cells: &mut HashMap<(i64, i64), Cell>, min_x: f64, max_x: f64, min_z: f64, max_z: f64, put: &dyn Fn(&mut Cell)| {
            for cx in (min_x / CELL).floor() as i64..=(max_x / CELL).floor() as i64 {
                for cz in (min_z / CELL).floor() as i64..=(max_z / CELL).floor() as i64 {
                    put(cells.entry((cx, cz)).or_default());
                }
            }
        };
        for (i, c) in self.circles.iter().enumerate() {
            add(&mut self.cells, c.x - c.r, c.x + c.r, c.z - c.r, c.z + c.r, &|cell| {
                cell.circles.push(i)
            });
        }
        for (i, w) in self.walls.iter().enumerate() {
            add(&mut self.cells, w.min_x, w.max_x, w.min_z, w.max_z, &|cell| cell.walls.push(i));
        }
    }

    /// The posts and walls that might be within `reach` meters of (x, z), as indices.
    pub fn nearby(&self, x: f64, z: f64, reach: f64) -> (Vec<usize>, Vec<usize>) {
        let mut circles = Vec::new();
        let mut walls = Vec::new();
        for cx in ((x - reach) / CELL).floor() as i64..=((x + reach) / CELL).floor() as i64 {
            for cz in ((z - reach) / CELL).floor() as i64..=((z + reach) / CELL).floor() as i64 {
                if let Some(cell) = self.cells.get(&(cx, cz)) {
                    circles.extend_from_slice(&cell.circles);
                    walls.extend_from_slice(&cell.walls);
                }
            }
        }
        circles.sort_unstable();
        circles.dedup();
        walls.sort_unstable();
        walls.dedup();
        (circles, walls)
    }

    // ---------- driving ----------

    /// Ground height at a point: non-zero on banked track, ramps and rubble piles.
    pub fn height_at(&self, x: f64, z: f64) -> f64 {
        let mut best = self.track.as_ref().map_or(0.0, |t| t.height_at(x, z));
        for r in &self.ramps {
            let (dx, dz) = (x - r.x, z - r.z);
            let (s, c) = r.angle.sin_cos();
            let u = dx * s + dz * c;
            let v = dx * c - dz * s;
            if v.abs() > r.width / 2.0 || u.abs() > r.length / 2.0 {
                continue;
            }
            let from_start = u + r.length / 2.0;
            let peak = r.length * RISE;
            let h = if from_start <= peak {
                r.height * from_start / peak
            } else {
                r.height * (r.length - from_start) / (r.length - peak)
            };
            best = best.max(h);
        }
        for t in &self.targets {
            if t.kind != TargetKind::Building || t.rubble_height <= 0.0 {
                continue;
            }
            best = best.max(t.rubble_height * rubble_shape(x - t.x, z - t.z, t.width, t.depth));
        }
        best
    }

    /// How much grip the tires have at (x, z), compared to dry asphalt.
    pub fn grip_at(&self, x: f64, z: f64) -> f64 {
        let weather = if self.map == "city" { 1.0 } else { self.theme().grip };
        if self.off_road(x, z) { weather * 0.75 } else { weather }
    }

    /// Whether (x, z) is grass that slows the car down: anywhere off the asphalt on the race tracks.
    pub fn off_road(&self, x: f64, z: f64) -> bool {
        match &self.track {
            None => false,
            Some(t) if t.is_paved(x, z) => false,
            Some(_) => !self.paved_areas.iter().any(|a| x > a[0] && x < a[1] && z > a[2] && z < a[3]),
        }
    }

    /// Knocks over cones the car touches. Returns how many were hit.
    pub fn hit_cones(&mut self, x: f64, z: f64, car_y: f64, radius: f64, vx: f64, vz: f64) -> u32 {
        if car_y + 0.2 > CONE_HEIGHT {
            return 0;
        }
        let mut hits = 0;
        for i in 0..self.cones.len() {
            let c = &self.cones[i];
            if c.knocked {
                continue;
            }
            let dx = c.pos.x - x;
            let dz = c.pos.z - z;
            if dx * dx + dz * dz > (radius + 0.3).powi(2) {
                continue;
            }
            let speed = vx.hypot(vz);
            self.knock_cone(i, DVec3::new(vx * 1.1 + dx * 2.0, 2.0 + speed * 0.18, vz * 1.1 + dz * 2.0));
            hits += 1;
        }
        hits
    }

    fn knock_cone(&mut self, i: usize, velocity: DVec3) {
        let clock = self.clock;
        let c = &mut self.cones[i];
        c.knocked = true;
        c.settled = false;
        c.knocked_at = clock;
        c.vel = velocity;
        c.spin = DVec3::new(rng::centered() * 14.0, rng::centered() * 6.0, rng::centered() * 14.0);
    }

    /// A car driving into a lamp or tree fast enough (40 km/h) knocks it straight over.
    pub fn ram(&mut self, t: usize, speed: f64, direction: DVec3) -> bool {
        let target = &self.targets[t];
        if !target.alive || !matches!(target.kind, TargetKind::Lamp | TargetKind::Tree) || speed < 11.0 {
            return false;
        }
        self.destroy(t, direction);
        true
    }

    // ---------- weapons ----------

    /// The first thing a ray hits (direction length 1), within `max_distance`.
    pub fn raycast(&self, origin: DVec3, direction: DVec3, max_distance: f64) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        let mut consider = |point: DVec3, target: Option<usize>, cone: Option<usize>, ground: bool, hit_box: Option<usize>| {
            let distance = origin.distance(point);
            if distance <= max_distance && best.is_none_or(|b| distance < b.distance) {
                best = Some(RayHit {
                    distance,
                    point,
                    target,
                    cone,
                    ground,
                    hit_box,
                });
            }
        };
        if let Some(track) = &self.track {
            let below = |t: f64| {
                let p = origin + direction * t;
                (p.y <= track.height_at(p.x, p.z), p)
            };
            let mut t = 1.0;
            while t < max_distance + 1.0 {
                if below(t).0 {
                    let (mut lo, mut hi) = (t - 1.0, t);
                    for _ in 0..10 {
                        let mid = (lo + hi) / 2.0;
                        if below(mid).0 {
                            hi = mid;
                        } else {
                            lo = mid;
                        }
                    }
                    consider(below(hi).1, None, None, true, None);
                    break;
                }
                t += 1.0;
            }
        } else if direction.y < 0.0 {
            let t = -origin.y / direction.y;
            if t >= 0.0 {
                consider(origin + direction * t, None, None, true, None);
            }
        }
        for b in &self.boxes {
            if !b.solid {
                continue;
            }
            if let Some(p) = ray_box(
                origin,
                direction,
                DVec3::new(b.min_x, 0.0, b.min_z),
                DVec3::new(b.max_x, b.height, b.max_z),
            ) {
                consider(p, b.target, None, false, None);
            }
        }
        for w in &self.walls {
            let sx = w.bx - w.ax;
            let sz = w.bz - w.az;
            let denom = direction.x * sz - direction.z * sx;
            if denom.abs() < 1e-9 {
                continue;
            }
            let qx = w.ax - origin.x;
            let qz = w.az - origin.z;
            let t = (qx * sz - qz * sx) / denom;
            let u = (qx * direction.z - qz * direction.x) / denom;
            if t < 0.0 || !(0.0..=1.0).contains(&u) {
                continue;
            }
            let p = origin + direction * t;
            if p.y >= 0.0 && p.y <= w.top {
                consider(p, None, None, false, None);
            }
        }
        for c in &self.circles {
            if !c.solid {
                continue;
            }
            let r = c.hit_radius;
            if let Some(p) = ray_box(
                origin,
                direction,
                DVec3::new(c.x - r, 0.0, c.z - r),
                DVec3::new(c.x + r, c.height, c.z + r),
            ) {
                consider(p, c.target, None, false, None);
            }
        }
        for (i, c) in self.cones.iter().enumerate() {
            if c.knocked {
                continue;
            }
            if let Some(p) = ray_box(
                origin,
                direction,
                DVec3::new(c.pos.x - 0.3, 0.0, c.pos.z - 0.3),
                DVec3::new(c.pos.x + 0.3, CONE_HEIGHT, c.pos.z + 0.3),
            ) {
                consider(p, None, Some(i), false, None);
            }
        }
        for (i, h) in self.hit_boxes.iter().enumerate() {
            if !h.alive {
                continue;
            }
            if let Some(p) = ray_box(
                origin,
                direction,
                DVec3::new(h.x - h.half, h.y, h.z - h.half),
                DVec3::new(h.x + h.half, h.y + h.height, h.z + h.half),
            ) {
                consider(p, None, None, false, Some(i));
            }
        }
        best
    }

    /// A bullet or rocket hit: damages what was hit, or knocks a cone flying.
    pub fn apply_hit(&mut self, hit: &RayHit, damage: f64, direction: DVec3) {
        if let Some(t) = hit.target {
            self.damage(t, damage, direction);
        }
        if let Some(c) = hit.cone {
            self.knock_cone(c, DVec3::new(direction.x * 8.0, 4.0, direction.z * 8.0));
        }
        if let Some(h) = hit.hit_box {
            let owner = self.hit_boxes[h].owner;
            self.hit_box_hits.push((owner, damage, direction));
        }
    }

    /// Damages everything within `radius` of a point: full damage at the center, 30% at the edge.
    pub fn explode(&mut self, at: DVec3, radius: f64, damage: f64) {
        for i in 0..self.targets.len() {
            let t = &self.targets[i];
            if !t.alive {
                continue;
            }
            let (min, max) = match t.kind {
                TargetKind::Building | TargetKind::Car => {
                    let b = &self.boxes[t.shape];
                    (DVec3::new(b.min_x, 0.0, b.min_z), DVec3::new(b.max_x, b.height, b.max_z))
                }
                _ => {
                    let c = &self.circles[t.shape];
                    (
                        DVec3::new(t.x - c.r * 2.0, 0.0, t.z - c.r * 2.0),
                        DVec3::new(t.x + c.r * 2.0, c.height, t.z + c.r * 2.0),
                    )
                }
            };
            let distance = at.clamp(min, max).distance(at);
            if distance > radius {
                continue;
            }
            let away = DVec3::new(t.x - at.x, 0.0, t.z - at.z).normalize_or_zero();
            self.damage(i, damage * (1.0 - 0.7 * (distance / radius)), away);
        }
        for h in &self.hit_boxes {
            if !h.alive {
                continue;
            }
            let min = DVec3::new(h.x - h.half, h.y, h.z - h.half);
            let max = DVec3::new(h.x + h.half, h.y + h.height, h.z + h.half);
            let distance = at.clamp(min, max).distance(at);
            if distance > radius {
                continue;
            }
            self.hit_box_hits.push((
                h.owner,
                damage * (1.0 - 0.7 * (distance / radius)),
                DVec3::new(h.x - at.x, 0.0, h.z - at.z).normalize_or_zero(),
            ));
        }
        for i in 0..self.cones.len() {
            let c = &self.cones[i];
            let away = DVec3::new(c.pos.x - at.x, 0.0, c.pos.z - at.z);
            let distance = away.length();
            if distance > radius * 1.3 {
                continue;
            }
            let push = 3.0 + 18.0 * (1.0 - distance / (radius * 1.3));
            let v = away.normalize_or_zero() * push;
            self.knock_cone(i, DVec3::new(v.x, 4.0 + push * 0.5, v.z));
        }
    }

    /// Takes `amount` of health away, destroying the thing when it runs out.
    pub fn damage(&mut self, t: usize, amount: f64, direction: DVec3) {
        if !self.targets[t].alive {
            return;
        }
        self.targets[t].hp -= amount;
        if self.targets[t].hp <= 0.0 {
            self.destroy(t, direction);
        }
    }

    /// Puts everything destroyed back at once ("Rebuild the city"), except
    /// anything under the player's car, which comes back later as usual.
    pub fn rebuild_all(&mut self, car_x: f64, car_z: f64, car_radius: f64) {
        for i in 0..self.targets.len() {
            let t = &self.targets[i];
            if !t.alive && self.is_under_car(i, car_x, car_z, car_radius) {
                continue;
            }
            if !t.alive || (t.kind == TargetKind::Building && t.state != BuildingState::Standing) {
                self.restore(i, false);
            }
            let t = &mut self.targets[i];
            t.hp = t.max_hp;
        }
        self.effects.clear();
    }

    fn is_under_car(&self, i: usize, x: f64, z: f64, car_radius: f64) -> bool {
        let t = &self.targets[i];
        let margin = car_radius + 0.5;
        match t.kind {
            TargetKind::Building => {
                let b = &self.boxes[t.shape];
                x > b.min_x - margin && x < b.max_x + margin && z > b.min_z - margin && z < b.max_z + margin
            }
            TargetKind::Car => (x - t.home.x).hypot(z - t.home.z) < 2.5 + margin,
            _ => {
                let c = &self.circles[t.shape];
                (x - c.x).hypot(z - c.z) < c.r + margin
            }
        }
    }

    /// Destroys something; `direction` is the way it was hit, so things fall away from you.
    fn destroy(&mut self, i: usize, direction: DVec3) {
        let clock = self.clock;
        let t = &mut self.targets[i];
        t.alive = false;
        t.hp = 0.0;
        t.destroyed_at = clock;
        match t.kind {
            TargetKind::Building => {
                t.state = BuildingState::Falling;
                t.timer = 0.0;
                self.boxes[t.shape].solid = false;
                t.lean = (direction.z * 0.12, -direction.x * 0.12);
                let (x, z, w, d, h, color) = (t.x, t.z, t.width, t.depth, t.height, t.color);
                self.changes.push(Change::MakeRubble(i));
                let fx = &mut self.effects;
                fx.explosion(DVec3::new(x, 2.0, z), 2.2);
                fx.shatter(DVec3::new(x, h / 2.0, z), 40, &[color, 0x6d6a66, 0x8a8783], 1.6, 9.0);
                fx.dust(x, z, w / 2.0, d / 2.0, 60, 1.3);
            }
            TargetKind::Car => {
                t.burn_until = clock + 15.0;
                t.flying = true;
                self.boxes[t.shape].solid = false;
                t.velocity = DVec3::new(
                    direction.x * 5.0 + rng::centered() * 2.0,
                    12.0,
                    direction.z * 5.0 + rng::centered() * 2.0,
                );
                t.spin = DVec3::new(rng::centered() * 6.0, rng::centered() * 4.0, rng::centered() * 6.0);
                let p = t.pos;
                self.changes.push(Change::Wreck(i, true));
                self.effects.explosion(DVec3::new(p.x, 0.8, p.z), 1.6);
                self.effects
                    .shatter(DVec3::new(p.x, 0.8, p.z), 10, &[0x222222, 0x444444, 0x9cc3d9], 0.5, 8.0);
            }
            TargetKind::Lamp | TargetKind::Tree => {
                self.circles[t.shape].solid = false;
                let lamp = t.kind == TargetKind::Lamp;
                let (x, z, size, prop) = (t.x, t.z, t.size, t.prop);
                self.set_prop_visible(prop, false);
                let height = if lamp { 7.0 } else { 2.4 } * size;
                let fx = &mut self.effects;
                fx.add_debris(
                    DVec3::new(x + direction.x, height / 2.0, z + direction.z),
                    DVec3::new(direction.x * 6.0, 3.0, direction.z * 6.0),
                    if lamp {
                        DVec3::new(0.24 * size, height, 0.24 * size)
                    } else {
                        DVec3::new(0.5 * size, height, 0.5 * size)
                    },
                    if lamp { 0x3a3d42 } else { 0x6b4a2f },
                    4.0,
                );
                if lamp {
                    fx.shatter(DVec3::new(x, height, z), 4, &[0xffffff, 0xcccccc], 0.3, 5.0);
                } else {
                    fx.shatter(DVec3::new(x, 3.6 * size, z), 14, &[0x4f8a3a, 0x3f7a2d, 0x5f9a45], 0.9 * size, 5.0);
                }
            }
        }
        self.destroyed.push(i);
    }

    /// Brings a destroyed thing back. Buildings rise out of the ground if `animate` is set.
    fn restore(&mut self, i: usize, animate: bool) {
        let t = &mut self.targets[i];
        t.alive = true;
        t.hp = t.max_hp;
        match t.kind {
            TargetKind::Building => {
                self.boxes[t.shape].solid = true;
                if let Some(e) = t.entity {
                    self.changes.push(Change::Show(e, true));
                }
                if animate {
                    t.state = BuildingState::Rising;
                    t.timer = 0.0;
                } else {
                    t.state = BuildingState::Standing;
                    t.rubble_height = 0.0;
                    if let Some(e) = t.entity {
                        self.changes.push(Change::Move(
                            e,
                            Transform::from_xyz(t.x as f32, (t.height / 2.0) as f32, t.z as f32),
                        ));
                    }
                    if let Some(r) = t.rubble {
                        self.changes.push(Change::Show(r, false));
                    }
                }
            }
            TargetKind::Car => {
                t.flying = false;
                t.burn_until = 0.0;
                t.pos = DVec3::new(t.home.x, 0.0, t.home.z);
                t.rot = DVec3::new(0.0, t.home.heading, 0.0);
                self.changes.push(Change::Wreck(i, false));
                self.fit_car_box(i);
                let t = &self.targets[i];
                self.boxes[t.shape].solid = true;
                self.push_car_transform(i);
            }
            _ => {
                self.circles[t.shape].solid = true;
                let prop = t.prop;
                self.set_prop_visible(prop, true);
            }
        }
    }

    fn set_prop_visible(&mut self, prop: usize, visible: bool) {
        let p = &mut self.props[prop];
        if p.visible != visible {
            p.visible = visible;
            self.changes.push(Change::Chunk(p.chunk));
        }
    }

    /// Moves a car's collision box to wherever the car (or wreck) now is.
    pub fn fit_car_box(&mut self, i: usize) {
        let t = &mut self.targets[i];
        let Some(rig) = &t.rig else { return };
        let (c, s) = (t.rot.y.cos().abs(), t.rot.y.sin().abs());
        let half_width = rig.width / 2.0 + 0.05;
        let half_length = rig.length / 2.0;
        let ex = c * half_width + s * half_length;
        let ez = s * half_width + c * half_length;
        let b = &mut self.boxes[t.shape];
        b.min_x = t.pos.x - ex;
        b.max_x = t.pos.x + ex;
        b.min_z = t.pos.z - ez;
        b.max_z = t.pos.z + ez;
        t.x = t.pos.x;
        t.z = t.pos.z;
    }

    pub(crate) fn push_car_transform(&mut self, i: usize) {
        let t = &self.targets[i];
        if let Some(rig) = &t.rig {
            let tr = Transform::from_translation(t.pos.as_vec3())
                .with_rotation(Quat::from_euler(EulerRot::XYZ, t.rot.x as f32, t.rot.y as f32, t.rot.z as f32))
                .with_scale(Vec3::splat(rig.config.size as f32));
            self.changes.push(Change::Move(rig.root, tr));
        }
    }

    /// Moves a traffic car (and its collision box) to a new spot.
    pub fn move_car(&mut self, i: usize, x: f64, z: f64, heading: f64) {
        let t = &mut self.targets[i];
        t.pos = DVec3::new(x, 0.0, z);
        t.rot = DVec3::new(0.0, heading, 0.0);
        self.fit_car_box(i);
        self.push_car_transform(i);
    }

    /// Takes a traffic car out of the world. Its slot stays (so other indices
    /// don't change) but it's gone for good.
    pub fn remove_car(&mut self, i: usize) {
        let t = &mut self.targets[i];
        t.alive = false;
        t.flying = false;
        t.burn_until = 0.0;
        t.destroyed_at = f64::INFINITY;
        t.traffic = false;
        t.kind = TargetKind::Car;
        self.boxes[t.shape].solid = false;
        self.boxes[t.shape].min_x = f64::INFINITY;
        self.boxes[t.shape].max_x = f64::NEG_INFINITY;
        if let Some(rig) = t.rig.take() {
            self.changes.push(Change::Despawn(rig.root));
        }
    }

    /// Lights up the first `count` of the five start lights.
    pub fn set_start_lights(&mut self, count: usize) {
        self.start_lights = count;
    }

    // ---------- every frame ----------

    /// `car_radius` is half the car's diagonal: how far its footprint reaches from its center.
    pub fn update(&mut self, dt: f64, car_x: f64, car_z: f64, car_radius: f64) {
        self.clock += dt;
        self.update_buildings(dt);
        self.update_wrecks(dt);
        self.update_cones(dt, car_x, car_z);
        for i in 0..self.targets.len() {
            let t = &self.targets[i];
            if t.alive || (t.kind == TargetKind::Car && (t.flying || t.rig.is_none())) {
                continue;
            }
            let long_ago = self.clock - t.destroyed_at > COMES_BACK_AFTER;
            let far_away = (car_x - t.x).hypot(car_z - t.z) > COMES_BACK_DISTANCE + car_radius;
            if long_ago && far_away && !self.is_under_car(i, car_x, car_z, car_radius) {
                self.restore(i, true);
            }
        }
        self.update_weather(dt, car_x, car_z);
        self.effects.update(dt);
    }

    fn update_buildings(&mut self, dt: f64) {
        for i in 0..self.targets.len() {
            let t = &mut self.targets[i];
            if t.kind != TargetKind::Building {
                continue;
            }
            match t.state {
                BuildingState::Falling => {
                    t.timer += dt;
                    let progress = (t.timer / FALL_TIME).min(1.0);
                    let sunk = progress * progress;
                    let shake = 0.3 * (1.0 - progress);
                    let pos = Vec3::new(
                        (t.x + rng::centered() * shake) as f32,
                        (t.height / 2.0 - t.height * sunk) as f32,
                        (t.z + rng::centered() * shake) as f32,
                    );
                    let rot = Quat::from_euler(EulerRot::XYZ, (t.lean.0 * progress) as f32, 0.0, (t.lean.1 * progress) as f32);
                    t.rubble_height = RUBBLE_HEIGHT * progress;
                    if let Some(e) = t.entity {
                        self.changes
                            .push(Change::Move(e, Transform::from_translation(pos).with_rotation(rot)));
                    }
                    if let Some(r) = t.rubble {
                        self.changes.push(Change::Move(
                            r,
                            Transform::from_xyz(t.x as f32, 0.02, t.z as f32).with_scale(Vec3::new(1.0, progress.max(0.001) as f32, 1.0)),
                        ));
                    }
                    if rng::random() < dt * 30.0 {
                        self.effects.dust(t.x, t.z, t.width / 2.0, t.depth / 2.0, 4, 1.3);
                    }
                    if progress >= 1.0 {
                        t.state = BuildingState::Rubble;
                        if let Some(e) = t.entity {
                            self.changes.push(Change::Show(e, false));
                        }
                    }
                }
                BuildingState::Rising => {
                    t.timer += dt;
                    let progress = (t.timer / RISE_TIME).min(1.0);
                    let risen = 1.0 - (1.0 - progress).powi(3);
                    t.rubble_height = RUBBLE_HEIGHT * (1.0 - progress);
                    if let Some(e) = t.entity {
                        self.changes.push(Change::Move(
                            e,
                            Transform::from_xyz(t.x as f32, (t.height / 2.0 - t.height * (1.0 - risen)) as f32, t.z as f32),
                        ));
                    }
                    if let Some(r) = t.rubble {
                        self.changes.push(Change::Move(
                            r,
                            Transform::from_xyz(t.x as f32, 0.02, t.z as f32).with_scale(Vec3::new(
                                1.0,
                                (1.0 - progress).max(0.001) as f32,
                                1.0,
                            )),
                        ));
                    }
                    if progress >= 1.0 {
                        t.state = BuildingState::Standing;
                        t.rubble_height = 0.0;
                        if let Some(r) = t.rubble {
                            self.changes.push(Change::Show(r, false));
                        }
                    }
                }
                _ => {
                    if t.alive && t.hp < t.max_hp * 0.6 {
                        let strength = 1.2 * (1.0 - t.hp / t.max_hp);
                        let at = DVec3::new(t.x, t.height, t.z);
                        self.effects.burn(at, strength, dt);
                    }
                }
            }
        }
    }

    fn update_wrecks(&mut self, dt: f64) {
        for i in 0..self.targets.len() {
            let t = &mut self.targets[i];
            if t.kind != TargetKind::Car || t.alive || t.rig.is_none() {
                continue;
            }
            if t.flying {
                t.velocity.y -= 16.0 * dt;
                t.pos += t.velocity * dt;
                t.rot += t.spin * dt;
                if t.pos.y <= 0.0 && t.velocity.y < 0.0 {
                    t.pos.y = 0.0;
                    if t.velocity.y < -5.0 {
                        t.velocity = DVec3::new(t.velocity.x * 0.5, -t.velocity.y * 0.3, t.velocity.z * 0.5);
                        t.spin *= 0.4;
                        let (x, z) = (t.pos.x, t.pos.z);
                        self.effects.dust(x, z, 1.0, 2.0, 6, 0.4);
                    } else {
                        t.flying = false;
                        t.rot = DVec3::new(0.0, t.rot.y, 0.0);
                        t.pos.x = t.pos.x.clamp(-self.half + 3.0, self.half - 3.0);
                        t.pos.z = t.pos.z.clamp(-self.half + 3.0, self.half - 3.0);
                        self.fit_car_box(i);
                        let t = &self.targets[i];
                        self.boxes[t.shape].solid = true;
                    }
                }
                self.push_car_transform(i);
            }
            let t = &self.targets[i];
            if self.clock < t.burn_until {
                let at = DVec3::new(t.pos.x, t.pos.y + 0.8, t.pos.z);
                self.effects.burn(at, 0.9, dt);
            }
        }
    }

    fn update_cones(&mut self, dt: f64, car_x: f64, car_z: f64) {
        for c in &mut self.cones {
            if !c.knocked {
                continue;
            }
            if !c.settled {
                c.vel.y -= 14.0 * dt;
                c.pos += c.vel * dt;
                c.rot += c.spin * dt;
                if c.pos.y < 0.0 {
                    c.pos.y = 0.0;
                    c.vel.y = c.vel.y.abs() * 0.3;
                    c.vel.x *= 0.6;
                    c.vel.z *= 0.6;
                    c.spin *= 0.6;
                    if c.vel.length_squared() < 0.5 {
                        c.vel = DVec3::ZERO;
                        c.rot = DVec3::new(std::f64::consts::FRAC_PI_2, c.rot.y, 0.0);
                        c.pos.y = CONE_LYING_Y;
                        c.settled = true;
                    }
                }
            }
            if self.clock - c.knocked_at > 12.0 && (car_x - c.home.x).hypot(car_z - c.home.z) > 40.0 {
                c.knocked = false;
                c.settled = false;
                c.vel = DVec3::ZERO;
                c.pos = c.home;
                c.rot = DVec3::ZERO;
            }
            let tr = Transform::from_translation(c.pos.as_vec3()).with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                c.rot.x as f32,
                c.rot.y as f32,
                c.rot.z as f32,
            ));
            self.changes.push(Change::Move(c.entity, tr));
        }
    }

    fn update_weather(&mut self, dt: f64, x: f64, z: f64) {
        if self.weather.is_empty() {
            return;
        }
        let rain = self.weather_speed > 5.0;
        let per_drop = if rain { 2 } else { 1 };
        let fall = (self.weather_speed * dt) as f32;
        let sway = if per_drop == 1 {
            ((self.clock * 0.7).sin() * 0.6 * dt) as f32
        } else {
            0.0
        };
        let half = (WEATHER_BOX / 2.0) as f32;
        let (x, z) = (x as f32, z as f32);
        for drop in self.weather.chunks_mut(per_drop) {
            let a = drop[0];
            let mut dy = -fall;
            if a[1] - fall < 0.0 {
                dy += WEATHER_HEIGHT as f32;
            }
            let b = WEATHER_BOX as f32;
            let dx = if a[0] - x > half {
                -b
            } else if a[0] - x < -half {
                b
            } else {
                0.0
            };
            let dz = if a[2] - z > half {
                -b
            } else if a[2] - z < -half {
                b
            } else {
                0.0
            };
            for p in drop.iter_mut() {
                p[0] += dx + sway;
                p[1] += dy;
                p[2] += dz;
            }
        }
    }

    /// How far you can see before everything fades into the fog.
    pub fn set_view_distance(&mut self, distance: f64) {
        self.view_distance = distance;
    }

    /// Day or night (some venues, like the neon city, are always at night).
    pub fn set_night(&mut self, night: bool) {
        self.night = night;
    }
}

/// Where a ray (direction length 1) first enters a box, if it does.
pub fn ray_box(origin: DVec3, dir: DVec3, min: DVec3, max: DVec3) -> Option<DVec3> {
    let mut t_min = 0.0f64;
    let mut t_max = f64::INFINITY;
    for k in 0..3 {
        let (o, d, lo, hi) = (origin[k], dir[k], min[k], max[k]);
        if d.abs() < 1e-12 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let (mut t1, mut t2) = ((lo - o) / d, (hi - o) / d);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_min = t_min.max(t1);
            t_max = t_max.min(t2);
            if t_min > t_max {
                return None;
            }
        }
    }
    Some(origin + dir * t_min)
}

/// The shape of a rubble pile, 0 at its edge to 1 on top, at (dx, dz) from the middle.
pub fn rubble_shape(dx: f64, dz: f64, width: f64, depth: f64) -> f64 {
    let from_middle = (dx.abs() / (width / 2.0)).max(dz.abs() / (depth / 2.0));
    ((1.0 - from_middle) * 2.5).clamp(0.0, 1.0)
}
