//! Builds the 3D car from a CarConfig. Everything is made from simple shapes,
//! so there are no model files to load. Parts that share a material are
//! merged into one mesh, so a car is a couple of dozen draws, not hundreds.
use std::collections::BTreeMap;
use std::f32::consts::PI;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::shape::{Cone, Cuboid, Cylinder, Rectangle, Sphere, Torus};

use crate::canvas::Rgba;
use crate::config::CarConfig;
use crate::meshes::{MeshData, tapered_cylinder};
use crate::textures;

type P = (f32, f32); // (z, y) point on the car's side profile

#[derive(Clone, Debug)]
pub struct BodySpec {
    pub width: f32,
    pub top: Vec<P>,
    pub bottom_y: f32,
    pub cabin: [P; 4],
    pub cabin_width: f32,
    pub axle_rear: f32,
    pub axle_front: f32,
    pub track: f32,
    pub wheel_radius: f32,
    pub tire_width: f32,
    pub spoiler: P,
    pub headlight: P,
    pub taillight: P,
    pub bed: Option<(f32, f32, f32)>,
}

fn body_spec(shape: &str) -> BodySpec {
    match shape {
        "muscle" => BodySpec {
            width: 1.95,
            top: vec![
                (-2.4, 0.3),
                (-2.4, 0.9),
                (-2.0, 0.96),
                (-1.3, 0.96),
                (0.7, 0.95),
                (2.3, 0.92),
                (2.42, 0.8),
                (2.44, 0.34),
            ],
            bottom_y: 0.24,
            cabin: [(-1.45, 0.94), (-0.95, 1.36), (0.2, 1.38), (0.85, 0.93)],
            cabin_width: 1.6,
            axle_rear: -1.5,
            axle_front: 1.48,
            track: 0.82,
            wheel_radius: 0.36,
            tire_width: 0.3,
            spoiler: (-2.15, 0.96),
            headlight: (2.4, 0.72),
            taillight: (-2.4, 0.8),
            bed: None,
        },
        "hatch" => BodySpec {
            width: 1.76,
            top: vec![
                (-1.95, 0.3),
                (-1.97, 0.95),
                (-1.6, 1.0),
                (0.5, 0.98),
                (1.6, 0.85),
                (1.95, 0.68),
                (1.97, 0.32),
            ],
            bottom_y: 0.2,
            cabin: [(-1.9, 0.97), (-1.75, 1.55), (0.25, 1.58), (1.05, 0.95)],
            cabin_width: 1.56,
            axle_rear: -1.25,
            axle_front: 1.22,
            track: 0.74,
            wheel_radius: 0.32,
            tire_width: 0.22,
            spoiler: (-1.72, 1.57),
            headlight: (1.9, 0.72),
            taillight: (-1.98, 0.86),
            bed: None,
        },
        "pickup" => BodySpec {
            width: 2.0,
            top: vec![
                (-2.5, 0.55),
                (-2.5, 1.18),
                (-0.75, 1.2),
                (0.9, 1.2),
                (2.2, 1.15),
                (2.5, 1.0),
                (2.52, 0.55),
            ],
            bottom_y: 0.42,
            cabin: [(-0.75, 1.18), (-0.6, 1.9), (0.75, 1.92), (1.25, 1.18)],
            cabin_width: 1.8,
            axle_rear: -1.6,
            axle_front: 1.55,
            track: 0.86,
            wheel_radius: 0.44,
            tire_width: 0.32,
            spoiler: (-2.3, 1.2),
            headlight: (2.46, 0.98),
            taillight: (-2.5, 1.02),
            bed: Some((-2.4, -0.85, 1.205)),
        },
        "van" => BodySpec {
            width: 1.95,
            top: vec![(-2.4, 0.35), (-2.42, 1.12), (1.9, 1.12), (2.35, 0.95), (2.44, 0.8), (2.45, 0.38)],
            bottom_y: 0.3,
            cabin: [(-2.35, 1.1), (-2.3, 1.98), (1.0, 2.0), (1.9, 1.1)],
            cabin_width: 1.88,
            axle_rear: -1.55,
            axle_front: 1.6,
            track: 0.84,
            wheel_radius: 0.36,
            tire_width: 0.26,
            spoiler: (-2.2, 2.0),
            headlight: (2.38, 0.82),
            taillight: (-2.42, 0.9),
            bed: None,
        },
        "wedge" => BodySpec {
            width: 1.98,
            top: vec![
                (-2.25, 0.3),
                (-2.27, 0.78),
                (-1.6, 0.84),
                (-0.6, 0.86),
                (1.0, 0.74),
                (2.1, 0.48),
                (2.3, 0.34),
                (2.32, 0.26),
            ],
            bottom_y: 0.18,
            cabin: [(-1.3, 0.84), (-0.7, 1.12), (0.05, 1.14), (1.1, 0.71)],
            cabin_width: 1.5,
            axle_rear: -1.4,
            axle_front: 1.38,
            track: 0.82,
            wheel_radius: 0.35,
            tire_width: 0.3,
            spoiler: (-2.0, 0.84),
            headlight: (2.15, 0.5),
            taillight: (-2.27, 0.7),
            bed: None,
        },
        _ => BodySpec {
            width: 1.86,
            top: vec![
                (-2.2, 0.26),
                (-2.22, 0.72),
                (-1.9, 0.86),
                (-1.25, 0.9),
                (0.55, 0.86),
                (1.7, 0.72),
                (2.2, 0.52),
                (2.22, 0.28),
            ],
            bottom_y: 0.2,
            cabin: [(-1.35, 0.86), (-0.75, 1.24), (0.15, 1.26), (0.75, 0.84)],
            cabin_width: 1.5,
            axle_rear: -1.35,
            axle_front: 1.32,
            track: 0.78,
            wheel_radius: 0.34,
            tire_width: 0.26,
            spoiler: (-1.95, 0.86),
            headlight: (2.08, 0.6),
            taillight: (-2.22, 0.66),
            bed: None,
        },
    }
}

/// Stretches a body: longer, wider and taller. Heights grow from the bottom of the body.
fn stretch_body(spec: BodySpec, long: f32, wide: f32, tall: f32) -> BodySpec {
    let by = spec.bottom_y;
    let zy = |(z, y): P| (z * long, by + (y - by) * tall);
    BodySpec {
        width: spec.width * wide,
        cabin_width: spec.cabin_width * wide,
        track: spec.track * wide,
        top: spec.top.iter().map(|p| zy(*p)).collect(),
        cabin: spec.cabin.map(zy),
        axle_rear: spec.axle_rear * long,
        axle_front: spec.axle_front * long,
        spoiler: zy(spec.spoiler),
        headlight: zy(spec.headlight),
        taillight: zy(spec.taillight),
        bed: spec.bed.map(|(z0, z1, y)| (z0 * long, z1 * long, by + (y - by) * tall)),
        ..spec
    }
}

pub const BEVEL: f32 = 0.05;

#[derive(Clone, Debug)]
pub struct WheelRig {
    pub steer: Entity,
    pub spin: Entity,
    pub front: bool,
    /// Meters, with the car's size.
    pub radius: f64,
}

/// A car body's shape after its length, width and height settings.
#[derive(Clone, Debug)]
pub struct CarShape {
    pub spec: BodySpec,
    pub wheel_y: f32,
    pub arch_r: f32,
    pub wheel_x: f32,
    pub tire_width: f32,
    pub front_z: f32,
    pub rear_z: f32,
}

/// The materials of a car that change while it's driven.
#[derive(Clone, Debug)]
pub struct CarMaterials {
    pub paint: Handle<StandardMaterial>,
    pub body_paint: Handle<StandardMaterial>,
    pub head: Handle<StandardMaterial>,
    pub tail: Handle<StandardMaterial>,
    pub glow: Option<Handle<StandardMaterial>>,
    pub glass: Handle<StandardMaterial>,
    pub burned: Handle<StandardMaterial>,
}

/// A built car: its entities, its size, and the parts that move or light up.
#[derive(Clone, Debug)]
pub struct CarRig {
    pub root: Entity,
    pub body: Entity,
    pub wheels: Vec<WheelRig>,
    pub length: f64,
    pub width: f64,
    pub height: f64,
    pub wheel_base: f64,
    pub eye_height: f64,
    /// Exhaust tips in the car's own coordinates (before the size scale), for boost trails.
    pub exhausts: Vec<Vec3>,
    /// Where bullets and rockets leave the car, in the car's own coordinates (+z forward, before scaling).
    pub gun_muzzle: Vec3,
    pub rocket_muzzle: Vec3,
    /// The body's height above the root (body coordinates have y = 0 at the stock car's ground).
    pub body_y: f32,
    pub config: CarConfig,
    pub shape: CarShape,
    pub mats: CarMaterials,
    pub flames: Vec<Entity>,
    pub flag: Option<Entity>,
    pub beam: Option<Entity>,
    pub glow_light: Option<Entity>,
    pub glow_decal: Option<Entity>,
    /// Every mesh with a lit material, and that material, for wrecking and repairing.
    pub lit_parts: Vec<(Entity, Handle<StandardMaterial>)>,
    /// The body shell, cabin, roof, body stripes and lamps (a real car body from a pack replaces these).
    pub shell: Vec<Entity>,
    pub wrecked: bool,
    pub lights: (bool, bool),
}

impl CarRig {
    pub fn dims(&self) -> crate::physics::Dims {
        crate::physics::Dims {
            length: self.length,
            width: self.width,
            height: self.height,
            wheel_base: self.wheel_base,
        }
    }
}

/// Things that build cars need.
pub struct CarBuildCtx<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub images: &'a mut Assets<Image>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Mat {
    Paint,
    BodyPaint,
    Accent,
    Black,
    Carbon,
    Chrome,
    Rubber,
    Rim,
    Glass,
    Head,
    Tail,
    Number,
    Plate,
    GunMetal,
    Gold,
    Gem,
    HatBlack,
    Orange,
    White,
    Helmet,
    Horn,
    Halo,
    Duck,
    Beak,
    Eye,
}

fn srgb(hex: &str) -> Color {
    let c = Rgba::css(hex);
    Color::srgb(c.0, c.1, c.2)
}

fn plain(color: Color, metal: f32, rough: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        metallic: metal,
        perceptual_roughness: rough,
        ..default()
    }
}

/// The paint for a finish (the color is "#rrggbb").
fn paint_material(color: &str, finish: &str, carbon: &Handle<Image>) -> StandardMaterial {
    let c = srgb(color);
    let mut m = StandardMaterial {
        base_color: c,
        ..default()
    };
    let (metal, rough, coat, coat_rough) = match finish {
        "gloss" => (0.05, 0.3, 1.0, 0.05),
        "metallic" => (0.75, 0.32, 1.0, 0.08),
        "matte" => (0.0, 0.85, 0.0, 0.0),
        "pearl" => (0.35, 0.3, 1.0, 0.05),
        "chrome" => (1.0, 0.06, 0.6, 0.1),
        "carbon" => (0.2, 0.35, 1.0, 0.1),
        "glow" => (0.1, 0.4, 0.0, 0.0),
        _ => (0.4, 0.25, 1.0, 0.05),
    };
    m.metallic = metal;
    m.perceptual_roughness = rough;
    m.clearcoat = coat;
    m.clearcoat_perceptual_roughness = coat_rough;
    if finish == "carbon" {
        m.base_color_texture = Some(carbon.clone());
        m.base_color = Color::WHITE.mix(&c, 0.35);
    }
    if finish == "glow" {
        m.emissive = c.to_linear() * 0.7 * 3.0;
    }
    m
}

/// Height of the top outline at a given z.
fn top_at(spec: &BodySpec, z: f32) -> f32 {
    for w in spec.top.windows(2) {
        let ((z0, y0), (z1, y1)) = (w[0], w[1]);
        if (z >= z0 && z <= z1) || (z >= z1 && z <= z0) {
            let t = if z1 == z0 { 0.0 } else { (z - z0) / (z1 - z0) };
            return y0 + (y1 - y0) * t;
        }
    }
    spec.top[0].1
}

/// Side-profile outline with wheel arches cut into the bottom edge.
fn lower_profile(spec: &BodySpec, arch_r: f32, wheel_y: f32) -> Vec<P> {
    let mut pts: Vec<P> = spec.top.clone();
    let front_z = spec.top.last().unwrap().0;
    let rear_z = spec.top[0].0;
    pts.push((front_z - 0.1, spec.bottom_y));
    for axle in [spec.axle_front, spec.axle_rear] {
        let s = ((spec.bottom_y - wheel_y) / arch_r).clamp(-1.0, 1.0);
        let t0 = s.asin();
        pts.push((axle + arch_r * t0.cos(), spec.bottom_y));
        let steps = 14;
        for i in 0..=steps {
            let t = t0 + ((PI - 2.0 * t0) * i as f32) / steps as f32;
            pts.push((axle + arch_r * t.cos(), wheel_y + arch_r * t.sin()));
        }
        pts.push((axle - arch_r * t0.cos(), spec.bottom_y));
    }
    pts.push((rear_z + 0.1, spec.bottom_y));
    pts
}

/// Pushes a closed outline out by `amount` (each corner along the average of its edges' normals).
fn grow(shape: &[P], amount: f32) -> Vec<P> {
    let n = shape.len();
    let area: f32 = (0..n)
        .map(|i| shape[i].0 * shape[(i + 1) % n].1 - shape[(i + 1) % n].0 * shape[i].1)
        .sum();
    let out = if area >= 0.0 { 1.0 } else { -1.0 };
    (0..n)
        .map(|i| {
            let (a, b, c) = (shape[(i + n - 1) % n], shape[i], shape[(i + 1) % n]);
            let e1 = Vec2::new(b.0 - a.0, b.1 - a.1).normalize_or_zero();
            let e2 = Vec2::new(c.0 - b.0, c.1 - b.1).normalize_or_zero();
            let n1 = Vec2::new(e1.y, -e1.x) * out;
            let n2 = Vec2::new(e2.y, -e2.x) * out;
            let normal = (n1 + n2).normalize_or_zero();
            let scale = 1.0 / normal.dot(n1).max(0.5);
            (b.0 + normal.x * amount * scale, b.1 + normal.y * amount * scale)
        })
        .collect()
}

/// Extrudes a side profile across the car's width, centered on x = 0, with rounded edges.
/// The profile's first coordinate is the car's z (forward). `side_on` gives UVs in meters (z, y).
fn extrude_profile(shape: &[P], width: f32, bevel: f32, bevel_size: f32) -> MeshData {
    let depth = (width - 2.0 * bevel).max(0.01);
    // Layers across the car: the bevel rounds out to the full outline over 3 steps on each side.
    let mut layers: Vec<(f32, f32)> = Vec::new(); // (x, grow)
    let steps = if bevel > 0.0 { 3 } else { 0 };
    for i in 0..=steps {
        let a = (i as f32 / steps.max(1) as f32) * PI / 2.0;
        layers.push((-depth / 2.0 - bevel * a.cos(), bevel_size * a.sin()));
    }
    for i in (0..=steps).rev() {
        let a = (i as f32 / steps.max(1) as f32) * PI / 2.0;
        layers.push((depth / 2.0 + bevel * a.cos(), bevel_size * a.sin()));
    }
    if steps == 0 {
        layers = vec![(-depth / 2.0, 0.0), (depth / 2.0, 0.0)];
    }
    let n = shape.len();
    let area: f32 = (0..n)
        .map(|i| shape[i].0 * shape[(i + 1) % n].1 - shape[(i + 1) % n].0 * shape[i].1)
        .sum();
    let mut m = MeshData::default();
    let rings: Vec<Vec<P>> = layers
        .iter()
        .map(|(_, g)| if *g == 0.0 { shape.to_vec() } else { grow(shape, *g) })
        .collect();
    // The sides, ring to ring.
    for k in 0..layers.len() - 1 {
        let base = m.positions.len() as u32;
        for (ring, (x, _)) in [(&rings[k], layers[k]), (&rings[k + 1], layers[k + 1])] {
            for p in ring.iter() {
                m.positions.push([x, p.1, p.0]);
                m.uvs.push([p.0, p.1]);
            }
        }
        for i in 0..n as u32 {
            let j = (i + 1) % n as u32;
            let (a, b, c, d) = (base + i, base + j, base + n as u32 + j, base + n as u32 + i);
            if area >= 0.0 {
                m.indices.extend([a, c, b, a, d, c]);
            } else {
                m.indices.extend([a, b, c, a, c, d]);
            }
        }
    }
    // The two flat ends.
    let tris = crate::meshes::triangulate(shape);
    for (x, left) in [(layers[0].0, true), (layers[layers.len() - 1].0, false)] {
        let base = m.positions.len() as u32;
        for p in shape {
            m.positions.push([x, p.1, p.0]);
            m.uvs.push([p.0, p.1]);
        }
        // Triangles come back counter-clockwise in (z, y), which faces -x; flip them for the right end.
        for t in &tris {
            if left {
                m.indices.extend([base + t[0], base + t[1], base + t[2]]);
            } else {
                m.indices.extend([base + t[0], base + t[2], base + t[1]]);
            }
        }
    }
    m.compute_normals();
    m
}

/// Builds a car. `with_lights` adds a real headlight beam and underglow light
/// (only the player's car needs them: every light costs drawing time).
pub fn build_car(ctx: &mut CarBuildCtx, config: &CarConfig, night: bool, with_lights: bool) -> CarRig {
    let spec = stretch_body(
        body_spec(&config.body),
        config.length as f32,
        config.width as f32,
        config.height as f32,
    );
    let front_z = spec.top.iter().map(|p| p.0).fold(f32::MIN, f32::max) + BEVEL;
    let rear_z = spec.top.iter().map(|p| p.0).fold(f32::MAX, f32::min) - BEVEL;
    let top_y = spec.top.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let length = front_z - rear_z;

    // Materials
    let carbon_map = ctx.images.add(textures::carbon_texture().into_image(true));
    let mut mats: BTreeMap<Mat, Handle<StandardMaterial>> = BTreeMap::new();
    let paint = ctx.materials.add(paint_material(&config.paint_color, &config.finish, &carbon_map));
    let accent_finish = if config.finish == "chrome" || config.finish == "rainbow" {
        "gloss"
    } else {
        config.finish.as_str()
    };
    mats.insert(Mat::Paint, paint.clone());
    mats.insert(
        Mat::Accent,
        ctx.materials.add(paint_material(&config.accent_color, accent_finish, &carbon_map)),
    );
    mats.insert(Mat::Black, ctx.materials.add(plain(srgb("#111214"), 0.0, 0.6)));
    mats.insert(
        Mat::Carbon,
        ctx.materials.add(StandardMaterial {
            base_color_texture: Some(carbon_map.clone()),
            perceptual_roughness: 0.35,
            clearcoat: 1.0,
            metallic: 0.2,
            uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(4.0)),
            ..default()
        }),
    );
    mats.insert(Mat::Chrome, ctx.materials.add(plain(srgb("#dddddd"), 1.0, 0.15)));
    mats.insert(Mat::Rubber, ctx.materials.add(plain(srgb(&config.tire_color), 0.0, 0.92)));
    mats.insert(Mat::Rim, ctx.materials.add(plain(srgb(&config.rim_color), 0.85, 0.25)));
    let tint = config.window_tint as f32;
    mats.insert(
        Mat::Glass,
        ctx.materials.add(StandardMaterial {
            base_color: srgb("#9cc3d9").mix(&srgb("#06080a"), tint).with_alpha(0.55 + tint * 0.4),
            metallic: 0.3,
            perceptual_roughness: 0.05,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    );
    let head = ctx.materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: srgb(&config.headlight_color).to_linear() * 0.6,
        ..default()
    });
    let tail = ctx.materials.add(StandardMaterial {
        base_color: srgb("#550000"),
        emissive: srgb("#ff1a1a").to_linear() * 0.4,
        ..default()
    });
    mats.insert(Mat::Head, head.clone());
    mats.insert(Mat::Tail, tail.clone());
    let part_mat = |key: &str| match key {
        "body" => Mat::Paint,
        "accent" => Mat::Accent,
        "black" => Mat::Black,
        _ => Mat::Carbon,
    };

    // A paint pattern goes on a copy of the paint, used only for the main body.
    let mut body_paint = paint.clone();
    if config.pattern != "none" {
        let livery = textures::livery_texture(
            &config.pattern,
            &config.paint_color,
            &config.accent_color,
            length as f64,
            (top_y - spec.bottom_y) as f64,
        );
        let mut m = paint_material(&config.paint_color, &config.finish, &carbon_map);
        m.base_color_texture = Some(ctx.images.add(livery.into_image(false)));
        m.base_color = Color::WHITE;
        // The shell's UVs are meters, side-on; map them onto the picture (whose top row is the top of the body).
        let h = top_y - spec.bottom_y;
        m.uv_transform =
            bevy::math::Affine2::from_cols_array(&[1.0 / length, 0.0, 0.0, -1.0 / h, -rear_z / length, 1.0 + spec.bottom_y / h]);
        body_paint = ctx.materials.add(m);
    }
    mats.insert(Mat::BodyPaint, body_paint.clone());

    // Parts collected per group and material, merged at the end.
    let mut body_parts: BTreeMap<Mat, MeshData> = BTreeMap::new();
    let mut no_shadow: BTreeMap<Mat, MeshData> = BTreeMap::new();
    // The body shell, cabin, roof, body stripes and lamps: what a real car body (Photo Real) replaces.
    let mut shell: BTreeMap<Mat, MeshData> = BTreeMap::new();
    let mut shell_no_shadow: BTreeMap<Mat, MeshData> = BTreeMap::new();
    fn put(parts: &mut BTreeMap<Mat, MeshData>, mat: Mat, mesh: &Mesh, t: Transform) {
        parts.entry(mat).or_default().add_mesh(mesh, &t, [1.0; 4]);
    }
    let at = |x: f32, y: f32, z: f32| Transform::from_xyz(x, y, z);

    // ---------- wheels and suspension height ----------
    let big_r = spec.wheel_radius;
    let r = big_r * config.wheel_size as f32;
    let mut wheel_y = big_r - config.lift as f32;
    let arch_max = ((spec.axle_front - spec.axle_rear) / 2.0)
        .min(front_z - spec.axle_front)
        .min(spec.axle_rear - rear_z)
        - 0.05;
    let mut arch_r = r + 0.07 + BEVEL;
    if arch_r > arch_max {
        arch_r = arch_max;
        wheel_y = wheel_y.min(spec.bottom_y - r - 0.08);
    }
    let body_y = r - wheel_y;

    // ---------- main body and cabin ----------
    let lower = lower_profile(&spec, arch_r, wheel_y);
    shell
        .entry(Mat::BodyPaint)
        .or_default()
        .append(&extrude_profile(&lower, spec.width, BEVEL, BEVEL));
    let glass = extrude_profile(&spec.cabin, spec.cabin_width, 0.03, 0.03);
    shell_no_shadow.entry(Mat::Glass).or_default().append(&glass);
    let [_, roof_rear, roof_front, _] = spec.cabin;
    let roof_len = roof_front.0 - roof_rear.0;
    let roof_y = roof_rear.1.max(roof_front.1);
    let roof_mid = (roof_rear.0 + roof_front.0) / 2.0;
    let roof_top = roof_y + 0.045;
    put(
        &mut shell,
        Mat::Paint,
        &Cuboid::new(spec.cabin_width * 0.97, 0.05, roof_len + 0.06).mesh().build(),
        at(0.0, roof_y + 0.02, roof_mid),
    );
    if let Some((z0, z1, y)) = spec.bed {
        put(
            &mut body_parts,
            Mat::Black,
            &Cuboid::new(spec.width - 0.22, 0.02, z1 - z0).mesh().build(),
            at(0.0, y + BEVEL, (z0 + z1) / 2.0),
        );
    }

    // ---------- stripes and race number ----------
    if config.stripes == "racing" {
        for side in [-1.0, 1.0] {
            let mut stripe = extrude_profile(&lower, 0.22, 0.004, BEVEL + 0.006);
            for p in &mut stripe.positions {
                p[0] += side * 0.2;
            }
            shell.entry(Mat::Accent).or_default().append(&stripe);
            put(
                &mut shell,
                Mat::Accent,
                &Cuboid::new(0.22, 0.012, roof_len + 0.06).mesh().build(),
                at(side * 0.2, roof_y + 0.05, roof_mid),
            );
        }
    } else if config.stripes == "side" {
        let z0 = spec.axle_rear + arch_r;
        let z1 = spec.axle_front - arch_r;
        let mid_z = (z0 + z1) / 2.0;
        let y = (spec.bottom_y + top_at(&spec, mid_z)) / 2.0 + 0.05;
        for side in [-1.0, 1.0] {
            put(
                &mut body_parts,
                Mat::Accent,
                &Cuboid::new(0.012, 0.09, (z1 - z0).max(0.1)).mesh().build(),
                at(side * (spec.width / 2.0 + 0.004), y, mid_z),
            );
            put(
                &mut body_parts,
                Mat::Accent,
                &Cuboid::new(0.012, 0.03, (z1 - z0).max(0.1)).mesh().build(),
                at(side * (spec.width / 2.0 + 0.004), y - 0.09, mid_z),
            );
        }
    }
    if config.race_number > 0.0 {
        let image = ctx
            .images
            .add(textures::race_number_texture(config.race_number as u32).into_image(false));
        mats.insert(
            Mat::Number,
            ctx.materials.add(StandardMaterial {
                base_color_texture: Some(image),
                alpha_mode: AlphaMode::Mask(0.5),
                perceptual_roughness: 0.5,
                ..default()
            }),
        );
        let mid_z = (spec.axle_rear + spec.axle_front) / 2.0;
        let size = 0.5f32.min((top_at(&spec, mid_z) - spec.bottom_y) * 0.8);
        for side in [-1.0f32, 1.0] {
            let t = at(
                side * (spec.width / 2.0 + 0.012),
                (spec.bottom_y + top_at(&spec, mid_z)) / 2.0 + 0.03,
                mid_z,
            )
            .with_rotation(Quat::from_rotation_y(side * PI / 2.0));
            put(&mut no_shadow, Mat::Number, &Rectangle::new(size, size).mesh().build(), t);
        }
    }

    // ---------- spoiler ----------
    if config.spoiler != "none" {
        let mat = part_mat(&config.spoiler_color);
        let (sz, sy) = spec.spoiler;
        let base_y = sy + BEVEL;
        if config.spoiler == "lip" {
            put(
                &mut body_parts,
                mat,
                &Cuboid::new(spec.width * 0.86, 0.05, 0.2).mesh().build(),
                at(0.0, base_y + 0.03, sz - 0.05).with_rotation(Quat::from_rotation_x(0.2)),
            );
        } else {
            let big = config.spoiler != "wing";
            let post_h = if config.spoiler == "drag" {
                1.0
            } else if big {
                0.38
            } else {
                0.22
            };
            for side in [-1.0, 1.0] {
                put(
                    &mut body_parts,
                    mat,
                    &Cuboid::new(0.05, post_h, 0.12).mesh().build(),
                    at(side * spec.width * 0.3, base_y + post_h / 2.0, sz),
                );
            }
            let plank_w = spec.width * if big { 1.02 } else { 0.9 };
            let plank_d = if config.spoiler == "drag" {
                0.6
            } else if big {
                0.4
            } else {
                0.3
            };
            put(
                &mut body_parts,
                mat,
                &Cuboid::new(plank_w, 0.045, plank_d).mesh().build(),
                at(0.0, base_y + post_h + 0.02, sz - 0.04).with_rotation(Quat::from_rotation_x(-0.12)),
            );
            if big {
                for side in [-1.0, 1.0] {
                    put(
                        &mut body_parts,
                        mat,
                        &Cuboid::new(0.025, 0.2, plank_d + 0.04).mesh().build(),
                        at(side * (plank_w / 2.0 + 0.012), base_y + post_h + 0.02, sz - 0.04),
                    );
                }
            }
        }
    }

    // ---------- lights and plates ----------
    let (hz, hy) = spec.headlight;
    for side in [-1.0, 1.0] {
        put(
            &mut shell_no_shadow,
            Mat::Head,
            &Cuboid::new(0.34, 0.1, 0.1).mesh().build(),
            at(side * (spec.width / 2.0 - 0.3), hy, hz + BEVEL),
        );
    }
    let (tz, ty) = spec.taillight;
    for side in [-1.0, 1.0] {
        put(
            &mut shell_no_shadow,
            Mat::Tail,
            &Cuboid::new(0.34, 0.08, 0.06).mesh().build(),
            at(side * (spec.width / 2.0 - 0.26), ty, tz - BEVEL),
        );
    }
    let plate_image = ctx.images.add(textures::plate_texture(&config.plate).into_image(false));
    mats.insert(
        Mat::Plate,
        ctx.materials.add(StandardMaterial {
            base_color_texture: Some(plate_image),
            perceptual_roughness: 0.5,
            ..default()
        }),
    );
    let plate_y = (spec.bottom_y + 0.14).max(ty - 0.24);
    let plate = Rectangle::new(0.52, 0.19).mesh().build();
    put(
        &mut body_parts,
        Mat::Plate,
        &plate,
        at(0.0, plate_y, rear_z - 0.012).with_rotation(Quat::from_rotation_y(PI)),
    );
    put(
        &mut body_parts,
        Mat::Plate,
        &plate,
        at(0.0, (spec.bottom_y + 0.14).max(hy - 0.24), front_z + 0.012),
    );

    // ---------- exhausts ----------
    let mut tips: Vec<(f32, f32, f32, bool)> = Vec::new();
    let pipe_r = match config.exhaust.as_str() {
        "quad" => 0.038,
        "stacks" => 0.06,
        _ => 0.048,
    };
    if config.exhaust == "stacks" {
        let z = (rear_z + 0.25).max(spec.cabin[0].0 - 0.15);
        let base_y = top_at(&spec, z) + BEVEL;
        for side in [-1.0, 1.0] {
            let x = side * (spec.width / 2.0 - 0.14);
            put(
                &mut body_parts,
                Mat::Chrome,
                &Cylinder::new(pipe_r, 1.0).mesh().resolution(14).build(),
                at(x, base_y + 0.5, z),
            );
            tips.push((x, base_y + 1.02, z, true));
        }
    } else {
        let w = spec.width;
        let xs: Vec<f32> = match config.exhaust.as_str() {
            "single" => vec![-w * 0.3],
            "dual" => vec![-w * 0.3, w * 0.3],
            _ => vec![-w * 0.36, -w * 0.27, w * 0.27, w * 0.36],
        };
        for x in xs {
            put(
                &mut body_parts,
                Mat::Chrome,
                &Cylinder::new(pipe_r, 0.3).mesh().resolution(14).build(),
                at(x, spec.bottom_y + 0.06, rear_z + 0.06).with_rotation(Quat::from_rotation_x(PI / 2.0)),
            );
            tips.push((x, spec.bottom_y + 0.06, rear_z - 0.08, false));
        }
    }

    // ---------- roof weapons ----------
    let mut gun_muzzle = Vec3::new(0.32, body_y + roof_top + 0.2, roof_mid + 0.9);
    let mut rocket_muzzle = Vec3::new(-0.32, body_y + roof_top + 0.22, roof_mid + 0.55);
    let roof_gun = config.roof_guns && config.armed.gun;
    let roof_pod = config.roof_guns && config.armed.rockets;
    let gun_metal = roof_gun || roof_pod;
    if gun_metal {
        mats.insert(Mat::GunMetal, ctx.materials.add(plain(srgb("#2b2e33"), 0.8, 0.35)));
        put(
            &mut body_parts,
            Mat::Black,
            &Cuboid::new(1.0, 0.06, 0.9).mesh().build(),
            at(0.0, roof_top + 0.03, roof_mid),
        );
    }
    if roof_gun {
        put(
            &mut body_parts,
            Mat::GunMetal,
            &Cuboid::new(0.2, 0.16, 0.5).mesh().build(),
            at(0.32, roof_top + 0.2, roof_mid + 0.05),
        );
        put(
            &mut body_parts,
            Mat::GunMetal,
            &tapered_cylinder(0.05, 0.07, 0.12, 10),
            at(0.32, roof_top + 0.1, roof_mid),
        );
        for dx in [-0.045, 0.045] {
            put(
                &mut body_parts,
                Mat::GunMetal,
                &Cylinder::new(0.022, 0.6).mesh().resolution(10).build(),
                at(0.32 + dx, roof_top + 0.2, roof_mid + 0.6).with_rotation(Quat::from_rotation_x(PI / 2.0)),
            );
        }
        put(
            &mut body_parts,
            Mat::Accent,
            &Cuboid::new(0.14, 0.12, 0.2).mesh().build(),
            at(0.47, roof_top + 0.12, roof_mid - 0.05),
        );
    } else {
        gun_muzzle = Vec3::new(0.3, body_y + spec.headlight.1 + 0.2, front_z + 0.1);
    }
    if roof_pod {
        put(
            &mut body_parts,
            Mat::GunMetal,
            &Cuboid::new(0.34, 0.3, 0.7).mesh().build(),
            at(-0.32, roof_top + 0.22, roof_mid + 0.05),
        );
        for dx in [-0.075, 0.075] {
            for dy in [-0.07, 0.07] {
                let turn = Quat::from_rotation_x(PI / 2.0);
                put(
                    &mut body_parts,
                    Mat::Black,
                    &Cylinder::new(0.055, 0.04).mesh().resolution(12).build(),
                    at(-0.32 + dx, roof_top + 0.22 + dy, roof_mid + 0.41).with_rotation(turn),
                );
                put(
                    &mut body_parts,
                    Mat::Accent,
                    &Cone {
                        radius: 0.045,
                        height: 0.1,
                    }
                    .mesh()
                    .resolution(10)
                    .build(),
                    at(-0.32 + dx, roof_top + 0.22 + dy, roof_mid + 0.44).with_rotation(turn),
                );
            }
        }
    } else {
        rocket_muzzle = Vec3::new(-0.3, body_y + spec.headlight.1 + 0.2, front_z + 0.1);
    }

    // ---------- topper ----------
    if config.topper != "none" {
        let base = at(
            0.0,
            roof_top + if gun_metal { 0.06 } else { 0.0 },
            roof_mid - if gun_metal { 0.15 } else { 0.0 },
        );
        build_topper(ctx, &config.topper, &mut mats, &mut body_parts, &mut no_shadow, base);
    }

    // ---------- antenna ----------
    let mut flag_part: Option<(Transform, MeshData)> = None;
    if config.antenna != "none" {
        let z = rear_z + 0.45;
        let base = Vec3::new(-(spec.width / 2.0 - 0.15), top_at(&spec, z) + BEVEL, z);
        let place = |y: f32| Transform::from_translation(base + Vec3::Y * y);
        match config.antenna.as_str() {
            "whip" => {
                put(&mut body_parts, Mat::Black, &tapered_cylinder(0.006, 0.012, 1.1, 6), place(0.55));
                put(&mut body_parts, Mat::Black, &Sphere::new(0.02).mesh().uv(8, 6), place(1.1));
            }
            "flag" => {
                put(
                    &mut body_parts,
                    Mat::Chrome,
                    &Cylinder::new(0.01, 1.0).mesh().resolution(8).build(),
                    place(0.5),
                );
                // The flag hangs off the back of the pole; it waves (see animate).
                let cloth = Rectangle::new(0.45, 0.28).mesh().build();
                let mut m = MeshData::from_mesh(&cloth, &Transform::from_xyz(-0.225, 0.0, 0.0), [1.0; 4]);
                m.double_sided();
                flag_part = Some((place(0.85), m));
            }
            _ => {
                put(
                    &mut body_parts,
                    Mat::Black,
                    &Cylinder::new(0.008, 0.6).mesh().resolution(6).build(),
                    place(0.3),
                );
                put(&mut body_parts, Mat::Accent, &Sphere::new(0.09).mesh().uv(16, 12), place(0.66));
            }
        }
    }

    if config.bull_bar {
        let z = front_z + 0.14;
        let top = hy + 0.12;
        for side in [-1.0, 1.0] {
            put(
                &mut body_parts,
                Mat::Chrome,
                &Cylinder::new(0.035, top - spec.bottom_y).mesh().resolution(10).build(),
                at(side * spec.width * 0.25, (top + spec.bottom_y) / 2.0, z),
            );
        }
        for y in [spec.bottom_y + 0.12, top] {
            put(
                &mut body_parts,
                Mat::Chrome,
                &Cylinder::new(0.035, spec.width * 0.75).mesh().resolution(10).build(),
                at(0.0, y, z).with_rotation(Quat::from_rotation_z(PI / 2.0)),
            );
        }
    }
    if config.light_bar {
        let bar_width = (spec.cabin_width * 0.9).min(1.4);
        let z = roof_front.0 - 0.12;
        put(
            &mut body_parts,
            Mat::Black,
            &Cuboid::new(bar_width, 0.08, 0.14).mesh().build(),
            at(0.0, roof_top + 0.05, z),
        );
        for i in 0..6 {
            put(
                &mut no_shadow,
                Mat::Head,
                &Cuboid::new(bar_width / 7.0, 0.05, 0.02).mesh().build(),
                at((i as f32 - 2.5) * (bar_width / 6.2), roof_top + 0.05, z + 0.075),
            );
        }
    }

    // ---------- spawn the body ----------
    let scale = config.size as f32;
    let root = ctx
        .commands
        .spawn((Transform::from_scale(Vec3::splat(scale)), Visibility::default(), Name::new("car")))
        .id();
    let body = ctx
        .commands
        .spawn((Transform::from_xyz(0.0, body_y, 0.0), Visibility::default(), ChildOf(root)))
        .id();
    let mut lit_parts = Vec::new();
    let mut shell_parts = Vec::new();
    for (shadow, is_shell, parts) in [
        (true, false, body_parts),
        (false, false, no_shadow),
        (true, true, shell),
        (false, true, shell_no_shadow),
    ] {
        for (key, data) in parts {
            let mat = mats[&key].clone();
            let mut e = ctx.commands.spawn((
                Mesh3d(ctx.meshes.add(data.build_editable())),
                MeshMaterial3d(mat.clone()),
                Transform::default(),
                ChildOf(body),
            ));
            if !shadow {
                e.insert(NotShadowCaster);
            }
            if is_shell {
                shell_parts.push(e.id());
            }
            lit_parts.push((e.id(), mat));
        }
    }
    let flag = flag_part.map(|(t, data)| {
        let cloth = ctx.materials.add(StandardMaterial {
            base_color: srgb(&config.accent_color),
            perceptual_roughness: 0.8,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let pivot = ctx.commands.spawn((t, Visibility::default(), ChildOf(body))).id();
        let e = ctx
            .commands
            .spawn((
                Mesh3d(ctx.meshes.add(data.build())),
                MeshMaterial3d(cloth.clone()),
                Transform::from_rotation(Quat::from_rotation_y(-PI / 2.0)),
                NotShadowCaster,
                ChildOf(pivot),
            ))
            .id();
        lit_parts.push((e, cloth));
        e
    });

    // Nitro flames: two nested cones at each tip, hidden until boosting.
    let flame_mesh = ctx.meshes.add({
        let cone = Cone { radius: 1.0, height: 1.0 }.mesh().resolution(12).build();
        // Wide at the pipe, pointed 1 m behind it.
        MeshData::from_mesh(
            &cone,
            &Transform::from_rotation(Quat::from_rotation_x(-PI / 2.0)).with_translation(Vec3::new(0.0, 0.0, -0.5)),
            [1.0; 4],
        )
        .build()
    });
    let flame_outer = ctx.materials.add(StandardMaterial {
        base_color: srgb(&config.nitro_color).with_alpha(0.8),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let flame_core = ctx.materials.add(StandardMaterial {
        base_color: Color::WHITE.with_alpha(0.9),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let mut flames = Vec::new();
    for &(x, y, z, up) in &tips {
        let rot = if up { Quat::from_rotation_x(-PI / 2.0) } else { Quat::IDENTITY };
        let flame = ctx
            .commands
            .spawn((Transform::from_xyz(x, y, z).with_rotation(rot), Visibility::Hidden, ChildOf(body)))
            .id();
        ctx.commands.spawn((
            Mesh3d(flame_mesh.clone()),
            MeshMaterial3d(flame_outer.clone()),
            Transform::from_scale(Vec3::new(pipe_r * 2.2, pipe_r * 2.2, 0.9)),
            NotShadowCaster,
            ChildOf(flame),
        ));
        ctx.commands.spawn((
            Mesh3d(flame_mesh.clone()),
            MeshMaterial3d(flame_core.clone()),
            Transform::from_scale(Vec3::new(pipe_r * 1.1, pipe_r * 1.1, 0.45)),
            NotShadowCaster,
            ChildOf(flame),
        ));
        flames.push(flame);
    }
    let exhausts = tips
        .iter()
        .map(|&(x, y, z, up)| Vec3::new(x, y + if up { 0.3 } else { 0.0 } + body_y, z - if up { 0.0 } else { 0.3 }))
        .collect();

    // Headlight beam.
    let beam = with_lights.then(|| {
        ctx.commands
            .spawn((
                SpotLight {
                    color: srgb(&config.headlight_color),
                    intensity: 0.0,
                    range: 70.0 * scale,
                    outer_angle: 0.55,
                    inner_angle: 0.55 * 0.4,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, hy, front_z).looking_at(Vec3::new(0.0, -1.5, front_z + 20.0), Vec3::Y),
                ChildOf(body),
            ))
            .id()
    });

    // ---------- wheels ----------
    let tire_w = spec.tire_width * config.tire_width as f32;
    let wheel_x = spec.track * config.track as f32 + (tire_w - spec.tire_width) / 2.0;
    let camber = config.camber as f32 * PI / 180.0;
    let wheel_meshes = build_wheel(&config.wheel_style, r, tire_w);
    let wheel_handles: Vec<(Mat, Handle<Mesh>)> = wheel_meshes.into_iter().map(|(m, d)| (m, ctx.meshes.add(d.build()))).collect();
    let mut wheels = Vec::new();
    for (z, front) in [(spec.axle_front, true), (spec.axle_rear, false)] {
        for side in [-1.0f32, 1.0] {
            let steer = ctx
                .commands
                .spawn((Transform::from_xyz(side * wheel_x, r, z), Visibility::default(), ChildOf(root)))
                .id();
            let tilt = ctx
                .commands
                .spawn((
                    Transform::from_rotation(Quat::from_rotation_z(-side * camber)),
                    Visibility::default(),
                    ChildOf(steer),
                ))
                .id();
            let spin = ctx
                .commands
                .spawn((Transform::default(), Visibility::default(), ChildOf(tilt)))
                .id();
            let face = if side < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
            for (mat, mesh) in &wheel_handles {
                let m = mats[mat].clone();
                let e = ctx
                    .commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(m.clone()),
                        Transform::from_rotation(face),
                        ChildOf(spin),
                    ))
                    .id();
                lit_parts.push((e, m));
            }
            wheels.push(WheelRig {
                steer,
                spin,
                front,
                radius: (r * scale) as f64,
            });
        }
    }
    // A lifted body gets a shock absorber from each wheel up to the body.
    let body_bottom = body_y + spec.bottom_y;
    if body_bottom - r > 0.25 {
        let mut struts = MeshData::default();
        for z in [spec.axle_front, spec.axle_rear] {
            for side in [-1.0, 1.0] {
                let mesh = Cylinder::new(0.05, body_bottom - r + 0.1).mesh().resolution(10).build();
                struts.add_mesh(
                    &mesh,
                    &at(side * (wheel_x - tire_w / 2.0 - 0.08).max(0.2), (body_bottom + r) / 2.0, z),
                    [1.0; 4],
                );
            }
        }
        let m = mats[&Mat::Chrome].clone();
        let e = ctx
            .commands
            .spawn((
                Mesh3d(ctx.meshes.add(struts.build())),
                MeshMaterial3d(m.clone()),
                Transform::default(),
                ChildOf(root),
            ))
            .id();
        lit_parts.push((e, m));
    }

    // ---------- underglow ----------
    let glow_light = with_lights.then(|| {
        ctx.commands
            .spawn((
                PointLight {
                    color: srgb(&config.underglow_color),
                    intensity: 0.0,
                    range: 6.0 * scale,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.15, 0.0),
                ChildOf(root),
            ))
            .id()
    });
    let mut glow = None;
    let mut glow_decal = None;
    if config.underglow != "off" {
        let glow_map = ctx.images.add(crate::canvas::Canvas::radial_glow(128).into_image(false));
        let gm = ctx.materials.add(StandardMaterial {
            base_color: srgb(&config.underglow_color),
            base_color_texture: Some(glow_map),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        });
        let decal = Rectangle::new(spec.width + 1.6, length + 1.4).mesh().build();
        let e = ctx
            .commands
            .spawn((
                Mesh3d(ctx.meshes.add(decal)),
                MeshMaterial3d(gm.clone()),
                Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_x(-PI / 2.0)),
                NotShadowCaster,
                bevy::light::NotShadowReceiver,
                ChildOf(root),
            ))
            .id();
        glow = Some(gm);
        glow_decal = Some(e);
    }

    let burned = ctx.materials.add(plain(srgb("#1c1a18"), 0.2, 1.0));
    let mut rig = CarRig {
        root,
        body,
        wheels,
        length: (length * scale) as f64,
        width: (spec.width.max(2.0 * (wheel_x + tire_w / 2.0)) * scale) as f64,
        height: ((body_y + roof_top) * scale) as f64,
        wheel_base: ((spec.axle_front - spec.axle_rear) * scale) as f64,
        eye_height: ((roof_y - 0.2 + body_y) * scale) as f64,
        exhausts,
        gun_muzzle,
        rocket_muzzle,
        body_y,
        config: config.clone(),
        shape: CarShape {
            spec,
            wheel_y,
            arch_r,
            wheel_x,
            tire_width: tire_w,
            front_z,
            rear_z,
        },
        mats: CarMaterials {
            glass: mats[&Mat::Glass].clone(),
            paint,
            body_paint,
            head,
            tail,
            glow,
            burned,
        },
        flames,
        flag,
        beam,
        glow_light,
        glow_decal,
        lit_parts,
        shell: shell_parts,
        wrecked: false,
        lights: (night, false),
    };
    rig.set_lights(night, false, ctx.materials, ctx.commands);
    // Lets packs find every car and change how it looks.
    ctx.commands.entity(root).insert(CarLook(Box::new(rig.clone())));
    rig
}

/// On every car's root: what it was built from, for packs that redraw cars.
#[derive(Component, Clone, Debug)]
pub struct CarLook(pub Box<CarRig>);

/// On a car that mustn't be redrawn by packs (the see-through ghost).
#[derive(Component)]
pub struct Ghost;

/// The parts of a wheel, by material, in the wheel's own space (axle along x, rim facing +x).
fn build_wheel(style: &str, radius: f32, width: f32) -> Vec<(Mat, MeshData)> {
    let mut parts: BTreeMap<Mat, MeshData> = BTreeMap::new();
    let mut put = |mat: Mat, mesh: &Mesh, t: Transform| parts.entry(mat).or_default().add_mesh(mesh, &t, [1.0; 4]);
    let axle = Quat::from_rotation_z(PI / 2.0);
    put(
        Mat::Rubber,
        &Cylinder::new(radius, width).mesh().resolution(28).build(),
        Transform::from_rotation(axle),
    );
    let face = width / 2.0 + 0.006;
    let rim_r = radius * if style == "offroad" { 0.56 } else { 0.66 };
    let disc = |put: &mut dyn FnMut(Mat, &Mesh, Transform), r: f32, thick: f32, mat: Mat, x: f32| {
        put(
            mat,
            &Cylinder::new(r, thick).mesh().resolution(24).build(),
            Transform::from_xyz(x, 0.0, 0.0).with_rotation(axle),
        );
    };
    let ring = |put: &mut dyn FnMut(Mat, &Mesh, Transform), r: f32, tube: f32| {
        // Bevy's torus lies flat (around y); stand it up to face along x.
        put(
            Mat::Rim,
            &Torus::new(r - tube, r + tube)
                .mesh()
                .minor_resolution(8)
                .major_resolution(28)
                .build(),
            Transform::from_xyz(face, 0.0, 0.0).with_rotation(Quat::from_rotation_z(PI / 2.0)),
        );
    };
    let spokes = |put: &mut dyn FnMut(Mat, &Mesh, Transform), count: usize, thickness: f32| {
        for i in 0..count {
            let pivot = Transform::from_rotation(Quat::from_rotation_x(i as f32 / count as f32 * PI * 2.0));
            let spoke = Transform::from_xyz(face, rim_r * 0.48, 0.0);
            put(Mat::Rim, &Cuboid::new(0.03, rim_r * 0.95, thickness).mesh().build(), pivot * spoke);
        }
    };
    let bolts = |put: &mut dyn FnMut(Mat, &Mesh, Transform), count: usize, r: f32, size: f32, mat: Mat| {
        for i in 0..count {
            let a = i as f32 / count as f32 * PI * 2.0;
            put(
                mat,
                &Cylinder::new(size, 0.03).mesh().resolution(8).build(),
                Transform::from_xyz(face + 0.012, a.cos() * r, a.sin() * r).with_rotation(axle),
            );
        }
    };
    match style {
        "fiveSpoke" => {
            ring(&mut put, rim_r, 0.028);
            spokes(&mut put, 5, 0.075);
            disc(&mut put, rim_r * 0.22, 0.05, Mat::Rim, face);
        }
        "multiSpoke" => {
            ring(&mut put, rim_r, 0.024);
            ring(&mut put, rim_r * 0.55, 0.014);
            spokes(&mut put, 12, 0.028);
            disc(&mut put, rim_r * 0.18, 0.05, Mat::Rim, face);
        }
        "dish" => {
            disc(&mut put, rim_r, 0.02, Mat::Rim, face - 0.004);
            disc(&mut put, rim_r * 0.25, 0.04, Mat::Rim, face + 0.01);
            bolts(&mut put, 5, rim_r * 0.42, 0.018, Mat::Black);
        }
        _ => {
            disc(&mut put, rim_r, 0.02, Mat::Rim, face - 0.004);
            ring(&mut put, rim_r * 0.98, 0.03);
            bolts(&mut put, 6, rim_r * 0.62, 0.05, Mat::Black);
            disc(&mut put, rim_r * 0.2, 0.05, Mat::Rim, face + 0.01);
            for i in 0..16 {
                let pivot = Transform::from_rotation(Quat::from_rotation_x(i as f32 / 16.0 * PI * 2.0));
                put(
                    Mat::Rubber,
                    &Cuboid::new(width * 1.04, 0.05, 0.08).mesh().build(),
                    pivot * Transform::from_xyz(0.0, radius, 0.0),
                );
            }
        }
    }
    parts.into_iter().collect()
}

/// The top half of a sphere, standing on y = 0.
fn dome(radius: f32) -> Mesh {
    let mut m = MeshData::default();
    let (rings, segs) = (10u32, 20u32);
    for i in 0..=rings {
        let theta = i as f32 / rings as f32 * PI / 2.0;
        for j in 0..=segs {
            let phi = j as f32 / segs as f32 * PI * 2.0;
            let n = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
            m.positions.push((n * radius).to_array());
            m.normals.push(n.to_array());
            m.uvs.push([j as f32 / segs as f32, i as f32 / rings as f32]);
        }
    }
    for i in 0..rings {
        for j in 0..segs {
            let a = i * (segs + 1) + j;
            let b = a + segs + 1;
            m.indices.extend([a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    m.build()
}

/// A roof topper, standing on `base`.
fn build_topper(
    ctx: &mut CarBuildCtx,
    kind: &str,
    mats: &mut BTreeMap<Mat, Handle<StandardMaterial>>,
    parts: &mut BTreeMap<Mat, MeshData>,
    no_shadow: &mut BTreeMap<Mat, MeshData>,
    base: Transform,
) {
    let mut mat = |ctx: &mut CarBuildCtx, key: Mat, m: StandardMaterial| {
        mats.entry(key).or_insert_with(|| ctx.materials.add(m));
    };
    let mut put = |key: Mat, mesh: &Mesh, t: Transform| parts.entry(key).or_default().add_mesh(mesh, &(base * t), [1.0; 4]);
    let at = |x: f32, y: f32, z: f32| Transform::from_xyz(x, y, z);
    match kind {
        "crown" => {
            mat(
                ctx,
                Mat::Gold,
                StandardMaterial {
                    double_sided: true,
                    cull_mode: None,
                    ..plain(srgb("#d4af37"), 1.0, 0.25)
                },
            );
            mat(ctx, Mat::Gem, plain(srgb("#d7263d"), 0.2, 0.2));
            put(Mat::Gold, &tapered_cylinder(0.17, 0.15, 0.1, 20), at(0.0, 0.05, 0.0));
            for i in 0..5 {
                let a = i as f32 / 5.0 * PI * 2.0;
                put(
                    Mat::Gold,
                    &Cone {
                        radius: 0.035,
                        height: 0.11,
                    }
                    .mesh()
                    .resolution(8)
                    .build(),
                    at(a.sin() * 0.16, 0.15, a.cos() * 0.16),
                );
                put(
                    Mat::Gem,
                    &Sphere::new(0.022).mesh().uv(8, 6),
                    at(a.sin() * 0.175, 0.05, a.cos() * 0.175),
                );
            }
        }
        "topHat" => {
            mat(ctx, Mat::HatBlack, plain(srgb("#111111"), 0.0, 0.7));
            put(
                Mat::HatBlack,
                &Cylinder::new(0.22, 0.02).mesh().resolution(24).build(),
                at(0.0, 0.01, 0.0),
            );
            put(
                Mat::HatBlack,
                &Cylinder::new(0.13, 0.3).mesh().resolution(20).build(),
                at(0.0, 0.17, 0.0),
            );
            put(
                Mat::Accent,
                &Cylinder::new(0.133, 0.06).mesh().resolution(20).build(),
                at(0.0, 0.06, 0.0),
            );
        }
        "cone" => {
            mat(ctx, Mat::Orange, plain(srgb("#ff6a00"), 0.0, 0.6));
            mat(ctx, Mat::White, plain(Color::WHITE, 0.0, 0.5));
            put(Mat::Orange, &Cuboid::new(0.32, 0.03, 0.32).mesh().build(), at(0.0, 0.015, 0.0));
            put(
                Mat::Orange,
                &Cone {
                    radius: 0.14,
                    height: 0.45,
                }
                .mesh()
                .resolution(16)
                .build(),
                at(0.0, 0.25, 0.0),
            );
            put(Mat::White, &tapered_cylinder(0.075, 0.095, 0.07, 16), at(0.0, 0.22, 0.0));
        }
        "horns" => {
            mat(ctx, Mat::Helmet, plain(srgb("#9aa0a6"), 0.9, 0.3));
            mat(ctx, Mat::Horn, plain(srgb("#f2e8d0"), 0.0, 0.5));
            put(Mat::Helmet, &dome(0.17), Transform::IDENTITY);
            for side in [-1.0, 1.0] {
                put(
                    Mat::Horn,
                    &Cone {
                        radius: 0.055,
                        height: 0.4,
                    }
                    .mesh()
                    .resolution(12)
                    .build(),
                    at(side * 0.22, 0.2, 0.0).with_rotation(Quat::from_rotation_z(-side * 0.8)),
                );
            }
        }
        "sharkFin" => {
            // A curved fin, drawn side-on.
            let mut fin: Vec<P> = vec![(-0.3, 0.0), (0.15, 0.0)];
            let quad = |a: P, c: P, b: P, out: &mut Vec<P>| {
                for i in 1..=8 {
                    let t = i as f32 / 8.0;
                    let u = 1.0 - t;
                    out.push((
                        u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0,
                        u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1,
                    ));
                }
            };
            quad((0.15, 0.0), (-0.05, 0.15), (-0.25, 0.45), &mut fin);
            quad((-0.25, 0.45), (-0.22, 0.2), (-0.3, 0.0), &mut fin);
            fin.pop();
            parts.entry(Mat::Paint).or_default().append(&{
                let mut m = extrude_profile(&fin, 0.05, 0.01, 0.01);
                for p in &mut m.positions {
                    *p = base.transform_point(Vec3::from(*p)).to_array();
                }
                m
            });
        }
        "halo" => {
            mat(
                ctx,
                Mat::Halo,
                StandardMaterial {
                    base_color: srgb("#ffe680"),
                    unlit: true,
                    ..default()
                },
            );
            let ring = Torus::new(0.19, 0.25).mesh().minor_resolution(8).major_resolution(32).build();
            no_shadow
                .entry(Mat::Halo)
                .or_default()
                .add_mesh(&ring, &(base * at(0.0, 0.45, 0.0)), [1.0; 4]);
        }
        "duck" => {
            mat(ctx, Mat::Duck, plain(srgb("#ffd23f"), 0.0, 0.35));
            mat(ctx, Mat::Beak, plain(srgb("#ff7a1a"), 0.0, 0.5));
            mat(ctx, Mat::Eye, plain(srgb("#111111"), 0.0, 0.5));
            put(
                Mat::Duck,
                &Sphere::new(0.16).mesh().uv(20, 14),
                at(0.0, 0.12, 0.0).with_scale(Vec3::new(1.0, 0.75, 1.25)),
            );
            put(Mat::Duck, &Sphere::new(0.1).mesh().uv(16, 12), at(0.0, 0.3, 0.1));
            put(
                Mat::Beak,
                &Cone { radius: 0.04, height: 0.1 }.mesh().resolution(10).build(),
                at(0.0, 0.29, 0.22).with_rotation(Quat::from_rotation_x(PI / 2.0)),
            );
            for side in [-1.0, 1.0] {
                put(Mat::Eye, &Sphere::new(0.014).mesh().uv(8, 6), at(side * 0.045, 0.33, 0.18));
            }
        }
        _ => {}
    }
}

impl CarRig {
    /// Headlights, brake lights and underglow for day, night and braking.
    pub fn set_lights(&mut self, night: bool, braking: bool, materials: &mut Assets<StandardMaterial>, commands: &mut Commands) {
        self.lights = (night, braking);
        if self.wrecked {
            return;
        }
        let head_color = srgb(&self.config.headlight_color).to_linear();
        if let Some(mut m) = materials.get_mut(&self.mats.head) {
            m.emissive = head_color * if night { 2.2 } else { 0.6 } * 3.0;
        }
        if let Some(mut m) = materials.get_mut(&self.mats.tail) {
            m.emissive = srgb("#ff1a1a").to_linear()
                * if braking {
                    2.5
                } else if night {
                    1.0
                } else {
                    0.35
                }
                * 3.0;
        }
        if let Some(beam) = self.beam {
            let intensity = if night { 60.0 * crate::world_render::LUMENS } else { 0.0 };
            commands.entity(beam).queue(move |mut e: EntityWorldMut| {
                if let Some(mut l) = e.get_mut::<SpotLight>() {
                    l.intensity = intensity;
                }
            });
        }
        if let Some(light) = self.glow_light {
            let intensity = if self.config.underglow != "off" {
                (if night { 6.0 } else { 2.0 }) * crate::world_render::LUMENS
            } else {
                0.0
            };
            commands.entity(light).queue(move |mut e: EntityWorldMut| {
                if let Some(mut l) = e.get_mut::<PointLight>() {
                    l.intensity = intensity;
                }
            });
        }
    }

    /// Shows flickering nitro flames out of the exhausts. Call every frame.
    pub fn set_nitro(&self, on: bool, commands: &mut Commands) {
        for &flame in &self.flames {
            let stretch = 0.75 + crate::rng::random() as f32 * 0.5;
            commands.entity(flame).queue(move |mut e: EntityWorldMut| {
                if let Some(mut v) = e.get_mut::<Visibility>() {
                    *v = if on { Visibility::Inherited } else { Visibility::Hidden };
                }
                if let Some(mut t) = e.get_mut::<Transform>() {
                    t.scale = Vec3::new(1.2, 1.2, stretch);
                }
            });
        }
    }

    /// Turns the car into a burned-out wreck, or back again.
    pub fn set_wrecked(&mut self, on: bool, materials: &mut Assets<StandardMaterial>, commands: &mut Commands) {
        if on == self.wrecked {
            return;
        }
        self.wrecked = on;
        for (e, m) in &self.lit_parts {
            let material = if on { self.mats.burned.clone() } else { m.clone() };
            if let Ok(mut ec) = commands.get_entity(*e) {
                ec.insert(MeshMaterial3d(material));
            }
        }
        if let Some(d) = self.glow_decal
            && let Ok(mut ec) = commands.get_entity(d)
        {
            ec.insert(if on { Visibility::Hidden } else { Visibility::Inherited });
        }
        if on {
            for light in [self.beam, self.glow_light].into_iter().flatten() {
                commands.entity(light).queue(|mut e: EntityWorldMut| {
                    if let Some(mut l) = e.get_mut::<SpotLight>() {
                        l.intensity = 0.0;
                    }
                    if let Some(mut l) = e.get_mut::<PointLight>() {
                        l.intensity = 0.0;
                    }
                });
            }
        } else {
            let (n, b) = self.lights;
            self.set_lights(n, b, materials, commands);
        }
    }

    /// Rainbow paint and underglow, and the waving flag. `time` is in seconds.
    pub fn animate(&self, time: f64, materials: &mut Assets<StandardMaterial>, commands: &mut Commands) {
        if self.wrecked {
            return;
        }
        let hue = ((time * 0.08) % 1.0) as f32;
        if self.config.finish == "rainbow" {
            let c = crate::canvas::hsl(hue, 0.85, 0.5);
            if let Some(mut m) = materials.get_mut(&self.mats.paint) {
                m.base_color = Color::srgb(c.0, c.1, c.2);
            }
            if self.mats.body_paint != self.mats.paint {
                let c = crate::canvas::hsl(hue, 0.7, 0.75);
                if let Some(mut m) = materials.get_mut(&self.mats.body_paint) {
                    m.base_color = Color::srgb(c.0, c.1, c.2);
                }
            }
        }
        if self.config.underglow == "rainbow"
            && let Some(glow) = &self.mats.glow
        {
            let c = crate::canvas::hsl((hue + 0.5) % 1.0, 1.0, 0.55);
            let color = Color::srgb(c.0, c.1, c.2);
            if let Some(mut m) = materials.get_mut(glow) {
                m.base_color = color;
            }
            if let Some(light) = self.glow_light {
                commands.entity(light).queue(move |mut e: EntityWorldMut| {
                    if let Some(mut l) = e.get_mut::<PointLight>() {
                        l.color = color;
                    }
                });
            }
        }
        if let Some(flag) = self.flag {
            let turn = -PI / 2.0 + (time as f32 * 7.0).sin() * 0.3;
            commands.entity(flag).queue(move |mut e: EntityWorldMut| {
                if let Some(mut t) = e.get_mut::<Transform>() {
                    t.rotation = Quat::from_rotation_y(turn);
                }
            });
        }
    }

    /// Removes the car from the world.
    pub fn despawn(&self, commands: &mut Commands) {
        if let Ok(mut e) = commands.get_entity(self.root) {
            e.despawn();
        }
    }
}
