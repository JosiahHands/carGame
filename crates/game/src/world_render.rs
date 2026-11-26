//! Shows the world on screen: applies what changed (things moving, falling
//! over, wrecked or rebuilt), draws the lamps and trees in merged groups, the
//! sky, fog, sun, weather and the effects.
use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::meshes::MeshData;
use crate::venues::Weather;
use crate::world::{Change, World};
use crate::world_build::{PropShapes, StartLight, WorldMats, css_color, glow, hex_color};

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
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                (
                    apply_changes,
                    rebuild_chunks,
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
        chunks.spawned.clear(); // their entities went with the old map
        chunks.members.clear();
        for (i, p) in world.props.iter().enumerate() {
            chunks.members.entry(p.chunk).or_default().push(i);
        }
        chunks.dirty = chunks.members.keys().copied().collect();
    }
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

/// Draws each group of lamps and trees that changed as a few merged meshes.
fn rebuild_chunks(
    world: Res<World>,
    mut chunks: ResMut<Chunks>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mats: Res<WorldMats>,
) {
    if chunks.dirty.is_empty() {
        return;
    }
    let Some(root) = world.root else { return };
    let dirty: Vec<(i32, i32)> = chunks.dirty.drain().collect();
    for key in dirty {
        if let Some(old) = chunks.spawned.remove(&key) {
            for e in old {
                if let Ok(mut ec) = commands.get_entity(e) {
                    ec.despawn();
                }
            }
        }
        let Some(members) = chunks.members.get(&key) else { continue };
        let s = &chunks.shapes;
        let (mut wood, mut leaves, mut heads) = (MeshData::default(), MeshData::default(), MeshData::default());
        for &i in members {
            let p = &world.props[i];
            if !p.visible {
                continue;
            }
            if p.lamp {
                append_moved(&mut wood, &s.pole, &p.transform, crate::meshes::linear(0x3a3d42));
                append_moved(&mut heads, &s.head, &p.transform, [1.0; 4]);
            } else {
                append_moved(&mut wood, &s.trunk, &p.transform, s.trunk_color);
                append_moved(&mut leaves, &s.leaves, &p.transform, p.color);
            }
        }
        let mut spawned = Vec::new();
        for (data, mat) in [
            (wood, mats.vertex_colored.clone()),
            (leaves, mats.vertex_colored_flat.clone()),
            (heads, mats.lamp_head.clone()),
        ] {
            if data.is_empty() {
                continue;
            }
            spawned.push(
                commands
                    .spawn((
                        Mesh3d(meshes.add(data.build())),
                        MeshMaterial3d(mat),
                        Transform::default(),
                        ChildOf(root),
                    ))
                    .id(),
            );
        }
        chunks.spawned.insert(key, spawned);
    }
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
    let weather = if theme.is_some_and(|t| t.weather != Weather::Clear) {
        0.6
    } else {
        1.0
    };
    let fog_color = if night {
        hex_color(0x0b1222)
    } else {
        hex_color(theme.map_or(0xc4dcef, |t| t.fog))
    };
    let start = (60.0f32).min(graphics.view_distance * 0.2) * weather;
    let end = graphics.view_distance * weather;
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
        t.scale = Vec3::splat(graphics.view_distance.max(200.0) + 150.0);
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
                [srgb.red, srgb.green, srgb.blue, alpha]
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
        let t =
            Transform::from_translation(mid)
                .looking_to(dir, Vec3::Y)
                .with_scale(Vec3::new(1.0, 1.0, ((front - back) as f32).max(0.01)));
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
