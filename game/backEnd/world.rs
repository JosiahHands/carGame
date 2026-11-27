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
use crate::venues::{Theme, Venue};

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
    /// The ground under it (on a hillside in the open world; 0 elsewhere).
    pub base: f64,
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
    /// The ground under it.
    pub base: f64,
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
    /// What kind of tree (the open world has every kind).
    pub kind: crate::venues::Trees,
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
    pub track: Option<std::sync::Arc<RaceTrack>>,
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
    /// The open world around the city (free roam on the City map only).
    pub land: Option<Box<crate::land::Land>>,
    /// Build the City map as the open world (set before `load`).
    pub open: bool,
    /// The generative map: the open world never ends (set before `load`).
    pub generative: bool,
    /// The weather picked for this map in the garage, if not the venue's own.
    pub weather_choice: Option<crate::venues::Weather>,
    /// Boxes that never move, by 16 m square; the cars' boxes are in `car_boxes`.
    box_cells: HashMap<(i64, i64), Vec<usize>>,
    pub car_boxes: Vec<usize>,
    /// Buildings with a rubble pile (they change the ground's height).
    pub(crate) rubble: Vec<usize>,
    /// How many circles, walls and boxes are in the lookup grids so far.
    indexed: (usize, usize, usize),
    /// Towns just built (indices into `land.towns`), for their people to move in.
    pub built_towns: Vec<usize>,
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
            land: None,
            open: false,
            generative: false,
            weather_choice: None,
            box_cells: HashMap::new(),
            car_boxes: Vec::new(),
            rubble: Vec::new(),
            indexed: (0, 0, 0),
            built_towns: Vec::new(),
        }
    }

    /// Rain, snow or clear: the garage's choice for this map, or the venue's own.
    pub fn weather_kind(&self) -> crate::venues::Weather {
        self.weather_choice.unwrap_or(if self.map == "city" {
            crate::venues::Weather::Clear
        } else {
            self.theme().weather
        })
    }

    /// The City map as the open world (free roam).
    pub fn is_open(&self) -> bool {
        self.land.is_some()
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

    /// Fills the lookup grids of posts, walls and boxes. They never move, so
    /// each is added once (the open world adds more as you go: `index_new`).
    pub(crate) fn build_cells(&mut self) {
        self.cells.clear();
        self.box_cells.clear();
        self.car_boxes.clear();
        self.indexed = (0, 0, 0);
        self.index_new();
    }

    /// Adds the posts, walls and boxes made since last time to the lookup grids.
    pub fn index_new(&mut self) {
        let squares = |min_x: f64, max_x: f64, min_z: f64, max_z: f64| {
            let xs = (min_x / CELL).floor() as i64..=(max_x / CELL).floor() as i64;
            let zs = (min_z / CELL).floor() as i64..=(max_z / CELL).floor() as i64;
            xs.flat_map(move |cx| zs.clone().map(move |cz| (cx, cz)))
        };
        for i in self.indexed.0..self.circles.len() {
            let c = self.circles[i];
            for key in squares(c.x - c.r, c.x + c.r, c.z - c.r, c.z + c.r) {
                self.cells.entry(key).or_default().circles.push(i);
            }
        }
        for i in self.indexed.1..self.walls.len() {
            let w = self.walls[i];
            for key in squares(w.min_x, w.max_x, w.min_z, w.max_z) {
                self.cells.entry(key).or_default().walls.push(i);
            }
        }
        for i in self.indexed.2..self.boxes.len() {
            let b = self.boxes[i];
            let moves = b
                .target
                .is_some_and(|t| self.targets.get(t).is_some_and(|t| t.kind == TargetKind::Car));
            if moves || !b.min_x.is_finite() {
                self.car_boxes.push(i);
                continue;
            }
            for key in squares(b.min_x, b.max_x, b.min_z, b.max_z) {
                self.box_cells.entry(key).or_default().push(i);
            }
        }
        self.indexed = (self.circles.len(), self.walls.len(), self.boxes.len());
    }

    /// The boxes that might be within `reach` meters of (x, z): the still
    /// ones nearby, and every car.
    pub fn boxes_near(&self, x: f64, z: f64, reach: f64) -> Vec<usize> {
        let mut out = Vec::new();
        for cx in ((x - reach) / CELL).floor() as i64..=((x + reach) / CELL).floor() as i64 {
            for cz in ((z - reach) / CELL).floor() as i64..=((z + reach) / CELL).floor() as i64 {
                if let Some(list) = self.box_cells.get(&(cx, cz)) {
                    out.extend_from_slice(list);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out.extend_from_slice(&self.car_boxes);
        // Boxes made since the grids were last filled.
        out.extend(self.indexed.2..self.boxes.len());
        out
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

    /// Ground height at a point: non-zero on banked track, ramps, rubble
    /// piles and the open world's hills.
    pub fn height_at(&self, x: f64, z: f64) -> f64 {
        let mut best = match (&self.track, &self.land) {
            (Some(t), _) => t.height_at(x, z),
            (None, Some(land)) => land.ground(x, z),
            _ => 0.0,
        };
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
        for &i in &self.rubble {
            let t = &self.targets[i];
            if t.rubble_height <= 0.0 {
                continue;
            }
            best = best.max(t.rubble_height * rubble_shape(x - t.x, z - t.z, t.width, t.depth));
        }
        best
    }

    /// How much grip the tires have at (x, z), compared to dry asphalt.
    pub fn grip_at(&self, x: f64, z: f64) -> f64 {
        let weather = match self.weather_choice {
            Some(crate::venues::Weather::Rain) => self.theme().grip.min(0.84),
            Some(crate::venues::Weather::Snow) => self.theme().grip.min(0.72),
            Some(crate::venues::Weather::Clear) => 1.0,
            None if self.map == "city" => 1.0,
            None => self.theme().grip,
        };
        if self.off_road(x, z) { weather * 0.75 } else { weather }
    }

    /// Whether (x, z) is grass that slows the car down: anywhere off the
    /// asphalt on the race tracks, and off the roads and out of the towns in
    /// the open world.
    pub fn off_road(&self, x: f64, z: f64) -> bool {
        if let Some(land) = &self.land {
            return !land.paved(x, z);
        }
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
        if self.track.is_some() || self.land.is_some() {
            let below = |t: f64| {
                let p = origin + direction * t;
                (p.y <= self.ground_only(p.x, p.z), p)
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
                DVec3::new(b.min_x, b.base, b.min_z),
                DVec3::new(b.max_x, b.base + b.height, b.max_z),
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
                DVec3::new(c.x - r, c.base, c.z - r),
                DVec3::new(c.x + r, c.base + c.height, c.z + r),
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

    /// The ground itself (track or hills), without ramps or rubble.
    pub fn ground_only(&self, x: f64, z: f64) -> f64 {
        match (&self.track, &self.land) {
            (Some(t), _) => t.height_at(x, z),
            (None, Some(land)) => land.ground(x, z),
            _ => 0.0,
        }
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
            // Removed traffic keeps its slot, but has no model or valid collision bounds.
            if !t.alive && (!t.destroyed_at.is_finite() || self.is_under_car(i, car_x, car_z, car_radius)) {
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
                if !self.rubble.contains(&i) {
                    self.rubble.push(i);
                }
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
                let y = self.land.as_ref().map_or(0.0, |l| l.ground(t.home.x, t.home.z));
                t.pos = DVec3::new(t.home.x, y, t.home.z);
                t.rot = DVec3::new(0.0, t.home.heading, 0.0);
                self.boxes[t.shape].base = y;
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
        let y = match &self.land {
            Some(land) => land.ground(x, z),
            None => 0.0,
        };
        let t = &mut self.targets[i];
        t.pos = DVec3::new(x, y, z);
        self.boxes[t.shape].base = y;
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
        let targets = &self.targets;
        self.rubble
            .retain(|&i| targets[i].rubble_height > 0.0 || targets[i].state != BuildingState::Standing);
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
                let ground = match (&self.track, &self.land) {
                    (None, Some(land)) => land.ground(t.pos.x, t.pos.z),
                    _ => 0.0,
                };
                if t.pos.y <= ground && t.velocity.y < 0.0 {
                    t.pos.y = ground;
                    if t.velocity.y < -5.0 {
                        t.velocity = DVec3::new(t.velocity.x * 0.5, -t.velocity.y * 0.3, t.velocity.z * 0.5);
                        t.spin *= 0.4;
                        let (x, z) = (t.pos.x, t.pos.z);
                        self.effects.dust(x, z, 1.0, 2.0, 6, 0.4);
                    } else {
                        t.flying = false;
                        t.rot = DVec3::new(0.0, t.rot.y, 0.0);
                        self.boxes[t.shape].base = t.pos.y;
                        if self.land.is_none() {
                            t.pos.x = t.pos.x.clamp(-self.half + 3.0, self.half - 3.0);
                            t.pos.z = t.pos.z.clamp(-self.half + 3.0, self.half - 3.0);
                        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_map_event_reset_never_restores_removed_traffic() {
        let mut world = World::empty();
        world.boxes.push(BoxShape {
            min_x: -2.0,
            max_x: 2.0,
            min_z: -1.0,
            max_z: 1.0,
            height: 1.7,
            solid: true,
            target: Some(0),
            base: 0.0,
        });
        world.targets.push(Target::new_public(TargetKind::Car, 0.0, 0.0, 100.0, 500.0, 0));
        world.targets[0].traffic = true;

        world.remove_car(0);
        world.rebuild_all(1e6, 1e6, 0.0);

        assert!(!world.targets[0].alive);
        assert!(!world.boxes[0].solid);
        assert!(world.raycast(DVec3::new(0.0, 1.0, -4.0), DVec3::Z, 10.0).is_none());
    }
}

pub mod track {
    //! A closed race track, used by every map except the city.
    //!
    //! The track is a center line sampled every few meters. Each sample knows which
    //! way the road runs, how sharply it turns and how much it is banked. From
    //! that, the track answers "how high is the road here?" and "is this on the
    //! asphalt?", gives the barrier walls for the car to bump into, and builds its
    //! own meshes: road, curbs, start line, barriers and catch fence.
    //!
    //! Positions across the road are an offset from the center line, in meters,
    //! positive to the left (as seen driving around the track).
    //!
    //! Tracks are drawn like a turtle drawing: straights and corners in turn (see
    //! `draw_track`). `generate_layout` makes new ones from a seed number.
    use std::collections::HashMap;
    use std::f64::consts::PI;

    use crate::canvas::{Canvas, Rgba};
    use crate::meshes::MeshData;
    use crate::rng::Seeded;

    const SPACING: f64 = 4.0; // meters between samples
    const GRID_CELL: f64 = 20.0; // size of the lookup grid's cells, meters

    /// One sample of the center line.
    #[derive(Clone, Copy, Debug)]
    pub struct TrackPoint {
        pub x: f64,
        pub z: f64,
        /// Direction of travel, length 1.
        pub dx: f64,
        pub dz: f64,
        /// How sharply it turns: 1 / radius, positive turning left.
        pub curve: f64,
        /// Bank angle in radians; positive raises the left side of the road.
        pub bank: f64,
        /// Meters from the start line.
        pub along: f64,
    }

    /// How a track looks and where its barriers are.
    #[derive(Clone, Debug)]
    pub struct TrackStyle {
        pub width: f64,
        /// Bank angle in degrees on the straights and in the tightest turns.
        pub bank: [f64; 2],
        pub curbs: bool,
        pub outside_runoff: f64,
        /// Grass on the inside; None means no barrier on the inside at all.
        pub inside_runoff: Option<f64>,
        /// No barriers at all, just grass and trees (a rally stage).
        pub no_walls: bool,
        pub apron: f64,
        pub wall_height: f64,
        /// A wire catch fence on top of the outside wall, this tall (0 for none).
        pub fence: f64,
        pub wall_colors: [&'static str; 2],
    }

    /// Walls are this thick: the car can't come within half of that of a wall's line.
    pub const WALL_THICKNESS: f64 = 0.6;

    /// A wall the car can hit: a line on the ground from a to b, solid up to `top`.
    #[derive(Clone, Copy, Debug)]
    pub struct Wall {
        pub ax: f64,
        pub az: f64,
        pub bx: f64,
        pub bz: f64,
        pub top: f64,
        pub min_x: f64,
        pub max_x: f64,
        pub min_z: f64,
        pub max_z: f64,
    }

    pub fn make_wall(ax: f64, az: f64, bx: f64, bz: f64, top: f64) -> Wall {
        Wall {
            ax,
            az,
            bx,
            bz,
            top,
            min_x: ax.min(bx),
            max_x: ax.max(bx),
            min_z: az.min(bz),
            max_z: az.max(bz),
        }
    }

    /// Where something sits on the track.
    #[derive(Clone, Copy, Debug)]
    pub struct TrackPose {
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub heading: f64,
        pub bank: f64,
    }

    struct Nearest {
        offset: f64,
        bank: f64,
        along: f64,
    }

    /// A straight (meters) or a corner (degrees, radius; positive degrees turn left).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum TrackStep {
        Straight(f64),
        Corner(f64, f64),
    }

    pub struct RaceTrack {
        pub points: Vec<TrackPoint>,
        pub length: f64,
        /// +1 if the track mostly turns left (so the inside is on the left), -1 if right.
        pub inside: f64,
        pub left_wall: Option<f64>,
        pub right_wall: Option<f64>,
        pub style: TrackStyle,
        grid: HashMap<(i64, i64), Vec<usize>>,
    }

    fn wrap_angle(a: f64) -> f64 {
        a.sin().atan2(a.cos())
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

    impl RaceTrack {
        /// `path` is a smooth closed loop of points, in driving order, starting at the start line.
        pub fn new(path: &[(f64, f64)], style: TrackStyle) -> Self {
            let mut lengths = vec![0.0];
            for i in 1..=path.len() {
                let a = path[i - 1];
                let b = path[i % path.len()];
                lengths.push(lengths[i - 1] + (b.0 - a.0).hypot(b.1 - a.1));
            }
            let total = lengths[path.len()];
            let count = (total / SPACING).round() as usize;
            let mut points = Vec::with_capacity(count);
            let mut seg = 0;
            for i in 0..count {
                let along = (i as f64 / count as f64) * total;
                while lengths[seg + 1] < along {
                    seg += 1;
                }
                let a = path[seg];
                let b = path[(seg + 1) % path.len()];
                let span = lengths[seg + 1] - lengths[seg];
                let f = (along - lengths[seg]) / if span == 0.0 { 1.0 } else { span };
                points.push(TrackPoint {
                    x: a.0 + (b.0 - a.0) * f,
                    z: a.1 + (b.1 - a.1) * f,
                    dx: 0.0,
                    dz: 1.0,
                    curve: 0.0,
                    bank: 0.0,
                    along,
                });
            }
            let n = points.len();
            let at = |i: isize| (i.rem_euclid(n as isize)) as usize;
            for i in 0..n as isize {
                let (p1, p0) = (points[at(i + 1)], points[at(i - 1)]);
                let (dx, dz) = (p1.x - p0.x, p1.z - p0.z);
                let d = dx.hypot(dz);
                let d = if d == 0.0 { 1.0 } else { d };
                points[at(i)].dx = dx / d;
                points[at(i)].dz = dz / d;
            }
            let heading = |p: &TrackPoint| p.dx.atan2(p.dz);
            let mut turning = 0.0;
            for i in 0..n as isize {
                let c = wrap_angle(heading(&points[at(i + 1)]) - heading(&points[at(i - 1)])) / (2.0 * SPACING);
                points[at(i)].curve = c;
                turning += c;
            }
            let inside = if turning >= 0.0 { 1.0 } else { -1.0 };

            let straight = style.bank[0] * PI / 180.0;
            let turn = style.bank[1] * PI / 180.0;
            let raw: Vec<f64> = points
                .iter()
                .map(|p| -inside * (straight + (turn - straight) * (p.curve.abs() * 260.0).min(1.0)))
                .collect();
            let window: isize = 15;
            for i in 0..n as isize {
                let mut sum = 0.0;
                for k in -window..=window {
                    sum += raw[at(i + k)];
                }
                points[at(i)].bank = sum / (window * 2 + 1) as f64;
            }

            let edge = style.width / 2.0;
            let inside_wall = if style.no_walls {
                None
            } else {
                style.inside_runoff.map(|r| edge + r)
            };
            let outside_wall = if style.no_walls { None } else { Some(edge + style.outside_runoff) };
            let (left_wall, right_wall) = if inside > 0.0 {
                (inside_wall, outside_wall)
            } else {
                (outside_wall, inside_wall)
            };

            let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
            let reach = edge + style.outside_runoff.max(style.inside_runoff.unwrap_or(0.0)).max(style.apron) + 40.0;
            let cells = (reach / GRID_CELL).ceil() as i64;
            for (i, p) in points.iter().enumerate() {
                let cx = (p.x / GRID_CELL).floor() as i64;
                let cz = (p.z / GRID_CELL).floor() as i64;
                for gx in cx - cells..=cx + cells {
                    for gz in cz - cells..=cz + cells {
                        grid.entry((gx, gz)).or_default().push(i);
                    }
                }
            }
            RaceTrack {
                points,
                length: total,
                inside,
                left_wall,
                right_wall,
                style,
                grid,
            }
        }

        fn nearest(&self, x: f64, z: f64) -> Option<Nearest> {
            let candidates = self.grid.get(&((x / GRID_CELL).floor() as i64, (z / GRID_CELL).floor() as i64))?;
            let mut best = 0;
            let mut best_dist = f64::INFINITY;
            for &i in candidates {
                let p = &self.points[i];
                let d = (p.x - x).powi(2) + (p.z - z).powi(2);
                if d < best_dist {
                    best_dist = d;
                    best = i;
                }
            }
            let n = self.points.len();
            let mut result = None;
            let mut result_dist = f64::INFINITY;
            for start in [(best + n - 1) % n, best] {
                let a = &self.points[start];
                let b = &self.points[(start + 1) % n];
                let (sx, sz) = (b.x - a.x, b.z - a.z);
                let t = (((x - a.x) * sx + (z - a.z) * sz) / (sx * sx + sz * sz)).clamp(0.0, 1.0);
                let (px, pz) = (a.x + sx * t, a.z + sz * t);
                let d = (x - px).powi(2) + (z - pz).powi(2);
                if d < result_dist {
                    result_dist = d;
                    let dx = a.dx + (b.dx - a.dx) * t;
                    let dz = a.dz + (b.dz - a.dz) * t;
                    let len = dx.hypot(dz);
                    let offset = ((x - px) * dz - (z - pz) * dx) / if len == 0.0 { 1.0 } else { len };
                    result = Some(Nearest {
                        offset,
                        bank: a.bank + (b.bank - a.bank) * t,
                        along: a.along + SPACING * t,
                    });
                }
            }
            result
        }

        /// Height of the road surface at an offset across a banked road.
        fn surface_height(&self, bank: f64, offset: f64) -> f64 {
            if bank == 0.0 {
                return 0.0;
            }
            let w = self.style.width;
            bank.abs().tan() * (w / 2.0 + sign(bank) * offset).clamp(0.0, w)
        }

        /// Height of the ground at (x, z): above zero only on banked track, inside the outside wall.
        pub fn height_at(&self, x: f64, z: f64) -> f64 {
            if self.style.bank[1] == 0.0 {
                return 0.0;
            }
            let Some(near) = self.nearest(x, z) else { return 0.0 };
            let outside_wall = if self.inside > 0.0 { self.right_wall } else { self.left_wall }.unwrap_or(f64::INFINITY);
            if -self.inside * near.offset > outside_wall {
                return 0.0;
            }
            self.surface_height(near.bank, near.offset)
        }

        /// Whether (x, z) is on the road, its curbs or other asphalt, rather than on grass.
        pub fn is_paved(&self, x: f64, z: f64) -> bool {
            let Some(near) = self.nearest(x, z) else { return false };
            let s = &self.style;
            let curb = if s.curbs { 1.4 } else { 0.3 };
            let inside_paved = s.width / 2.0 + s.apron.max(curb);
            let outside_paved = s.width / 2.0 + if s.fence > 0.0 { s.outside_runoff } else { curb };
            let towards_inside = self.inside * near.offset;
            towards_inside <= inside_paved && -towards_inside <= outside_paved
        }

        /// Where (x, z) is along the track and how far left of the center line.
        pub fn locate(&self, x: f64, z: f64) -> Option<(f64, f64)> {
            self.nearest(x, z).map(|n| (n.along, n.offset))
        }

        /// The sample at `along` meters from the start line (wrapping around the lap).
        pub fn point_at(&self, along: f64) -> TrackPoint {
            let n = self.points.len();
            let i = ((along.rem_euclid(self.length) / self.length * n as f64).floor() as usize) % n;
            self.points[i]
        }

        /// How far (x, z) is from the middle of the road, or infinity far from the track.
        pub fn distance_from(&self, x: f64, z: f64) -> f64 {
            self.nearest(x, z).map_or(f64::INFINITY, |n| n.offset.abs())
        }

        /// A spot `along` meters from the start line and `offset` meters left of the center.
        pub fn pose(&self, along: f64, offset: f64) -> TrackPose {
            let n = self.points.len();
            let f = along.rem_euclid(self.length) / self.length * n as f64;
            let i = (f.floor() as usize) % n;
            let t = f - f.floor();
            let a = self.points[i];
            let b = self.points[(i + 1) % n];
            let dx = a.dx + (b.dx - a.dx) * t;
            let dz = a.dz + (b.dz - a.dz) * t;
            let d = dx.hypot(dz);
            let d = if d == 0.0 { 1.0 } else { d };
            let x = a.x + (b.x - a.x) * t + (dz / d) * offset;
            let z = a.z + (b.z - a.z) * t - (dx / d) * offset;
            let bank = a.bank + (b.bank - a.bank) * t;
            TrackPose {
                x,
                y: self.surface_height(bank, offset),
                z,
                heading: dx.atan2(dz),
                bank,
            }
        }

        fn wall_sides(&self) -> Vec<f64> {
            [self.left_wall, self.right_wall.map(|w| -w)].into_iter().flatten().collect()
        }

        /// The barrier walls for the car to hit, as short straight pieces.
        pub fn walls(&self) -> Vec<Wall> {
            let mut walls = Vec::new();
            let n = self.points.len();
            for side in self.wall_sides() {
                let outside = -self.inside * side > 0.0;
                let middle = side + sign(side) * (WALL_THICKNESS / 2.0);
                for i in 0..n {
                    let a = self.offset_point(i, middle);
                    let b = self.offset_point((i + 1) % n, middle);
                    let top = a.1.max(b.1) + self.style.wall_height + if outside { self.style.fence } else { 0.0 };
                    walls.push(make_wall(a.0, a.2, b.0, b.2, top));
                }
            }
            walls
        }

        /// Sample i moved `offset` meters to the left, with the road height there.
        fn offset_point(&self, i: usize, offset: f64) -> (f64, f64, f64) {
            let p = &self.points[i];
            (p.x + p.dz * offset, self.surface_height(p.bank, offset), p.z - p.dx * offset)
        }

        // ---------- meshes ----------

        /// The road, curbs, start line, barriers and fence.
        pub fn build_meshes(&self) -> TrackMeshes {
            let s = &self.style;
            let edge = s.width / 2.0;
            let n = self.points.len();
            let all: Vec<usize> = (0..=n).map(|i| i % n).collect();
            let road = self.ribbon(&all, -edge, edge, 0.03, 10.0);
            let mut paved = MeshData::default();
            if s.apron > 0.0 {
                paved.append(&self.ribbon(&all, self.inside * edge, self.inside * (edge + s.apron), 0.025, 10.0));
            }
            if s.fence > 0.0 {
                paved.append(&self.ribbon(&all, -self.inside * edge, -self.inside * (edge + s.outside_runoff), 0.025, 10.0));
            }
            let mut curbs = MeshData::default();
            if s.curbs {
                let tight: Vec<bool> = self.points.iter().map(|p| p.curve.abs() > 1.0 / 160.0).collect();
                let wanted: Vec<bool> = (0..n)
                    .map(|i| (-4..=4).any(|k: isize| tight[(i as isize + k).rem_euclid(n as isize) as usize]))
                    .collect();
                for run in runs_of(&wanted) {
                    for side in [1.0, -1.0] {
                        curbs.append(&self.ribbon(&run, side * edge, side * (edge + 1.4), 0.04, 3.0));
                    }
                }
            }
            // Start / finish line, and the starting grid bars.
            let start = self.pose(0.0, 0.0);
            let mut line = MeshData::default();
            flat_quad(
                &mut line,
                &start,
                s.width,
                2.4,
                self.surface_height(start.bank, 0.0) + 0.045,
                0.0,
                [0.0, 0.0, 1.0, 1.0],
            );
            let mut grid = MeshData::default();
            for k in 0..10 {
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                let p = self.pose(-12.0 - k as f64 * 9.0, side * edge * 0.45);
                flat_quad(&mut grid, &p, edge * 0.6, 0.4, p.y + 0.045, 0.0, [0.0, 0.0, 1.0, 1.0]);
            }
            let mut walls = MeshData::default();
            let mut fence = MeshData::default();
            let mut posts = Vec::new();
            for side in self.wall_sides() {
                let outside = -self.inside * side > 0.0;
                walls.append(&self.wall_mesh(side));
                if outside && s.fence > 0.0 {
                    let (f, p) = self.fence_mesh(side);
                    fence.append(&f);
                    posts.extend(p);
                }
            }
            TrackMeshes {
                road,
                paved,
                curbs,
                line,
                grid,
                walls,
                fence,
                posts,
            }
        }

        fn ribbon(&self, indices: &[usize], from: f64, to: f64, lift: f64, meters_per_repeat: f64) -> MeshData {
            let mut m = MeshData::default();
            let mut distance = 0.0;
            for (k, &i) in indices.iter().enumerate() {
                if k > 0 {
                    let prev = &self.points[indices[k - 1]];
                    distance += (self.points[i].x - prev.x).hypot(self.points[i].z - prev.z);
                }
                for (offset, u) in [(from, 0.0), (to, 1.0)] {
                    let p = self.offset_point(i, offset);
                    m.vertex([p.0, p.1 + lift, p.2], [u, distance / meters_per_repeat]);
                }
                if k > 0 {
                    let a = ((k - 1) * 2) as u32;
                    m.tri(a, a + 1, a + 2);
                    m.tri(a + 1, a + 3, a + 2);
                }
            }
            // Make every triangle face up (which way they wind depends on which side `from` is).
            for t in m.indices.chunks_mut(3) {
                let p = |i: u32| bevy::math::Vec3::from(m.positions[i as usize]);
                if (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).y < 0.0 {
                    t.swap(1, 2);
                }
            }
            m.compute_normals();
            m
        }

        fn wall_mesh(&self, offset: f64) -> MeshData {
            let n = self.points.len();
            let thick = sign(offset) * WALL_THICKNESS;
            let mut m = MeshData::default();
            let mut distance = 0.0;
            for k in 0..=n {
                let i = k % n;
                let inner = self.offset_point(i, offset);
                let outer = self.offset_point(i, offset + thick);
                if k > 0 {
                    let prev = self.offset_point((k - 1) % n, offset);
                    distance += (inner.0 - prev.0).hypot(inner.2 - prev.2);
                }
                let top = inner.1 + self.style.wall_height;
                let v = distance / 8.0;
                m.vertex([inner.0, 0.0, inner.2], [v, 1.0]);
                m.vertex([inner.0, top, inner.2], [v, 0.0]);
                m.vertex([outer.0, top, outer.2], [v, 0.0]);
                m.vertex([outer.0, 0.0, outer.2], [v, 1.0]);
                if k > 0 {
                    let a = ((k - 1) * 4) as u32;
                    let b = (k * 4) as u32;
                    for (c0, c1) in [(0, 1), (1, 2), (2, 3)] {
                        m.tri(a + c0, b + c0, a + c1);
                        m.tri(a + c1, b + c0, b + c1);
                    }
                }
            }
            m.double_sided();
            m
        }

        fn fence_mesh(&self, offset: f64) -> (MeshData, Vec<[f64; 3]>) {
            let n = self.points.len();
            let mut m = MeshData::default();
            let mut distance = 0.0;
            let mut posts = Vec::new();
            for k in 0..=n {
                let p = self.offset_point(k % n, offset);
                if k > 0 {
                    let prev = self.offset_point((k - 1) % n, offset);
                    distance += (p.0 - prev.0).hypot(p.2 - prev.2);
                }
                let bottom = p.1 + self.style.wall_height;
                m.vertex([p.0, bottom, p.2], [distance / 4.0, 1.0]);
                m.vertex(
                    [p.0, bottom + self.style.fence, p.2],
                    [distance / 4.0, 1.0 - self.style.fence / 4.0],
                );
                if k > 0 {
                    let a = ((k - 1) * 2) as u32;
                    m.tri(a, a + 2, a + 1);
                    m.tri(a + 1, a + 2, a + 3);
                }
                if k % 3 == 0 && k < n {
                    posts.push([p.0, bottom, p.2]);
                }
            }
            m.double_sided();
            (m, posts)
        }
    }

    /// A flat rectangle on the road at a pose (w across, l along), raised to height y.
    pub fn flat_quad(m: &mut MeshData, p: &TrackPose, w: f64, l: f64, y: f64, along_shift: f64, uv: [f64; 4]) {
        let (fx, fz) = (p.heading.sin(), p.heading.cos());
        let (lx, lz) = (fz, -fx); // left
        let tilt = p.bank.tan();
        let corner = |a: f64, c: f64| {
            [
                p.x + fx * (a + along_shift) + lx * c,
                y + c * tilt,
                p.z + fz * (a + along_shift) + lz * c,
            ]
        };
        let base = m.positions.len() as u32;
        m.vertex(corner(-l / 2.0, w / 2.0), [uv[0], uv[1]]);
        m.vertex(corner(-l / 2.0, -w / 2.0), [uv[2], uv[1]]);
        m.vertex(corner(l / 2.0, -w / 2.0), [uv[2], uv[3]]);
        m.vertex(corner(l / 2.0, w / 2.0), [uv[0], uv[3]]);
        for _ in 0..4 {
            m.normals.push([0.0, 1.0, 0.0]);
        }
        m.tri(base, base + 2, base + 1);
        m.tri(base, base + 3, base + 2);
    }

    /// The pieces of a track, ready to give materials.
    pub struct TrackMeshes {
        pub road: MeshData,
        pub paved: MeshData,
        pub curbs: MeshData,
        pub line: MeshData,
        pub grid: MeshData,
        pub walls: MeshData,
        pub fence: MeshData,
        pub posts: Vec<[f64; 3]>,
    }

    /// Turns straights and corners into a closed loop of points about 2 m apart,
    /// starting at (0, 0) heading along +x. Any small gap left where the loop
    /// should join up is spread along the whole lap.
    pub fn draw_track(steps: &[TrackStep]) -> Vec<(f64, f64)> {
        let (mut x, mut z) = (0.0, 0.0);
        let mut heading = PI / 2.0;
        let mut points = vec![(x, z)];
        for step in steps {
            match *step {
                TrackStep::Straight(length) => {
                    let n = (length / 2.0).ceil().max(1.0) as usize;
                    for i in 1..=n {
                        points.push((
                            x + heading.sin() * length * i as f64 / n as f64,
                            z + heading.cos() * length * i as f64 / n as f64,
                        ));
                    }
                    x += heading.sin() * length;
                    z += heading.cos() * length;
                }
                TrackStep::Corner(degrees, radius) => {
                    let turn = degrees * PI / 180.0;
                    let side = sign(turn);
                    let cx = x + side * radius * heading.cos();
                    let cz = z - side * radius * heading.sin();
                    let n = ((turn.abs() * radius) / 2.0).ceil().max(1.0) as usize;
                    for i in 1..=n {
                        let h = heading + turn * i as f64 / n as f64;
                        points.push((cx - side * radius * h.cos(), cz + side * radius * h.sin()));
                    }
                    heading += turn;
                    (x, z) = *points.last().unwrap();
                }
            }
        }
        let (gap_x, gap_z) = (x, z);
        points.pop();
        let count = points.len() as f64;
        points
            .iter()
            .enumerate()
            .map(|(i, p)| (p.0 - gap_x * i as f64 / count, p.1 - gap_z * i as f64 / count))
            .collect()
    }

    /// What kind of track generate_layout should make.
    #[derive(Clone, Debug)]
    pub struct LayoutOptions {
        pub length: f64,
        pub radius: [f64; 2],
        pub corners: [f64; 2],
        pub left_only: bool,
        pub clearance: f64,
    }

    /// Makes a new closed track from a seed number: the same seed always gives the same track.
    pub fn generate_layout(seed: f64, o: &LayoutOptions) -> Vec<TrackStep> {
        let mut rand = Seeded::new(seed * 7919.0 + 13.0);
        for _ in 0..600 {
            let count = rand.between(o.corners[0], o.corners[1]).round() as usize;
            let mut angles: Vec<f64>;
            if o.left_only {
                let weights: Vec<f64> = (0..count).map(|_| rand.between(0.6, 1.4)).collect();
                let total: f64 = weights.iter().sum();
                angles = weights.iter().map(|w| w / total * 360.0).collect();
            } else {
                angles = (0..count)
                    .map(|_| {
                        let s = if rand.next() < 0.62 { 1.0 } else { -1.0 };
                        s * rand.between(30.0, 150.0)
                    })
                    .collect();
                let missing = 360.0 - angles.iter().sum::<f64>();
                let lefts = angles.iter().filter(|a| **a > 0.0).count();
                if lefts == 0 {
                    continue;
                }
                angles = angles
                    .iter()
                    .map(|a| if *a > 0.0 { a + missing / lefts as f64 } else { *a })
                    .collect();
            }
            if angles.iter().any(|a| a.abs() < 20.0 || a.abs() > 180.0) {
                continue;
            }
            let radii: Vec<f64> = angles
                .iter()
                .map(|_| o.radius[0] + (o.radius[1] - o.radius[0]) * rand.next().powf(1.6))
                .collect();
            let main = (o.length * rand.between(0.12, 0.18)).clamp(110.0, 380.0);
            let arcs: f64 = angles.iter().zip(&radii).map(|(a, r)| a.abs() * PI * r / 180.0).sum();
            let spare = (count as f64 * 30.0).max(o.length - main - arcs);
            let weights: Vec<f64> = (0..count.saturating_sub(1)).map(|_| rand.between(0.2, 1.8)).collect();
            let weight_sum = {
                let s: f64 = weights.iter().sum();
                if s == 0.0 { 1.0 } else { s }
            };
            let mut straights: Vec<f64> = weights.iter().map(|w| (w / weight_sum * spare).max(20.0)).collect();
            let steps = |straights: &[f64]| {
                let mut out = vec![TrackStep::Straight(main / 2.0)];
                for (i, a) in angles.iter().enumerate() {
                    out.push(TrackStep::Corner(*a, radii[i]));
                    if i < count - 1 {
                        out.push(TrackStep::Straight(straights[i]));
                    }
                }
                out.push(TrackStep::Straight(main / 2.0));
                out
            };
            let end = end_of(&steps(&straights));
            let mut directions = Vec::new();
            let mut heading = PI / 2.0;
            for a in angles.iter().take(count.saturating_sub(1)) {
                heading += a * PI / 180.0;
                directions.push((heading.sin(), heading.cos()));
            }
            let mut best: Option<(usize, usize, f64, f64)> = None;
            let longest = (o.length * 0.3).max(120.0);
            for i in 0..directions.len() {
                for j in i + 1..directions.len() {
                    let (ax, az) = directions[i];
                    let (bx, bz) = directions[j];
                    let det = ax * bz - bx * az;
                    if det.abs() < 0.5 {
                        continue;
                    }
                    let ka = (-end.0 * bz + end.1 * bx) / det;
                    let kb = (-ax * end.1 + az * end.0) / det;
                    let la = straights[i] + ka;
                    let lb = straights[j] + kb;
                    if la < 25.0 || lb < 25.0 || la > longest || lb > longest {
                        continue;
                    }
                    if best.is_none_or(|b| ka.abs() + kb.abs() < b.2.abs() + b.3.abs()) {
                        best = Some((i, j, ka, kb));
                    }
                }
            }
            let Some((i, j, ka, kb)) = best else { continue };
            straights[i] += ka;
            straights[j] += kb;
            let layout = steps(&straights);
            if is_clear_of_itself(&draw_track(&layout), o.clearance) {
                return layout;
            }
        }
        let r = o.radius[1];
        use TrackStep::*;
        vec![
            Straight(200.0),
            Corner(90.0, r),
            Straight(150.0),
            Corner(90.0, r),
            Straight(400.0),
            Corner(90.0, r),
            Straight(150.0),
            Corner(90.0, r),
            Straight(200.0),
        ]
    }

    fn end_of(steps: &[TrackStep]) -> (f64, f64) {
        let (mut x, mut z) = (0.0, 0.0);
        let mut heading = PI / 2.0;
        for step in steps {
            match *step {
                TrackStep::Straight(l) => {
                    x += heading.sin() * l;
                    z += heading.cos() * l;
                }
                TrackStep::Corner(degrees, radius) => {
                    let turn = degrees * PI / 180.0;
                    let side = sign(turn);
                    let cx = x + side * radius * heading.cos();
                    let cz = z - side * radius * heading.sin();
                    heading += turn;
                    x = cx - side * radius * heading.cos();
                    z = cz + side * radius * heading.sin();
                }
            }
        }
        (x, z)
    }

    fn is_clear_of_itself(points: &[(f64, f64)], clearance: f64) -> bool {
        let step = 4;
        let pts: Vec<(f64, f64)> = points.iter().step_by(step).copied().collect();
        let n = pts.len();
        let apart = (((clearance * 2.4).max(140.0) / 2.0) / step as f64).ceil() as usize;
        for i in 0..n {
            for j in i + apart..n {
                if n - (j - i) < apart {
                    continue;
                }
                if (pts[i].0 - pts[j].0).powi(2) + (pts[i].1 - pts[j].1).powi(2) < clearance * clearance {
                    return false;
                }
            }
        }
        true
    }

    /// Splits a loop of flags into runs of neighboring set samples, each including the sample after it.
    fn runs_of(flags: &[bool]) -> Vec<Vec<usize>> {
        let n = flags.len();
        if flags.iter().all(|f| *f) {
            return vec![(0..=n).map(|i| i % n).collect()];
        }
        let start = flags.iter().position(|f| !f).unwrap_or(0);
        let mut runs = Vec::new();
        let mut run = Vec::new();
        for k in 1..=n {
            let i = (start + k) % n;
            if flags[i] {
                run.push(i);
            } else if !run.is_empty() {
                run.push(i);
                runs.push(std::mem::take(&mut run));
            }
        }
        runs
    }

    // ---------- textures ----------

    /// Race track asphalt running along V, with a white line down each edge.
    pub fn track_texture() -> Canvas {
        let mut rand = Seeded::new(21.0);
        let mut c = Canvas::filled(256, 256, Rgba::hex(0x2f3135));
        for _ in 0..2200 {
            let v = 38 + (rand.next() * 34.0).floor() as u8;
            let (x, y) = (rand.next() as f32 * 256.0, rand.next() as f32 * 256.0);
            c.fill_rect(x, y, 1.5, 1.5, Rgba::rgb8(v, v, v + 4, 0.5));
        }
        c.fill_rect(70.0, 0.0, 116.0, 256.0, Rgba::rgb8(10, 10, 12, 0.25));
        c.fill_rect(4.0, 0.0, 6.0, 256.0, Rgba::hex(0xeeeeee));
        c.fill_rect(246.0, 0.0, 6.0, 256.0, Rgba::hex(0xeeeeee));
        c
    }

    /// Red and white curb stripes across V.
    pub fn curb_texture() -> Canvas {
        let mut c = Canvas::filled(32, 64, Rgba::hex(0xd7263d));
        c.fill_rect(0.0, 32.0, 32.0, 32.0, Rgba::hex(0xf4f4f4));
        c
    }

    pub fn checker_texture() -> Canvas {
        let mut c = Canvas::new(256, 32);
        for x in 0..16 {
            for y in 0..2 {
                let color = if (x + y) % 2 == 0 { 0x111111 } else { 0xf4f4f4 };
                c.fill_rect(x as f32 * 16.0, y as f32 * 16.0, 16.0, 16.0, Rgba::hex(color));
            }
        }
        c
    }

    /// Barrier paint: blocks of two colors along U, with a dark base.
    pub fn wall_texture(colors: [&str; 2]) -> Canvas {
        let mut c = Canvas::filled(128, 64, Rgba::css(colors[0]));
        c.fill_rect(64.0, 0.0, 64.0, 64.0, Rgba::css(colors[1]));
        c.fill_rect(0.0, 56.0, 128.0, 8.0, Rgba(0.0, 0.0, 0.0, 0.25));
        c
    }

    /// A see-through diamond wire mesh.
    pub fn fence_texture() -> Canvas {
        let mut c = Canvas::new(64, 64);
        let wire = Rgba::rgb8(200, 205, 212, 0.95);
        let mut k = -64.0;
        while k <= 64.0 {
            c.line(k, 0.0, k + 64.0, 64.0, 2.0, wire);
            c.line(k + 64.0, 0.0, k, 64.0, 2.0, wire);
            k += 16.0;
        }
        c.fill_rect(0.0, 0.0, 64.0, 3.0, Rgba::rgb8(160, 165, 170, 1.0));
        c
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn tracks_close_up() {
            let o = LayoutOptions {
                length: 2100.0,
                radius: [30.0, 110.0],
                corners: [8.0, 12.0],
                left_only: false,
                clearance: 64.0,
            };
            for seed in [1.0, 7.0, 1031.0] {
                let pts = draw_track(&generate_layout(seed, &o));
                let (a, b) = (pts[0], pts[pts.len() - 1]);
                assert!((a.0 - b.0).hypot(a.1 - b.1) < 4.0, "seed {seed} doesn't close");
            }
        }
    }
}

pub mod venues {
    //! The places you can drive: the city and the race track venues. Each venue
    //! has a look (sky, ground, trees, weather, scenery) and a race track style.
    //!
    //! A venue's main track (layout 0) is the one you drive in free roam. Any
    //! other layout number makes a different track with `generate_layout`, so
    //! career levels and sandbox races can use endless tracks.
    use crate::track::{LayoutOptions, TrackStep, TrackStyle, generate_layout};

    pub const MAP_NAMES: &[&str] = &["city", "circuit", "oval", "canyon", "alpine", "neon", "forest", "kart"];

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Ground {
        Grass,
        Sand,
        Snow,
        Dirt,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Trees {
        Round,
        Pine,
        Cactus,
        SnowPine,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Weather {
        Clear,
        Rain,
        Snow,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Scenery {
        Paddock,
        Mesas,
        Mountains,
        City,
        Woods,
    }

    /// What a venue looks like.
    #[derive(Clone, Debug)]
    pub struct Theme {
        /// Sky gradient, top to bottom, in the daytime.
        pub sky: [&'static str; 2],
        pub fog: u32,
        pub ground: Ground,
        pub hills: u32,
        pub trees: Trees,
        pub night: bool,
        pub weather: Weather,
        /// How grippy the road is: 1 is dry asphalt.
        pub grip: f64,
        pub scenery: Scenery,
        pub banner: &'static str,
    }

    pub struct Venue {
        pub value: &'static str,
        pub label: &'static str,
        pub about: &'static str,
        pub theme: Theme,
        pub style: Option<TrackStyle>,
        pub layout: Option<Vec<TrackStep>>,
        pub generate: Option<LayoutOptions>,
        pub seed: Option<f64>,
    }

    const DAY: Theme = Theme {
        sky: ["#5fa8e8", "#cfe5f5"],
        fog: 0xc4dcef,
        ground: Ground::Grass,
        hills: 0x55734a,
        trees: Trees::Round,
        night: false,
        weather: Weather::Clear,
        grip: 1.0,
        scenery: Scenery::Paddock,
        banner: "GRAND PRIX",
    };

    /// The speedway's turn radius.
    pub const OVAL_RADIUS: f64 = 220.0;

    fn circuit() -> Vec<TrackStep> {
        use TrackStep::*;
        vec![
            Straight(238.42),
            Corner(110.0, 40.0),
            Straight(120.0),
            Corner(-50.0, 80.0),
            Straight(90.0),
            Corner(120.0, 50.0),
            Straight(120.0),
            Corner(90.0, 40.0),
            Straight(130.0),
            Corner(-180.0, 32.0),
            Straight(40.0),
            Corner(90.0, 35.0),
            Straight(60.0),
            Corner(-40.0, 50.0),
            Corner(40.0, 50.0),
            Straight(60.0),
            Corner(90.0, 45.0),
            Straight(145.14),
            Corner(30.0, 40.0),
            Corner(-30.0, 40.0),
            Straight(50.0),
            Corner(90.0, 35.0),
            Straight(200.0),
        ]
    }

    fn style(
        width: f64,
        bank: [f64; 2],
        curbs: bool,
        outside: f64,
        inside: Option<f64>,
        apron: f64,
        wall: f64,
        fence: f64,
        colors: [&'static str; 2],
    ) -> TrackStyle {
        TrackStyle {
            width,
            bank,
            curbs,
            outside_runoff: outside,
            inside_runoff: inside,
            no_walls: false,
            apron,
            wall_height: wall,
            fence,
            wall_colors: colors,
        }
    }

    fn layout_options(length: f64, radius: [f64; 2], corners: [f64; 2], clearance: f64) -> LayoutOptions {
        LayoutOptions {
            length,
            radius,
            corners,
            left_only: false,
            clearance,
        }
    }

    pub fn venues() -> &'static [Venue] {
        use std::sync::OnceLock;
        static VENUES: OnceLock<Vec<Venue>> = OnceLock::new();
        VENUES.get_or_init(|| {
            use TrackStep::*;
            vec![
                Venue {
                    value: "city",
                    label: "City",
                    about: "A huge open world: the city, then highways out to towns, mountains, desert, lakes and forest. Get out and walk with F.",
                    theme: Theme {
                        scenery: Scenery::City,
                        ..DAY
                    },
                    style: None,
                    layout: None,
                    generate: None,
                    seed: None,
                },
                Venue {
                    value: "circuit",
                    label: "Grand Prix circuit",
                    about: "A twisty 2 km track: fast sweepers, a hairpin, esses and a chicane, with curbs and grandstands.",
                    theme: DAY,
                    style: Some(style(16.0, [0.0, 0.0], true, 9.0, Some(9.0), 0.0, 1.1, 0.0, ["#d7263d", "#f2f2f2"])),
                    layout: Some(circuit()),
                    generate: Some(layout_options(2100.0, [30.0, 110.0], [8.0, 12.0], 64.0)),
                    seed: None,
                },
                Venue {
                    value: "oval",
                    label: "Speedway",
                    about: "A 3 km oval with turns banked at 26°. Built for 500 mph.",
                    theme: Theme { banner: "SPEEDWAY", ..DAY },
                    style: Some(style(24.0, [5.0, 26.0], false, 0.6, None, 10.0, 1.2, 5.0, ["#f4f4f4", "#2b6fd6"])),
                    layout: Some(vec![
                        Straight(400.0),
                        Corner(180.0, OVAL_RADIUS),
                        Straight(800.0),
                        Corner(180.0, OVAL_RADIUS),
                        Straight(400.0),
                    ]),
                    generate: Some(LayoutOptions {
                        left_only: true,
                        ..layout_options(2700.0, [150.0, 320.0], [4.0, 6.0], 120.0)
                    }),
                    seed: None,
                },
                Venue {
                    value: "canyon",
                    label: "Red Rock Canyon",
                    about: "A long, fast desert track between towering red mesas. Sand everywhere off the road.",
                    theme: Theme {
                        sky: ["#3d8fd6", "#f6d7a8"],
                        fog: 0xf0cf9e,
                        ground: Ground::Sand,
                        hills: 0xb5552b,
                        trees: Trees::Cactus,
                        night: false,
                        weather: Weather::Clear,
                        grip: 1.0,
                        scenery: Scenery::Mesas,
                        banner: "CANYON RUN",
                    },
                    style: Some(style(14.0, [0.0, 4.0], true, 7.0, Some(7.0), 0.0, 1.2, 0.0, ["#a0522d", "#e8c39e"])),
                    layout: None,
                    generate: Some(layout_options(2600.0, [28.0, 140.0], [9.0, 13.0], 56.0)),
                    seed: Some(3.0),
                },
                Venue {
                    value: "alpine",
                    label: "Alpine Pass",
                    about: "Snowy mountain roads in falling snow. The road is slippery, so brake early.",
                    theme: Theme {
                        sky: ["#8fb4d9", "#e9f1f8"],
                        fog: 0xe4edf5,
                        ground: Ground::Snow,
                        hills: 0xdfe7ef,
                        trees: Trees::SnowPine,
                        night: false,
                        weather: Weather::Snow,
                        grip: 0.8,
                        scenery: Scenery::Mountains,
                        banner: "ALPINE PASS",
                    },
                    style: Some(style(13.0, [0.0, 6.0], true, 6.0, Some(6.0), 0.0, 1.0, 0.0, ["#2b6fd6", "#f2f2f2"])),
                    layout: None,
                    generate: Some(layout_options(2200.0, [25.0, 100.0], [10.0, 14.0], 55.0)),
                    seed: Some(6.0),
                },
                Venue {
                    value: "neon",
                    label: "Neon Night",
                    about: "A tight street circuit through a city that never sleeps. Concrete walls, no room for mistakes.",
                    theme: Theme {
                        sky: ["#0a0f24", "#2a1a48"],
                        fog: 0x1a1430,
                        ground: Ground::Dirt,
                        hills: 0x1b1d2b,
                        trees: Trees::Round,
                        night: true,
                        weather: Weather::Clear,
                        grip: 1.0,
                        scenery: Scenery::City,
                        banner: "NEON NIGHT",
                    },
                    style: Some(style(14.0, [0.0, 0.0], true, 2.5, Some(2.5), 0.0, 1.3, 2.5, ["#ff3bd0", "#2b2e33"])),
                    layout: None,
                    generate: Some(layout_options(1900.0, [18.0, 60.0], [10.0, 14.0], 52.0)),
                    seed: Some(7.0),
                },
                Venue {
                    value: "forest",
                    label: "Forest Rally",
                    about: "A narrow rally stage through the woods, in the rain. No barriers, just trees.",
                    theme: Theme {
                        sky: ["#6f7f8c", "#b9c4cc"],
                        fog: 0xaab6bf,
                        ground: Ground::Grass,
                        hills: 0x3d5a35,
                        trees: Trees::Pine,
                        night: false,
                        weather: Weather::Rain,
                        grip: 0.85,
                        scenery: Scenery::Woods,
                        banner: "FOREST RALLY",
                    },
                    style: Some(TrackStyle {
                        no_walls: true,
                        ..style(11.0, [0.0, 3.0], false, 0.0, Some(0.0), 0.0, 1.0, 0.0, ["#6b4a2f", "#d9c9a3"])
                    }),
                    layout: None,
                    generate: Some(layout_options(2100.0, [22.0, 90.0], [10.0, 15.0], 50.0)),
                    seed: Some(5.0),
                },
                Venue {
                    value: "kart",
                    label: "Kart Park",
                    about: "A short, tight, twisty track with tire walls. Corners come thick and fast.",
                    theme: Theme {
                        banner: "KART PARK",
                        ..DAY
                    },
                    style: Some(style(9.0, [0.0, 0.0], true, 3.0, Some(3.0), 0.0, 0.8, 0.0, ["#1a1a1a", "#f2f2f2"])),
                    layout: None,
                    generate: Some(layout_options(950.0, [12.0, 35.0], [10.0, 16.0], 34.0)),
                    seed: Some(3.0),
                },
            ]
        })
    }

    /// The venue with this name (the city if there's none).
    pub fn venue(map: &str) -> &'static Venue {
        venues().iter().find(|v| v.value == map).unwrap_or(&venues()[0])
    }

    /// The track for a venue's layout number: its main track for 0, otherwise a generated one.
    pub fn track_layout(v: &Venue, layout: u32) -> Vec<TrackStep> {
        if layout == 0
            && let Some(l) = &v.layout
        {
            return l.clone();
        }
        let index = venues().iter().position(|o| std::ptr::eq(o, v)).unwrap_or(0) as f64;
        let seed = if layout == 0 {
            v.seed.unwrap_or(1.0)
        } else {
            1000.0 + layout as f64 * 31.0 + index * 100003.0
        };
        generate_layout(seed, v.generate.as_ref().expect("a race venue"))
    }
}
