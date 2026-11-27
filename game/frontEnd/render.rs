//! Building the worlds and drawing them: the maps, roads and buildings, the
//! sky, lights, fog and weather.

pub mod world_build {
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
        Block, BlockKind, BoxShape, CHUNK, CONE_HEIGHT, Checkpoint, Circle, Cone, Course, HALF_SIZE, Prop, RISE, ROAD_LINES, ROAD_WIDTH,
        Ramp, Spot, Target, TargetKind, WEATHER_BOX, WEATHER_DROPS, WEATHER_HEIGHT, World, city_course,
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

    /// The shapes lamps and trees are drawn with.
    #[derive(Clone, Default)]
    pub struct PropShapes {
        /// For each kind of tree (`Trees as usize`): trunk, trunk color, leaves, and leaves far away.
        pub trees: Vec<(MeshData, [f32; 4], MeshData, MeshData)>,
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
            if map == "city" && self.open {
                self.land = Some(Box::new(crate::land::Land::new(self.generative)));
                self.build_city(b);
                self.half = if self.generative { 1e9 } else { crate::land::MAP_HALF };
                if !self.generative {
                    // The made map ends here, in the mountains.
                    let h = crate::land::MAP_HALF;
                    for (ax, az, bx, bz) in [(-h, -h, h, -h), (h, -h, h, h), (h, h, -h, h), (-h, h, -h, -h)] {
                        self.walls.push(make_wall(ax, az, bx, bz, 2000.0));
                    }
                }
            } else if map == "city" {
                self.build_city(b);
            } else if map == "circuit" && self.layout == 0 {
                self.build_circuit(b);
            } else if map == "oval" && self.layout == 0 {
                self.build_oval(b);
            } else {
                self.build_venue_track(b);
            }
            if self.land.is_none() {
                self.build_edges(b);
            }
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
            self.land = None;
            self.rubble.clear();
            self.watch.clear();
            self.built_towns.clear();
        }

        fn build_city(&mut self, b: &mut Build) {
            self.half = HALF_SIZE;
            self.spawn = Spot {
                x: -3.0,
                z: -20.0,
                heading: 0.0,
            };
            self.course = city_course();
            if self.land.is_none() {
                self.build_ground(b);
            }
            self.build_streets(b, 0.0, 0.0, &ROAD_LINES, HALF_SIZE - 8.0);
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
            let e = self.spawn(b, mesh, mat, Transform::default());
            b.commands.entity(e).insert(Wet);
        }

        /// A grid of streets around (cx, cz): one along each line (offsets from
        /// the middle, the same both ways), `half` meters each way, with street lamps.
        fn build_streets(&mut self, b: &mut Build, cx: f64, cz: f64, lines: &[f64], half: f64) {
            let len = half * 2.0;
            let mat = Self::tex_mat(b, b.mats.road.clone(), Vec2::new(1.0, (len / 12.0) as f32), 0.9);
            let mut roads = MeshData::default();
            for (i, &c) in lines.iter().enumerate() {
                let plane = Rectangle::new(ROAD_WIDTH as f32, len as f32).mesh().build();
                let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                // Along x (turned a quarter), and along z.
                roads.add_mesh(
                    &plane,
                    &Transform::from_xyz(cx as f32, 0.02 + i as f32 * 0.0005, (cz + c) as f32)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * flat),
                    [1.0; 4],
                );
                roads.add_mesh(
                    &plane,
                    &Transform::from_xyz((cx + c) as f32, 0.025 + i as f32 * 0.0005, cz as f32).with_rotation(flat),
                    [1.0; 4],
                );
            }
            roads.colors.clear();
            let e = self.spawn(b, roads.build(), mat, Transform::default());
            b.commands.entity(e).insert((NotShadowCaster, Wet));
            // Plain asphalt squares at the crossings, so dashed lines don't overlap.
            let mut crossings = MeshData::default();
            let square = bevy::shape::Plane3d::default()
                .mesh()
                .size(ROAD_WIDTH as f32, ROAD_WIDTH as f32)
                .build();
            for &x in lines {
                for &z in lines {
                    crossings.add_mesh(&square, &Transform::from_xyz((cx + x) as f32, 0.035, (cz + z) as f32), [1.0; 4]);
                }
            }
            crossings.colors.clear();
            let cross_mat = Self::color_mat(b, hex_color(0x34363b), 0.9);
            let e = self.spawn(b, crossings.build(), cross_mat, Transform::default());
            b.commands.entity(e).insert((NotShadowCaster, Wet));
            let mut poles = Vec::new();
            let along: Vec<f64> = if lines.len() > 1 {
                lines[..lines.len() - 1].iter().flat_map(|c| [c + 20.0, c + 60.0]).collect()
            } else {
                vec![-half + 4.0, half - 4.0]
            };
            for &c in lines {
                for &p in &along {
                    poles.push(DVec3::new(cx + c + ROAD_WIDTH / 2.0 + 1.0, 0.0, cz + p));
                    poles.push(DVec3::new(cx + p, 0.0, cz + c - ROAD_WIDTH / 2.0 - 1.0));
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
                    base: 0.0,
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
                    kind: Trees::Round,
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
                // In the open world, the highways leave the city along x = 0 and z = 0.
                if self.land.is_some() && (p.x.abs() < 14.0 || p.z.abs() < 14.0) {
                    continue;
                }
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
                base: 0.0,
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
            self.add_trees_of(points, kind);
        }

        /// Trees of one kind, standing on the ground at each point (its y).
        fn add_trees_of(&mut self, points: &[DVec3], kind: Trees) {
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
                    base: p.y,
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
                    kind,
                    transform: Transform::from_translation(p.as_vec3())
                        .with_rotation(Quat::from_rotation_y(turn as f32))
                        .with_scale(Vec3::splat(s as f32)),
                    color,
                    visible: true,
                    chunk: chunk_of(p.x, p.z),
                });
            }
        }

        /// The shapes for lamps and every kind of tree.
        pub fn prop_shapes(&self) -> PropShapes {
            let at = |x: f32, y: f32, z: f32| Transform::from_xyz(x, y, z);
            PropShapes {
                trees: [Trees::Round, Trees::Pine, Trees::Cactus, Trees::SnowPine]
                    .into_iter()
                    .map(|kind| {
                        let (trunk, color, leaves) = Self::tree_shape(kind);
                        let distant = crate::world_render::distant_foliage(&leaves);
                        (trunk, color, leaves, distant)
                    })
                    .collect(),
                pole: MeshData::from_mesh(&tapered_cylinder(0.12, 0.16, 7.0, 8), &at(0.0, 3.5, 0.0), [1.0; 4]),
                head: MeshData::from_mesh(&Cuboid::new(0.7, 0.25, 0.7).mesh().build(), &at(0.0, 7.1, 0.0), [1.0; 4]),
            }
        }

        fn tree_shape(kind: Trees) -> (MeshData, [f32; 4], MeshData) {
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
            (trunk, linear(trunk_color), leaves.flat_shaded())
        }

        fn build_parking_lot(&mut self, b: &mut Build, bx: f64, bz: f64) {
            self.fill_parking_lot(b, bx, bz, 0.55);
        }

        /// A parking lot, each space taken with chance `taken`.
        fn fill_parking_lot(&mut self, b: &mut Build, bx: f64, bz: f64, taken: f64) {
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
                    if self.rand.next() < taken {
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
            self.add_car_with(b, x, z, heading, &config)
        }

        /// A parked car built from `config`. Returns its target index.
        pub fn add_car_with(&mut self, b: &mut Build, x: f64, z: f64, heading: f64, config: &CarConfig) -> usize {
            let rig = build_car(&mut b.car_ctx(), config, false, false);
            let y = self.height_at(x, z);
            self.park_car(b, rig, x, z, heading, y)
        }

        /// A patrol car (see police.rs).
        pub fn add_police_car(&mut self, b: &mut Build, x: f64, z: f64, heading: f64) -> usize {
            let config = CarConfig {
                body: "muscle".into(),
                paint_color: "#f4f4f4".into(),
                accent_color: "#1d2c4a".into(),
                finish: "gloss".into(),
                pattern: "twoTone".into(),
                stripes: "side".into(),
                spoiler: "none".into(),
                light_bar: true,
                roof_guns: false,
                plate: "POLICE".into(),
                ..CarConfig::default()
            };
            let i = self.add_car_with(b, x, z, heading, &config);
            self.targets[i].value = 30000.0;
            i
        }

        /// Leaves a car (and its model) parked in the world, like any other
        /// parked car: where the player got out of it. Returns its target index.
        /// `y` is the height it stands at (on a banked track or a ramp, say).
        pub fn park_car(&mut self, b: &mut Build, rig: crate::carmodel::CarRig, x: f64, z: f64, heading: f64, y: f64) -> usize {
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
                height: rig.height.max(1.4),
                solid: true,
                target: Some(index),
                base: y,
            });
            let mut t = Target::new_prop(TargetKind::Car, x, z, 70.0, 12000.0, shape, 0, 1.0);
            t.rig = Some(rig);
            t.home = Spot { x, z, heading };
            t.pos = DVec3::new(x, y, z);
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
                    b.commands.entity(e).insert((NotShadowCaster, Wet));
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
                if pit_length > 60.0
                    && self.is_area_clear(pit.x - pit_length / 2.0, pit.x + pit_length / 2.0, pit.z - 6.0, pit.z + 6.0, 1.0)
                {
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
                    base: 0.0,
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
                    base: 0.0,
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
                    base: 0.0,
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
                let spread = self.rand.next() * 80.0;
                let r = 50.0 + self.rand.next() * 60.0;
                let dist = hill_distance(a, half, r, spread);
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
            let kind = self.weather_kind();
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

    /// Rocks and landmarks out in the country are drawn this far away.
    const SCENERY_SHOWN: f32 = 3000.0;

    /// Sidewalk blocks are this big inside their streets.
    const BLOCK_INNER: f64 = 80.0 - ROAD_WIDTH;

    /// The tree that grows in a biome, at height `h`.
    pub fn tree_for(biome: crate::land::Biome, h: f64) -> Trees {
        use crate::land::Biome;
        match biome {
            Biome::Forest if h > 120.0 => Trees::SnowPine,
            Biome::Forest => Trees::Pine,
            Biome::Mountains if h > 95.0 => Trees::SnowPine,
            Biome::Mountains => Trees::Pine,
            Biome::Desert => Trees::Cactus,
            Biome::Plains | Biome::Lakes => Trees::Round,
        }
    }

    /// Something that turns slowly about its own z axis (a wind turbine's blades), radians a second.
    #[derive(Component)]
    pub struct Spin(pub f32);

    impl World {
        // ---------- the open world (see land.rs) ----------

        /// Builds what's near (x, z) in the open world: the ground's heights,
        /// towns within 1.7 km, and trees and scenery within 800 m. Spends up
        /// to `budget` of time, nearest first.
        pub fn prepare_land(&mut self, b: &mut Build, x: f64, z: f64, budget: std::time::Duration) {
            let started = std::time::Instant::now();
            let Some(land) = self.land.as_mut() else { return };
            land.ensure(x, z);
            land.prepare(x, z, 900.0, 3400.0, budget.mul_f32(0.5));
            let town = land
                .towns
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.built && t.at.distance(bevy::math::DVec2::new(x, z)) < 1700.0)
                .min_by(|a, b| {
                    let p = bevy::math::DVec2::new(x, z);
                    a.1.at.distance_squared(p).total_cmp(&b.1.at.distance_squared(p))
                })
                .map(|(i, _)| i);
            if let Some(t) = town {
                self.build_town(b, t);
            }
            let hamlet = self.land.as_ref().and_then(|l| {
                l.hamlets
                    .iter()
                    .position(|h| !h.built && h.at.distance(bevy::math::DVec2::new(x, z)) < 1300.0)
            });
            if let Some(h) = hamlet {
                self.build_hamlet(b, h);
            }
            let here = crate::land::tile_of(x, z);
            let mut wanted = Vec::new();
            for dx in -4..=4 {
                for dz in -4..=4 {
                    let key = (here.0 + dx, here.1 + dz);
                    let (cx, cz) = ((key.0 as f64 + 0.5) * crate::land::TILE, (key.1 as f64 + 0.5) * crate::land::TILE);
                    let d = (cx - x).hypot(cz - z);
                    if d < 900.0 && !self.land.as_ref().is_some_and(|l| l.tiles.contains(&key)) {
                        wanted.push((d, key));
                    }
                }
            }
            wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (i, (_, key)) in wanted.into_iter().enumerate() {
                if i > 0 && started.elapsed() > budget {
                    break;
                }
                self.build_tile(b, key);
            }
            self.index_new();
        }

        /// A town's streets, blocks, buildings, parks, parking and lamps.
        fn build_town(&mut self, b: &mut Build, index: usize) {
            let Some(land) = self.land.as_mut() else { return };
            land.towns[index].built = true;
            let town = land.towns[index].clone();
            self.built_towns.push(index);
            let mut rand = Seeded::new(crate::land::hash(town.key.0 as i64, town.key.1 as i64, 50) as f64);
            let (cx, cz) = (town.at.x, town.at.y);
            self.build_streets(b, cx, cz, &town.lines, town.half);
            if town.size == crate::land::Size::Crossroads {
                // A filling station on one corner.
                let (sx, sz) = (cx + 30.0, cz + 30.0);
                self.add_building(b, sx, sz + 4.0, 14.0, 9.0, 4.5);
                let mut canopy = MeshData::default();
                canopy.add_mesh(
                    &Cuboid::new(12.0, 0.6, 7.0).mesh().build(),
                    &Transform::from_xyz(sx as f32, 4.6, (sz - 7.0) as f32),
                    linear(0xd94f2b),
                );
                for (ox, oz) in [(-5.0, -3.0), (5.0, -3.0), (-5.0, 3.0), (5.0, 3.0)] {
                    canopy.add_mesh(
                        &Cuboid::new(0.35, 4.3, 0.35).mesh().build(),
                        &Transform::from_xyz((sx + ox) as f32, 2.15, (sz - 7.0 + oz) as f32),
                        linear(0xe8e8e8),
                    );
                }
                for ox in [-2.5, 2.5] {
                    canopy.add_mesh(
                        &Cuboid::new(0.8, 1.6, 0.6).mesh().build(),
                        &Transform::from_xyz((sx + ox) as f32, 0.8, (sz - 7.0) as f32),
                        linear(0x2e6fb5),
                    );
                    self.circles.push(Circle {
                        x: sx + ox,
                        z: sz - 7.0,
                        r: 0.5,
                        height: 1.6,
                        hit_radius: 0.5,
                        solid: true,
                        target: None,
                        base: 0.0,
                    });
                }
                self.spawn(b, canopy.build(), b.mats.vertex_colored.clone(), Transform::default());
                return;
            }
            let village = town.size == crate::land::Size::Village;
            let metropolis = town.size == crate::land::Size::Metropolis;
            let inner = BLOCK_INNER;
            let sidewalk = Self::tex_mat(b, b.mats.concrete_tex.clone(), Vec2::splat((inner / 4.0) as f32), 0.95);
            let mut slabs = MeshData::default();
            let mut grass = MeshData::default();
            let mut trees = Vec::new();
            let tree_kind = tree_for(town.biome, 0.0);
            for (k, block) in town.blocks().into_iter().enumerate() {
                let (bx, bz) = (block.x, block.y);
                let plane = bevy::shape::Plane3d::default().mesh().size(inner as f32, inner as f32).build();
                slabs.add_mesh(&plane, &Transform::from_xyz(bx as f32, 0.015, bz as f32), [1.0; 4]);
                let r = rand.next();
                let park = if village {
                    r < 0.3
                } else if metropolis {
                    r < 0.07
                } else {
                    r < 0.22
                };
                let lot = !village && !park && r < if metropolis { 0.1 } else { 0.36 };
                if park {
                    let lawn = bevy::shape::Plane3d::default()
                        .mesh()
                        .size((inner - 8.0) as f32, (inner - 8.0) as f32)
                        .build();
                    grass.add_mesh(&lawn, &Transform::from_xyz(bx as f32, 0.018, bz as f32), [1.0; 4]);
                    for _ in 0..14 {
                        let x = bx + (rand.next() - 0.5) * (inner - 14.0);
                        let z = bz + (rand.next() - 0.5) * (inner - 14.0);
                        trees.push(DVec3::new(x, 0.0, z));
                    }
                } else if lot && k % 3 == 0 {
                    std::mem::swap(&mut self.rand, &mut rand);
                    self.fill_parking_lot(b, bx, bz, 0.22);
                    std::mem::swap(&mut self.rand, &mut rand);
                } else {
                    for ox in [-1.0, 1.0] {
                        for oz in [-1.0, 1.0] {
                            if village && rand.next() < 0.35 {
                                continue;
                            }
                            let (w, d, h) = if village {
                                (9.0 + rand.next() * 6.0, 9.0 + rand.next() * 6.0, 4.5 + rand.next() * 5.0)
                            } else if metropolis {
                                // Towers, tallest in the middle of the city.
                                let middle = 1.0 - ((bx - cx).hypot(bz - cz) / town.half).min(1.0);
                                let tall = 30.0 + rand.next().powf(1.4) * (90.0 + 170.0 * middle);
                                (18.0 + rand.next() * 10.0, 18.0 + rand.next() * 10.0, tall)
                            } else {
                                (
                                    12.0 + rand.next() * 12.0,
                                    12.0 + rand.next() * 12.0,
                                    6.0 + rand.next().powf(1.7) * 30.0,
                                )
                            };
                            std::mem::swap(&mut self.rand, &mut rand);
                            self.add_building(b, bx + ox * 17.0, bz + oz * 17.0, w, d, h);
                            std::mem::swap(&mut self.rand, &mut rand);
                        }
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
            std::mem::swap(&mut self.rand, &mut rand);
            self.add_trees_of(&trees, tree_kind);
            std::mem::swap(&mut self.rand, &mut rand);
        }

        /// The trees, rocks and landmarks in one 250 m square of the country.
        fn build_tile(&mut self, b: &mut Build, key: (i32, i32)) {
            let Some(land) = self.land.as_mut() else { return };
            land.tiles.insert(key);
            let land = self.land.as_ref().unwrap();
            let tile = crate::land::TILE;
            let (x0, z0) = (key.0 as f64 * tile, key.1 as f64 * tile);
            let mut rand = Seeded::new(crate::land::hash(key.0 as i64, key.1 as i64, 60) as f64);
            let w = land.biomes(x0 + tile / 2.0, z0 + tile / 2.0);
            // Trees.
            // Woods here and there on the farmland, deep forest in the west.
            let woods = crate::land::smoothstep(0.15, 0.4, crate::land::fbm(x0 / 900.0, z0 / 900.0, 61, 2));
            let count = (5.0 * w[0] + 90.0 * w[0] * woods + 125.0 * w[1] + 40.0 * w[2] + 4.0 * w[3] + 16.0 * w[4]).round() as usize;
            let mut by_kind: [Vec<DVec3>; 4] = Default::default();
            for _ in 0..count {
                let (x, z) = (x0 + rand.next() * tile, z0 + rand.next() * tile);
                if !land.clear(x, z, 3.0) {
                    continue;
                }
                let h = land.ground(x, z);
                let biome = land.main_biome(x, z);
                if biome == crate::land::Biome::Mountains && h > 175.0 + rand.next() * 40.0 {
                    continue; // above the tree line
                }
                // On the farmland, trees grow in the woods, and only here and there in the fields.
                if biome == crate::land::Biome::Plains && rand.next() > 0.5 + woods * 0.5 {
                    continue;
                }
                let kind = tree_for(biome, h);
                by_kind[kind as usize].push(DVec3::new(x, h, z));
            }
            // Rocks and boulders.
            let rocks = (4.0 * w[2] + 3.5 * w[3] + 0.6 * w[1]).round() as usize;
            let mut stone = MeshData::default();
            let shape = tapered_cylinder(1.0, 1.25, 1.0, 7);
            for _ in 0..rocks {
                let (x, z) = (x0 + rand.next() * tile, z0 + rand.next() * tile);
                let r = 1.5 + rand.next() * 5.0;
                if !land.clear(x, z, r + 2.0) {
                    continue;
                }
                let base = [(-r, -r), (r, -r), (-r, r), (r, r)]
                    .iter()
                    .map(|(dx, dz)| land.ground(x + dx, z + dz))
                    .fold(f64::INFINITY, f64::min);
                let h = r * (0.8 + rand.next() * 1.4);
                let desert = land.main_biome(x, z) == crate::land::Biome::Desert;
                let color = if desert {
                    linear_hsl(0.04 + rand.next() * 0.03, 0.5, 0.38 + rand.next() * 0.1)
                } else {
                    linear_hsl(0.08, 0.06, 0.4 + rand.next() * 0.12)
                };
                let t = Transform::from_xyz(x as f32, (base + h / 2.0 - 0.3) as f32, z as f32)
                    .with_rotation(Quat::from_rotation_y((rand.next() * 6.0) as f32))
                    .with_scale(Vec3::new(r as f32, h as f32, r as f32));
                stone.add_mesh(&shape, &t, color);
                self.circles.push(Circle {
                    x,
                    z,
                    r: r * 1.1,
                    height: h,
                    hit_radius: r * 1.1,
                    solid: true,
                    target: None,
                    base: base - 0.3,
                });
            }
            // Now and then a landmark: wind turbines and barns on the farmland,
            // cabins in the woods, lighthouses by the lakes, masts on the mountains.
            let mut scenery = MeshData::default();
            let mut turbines = Vec::new();
            let roll = rand.next();
            let (lx, lz) = (x0 + 40.0 + rand.next() * (tile - 80.0), z0 + 40.0 + rand.next() * (tile - 80.0));
            let biome = land.main_biome(lx, lz);
            let ground = land.ground(lx, lz);
            use crate::land::Biome;
            match biome {
                Biome::Plains if roll < 0.14 && land.clear(lx, lz, 14.0) => {
                    for k in 0..3 {
                        let (tx, tz) = (lx + k as f64 * 70.0 - 70.0, lz + (rand.next() - 0.5) * 40.0);
                        if land.clear(tx, tz, 10.0) {
                            turbines.push(DVec3::new(tx, land.ground(tx, tz), tz));
                        }
                    }
                }
                Biome::Plains if roll < 0.24 && land.clear(lx, lz, 22.0) => {
                    let turn = Quat::from_rotation_y((rand.next() * PI) as f32);
                    let at = Vec3::new(lx as f32, ground as f32, lz as f32);
                    scenery.add_mesh(
                        &Cuboid::new(14.0, 7.0, 20.0).mesh().build(),
                        &Transform::from_translation(at + turn * Vec3::new(0.0, 3.5, 0.0)).with_rotation(turn),
                        linear(0x9c2f25),
                    );
                    let roof = Cuboid::new(10.5, 10.5, 20.6).mesh().build();
                    scenery.add_mesh(
                        &roof,
                        &Transform::from_translation(at + turn * Vec3::new(0.0, 7.0, 0.0))
                            .with_rotation(turn * Quat::from_rotation_z(PI as f32 / 4.0))
                            .with_scale(Vec3::new(0.95, 0.5, 1.0)),
                        linear(0x5a5552),
                    );
                    scenery.add_mesh(
                        &Cylinder::new(3.0, 16.0).mesh().resolution(14).build(),
                        &Transform::from_translation(at + turn * Vec3::new(11.0, 8.0, 4.0)),
                        linear(0xb8bcc0),
                    );
                    self.add_solid_box(lx, lz, 13.0, ground, 9.0);
                }
                Biome::Forest if roll < 0.1 && land.clear(lx, lz, 10.0) => {
                    let turn = Quat::from_rotation_y((rand.next() * PI) as f32);
                    let at = Vec3::new(lx as f32, ground as f32, lz as f32);
                    scenery.add_mesh(
                        &Cuboid::new(7.0, 3.2, 9.0).mesh().build(),
                        &Transform::from_translation(at + Vec3::Y * 1.6).with_rotation(turn),
                        linear(0x7a5232),
                    );
                    scenery.add_mesh(
                        &Cuboid::new(5.6, 5.6, 9.6).mesh().build(),
                        &Transform::from_translation(at + Vec3::Y * 3.3)
                            .with_rotation(turn * Quat::from_rotation_z(PI as f32 / 4.0))
                            .with_scale(Vec3::new(1.0, 0.55, 1.0)),
                        linear(0x3d3a36),
                    );
                    scenery.add_mesh(
                        &Cuboid::new(0.9, 2.5, 0.9).mesh().build(),
                        &Transform::from_translation(at + turn * Vec3::new(2.0, 4.6, 2.0)),
                        linear(0x6b625a),
                    );
                    self.add_solid_box(lx, lz, 5.5, ground, 4.5);
                }
                Biome::Lakes if roll < 0.12 && land.clear(lx, lz, 6.0) && ground < 3.0 => {
                    let at = Vec3::new(lx as f32, ground as f32, lz as f32);
                    for k in 0..6 {
                        let color = if k % 2 == 0 { 0xf2f2f2 } else { 0xc8322b };
                        scenery.add_mesh(
                            &tapered_cylinder(1.6 - k as f32 * 0.12, 1.72 - k as f32 * 0.12, 3.0, 12),
                            &Transform::from_translation(at + Vec3::Y * (1.5 + k as f32 * 3.0)),
                            linear(color),
                        );
                    }
                    scenery.add_mesh(
                        &Cylinder::new(1.3, 2.0).mesh().resolution(12).build(),
                        &Transform::from_translation(at + Vec3::Y * 19.0),
                        linear(0xffe9a8),
                    );
                    scenery.add_mesh(
                        &ConeShape { radius: 1.6, height: 1.6 }.mesh().resolution(12).build(),
                        &Transform::from_translation(at + Vec3::Y * 20.8),
                        linear(0x2b2b2b),
                    );
                    self.circles.push(Circle {
                        x: lx,
                        z: lz,
                        r: 1.8,
                        height: 21.0,
                        hit_radius: 1.8,
                        solid: true,
                        target: None,
                        base: ground,
                    });
                }
                Biome::Mountains if roll < 0.06 && ground > 60.0 && land.clear(lx, lz, 6.0) => {
                    let at = Vec3::new(lx as f32, ground as f32, lz as f32);
                    for k in 0..8 {
                        let s = 1.0 - k as f32 * 0.1;
                        scenery.add_mesh(
                            &Cuboid::new(1.6 * s, 6.0, 1.6 * s).mesh().build(),
                            &Transform::from_translation(at + Vec3::Y * (3.0 + k as f32 * 6.0)),
                            linear(if k % 2 == 0 { 0xd23b2b } else { 0xf0f0f0 }),
                        );
                    }
                    self.circles.push(Circle {
                        x: lx,
                        z: lz,
                        r: 1.0,
                        height: 48.0,
                        hit_radius: 1.0,
                        solid: true,
                        target: None,
                        base: ground,
                    });
                }
                _ => {}
            }
            for t in &turbines {
                let at = t.as_vec3();
                scenery.add_mesh(
                    &tapered_cylinder(0.9, 1.6, 60.0, 12),
                    &Transform::from_translation(at + Vec3::Y * 30.0),
                    linear(0xeceff1),
                );
                scenery.add_mesh(
                    &Cuboid::new(2.2, 2.4, 5.0).mesh().build(),
                    &Transform::from_translation(at + Vec3::new(0.0, 61.0, 0.0)),
                    linear(0xe3e6e8),
                );
                self.circles.push(Circle {
                    x: t.x,
                    z: t.z,
                    r: 1.7,
                    height: 62.0,
                    hit_radius: 1.7,
                    solid: true,
                    target: None,
                    base: t.y,
                });
                // The blades turn (see world_render's `spin`).
                let mut blades = MeshData::default();
                for k in 0..3 {
                    let a = k as f32 * std::f32::consts::TAU / 3.0;
                    blades.add_mesh(
                        &Cuboid::new(1.4, 26.0, 0.3).mesh().build(),
                        &(Transform::from_rotation(Quat::from_rotation_z(a)) * Transform::from_xyz(0.0, 13.5, 0.0)),
                        linear(0xf4f6f7),
                    );
                }
                let root = self.root.expect("map root");
                let face = Quat::from_rotation_y((t.x * 0.001 + t.z * 0.002) as f32 % 0.6);
                b.commands.spawn((
                    Mesh3d(b.meshes.add(blades.build())),
                    MeshMaterial3d(b.mats.vertex_colored.clone()),
                    Transform::from_translation(at + Vec3::new(0.0, 61.0, 0.0) + face * Vec3::new(0.0, 0.0, 2.8)).with_rotation(face),
                    Spin(0.9 + (t.x.abs() % 0.4) as f32),
                    bevy::camera::visibility::VisibilityRange::abrupt(0.0, SCENERY_SHOWN),
                    ChildOf(root),
                ));
            }
            for (k, points) in by_kind.iter().enumerate() {
                if !points.is_empty() {
                    let kind = [Trees::Round, Trees::Pine, Trees::Cactus, Trees::SnowPine][k];
                    std::mem::swap(&mut self.rand, &mut rand);
                    self.add_trees_of(points, kind);
                    std::mem::swap(&mut self.rand, &mut rand);
                }
            }
            // Far behind you, the country's rocks and landmarks aren't drawn.
            // (Each mesh is placed at the tile's middle, so the distance is measured from there.)
            let far = bevy::camera::visibility::VisibilityRange::abrupt(0.0, SCENERY_SHOWN);
            let middle = Vec3::new((x0 + tile / 2.0) as f32, 0.0, (z0 + tile / 2.0) as f32);
            let local = |mut m: MeshData| {
                for p in &mut m.positions {
                    p[0] -= middle.x;
                    p[2] -= middle.z;
                }
                m
            };
            if !stone.is_empty() {
                let e = self.spawn(
                    b,
                    local(stone.flat_shaded()).build(),
                    b.mats.vertex_colored.clone(),
                    Transform::from_translation(middle),
                );
                b.commands.entity(e).insert(far.clone());
            }
            if !scenery.is_empty() {
                let e = self.spawn(
                    b,
                    local(scenery).build(),
                    b.mats.vertex_colored.clone(),
                    Transform::from_translation(middle),
                );
                b.commands.entity(e).insert(far);
            }
        }

        /// A few houses either side of the highway, a barn and some trees.
        fn build_hamlet(&mut self, b: &mut Build, index: usize) {
            let Some(land) = self.land.as_mut() else { return };
            land.hamlets[index].built = true;
            let h = land.hamlets[index].clone();
            let mut rand = Seeded::new((h.at.x * 7.0 + h.at.y * 13.0).abs().floor());
            let right = bevy::math::DVec2::new(-h.dir.y, h.dir.x);
            let houses = 3 + (rand.next() * 4.0) as usize;
            let mut trees = Vec::new();
            std::mem::swap(&mut self.rand, &mut rand);
            for k in 0..houses {
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                let along = (k as f64 - houses as f64 / 2.0) * 16.0 + (self.rand.next() - 0.5) * 4.0;
                let at = h.at + h.dir * along + right * side * (17.0 + self.rand.next() * 6.0);
                let (w, d, ht) = (
                    7.5 + self.rand.next() * 4.0,
                    7.0 + self.rand.next() * 3.0,
                    4.0 + self.rand.next() * 3.0,
                );
                self.add_building(b, at.x, at.y, w, d, ht);
                let behind = at + right * side * 12.0 + h.dir * (self.rand.next() - 0.5) * 10.0;
                trees.push(DVec3::new(behind.x, 0.0, behind.y));
            }
            let barn = h.at - h.dir * (houses as f64 * 9.0 + 14.0) + right * 22.0;
            self.add_building(b, barn.x, barn.y, 12.0, 16.0, 8.0);
            let lamps = [h.at + right * 9.0, h.at - right * 9.0 + h.dir * 20.0];
            self.add_lamps(&lamps.map(|p| DVec3::new(p.x, 0.0, p.y)), 1.0);
            self.add_trees_of(&trees, Trees::Round);
            std::mem::swap(&mut self.rand, &mut rand);
        }

        /// A square block nobody can drive through (a barn, a cabin), `half` meters each way.
        fn add_solid_box(&mut self, x: f64, z: f64, half: f64, base: f64, height: f64) {
            self.boxes.push(BoxShape {
                min_x: x - half,
                max_x: x + half,
                min_z: z - half,
                max_z: z + half,
                height,
                solid: true,
                target: None,
                base: base - 0.5,
            });
        }
    }

    /// How far from the middle of the map a horizon hill `r` meters across
    /// (at its foot) goes, in direction `angle`. The map and its walls are a
    /// square, so the hill goes past the square's edge in that direction, by
    /// more than its own radius: never over the corners, where tracks reach.
    pub fn hill_distance(angle: f64, half: f64, r: f64, spread: f64) -> f64 {
        let (s, c) = angle.sin_cos();
        (half + r + 10.0 + spread) / s.abs().max(c.abs())
    }

    /// Ground, roads and pavement: a pack can make them look wet in the rain.
    #[derive(Component)]
    pub struct Wet;

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

    #[cfg(test)]
    mod tests {
        use super::*;

        /// No hill reaches inside the map's walls, where the track can go
        /// right up to the corners.
        #[test]
        fn horizon_hills_stay_off_the_map() {
            let half = 700.0;
            for i in 0..720 {
                let a = i as f64 / 720.0 * PI * 2.0;
                for r in [50.0, 110.0] {
                    let d = hill_distance(a, half, r, 0.0);
                    let (x, z) = (a.cos() * d, a.sin() * d);
                    // The nearest point of the square to the hill's middle.
                    let (nx, nz) = (x.clamp(-half, half), z.clamp(-half, half));
                    assert!((x - nx).hypot(z - nz) >= r, "angle {a}: hill overlaps the map");
                    assert!(d < (half + r + 10.0) * 1.5);
                }
            }
        }
    }
}

pub mod world_render {
    //! Shows the world on screen: applies what changed (things moving, falling
    //! over, wrecked or rebuilt), draws the lamps and trees in merged groups, the
    //! sky, fog, sun, weather and the effects.
    use std::collections::{HashMap, HashSet};

    use bevy::asset::RenderAssetUsages;
    use bevy::camera::visibility::VisibilityRange;
    use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::pbr::{DistanceFog, FogFalloff};
    use bevy::prelude::*;

    use crate::meshes::MeshData;
    use crate::venues::Weather;
    use crate::world::{Change, World};
    use crate::world_build::{PropShapes, Spin, StartLight, Wet, WorldMats, css_color, glow, hex_color};

    /// The camera the game is seen through.
    #[derive(Component)]
    pub struct MainCamera;

    #[derive(Component)]
    pub struct Sun;

    /// The game's own painted sky (a pack can hide it and draw a real one).
    #[derive(Component)]
    pub struct SkyDome;

    /// Where the player's car is, so the sun's shadows and the weather stay around it.
    #[derive(Resource, Default, Clone, Copy)]
    pub struct Focus {
        pub x: f64,
        pub z: f64,
    }

    /// Graphics choices the world needs to know about.
    #[derive(Resource, Clone)]
    pub struct Graphics {
        /// "off", "low", "high" or "ultra".
        pub shadows: String,
        /// How far the shadows reach, for big cars (×).
        pub shadow_spread: f32,
        pub view_distance: f32,
    }

    impl Default for Graphics {
        fn default() -> Self {
            Graphics {
                shadows: "high".into(),
                shadow_spread: 1.0,
                view_distance: 430.0,
            }
        }
    }

    #[derive(Resource, Default)]
    struct Chunks {
        generation: u64,
        shapes: PropShapes,
        /// How many of the world's props are in `members`.
        known: usize,
        spawned: HashMap<(i32, i32), Vec<Entity>>,
        members: HashMap<(i32, i32), Vec<usize>>,
        dirty: HashSet<(i32, i32)>,
    }

    #[derive(Resource)]
    struct EffectMeshes {
        glow: Handle<Mesh>,
        smoke: Handle<Mesh>,
        debris: Handle<Mesh>,
        flashes: Handle<Mesh>,
        scorch: Handle<Mesh>,
        weather: Handle<Mesh>,
        weather_entity: Entity,
        light: Entity,
    }

    pub struct WorldRenderPlugin;

    impl Plugin for WorldRenderPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<Focus>()
                .init_resource::<Graphics>()
                .init_resource::<Chunks>()
                .init_resource::<LandMeshes>()
                .add_systems(Startup, setup)
                .add_systems(Update, spin)
                .add_systems(
                    PostUpdate,
                    (
                        apply_changes,
                        rebuild_chunks,
                        land_meshes,
                        environment,
                        start_lights,
                        effects_meshes,
                        weather_mesh,
                    )
                        .chain()
                        .in_set(WorldRenderSet)
                        .before(TransformSystems::Propagate),
                );
        }
    }

    /// The world's drawing systems (packs can run theirs after, to change the lighting).
    #[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
    pub struct WorldRenderSet;

    fn empty_mesh(meshes: &mut Assets<Mesh>, topology: PrimitiveTopology) -> Handle<Mesh> {
        let mut m = Mesh::new(topology, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
        m.insert_indices(Indices::U32(vec![0, 1, 2]));
        meshes.add(m)
    }

    fn setup(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut images: ResMut<Assets<Image>>,
    ) {
        let mats = WorldMats::new(&mut materials, &mut images, &mut meshes);
        commands.insert_resource(mats);
        // The sun.
        commands.spawn((
            Sun,
            DirectionalLight {
                color: hex_color(0xfff2dd),
                illuminance: 2.6 * LUX,
                shadow_maps_enabled: true,
                shadow_depth_bias: 0.04,
                shadow_normal_bias: 1.2,
                ..default()
            },
            Transform::from_xyz(40.0, 80.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        // The sky: a big ball around the camera, colored top to bottom.
        let sky_mesh = {
            let sphere = bevy::shape::Sphere::new(1.0).mesh().uv(32, 16);
            let mut m = MeshData::from_mesh(&sphere, &Transform::default(), [1.0; 4]);
            for t in m.indices.chunks_mut(3) {
                t.swap(1, 2); // seen from inside
            }
            m.build_editable()
        };
        let sky_mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            fog_enabled: false,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            SkyDome,
            Mesh3d(meshes.add(sky_mesh)),
            MeshMaterial3d(sky_mat),
            Transform::default(),
            NotShadowCaster,
            NotShadowReceiver,
        ));

        // One mesh for each kind of effect, rebuilt every frame.
        let glow_tex = images.add(crate::canvas::Canvas::radial_glow(128).into_image(false));
        let additive = materials.add(StandardMaterial {
            base_color_texture: Some(glow_tex.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            fog_enabled: false,
            cull_mode: None,
            ..default()
        });
        let blended = materials.add(StandardMaterial {
            base_color_texture: Some(glow_tex.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        });
        let flashes = materials.add(StandardMaterial {
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        });
        let scorch = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            base_color_texture: Some(glow_tex.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            depth_bias: 2.0,
            ..default()
        });
        let debris = materials.add(StandardMaterial {
            perceptual_roughness: 0.9,
            ..default()
        });
        let weather_mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(glow_tex.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        let tri = PrimitiveTopology::TriangleList;
        let glow_mesh = empty_mesh(&mut meshes, tri);
        let smoke_mesh = empty_mesh(&mut meshes, tri);
        let debris_mesh = empty_mesh(&mut meshes, tri);
        let flash_mesh = empty_mesh(&mut meshes, tri);
        let scorch_mesh = empty_mesh(&mut meshes, tri);
        let weather_mesh = empty_mesh(&mut meshes, tri);
        let quiet = || (NotShadowCaster, NotShadowReceiver, bevy::camera::visibility::NoFrustumCulling);
        commands.spawn((Mesh3d(glow_mesh.clone()), MeshMaterial3d(additive), Transform::default(), quiet()));
        commands.spawn((Mesh3d(smoke_mesh.clone()), MeshMaterial3d(blended), Transform::default(), quiet()));
        commands.spawn((
            Mesh3d(debris_mesh.clone()),
            MeshMaterial3d(debris),
            Transform::default(),
            bevy::camera::visibility::NoFrustumCulling,
        ));
        commands.spawn((Mesh3d(flash_mesh.clone()), MeshMaterial3d(flashes), Transform::default(), quiet()));
        commands.spawn((Mesh3d(scorch_mesh.clone()), MeshMaterial3d(scorch), Transform::default(), quiet()));
        let weather_entity = commands
            .spawn((
                Mesh3d(weather_mesh.clone()),
                MeshMaterial3d(weather_mat),
                Transform::default(),
                quiet(),
                Visibility::Hidden,
            ))
            .id();
        let light = commands
            .spawn((
                PointLight {
                    color: hex_color(0xffa040),
                    intensity: 0.0,
                    range: 40.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::default(),
            ))
            .id();
        commands.insert_resource(EffectMeshes {
            glow: glow_mesh,
            smoke: smoke_mesh,
            debris: debris_mesh,
            flashes: flash_mesh,
            scorch: scorch_mesh,
            weather: weather_mesh,
            weather_entity,
            light,
        });
    }

    /// Water in the open world's lakes (a pack can make it shine).
    #[derive(Component)]
    pub struct Water;

    /// The open world's ground and highways that are on the screen.
    #[derive(Resource, Default)]
    struct LandMeshes {
        generation: u64,
        /// Chunks drawn in detail (8 m) and far off (32 m): the ground, and any water.
        near: HashMap<(i32, i32), Vec<Entity>>,
        far: HashMap<(i32, i32), Vec<Entity>>,
        /// Pieces of highway drawn: (road, piece) and its entity, and where it is.
        roads: HashMap<(usize, usize), (Entity, f64, f64)>,
        mats: Option<(Handle<StandardMaterial>, Handle<StandardMaterial>, Handle<StandardMaterial>)>,
        /// Where the highways were last looked at, and the land's version then.
        last: Option<(f64, f64, u64)>,
        /// Where far-off ground was last put away.
        last_evict: Option<(f64, f64)>,
    }

    /// The ground is drawn in detail within this distance, and roughly to `LAND_FAR`.
    const LAND_NEAR: f64 = 900.0;
    const LAND_FAR: f64 = 3400.0;
    /// Highways are this many points (6 m apart) to a piece.
    const ROAD_PIECE: usize = 40;

    fn spin(time: Res<Time>, mut q: Query<(&Spin, &mut Transform)>) {
        for (s, mut t) in &mut q {
            t.rotate_local_z(s.0 * time.delta_secs());
        }
    }

    /// One chunk of ground as a mesh, from its height grid (`cells` squares a
    /// side), with skirts round the edge so chunks of the other detail never
    /// leave a gap. Also a water surface if any of it is under water.
    fn ground_mesh(land: &crate::land::Land, key: (i32, i32), grid: &[f32], cells: usize) -> (MeshData, Option<MeshData>) {
        use crate::land::{CHUNK, WATER};
        let step = CHUNK / cells as f64;
        let (x0, z0) = (key.0 as f64 * CHUNK, key.1 as f64 * CHUNK);
        let n = cells + 1;
        let h = |i: usize, j: usize| f64::from(grid[j.min(cells) * n + i.min(cells)]);
        let mut m = MeshData::default();
        let mut lowest = f64::INFINITY;
        for j in 0..n {
            for i in 0..n {
                let (x, z) = (x0 + i as f64 * step, z0 + j as f64 * step);
                let y = h(i, j);
                lowest = lowest.min(y);
                let dx = h((i + 1).min(cells), j) - h(i.saturating_sub(1), j);
                let dz = h(i, (j + 1).min(cells)) - h(i, j.saturating_sub(1));
                let span_x = step * ((i + 1).min(cells) - i.saturating_sub(1)) as f64;
                let span_z = step * ((j + 1).min(cells) - j.saturating_sub(1)) as f64;
                let normal = Vec3::new((-dx / span_x) as f32, 1.0, (-dz / span_z) as f32).normalize();
                let slope = 1.0 - normal.y as f64;
                let c = land.ground_color(x, z, y, (slope * 3.0).min(1.0));
                m.vertex([x, y, z], [x / 6.5, z / 6.5]);
                m.normals.push(normal.to_array());
                m.colors.push([c[0], c[1], c[2], 1.0]);
            }
        }
        for j in 0..cells as u32 {
            for i in 0..cells as u32 {
                let a = j * n as u32 + i;
                let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
                // The same split as Land::ground: (a, b, c) and (b, d, c).
                m.tri(a, c, b);
                m.tri(b, c, d);
            }
        }
        // Skirts: each edge hangs down 10 m.
        let edges: [Vec<u32>; 4] = [
            (0..n as u32).collect(),
            (0..n as u32).map(|i| (n as u32 - 1) * n as u32 + i).collect(),
            (0..n as u32).map(|j| j * n as u32).collect(),
            (0..n as u32).map(|j| j * n as u32 + n as u32 - 1).collect(),
        ];
        for edge in edges {
            for w in edge.windows(2) {
                let (a, b) = (w[0] as usize, w[1] as usize);
                let base = m.positions.len() as u32;
                for &k in &[a, b] {
                    let p = m.positions[k];
                    m.positions.push([p[0], p[1] - 10.0, p[2]]);
                    m.uvs.push(m.uvs[k]);
                    m.normals.push(m.normals[k]);
                    m.colors.push(m.colors[k]);
                }
                // Both sides, so the skirt shows whichever way it faces.
                m.tri(a as u32, base, b as u32);
                m.tri(b as u32, base, base + 1);
                m.tri(a as u32, b as u32, base);
                m.tri(b as u32, base + 1, base);
            }
        }
        let water = (lowest < WATER).then(|| {
            let mut w = MeshData::default();
            let y = WATER;
            let a = w.vertex([x0, y, z0], [0.0, 0.0]);
            let b = w.vertex([x0 + CHUNK, y, z0], [1.0, 0.0]);
            let c = w.vertex([x0, y, z0 + CHUNK], [0.0, 1.0]);
            let d = w.vertex([x0 + CHUNK, y, z0 + CHUNK], [1.0, 1.0]);
            w.normals = vec![[0.0, 1.0, 0.0]; 4];
            w.tri(a, c, b);
            w.tri(b, c, d);
            w
        });
        (m, water)
    }

    /// A piece of highway: the asphalt ribbon, a little above the flat ground beside it.
    fn road_piece(land: &crate::land::Land, road: usize, piece: usize) -> Option<MeshData> {
        let r = &land.roads[road];
        let start = piece * ROAD_PIECE;
        if start + 1 >= r.points.len() {
            return None;
        }
        let end = (start + ROAD_PIECE).min(r.points.len() - 1);
        let (from, to) = (&land.towns[r.from], &land.towns[r.to]);
        // A highway is one strip; an interstate, two carriageways either side of a median.
        let strips: &[(f32, f32)] = if r.interstate {
            &[(2.2, 11.8), (-11.8, -2.2)]
        } else {
            &[(-7.0, 7.0)]
        };
        let mut m = MeshData::default();
        for &(inner, outer) in strips {
            road_strip(&mut m, r, start, end, from, to, inner, outer);
        }
        (!m.is_empty()).then_some(m)
    }

    /// One strip of asphalt along points `start..=end`, from `inner` to `outer` meters right of the middle.
    #[allow(clippy::too_many_arguments)]
    fn road_strip(
        m: &mut MeshData,
        r: &crate::land::Road,
        start: usize,
        end: usize,
        from: &crate::land::Town,
        to: &crate::land::Town,
        inner: f32,
        outer: f32,
    ) {
        let mut last: Option<(u32, u32)> = None;
        for p in &r.points[start..=end] {
            // The towns draw their own streets (the highway reaches 8 m into
            // them, more than a point's spacing, so there's never a gap).
            if from.contains(p.x, p.z, -8.0) || to.contains(p.x, p.z, -8.0) {
                last = None;
                continue;
            }
            let (rx, rz) = (-p.dz as f32, p.dx as f32);
            let (x, z) = (p.x as f32, p.z as f32);
            let v = (p.along / 12.0) as f32;
            let y = 0.06;
            let l = m.positions.len() as u32;
            m.positions.push([x + rx * outer, y, z + rz * outer]);
            m.positions.push([x + rx * inner, y, z + rz * inner]);
            m.uvs.push([0.0, v]);
            m.uvs.push([1.0, v]);
            m.normals.push([0.0, 1.0, 0.0]);
            m.normals.push([0.0, 1.0, 0.0]);
            if let Some((a, b)) = last {
                m.tri(a, l, b);
                m.tri(b, l, l + 1);
            }
            last = Some((l, l + 1));
        }
    }

    /// Streams the open world's ground and highways in and out around the car.
    #[allow(clippy::too_many_arguments)]
    fn land_meshes(
        world: Res<World>,
        focus: Res<Focus>,
        mats: Res<WorldMats>,
        mut lm: ResMut<LandMeshes>,
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        if lm.generation != world.generation {
            // The old map's entities went with it.
            lm.generation = world.generation;
            lm.near.clear();
            lm.far.clear();
            lm.roads.clear();
            lm.last = None;
            lm.last_evict = None;
        }
        let (Some(land), Some(root)) = (world.land.as_deref(), world.root) else {
            return;
        };
        if lm.mats.is_none() {
            let ground = materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 1.0,
                reflectance: 0.2,
                ..default()
            });
            let water = materials.add(StandardMaterial {
                base_color: hex_color(0x2f6f8f),
                perceptual_roughness: 0.12,
                reflectance: 0.6,
                ..default()
            });
            let road = materials.add(StandardMaterial {
                base_color_texture: Some(mats.road.clone()),
                perceptual_roughness: 0.9,
                ..default()
            });
            lm.mats = Some((ground, water, road));
        }
        let (ground_mat, water_mat, road_mat) = lm.mats.clone().unwrap();
        let (x, z) = (focus.x, focus.z);
        let moved = lm.last.is_none_or(|l| l.2 != land.version || (x - l.0).hypot(z - l.1) > 24.0);
        let started = std::time::Instant::now();
        let budget = std::time::Duration::from_millis(4);
        let here = crate::land::chunk_of(x, z);
        let size = crate::land::CHUNK;
        let distance = |key: (i32, i32)| {
            let (cx, cz) = ((key.0 as f64 + 0.5) * size, (key.1 as f64 + 0.5) * size);
            ((cx - x).hypot(cz - z) - size * 0.7).max(0.0)
        };
        let despawn = |commands: &mut Commands, list: Vec<Entity>| {
            for e in list {
                if let Ok(mut ec) = commands.get_entity(e) {
                    ec.despawn();
                }
            }
        };
        if lm.last_evict.is_none_or(|l| (x - l.0).hypot(z - l.1) > 24.0) {
            lm.last_evict = Some((x, z));
            let near_keys: Vec<_> = lm.near.keys().copied().collect();
            for key in near_keys {
                if distance(key) > LAND_NEAR + 250.0 {
                    let list = lm.near.remove(&key).unwrap_or_default();
                    despawn(&mut commands, list);
                }
            }
            let far_keys: Vec<_> = lm.far.keys().copied().collect();
            for key in far_keys {
                if distance(key) > LAND_FAR + 400.0 || lm.near.contains_key(&key) {
                    let list = lm.far.remove(&key).unwrap_or_default();
                    despawn(&mut commands, list);
                }
            }
        }
        // Nearest first: detailed chunks close by, rough ones further out.
        let r = (LAND_FAR / size).ceil() as i32 + 1;
        let mut wanted = Vec::new();
        for dx in -r..=r {
            for dz in -r..=r {
                let key = (here.0 + dx, here.1 + dz);
                let d = distance(key);
                if d <= LAND_NEAR && !lm.near.contains_key(&key) && land.fine_chunk(key).is_some() {
                    wanted.push((d, key, true));
                } else if d <= LAND_FAR && !lm.near.contains_key(&key) && !lm.far.contains_key(&key) {
                    wanted.push((d, key, false));
                }
            }
        }
        wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
        let spawn = |commands: &mut Commands, meshes: &mut Assets<Mesh>, data: (MeshData, Option<MeshData>)| {
            let mut list = Vec::new();
            let e = commands
                .spawn((
                    Mesh3d(meshes.add(data.0.build())),
                    MeshMaterial3d(ground_mat.clone()),
                    Transform::default(),
                    Wet,
                    ChildOf(root),
                ))
                .id();
            list.push(e);
            if let Some(w) = data.1 {
                list.push(
                    commands
                        .spawn((
                            Mesh3d(meshes.add(w.build())),
                            MeshMaterial3d(water_mat.clone()),
                            Transform::default(),
                            NotShadowCaster,
                            Water,
                            ChildOf(root),
                        ))
                        .id(),
                );
            }
            list
        };
        for (_, key, fine) in wanted {
            if started.elapsed() > budget {
                break;
            }
            if fine {
                let Some(grid) = land.fine_chunk(key) else { continue };
                let list = spawn(&mut commands, &mut meshes, ground_mesh(land, key, &grid, crate::land::FINE));
                lm.near.insert(key, list);
                if let Some(old) = lm.far.remove(&key) {
                    despawn(&mut commands, old);
                }
            } else {
                let Some(grid) = land.coarse_chunk(key) else { continue };
                let list = spawn(&mut commands, &mut meshes, ground_mesh(land, key, &grid, crate::land::COARSE));
                lm.far.insert(key, list);
            }
        }
        if moved || started.elapsed() < budget {
            lm.last = Some((x, z, land.version));
            // Highways within 2.5 km; put away again past 3.2 km.
            let far: Vec<(usize, usize)> = lm
                .roads
                .iter()
                .filter(|(_, v)| (v.1 - x).hypot(v.2 - z) > 3200.0)
                .map(|(k, _)| *k)
                .collect();
            for key in far {
                if let Some((e, _, _)) = lm.roads.remove(&key)
                    && let Ok(mut ec) = commands.get_entity(e)
                {
                    ec.despawn();
                }
            }
            for road in land.roads_near(x, z, 2500.0) {
                let r = &land.roads[road];
                let pieces = r.points.len().div_ceil(ROAD_PIECE);
                for piece in 0..pieces {
                    if lm.roads.contains_key(&(road, piece)) {
                        continue;
                    }
                    let p = r.points[(piece * ROAD_PIECE).min(r.points.len() - 1)];
                    if (p.x - x).hypot(p.z - z) > 2500.0 {
                        continue;
                    }
                    let e = match road_piece(land, road, piece) {
                        Some(m) => commands
                            .spawn((
                                Mesh3d(meshes.add(m.build())),
                                MeshMaterial3d(road_mat.clone()),
                                Transform::default(),
                                NotShadowCaster,
                                Wet,
                                ChildOf(root),
                            ))
                            .id(),
                        None => commands.spawn((Transform::default(), ChildOf(root))).id(),
                    };
                    lm.roads.insert((road, piece), (e, p.x, p.z));
                    if started.elapsed() > budget {
                        return;
                    }
                }
            }
        }
    }

    /// Brightness of three.js's lights in Bevy's units: lux for the sun, lumens for lamps.
    pub const LUX: f32 = 3135.0;
    pub const LUMENS: f32 = 12540.0;

    #[allow(clippy::too_many_arguments)]
    fn apply_changes(
        mut world: ResMut<World>,
        mut chunks: ResMut<Chunks>,
        mut transforms: Query<&mut Transform>,
        mut visibility: Query<&mut Visibility>,
        mut commands: Commands,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut meshes: ResMut<Assets<Mesh>>,
    ) {
        if world.generation != chunks.generation {
            chunks.generation = world.generation;
            chunks.shapes = world.prop_shapes();
            chunks.known = 0;
            chunks.spawned.clear(); // their entities went with the old map
            chunks.members.clear();
            for (i, p) in world.props.iter().enumerate() {
                chunks.members.entry(p.chunk).or_default().push(i);
            }
            chunks.dirty = chunks.members.keys().copied().collect();
            chunks.known = world.props.len();
        }
        // The open world adds trees and lamps as you go.
        for i in chunks.known..world.props.len() {
            let key = world.props[i].chunk;
            chunks.members.entry(key).or_default().push(i);
            chunks.dirty.insert(key);
        }
        chunks.known = world.props.len();
        let changes = std::mem::take(&mut world.changes);
        for change in changes {
            match change {
                Change::Move(e, t) => {
                    if let Ok(mut tr) = transforms.get_mut(e) {
                        *tr = t;
                    }
                }
                Change::Show(e, on) => {
                    if let Ok(mut v) = visibility.get_mut(e) {
                        *v = if on { Visibility::Inherited } else { Visibility::Hidden };
                    }
                }
                Change::Wreck(i, on) => {
                    if let Some(rig) = world.targets[i].rig.as_mut() {
                        rig.set_wrecked(on, &mut materials, &mut commands);
                    }
                }
                Change::Chunk(c) => {
                    chunks.dirty.insert(c);
                }
                Change::MakeRubble(i) => {
                    let t = &world.targets[i];
                    if let Some(r) = t.rubble {
                        if let Ok(mut v) = visibility.get_mut(r) {
                            *v = Visibility::Inherited;
                        }
                        continue;
                    }
                    let mesh = rubble_mesh(t.width, t.depth);
                    let color = hex_color(t.color).to_linear() * 0.7;
                    let mat = materials.add(StandardMaterial {
                        base_color: Color::LinearRgba(color.with_alpha(1.0)),
                        perceptual_roughness: 1.0,
                        ..default()
                    });
                    let Some(root) = world.root else { continue };
                    let e = commands
                        .spawn((
                            Mesh3d(meshes.add(mesh)),
                            MeshMaterial3d(mat),
                            Transform::from_xyz(t.x as f32, 0.02, t.z as f32).with_scale(Vec3::new(1.0, 0.001, 1.0)),
                            ChildOf(root),
                        ))
                        .id();
                    world.targets[i].rubble = Some(e);
                }
                Change::Despawn(e) => {
                    if let Ok(mut ec) = commands.get_entity(e) {
                        ec.despawn();
                    }
                }
            }
        }
    }

    /// A lumpy mound of rubble covering a building's footprint (see world::rubble_shape).
    fn rubble_mesh(width: f64, depth: f64) -> Mesh {
        let (nx, nz) = ((width / 1.5).round().max(1.0) as usize, (depth / 1.5).round().max(1.0) as usize);
        let mut m = MeshData::default();
        for j in 0..=nz {
            for i in 0..=nx {
                let x = -width / 2.0 + width * i as f64 / nx as f64;
                let z = -depth / 2.0 + depth * j as f64 / nz as f64;
                let shape = crate::world::rubble_shape(x, z, width, depth);
                let lumps = if shape > 0.0 { (crate::rng::random() - 0.3) * 0.35 } else { 0.0 };
                m.vertex([x, (crate::world::RUBBLE_HEIGHT * shape + lumps).max(0.0), z], [0.0, 0.0]);
            }
        }
        for j in 0..nz as u32 {
            for i in 0..nx as u32 {
                let a = j * (nx as u32 + 1) + i;
                let b = a + nx as u32 + 1;
                m.tri(a, b, a + 1);
                m.tri(a + 1, b, b + 1);
            }
        }
        m.flat_shaded().build()
    }

    /// How far from the car groups of trees and lamps are drawn in the open
    /// world (they're put away a little further out, so they don't flicker).
    const PROPS_SHOWN: f64 = 1300.0;
    const PROPS_PUT_AWAY: f64 = 1500.0;

    fn chunk_distance(key: (i32, i32), x: f64, z: f64) -> f64 {
        let size = crate::world::CHUNK;
        let (cx, cz) = ((key.0 as f64 + 0.5) * size, (key.1 as f64 + 0.5) * size);
        (cx - x).hypot(cz - z)
    }

    /// Draws each group of lamps and trees that changed as a few merged meshes.
    fn rebuild_chunks(
        world: Res<World>,
        focus: Res<Focus>,
        mut chunks: ResMut<Chunks>,
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mats: Res<WorldMats>,
        mut last: Local<Option<(f64, f64, u64)>>,
    ) {
        let Some(root) = world.root else { return };
        let open = world.is_open();
        if open && last.is_none_or(|l| l.2 != world.generation || (focus.x - l.0).hypot(focus.z - l.1) > 40.0) {
            *last = Some((focus.x, focus.z, world.generation));
            let keys: Vec<(i32, i32)> = chunks.members.keys().copied().collect();
            for key in keys {
                let d = chunk_distance(key, focus.x, focus.z);
                if d > PROPS_PUT_AWAY {
                    if let Some(old) = chunks.spawned.remove(&key) {
                        for e in old {
                            if let Ok(mut ec) = commands.get_entity(e) {
                                ec.despawn();
                            }
                        }
                    }
                } else if d < PROPS_SHOWN && !chunks.spawned.contains_key(&key) {
                    chunks.dirty.insert(key);
                }
            }
        }
        if chunks.dirty.is_empty() {
            return;
        }
        let dirty: Vec<(i32, i32)> = chunks.dirty.drain().collect();
        for key in dirty {
            if open && chunk_distance(key, focus.x, focus.z) > PROPS_SHOWN && !chunks.spawned.contains_key(&key) {
                continue;
            }
            if let Some(old) = chunks.spawned.remove(&key) {
                for e in old {
                    if let Ok(mut ec) = commands.get_entity(e) {
                        ec.despawn();
                    }
                }
            }
            let Some(members) = chunks.members.get(&key) else { continue };
            let s = &chunks.shapes;
            let (mut wood, mut leaves, mut heads, mut distant) =
                (MeshData::default(), MeshData::default(), MeshData::default(), MeshData::default());
            // Both detail levels use the same origin for a stable crossfade.
            let size = crate::world::CHUNK as f32;
            let center = Vec3::new((key.0 as f32 + 0.5) * size, 0.0, (key.1 as f32 + 0.5) * size);
            for &i in members {
                let p = &world.props[i];
                if !p.visible {
                    continue;
                }
                let mut local = p.transform;
                local.translation -= center;
                if p.lamp {
                    append_moved(&mut wood, &s.pole, &local, crate::meshes::linear(0x3a3d42));
                    append_moved(&mut heads, &s.head, &local, [1.0; 4]);
                } else {
                    let (trunk, trunk_color, shape, far) = &s.trees[p.kind as usize];
                    append_moved(&mut wood, trunk, &local, *trunk_color);
                    append_moved(&mut leaves, shape, &local, p.color);
                    append_moved(&mut distant, far, &local, p.color);
                }
            }
            let mut spawned = Vec::new();
            let near = VisibilityRange {
                end_margin: 115.0..145.0,
                ..VisibilityRange::abrupt(0.0, 145.0)
            };
            let far = VisibilityRange {
                start_margin: near.end_margin.clone(),
                ..VisibilityRange::abrupt(115.0, f32::INFINITY)
            };
            for (data, mat, range) in [
                (wood, mats.vertex_colored.clone(), None),
                (leaves, mats.vertex_colored_flat.clone(), Some(near)),
                (heads, mats.lamp_head.clone(), None),
                (distant, mats.vertex_colored_flat.clone(), Some(far)),
            ] {
                if data.is_empty() {
                    continue;
                }
                let mut entity = commands.spawn((
                    Mesh3d(meshes.add(data.build())),
                    MeshMaterial3d(mat),
                    Transform::from_translation(center),
                    ChildOf(root),
                ));
                if let Some(range) = range {
                    entity.insert(range);
                }
                spawned.push(entity.id());
            }
            chunks.spawned.insert(key, spawned);
        }
    }

    /// Eight triangles retain the shape's height and spread at a distance.
    pub fn distant_foliage(shape: &MeshData) -> MeshData {
        if shape.is_empty() {
            return MeshData::default();
        }
        let (min, max) = shape
            .positions
            .iter()
            .map(|p| Vec3::from_array(*p))
            .fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(min, max), p| {
                (min.min(p), max.max(p))
            });
        let half = (max - min) * 0.5;
        let center = (max + min) * 0.5;
        let mut data = MeshData::default();
        for offset in [Vec3::Y, Vec3::NEG_Y, Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z] {
            data.vertex((center + half * offset).as_dvec3().to_array(), [0.0, 0.0]);
        }
        for i in 0..4 {
            let a = i + 2;
            let b = (i + 1) % 4 + 2;
            data.tri(0, b, a);
            data.tri(1, a, b);
        }
        data.flat_shaded()
    }

    fn append_moved(out: &mut MeshData, shape: &MeshData, t: &Transform, color: [f32; 4]) {
        let base = out.positions.len() as u32;
        let m = t.to_matrix();
        let rot = t.rotation;
        for (i, p) in shape.positions.iter().enumerate() {
            out.positions.push(m.transform_point3(Vec3::from(*p)).to_array());
            out.normals.push((rot * Vec3::from(shape.normals[i])).to_array());
            out.uvs.push(shape.uvs.get(i).copied().unwrap_or([0.0; 2]));
            out.colors.push(color);
        }
        out.indices.extend(shape.indices.iter().map(|i| i + base));
    }

    /// Sky, fog, sun and ambient light for the venue, day or night, and the lit windows and lamps.
    #[allow(clippy::too_many_arguments)]
    fn environment(
        world: Res<World>,
        focus: Res<Focus>,
        graphics: Res<Graphics>,
        mats: Res<WorldMats>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut sun: Query<
            (&mut DirectionalLight, &mut Transform, Option<&mut bevy::light::CascadeShadowConfig>),
            (With<Sun>, Without<SkyDome>, Without<MainCamera>),
        >,
        mut sky: Query<(&Mesh3d, &mut Transform), (With<SkyDome>, Without<MainCamera>)>,
        mut camera: Query<(Entity, &Transform, Option<&mut DistanceFog>, Option<&mut AmbientLight>), With<MainCamera>>,
        mut commands: Commands,
        mut last: Local<Option<(bool, u64, String, String)>>,
    ) {
        let night = world.is_night();
        let theme = if world.map == "city" { None } else { Some(world.theme()) };
        let Ok((cam_entity, cam_t, fog, ambient)) = camera.single_mut() else {
            return;
        };
        let weather = if world.weather_kind() != Weather::Clear { 0.6 } else { 1.0 };
        let fog_color = if night {
            hex_color(0x0b1222)
        } else {
            hex_color(theme.map_or(0xc4dcef, |t| t.fog))
        };
        // The open world fades into a far haze, so the mountains show.
        let (start, end) = if world.is_open() {
            (
                graphics.view_distance * 0.5 * weather,
                (LAND_FAR as f32 - 150.0) * weather.max(0.75),
            )
        } else {
            (
                (60.0f32).min(graphics.view_distance * 0.2) * weather,
                graphics.view_distance * weather,
            )
        };
        match fog {
            Some(mut f) => {
                f.color = fog_color;
                f.falloff = FogFalloff::Linear { start, end };
            }
            None => {
                commands.entity(cam_entity).insert(DistanceFog {
                    color: fog_color,
                    falloff: FogFalloff::Linear { start, end },
                    ..default()
                });
            }
        }
        let ambient_color = if night { hex_color(0x6b7fb3) } else { hex_color(0xdfefff) };
        let ambient_brightness = if night { 0.25 } else { 1.2 } * 420.0;
        match ambient {
            Some(mut a) => {
                a.color = ambient_color;
                a.brightness = ambient_brightness;
            }
            None => {
                commands.entity(cam_entity).insert(AmbientLight {
                    color: ambient_color,
                    brightness: ambient_brightness,
                    ..default()
                });
            }
        }
        if let Ok((mut light, mut t, cascades)) = sun.single_mut() {
            light.illuminance = if night { 0.35 } else { 2.6 } * LUX;
            light.color = if night { hex_color(0x9bb0ff) } else { hex_color(0xfff2dd) };
            light.shadow_maps_enabled = graphics.shadows != "off";
            let (fx, fz) = (focus.x as f32, focus.z as f32);
            *t = Transform::from_xyz(fx + 40.0, 80.0, fz + 25.0).looking_at(Vec3::new(fx, 0.0, fz), Vec3::Y);
            let key = (
                night,
                world.generation,
                graphics.shadows.clone(),
                format!("{}", graphics.shadow_spread),
            );
            if last.as_ref() != Some(&key) {
                let (cascade_count, reach) = match graphics.shadows.as_str() {
                    "low" => (1, 35.0),
                    "ultra" => (3, 70.0),
                    _ => (2, 45.0),
                };
                let config = CascadeShadowConfigBuilder {
                    num_cascades: cascade_count,
                    maximum_distance: reach * 2.2 * graphics.shadow_spread,
                    first_cascade_far_bound: reach * 0.8 * graphics.shadow_spread,
                    ..default()
                }
                .build();
                match cascades {
                    Some(mut c) => *c = config,
                    None => {
                        commands.entity(cam_entity).insert(config);
                    }
                }
            }
        }
        // The sky follows the camera, just inside the far end of the view.
        if let Ok((mesh, mut t)) = sky.single_mut() {
            t.translation = cam_t.translation;
            t.scale = Vec3::splat(if world.is_open() {
                LAND_FAR as f32 + 300.0
            } else {
                graphics.view_distance.max(200.0) + 150.0
            });
            let key = (night, world.generation, String::new(), String::new());
            if last.as_ref().map(|k| (k.0, k.1)) != Some((key.0, key.1))
                && let Some(mut m) = meshes.get_mut(&mesh.0)
            {
                let (top, bottom) = if night {
                    (css_color("#02040b"), css_color("#18233a"))
                } else {
                    (css_color(world.theme().sky[0]), css_color(world.theme().sky[1]))
                };
                let (top, bottom) = (top.to_linear(), bottom.to_linear());
                let colors: Vec<[f32; 4]> = match m.attribute(Mesh::ATTRIBUTE_POSITION) {
                    Some(bevy::mesh::VertexAttributeValues::Float32x3(ps)) => ps
                        .iter()
                        .map(|p| {
                            // Gradient over the top half; below the horizon stays the bottom color.
                            let k = p[1].clamp(0.0, 1.0).powf(0.7);
                            let c = bottom * (1.0 - k) + top * k;
                            [c.red, c.green, c.blue, 1.0]
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                m.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            }
        }
        let key = (
            night,
            world.generation,
            graphics.shadows.clone(),
            format!("{}", graphics.shadow_spread),
        );
        if last.as_ref() != Some(&key) {
            for (m, _) in &mats.buildings {
                if let Some(mut mat) = materials.get_mut(m) {
                    mat.emissive = if night { glow(Color::WHITE, 1.4) } else { LinearRgba::NONE };
                }
            }
            if let Some(mut mat) = materials.get_mut(&mats.lamp_head) {
                mat.emissive = if night { glow(hex_color(0xffd9a0), 3.0) } else { LinearRgba::NONE };
            }
            *last = Some(key);
        }
    }

    fn start_lights(
        world: Res<World>,
        lights: Query<(&StartLight, &MeshMaterial3d<StandardMaterial>)>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut shown: Local<Option<(usize, u64)>>,
    ) {
        if *shown == Some((world.start_lights, world.generation)) {
            return;
        }
        *shown = Some((world.start_lights, world.generation));
        for (light, mat) in &lights {
            if let Some(mut m) = materials.get_mut(&mat.0) {
                m.emissive = if light.0 < world.start_lights {
                    glow(Color::srgb(1.0, 0.0, 0.0), 1.6)
                } else {
                    LinearRgba::NONE
                };
            }
        }
    }

    fn set_mesh(mesh: &mut Mesh, data: MeshData) {
        let empty = data.positions.is_empty();
        let n = data.positions.len();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, if empty { vec![[0.0; 3]; 3] } else { data.positions });
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            if empty {
                vec![[0.0, 1.0, 0.0]; 3]
            } else if data.normals.len() == n {
                data.normals
            } else {
                vec![[0.0, 1.0, 0.0]; n]
            },
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, if empty { vec![[0.0; 2]; 3] } else { data.uvs });
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, if empty { vec![[0.0; 4]; 3] } else { data.colors });
        mesh.insert_indices(Indices::U32(if empty { vec![0, 1, 2] } else { data.indices }));
    }

    /// A square facing the camera.
    fn billboard(m: &mut MeshData, at: Vec3, size: f32, right: Vec3, up: Vec3, color: [f32; 4]) {
        let base = m.positions.len() as u32;
        let (r, u) = (right * size / 2.0, up * size / 2.0);
        for (p, uv) in [
            (at - r - u, [0.0, 1.0]),
            (at + r - u, [1.0, 1.0]),
            (at + r + u, [1.0, 0.0]),
            (at - r + u, [0.0, 0.0]),
        ] {
            m.positions.push(p.to_array());
            m.uvs.push(uv);
            m.colors.push(color);
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Rebuilds the effect meshes from the particles, debris and flashes.
    fn effects_meshes(
        world: Res<World>,
        handles: Option<Res<EffectMeshes>>,
        mut meshes: ResMut<Assets<Mesh>>,
        camera: Query<&GlobalTransform, With<MainCamera>>,
        mut lights: Query<(&mut PointLight, &mut Transform)>,
    ) {
        let Some(h) = handles else { return };
        let Ok(cam) = camera.single() else { return };
        let (right, up) = (cam.right().as_vec3(), cam.up().as_vec3());
        let fx = &world.effects;
        // Smoke and dust aren't lit by the scene's lights, so at night they're
        // darkened here (otherwise tire smoke glows white in the dark).
        let smoke_light = if world.is_night() { 0.08 } else { 1.0 };
        for (pool, handle, additive) in [(&fx.glow, &h.glow, true), (&fx.smoke, &h.smoke, false)] {
            let mut m = MeshData::default();
            for p in pool.iter().filter(|p| p.alive) {
                let t = (p.age / p.life) as f32;
                let size = (p.size[0] + (p.size[1] - p.size[0]) * t as f64) as f32;
                let mix = |a: f32, b: f32| a + (b - a) * t;
                let srgb = Color::srgb(
                    mix(p.start_color[0], p.end_color[0]),
                    mix(p.start_color[1], p.end_color[1]),
                    mix(p.start_color[2], p.end_color[2]),
                )
                .to_linear();
                let alpha = (p.opacity as f32) * (1.0 - t);
                let color = if additive {
                    [srgb.red * alpha, srgb.green * alpha, srgb.blue * alpha, alpha]
                } else {
                    [srgb.red * smoke_light, srgb.green * smoke_light, srgb.blue * smoke_light, alpha]
                };
                billboard(&mut m, p.pos.as_vec3(), size, right, up, color);
            }
            if let Some(mut mesh) = meshes.get_mut(handle) {
                set_mesh(&mut mesh, m);
            }
        }
        // Debris: little boxes.
        let mut m = MeshData::default();
        let cube = bevy::shape::Cuboid::new(1.0, 1.0, 1.0).mesh().build();
        for d in fx.debris.iter().filter(|d| d.alive) {
            let t = Transform::from_translation(d.pos.as_vec3())
                .with_rotation(Quat::from_euler(EulerRot::XYZ, d.rot.x as f32, d.rot.y as f32, d.rot.z as f32))
                .with_scale(d.scale.as_vec3().max(Vec3::splat(0.0001)));
            let c = Color::srgb(d.color[0], d.color[1], d.color[2]).to_linear();
            m.add_mesh(&cube, &t, [c.red, c.green, c.blue, 1.0]);
        }
        if let Some(mut mesh) = meshes.get_mut(&h.debris) {
            set_mesh(&mut mesh, m);
        }
        // Fireballs, rings and tracers glow.
        let mut m = MeshData::default();
        let ball = bevy::shape::Sphere::new(1.0).mesh().ico(2).unwrap();
        for f in fx.fireballs.iter().filter(|f| f.alive) {
            let t = (f.age / f.life) as f32;
            let a = (1.0 - t) * 0.9;
            let c = Color::srgb(1.0, 0.65 - t * 0.4, (0.25 - t * 0.2).max(0.0)).to_linear();
            m.add_mesh(
                &ball,
                &Transform::from_translation(f.pos.as_vec3()).with_scale(Vec3::splat((f.size as f32) * (0.4 + t.sqrt() * 1.1))),
                [c.red * a, c.green * a, c.blue * a, a],
            );
        }
        for f in fx.rings.iter().filter(|f| f.alive) {
            let t = (f.age / f.life) as f32;
            let a = (1.0 - t) * 0.9;
            let c = hex_color(0x9fdcff).to_linear();
            let s = 1.0 + f.size as f32 * t.sqrt();
            let ring = bevy::shape::Annulus::new(0.8, 1.0).mesh().resolution(48).build();
            m.add_mesh(
                &ring,
                &Transform::from_translation(f.pos.as_vec3())
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::splat(s)),
                [c.red * a, c.green * a, c.blue * a, a],
            );
        }
        let rod = bevy::shape::Cuboid::new(0.05, 0.05, 1.0).mesh().build();
        let tracer_color = hex_color(0xffd27a).to_linear();
        for f in fx.tracers.iter().filter(|f| f.alive) {
            let length = f.from.distance(f.to);
            let front = length.min(f.age / f.life * length + 2.0);
            let back = (front - 4.0).max(0.0);
            let dir = (f.to - f.from).normalize_or_zero().as_vec3();
            if dir == Vec3::ZERO {
                continue;
            }
            let mid = f.from.as_vec3() + dir * ((back + front) / 2.0) as f32;
            let t = Transform::from_translation(mid).looking_to(dir, Vec3::Y).with_scale(Vec3::new(
                1.0,
                1.0,
                ((front - back) as f32).max(0.01),
            ));
            m.add_mesh(&rod, &t, [tracer_color.red, tracer_color.green, tracer_color.blue, 1.0]);
        }
        if let Some(mut mesh) = meshes.get_mut(&h.flashes) {
            set_mesh(&mut mesh, m);
        }
        // Scorch marks on the ground.
        let mut m = MeshData::default();
        let flat = bevy::shape::Plane3d::default().mesh().size(1.0, 1.0).build();
        for f in fx.scorch_marks.iter().filter(|f| f.alive) {
            let a = 0.75 * ((f.life - f.age) / 10.0).min(1.0) as f32;
            let t = Transform::from_translation(f.pos.as_vec3())
                .with_rotation(Quat::from_rotation_y(f.turn as f32))
                .with_scale(Vec3::new(f.size as f32, 1.0, f.size as f32));
            m.add_mesh(&flat, &t, [0.0, 0.0, 0.0, a]);
        }
        if let Some(mut mesh) = meshes.get_mut(&h.scorch) {
            set_mesh(&mut mesh, m);
        }
        if let Ok((mut light, mut t)) = lights.get_mut(h.light) {
            light.intensity = fx.light_intensity as f32 * LUMENS;
            light.color = Color::srgb(fx.light_color[0], fx.light_color[1], fx.light_color[2]);
            t.translation = fx.light_pos.as_vec3();
        }
    }

    /// Rain as thin streaks, snow as soft flakes.
    fn weather_mesh(
        world: Res<World>,
        handles: Option<Res<EffectMeshes>>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut vis: Query<&mut Visibility>,
        camera: Query<&GlobalTransform, With<MainCamera>>,
    ) {
        let Some(h) = handles else { return };
        if let Ok(mut v) = vis.get_mut(h.weather_entity) {
            let want = if world.weather.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
            if *v != want {
                *v = want;
            }
        }
        if world.weather.is_empty() {
            return;
        }
        let Ok(cam) = camera.single() else { return };
        let (right, up) = (cam.right().as_vec3(), cam.up().as_vec3());
        let rain = world.weather_speed > 5.0;
        let mut m = MeshData::default();
        if rain {
            let c = hex_color(0xaec4d6).to_linear();
            let color = [c.red, c.green, c.blue, 0.45];
            for pair in world.weather.chunks(2) {
                let (a, b) = (Vec3::from(pair[0]), Vec3::from(pair[1]));
                let side = right * 0.015;
                let base = m.positions.len() as u32;
                for p in [a - side, a + side, b + side, b - side] {
                    m.positions.push(p.to_array());
                    m.uvs.push([0.5, 0.5]);
                    m.colors.push(color);
                }
                m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        } else {
            for p in &world.weather {
                billboard(&mut m, Vec3::from(*p), 0.28, right, up, [1.0, 1.0, 1.0, 0.95]);
            }
        }
        if let Some(mut mesh) = meshes.get_mut(&h.weather) {
            set_mesh(&mut mesh, m);
        }
    }
}
