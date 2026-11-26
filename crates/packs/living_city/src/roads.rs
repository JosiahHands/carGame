//! Routes along the city's streets and parking beside the sidewalk.
use std::collections::VecDeque;

use bevy::math::DVec2;
use game::world::ROAD_LINES;

fn road(at: f64) -> f64 {
    ROAD_LINES
        .into_iter()
        .min_by(|a, b| (*a - at).abs().total_cmp(&(*b - at).abs()))
        .unwrap_or(0.0)
}

fn vertical(at: DVec2) -> bool {
    (road(at.x) - at.x).abs() <= (road(at.y) - at.y).abs()
}

pub fn curb(at: DVec2) -> (DVec2, f64) {
    if vertical(at) {
        let line = road(at.x);
        let side = if at.x >= line { 1.0 } else { -1.0 };
        (
            DVec2::new(line + 4.6 * side, at.y.clamp(-231.0, 231.0)),
            if side > 0.0 { 0.0 } else { std::f64::consts::PI },
        )
    } else {
        let line = road(at.y);
        let side = if at.y >= line { 1.0 } else { -1.0 };
        (
            DVec2::new(at.x.clamp(-231.0, 231.0), line + 4.6 * side),
            -side * std::f64::consts::FRAC_PI_2,
        )
    }
}

pub fn route(from: DVec2, to: DVec2) -> VecDeque<DVec2> {
    let a = DVec2::new(road(from.x), road(from.y));
    let b = DVec2::new(road(to.x), road(to.y));
    let corner = if vertical(from) {
        DVec2::new(a.x, b.y)
    } else {
        DVec2::new(b.x, a.y)
    };
    let mut path = VecDeque::new();
    for point in [a, corner, b, to] {
        if path.back() != Some(&point) && from.distance_squared(point) > 1.0 {
            path.push_back(point);
        }
    }
    path
}

pub fn pursuit_target(from: DVec2, to: DVec2) -> DVec2 {
    let a = DVec2::new(road(from.x), road(from.y));
    let b = DVec2::new(road(to.x), road(to.y));
    if a.distance_squared(from) < 11.0_f64.powi(2) {
        if (a.x - b.x).abs() > (a.y - b.y).abs() {
            DVec2::new(b.x, a.y)
        } else if a.y != b.y {
            DVec2::new(a.x, b.y)
        } else {
            to
        }
    } else if vertical(from) {
        let z = if (b.x - a.x).abs() < 0.01 { to.y } else { a.y };
        DVec2::new(a.x, z)
    } else {
        let x = if (b.y - a.y).abs() < 0.01 { to.x } else { a.x };
        DVec2::new(x, a.y)
    }
}
