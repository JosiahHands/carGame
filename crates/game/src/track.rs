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
