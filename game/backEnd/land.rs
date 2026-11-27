//! The open world around the city, for free roam on the City map.
//!
//! The land is a lattice of places 2 km apart: the city in the middle, then
//! towns, villages and crossroads, joined by highways that wind across the
//! country. Each direction has its own scenery: mountains with snowy peaks to
//! the north, red desert mesas to the west (+x: on the left facing north),
//! lakes and then the sea to the south, and pine forest to the east, with
//! farmland and woods in between.
//!
//! The big roads through the city, east to west and north to south, are
//! interstates: two lanes each way with a grass median. Hamlets of a few
//! houses sit along the other highways. The south ends at the sea, with
//! beaches and Bayport, a big city on the coast.
//!
//! The made map reaches 5 km from the city each way, and ends in a ring of
//! mountains (and the sea to the south). With the generative map on, there's no end: places, roads and
//! scenery past the made map are made from the place's own number as you get
//! near, so the same spot always looks the same.
//!
//! The ground is a height for every point. It's flat (height 0) along the
//! roads and in towns, so everything built there stands on level ground, and
//! rises into hills and mountains away from them. Heights are worked out in
//! chunks as the car gets near: an 8 m grid close by, a 32 m one far away.
//!
//! This file is the land's rules: where things are and how high the ground
//! is. world_build.rs dresses it with trees, buildings and towns, and
//! world_render.rs draws the ground and the roads.
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use bevy::math::DVec2;

/// Meters between neighboring places.
pub const SPACING: f64 = 2000.0;
/// The made map reaches this far from the city each way.
pub const MAP_HALF: f64 = 5000.0;
/// The made map's places: this many either side of the city.
pub const MAP_NODES: i32 = 2;
/// Ground heights are worked out in squares this big.
pub const CHUNK: f64 = 256.0;
/// Close by, a height every 8 m...
pub const FINE: usize = 32;
/// ...and far away, every 32 m.
pub const COARSE: usize = 8;
pub const ROAD_WIDTH: f64 = 14.0;
/// Traffic drives this far to the right of a highway's middle.
pub const LANE: f64 = 3.5;
/// The water in lakes is at this height.
pub const WATER: f64 = -0.6;
/// Town blocks are this big, street to street (like the city's).
pub const BLOCK: f64 = 80.0;
/// The city's square, where its own streets end.
pub const CITY_EDGE: f64 = 252.0;
const ROAD_STEP: f64 = 6.0;
const ROAD_CELL: f64 = 64.0;
/// The ground starts to rise this far from a road's middle...
const ROAD_CLEAR: f64 = 13.0;
/// ...and this far outside a town's square.
const TOWN_CLEAR: f64 = 22.0;
/// How far from a road (or town) the ground can still be pulled flat.
const FLATTEN_REACH: f64 = 260.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    Plains,
    Forest,
    Mountains,
    Desert,
    Lakes,
}

pub const BIOMES: [Biome; 5] = [Biome::Plains, Biome::Forest, Biome::Mountains, Biome::Desert, Biome::Lakes];

impl Biome {
    pub fn name(self) -> &'static str {
        match self {
            Biome::Plains => "farmland",
            Biome::Forest => "pine forest",
            Biome::Mountains => "mountains",
            Biome::Desert => "desert",
            Biome::Lakes => "lakes",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    City,
    /// A big city of skyscrapers, ten blocks across.
    Metropolis,
    Town,
    Village,
    Crossroads,
}

/// A few houses beside a highway, between places.
#[derive(Clone, Debug)]
pub struct Hamlet {
    pub name: String,
    pub at: DVec2,
    /// The road's direction there.
    pub dir: DVec2,
    pub built: bool,
}

/// Hamlets are flat this far around their middle.
pub const HAMLET_RADIUS: f64 = 50.0;
/// An interstate's half width (two carriageways and a median); a highway's is 7 m.
pub const INTERSTATE_HALF: f64 = 12.5;

/// A place on the lattice: the city, a town, a village or a crossroads.
#[derive(Clone, Debug)]
pub struct Town {
    pub key: (i32, i32),
    pub name: String,
    pub at: DVec2,
    pub size: Size,
    pub biome: Biome,
    /// The streets, as offsets from the middle (the same both ways).
    pub lines: Vec<f64>,
    /// Half the town's square: its streets reach this far.
    pub half: f64,
    /// Its buildings and trees are up (see world_build.rs).
    pub built: bool,
    /// The roads that start or end here.
    pub roads: Vec<usize>,
}

impl Town {
    /// How far outside the town's square (x, z) is (0 inside).
    pub fn distance(&self, x: f64, z: f64) -> f64 {
        let dx = (x - self.at.x).abs() - self.half;
        let dz = (z - self.at.y).abs() - self.half;
        dx.max(dz).max(0.0)
    }

    pub fn contains(&self, x: f64, z: f64, margin: f64) -> bool {
        (x - self.at.x).abs() <= self.half + margin && (z - self.at.y).abs() <= self.half + margin
    }

    /// The middle of each block, as offsets from the town's middle.
    pub fn blocks(&self) -> Vec<DVec2> {
        let mut out = Vec::new();
        for w in self.lines.windows(2) {
            for v in self.lines.windows(2) {
                out.push(self.at + DVec2::new((w[0] + w[1]) / 2.0, (v[0] + v[1]) / 2.0));
            }
        }
        out
    }

    pub fn label(&self) -> String {
        match self.size {
            Size::Crossroads => format!("{} crossroads", self.name),
            _ => self.name.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RoadPoint {
    pub x: f64,
    pub z: f64,
    /// Direction of travel (from `from` to `to`), length 1.
    pub dx: f64,
    pub dz: f64,
    pub along: f64,
}

/// A highway between two places, as points every 6 m.
#[derive(Clone, Debug)]
pub struct Road {
    pub from: usize,
    pub to: usize,
    pub points: Vec<RoadPoint>,
    pub length: f64,
    /// Two lanes each way (see `lanes`).
    pub interstate: bool,
    /// Half its width.
    pub half: f64,
    /// Every fourth point, for working out how far the ground is from the road.
    coarse: Vec<DVec2>,
    /// The road's extent: min x, max x, min z, max z.
    pub bounds: [f64; 4],
}

impl Road {
    /// Where traffic drives, meters right of the middle: one lane each way,
    /// or the inner and outer lanes of an interstate's carriageway.
    pub fn lanes(&self) -> &'static [f64] {
        if self.interstate { &[4.2, 8.6] } else { &[LANE] }
    }

    /// Where on the road `along` meters from its start, `offset` meters to
    /// the right (as seen driving from start to end): (x, z, heading).
    pub fn pose(&self, along: f64, offset: f64) -> (f64, f64, f64) {
        let along = along.clamp(0.0, self.length);
        let i = self
            .points
            .partition_point(|p| p.along <= along)
            .saturating_sub(1)
            .min(self.points.len().saturating_sub(2));
        let (a, b) = (self.points[i], self.points[(i + 1).min(self.points.len() - 1)]);
        let span = (b.along - a.along).max(1e-6);
        let t = ((along - a.along) / span).clamp(0.0, 1.0);
        let (dx, dz) = {
            let (x, z) = (a.dx + (b.dx - a.dx) * t, a.dz + (b.dz - a.dz) * t);
            let l = x.hypot(z).max(1e-9);
            (x / l, z / l)
        };
        let x = a.x + (b.x - a.x) * t;
        let z = a.z + (b.z - a.z) * t;
        // Right of the direction (dx, dz) is (-dz, dx) with +x to the left as seen from behind.
        (x - dz * offset, z + dx * offset, dx.atan2(dz))
    }

    /// The point nearest (x, z): (along, signed offset to the right, distance).
    pub fn nearest(&self, x: f64, z: f64) -> (f64, f64, f64) {
        let mut best = (0.0, 0.0, f64::INFINITY);
        for w in self.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (sx, sz) = (b.x - a.x, b.z - a.z);
            let len2 = (sx * sx + sz * sz).max(1e-9);
            let t = (((x - a.x) * sx + (z - a.z) * sz) / len2).clamp(0.0, 1.0);
            let (px, pz) = (a.x + sx * t, a.z + sz * t);
            let d = (x - px).hypot(z - pz);
            if d < best.2 {
                let side = (x - px) * -a.dz + (z - pz) * a.dx;
                best = (a.along + (b.along - a.along) * t, side.signum() * d, d);
            }
        }
        best
    }
}

/// The open world: places, roads and ground heights.
pub struct Land {
    pub generative: bool,
    pub towns: Vec<Town>,
    town_at: HashMap<(i32, i32), usize>,
    pub roads: Vec<Road>,
    edges: HashSet<((i32, i32), (i32, i32))>,
    /// Road segments in each 64 m square: (road, point index).
    cells: HashMap<(i32, i32), Vec<(u32, u32)>>,
    fine: HashMap<(i32, i32), Arc<Vec<f32>>>,
    coarse: HashMap<(i32, i32), Arc<Vec<f32>>>,
    /// Squares (see `TILE`) whose trees and scenery are up.
    pub tiles: HashSet<(i32, i32)>,
    /// Counts up whenever places or roads are added (so the screen draws them).
    pub version: u64,
    /// The lattice squares looked at so far.
    seen: HashSet<(i32, i32)>,
    pub hamlets: Vec<Hamlet>,
}

/// Trees and scenery are added in squares this big.
pub const TILE: f64 = 250.0;

pub fn tile_of(x: f64, z: f64) -> (i32, i32) {
    ((x / TILE).floor() as i32, (z / TILE).floor() as i32)
}

pub fn chunk_of(x: f64, z: f64) -> (i32, i32) {
    ((x / CHUNK).floor() as i32, (z / CHUNK).floor() as i32)
}

// ---------- numbers made from places ----------

/// A well-mixed number from a few whole numbers (splitmix64).
pub fn hash(a: i64, b: i64, c: i64) -> u64 {
    let mut x = (a as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add((c as u64).wrapping_mul(0x1656_67B1_9E37_79F9))
        .wrapping_add(0x2545_F491_4F6C_DD1D);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// 0..1 from a few whole numbers.
pub fn hash01(a: i64, b: i64, c: i64) -> f64 {
    (hash(a, b, c) >> 11) as f64 / (1u64 << 53) as f64
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

pub fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    smooth(((x - a) / (b - a)).clamp(0.0, 1.0))
}

/// Smooth noise, -1..1, changing over about one unit.
pub fn noise(x: f64, z: f64, seed: i64) -> f64 {
    let (ix, iz) = (x.floor(), z.floor());
    let (fx, fz) = (smooth(x - ix), smooth(z - iz));
    let (ix, iz) = (ix as i64, iz as i64);
    let v = |i: i64, j: i64| hash01(i, j, seed) * 2.0 - 1.0;
    let a = v(ix, iz) + (v(ix + 1, iz) - v(ix, iz)) * fx;
    let b = v(ix, iz + 1) + (v(ix + 1, iz + 1) - v(ix, iz + 1)) * fx;
    a + (b - a) * fz
}

/// Noise with finer detail on top, -1..1.
pub fn fbm(x: f64, z: f64, seed: i64, octaves: u32) -> f64 {
    let (mut sum, mut amp, mut freq, mut total) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += noise(x * freq + o as f64 * 17.3, z * freq - o as f64 * 9.1, seed + o as i64) * amp;
        total += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / total
}

/// Sharp ridges, 0..1, for mountain ranges.
fn ridged(x: f64, z: f64, seed: i64) -> f64 {
    let (mut sum, mut amp, mut freq, mut total) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..4 {
        let n = 1.0 - noise(x * freq, z * freq, seed + o).abs();
        sum += n * n * amp;
        total += amp;
        amp *= 0.5;
        freq *= 2.1;
    }
    sum / total
}

// ---------- the map's places ----------

const TOWN_NAMES: [&str; 40] = [
    "Ashford",
    "Bramble",
    "Cedar Falls",
    "Dunmore",
    "Elmwood",
    "Fairhaven",
    "Glenrock",
    "Hartley",
    "Ironbridge",
    "Juniper",
    "Kestrel",
    "Larkspur",
    "Millbrook",
    "Northgate",
    "Oakridge",
    "Pebble Creek",
    "Quarry Hill",
    "Rosewood",
    "Stonebury",
    "Thornton",
    "Upton",
    "Valemont",
    "Westbrook",
    "Yarrow",
    "Amberly",
    "Birchwood",
    "Copperton",
    "Driftwood",
    "Easton",
    "Foxhollow",
    "Granite",
    "Hollowell",
    "Indigo Bay",
    "Jasper",
    "Kingsley",
    "Lowell",
    "Maplewood",
    "Newbury",
    "Orchard",
    "Prairie View",
];

/// The made map's places: (lattice x, lattice z, size, name).
const MADE: [(i32, i32, Size, &str); 25] = [
    (0, 0, Size::City, "Haven City"),
    (0, 2, Size::Town, "Snowcap"),
    (2, 0, Size::Town, "Red Rock"),
    (0, -2, Size::Town, "Harbor Lake"),
    (-2, 0, Size::Town, "Pinewood"),
    (2, 2, Size::Village, "Coldwater"),
    (-2, 2, Size::Village, "Elk Pass"),
    (2, -2, Size::Village, "Cactus Flats"),
    (-2, -2, Size::Town, "Willow Bay"),
    (-1, 0, Size::Village, "Meadowbrook"),
    (0, 1, Size::Village, "Hillcrest"),
    (-1, 1, Size::Village, "Fern Hollow"),
    (1, -1, Size::Village, "Sundown"),
    (1, 0, Size::Crossroads, "Mile Ten"),
    (0, -1, Size::Crossroads, "Lakeview"),
    (1, 1, Size::Crossroads, "Ridgeway"),
    (-1, -1, Size::Crossroads, "Reedfield"),
    (2, 1, Size::Crossroads, "Canyon"),
    (2, -1, Size::Crossroads, "Dust Devil"),
    (-2, 1, Size::Crossroads, "Timber"),
    (-2, -1, Size::Crossroads, "Mossy Rock"),
    (1, 2, Size::Village, "Eagle Peak"),
    (-1, 2, Size::Crossroads, "Avalanche"),
    (1, -2, Size::Metropolis, "Bayport"),
    (-1, -2, Size::Village, "Sandpiper"),
];

/// The made map's roads that aren't there (the rest of the lattice is).
const MISSING: [((i32, i32), (i32, i32)); 6] = [
    ((1, 1), (2, 1)),
    ((-2, -1), (-1, -1)),
    ((1, -2), (2, -2)),
    ((-1, 1), (-1, 2)),
    ((1, -1), (1, 0)),
    ((-2, 1), (-2, 2)),
];

fn in_map(key: (i32, i32)) -> bool {
    key.0.abs() <= MAP_NODES && key.1.abs() <= MAP_NODES
}

fn streets(size: Size) -> (Vec<f64>, f64) {
    match size {
        Size::City => (crate::world::ROAD_LINES.to_vec(), CITY_EDGE),
        Size::Metropolis => ((-5..=5).map(|k| k as f64 * 80.0).collect(), 406.0),
        Size::Town => (vec![-160.0, -80.0, 0.0, 80.0, 160.0], 166.0),
        Size::Village => (vec![-80.0, 0.0, 80.0], 86.0),
        Size::Crossroads => (vec![0.0], 14.0),
    }
}

impl Land {
    pub fn new(generative: bool) -> Self {
        let mut land = Land {
            generative,
            towns: Vec::new(),
            town_at: HashMap::new(),
            roads: Vec::new(),
            edges: HashSet::new(),
            cells: HashMap::new(),
            fine: HashMap::new(),
            coarse: HashMap::new(),
            tiles: HashSet::new(),
            version: 0,
            seen: HashSet::new(),
            hamlets: Vec::new(),
        };
        // The city is always place 0.
        let city = land.make_town((0, 0));
        land.town_at.insert((0, 0), 0);
        land.towns.push(city);
        land.ensure(0.0, 0.0);
        land
    }

    /// Whether the lattice has a place at `key`.
    fn exists(&self, key: (i32, i32)) -> bool {
        in_map(key) || self.generative
    }

    fn make_town(&self, key: (i32, i32)) -> Town {
        let (size, name) = if let Some(m) = MADE.iter().find(|m| (m.0, m.1) == key) {
            (m.2, m.3.to_string())
        } else {
            let r = hash01(key.0 as i64, key.1 as i64, 1);
            let size = if r < 0.04 {
                Size::Metropolis
            } else if r < 0.22 {
                Size::Town
            } else if r < 0.55 {
                Size::Village
            } else {
                Size::Crossroads
            };
            let n = hash(key.0 as i64, key.1 as i64, 2) as usize;
            (size, TOWN_NAMES[n % TOWN_NAMES.len()].to_string())
        };
        let at = if key == (0, 0) {
            DVec2::ZERO
        } else {
            let jitter = if in_map(key) { 300.0 } else { 450.0 };
            DVec2::new(
                key.0 as f64 * SPACING + (hash01(key.0 as i64, key.1 as i64, 3) - 0.5) * 2.0 * jitter,
                key.1 as f64 * SPACING + (hash01(key.0 as i64, key.1 as i64, 4) - 0.5) * 2.0 * jitter,
            )
        };
        let (lines, half) = streets(size);
        Town {
            key,
            name,
            at,
            size,
            biome: self.main_biome(at.x, at.y),
            lines,
            half,
            built: key == (0, 0),
            roads: Vec::new(),
        }
    }

    /// Whether the lattice has a road between neighbors `a` and `b` (b is east or north of a).
    fn has_edge(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        if !self.exists(a) || !self.exists(b) {
            return false;
        }
        if in_map(a) && in_map(b) {
            return !MISSING.contains(&(a, b));
        }
        let horizontal = b.1 == a.1;
        // Every third row and column always goes through, so everywhere is reachable.
        if (horizontal && a.1.rem_euclid(3) == 0) || (!horizontal && a.0.rem_euclid(3) == 0) {
            return true;
        }
        hash01(a.0 as i64 * 2 + i64::from(horizontal), a.1 as i64, 5) < 0.6
    }

    /// Makes sure the places and roads within about 7 km of (x, z) exist.
    pub fn ensure(&mut self, x: f64, z: f64) {
        let ci = (x / SPACING).round() as i32;
        let cj = (z / SPACING).round() as i32;
        let reach = if self.generative { 4 } else { MAP_NODES };
        if self.seen.contains(&(ci, cj)) {
            return;
        }
        self.seen.insert((ci, cj));
        // Places one further out than the roads, so every road with an end
        // in range has both its places.
        let range = |c: i32| {
            if self.generative {
                (c - reach - 1)..=(c + reach + 1)
            } else {
                -MAP_NODES..=MAP_NODES
            }
        };
        let (ri, rj) = (range(ci), range(cj));
        let mut added = false;
        for i in ri.clone() {
            for j in rj.clone() {
                if self.exists((i, j)) && !self.town_at.contains_key(&(i, j)) {
                    let t = self.make_town((i, j));
                    self.town_at.insert((i, j), self.towns.len());
                    self.towns.push(t);
                    added = true;
                }
            }
        }
        for i in ri {
            for j in rj.clone() {
                for b in [(i + 1, j), (i, j + 1)] {
                    if self.edges.contains(&((i, j), b)) || !self.town_at.contains_key(&(i, j)) || !self.town_at.contains_key(&b) {
                        continue;
                    }
                    if self.has_edge((i, j), b) {
                        self.edges.insert(((i, j), b));
                        self.build_road((i, j), b);
                        added = true;
                    }
                }
            }
        }
        // A place that the lattice gives no road at all gets one towards the
        // city. (Only once all its neighbors are known, so it's the same
        // whichever way you come.)
        let lonely: Vec<usize> = (0..self.towns.len())
            .filter(|&t| {
                let (i, j) = self.towns[t].key;
                self.towns[t].roads.is_empty()
                    && [(i - 1, j), (i + 1, j), (i, j - 1), (i, j + 1)]
                        .iter()
                        .all(|k| self.town_at.contains_key(k) || !self.exists(*k))
            })
            .collect();
        for t in lonely {
            let key = self.towns[t].key;
            let toward = if key.0.abs() >= key.1.abs() {
                (key.0 - key.0.signum(), key.1)
            } else {
                (key.0, key.1 - key.1.signum())
            };
            if !self.town_at.contains_key(&toward) {
                continue;
            }
            let (a, b) = if toward.0 < key.0 || toward.1 < key.1 {
                (toward, key)
            } else {
                (key, toward)
            };
            if self.edges.insert((a, b)) {
                self.build_road(a, b);
                added = true;
            }
        }
        if added {
            self.version += 1;
        }
    }

    /// Where a road leaves a place's middle, in direction (dx, dz): the
    /// straight bit to the edge of its square and a little past it.
    fn exit(&self, t: usize, dir: DVec2) -> (DVec2, DVec2) {
        let town = &self.towns[t];
        let start = if town.size == Size::City {
            // The city's own streets go to its edge; the highway starts there.
            town.at + dir * CITY_EDGE
        } else {
            town.at
        };
        (start, town.at + dir * (town.half + 160.0))
    }

    /// Whether the road between neighbors `a` and `b` is an interstate: the
    /// rows and columns through the city (and every sixth one out past the
    /// made map).
    fn is_interstate(a: (i32, i32), b: (i32, i32)) -> bool {
        let horizontal = b.1 == a.1;
        let line = if horizontal { a.1 } else { a.0 };
        line == 0 || (line.rem_euclid(6) == 0 && !(in_map(a) && in_map(b)))
    }

    fn build_road(&mut self, a: (i32, i32), b: (i32, i32)) {
        let (ta, tb) = (self.town_at[&a], self.town_at[&b]);
        let dir = if b.1 == a.1 { DVec2::X } else { DVec2::Y };
        let interstate = Self::is_interstate(a, b);
        let (a0, a1) = self.exit(ta, dir);
        let (b0, b1) = self.exit(tb, -dir);
        let span = b1 - a1;
        let perp = DVec2::new(-span.y, span.x).normalize_or_zero();
        let amp = if interstate { 110.0 } else { 260.0 };
        let wobble = |k: i64| (hash01(a.0 as i64 * 31 + b.0 as i64, a.1 as i64 * 17 + b.1 as i64, k) - 0.5) * 2.0 * amp;
        let m1 = a1 + span / 3.0 + perp * wobble(6);
        let m2 = a1 + span * 2.0 / 3.0 + perp * wobble(7);
        let controls = [a1, m1, m2, b1];
        let tangents = [dir * (m1 - a1).length(), (m2 - a1) / 2.0, (b1 - m1) / 2.0, dir * (b1 - m2).length()];
        // The curve, sampled finely, then evened out to a point every 6 m.
        let mut dense = vec![a0];
        let straight = |from: DVec2, to: DVec2, out: &mut Vec<DVec2>| {
            let n = ((to - from).length() / 2.0).ceil().max(1.0) as usize;
            for k in 1..=n {
                out.push(from + (to - from) * (k as f64 / n as f64));
            }
        };
        straight(a0, a1, &mut dense);
        for s in 0..3 {
            let (p0, p1, m0, m1) = (controls[s], controls[s + 1], tangents[s], tangents[s + 1]);
            let n = 300;
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let (t2, t3) = (t * t, t * t * t);
                let p = p0 * (2.0 * t3 - 3.0 * t2 + 1.0) + m0 * (t3 - 2.0 * t2 + t) + p1 * (-2.0 * t3 + 3.0 * t2) + m1 * (t3 - t2);
                dense.push(p);
            }
        }
        straight(b1, b0, &mut dense);
        let mut points: Vec<DVec2> = vec![dense[0]];
        let mut carry = 0.0;
        for w in dense.windows(2) {
            let seg = w[1] - w[0];
            let len = seg.length();
            let mut at = ROAD_STEP - carry;
            while at <= len {
                points.push(w[0] + seg * (at / len));
                at += ROAD_STEP;
            }
            carry = len - (at - ROAD_STEP);
        }
        if points.last().is_some_and(|p| p.distance(b0) > 0.5) {
            points.push(b0);
        }
        let n = points.len();
        let mut road_points = Vec::with_capacity(n);
        let mut along = 0.0;
        for i in 0..n {
            if i > 0 {
                along += points[i].distance(points[i - 1]);
            }
            let d = (points[(i + 1).min(n - 1)] - points[i.saturating_sub(1)]).normalize_or(dir);
            road_points.push(RoadPoint {
                x: points[i].x,
                z: points[i].y,
                dx: d.x,
                dz: d.y,
                along,
            });
        }
        let mut bounds = [f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY];
        for p in &points {
            bounds = [bounds[0].min(p.x), bounds[1].max(p.x), bounds[2].min(p.y), bounds[3].max(p.y)];
        }
        let coarse = points.iter().step_by(4).copied().chain(std::iter::once(points[n - 1])).collect();
        let index = self.roads.len();
        let half = if interstate { INTERSTATE_HALF } else { ROAD_WIDTH / 2.0 };
        let margin = half + 7.0;
        for (i, w) in road_points.windows(2).enumerate() {
            let (lo_x, hi_x) = (w[0].x.min(w[1].x) - margin, w[0].x.max(w[1].x) + margin);
            let (lo_z, hi_z) = (w[0].z.min(w[1].z) - margin, w[0].z.max(w[1].z) + margin);
            for cx in (lo_x / ROAD_CELL).floor() as i32..=(hi_x / ROAD_CELL).floor() as i32 {
                for cz in (lo_z / ROAD_CELL).floor() as i32..=(hi_z / ROAD_CELL).floor() as i32 {
                    self.cells.entry((cx, cz)).or_default().push((index as u32, i as u32));
                }
            }
        }
        // A hamlet halfway along most highways (not the interstates).
        let roll = hash01(a.0 as i64 * 7 + b.0 as i64, a.1 as i64 * 5 + b.1 as i64, 9);
        if !interstate && roll < 0.6 && along > 900.0 {
            let k = ((0.35 + 0.3 * roll / 0.6) * (road_points.len() - 1) as f64) as usize;
            let p = road_points[k];
            let name = TOWN_NAMES[(hash(a.0 as i64, b.1 as i64, 10) as usize) % TOWN_NAMES.len()];
            let near_town = self.towns.iter().any(|t| t.distance(p.x, p.z) < 250.0);
            if !near_town {
                self.hamlets.push(Hamlet {
                    name: format!("{name} Corner"),
                    at: DVec2::new(p.x, p.z),
                    dir: DVec2::new(p.dx, p.dz),
                    built: false,
                });
            }
        }
        self.roads.push(Road {
            from: ta,
            to: tb,
            length: along,
            points: road_points,
            coarse,
            bounds,
            interstate,
            half,
        });
        self.towns[ta].roads.push(index);
        self.towns[tb].roads.push(index);
        // Roads change the ground near them.
        let (lo, hi) = (
            chunk_of(bounds[0] - FLATTEN_REACH, bounds[2] - FLATTEN_REACH),
            chunk_of(bounds[1] + FLATTEN_REACH, bounds[3] + FLATTEN_REACH),
        );
        self.fine.retain(|k, _| k.0 < lo.0 || k.0 > hi.0 || k.1 < lo.1 || k.1 > hi.1);
        self.coarse.retain(|k, _| k.0 < lo.0 || k.0 > hi.0 || k.1 < lo.1 || k.1 > hi.1);
    }

    // ---------- where things are ----------

    pub fn town(&self, key: (i32, i32)) -> Option<&Town> {
        self.town_at.get(&key).map(|&i| &self.towns[i])
    }

    /// The town whose square (x, z) is in, give or take `margin`.
    pub fn town_at(&self, x: f64, z: f64, margin: f64) -> Option<usize> {
        let key = ((x / SPACING).round() as i32, (z / SPACING).round() as i32);
        let mut best = None;
        for i in key.0 - 1..=key.0 + 1 {
            for j in key.1 - 1..=key.1 + 1 {
                if let Some(&t) = self.town_at.get(&(i, j))
                    && self.towns[t].contains(x, z, margin)
                {
                    best = Some(t);
                }
            }
        }
        best
    }

    /// The nearest place to (x, z), and how far outside its square that is.
    pub fn nearest_town(&self, x: f64, z: f64) -> Option<(usize, f64)> {
        let key = ((x / SPACING).round() as i32, (z / SPACING).round() as i32);
        let mut best: Option<(usize, f64)> = None;
        for i in key.0 - 1..=key.0 + 1 {
            for j in key.1 - 1..=key.1 + 1 {
                if let Some(&t) = self.town_at.get(&(i, j)) {
                    let d = self.towns[t].distance(x, z);
                    if best.is_none_or(|b| d < b.1) {
                        best = Some((t, d));
                    }
                }
            }
        }
        best
    }

    /// Roads with a segment within `reach` of (x, z): (road, nearest point index).
    pub fn roads_near(&self, x: f64, z: f64, reach: f64) -> Vec<usize> {
        let mut out = Vec::new();
        for cx in ((x - reach) / ROAD_CELL).floor() as i32..=((x + reach) / ROAD_CELL).floor() as i32 {
            for cz in ((z - reach) / ROAD_CELL).floor() as i32..=((z + reach) / ROAD_CELL).floor() as i32 {
                if let Some(list) = self.cells.get(&(cx, cz)) {
                    for &(r, _) in list {
                        if !out.contains(&(r as usize)) {
                            out.push(r as usize);
                        }
                    }
                }
            }
        }
        out
    }

    /// How far (x, z) is from the edge of the nearest road (negative on it),
    /// looking up to `reach` away.
    pub fn road_distance(&self, x: f64, z: f64, reach: f64) -> f64 {
        let mut best = f64::INFINITY;
        for cx in ((x - reach) / ROAD_CELL).floor() as i32..=((x + reach) / ROAD_CELL).floor() as i32 {
            for cz in ((z - reach) / ROAD_CELL).floor() as i32..=((z + reach) / ROAD_CELL).floor() as i32 {
                let Some(list) = self.cells.get(&(cx, cz)) else { continue };
                for &(r, i) in list {
                    let road = &self.roads[r as usize];
                    let pts = &road.points;
                    let (a, b) = (pts[i as usize], pts[(i as usize + 1).min(pts.len() - 1)]);
                    best = best.min(segment_distance(x, z, a.x, a.z, b.x, b.z) - road.half);
                }
            }
        }
        best
    }

    /// Whether (x, z) is on a road or in a town: driving there is at full speed.
    pub fn paved(&self, x: f64, z: f64) -> bool {
        self.town_at(x, z, 0.0).is_some() || self.road_distance(x, z, INTERSTATE_HALF + 2.0) <= 1.0
    }

    /// Whether trees and scenery can go at (x, z): off the roads, out of
    /// towns, out of the water, and `margin` meters clear of all of them.
    pub fn clear(&self, x: f64, z: f64, margin: f64) -> bool {
        if self.nearest_town(x, z).is_some_and(|(_, d)| d < margin + 12.0) {
            return false;
        }
        if self.road_distance(x, z, INTERSTATE_HALF + margin + 6.0) < margin + 6.0 {
            return false;
        }
        if self
            .hamlets
            .iter()
            .any(|h| h.at.distance(DVec2::new(x, z)) < HAMLET_RADIUS + margin)
        {
            return false;
        }
        if !self.generative && (x.abs() > MAP_HALF - 20.0 || z.abs() > MAP_HALF - 20.0) {
            return false;
        }
        self.ground(x, z) > WATER + 0.4
    }

    /// A route between two places along the roads (fewest roads first), as points.
    /// (Place 0 is the city.)
    pub fn route(&self, from: usize, to: usize) -> Option<Vec<DVec2>> {
        if from == to {
            return Some(vec![self.towns[to].at]);
        }
        let mut previous: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut queue = VecDeque::from([from]);
        let mut seen = HashSet::from([from]);
        while let Some(t) = queue.pop_front() {
            if t == to {
                break;
            }
            for &r in &self.towns[t].roads {
                let road = &self.roads[r];
                let next = if road.from == t { road.to } else { road.from };
                if seen.insert(next) {
                    previous.insert(next, (t, r));
                    queue.push_back(next);
                }
            }
        }
        if !previous.contains_key(&to) {
            return None;
        }
        let mut legs = Vec::new();
        let mut at = to;
        while at != from {
            let (back, r) = previous[&at];
            legs.push((r, back));
            at = back;
        }
        legs.reverse();
        let mut points = Vec::new();
        for (r, start) in legs {
            let road = &self.roads[r];
            let forward = road.from == start;
            let lane = road.lanes()[0];
            let offset = if forward { lane } else { -lane };
            let n = road.points.len();
            for k in (0..n).step_by(5).chain(std::iter::once(n - 1)) {
                let p = road.points[if forward { k } else { n - 1 - k }];
                let (x, z, _) = road.pose(p.along, offset);
                points.push(DVec2::new(x, z));
            }
        }
        points.push(self.towns[to].at);
        Some(points)
    }

    // ---------- scenery ----------

    /// How much of each biome there is at (x, z): plains, forest, mountains, desert, lakes.
    pub fn biomes(&self, x: f64, z: f64) -> [f64; 5] {
        let edge = x.abs().max(z.abs());
        let outside = if self.generative {
            smoothstep(MAP_HALF - 600.0, MAP_HALF + 1800.0, edge)
        } else {
            0.0
        };
        let made = if outside < 1.0 { made_biomes(x, z) } else { [0.0; 5] };
        if outside <= 0.0 {
            return made;
        }
        let wild = wild_biomes(x, z);
        let mut out = [0.0; 5];
        for i in 0..5 {
            out[i] = made[i] * (1.0 - outside) + wild[i] * outside;
        }
        out
    }

    pub fn main_biome(&self, x: f64, z: f64) -> Biome {
        let w = self.biomes(x, z);
        let mut best = 0;
        for i in 1..5 {
            if w[i] > w[best] {
                best = i;
            }
        }
        BIOMES[best]
    }

    /// The ground's height before roads and towns flatten it.
    pub fn wild_height(&self, x: f64, z: f64) -> f64 {
        let w = self.biomes(x, z);
        let base = fbm(x / 420.0, z / 420.0, 11, 4);
        let mut h = 0.0;
        if w[0] > 0.0 {
            h += w[0] * (6.0 * base + 2.0 * fbm(x / 90.0, z / 90.0, 12, 2));
        }
        if w[1] > 0.0 {
            h += w[1] * (14.0 + 22.0 * fbm(x / 300.0, z / 300.0, 13, 4));
        }
        if w[2] > 0.0 {
            let ridges = ridged(x / 1300.0, z / 1300.0, 14);
            h += w[2] * (35.0 + 300.0 * ridges.powf(1.6) + 25.0 * fbm(x / 160.0, z / 160.0, 15, 3));
        }
        if w[3] > 0.0 {
            // Mesas: flat-topped steps of red rock.
            let m = (fbm(x / 650.0, z / 650.0, 16, 3) * 0.5 + 0.5) * 4.0;
            let step = m.floor() + smoothstep(0.82, 1.0, m.fract());
            h += w[3] * (step * 22.0 + 3.0 * fbm(x / 120.0, z / 120.0, 17, 2) - 8.0);
        }
        if w[4] > 0.0 {
            let lake = fbm(x / 380.0, z / 380.0, 18, 4);
            h += w[4] * (if lake < 0.05 { -3.5 + lake * 4.0 } else { (lake - 0.05) * 30.0 });
        }
        if !self.generative {
            // The made map ends in a ring of mountains (but the sea to the south).
            let rim = smoothstep(MAP_HALF - 700.0, MAP_HALF - 80.0, x.abs().max(z));
            h = h * (1.0 - rim) + rim * (140.0 + 120.0 * ridged(x / 500.0, z / 500.0, 19));
        }
        // The sea: beaches, then deep water. (Out past the made map, on the
        // generative map, the land comes back as islands.)
        let shore = -3950.0 + 160.0 * noise(x / 900.0, 3.0, 23);
        let mut sea = smoothstep(shore, shore - 420.0, z);
        if self.generative {
            sea *= 1.0 - smoothstep(-6400.0, -7400.0, z);
        }
        if sea > 0.0 {
            // A gentle beach slope near the water's edge, then the deep.
            let beach = smoothstep(0.0, 0.35, sea);
            h = h * (1.0 - beach) + beach * (1.2 - 9.0 * smoothstep(0.3, 1.0, sea));
        }
        h
    }

    /// How much the roads and towns near (x, z) flatten the ground there (0 flat, 1 wild).
    fn wildness(&self, x: f64, z: f64, h: f64, roads: &[&Road], towns: &[&Town], hamlets: &[&Hamlet]) -> f64 {
        let fade = (25.0 + 0.75 * h.abs()).min(FLATTEN_REACH - ROAD_CLEAR);
        let mut d_road = f64::INFINITY;
        for r in roads {
            // Measured from a highway-wide road's edge, so wider roads are flat wider.
            let wider = r.half - ROAD_WIDTH / 2.0;
            for w in r.coarse.windows(2) {
                d_road = d_road.min(segment_distance(x, z, w[0].x, w[0].y, w[1].x, w[1].y) - wider);
            }
        }
        let mut d_town = f64::INFINITY;
        for t in towns {
            d_town = d_town.min(t.distance(x, z));
        }
        for hm in hamlets {
            d_town = d_town.min((hm.at.distance(DVec2::new(x, z)) - HAMLET_RADIUS).max(0.0));
        }
        smoothstep(ROAD_CLEAR, ROAD_CLEAR + fade, d_road) * smoothstep(TOWN_CLEAR, TOWN_CLEAR + fade, d_town)
    }

    fn nearby_for(&self, min_x: f64, max_x: f64, min_z: f64, max_z: f64) -> (Vec<&Road>, Vec<&Town>, Vec<&Hamlet>) {
        let reach = FLATTEN_REACH;
        let roads = self
            .roads
            .iter()
            .filter(|r| {
                r.bounds[0] - reach < max_x && r.bounds[1] + reach > min_x && r.bounds[2] - reach < max_z && r.bounds[3] + reach > min_z
            })
            .collect();
        let towns = self
            .towns
            .iter()
            .filter(|t| {
                let r = t.half + reach;
                t.at.x - r < max_x && t.at.x + r > min_x && t.at.y - r < max_z && t.at.y + r > min_z
            })
            .collect();
        let r = HAMLET_RADIUS + FLATTEN_REACH;
        let hamlets = self
            .hamlets
            .iter()
            .filter(|h| h.at.x - r < max_x && h.at.x + r > min_x && h.at.y - r < max_z && h.at.y + r > min_z)
            .collect();
        (roads, towns, hamlets)
    }

    /// The ground's height at (x, z), worked out from scratch.
    pub fn height(&self, x: f64, z: f64) -> f64 {
        let (roads, towns, hamlets) = self.nearby_for(x, x, z, z);
        let h = self.wild_height(x, z);
        h * self.wildness(x, z, h, &roads, &towns, &hamlets)
    }

    fn grid(&self, key: (i32, i32), cells: usize) -> Vec<f32> {
        let (x0, z0) = (key.0 as f64 * CHUNK, key.1 as f64 * CHUNK);
        let (roads, towns, hamlets) = self.nearby_for(x0, x0 + CHUNK, z0, z0 + CHUNK);
        let step = CHUNK / cells as f64;
        let mut out = Vec::with_capacity((cells + 1) * (cells + 1));
        for j in 0..=cells {
            for i in 0..=cells {
                let (x, z) = (x0 + i as f64 * step, z0 + j as f64 * step);
                let h = self.wild_height(x, z);
                out.push((h * self.wildness(x, z, h, &roads, &towns, &hamlets)) as f32);
            }
        }
        out
    }

    /// Works out ground heights near (x, z): an 8 m grid within `near`
    /// meters, a 32 m one within `far`, nearest first, for up to `budget` of
    /// time (the chunk under (x, z) and its neighbors always). Far-off ones
    /// are forgotten.
    pub fn prepare(&mut self, x: f64, z: f64, near: f64, far: f64, budget: std::time::Duration) {
        let started = std::time::Instant::now();
        let here = chunk_of(x, z);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let key = (here.0 + dx, here.1 + dz);
                if !self.fine.contains_key(&key) {
                    let g = self.grid(key, FINE);
                    self.fine.insert(key, Arc::new(g));
                }
            }
        }
        let mut wanted: Vec<(f64, (i32, i32), bool)> = Vec::new();
        let r = (far / CHUNK).ceil() as i32 + 1;
        for dx in -r..=r {
            for dz in -r..=r {
                let key = (here.0 + dx, here.1 + dz);
                let d = chunk_distance(key, x, z);
                if d <= near && !self.fine.contains_key(&key) {
                    wanted.push((d, key, true));
                } else if d <= far && !self.coarse.contains_key(&key) {
                    wanted.push((d, key, false));
                }
            }
        }
        wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, key, fine) in wanted {
            if started.elapsed() > budget {
                break;
            }
            if fine {
                let g = self.grid(key, FINE);
                self.fine.insert(key, Arc::new(g));
            } else {
                let g = self.grid(key, COARSE);
                self.coarse.insert(key, Arc::new(g));
            }
        }
        self.fine.retain(|k, _| chunk_distance(*k, x, z) < near * 2.0 + 600.0);
        self.coarse.retain(|k, _| chunk_distance(*k, x, z) < far * 1.5 + 1000.0);
    }

    /// The 8 m height grid of a chunk, if it's been worked out.
    pub fn fine_chunk(&self, key: (i32, i32)) -> Option<Arc<Vec<f32>>> {
        self.fine.get(&key).cloned()
    }

    /// The 32 m height grid of a chunk, if it's been worked out.
    pub fn coarse_chunk(&self, key: (i32, i32)) -> Option<Arc<Vec<f32>>> {
        self.coarse.get(&key).cloned()
    }

    /// The ground's height at (x, z): from the 8 m grid where it's ready.
    pub fn ground(&self, x: f64, z: f64) -> f64 {
        let key = chunk_of(x, z);
        let Some(g) = self.fine.get(&key) else {
            return self.height(x, z);
        };
        let step = CHUNK / FINE as f64;
        let (lx, lz) = ((x - key.0 as f64 * CHUNK) / step, (z - key.1 as f64 * CHUNK) / step);
        let (i, j) = ((lx.floor() as usize).min(FINE - 1), (lz.floor() as usize).min(FINE - 1));
        let (fx, fz) = (lx - i as f64, lz - j as f64);
        let at = |i: usize, j: usize| f64::from(g[j * (FINE + 1) + i]);
        // Two triangles per square, split the same way the ground mesh is.
        if fx + fz <= 1.0 {
            at(i, j) + (at(i + 1, j) - at(i, j)) * fx + (at(i, j + 1) - at(i, j)) * fz
        } else {
            at(i + 1, j + 1) + (at(i, j + 1) - at(i + 1, j + 1)) * (1.0 - fx) + (at(i + 1, j) - at(i + 1, j + 1)) * (1.0 - fz)
        }
    }

    /// The ground's color at (x, z), height `h`, with `slope` (0 flat, 1 a cliff). Linear RGB.
    pub fn ground_color(&self, x: f64, z: f64, h: f64, slope: f64) -> [f32; 3] {
        let w = self.biomes(x, z);
        let n = fbm(x / 60.0, z / 60.0, 21, 2);
        let mix = |a: [f64; 3], b: [f64; 3], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        let lin = |rgb: u32| {
            let c = |v: u32| {
                let s = f64::from(v & 0xff) / 255.0;
                if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
            };
            [c(rgb >> 16), c(rgb >> 8), c(rgb)]
        };
        let rock = lin(0x77736b);
        // Farmland: a patchwork of fields.
        let field = {
            let (fi, fj) = ((x / 110.0).floor() as i64, (z / 80.0).floor() as i64);
            match (hash01(fi, fj, 22) * 5.0) as u32 {
                0 => lin(0xc9b45c),
                1 => lin(0x8aa84e),
                2 => lin(0x9c7f4e),
                _ => mix(lin(0x6f9e48), lin(0x5c8c3d), n * 0.5 + 0.5),
            }
        };
        let plains = field;
        let forest = mix(lin(0x3c6531), lin(0x4a7a3a), n * 0.5 + 0.5);
        let snow_line = 150.0 + 30.0 * n;
        let mountains = if h > snow_line {
            lin(0xf1f4f7)
        } else {
            mix(
                mix(lin(0x5f7f4a), rock, smoothstep(40.0, 120.0, h)),
                lin(0xe8edf2),
                smoothstep(snow_line - 25.0, snow_line, h),
            )
        };
        let desert = mix(lin(0xd6b07a), lin(0xb3643c), smoothstep(8.0, 30.0, h) * 0.8 + n.max(0.0) * 0.2);
        let lakes = if h < WATER + 0.2 {
            lin(0x6d6650)
        } else if h < 1.4 {
            lin(0xd8c99a)
        } else {
            mix(lin(0x5f9a50), lin(0x4f8a45), n * 0.5 + 0.5)
        };
        let mut c = [0.0; 3];
        for (k, part) in [plains, forest, mountains, desert, lakes].iter().enumerate() {
            for i in 0..3 {
                c[i] += part[i] * w[k];
            }
        }
        // Steep ground is bare rock (or red rock in the desert).
        let cliff = if w[3] > 0.5 { lin(0xa2573a) } else { rock };
        let c = mix(
            c,
            cliff,
            smoothstep(0.45, 0.8, slope) * if h > snow_line && w[2] > 0.5 { 0.3 } else { 1.0 },
        );
        [c[0] as f32, c[1] as f32, c[2] as f32]
    }
}

fn chunk_distance(key: (i32, i32), x: f64, z: f64) -> f64 {
    let (x0, z0) = (key.0 as f64 * CHUNK, key.1 as f64 * CHUNK);
    let dx = (x0 - x).max(0.0).max(x - (x0 + CHUNK));
    let dz = (z0 - z).max(0.0).max(z - (z0 + CHUNK));
    dx.hypot(dz)
}

pub fn segment_distance(x: f64, z: f64, ax: f64, az: f64, bx: f64, bz: f64) -> f64 {
    let (sx, sz) = (bx - ax, bz - az);
    let len2 = sx * sx + sz * sz;
    let t = if len2 > 0.0 {
        (((x - ax) * sx + (z - az) * sz) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (x - (ax + sx * t)).hypot(z - (az + sz * t))
}

/// The made map's biomes: mountains north (+z), desert west (+x), lakes south, forest east (-x).
fn made_biomes(x: f64, z: f64) -> [f64; 5] {
    let wx = x + 500.0 * noise(x / 1400.0, z / 1400.0, 31);
    let wz = z + 500.0 * noise(x / 1400.0 + 40.0, z / 1400.0, 32);
    let mountains = smoothstep(800.0, 2600.0, wz);
    let desert = smoothstep(800.0, 2600.0, wx);
    let lakes = smoothstep(800.0, 2600.0, -wz);
    let forest = smoothstep(800.0, 2600.0, -wx);
    normalize([0.0, forest, mountains, desert, lakes])
}

/// Biomes past the made map, from slow noise: hot and dry is desert, wet is forest or lakes.
fn wild_biomes(x: f64, z: f64) -> [f64; 5] {
    let heat = noise(x / 5200.0, z / 5200.0, 41);
    let wet = noise(x / 4300.0 + 7.0, z / 4300.0, 42);
    let high = ridged(x / 6000.0, z / 6000.0, 43);
    let mountains = smoothstep(0.62, 0.8, high);
    let desert = smoothstep(0.15, 0.45, heat) * smoothstep(0.1, -0.3, wet) * (1.0 - mountains);
    let forest = smoothstep(0.05, 0.4, wet) * (1.0 - mountains) * (1.0 - desert);
    let lakes = smoothstep(-0.2, -0.5, heat) * smoothstep(0.0, 0.3, wet) * (1.0 - mountains) * (1.0 - desert);
    normalize([0.0, forest, mountains, desert, lakes])
}

/// Makes the weights add up to 1, with farmland filling any gap.
fn normalize(mut w: [f64; 5]) -> [f64; 5] {
    let sum: f64 = w[1..].iter().sum();
    if sum > 1.0 {
        for v in &mut w[1..] {
            *v /= sum;
        }
        w[0] = 0.0;
    } else {
        w[0] = 1.0 - sum;
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_made_map_has_its_places_and_roads() {
        let land = Land::new(false);
        assert_eq!(land.towns.len(), 25);
        assert!(land.roads.len() >= 30, "{} roads", land.roads.len());
        // Every place can be reached from the city.
        for t in 0..land.towns.len() {
            assert!(land.route(0, t).is_some(), "{} can't be reached", land.towns[t].name);
        }
        // The city has highways out on all four sides.
        assert_eq!(land.towns[0].roads.len(), 4);
    }

    #[test]
    fn roads_and_towns_are_flat_and_the_country_is_not() {
        let mut land = Land::new(false);
        for road in land.roads.iter().take(8) {
            for p in road.points.iter().step_by(25) {
                assert!(
                    land.height(p.x, p.z).abs() < 0.01,
                    "road at {},{} is {}",
                    p.x,
                    p.z,
                    land.height(p.x, p.z)
                );
            }
        }
        assert_eq!(land.height(10.0, 10.0), 0.0);
        // Mountains to the north rise well above the roads.
        let mut highest = 0.0f64;
        for i in 0..40 {
            let (x, z) = (-1500.0 + i as f64 * 75.0, 3300.0);
            highest = highest.max(land.height(x, z));
        }
        assert!(highest > 60.0, "highest {highest}");
        // The 8 m grid agrees with the heights worked out from scratch.
        land.prepare(900.0, 900.0, 300.0, 600.0, std::time::Duration::from_secs(5));
        for (x, z) in [(900.0, 900.0), (777.0, 1020.0), (1100.0, 850.0)] {
            assert!((land.ground(x, z) - land.height(x, z)).abs() < 1.5, "{x},{z}");
        }
    }

    #[test]
    fn the_generative_map_keeps_going_and_is_the_same_every_time() {
        let mut a = Land::new(true);
        a.ensure(14000.0, -9000.0);
        let mut b = Land::new(true);
        b.ensure(14000.0, -9000.0);
        assert!(a.town((7, -4)).is_some());
        assert_eq!(a.town((7, -4)).unwrap().name, b.town((7, -4)).unwrap().name);
        assert_eq!(a.height(14100.0, -9050.0), b.height(14100.0, -9050.0));
        let t = a.town_at.get(&(7, -4)).copied().unwrap();
        assert!(a.route(0, t).is_some());
        // Without it, nothing past the made map.
        let mut c = Land::new(false);
        c.ensure(14000.0, -9000.0);
        assert!(c.town((7, -4)).is_none());
    }
}
