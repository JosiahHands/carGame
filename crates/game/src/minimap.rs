//! The minimap in the top-right corner. It turns with the car, so the way you
//! are driving is always up.
use bevy_egui::egui::{self, Color32, Pos2, Shape, Stroke, Vec2};

use crate::world::{BlockKind, BuildingState, HALF_SIZE, ROAD_LINES, ROAD_WIDTH, TargetKind, World};

pub const SIZE: f32 = 180.0;

fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Draws the map around (x, z) into `rect`. Markers are (x, z, color, size).
pub fn draw(painter: &egui::Painter, rect: egui::Rect, world: &World, x: f64, z: f64, heading: f64, markers: &[(f64, f64, u32, f32)]) {
    let painter = painter.with_clip_rect(rect);
    let center = rect.center();
    let range = world.track.as_ref().map_or(130.0, |t| (t.length / 10.0).clamp(120.0, 300.0));
    let ppm = (SIZE / 2.0) as f64 / range;
    let (s, c) = heading.sin_cos();
    // World to screen: forward is up, +x is to the left (as seen from behind the car).
    let to_screen = |wx: f64, wz: f64| {
        let (u, v) = (-(wx - x) * ppm, -(wz - z) * ppm);
        Pos2::new(center.x + (u * c - v * s) as f32, center.y + (u * s + v * c) as f32)
    };
    let quad = |cx: f64, cz: f64, w: f64, d: f64, turn: f64, color: Color32| {
        let (ts, tc) = turn.sin_cos();
        let corner = |a: f64, b: f64| to_screen(cx + a * tc + b * ts, cz - a * ts + b * tc);
        Shape::convex_polygon(
            vec![
                corner(-w / 2.0, -d / 2.0),
                corner(w / 2.0, -d / 2.0),
                corner(w / 2.0, d / 2.0),
                corner(-w / 2.0, d / 2.0),
            ],
            color,
            Stroke::NONE,
        )
    };
    painter.rect_filled(rect, 22.0, hex(0x3d5a35));
    let near = |px: f64, pz: f64, size: f64| (px - x).abs() < range * 1.5 + size && (pz - z).abs() < range * 1.5 + size;
    if world.map == "city" {
        let inner = 80.0 - ROAD_WIDTH;
        for b in &world.blocks {
            if !near(b.x, b.z, 40.0) {
                continue;
            }
            let color = match b.kind {
                BlockKind::Park => 0x4f7d3f,
                BlockKind::Lot => 0x45474c,
                _ => 0x8b8f96,
            };
            painter.add(quad(b.x, b.z, inner, inner, 0.0, hex(color)));
            if b.kind == BlockKind::Stunt {
                painter.add(quad(b.x, b.z, inner - 6.0, inner - 6.0, 0.0, hex(0x9a9488)));
            }
        }
        let len = HALF_SIZE * 2.0 - 16.0;
        for &line in &ROAD_LINES {
            painter.add(quad(line, 0.0, ROAD_WIDTH, len, 0.0, hex(0x2b2d31)));
            painter.add(quad(0.0, line, len, ROAD_WIDTH, 0.0, hex(0x2b2d31)));
        }
    }
    if let Some(track) = &world.track {
        let mut pts: Vec<Pos2> = track.points.iter().map(|p| to_screen(p.x, p.z)).collect();
        pts.push(pts[0]);
        painter.add(Shape::line(pts, Stroke::new((track.style.width * ppm) as f32, hex(0x2b2d31))));
        let start = track.pose(0.0, 0.0);
        painter.add(quad(
            start.x,
            start.z,
            track.style.width,
            3.0,
            start.heading - std::f64::consts::FRAC_PI_2 + std::f64::consts::FRAC_PI_2,
            hex(0xf4f4f4),
        ));
    }
    let wall = Stroke::new(1.5, hex(0xb8bcc2));
    for w in &world.walls {
        if near(w.ax, w.az, 10.0) || near(w.bx, w.bz, 10.0) {
            painter.line_segment([to_screen(w.ax, w.az), to_screen(w.bx, w.bz)], wall);
        }
    }
    for r in &world.ramps {
        if near(r.x, r.z, 20.0) {
            painter.add(quad(r.x, r.z, r.width, r.length, r.angle, hex(0xe0a03a)));
        }
    }
    for t in &world.targets {
        if !near(t.x, t.z, 60.0) {
            continue;
        }
        match t.kind {
            TargetKind::Building => {
                let color = if t.alive {
                    0xd8d2c6
                } else if t.state == BuildingState::Rising {
                    0xa8a296
                } else {
                    0x6a5a4a
                };
                painter.add(quad(t.x, t.z, t.width, t.depth, 0.0, hex(color)));
            }
            TargetKind::Car if t.rig.is_some() => {
                painter.add(quad(t.x, t.z, 2.4, 2.4, 0.0, hex(if t.alive { 0xb9c3d6 } else { 0x1a1a1a })));
            }
            _ => {}
        }
    }
    let h = world.half;
    let edge = [
        to_screen(-h, -h),
        to_screen(h, -h),
        to_screen(h, h),
        to_screen(-h, h),
        to_screen(-h, -h),
    ];
    painter.add(Shape::line(edge.to_vec(), Stroke::new(2.0, hex(0xb8bcc2))));
    for &(mx, mz, color, size) in markers {
        let (mut dx, mut dz) = (mx - x, mz - z);
        let d = dx.hypot(dz);
        let max = range - 8.0;
        if d > max {
            dx *= max / d;
            dz *= max / d;
        }
        painter.circle_filled(to_screen(x + dx, z + dz), size, hex(color));
    }
    // The player: an arrow in the middle, always pointing up.
    let arrow = [Vec2::new(0.0, -8.0), Vec2::new(6.0, 6.0), Vec2::new(0.0, 3.0), Vec2::new(-6.0, 6.0)].map(|v| center + v);
    painter.add(Shape::convex_polygon(
        vec![arrow[0], arrow[1], arrow[2]],
        hex(0xff5a36),
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![arrow[0], arrow[2], arrow[3]],
        hex(0xff5a36),
        Stroke::NONE,
    ));
    painter.add(Shape::closed_line(arrow.to_vec(), Stroke::new(1.5, Color32::WHITE)));
    painter.rect_stroke(
        rect,
        22.0,
        Stroke::new(2.0, Color32::from_white_alpha(90)),
        egui::StrokeKind::Inside,
    );
}
