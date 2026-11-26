//! A tiny painter for the game's textures (asphalt, curbs, walls, signs):
//! the game draws them when it starts instead of loading picture files.
use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// A color with alpha, 0..1 each, in sRGB like CSS colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    pub fn hex(rgb: u32) -> Self {
        Rgba(
            ((rgb >> 16) & 255) as f32 / 255.0,
            ((rgb >> 8) & 255) as f32 / 255.0,
            (rgb & 255) as f32 / 255.0,
            1.0,
        )
    }
    /// "#rrggbb" (anything else is grey).
    pub fn css(text: &str) -> Self {
        Rgba::hex(parse_hex(text).unwrap_or(0x808080))
    }
    pub fn rgb8(r: u8, g: u8, b: u8, a: f32) -> Self {
        Rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
    }
    pub fn alpha(self, a: f32) -> Self {
        Rgba(self.0, self.1, self.2, a)
    }
}

/// "#rrggbb" as a number.
pub fn parse_hex(text: &str) -> Option<u32> {
    let t = text.strip_prefix('#')?;
    if t.len() != 6 {
        return None;
    }
    u32::from_str_radix(t, 16).ok()
}

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<f32>, // RGBA, 0..1, straight alpha
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Canvas {
            width,
            height,
            pixels: vec![0.0; (width * height * 4) as usize],
        }
    }

    pub fn filled(width: u32, height: u32, color: Rgba) -> Self {
        let mut c = Canvas::new(width, height);
        c.fill_rect(0.0, 0.0, width as f32, height as f32, color);
        c
    }

    /// Blends `color` into one pixel, `cover` (0..1) of the way.
    pub fn blend(&mut self, x: i32, y: i32, color: Rgba, cover: f32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let a = (color.3 * cover).clamp(0.0, 1.0);
        let p = &mut self.pixels[i..i + 4];
        let out_a = a + p[3] * (1.0 - a);
        if out_a <= 0.0 {
            return;
        }
        for (k, c) in [color.0, color.1, color.2].into_iter().enumerate() {
            p[k] = (c * a + p[k] * p[3] * (1.0 - a)) / out_a;
        }
        p[3] = out_a;
    }

    /// Fills a rectangle, with soft edges where it covers part of a pixel (like a browser canvas).
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        let (x0, x1) = (x.max(0.0), (x + w).min(self.width as f32));
        let (y0, y1) = (y.max(0.0), (y + h).min(self.height as f32));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for py in y0.floor() as i32..y1.ceil() as i32 {
            let cy = (y1.min(py as f32 + 1.0) - y0.max(py as f32)).max(0.0);
            for px in x0.floor() as i32..x1.ceil() as i32 {
                let cx = (x1.min(px as f32 + 1.0) - x0.max(px as f32)).max(0.0);
                self.blend(px, py, color, cx * cy);
            }
        }
    }

    /// A filled circle.
    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Rgba) {
        for py in (cy - r - 1.0).floor() as i32..=(cy + r + 1.0).ceil() as i32 {
            for px in (cx - r - 1.0).floor() as i32..=(cx + r + 1.0).ceil() as i32 {
                let d = ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt();
                let cover = (r - d + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 {
                    self.blend(px, py, color, cover);
                }
            }
        }
    }

    /// A line `width` pixels thick.
    pub fn line(&mut self, ax: f32, ay: f32, bx: f32, by: f32, width: f32, color: Rgba) {
        let (minx, maxx) = (ax.min(bx) - width, ax.max(bx) + width);
        let (miny, maxy) = (ay.min(by) - width, ay.max(by) + width);
        let (sx, sy) = (bx - ax, by - ay);
        let len2 = (sx * sx + sy * sy).max(1e-6);
        for py in miny.floor() as i32..=maxy.ceil() as i32 {
            for px in minx.floor() as i32..=maxx.ceil() as i32 {
                let (qx, qy) = (px as f32 + 0.5, py as f32 + 0.5);
                let t = (((qx - ax) * sx + (qy - ay) * sy) / len2).clamp(0.0, 1.0);
                let d = ((qx - ax - sx * t).powi(2) + (qy - ay - sy * t).powi(2)).sqrt();
                let cover = (width / 2.0 - d + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 {
                    self.blend(px, py, color, cover);
                }
            }
        }
    }

    /// A filled polygon (even-odd rule), sampled once per pixel.
    pub fn fill_polygon(&mut self, points: &[(f32, f32)], color: Rgba) {
        if points.len() < 3 {
            return;
        }
        let miny = points.iter().map(|p| p.1).fold(f32::MAX, f32::min).max(0.0) as i32;
        let maxy = points.iter().map(|p| p.1).fold(f32::MIN, f32::max).min(self.height as f32) as i32;
        for py in miny..=maxy {
            let y = py as f32 + 0.5;
            let mut xs: Vec<f32> = Vec::new();
            for k in 0..points.len() {
                let (a, b) = (points[k], points[(k + 1) % points.len()]);
                if (a.1 <= y) != (b.1 <= y) {
                    xs.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(f32::total_cmp);
            for pair in xs.chunks(2) {
                if let [x0, x1] = pair {
                    self.fill_rect(*x0, py as f32, x1 - x0, 1.0, color);
                }
            }
        }
    }

    /// The finished texture. `repeat` tiles it; `srgb` is for colors (not for normal or roughness maps).
    pub fn into_image(self, repeat: bool) -> Image {
        let mut data = Vec::with_capacity(self.pixels.len());
        for p in self.pixels.chunks(4) {
            for c in &p[..3] {
                data.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
            data.push((p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
        // Smaller copies (mipmaps) so far-away textures don't shimmer.
        let (mut w, mut h) = (self.width, self.height);
        let mut level = data.clone();
        let mut levels = 1;
        while w > 1 && h > 1 && w % 2 == 0 && h % 2 == 0 {
            let (nw, nh) = (w / 2, h / 2);
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let at = |dx: u32, dy: u32| level[(((y * 2 + dy) * w + x * 2 + dx) * 4 + c) as usize] as u32;
                        next[((y * nw + x) * 4 + c) as usize] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                    }
                }
            }
            data.extend_from_slice(&next);
            level = next;
            (w, h) = (nw, nh);
            levels += 1;
        }
        let mut image = Image {
            data: Some(data),
            ..Default::default()
        };
        image.texture_descriptor.size = Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        };
        image.texture_descriptor.dimension = TextureDimension::D2;
        image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
        image.texture_descriptor.mip_level_count = levels;
        image.asset_usage = RenderAssetUsages::RENDER_WORLD;
        let mode = if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        };
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: mode,
            address_mode_v: mode,
            anisotropy_clamp: 8,
            ..ImageSamplerDescriptor::linear()
        });
        image
    }
}

impl Canvas {
    /// An ellipse, rotated by `rotation` radians.
    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, rotation: f32, color: Rgba) {
        let points: Vec<(f32, f32)> = (0..32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                let (x, y) = (a.cos() * rx, a.sin() * ry);
                let (s, c) = rotation.sin_cos();
                (cx + x * c - y * s, cy + x * s + y * c)
            })
            .collect();
        self.fill_polygon(&points, color);
    }

    /// An outlined rectangle `width` pixels thick, centered on the edges (like canvas strokeRect).
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32, color: Rgba) {
        let hw = width / 2.0;
        self.fill_rect(x - hw, y - hw, w + width, width, color);
        self.fill_rect(x - hw, y + h - hw, w + width, width, color);
        self.fill_rect(x - hw, y + hw, width, h - width, color);
        self.fill_rect(x + w - hw, y + hw, width, h - width, color);
    }

    /// Text centered on (cx, cy), `size` pixels tall, squeezed to fit `max_width`. `bold` thickens it.
    pub fn fill_text(&mut self, text: &str, cx: f32, cy: f32, size: f32, max_width: f32, bold: bool, color: Rgba) {
        use ab_glyph::{Font, FontRef, ScaleFont};
        let Ok(font) = FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT) else {
            return;
        };
        let scaled = font.as_scaled(size);
        let mut width = 0.0;
        let mut glyphs = Vec::new();
        let mut last = None;
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            if let Some(prev) = last {
                width += scaled.kern(prev, id);
            }
            glyphs.push((id, width));
            width += scaled.h_advance(id);
            last = Some(id);
        }
        let squeeze = if width > max_width { max_width / width } else { 1.0 };
        let left = cx - width * squeeze / 2.0;
        let baseline = cy + (scaled.ascent() + scaled.descent()) / 2.0;
        let offsets: &[(f32, f32)] = if bold {
            &[(0.0, 0.0), (size * 0.03, 0.0), (-size * 0.03, 0.0), (0.0, size * 0.025)]
        } else {
            &[(0.0, 0.0)]
        };
        for &(ox, oy) in offsets {
            for (id, x) in &glyphs {
                let glyph = id.with_scale_and_position(size, ab_glyph::point(0.0, 0.0));
                let Some(outlined) = font.outline_glyph(glyph) else { continue };
                let bounds = outlined.px_bounds();
                let gx = left + x * squeeze + ox;
                outlined.draw(|px, py, cover| {
                    let fx = gx + (bounds.min.x + px as f32) * squeeze;
                    let fy = baseline + bounds.min.y + py as f32 + oy;
                    self.blend(fx.round() as i32, fy.round() as i32, color, cover);
                });
            }
        }
    }

    /// A vertical gradient from `top` to `bottom`.
    pub fn gradient(width: u32, height: u32, top: Rgba, bottom: Rgba) -> Canvas {
        let mut c = Canvas::new(width, height);
        for y in 0..height {
            let t = y as f32 / (height - 1).max(1) as f32;
            let mix = |a: f32, b: f32| a + (b - a) * t;
            c.fill_rect(
                0.0,
                y as f32,
                width as f32,
                1.0,
                Rgba(
                    mix(top.0, bottom.0),
                    mix(top.1, bottom.1),
                    mix(top.2, bottom.2),
                    mix(top.3, bottom.3),
                ),
            );
        }
        c
    }

    /// A soft white blob that fades to transparent at the edge.
    pub fn radial_glow(size: u32) -> Canvas {
        let mut c = Canvas::new(size, size);
        let mid = size as f32 / 2.0;
        let inner = size as f32 / 32.0;
        for y in 0..size {
            for x in 0..size {
                let d = ((x as f32 + 0.5 - mid).powi(2) + (y as f32 + 0.5 - mid).powi(2)).sqrt();
                let t = ((d - inner) / (mid - inner)).clamp(0.0, 1.0);
                c.blend(x as i32, y as i32, Rgba(1.0, 1.0, 1.0, 0.9 * (1.0 - t)), 1.0);
            }
        }
        c
    }
}

/// HSL (h 0..1, s and l 0..1) to a color.
pub fn hsl(h: f32, s: f32, l: f32) -> Rgba {
    let f = |n: f32| {
        let k = (n + h * 12.0).rem_euclid(12.0);
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    Rgba(f(0.0), f(8.0), f(4.0), 1.0)
}
