//! Builds each map: the city's streets and blocks, or a race track with its
//! grandstands, paddock, scenery, trees and weather. The random choices use
//! the same seeded sequence as the 1.2.x game, so every map looks the same.
use std::f64::consts::PI;

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::shape::{Cone as ConeShape, Cuboid, Cylinder, Rectangle, Sphere};

use crate::canvas::Rgba;
use crate::carmodel::{CarBuildCtx, build_car};
use crate::config::{CarConfig, random_config};
use crate::meshes::{MeshData, extrude, linear, linear_hsl, tapered_cylinder};
use crate::rng::Seeded;
use crate::textures;
use crate::track::{self, RaceTrack, TrackStep, TrackStyle, draw_track, make_wall};
use crate::venues::{self, OVAL_RADIUS, Scenery, Trees, Weather, track_layout};
use crate::world::{
    Block, BlockKind, BoxShape, CHUNK, CONE_HEIGHT, Checkpoint, Circle, Cone, Course, HALF_SIZE, Prop, RISE, ROAD_LINES, ROAD_WIDTH, Ramp,
    Spot, Target, TargetKind, WEATHER_BOX, WEATHER_DROPS, WEATHER_HEIGHT, World, city_course,
};

/// Materials every map shares, made once.
#[derive(Resource)]
pub struct WorldMats {
    pub buildings: Vec<(Handle<StandardMaterial>, u32)>,
    pub lamp_head: Handle<StandardMaterial>,
    pub vertex_colored: Handle<StandardMaterial>,
    pub vertex_colored_flat: Handle<StandardMaterial>,
    pub concrete: Handle<StandardMaterial>,
    pub ramp: Handle<StandardMaterial>,
    pub stripe: Handle<StandardMaterial>,
    pub cone: Handle<StandardMaterial>,
    pub cone_mesh: Handle<Mesh>,
    pub line: Handle<StandardMaterial>,
    pub metal: Handle<StandardMaterial>,
    pub road: Handle<Image>,
    pub concrete_tex: Handle<Image>,
}

/// A color that glows on its own, `k` times as bright as the color (not dimmed by the camera's exposure).
pub fn glow(color: Color, k: f32) -> LinearRgba {
    let c = color.to_linear() * k;
    LinearRgba::new(c.red, c.green, c.blue, 0.0)
}

pub fn hex_color(rgb: u32) -> Color {
    Color::srgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub fn css_color(text: &str) -> Color {
    let c = Rgba::css(text);
    Color::srgb(c.0, c.1, c.2)
}

impl WorldMats {
    pub fn new(materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>) -> Self {
        let day = images.add(textures::window_texture(false).into_image(true));
        let lit = images.add(textures::window_texture(true).into_image(true));
        let buildings = ["#c9c2b8", "#9fa8b3", "#b98b73", "#7d8a99", "#d9d4c7", "#8c7f73", "#a7b6a5"]
            .iter()
            .map(|c| {
                let color = css_color(c);
                let m = materials.add(StandardMaterial {
                    base_color: color,
                    base_color_texture: Some(day.clone()),
                    emissive_texture: Some(lit.clone()),
                    emissive: LinearRgba::NONE,
                    perceptual_roughness: 0.85,
                    ..default()
                });
                (m, crate::canvas::parse_hex(c).unwrap_or(0))
            })
            .collect();
        let concrete_tex = images.add(textures::concrete_texture().into_image(true));
        WorldMats {
            buildings,
            lamp_head: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                emissive: LinearRgba::NONE,
                ..default()
            }),
            vertex_colored: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 1.0,
                ..default()
            }),
            vertex_colored_flat: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.9,
                ..default()
            }),
            concrete: materials.add(StandardMaterial {
                base_color: hex_color(0xb9bcc0),
                perceptual_roughness: 0.9,
                ..default()
            }),
            ramp: materials.add(StandardMaterial {
                base_color: hex_color(0xe0a03a),
                perceptual_roughness: 0.7,
                ..default()
            }),
            stripe: materials.add(StandardMaterial {
                base_color: hex_color(0x222222),
                perceptual_roughness: 0.7,
                ..default()
            }),
            cone: materials.add(StandardMaterial {
                base_color: hex_color(0xff6a13),
                perceptual_roughness: 0.6,
                ..default()
            }),
            cone_mesh: meshes.add({
                let m = ConeShape {
                    radius: 0.28,
                    height: CONE_HEIGHT as f32,
                }
                .mesh()
                .resolution(14)
                .build();
                MeshData::from_mesh(&m, &Transform::from_xyz(0.0, CONE_HEIGHT as f32 / 2.0, 0.0), [1.0; 4]).build()
            }),
            line: materials.add(StandardMaterial {
                base_color: hex_color(0xf0f0f0),
                unlit: true,
                ..default()
            }),
            metal: materials.add(StandardMaterial {
                base_color: hex_color(0x2b2e33),
                metallic: 0.6,
                perceptual_roughness: 0.4,
                ..default()
            }),
            road: images.add(textures::road_texture().into_image(true)),
            concrete_tex,
        }
    }
}

/// What building a map needs.
pub struct Build<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub images: &'a mut Assets<Image>,
    pub mats: &'a WorldMats,
}

impl<'w, 's> Build<'_, 'w, 's> {
    fn car_ctx(&mut self) -> CarBuildCtx<'_, 'w, 's> {
        CarBuildCtx {
            commands: &mut *self.commands,
            meshes: &mut *self.meshes,
            materials: &mut *self.materials,
            images: &mut *self.images,
        }
    }
}

/// The shapes lamps and trees are drawn with on this map.
#[derive(Clone, Default)]
pub struct PropShapes {
    pub trunk: MeshData,
    pub trunk_color: [f32; 4],
    pub leaves: MeshData,
    pub pole: MeshData,
    pub head: MeshData,
}

impl World {
    fn spawn(&self, b: &mut Build, mesh: Mesh, material: Handle<StandardMaterial>, t: Transform) -> Entity {
        let root = self.root.expect("map root");
        b.commands
            .spawn((Mesh3d(b.meshes.add(mesh)), MeshMaterial3d(material), t, ChildOf(root)))
            .id()
    }

    fn color_mat(b: &mut Build, color: Color, roughness: f32) -> Handle<StandardMaterial> {
        b.materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: roughness,
            ..default()
        })
    }

    fn tex_mat(b: &mut Build, image: Handle<Image>, repeat: Vec2, roughness: f32) -> Handle<StandardMaterial> {
        b.materials.add(StandardMaterial {
            base_color_texture: Some(image),
            perceptual_roughness: roughness,
            uv_transform: bevy::math::Affine2::from_scale(repeat),
            ..default()
        })
    }

    /// Throws away the current map and builds another one.
    pub fn load(&mut self, map: &str, layout: u32, b: &mut Build) {
        self.clear(b);
        self.map = map.to_string();
        self.layout = if map == "city" { 0 } else { layout };
        self.rand = Seeded::new(42.0 + self.layout as f64);
        self.root = Some(
            b.commands
                .spawn((Transform::default(), Visibility::default(), Name::new("map")))
                .id(),
        );
        if map == "city" {
            self.build_city(b);
        } else if map == "circuit" && self.layout == 0 {
            self.build_circuit(b);
        } else if map == "oval" && self.layout == 0 {
            self.build_oval(b);
        } else {
            self.build_venue_track(b);
        }
        self.build_edges(b);
        self.build_weather();
        self.build_cells();
        self.loaded = true;
        self.generation += 1;
    }

    fn clear(&mut self, b: &mut Build) {
        if let Some(root) = self.root.take()
            && let Ok(mut e) = b.commands.get_entity(root)
        {
            e.despawn();
        }
        self.boxes.clear();
        self.circles.clear();
        self.walls.clear();
        self.targets.clear();
        self.props.clear();
        self.blocks.clear();
        self.ramps.clear();
        self.cones.clear();
        self.paved_areas.clear();
        self.hit_boxes.clear();
        self.weather.clear();
        self.track = None;
        self.changes.clear();
        self.destroyed.clear();
        self.start_lights = 0;
        self.effects.clear();
    }

    fn build_city(&mut self, b: &mut Build) {
        self.half = HALF_SIZE;
        self.spawn = Spot {
            x: -3.0,
            z: -20.0,
            heading: 0.0,
        };
        self.course = city_course();
        self.build_ground(b);
        self.build_roads(b);
        self.build_blocks(b);
    }

    fn build_ground(&mut self, b: &mut Build) {
        let size = self.half * 2.0 + 400.0;
        let ground = if self.map == "city" {
            venues::Ground::Grass
        } else {
            self.theme().ground
        };
        let tex = b.images.add(textures::ground_texture(ground).into_image(true));
        let mat = Self::tex_mat(b, tex, Vec2::splat((size / 6.5) as f32), 1.0);
        let mesh = bevy::shape::Plane3d::default().mesh().size(size as f32, size as f32).build();
        self.spawn(b, mesh, mat, Transform::default());
    }

    fn build_roads(&mut self, b: &mut Build) {
        let len = HALF_SIZE * 2.0 - 16.0;
        let mat = Self::tex_mat(b, b.mats.road.clone(), Vec2::new(1.0, (len / 12.0) as f32), 0.9);
        let mut roads = MeshData::default();
        for (i, &c) in ROAD_LINES.iter().enumerate() {
            let plane = Rectangle::new(ROAD_WIDTH as f32, len as f32).mesh().build();
            let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            // Along x (turned a quarter), and along z.
            roads.add_mesh(
                &plane,
                &Transform::from_xyz(0.0, 0.02 + i as f32 * 0.0005, c as f32)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * flat),
                [1.0; 4],
            );
            roads.add_mesh(
                &plane,
                &Transform::from_xyz(c as f32, 0.025 + i as f32 * 0.0005, 0.0).with_rotation(flat),
                [1.0; 4],
            );
        }
        roads.colors.clear();
        let e = self.spawn(b, roads.build(), mat, Transform::default());
        b.commands.entity(e).insert(NotShadowCaster);
        // Plain asphalt squares at the crossings, so dashed lines don't overlap.
        let mut crossings = MeshData::default();
        let square = bevy::shape::Plane3d::default()
            .mesh()
            .size(ROAD_WIDTH as f32, ROAD_WIDTH as f32)
            .build();
        for &x in &ROAD_LINES {
            for &z in &ROAD_LINES {
                crossings.add_mesh(&square, &Transform::from_xyz(x as f32, 0.035, z as f32), [1.0; 4]);
            }
        }
        crossings.colors.clear();
        let cross_mat = Self::color_mat(b, hex_color(0x34363b), 0.9);
        let e = self.spawn(b, crossings.build(), cross_mat, Transform::default());
        b.commands.entity(e).insert(NotShadowCaster);
        let mut poles = Vec::new();
        let along: Vec<f64> = ROAD_LINES[..ROAD_LINES.len() - 1]
            .iter()
            .flat_map(|c| [c + 20.0, c + 60.0])
            .collect();
        for &c in &ROAD_LINES {
            for &p in &along {
                poles.push(DVec3::new(c + ROAD_WIDTH / 2.0 + 1.0, 0.0, p));
                poles.push(DVec3::new(p, 0.0, c - ROAD_WIDTH / 2.0 - 1.0));
            }
        }
        self.add_lamps(&poles, 1.0);
    }

    /// Lamp posts at each point, `size` times as tall as a street lamp (7 m).
    fn add_lamps(&mut self, points: &[DVec3], size: f64) {
        for p in points {
            let circle = self.circles.len();
            self.circles.push(Circle {
                x: p.x,
                z: p.z,
                r: 0.25 * size,
                height: 7.3 * size,
                hit_radius: 0.3 * size,
                solid: true,
                target: Some(self.targets.len()),
            });
            let mut t = Target::new_prop(
                TargetKind::Lamp,
                p.x,
                p.z,
                25.0,
                (1500.0 * size).round(),
                circle,
                self.props.len(),
                size,
            );
            t.size = size;
            self.targets.push(t);
            self.props.push(Prop {
                lamp: true,
                transform: Transform::from_translation(p.as_vec3()).with_scale(Vec3::splat(size as f32)),
                color: [1.0; 4],
                visible: true,
                chunk: chunk_of(p.x, p.z),
            });
        }
    }

    fn build_blocks(&mut self, b: &mut Build) {
        let centers: Vec<f64> = ROAD_LINES[..ROAD_LINES.len() - 1].iter().map(|c| c + 40.0).collect();
        let mut trees = Vec::new();
        let inner = 80.0 - ROAD_WIDTH;
        let sidewalk = Self::tex_mat(b, b.mats.concrete_tex.clone(), Vec2::splat((inner / 4.0) as f32), 0.95);
        let mut slabs = MeshData::default();
        let mut grass = MeshData::default();
        for &bx in &centers {
            for &bz in &centers {
                let plane = bevy::shape::Plane3d::default().mesh().size(inner as f32, inner as f32).build();
                slabs.add_mesh(&plane, &Transform::from_xyz(bx as f32, 0.015, bz as f32), [1.0; 4]);
                let kind = if bx == 40.0 && bz == 40.0 {
                    BlockKind::Stunt
                } else if (bx == -40.0 && bz == 40.0) || (bx == -40.0 && bz == -40.0) {
                    BlockKind::Park
                } else if bx == 40.0 && bz == -40.0 {
                    BlockKind::Lot
                } else {
                    let r = self.rand.next();
                    if r < 0.62 {
                        BlockKind::City
                    } else if r < 0.85 {
                        BlockKind::Park
                    } else {
                        BlockKind::Lot
                    }
                };
                self.blocks.push(Block { x: bx, z: bz, kind });
                match kind {
                    BlockKind::Park => {
                        let lawn = bevy::shape::Plane3d::default()
                            .mesh()
                            .size((inner - 8.0) as f32, (inner - 8.0) as f32)
                            .build();
                        grass.add_mesh(&lawn, &Transform::from_xyz(bx as f32, 0.018, bz as f32), [1.0; 4]);
                        for _ in 0..26 {
                            let x = bx + (self.rand.next() - 0.5) * (inner - 14.0);
                            let z = bz + (self.rand.next() - 0.5) * (inner - 14.0);
                            trees.push(DVec3::new(x, 0.0, z));
                        }
                    }
                    BlockKind::City => {
                        for ox in [-1.0, 1.0] {
                            for oz in [-1.0, 1.0] {
                                let w = 14.0 + self.rand.next() * 14.0;
                                let d = 14.0 + self.rand.next() * 14.0;
                                let h = 8.0 + self.rand.next().powf(1.6) * 60.0;
                                self.add_building(b, bx + ox * 17.0, bz + oz * 17.0, w, d, h);
                            }
                        }
                    }
                    BlockKind::Lot => self.build_parking_lot(b, bx, bz),
                    BlockKind::Stunt => self.build_stunt_park(b, bx, bz),
                }
            }
        }
        slabs.colors.clear();
        let e = self.spawn(b, slabs.build(), sidewalk, Transform::default());
        b.commands.entity(e).insert(NotShadowCaster);
        if !grass.is_empty() {
            grass.colors.clear();
            let m = Self::color_mat(b, hex_color(0x5b8f43), 1.0);
            let e = self.spawn(b, grass.build(), m, Transform::default());
            b.commands.entity(e).insert(NotShadowCaster);
        }
        for _ in 0..140 {
            let side = (self.rand.next() * 4.0).floor() as usize;
            let along = (self.rand.next() - 0.5) * (HALF_SIZE * 2.0 - 10.0);
            let off = HALF_SIZE - 3.0 - self.rand.next() * 6.0;
            let p = [
                DVec3::new(along, 0.0, off),
                DVec3::new(along, 0.0, -off),
                DVec3::new(off, 0.0, along),
                DVec3::new(-off, 0.0, along),
            ][side.min(3)];
            trees.push(p);
        }
        self.add_trees(&trees);
    }

    pub(crate) fn add_building(&mut self, b: &mut Build, x: f64, z: f64, w: f64, d: f64, h: f64) {
        let mesh = building_box(w as f32, h as f32, d as f32);
        let pick = ((self.rand.next() * b.mats.buildings.len() as f64).floor() as usize).min(b.mats.buildings.len() - 1);
        let (mat, color) = b.mats.buildings[pick].clone();
        let entity = self.spawn(b, mesh, mat, Transform::from_xyz(x as f32, (h / 2.0) as f32, z as f32));
        let shape = self.boxes.len();
        let index = self.targets.len();
        self.boxes.push(BoxShape {
            min_x: x - w / 2.0,
            max_x: x + w / 2.0,
            min_z: z - d / 2.0,
            max_z: z + d / 2.0,
            height: h,
            solid: true,
            target: Some(index),
        });
        let hp = (120.0 + h * 6.0).round();
        let mut t = Target::new_prop(TargetKind::Building, x, z, hp, (40000.0 + h * 2500.0).round(), shape, 0, 1.0);
        t.entity = Some(entity);
        t.width = w;
        t.depth = d;
        t.height = h;
        t.color = color;
        self.targets.push(t);
    }

    /// Trees in the venue's style: round leafy trees, pines, snowy pines or cactuses.
    fn add_trees(&mut self, points: &[DVec3]) {
        let kind = if self.map == "city" { Trees::Round } else { self.theme().trees };
        for p in points {
            let s = 0.8 + self.rand.next() * 0.6;
            let turn = self.rand.next() * PI * 2.0;
            let color = match kind {
                Trees::Pine => {
                    let h = 0.3 + self.rand.next() * 0.05;
                    linear_hsl(h, 0.45, 0.18 + self.rand.next() * 0.08)
                }
                Trees::SnowPine => {
                    let sat = 0.2 + self.rand.next() * 0.15;
                    linear_hsl(0.55, sat, 0.82 + self.rand.next() * 0.1)
                }
                Trees::Cactus => {
                    let h = 0.27 + self.rand.next() * 0.04;
                    linear_hsl(h, 0.4, 0.3 + self.rand.next() * 0.06)
                }
                Trees::Round => {
                    let h = 0.25 + self.rand.next() * 0.08;
                    linear_hsl(h, 0.5, 0.28 + self.rand.next() * 0.12)
                }
            };
            let circle = self.circles.len();
            self.circles.push(Circle {
                x: p.x,
                z: p.z,
                r: 0.45 * s,
                height: 5.4 * s,
                hit_radius: 1.4 * s,
                solid: true,
                target: Some(self.targets.len()),
            });
            self.targets.push(Target::new_prop(
                TargetKind::Tree,
                p.x,
                p.z,
                30.0,
                800.0,
                circle,
                self.props.len(),
                s,
            ));
            self.props.push(Prop {
                lamp: false,
                transform: Transform::from_translation(p.as_vec3())
                    .with_rotation(Quat::from_rotation_y(turn as f32))
                    .with_scale(Vec3::splat(s as f32)),
                color,
                visible: true,
                chunk: chunk_of(p.x, p.z),
            });
        }
    }

    /// The shapes for this map's lamps and trees.
    pub fn prop_shapes(&self) -> PropShapes {
        let kind = if self.map == "city" { Trees::Round } else { self.theme().trees };
        let at = |x: f32, y: f32, z: f32| Transform::from_xyz(x, y, z);
        let mut trunk = MeshData::from_mesh(&tapered_cylinder(0.22, 0.3, 2.4, 7), &at(0.0, 1.2, 0.0), [1.0; 4]);
        let mut trunk_color = 0x6b4a2f;
        let mut leaves = MeshData::from_mesh(&Sphere::new(1.8).mesh().ico(0).unwrap(), &at(0.0, 3.6, 0.0), [1.0; 4]);
        match kind {
            Trees::Pine | Trees::SnowPine => {
                leaves = MeshData::from_mesh(
                    &ConeShape { radius: 1.9, height: 3.6 }.mesh().resolution(7).build(),
                    &at(0.0, 3.2, 0.0),
                    [1.0; 4],
                );
                leaves.add_mesh(
                    &ConeShape { radius: 1.3, height: 2.8 }.mesh().resolution(7).build(),
                    &at(0.0, 5.2, 0.0),
                    [1.0; 4],
                );
            }
            Trees::Cactus => {
                trunk = MeshData::from_mesh(&tapered_cylinder(0.32, 0.38, 4.2, 8), &at(0.0, 2.1, 0.0), [1.0; 4]);
                trunk_color = 0x4f7a3a;
                let side = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
                leaves = MeshData::from_mesh(
                    &Cylinder::new(0.22, 1.0).mesh().resolution(7).build(),
                    &at(0.75, 2.0, 0.0).with_rotation(side),
                    [1.0; 4],
                );
                leaves.add_mesh(&Cylinder::new(0.22, 1.4).mesh().resolution(7).build(), &at(1.2, 2.6, 0.0), [1.0; 4]);
                leaves.add_mesh(
                    &Cylinder::new(0.2, 0.8).mesh().resolution(7).build(),
                    &at(-0.6, 2.8, 0.0).with_rotation(side),
                    [1.0; 4],
                );
                leaves.add_mesh(
                    &Cylinder::new(0.2, 1.1).mesh().resolution(7).build(),
                    &at(-0.95, 3.3, 0.0),
                    [1.0; 4],
                );
            }
            Trees::Round => {}
        }
        PropShapes {
            trunk,
            trunk_color: linear(trunk_color),
            leaves: leaves.flat_shaded(),
            pole: MeshData::from_mesh(&tapered_cylinder(0.12, 0.16, 7.0, 8), &at(0.0, 3.5, 0.0), [1.0; 4]),
            head: MeshData::from_mesh(&Cuboid::new(0.7, 0.25, 0.7).mesh().build(), &at(0.0, 7.1, 0.0), [1.0; 4]),
        }
    }

    fn build_parking_lot(&mut self, b: &mut Build, bx: f64, bz: f64) {
        let asphalt = bevy::shape::Plane3d::default().mesh().size(60.0, 60.0).build();
        let m = Self::color_mat(b, hex_color(0x3a3c40), 0.95);
        let e = self.spawn(b, asphalt, m, Transform::from_xyz(bx as f32, 0.02, bz as f32));
        b.commands.entity(e).insert(NotShadowCaster);
        let mut lines = MeshData::default();
        for row in [-1.0, 1.0] {
            for i in 0..9 {
                let x = bx - 24.0 + i as f64 * 6.0;
                let z = bz + row * 18.0;
                let line = bevy::shape::Plane3d::default().mesh().size(0.15, 9.0).build();
                lines.add_mesh(&line, &Transform::from_xyz((x - 3.0) as f32, 0.025, z as f32), [1.0; 4]);
                if self.rand.next() < 0.55 {
                    self.add_parked_car(b, x, z, if row > 0.0 { PI } else { 0.0 });
                }
            }
        }
        lines.colors.clear();
        let e = self.spawn(b, lines.build(), b.mats.line.clone(), Transform::default());
        b.commands.entity(e).insert((NotShadowCaster, NotShadowReceiver));
    }

    /// A random parked car that can be rammed or blown up. Returns its target index.
    pub fn add_parked_car(&mut self, b: &mut Build, x: f64, z: f64, heading: f64) -> usize {
        let config = random_config(&CarConfig::default());
        let rig = build_car(&mut b.car_ctx(), &config, false, false);
        if let Some(root) = self.root {
            b.commands.entity(rig.root).insert(ChildOf(root));
        }
        let shape = self.boxes.len();
        let index = self.targets.len();
        self.boxes.push(BoxShape {
            min_x: x,
            max_x: x,
            min_z: z,
            max_z: z,
            height: 1.4,
            solid: true,
            target: Some(index),
        });
        let mut t = Target::new_prop(TargetKind::Car, x, z, 70.0, 12000.0, shape, 0, 1.0);
        t.rig = Some(rig);
        t.home = Spot { x, z, heading };
        t.pos = DVec3::new(x, 0.0, z);
        t.rot = DVec3::new(0.0, heading, 0.0);
        self.targets.push(t);
        self.fit_car_box(index);
        self.push_car_transform(index);
        index
    }

    /// Adds a car that drives around the city (see traffic.rs). Returns its target index.
    pub fn add_traffic_car(&mut self, b: &mut Build, x: f64, z: f64, heading: f64) -> usize {
        let i = self.add_parked_car(b, x, z, heading);
        self.targets[i].value = 15000.0;
        self.targets[i].traffic = true;
        i
    }

    fn build_stunt_park(&mut self, b: &mut Build, bx: f64, bz: f64) {
        let ramps = [
            Ramp {
                x: bx - 18.0,
                z: bz - 12.0,
                angle: 0.0,
                width: 6.0,
                length: 10.0,
                height: 1.6,
            },
            Ramp {
                x: bx + 14.0,
                z: bz - 6.0,
                angle: PI / 2.0,
                width: 7.0,
                length: 14.0,
                height: 2.6,
            },
            Ramp {
                x: bx - 4.0,
                z: bz + 18.0,
                angle: PI,
                width: 5.0,
                length: 8.0,
                height: 1.2,
            },
            Ramp {
                x: bx + 20.0,
                z: bz + 22.0,
                angle: -PI / 2.0,
                width: 6.0,
                length: 12.0,
                height: 2.0,
            },
        ];
        for r in ramps {
            self.add_ramp(b, r);
        }
        let place = |w: &mut World, b: &mut Build, x: f64, z: f64| {
            let root = w.root.expect("map root");
            let entity = b
                .commands
                .spawn((
                    Mesh3d(b.mats.cone_mesh.clone()),
                    MeshMaterial3d(b.mats.cone.clone()),
                    Transform::from_xyz(x as f32, 0.0, z as f32),
                    ChildOf(root),
                ))
                .id();
            let home = DVec3::new(x, 0.0, z);
            w.cones.push(Cone {
                entity,
                home,
                pos: home,
                rot: DVec3::ZERO,
                vel: DVec3::ZERO,
                spin: DVec3::ZERO,
                knocked: false,
                settled: false,
                knocked_at: 0.0,
            });
        };
        for i in 0..10 {
            place(self, b, bx - 28.0 + i as f64 * 5.5, bz + 4.0 + (i % 2) as f64 * 2.0);
        }
        for i in 0..4 {
            for j in 0..4 {
                place(self, b, bx - 22.0 + i as f64 * 1.6, bz + 26.0 + j as f64 * 1.6);
            }
        }
        for i in 0..4 {
            for j in 0..4 {
                place(self, b, bx + 2.0 + i as f64 * 1.6, bz - 26.0 + j as f64 * 1.6);
            }
        }
    }

    // ---------- the race tracks ----------

    fn add_track(&mut self, b: &mut Build, steps: &[TrackStep], style: TrackStyle) {
        let path = draw_track(steps);
        let (mut min_x, mut max_x, mut min_z, mut max_z) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &path {
            min_x = min_x.min(p.0);
            max_x = max_x.max(p.0);
            min_z = min_z.min(p.1);
            max_z = max_z.max(p.1);
        }
        let (cx, cz) = ((min_x + max_x) / 2.0, (min_z + max_z) / 2.0);
        let centered: Vec<(f64, f64)> = path.iter().map(|p| (p.0 - cx, p.1 - cz)).collect();
        let track = RaceTrack::new(&centered, style);
        let reach = track.points.iter().map(|p| p.x.abs().max(p.z.abs())).fold(0.0, f64::max);
        self.half = (reach + 110.0).ceil();
        self.build_ground(b);
        self.spawn_track_meshes(b, &track);
        self.walls.extend(track.walls());
        self.add_gantry(b, &track);
        let count = ((track.length / 320.0).round() as usize).max(6);
        let checkpoints = (1..=count)
            .map(|k| {
                let p = track.pose(track.length * k as f64 / count as f64, 0.0);
                Checkpoint {
                    x: p.x,
                    y: p.y,
                    z: p.z,
                    heading: Some(p.heading),
                    bank: Some(p.bank),
                }
            })
            .collect();
        let start = track.pose(-20.0, 0.0);
        let radius = track.style.width / 2.0 + 1.5;
        self.course = Course {
            checkpoints,
            start: Spot {
                x: start.x,
                z: start.z,
                heading: start.heading,
            },
            radius,
        };
        self.spawn = self.course.start;
        self.track = Some(std::sync::Arc::new(track));
    }

    fn spawn_track_meshes(&mut self, b: &mut Build, track: &RaceTrack) {
        let m = track.build_meshes();
        let road = b.images.add(track::track_texture().into_image(true));
        let road_mat = b.materials.add(StandardMaterial {
            base_color_texture: Some(road),
            perceptual_roughness: 0.85,
            ..default()
        });
        let receive_only = |b: &mut Build, w: &World, mesh: MeshData, mat: Handle<StandardMaterial>| {
            if !mesh.is_empty() {
                let e = w.spawn(b, mesh.build(), mat, Transform::default());
                b.commands.entity(e).insert(NotShadowCaster);
            }
        };
        receive_only(b, self, m.road, road_mat);
        let paved = b.materials.add(StandardMaterial {
            base_color: hex_color(0x55575c),
            perceptual_roughness: 0.9,
            ..default()
        });
        receive_only(b, self, m.paved, paved);
        let curb_tex = b.images.add(track::curb_texture().into_image(true));
        let curb = b.materials.add(StandardMaterial {
            base_color_texture: Some(curb_tex),
            perceptual_roughness: 0.7,
            ..default()
        });
        receive_only(b, self, m.curbs, curb);
        let checker = b.images.add(track::checker_texture().into_image(false));
        let line = b.materials.add(StandardMaterial {
            base_color_texture: Some(checker),
            perceptual_roughness: 0.8,
            ..default()
        });
        receive_only(b, self, m.line, line);
        receive_only(b, self, m.grid, b.mats.line.clone());
        let wall_tex = b.images.add(track::wall_texture(track.style.wall_colors).into_image(true));
        let wall = b.materials.add(StandardMaterial {
            base_color_texture: Some(wall_tex),
            perceptual_roughness: 0.8,
            ..default()
        });
        if !m.walls.is_empty() {
            self.spawn(b, m.walls.build(), wall, Transform::default());
        }
        if !m.fence.is_empty() {
            let fence_tex = b.images.add(track::fence_texture().into_image(true));
            let fence = b.materials.add(StandardMaterial {
                base_color_texture: Some(fence_tex),
                unlit: true,
                alpha_mode: AlphaMode::Mask(0.3),
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let e = self.spawn(b, m.fence.build(), fence, Transform::default());
            b.commands.entity(e).insert(NotShadowCaster);
            let mut posts = MeshData::default();
            let post = Cylinder::new(0.08, track.style.fence as f32).mesh().resolution(6).build();
            for p in &m.posts {
                posts.add_mesh(
                    &post,
                    &Transform::from_xyz(p[0] as f32, p[1] as f32 + track.style.fence as f32 / 2.0, p[2] as f32),
                    [1.0; 4],
                );
            }
            posts.colors.clear();
            let pm = b.materials.add(StandardMaterial {
                base_color: hex_color(0x8a8f96),
                metallic: 0.6,
                perceptual_roughness: 0.4,
                ..default()
            });
            self.spawn(b, posts.build(), pm, Transform::default());
        }
    }

    fn build_circuit(&mut self, b: &mut Build) {
        let v = self.venue();
        self.add_track(b, &track_layout(v, 0), v.style.clone().expect("style"));
        let (start, wall, length) = {
            let t = self.track.as_ref().unwrap();
            (t.pose(0.0, 0.0), t.left_wall.unwrap_or(9.0), t.length)
        };
        self.add_track_grandstand(b, -105.0, 160.0, 14, "#d7263d");
        self.add_track_grandstand(b, 115.0, 170.0, 14, "#2b6fd6");
        self.add_track_grandstand(b, 760.0, 100.0, 10, "#f2c94c");
        self.add_track_grandstand(b, 1541.0, 110.0, 10, "#39a845");
        let pz = start.z - wall - 8.0;
        self.add_paddock(b, start.x - 175.0, start.x + 190.0, pz - 110.0, pz);
        self.add_building(b, start.x + 10.0, pz - 9.0, 190.0, 14.0, 8.0);
        self.add_building(b, start.x - 120.0, pz - 12.0, 16.0, 18.0, 30.0);
        for i in 0..5 {
            let h = 10.0 + self.rand.next() * 8.0;
            self.add_building(b, start.x - 100.0 + i as f64 * 52.0, pz - 62.0, 30.0, 18.0, h);
        }
        for i in 0..22 {
            self.add_parked_car(b, start.x - 150.0 + i as f64 * 14.0, pz - 34.0, PI / 2.0);
        }
        for i in 0..22 {
            if self.rand.next() < 0.7 {
                self.add_parked_car(b, start.x - 150.0 + i as f64 * 14.0, pz - 90.0, -PI / 2.0);
            }
        }
        let mut lights = Vec::new();
        let mut along = 60.0;
        while along < length {
            let p = self.track.as_ref().unwrap().pose(along, -(wall + 5.0));
            if self.is_clear(p.x, p.z, 3.0) {
                lights.push(DVec3::new(p.x, 0.0, p.z));
            }
            along += 140.0;
        }
        self.add_lamps(&lights, 2.2);
        self.scatter_trees(420, 4.0);
    }

    fn build_oval(&mut self, b: &mut Build) {
        let v = self.venue();
        let style = v.style.clone().expect("style");
        self.add_track(b, &track_layout(v, 0), style.clone());
        let (start, outside, length) = {
            let t = self.track.as_ref().unwrap();
            (t.pose(0.0, 0.0), t.right_wall.unwrap_or(13.0) + 0.6, t.length)
        };
        for (along, color) in [(-215.0, "#2b6fd6"), (0.0, "#d7263d"), (215.0, "#2b6fd6")] {
            self.add_track_grandstand(b, along, 200.0, 22, color);
        }
        let infield = start.z - OVAL_RADIUS;
        let pz = start.z - style.width / 2.0 - style.apron - 14.0;
        self.add_paddock(b, start.x - 260.0, start.x + 260.0, pz - 90.0, pz);
        self.add_building(b, start.x, pz - 9.0, 300.0, 14.0, 9.0);
        self.add_building(b, start.x - 200.0, pz - 14.0, 18.0, 18.0, 34.0);
        for i in 0..30 {
            self.add_parked_car(b, start.x - 230.0 + i as f64 * 16.0, pz - 34.0, PI / 2.0);
        }
        for i in 0..6 {
            let h = 8.0 + self.rand.next() * 10.0;
            self.add_building(b, start.x - 200.0 + i as f64 * 80.0, pz - 70.0, 40.0, 18.0, h);
        }
        for (dx, dz, angle, height) in [
            (-250.0, 10.0, PI / 2.0, 3.0),
            (0.0, 10.0, PI / 2.0, 5.0),
            (250.0, 10.0, PI / 2.0, 3.0),
            (-120.0, 70.0, -PI / 2.0, 4.0),
            (120.0, 70.0, -PI / 2.0, 4.0),
        ] {
            self.add_ramp(
                b,
                Ramp {
                    x: start.x + dx,
                    z: infield + dz,
                    angle,
                    width: 10.0,
                    length: height * 6.0,
                    height,
                },
            );
        }
        let mut lights = Vec::new();
        let mut along = 0.0;
        while along < length {
            let p = self.track.as_ref().unwrap().pose(along, -(outside + 6.0));
            if self.is_clear(p.x, p.z, 3.0) {
                lights.push(DVec3::new(p.x, 0.0, p.z));
            }
            along += 150.0;
        }
        self.add_lamps(&lights, 3.0);
        self.scatter_trees(380, 4.0);
    }

    fn build_venue_track(&mut self, b: &mut Build) {
        let v = self.venue();
        let style = v.style.clone().expect("style");
        self.add_track(b, &track_layout(v, self.layout), style.clone());
        let scenery = self.theme().scenery;
        let (ahead, behind, inside, wall, left_wall, right_wall, length) = {
            let t = self.track.as_ref().unwrap();
            let straight = |direction: f64| {
                let mut d = 0.0;
                while d < 400.0 && t.point_at(direction * d).curve.abs() < 1.0 / 400.0 {
                    d += 4.0;
                }
                (d - 15.0).max(0.0)
            };
            let wall = t
                .left_wall
                .unwrap_or(style.width / 2.0)
                .max(t.right_wall.unwrap_or(style.width / 2.0));
            (straight(1.0), straight(-1.0), t.inside, wall, t.left_wall, t.right_wall, t.length)
        };
        let _ = (left_wall, right_wall);
        let out = -inside;
        let colors = ["#d7263d", "#2b6fd6", "#f2c94c", "#39a845"];
        if scenery != Scenery::Woods {
            let room = |from: f64, to: f64| to - from > 50.0;
            if room(-behind, -12.0) {
                self.add_track_grandstand(
                    b,
                    (-behind - 12.0) / 2.0,
                    160f64.min(behind - 16.0),
                    12,
                    colors[self.layout as usize % 4],
                );
            }
            if room(12.0, ahead) {
                self.add_track_grandstand(
                    b,
                    (ahead + 12.0) / 2.0,
                    160f64.min(ahead - 16.0),
                    12,
                    colors[(self.layout as usize + 1) % 4],
                );
            }
            let pit_length = 180f64.min(ahead + behind - 20.0);
            let pit_along = (ahead - behind) / 2.0;
            let pit = self.track.as_ref().unwrap().pose(pit_along, inside * (wall + 9.0));
            if pit_length > 60.0 && self.is_area_clear(pit.x - pit_length / 2.0, pit.x + pit_length / 2.0, pit.z - 6.0, pit.z + 6.0, 1.0) {
                self.add_building(b, pit.x, pit.z, pit_length, 12.0, 8.0);
                let mut i = 0.0;
                while i < pit_length / 14.0 {
                    let car = self
                        .track
                        .as_ref()
                        .unwrap()
                        .pose(pit_along - pit_length / 2.0 + 7.0 + i * 14.0, inside * (wall + 24.0));
                    if self.is_clear(car.x, car.z, 2.5) {
                        self.add_parked_car(b, car.x, car.z, car.heading + PI / 2.0);
                    }
                    i += 1.0;
                }
            }
        }
        if scenery == Scenery::Mesas {
            self.add_rocks(b, 26);
        }
        if scenery == Scenery::City {
            self.add_city_blocks(b, 110);
        }
        let light_gap = if scenery == Scenery::City { 45.0 } else { 130.0 };
        let mut lights = Vec::new();
        let mut along = 30.0;
        while along < length {
            let p = self.track.as_ref().unwrap().pose(along, out * (wall + 4.0));
            if self.is_clear(p.x, p.z, 2.0) {
                lights.push(DVec3::new(p.x, 0.0, p.z));
            }
            along += light_gap;
        }
        self.add_lamps(
            &lights,
            if scenery == Scenery::City {
                1.3
            } else if length < 1500.0 {
                1.2
            } else {
                2.2
            },
        );
        let area = (self.half * 2.0).powi(2);
        let count = if scenery == Scenery::Woods {
            (area / 260.0).round()
        } else {
            (area / if scenery == Scenery::Mesas { 3000.0 } else { 1300.0 }).round()
        };
        self.scatter_trees(count as usize, if scenery == Scenery::Woods { 3.0 } else { 4.0 });
    }

    fn add_rocks(&mut self, b: &mut Build, count: usize) {
        let mut rocks: Vec<(f64, f64, f64, f64)> = Vec::new();
        let mut tries = 0;
        while rocks.len() < count && tries < count * 40 {
            tries += 1;
            let r = 6.0 + self.rand.next() * 16.0;
            let x = (self.rand.next() * 2.0 - 1.0) * (self.half - r - 4.0);
            let z = (self.rand.next() * 2.0 - 1.0) * (self.half - r - 4.0);
            if !self.is_clear(x, z, r + 8.0) || rocks.iter().any(|o| (o.0 - x).hypot(o.1 - z) < o.2 + r + 6.0) {
                continue;
            }
            rocks.push((x, z, r, 8.0 + self.rand.next() * 34.0));
        }
        let mut mesh = MeshData::default();
        let shape = tapered_cylinder(1.0, 1.25, 1.0, 7);
        for &(x, z, r, h) in &rocks {
            let turn = self.rand.next() * 6.0;
            let hue = 0.04 + self.rand.next() * 0.03;
            let color = linear_hsl(hue, 0.55, 0.38 + self.rand.next() * 0.12);
            let t = Transform::from_xyz(x as f32, (h / 2.0) as f32, z as f32)
                .with_rotation(Quat::from_rotation_y(turn as f32))
                .with_scale(Vec3::new(r as f32, h as f32, r as f32));
            mesh.add_mesh(&shape, &t, color);
            self.circles.push(Circle {
                x,
                z,
                r: r * 1.2,
                height: h,
                hit_radius: r * 1.2,
                solid: true,
                target: None,
            });
        }
        if !mesh.is_empty() {
            self.spawn(b, mesh.flat_shaded().build(), b.mats.vertex_colored.clone(), Transform::default());
        }
    }

    fn add_city_blocks(&mut self, b: &mut Build, count: usize) {
        let mut placed = 0;
        let mut tries = 0;
        while placed < count && tries < count * 30 {
            tries += 1;
            let w = 14.0 + self.rand.next() * 16.0;
            let d = 14.0 + self.rand.next() * 16.0;
            let x = (self.rand.next() * 2.0 - 1.0) * (self.half - 20.0);
            let z = (self.rand.next() * 2.0 - 1.0) * (self.half - 20.0);
            if !self.is_clear(x, z, w.hypot(d) / 2.0 + 5.0) {
                continue;
            }
            let h = 12.0 + self.rand.next().powf(1.5) * 70.0;
            self.add_building(b, x, z, w, d, h);
            placed += 1;
        }
    }

    /// A concrete area the car can drive on at full speed.
    fn add_paddock(&mut self, b: &mut Build, min_x: f64, max_x: f64, min_z: f64, max_z: f64) {
        let mat = Self::tex_mat(
            b,
            b.mats.concrete_tex.clone(),
            Vec2::new(((max_x - min_x) / 4.0) as f32, ((max_z - min_z) / 4.0) as f32),
            0.95,
        );
        let slab = bevy::shape::Plane3d::default()
            .mesh()
            .size((max_x - min_x) as f32, (max_z - min_z) as f32)
            .build();
        let e = self.spawn(
            b,
            slab,
            mat,
            Transform::from_xyz(((min_x + max_x) / 2.0) as f32, 0.015, ((min_z + max_z) / 2.0) as f32),
        );
        b.commands.entity(e).insert(NotShadowCaster);
        self.paved_areas.push([min_x, max_x, min_z, max_z]);
    }

    /// Whether a spot is clear of the track, barriers, buildings and grandstands, by `margin` meters.
    pub fn is_clear(&self, x: f64, z: f64, margin: f64) -> bool {
        if x.abs() > self.half - margin || z.abs() > self.half - margin {
            return false;
        }
        if let Some(t) = &self.track {
            let wall = t
                .left_wall
                .unwrap_or(0.0)
                .max(t.right_wall.unwrap_or(0.0))
                .max(t.style.width / 2.0 + t.style.apron);
            if t.distance_from(x, z) < wall + margin {
                return false;
            }
        }
        if self
            .boxes
            .iter()
            .any(|b| x > b.min_x - margin && x < b.max_x + margin && z > b.min_z - margin && z < b.max_z + margin)
        {
            return false;
        }
        if self
            .paved_areas
            .iter()
            .any(|a| x > a[0] - margin && x < a[1] + margin && z > a[2] - margin && z < a[3] + margin)
        {
            return false;
        }
        if self
            .ramps
            .iter()
            .any(|r| (x - r.x).hypot(z - r.z) < r.length / 2.0 + r.width + margin)
        {
            return false;
        }
        !self.walls.iter().any(|w| {
            let sx = w.bx - w.ax;
            let sz = w.bz - w.az;
            let len2 = sx * sx + sz * sz;
            let t = (((x - w.ax) * sx + (z - w.az) * sz) / if len2 == 0.0 { 1.0 } else { len2 }).clamp(0.0, 1.0);
            (x - (w.ax + sx * t)).hypot(z - (w.az + sz * t)) < margin
        })
    }

    fn is_area_clear(&self, min_x: f64, max_x: f64, min_z: f64, max_z: f64, margin: f64) -> bool {
        let step_x = ((max_x - min_x) / ((max_x - min_x) / 8.0).ceil()).max(1.0);
        let step_z = ((max_z - min_z) / ((max_z - min_z) / 8.0).ceil()).max(1.0);
        let mut x = min_x;
        while x <= max_x + 0.1 {
            let mut z = min_z;
            while z <= max_z + 0.1 {
                if !self.is_clear(x, z, margin) {
                    return false;
                }
                z += step_z;
            }
            x += step_x;
        }
        true
    }

    fn scatter_trees(&mut self, count: usize, margin: f64) {
        let mut trees = Vec::new();
        let mut tries = 0;
        while trees.len() < count && tries < count * 20 {
            tries += 1;
            let x = (self.rand.next() * 2.0 - 1.0) * (self.half - 4.0);
            let z = (self.rand.next() * 2.0 - 1.0) * (self.half - 4.0);
            if self.is_clear(x, z, margin) {
                trees.push(DVec3::new(x, 0.0, z));
            }
        }
        self.add_trees(&trees);
    }

    fn add_track_grandstand(&mut self, b: &mut Build, along: f64, length: f64, rows: usize, roof: &str) {
        let (x, z, facing) = {
            let t = self.track.as_ref().unwrap();
            let out = -t.inside;
            let wall = if out > 0.0 { t.left_wall } else { t.right_wall }.unwrap_or(t.style.width / 2.0);
            let p = t.pose(along, out * (wall + 3.5));
            (p.x, p.z, (out * p.heading.cos()).atan2(-out * p.heading.sin()))
        };
        self.add_grandstand(b, x, z, facing, length, rows, roof);
    }

    /// A grandstand full of fans. (x, z) is the middle of its front edge, and
    /// `facing` the direction from the front to the back (0 = +z).
    #[allow(clippy::too_many_arguments)]
    fn add_grandstand(&mut self, b: &mut Build, x: f64, z: f64, facing: f64, length: f64, rows: usize, roof_color: &str) {
        let place = Transform::from_xyz(x as f32, 0.0, z as f32).with_rotation(Quat::from_rotation_y(facing as f32));
        let (step, rise) = (1.0f32, 0.55f32);
        let depth = rows as f32 * step;
        let top = rows as f32 * rise;
        let len = length as f32;
        // The stepped seating, drawn side-on: x is depth back, y is up.
        let mut shape = vec![(0.0, 0.0)];
        for r in 0..rows {
            shape.push((r as f32 * step, (r + 1) as f32 * rise));
            shape.push(((r + 1) as f32 * step, (r + 1) as f32 * rise));
        }
        shape.push((depth, 0.0));
        let seats = extrude(&shape, len);
        // Extruded along +z; turn so depth runs along +z and the length along x (as in the old game).
        let mut concrete = MeshData::default();
        let turn =
            Transform::from_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)) * Transform::from_xyz(0.0, 0.0, -len / 2.0);
        for (i, p) in seats.positions.iter().enumerate() {
            concrete.positions.push(turn.transform_point(Vec3::from(*p)).to_array());
            concrete
                .normals
                .push(turn.rotation.mul_vec3(Vec3::from(seats.normals[i])).to_array());
            concrete.uvs.push(seats.uvs[i]);
        }
        concrete.indices = seats.indices.clone();
        concrete.add_mesh(
            &Cuboid::new(len, top + 5.0, 0.4).mesh().build(),
            &Transform::from_xyz(0.0, (top + 5.0) / 2.0, depth + 0.2),
            [1.0; 4],
        );
        let mut px = -len / 2.0;
        while px <= len / 2.0 {
            concrete.add_mesh(
                &Cuboid::new(0.3, 5.0, 0.3).mesh().build(),
                &Transform::from_xyz(px, top + 2.5, depth * 0.35),
                [1.0; 4],
            );
            px += 20.0;
        }
        concrete.colors.clear();
        let stand_concrete = b.mats.concrete.clone();
        self.spawn(b, concrete.build(), stand_concrete, place);
        let roof_mat = Self::color_mat(b, css_color(roof_color), 0.6);
        let roof = Cuboid::new(len + 2.0, 0.4, depth * 0.75).mesh().build();
        self.spawn(
            b,
            roof,
            roof_mat,
            place * Transform::from_xyz(0.0, top + 5.0, depth - depth * 0.75 / 2.0 + 0.4),
        );
        // The crowd: a body and a head for each fan, in bright shirts, with gaps.
        let mut fans = Vec::new();
        for r in 0..rows {
            let mut px = -len / 2.0 + 0.5;
            while px < len / 2.0 {
                if self.rand.next() < 0.15 {
                    px += 0.75;
                    continue;
                }
                let jitter = (self.rand.next() - 0.5) * 0.2;
                fans.push(Vec3::new(px + jitter as f32, (r + 1) as f32 * rise, r as f32 * step + 0.45));
                px += 0.75;
            }
        }
        let body = Cuboid::new(0.46, 0.7, 0.3).mesh().build();
        let head = Sphere::new(0.14).mesh().uv(6, 4);
        let mut crowd = MeshData::default();
        for f in &fans {
            let hue = self.rand.next();
            let shirt = linear_hsl(hue, 0.75, 0.35 + self.rand.next() * 0.3);
            let skin = linear_hsl(0.07, 0.45, 0.25 + self.rand.next() * 0.5);
            crowd.add_mesh(&body, &Transform::from_translation(*f + Vec3::Y * 0.35), shirt);
            crowd.add_mesh(&head, &Transform::from_translation(*f + Vec3::Y * 0.86), skin);
        }
        if !crowd.is_empty() {
            let e = self.spawn(b, crowd.build(), b.mats.vertex_colored.clone(), place);
            b.commands.entity(e).insert(NotShadowCaster);
        }
        let corner = |lx: f64, lz: f64| {
            let (s, c) = facing.sin_cos();
            (x + lx * c + lz * s, z - lx * s + lz * c)
        };
        let corners = [
            corner(-length / 2.0, 0.0),
            corner(length / 2.0, 0.0),
            corner(length / 2.0, depth as f64 + 0.4),
            corner(-length / 2.0, depth as f64 + 0.4),
        ];
        for i in 0..4 {
            let (a, c) = (corners[i], corners[(i + 1) % 4]);
            self.walls.push(make_wall(a.0, a.1, c.0, c.1, top as f64 + 5.0));
        }
    }

    /// The gantry over the start line, with its banner and the five start lights.
    fn add_gantry(&mut self, b: &mut Build, track: &RaceTrack) {
        let p = track.pose(0.0, 0.0);
        let place = Transform::from_xyz(p.x as f32, 0.0, p.z as f32).with_rotation(Quat::from_rotation_y(p.heading as f32));
        let s = &track.style;
        let left = track.left_wall.unwrap_or(s.width / 2.0 + s.apron) + 1.5;
        let right = -(track.right_wall.unwrap_or(s.width / 2.0 + s.apron) + 1.5);
        let height = 9.0 + track.pose(0.0, left).y.max(track.pose(0.0, right).y);
        let mut metal = MeshData::default();
        for x in [left, right] {
            metal.add_mesh(
                &Cuboid::new(1.0, height as f32, 1.0).mesh().build(),
                &Transform::from_xyz(x as f32, (height / 2.0) as f32, 0.0),
                [1.0; 4],
            );
            let at = track.pose(0.0, x);
            self.circles.push(Circle {
                x: at.x,
                z: at.z,
                r: 0.7,
                height,
                hit_radius: 0.7,
                solid: true,
                target: None,
            });
        }
        let span = left - right;
        let mid = ((left + right) / 2.0) as f32;
        metal.add_mesh(
            &Cuboid::new(span as f32 + 1.0, 2.4, 1.2).mesh().build(),
            &Transform::from_xyz(mid, height as f32 - 1.2, 0.0),
            [1.0; 4],
        );
        for i in 0..5 {
            metal.add_mesh(
                &Cuboid::new(0.9, 1.6, 0.5).mesh().build(),
                &Transform::from_xyz((i as f32 - 2.0) * 1.4, height as f32 - 3.3, -0.3),
                [1.0; 4],
            );
        }
        metal.colors.clear();
        self.spawn(b, metal.build(), b.mats.metal.clone(), place);
        let banner_tex = b.images.add(textures::banner_texture(self.theme().banner).into_image(false));
        let banner = b.materials.add(StandardMaterial {
            base_color_texture: Some(banner_tex),
            perceptual_roughness: 0.6,
            ..default()
        });
        for side in [-1.0f32, 1.0] {
            let sign = Rectangle::new(span.min(40.0) as f32, 2.0).mesh().build();
            let t = place
                * Transform::from_xyz(mid, height as f32 - 1.2, side * 0.61).with_rotation(Quat::from_rotation_y(if side < 0.0 {
                    std::f32::consts::PI
                } else {
                    0.0
                }));
            let e = self.spawn(b, sign, banner.clone(), t);
            b.commands.entity(e).insert(NotShadowCaster);
        }
        // Five start lights, facing the grid; lit by world_render from `start_lights`.
        for i in 0..5 {
            let light = b.materials.add(StandardMaterial {
                base_color: hex_color(0x300000),
                emissive: LinearRgba::NONE,
                ..default()
            });
            for y in [0.35f32, -0.35] {
                let bulb = bevy::shape::Circle::new(0.3).mesh().resolution(16).build();
                let t = place
                    * Transform::from_xyz((i as f32 - 2.0) * 1.4, height as f32 - 3.3 + y, -0.3 - 0.26)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
                let e = self.spawn(b, bulb, light.clone(), t);
                b.commands.entity(e).insert((NotShadowCaster, StartLight(i)));
            }
        }
    }

    /// A jump ramp: a wedge you drive up, with a steep drop off the back.
    fn add_ramp(&mut self, b: &mut Build, ramp: Ramp) {
        let l = ramp.length as f32;
        let shape = [(-l / 2.0, 0.0), (-l / 2.0 + l * RISE as f32, ramp.height as f32), (l / 2.0, 0.0)];
        let wedge = extrude(&shape, ramp.width as f32);
        // Profile along +z, centered across.
        let turn = Transform::from_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
            * Transform::from_xyz(0.0, 0.0, -ramp.width as f32 / 2.0);
        let mut m = MeshData::default();
        for (i, p) in wedge.positions.iter().enumerate() {
            m.positions.push(turn.transform_point(Vec3::from(*p)).to_array());
            m.normals.push(turn.rotation.mul_vec3(Vec3::from(wedge.normals[i])).to_array());
            m.uvs.push(wedge.uvs[i]);
        }
        m.indices = wedge.indices;
        let place = Transform::from_xyz(ramp.x as f32, 0.0, ramp.z as f32).with_rotation(Quat::from_rotation_y(ramp.angle as f32));
        self.spawn(b, m.build(), b.mats.ramp.clone(), place);
        let mut stripes = MeshData::default();
        for side in [-1.0f32, 1.0] {
            stripes.add_mesh(
                &Cuboid::new(0.3, 0.05, 0.3).mesh().build(),
                &Transform::from_xyz(
                    side * (ramp.width as f32 / 2.0 - 0.3),
                    ramp.height as f32 + 0.02,
                    -l / 2.0 + l * RISE as f32,
                ),
                [1.0; 4],
            );
        }
        stripes.colors.clear();
        self.spawn(b, stripes.build(), b.mats.stripe.clone(), place);
        self.ramps.push(ramp);
    }

    fn build_edges(&mut self, b: &mut Build) {
        let half = self.half;
        let len = half * 2.0 + 2.0;
        let mut edge = MeshData::default();
        for (x, z, w, d) in [
            (0.0, half + 1.0, len, 2.0),
            (0.0, -half - 1.0, len, 2.0),
            (half + 1.0, 0.0, 2.0, len),
            (-half - 1.0, 0.0, 2.0, len),
        ] {
            edge.add_mesh(
                &Cuboid::new(w as f32, 1.4, d as f32).mesh().build(),
                &Transform::from_xyz(x as f32, 0.7, z as f32),
                [1.0; 4],
            );
            self.boxes.push(BoxShape {
                min_x: x - w / 2.0,
                max_x: x + w / 2.0,
                min_z: z - d / 2.0,
                max_z: z + d / 2.0,
                height: 1.4,
                solid: true,
                target: None,
            });
        }
        edge.colors.clear();
        let m = Self::color_mat(b, hex_color(0xb8bcc2), 0.9);
        let e = self.spawn(b, edge.build(), m, Transform::default());
        b.commands.entity(e).insert(NotShadowCaster);
        // Hills on the horizon.
        let scenery = if self.map == "city" { Scenery::City } else { self.theme().scenery };
        let mesas = scenery == Scenery::Mesas;
        let peaks = scenery == Scenery::Mountains;
        let shape = if mesas {
            tapered_cylinder(0.8, 1.0, 1.0, 7)
        } else {
            ConeShape { radius: 1.0, height: 1.0 }.mesh().resolution(7).build()
        };
        let hill_color = if peaks {
            0x6f7a86
        } else if self.map == "city" {
            0x55734a
        } else {
            self.theme().hills
        };
        let mut hills = MeshData::default();
        let mut caps = MeshData::default();
        for i in 0..70 {
            let a = i as f64 / 70.0 * PI * 2.0;
            let dist = half + 80.0 + self.rand.next() * 80.0;
            let r = 50.0 + self.rand.next() * 60.0;
            let base_h = if peaks {
                110.0 + self.rand.next() * 150.0
            } else if mesas {
                25.0 + self.rand.next() * 45.0
            } else {
                30.0 + self.rand.next() * 60.0
            };
            let h = base_h * if scenery == Scenery::City && self.map != "city" { 0.6 } else { 1.0 };
            let q = Quat::from_rotation_y((self.rand.next() * PI) as f32);
            let t = Transform::from_xyz((a.cos() * dist) as f32, (h / 2.0 - 2.0) as f32, (a.sin() * dist) as f32)
                .with_rotation(q)
                .with_scale(Vec3::new(r as f32, h as f32, r as f32));
            hills.add_mesh(&shape, &t, linear(hill_color));
            if peaks {
                let ct = Transform::from_xyz(
                    (a.cos() * dist) as f32,
                    (h - 2.0 - h * 0.35 / 2.0 + 0.3) as f32,
                    (a.sin() * dist) as f32,
                )
                .with_rotation(q)
                .with_scale(Vec3::new((r * 0.36) as f32, (h * 0.35) as f32, (r * 0.36) as f32));
                caps.add_mesh(&shape, &ct, [1.0; 4]);
            }
        }
        let e = self.spawn(b, hills.flat_shaded().build(), b.mats.vertex_colored.clone(), Transform::default());
        b.commands.entity(e).insert(NotShadowCaster);
        if !caps.is_empty() {
            let e = self.spawn(
                b,
                caps.flat_shaded().build(),
                b.mats.vertex_colored_flat.clone(),
                Transform::default(),
            );
            b.commands.entity(e).insert(NotShadowCaster);
        }
    }

    /// Rain or snow, if the venue has weather.
    fn build_weather(&mut self) {
        let kind = if self.map == "city" { Weather::Clear } else { self.theme().weather };
        if kind == Weather::Clear {
            return;
        }
        let rain = kind == Weather::Rain;
        for _ in 0..WEATHER_DROPS {
            let x = ((self.rand.next() - 0.5) * WEATHER_BOX) as f32;
            let y = (self.rand.next() * WEATHER_HEIGHT) as f32;
            let z = ((self.rand.next() - 0.5) * WEATHER_BOX) as f32;
            for k in 0..if rain { 2 } else { 1 } {
                self.weather.push([x, y + k as f32 * 0.7, z]);
            }
        }
        self.weather_speed = if rain { 22.0 } else { 2.2 };
    }
}

/// Marks one of the five start lights (0-4) on the gantry.
#[derive(Component)]
pub struct StartLight(pub usize);

/// Which group of props a spot belongs to.
pub fn chunk_of(x: f64, z: f64) -> (i32, i32) {
    ((x / CHUNK).floor() as i32, (z / CHUNK).floor() as i32)
}

/// A building's box, with windows the same size on every building and plain roofs.
fn building_box(w: f32, h: f32, d: f32) -> Mesh {
    let mut m = MeshData::default();
    let (hw, hh, hd) = (w / 2.0, h / 2.0, d / 2.0);
    // (normal, right, up, face width, face height)
    let faces = [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y, d, h),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y, d, h),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z, 0.0, 0.0),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z, 0.0, 0.0),
        (Vec3::Z, Vec3::X, Vec3::Y, w, h),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y, w, h),
    ];
    for (n, right, up, fw, fh) in faces {
        let center = n * Vec3::new(hw, hh, hd);
        let half_r = (right * Vec3::new(hw, hh, hd)).abs().max_element();
        let half_u = (up * Vec3::new(hw, hh, hd)).abs().max_element();
        let base = m.positions.len() as u32;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = center + right * half_r * sx + up * half_u * sy;
            m.positions.push(p.to_array());
            m.normals.push(n.to_array());
            let uv = if fw == 0.0 {
                [0.02, 0.02]
            } else {
                [(sx + 1.0) / 2.0 * fw / 8.0, (1.0 - (sy + 1.0) / 2.0) * fh / 7.0]
            };
            m.uvs.push(uv);
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m.build()
}

impl Target {
    pub(crate) fn new_prop(kind: TargetKind, x: f64, z: f64, hp: f64, value: f64, shape: usize, prop: usize, size: f64) -> Self {
        let mut t = Target::new_public(kind, x, z, hp, value, shape);
        t.prop = prop;
        t.size = size;
        t
    }
}
