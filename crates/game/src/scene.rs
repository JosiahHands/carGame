//! Things in the 3D view that belong to the game rather than the map: the
//! time trial's arches and arrow, gold wrenches and challenge zones, rockets
//! in flight and the city's people. Also saving photos and the frame limit.
use bevy::camera::{ClearColorConfig, RenderTarget, visibility::RenderLayers};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use bevy::window::PrimaryWindow;
use bevy_egui::PrimaryEguiContext;

use crate::canvas::{Canvas, Rgba};
use crate::game::{Game, Mode};
use crate::meshes::MeshData;
use crate::timetrial::ArchLook;
use crate::world_build::{glow, hex_color};
use crate::world_render::MainCamera;

#[derive(Resource)]
struct SceneAssets {
    arch_next: Handle<StandardMaterial>,
    arch_finish: Handle<StandardMaterial>,
    arch_later: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    beam: Handle<StandardMaterial>,
    people_mat: Handle<StandardMaterial>,
    people_mesh: Handle<Mesh>,
    arrow: Entity,
    rockets: Vec<Entity>,
}

#[derive(Default, Resource)]
struct Spawned {
    trial_generation: u64,
    arches: Vec<Entity>,
    roam_generation: u64,
    roam: Vec<Entity>,
    spinners: Vec<(Entity, f64)>,
    signs: Vec<Entity>,
}

#[derive(Component)]
struct Billboard;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Spawned>()
            .add_systems(Startup, (setup, setup_display))
            .add_systems(
                Update,
                (trial_visuals, roam_visuals, rockets, people, billboards, photo, render_resolution).after(crate::GameSet::Frame),
            )
            .add_systems(Last, frame_limit);
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let arch_next = materials.add(StandardMaterial {
        base_color: hex_color(0xffd23f),
        emissive: glow(hex_color(0xffb000), 1.6),
        ..default()
    });
    let arch_finish = materials.add(StandardMaterial {
        base_color: hex_color(0x39ff88),
        emissive: glow(hex_color(0x20d060), 1.6),
        ..default()
    });
    let arch_later = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.45),
        emissive: glow(hex_color(0x88aaff), 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    // A flat arrow floating over the car, pointing at the next arch (along +z).
    let shape = [
        (0.0, 1.0),
        (0.7, 0.0),
        (0.25, 0.0),
        (0.25, -0.8),
        (-0.25, -0.8),
        (-0.25, 0.0),
        (-0.7, 0.0),
    ];
    let flat = crate::meshes::extrude(&shape, 0.15);
    let mut arrow_mesh = MeshData::default();
    let lay = Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    for (i, p) in flat.positions.iter().enumerate() {
        arrow_mesh.positions.push(lay.transform_point(Vec3::from(*p)).to_array());
        arrow_mesh.normals.push((lay.rotation * Vec3::from(flat.normals[i])).to_array());
        arrow_mesh.uvs.push(flat.uvs[i]);
    }
    arrow_mesh.indices = flat.indices;
    let arrow = commands
        .spawn((
            Mesh3d(meshes.add(arrow_mesh.build())),
            MeshMaterial3d(arch_next.clone()),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
        ))
        .id();
    let white = materials.add(StandardMaterial {
        base_color: hex_color(0xe8e8e8),
        metallic: 0.3,
        perceptual_roughness: 0.4,
        ..default()
    });
    let red = materials.add(StandardMaterial {
        base_color: hex_color(0xd7263d),
        perceptual_roughness: 0.5,
        ..default()
    });
    let flame = materials.add(StandardMaterial {
        base_color: hex_color(0xffb347),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let turn = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let body = meshes.add(bevy::shape::Cylinder::new(0.07, 0.7).mesh().resolution(10).build());
    let nose = meshes.add(bevy::shape::Cone { radius: 0.07, height: 0.2 }.mesh().resolution(10).build());
    let fins = meshes.add(bevy::shape::Cuboid::new(0.3, 0.02, 0.15).mesh().build());
    let exhaust = meshes.add(bevy::shape::Cone { radius: 0.08, height: 0.6 }.mesh().resolution(8).build());
    let rockets = (0..12)
        .map(|_| {
            let r = commands.spawn((Transform::default(), Visibility::Hidden)).id();
            commands.spawn((
                Mesh3d(body.clone()),
                MeshMaterial3d(white.clone()),
                Transform::from_rotation(turn),
                ChildOf(r),
            ));
            commands.spawn((
                Mesh3d(nose.clone()),
                MeshMaterial3d(red.clone()),
                Transform::from_xyz(0.0, 0.0, 0.45).with_rotation(turn),
                ChildOf(r),
            ));
            commands.spawn((
                Mesh3d(fins.clone()),
                MeshMaterial3d(red.clone()),
                Transform::from_xyz(0.0, 0.0, -0.28),
                ChildOf(r),
            ));
            commands.spawn((
                Mesh3d(exhaust.clone()),
                MeshMaterial3d(flame.clone()),
                Transform::from_xyz(0.0, 0.0, -0.62).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                NotShadowCaster,
                ChildOf(r),
            ));
            r
        })
        .collect();
    let people_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.8,
        ..default()
    });
    let people_mesh = meshes.add(MeshData::default().build());
    commands.spawn((
        Mesh3d(people_mesh.clone()),
        MeshMaterial3d(people_mat.clone()),
        Transform::default(),
        bevy::camera::visibility::NoFrustumCulling,
    ));
    commands.insert_resource(SceneAssets {
        arch_next,
        arch_finish,
        arch_later,
        gold: materials.add(StandardMaterial {
            base_color: hex_color(0xffc83d),
            emissive: glow(hex_color(0xb37400), 0.6),
            metallic: 0.9,
            perceptual_roughness: 0.25,
            ..default()
        }),
        beam: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.82, 0.25, 0.18),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        }),
        people_mat,
        people_mesh,
        arrow,
        rockets,
    });
}

fn despawn_all(commands: &mut Commands, list: &mut Vec<Entity>) {
    for e in list.drain(..) {
        if let Ok(mut ec) = commands.get_entity(e) {
            ec.despawn();
        }
    }
}

fn trial_visuals(
    game: Res<Game>,
    assets: Res<SceneAssets>,
    mut spawned: ResMut<Spawned>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
) {
    let t = &game.trial;
    if spawned.trial_generation != t.generation {
        spawned.trial_generation = t.generation;
        let mut old = std::mem::take(&mut spawned.arches);
        despawn_all(&mut commands, &mut old);
        if let Some(course) = &t.course {
            // Half a ring standing on the road.
            let ring = meshes.add({
                let torus = bevy::shape::Torus::new(course.radius as f32 - 0.3, course.radius as f32 + 0.3)
                    .mesh()
                    .minor_resolution(10)
                    .major_resolution(40)
                    .build();
                let mut m = MeshData::from_mesh(
                    &torus,
                    &Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    [1.0; 4],
                );
                // Keep the top half (y >= 0).
                let keep: Vec<u32> = m
                    .indices
                    .chunks(3)
                    .filter(|tri| tri.iter().all(|&i| m.positions[i as usize][1] >= -0.31))
                    .flatten()
                    .copied()
                    .collect();
                m.indices = keep;
                m.colors.clear();
                m.build()
            });
            for (x, y, z, heading, bank) in t.arch_poses() {
                let tr = Transform::from_xyz(x as f32, y as f32, z as f32).with_rotation(Quat::from_euler(
                    EulerRot::YXZ,
                    heading as f32,
                    0.0,
                    bank as f32,
                ));
                spawned.arches.push(
                    commands
                        .spawn((
                            Mesh3d(ring.clone()),
                            MeshMaterial3d(assets.arch_later.clone()),
                            tr,
                            Visibility::Hidden,
                            NotShadowCaster,
                        ))
                        .id(),
                );
            }
        }
    }
    for (i, &e) in spawned.arches.iter().enumerate() {
        let look = t.arches.get(i).copied().unwrap_or(ArchLook::Hidden);
        let mat = match look {
            ArchLook::Next => Some(assets.arch_next.clone()),
            ArchLook::Finish => Some(assets.arch_finish.clone()),
            ArchLook::Later => Some(assets.arch_later.clone()),
            ArchLook::Hidden => None,
        };
        if let Ok((_, mut v)) = q.get_mut(e) {
            let want = if mat.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
        if let Some(m) = mat {
            commands.entity(e).insert(MeshMaterial3d(m));
        }
    }
    if let Ok((mut tr, mut v)) = q.get_mut(assets.arrow) {
        match (&t.arrow, game.mode == Mode::Drive) {
            (Some((pose, angle)), true) => {
                *v = Visibility::Inherited;
                *tr = Transform::from_xyz(pose.x as f32, pose.y as f32, pose.z as f32).with_rotation(Quat::from_rotation_y(*angle as f32));
            }
            _ => *v = Visibility::Hidden,
        }
    }
}

fn sign_texture(label: &str, color: u32) -> Canvas {
    let mut c = Canvas::new(512, 128);
    c.fill_rect(4.0, 4.0, 504.0, 120.0, Rgba(10.0 / 255.0, 14.0 / 255.0, 24.0 / 255.0, 0.8));
    c.fill_text(label, 256.0, 66.0, 54.0, 480.0, true, Rgba::hex(color));
    c
}

fn roam_visuals(
    game: Res<Game>,
    world: Res<crate::world::World>,
    assets: Res<SceneAssets>,
    mut spawned: ResMut<Spawned>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
) {
    let roam = &game.free_roam;
    let show = game.event.is_none();
    if spawned.roam_generation != roam.generation {
        spawned.roam_generation = roam.generation;
        let mut old = std::mem::take(&mut spawned.roam);
        despawn_all(&mut commands, &mut old);
        spawned.spinners.clear();
        spawned.signs.clear();
        let handle = meshes.add(bevy::shape::Cuboid::new(0.22, 1.1, 0.12).mesh().build());
        let head = meshes.add({
            let torus = bevy::shape::Torus::new(0.18, 0.38)
                .mesh()
                .minor_resolution(8)
                .major_resolution(16)
                .build();
            MeshData::from_mesh(
                &torus,
                &Transform::from_xyz(0.0, 0.68, 0.0).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                [1.0; 4],
            )
            .build()
        });
        let beam = meshes.add(bevy::shape::Cylinder::new(0.5, 40.0).mesh().resolution(12).build());
        for (i, w) in roam.wrenches.iter().enumerate() {
            let root = commands
                .spawn((Transform::from_translation(w.pos.as_vec3()), Visibility::default()))
                .id();
            let spinner = commands
                .spawn((
                    Transform::from_rotation(Quat::from_rotation_z(0.5)),
                    Visibility::default(),
                    ChildOf(root),
                ))
                .id();
            commands.spawn((
                Mesh3d(handle.clone()),
                MeshMaterial3d(assets.gold.clone()),
                Transform::default(),
                ChildOf(spinner),
            ));
            commands.spawn((
                Mesh3d(head.clone()),
                MeshMaterial3d(assets.gold.clone()),
                Transform::default(),
                ChildOf(spinner),
            ));
            commands.spawn((
                Mesh3d(beam.clone()),
                MeshMaterial3d(assets.beam.clone()),
                Transform::from_xyz(0.0, 20.0, 0.0),
                NotShadowCaster,
                NotShadowReceiver,
                ChildOf(root),
            ));
            spawned.roam.push(root);
            spawned.spinners.push((spinner, i as f64));
        }
        for z in &roam.zones {
            let y = world.height_at(z.x, z.z) as f32;
            let c = hex_color(z.kind.color());
            let mat = materials.add(StandardMaterial {
                base_color: c.with_alpha(0.55),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let width = if z.kind == crate::freeroam::ZoneKind::Speed { 0.4 } else { 1.2 };
            let ring = meshes.add(
                bevy::shape::Annulus::new((z.radius - width) as f32, z.radius as f32)
                    .mesh()
                    .resolution(64)
                    .build(),
            );
            let root = commands
                .spawn((Transform::from_xyz(z.x as f32, y, z.z as f32), Visibility::default()))
                .id();
            commands.spawn((
                Mesh3d(ring),
                MeshMaterial3d(mat),
                Transform::from_xyz(0.0, 0.06, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                NotShadowCaster,
                NotShadowReceiver,
                ChildOf(root),
            ));
            let tex = images.add(sign_texture(z.kind.label(), z.kind.color()).into_image(false));
            let sign_mat = materials.add(StandardMaterial {
                base_color_texture: Some(tex),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            let sign = commands
                .spawn((
                    Mesh3d(meshes.add(bevy::shape::Rectangle::new(12.0, 3.0).mesh().build())),
                    MeshMaterial3d(sign_mat),
                    Transform::from_xyz(0.0, 8.0, 0.0),
                    Billboard,
                    NotShadowCaster,
                    ChildOf(root),
                ))
                .id();
            spawned.signs.push(sign);
            spawned.roam.push(root);
        }
    }
    for &e in &spawned.roam {
        if let Ok((_, mut v)) = q.get_mut(e) {
            let want = if show { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    }
    let clock = roam.clock;
    for &(e, i) in &spawned.spinners {
        if let Ok((mut t, _)) = q.get_mut(e) {
            t.rotation = Quat::from_rotation_y((clock * 2.5) as f32) * Quat::from_rotation_z(0.5);
            t.translation.y = ((clock * 2.0 + i).sin() * 0.2) as f32;
        }
    }
}

/// Signs turn to face the camera.
fn billboards(
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut signs: Query<(&mut Transform, &ChildOf), With<Billboard>>,
    parents: Query<&GlobalTransform, Without<Billboard>>,
) {
    let Ok(cam) = camera.single() else { return };
    for (mut t, parent) in &mut signs {
        let Ok(p) = parents.get(parent.parent()) else { continue };
        let world_pos = p.transform_point(t.translation);
        let to_cam = cam.translation() - world_pos;
        let yaw = to_cam.x.atan2(to_cam.z);
        t.rotation = Quat::from_rotation_y(yaw);
    }
}

fn rockets(game: Res<Game>, assets: Res<SceneAssets>, mut q: Query<(&mut Transform, &mut Visibility)>) {
    for (r, &e) in game.weapons.rockets.iter().zip(&assets.rockets) {
        let Ok((mut t, mut v)) = q.get_mut(e) else { continue };
        if !r.flying {
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
            continue;
        }
        *v = Visibility::Inherited;
        let dir = r.velocity.normalize_or_zero().as_vec3();
        // looking_to points -z along `dir`; the rocket's nose is +z, so turn it around.
        *t = Transform::from_translation(r.pos.as_vec3()).looking_to(dir, Vec3::Y);
        t.rotation *= Quat::from_rotation_y(std::f32::consts::PI);
    }
}

fn people(game: Res<Game>, assets: Res<SceneAssets>, mut meshes: ResMut<Assets<Mesh>>, mut last_empty: Local<bool>) {
    let empty = game.people.is_empty() || game.event.is_some();
    if empty && *last_empty {
        return;
    }
    *last_empty = empty;
    let data = if empty { MeshData::default() } else { game.people.mesh() };
    if let Some(mut m) = meshes.get_mut(&assets.people_mesh) {
        *m = if data.positions.is_empty() {
            // Nothing to draw: one invisible triangle.
            let mut d = MeshData::default();
            for _ in 0..3 {
                d.positions.push([0.0, -100.0, 0.0]);
                d.uvs.push([0.0; 2]);
                d.colors.push([0.0; 4]);
            }
            d.indices = vec![0, 1, 2];
            d.build()
        } else {
            data.build()
        };
    }
    let _ = &assets.people_mat;
}

/// Saves a photo of the view (without the menus) to the Pictures folder.
fn photo(mut game: ResMut<Game>, mut commands: Commands, mut wait: Local<u8>) {
    if !game.screen_shot {
        *wait = 0;
        return;
    }
    // Wait a frame so the menus are gone from the picture.
    *wait += 1;
    if *wait < 2 {
        return;
    }
    let dir = directories::UserDirs::new()
        .and_then(|u| u.picture_dir().map(|p| p.to_path_buf()))
        .or_else(crate::storage::data_dir)
        .unwrap_or_else(std::env::temp_dir);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = dir.join(format!("car-game-{secs}.png"));
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
    game.screen_shot = false;
    game.toast(format!("Photo saved to {}", path.display()), 2500.0);
}

/// The frame rate limit: waits until the next frame is due.
fn frame_limit(game: Option<Res<Game>>, mut next: Local<Option<std::time::Instant>>) {
    let Some(game) = game else { return };
    let limit = game.settings.fps_limit;
    if limit <= 0.0 {
        *next = None;
        return;
    }
    let interval = std::time::Duration::from_secs_f64(1.0 / limit);
    let now = std::time::Instant::now();
    let due = next.unwrap_or(now);
    if due > now {
        std::thread::sleep(due - now);
    }
    let after = std::time::Instant::now();
    *next = Some(if after.saturating_duration_since(due) > interval {
        after + interval
    } else {
        due + interval
    });
}

/// The window stays at full size so the HUD and menus stay sharp. Only the
/// 3D camera renders into the scaled buffer. At 100% it renders directly.
#[derive(Resource)]
struct Display {
    buffer: Handle<Image>,
    size: UVec2,
    scaled: bool,
}

#[derive(Component)]
struct GameImage;

fn setup_display(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let buffer = images.add(Image::new_target_texture(
        1,
        1,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    let layer = RenderLayers::layer(1);
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Msaa::Off,
        layer.clone(),
        PrimaryEguiContext,
    ));
    commands.spawn((GameImage, Sprite::from_image(buffer.clone()), Visibility::Hidden, layer));
    commands.insert_resource(Display {
        buffer,
        size: UVec2::ONE,
        scaled: false,
    });
}

fn render_resolution(
    game: Res<Game>,
    packs: Res<crate::packs::Packs>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut camera: Query<(&mut RenderTarget, &mut Msaa), With<MainCamera>>,
    mut image: Query<(&mut Sprite, &mut Visibility), With<GameImage>>,
    mut display: ResMut<Display>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Ok(window), Ok((mut camera, mut msaa)), Ok((mut sprite, mut visibility))) =
        (window.single(), camera.single_mut(), image.single_mut())
    else {
        return;
    };
    let mode = if game.settings.antialias && game.settings.shadows != "off" {
        Msaa::Sample4
    } else {
        Msaa::Off
    };
    if !packs.is_on("graphics") && *msaa != mode {
        *msaa = mode;
    }
    let scale = game.settings.resolution as f32;
    let scaled = scale < 0.999;
    if scaled != display.scaled {
        display.scaled = scaled;
        *camera = if scaled {
            RenderTarget::Image(display.buffer.clone().into())
        } else {
            RenderTarget::default()
        };
        *visibility = if scaled { Visibility::Inherited } else { Visibility::Hidden };
    }
    if !scaled {
        return;
    }
    let window_size = Vec2::new(window.physical_width() as f32, window.physical_height() as f32);
    let size = (window_size * scale).round().as_uvec2().max(UVec2::ONE);
    let logical_size = Vec2::new(window.width(), window.height());
    if sprite.custom_size != Some(logical_size) {
        sprite.custom_size = Some(logical_size);
    }
    if size != display.size
        && let Some(mut image) = images.get_mut(&display.buffer)
    {
        image.resize(Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        });
        display.size = size;
    }
}
