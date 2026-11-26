//! The game's textures, painted when the game starts so there are no image files to ship.
use crate::canvas::{Canvas, Rgba, hsl};
use crate::rng::Seeded;

pub fn grass_texture() -> Canvas {
    let mut rand = Seeded::new(7.0);
    let mut c = Canvas::filled(256, 256, Rgba::css("#4f7d3a"));
    for _ in 0..2600 {
        let g = 90 + (rand.next() * 60.0).floor() as u8;
        let (x, y, h) = (
            rand.next() as f32 * 256.0,
            rand.next() as f32 * 256.0,
            2.0 + rand.next() as f32 * 3.0,
        );
        c.fill_rect(x, y, 2.0, h, Rgba::rgb8(g - 40, g + 20, g - 60, 0.35));
    }
    c
}

/// The ground at a venue: grass, desert sand, snow, or dark city dirt.
pub fn ground_texture(kind: crate::venues::Ground) -> Canvas {
    use crate::venues::Ground;
    let (base, a, b, seed) = match kind {
        Ground::Grass => return grass_texture(),
        Ground::Sand => (
            Rgba::css("#d8b47a"),
            Rgba::rgb8(170, 120, 70, 0.35),
            Rgba::rgb8(250, 225, 180, 0.4),
            4.0 * 13.0,
        ),
        Ground::Snow => (
            Rgba::css("#eef3f7"),
            Rgba::rgb8(170, 190, 210, 0.35),
            Rgba::rgb8(255, 255, 255, 0.6),
            4.0 * 13.0,
        ),
        Ground::Dirt => (
            Rgba::css("#2a2b30"),
            Rgba::rgb8(60, 60, 70, 0.45),
            Rgba::rgb8(15, 15, 20, 0.5),
            4.0 * 13.0,
        ),
    };
    let mut rand = Seeded::new(seed);
    let mut c = Canvas::filled(256, 256, base);
    for i in 0..2400 {
        let (x, y) = (rand.next() as f32 * 256.0, rand.next() as f32 * 256.0);
        let (w, h) = (1.0 + rand.next() as f32 * 3.0, 1.0 + rand.next() as f32 * 2.0);
        c.fill_rect(x, y, w, h, if i % 2 == 0 { a } else { b });
    }
    c
}

/// Road running along V, with edge lines and a dashed center line.
pub fn road_texture() -> Canvas {
    let mut rand = Seeded::new(11.0);
    let mut c = Canvas::filled(128, 256, Rgba::css("#34363b"));
    for _ in 0..900 {
        let v = 40 + (rand.next() * 30.0).floor() as u8;
        let (x, y) = (rand.next() as f32 * 128.0, rand.next() as f32 * 256.0);
        c.fill_rect(x, y, 1.5, 1.5, Rgba::rgb8(v, v, v + 4, 0.5));
    }
    c.fill_rect(6.0, 0.0, 4.0, 256.0, Rgba::css("#e8e8e8"));
    c.fill_rect(118.0, 0.0, 4.0, 256.0, Rgba::css("#e8e8e8"));
    c.fill_rect(61.0, 0.0, 6.0, 128.0, Rgba::css("#f2c94c"));
    c
}

pub fn concrete_texture() -> Canvas {
    let mut rand = Seeded::new(5.0);
    let mut c = Canvas::filled(128, 128, Rgba::css("#8d8f93"));
    for _ in 0..700 {
        let v = 120 + (rand.next() * 40.0).floor() as u8;
        let (x, y) = (rand.next() as f32 * 128.0, rand.next() as f32 * 128.0);
        c.fill_rect(x, y, 2.0, 2.0, Rgba::rgb8(v, v, v, 0.4));
    }
    c.stroke_rect(0.0, 0.0, 128.0, 128.0, 2.0, Rgba::rgb8(60, 60, 60, 0.5));
    c
}

/// Building facade: a grid of windows. The lit version is the night-time glow map.
pub fn window_texture(lit: bool) -> Canvas {
    let mut rand = Seeded::new(if lit { 3.0 } else { 2.0 });
    let mut c = Canvas::filled(128, 128, if lit { Rgba::hex(0) } else { Rgba::hex(0xffffff) });
    for y in 0..4 {
        for x in 0..4 {
            let color = if lit {
                if rand.next() < 0.45 {
                    let h = (40.0 + rand.next() * 20.0) as f32 / 360.0;
                    let l = (55.0 + rand.next() * 20.0) as f32 / 100.0;
                    hsl(h, 0.9, l)
                } else {
                    Rgba::hex(0)
                }
            } else {
                Rgba::css("#5c6b7a")
            };
            c.fill_rect(x as f32 * 32.0 + 6.0, y as f32 * 32.0 + 7.0, 20.0, 18.0, color);
        }
    }
    c
}

pub fn plate_texture(text: &str) -> Canvas {
    let mut c = Canvas::filled(256, 96, Rgba::css("#f4f1e6"));
    c.stroke_rect(4.0, 4.0, 248.0, 88.0, 6.0, Rgba::css("#1b1b1b"));
    c.fill_text(text, 128.0, 52.0, 56.0, 232.0, true, Rgba::css("#1b1b1b"));
    c
}

pub fn carbon_texture() -> Canvas {
    let mut c = Canvas::filled(64, 64, Rgba::css("#151515"));
    for y in 0..8 {
        for x in 0..8 {
            let color = if (x + y) % 2 == 0 { "#2a2a2a" } else { "#0e0e0e" };
            c.fill_rect(x as f32 * 8.0, y as f32 * 8.0, 8.0, 8.0, Rgba::css(color));
        }
    }
    c
}

fn bezier(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), out: &mut Vec<(f32, f32)>) {
    for i in 1..=12 {
        let t = i as f32 / 12.0;
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        out.push((
            w[0] * p0.0 + w[1] * p1.0 + w[2] * p2.0 + w[3] * p3.0,
            w[0] * p0.1 + w[1] * p1.1 + w[2] * p2.1 + w[3] * p3.1,
        ));
    }
}

/// A paint pattern for the side of the car, looking at it side on: the left
/// edge is the back, the right edge the front, the top edge the top of the body.
pub fn livery_texture(pattern: &str, base: &str, accent: &str, length: f64, height: f64) -> Canvas {
    let width = 1024u32;
    let ppm = width as f32 / length as f32;
    let h = ((height as f32 * ppm).round() as u32).clamp(32, 1024);
    let hf = h as f32;
    let wf = width as f32;
    let mut rand = Seeded::new(9.0);
    let mut c = Canvas::filled(width, h, Rgba::css(base));
    let accent_c = Rgba::css(accent);
    match pattern {
        "twoTone" => c.fill_rect(0.0, hf * 0.55, wf, hf, accent_c),
        "checker" => {
            let square = 0.25 * ppm;
            let mut y = 0;
            while y as f32 * square < hf {
                let mut x = 0;
                while x as f32 * square < wf {
                    if (x + y) % 2 == 0 {
                        c.fill_rect(x as f32 * square, y as f32 * square, square, square, accent_c);
                    }
                    x += 1;
                }
                y += 1;
            }
        }
        "dots" => {
            let gap = 0.3 * ppm;
            let mut y = 0;
            while (y as f32) * gap < hf + gap {
                let mut x = 0;
                while (x as f32) * gap < wf + gap {
                    c.fill_circle(x as f32 * gap + (y % 2) as f32 * gap * 0.5, y as f32 * gap, 0.07 * ppm, accent_c);
                    x += 1;
                }
                y += 1;
            }
        }
        "camo" => {
            let b = Rgba::css(base);
            let dark = Rgba(b.0 * 0.55, b.1 * 0.55, b.2 * 0.55, 1.0);
            for i in 0..(length * 40.0) as usize {
                let color = if i % 2 == 0 { accent_c } else { dark };
                let (x, y) = (rand.next() as f32 * wf, rand.next() as f32 * hf);
                let rx = (0.12 + rand.next() as f32 * 0.25) * ppm;
                let ry = (0.08 + rand.next() as f32 * 0.15) * ppm;
                let rot = rand.next() as f32 * std::f32::consts::PI;
                c.fill_ellipse(x, y, rx, ry, rot, color);
            }
        }
        "tiger" => {
            let mut x = 0.15 * ppm;
            while x < wf {
                let from_top = ((x / (0.32 * ppm)).round() as i64) % 2 == 0;
                let tip_x = x + (rand.next() as f32 - 0.5) * 0.2 * ppm;
                let reach = hf * (0.45 + rand.next() as f32 * 0.35);
                let edge = if from_top { 0.0 } else { hf };
                let tip_y = if from_top { reach } else { hf - reach };
                c.fill_polygon(&[(x - 0.06 * ppm, edge), (tip_x, tip_y), (x + 0.06 * ppm, edge)], accent_c);
                x += 0.32 * ppm;
            }
        }
        "flames" => {
            let mut tongue = |c: &mut Canvas, y: f32, reach: f32, thick: f32, color: Rgba| {
                let mut pts = vec![(wf, y - thick)];
                bezier(
                    (wf, y - thick),
                    (wf - reach * 0.4, y - thick * 1.4),
                    (wf - reach * 0.7, y + thick * 0.6),
                    (wf - reach, y - thick * 0.3),
                    &mut pts,
                );
                let last = *pts.last().unwrap();
                bezier(
                    last,
                    (wf - reach * 0.7, y + thick * 1.2),
                    (wf - reach * 0.4, y + thick * 1.4),
                    (wf, y + thick),
                    &mut pts,
                );
                c.fill_polygon(&pts, color);
            };
            for i in 0..6 {
                let y = hf * (0.3 + i as f32 * 0.09);
                let reach = wf * (0.35 + rand.next() as f32 * 0.3);
                tongue(&mut c, y, reach, hf * 0.09, accent_c);
                tongue(&mut c, y, reach * 0.6, hf * 0.04, Rgba::css("#ffd23f"));
            }
        }
        _ => {}
    }
    c
}

/// A white race-number roundel with a black number, for the doors.
pub fn race_number_texture(n: u32) -> Canvas {
    let mut c = Canvas::new(128, 128);
    c.fill_circle(64.0, 64.0, 63.0, Rgba::css("#1b1b1b"));
    c.fill_circle(64.0, 64.0, 57.0, Rgba::css("#f4f1e6"));
    c.fill_text(&n.to_string(), 64.0, 68.0, 72.0, 100.0, true, Rgba::css("#1b1b1b"));
    c
}

/// White text on a dark banner, for the start gantry.
pub fn banner_texture(text: &str) -> Canvas {
    let mut c = Canvas::filled(1024, 64, Rgba::css("#15181d"));
    c.fill_rect(0.0, 0.0, 1024.0, 6.0, Rgba::css("#d7263d"));
    c.fill_rect(0.0, 58.0, 1024.0, 6.0, Rgba::css("#d7263d"));
    c.fill_text(
        &format!("{text}  ·  START  ·  FINISH"),
        512.0,
        34.0,
        40.0,
        1000.0,
        true,
        Rgba::hex(0xffffff),
    );
    c
}
