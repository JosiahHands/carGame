//! Sidewalk routes, storefronts and residents' front doors.
use std::collections::{HashMap, VecDeque};

use bevy::math::DVec2;
use game::world::{BlockKind, ROAD_LINES, World};

const LOOP: f64 = 32.0;
const SIDES: [DVec2; 4] = [DVec2::NEG_Y, DVec2::X, DVec2::Y, DVec2::NEG_X];

#[derive(Clone, Copy)]
pub struct Place {
    pub id: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
    pub at: DVec2,
}

#[derive(Default)]
pub struct CityMap {
    pub places: Vec<Place>,
    homes: Vec<DVec2>,
    corners: Vec<DVec2>,
    edges: Vec<Vec<(usize, f64)>>,
}

impl CityMap {
    pub fn build(world: &World) -> Self {
        let blocks = |kind| {
            let mut list: Vec<_> = world
                .blocks
                .iter()
                .filter(|b| b.kind == kind)
                .map(|b| DVec2::new(b.x, b.z))
                .collect();
            list.sort_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
            list
        };
        let homes = blocks(BlockKind::City);
        let shops = [
            ("coffee", "the coffee cart"),
            ("bank", "the bank"),
            ("diner", "the diner"),
            ("gym", "the gym"),
            ("music", "the music corner"),
            ("news", "the newsstand"),
            ("bakery", "the bakery"),
            ("library", "the library"),
            ("office", "the office"),
            ("market", "the market"),
        ];
        let mut places = Vec::new();
        if !homes.is_empty() {
            for (i, (kind, label)) in shops.into_iter().enumerate() {
                let block = homes[(i / 2) % homes.len()];
                places.push(Place {
                    id: kind,
                    kind,
                    label,
                    at: block + SIDES[(i * 3 + i / 2) % 4] * LOOP,
                });
            }
        }
        if let Some(block) = blocks(BlockKind::Lot).first().or_else(|| homes.get(4)) {
            places.push(Place {
                id: "garage",
                kind: "garage",
                label: "the garage",
                at: *block + DVec2::new(18.0, -28.0),
            });
        }
        for (i, block) in blocks(BlockKind::Park).into_iter().take(2).enumerate() {
            let (id, label, x) = if i == 0 {
                ("park0", "the park", -6.0)
            } else {
                ("park1", "the north park", 6.0)
            };
            places.push(Place {
                id,
                kind: "park",
                label,
                at: block + DVec2::new(x, 4.0),
            });
        }
        if let Some(block) = blocks(BlockKind::Stunt).first() {
            places.push(Place {
                id: "stunts",
                kind: "stunts",
                label: "the stunt park",
                at: *block + DVec2::new(-29.0, 0.0),
            });
        }

        let mut corners = Vec::new();
        let mut index = HashMap::new();
        for x in ROAD_LINES[..6].iter().map(|x| x + 40.0) {
            for z in ROAD_LINES[..6].iter().map(|z| z + 40.0) {
                for (dx, dz) in [(-LOOP, -LOOP), (LOOP, -LOOP), (LOOP, LOOP), (-LOOP, LOOP)] {
                    let at = DVec2::new(x + dx, z + dz);
                    index.insert((at.x as i32, at.y as i32), corners.len());
                    corners.push(at);
                }
            }
        }
        let edges = corners
            .iter()
            .map(|at| {
                let mut edges = Vec::new();
                for direction in SIDES {
                    for distance in [16.0, 64.0] {
                        let to = *at + direction * distance;
                        if let Some(i) = index.get(&(to.x as i32, to.y as i32)) {
                            edges.push((*i, distance));
                        }
                    }
                }
                edges
            })
            .collect();
        Self {
            places,
            homes,
            corners,
            edges,
        }
    }

    pub fn home(&self, id: &str) -> DVec2 {
        let hash = id.bytes().fold(2166136261_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
        if self.homes.is_empty() {
            return DVec2::ZERO;
        }
        let block = self.homes[hash as usize % self.homes.len()];
        let side = SIDES[((hash >> 8) % 4) as usize];
        let along = (f64::from((hash >> 12) % 1000) / 1000.0 - 0.5) * 52.0;
        block + side * LOOP + DVec2::new(side.y.abs(), side.x.abs()) * along
    }

    pub fn nearest_place(&self, at: DVec2) -> Option<&Place> {
        self.places
            .iter()
            .min_by(|a, b| a.at.distance_squared(at).total_cmp(&b.at.distance_squared(at)))
    }

    pub fn place(&self, kind: &str, at: DVec2) -> Option<Place> {
        self.places
            .iter()
            .filter(|p| p.id == kind || p.kind == kind)
            .min_by(|a, b| a.at.distance_squared(at).total_cmp(&b.at.distance_squared(at)))
            .copied()
    }

    fn nearest_corner(&self, at: DVec2) -> usize {
        self.corners
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
            .map_or(0, |(i, _)| i)
    }

    pub fn wander(&self, at: DVec2) -> DVec2 {
        let offset = DVec2::new(game::rng::centered(), game::rng::centered()) * 160.0;
        self.corners.get(self.nearest_corner(at + offset)).copied().unwrap_or(at)
    }

    pub fn route(&self, from: DVec2, to: DVec2) -> VecDeque<DVec2> {
        if from.distance_squared(to) < 40.0_f64.powi(2) || self.corners.is_empty() {
            return VecDeque::from([to]);
        }
        let start = self.nearest_corner(from);
        let goal = self.nearest_corner(to);
        let mut dist = vec![f64::INFINITY; self.corners.len()];
        let mut previous = vec![None; self.corners.len()];
        let mut done = vec![false; self.corners.len()];
        dist[start] = 0.0;
        while let Some(i) = (0..dist.len())
            .filter(|&i| !done[i] && dist[i].is_finite())
            .min_by(|&a, &b| dist[a].total_cmp(&dist[b]))
        {
            if i == goal {
                break;
            }
            done[i] = true;
            for &(next, cost) in &self.edges[i] {
                if dist[i] + cost < dist[next] {
                    dist[next] = dist[i] + cost;
                    previous[next] = Some(i);
                }
            }
        }
        let mut path = VecDeque::from([to]);
        let mut node = Some(goal);
        while let Some(i) = node {
            path.push_front(self.corners[i]);
            node = previous[i];
        }
        path
    }
}
