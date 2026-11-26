//! The pieces the pictures are made of: the scene's extras (arches, rockets,
//! people, photos), effects, painted textures, mesh building and the
//! graphics quality picked for this computer.

pub mod scene {
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
                .add_systems(Startup, (setup, setup_display.after(crate::game::setup_game)))
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
                    *tr = Transform::from_xyz(pose.x as f32, pose.y as f32, pose.z as f32)
                        .with_rotation(Quat::from_rotation_y(*angle as f32));
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
    /// 3D camera renders into the buffer. The display camera composites it with
    /// the HUD in one pass, including at 100% resolution.
    #[derive(Resource)]
    struct Display {
        buffer: Handle<Image>,
        size: UVec2,
    }

    #[derive(Component)]
    struct GameImage;

    fn setup_display(mut commands: Commands, mut images: ResMut<Assets<Image>>, camera: Query<Entity, With<MainCamera>>) {
        let Ok(scene) = camera.single() else { return };
        let buffer = images.add(Image::new_target_texture(
            1,
            1,
            TextureFormat::Rgba8Unorm,
            Some(TextureFormat::Rgba8UnormSrgb),
        ));
        commands.entity(scene).insert(RenderTarget::Image(buffer.clone().into()));
        let layer = RenderLayers::layer(1);
        commands.spawn((
            Camera2d,
            Camera {
                order: 1,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            Msaa::Off,
            layer.clone(),
            PrimaryEguiContext,
        ));
        commands.spawn((GameImage, Sprite::from_image(buffer.clone()), layer));
        commands.insert_resource(Display { buffer, size: UVec2::ONE });
    }

    fn render_resolution(
        game: Res<Game>,
        packs: Res<crate::packs::Packs>,
        window: Query<&Window, With<PrimaryWindow>>,
        mut camera: Query<&mut Msaa, With<MainCamera>>,
        mut image: Query<&mut Sprite, With<GameImage>>,
        mut display: ResMut<Display>,
        mut images: ResMut<Assets<Image>>,
    ) {
        let (Ok(window), Ok(mut msaa), Ok(mut sprite)) = (window.single(), camera.single_mut(), image.single_mut()) else {
            return;
        };
        let mode = if game.settings.antialias { Msaa::Sample4 } else { Msaa::Off };
        if !packs.is_on("graphics") && *msaa != mode {
            *msaa = mode;
        }
        let scale = game.settings.resolution as f32;
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
}

pub mod effects {
    //! Explosions and what they leave behind: fire, smoke, sparks, flying debris,
    //! fireballs, shockwave rings, scorch marks and bullet tracers.
    //!
    //! Everything lives in fixed-size pools that are reused, and each kind is
    //! drawn as one mesh rebuilt every frame (see effects_render in world_render.rs),
    //! so even a big explosion is only a handful of draws.
    use bevy::math::DVec3;

    use crate::rng::{self, random};

    /// One particle: a soft round blob that moves, grows and fades.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Particle {
        pub pos: DVec3,
        pub velocity: DVec3,
        pub age: f64,
        pub life: f64,
        pub size: [f64; 2],
        pub start_color: [f32; 3],
        pub end_color: [f32; 3],
        pub opacity: f64,
        pub rise: f64,
        pub drag: f64,
        pub alive: bool,
    }

    /// What a new particle looks like.
    #[derive(Clone, Copy, Debug)]
    pub struct ParticleOptions {
        pub at: DVec3,
        pub velocity: DVec3,
        pub life: f64,
        pub size: [f64; 2],
        pub color: [f32; 3],
        pub end_color: Option<[f32; 3]>,
        pub opacity: f64,
        pub rise: f64,
        pub drag: f64,
    }

    impl Default for ParticleOptions {
        fn default() -> Self {
            ParticleOptions {
                at: DVec3::ZERO,
                velocity: DVec3::ZERO,
                life: 1.0,
                size: [1.0, 1.0],
                color: [1.0; 3],
                end_color: None,
                opacity: 1.0,
                rise: 0.0,
                drag: 0.0,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct Debris {
        pub pos: DVec3,
        pub rot: DVec3,
        pub scale: DVec3,
        pub color: [f32; 3],
        pub velocity: DVec3,
        pub spin: DVec3,
        pub age: f64,
        pub life: f64,
        pub resting: bool,
        pub alive: bool,
    }

    /// A short animation: fireball, ring, tracer or scorch mark.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Flash {
        pub pos: DVec3,
        pub age: f64,
        pub life: f64,
        pub size: f64,
        pub turn: f64,
        pub from: DVec3,
        pub to: DVec3,
        pub alive: bool,
    }

    const GLOW_COUNT: usize = 300;
    const SMOKE_COUNT: usize = 300;
    const DEBRIS_COUNT: usize = 150;

    pub struct Effects {
        /// 0..1: how many particles each effect makes (the Effects setting).
        pub amount: f64,
        pub glow: Vec<Particle>,
        pub smoke: Vec<Particle>,
        next_glow: usize,
        next_smoke: usize,
        pub debris: Vec<Debris>,
        next_debris: usize,
        pub fireballs: Vec<Flash>,
        pub rings: Vec<Flash>,
        pub tracers: Vec<Flash>,
        pub scorch_marks: Vec<Flash>,
        next_scorch: usize,
        /// The flash of light from explosions: position, color and brightness.
        pub light_pos: DVec3,
        pub light_color: [f32; 3],
        pub light_intensity: f64,
    }

    impl Default for Effects {
        fn default() -> Self {
            Effects {
                amount: 1.0,
                glow: vec![Particle::default(); GLOW_COUNT],
                smoke: vec![Particle::default(); SMOKE_COUNT],
                next_glow: 0,
                next_smoke: 0,
                debris: vec![Debris::default(); DEBRIS_COUNT],
                next_debris: 0,
                fireballs: vec![Flash::default(); 8],
                rings: vec![Flash::default(); 3],
                tracers: vec![Flash::default(); 40],
                scorch_marks: vec![Flash::default(); 30],
                next_scorch: 0,
                light_pos: DVec3::ZERO,
                light_color: rgb(0xffa040),
                light_intensity: 0.0,
            }
        }
    }

    /// An sRGB hex color as 0..1 floats.
    pub fn rgb(hex: u32) -> [f32; 3] {
        [
            ((hex >> 16) & 255) as f32 / 255.0,
            ((hex >> 8) & 255) as f32 / 255.0,
            (hex & 255) as f32 / 255.0,
        ]
    }

    fn spread(range: f64) -> f64 {
        (random() - 0.5) * range
    }

    fn random_direction() -> DVec3 {
        DVec3::new(spread(2.0), spread(2.0), spread(2.0)).normalize_or_zero()
    }

    impl Effects {
        fn count(&self, n: usize) -> usize {
            ((n as f64 * self.amount).round() as usize).max(1)
        }

        pub fn add_glow(&mut self, o: ParticleOptions) {
            start(&mut self.glow[self.next_glow], o);
            self.next_glow = (self.next_glow + 1) % GLOW_COUNT;
        }

        pub fn add_smoke(&mut self, o: ParticleOptions) {
            start(&mut self.smoke[self.next_smoke], o);
            self.next_smoke = (self.next_smoke + 1) % SMOKE_COUNT;
        }

        pub fn add_debris(&mut self, at: DVec3, velocity: DVec3, size: DVec3, color: u32, spin: f64) {
            let d = &mut self.debris[self.next_debris];
            self.next_debris = (self.next_debris + 1) % DEBRIS_COUNT;
            *d = Debris {
                pos: at,
                rot: DVec3::new(random() * 6.0, random() * 6.0, random() * 6.0),
                scale: size,
                color: rgb(color),
                velocity,
                spin: DVec3::new(spread(spin), spread(spin), spread(spin)),
                age: 0.0,
                life: 8.0 + random() * 4.0,
                resting: false,
                alive: true,
            };
        }

        /// A firework: a ball of colored sparks that fall and fade.
        pub fn firework(&mut self, at: DVec3, color: u32, size: f64) {
            for _ in 0..self.count(60) {
                let y = random() * 2.0 - 1.0;
                let r = (1.0 - y * y).sqrt();
                let a = random() * std::f64::consts::TAU;
                let v = DVec3::new(a.cos() * r, y, a.sin() * r) * (14.0 * size * (0.8 + random() * 0.4));
                self.add_glow(ParticleOptions {
                    at,
                    velocity: v,
                    life: 1.2 + random() * 0.5,
                    size: [0.6 * size, 0.1 * size],
                    color: rgb(color),
                    end_color: Some([1.0; 3]),
                    rise: -5.0,
                    drag: 1.2,
                    ..Default::default()
                });
            }
        }

        /// Confetti: little paper squares thrown up around a point.
        pub fn confetti(&mut self, at: DVec3, count: usize) {
            let colors = [0xff3b3b, 0xffd23f, 0x3bb273, 0x1b98e0, 0xc62f9e, 0xffffff];
            for i in 0..self.count(count) {
                let a = random() * std::f64::consts::TAU;
                let s = 2.0 + random() * 5.0;
                self.add_debris(
                    DVec3::new(at.x + spread(1.0), at.y + 1.0, at.z + spread(1.0)),
                    DVec3::new(a.cos() * s, 6.0 + random() * 8.0, a.sin() * s),
                    DVec3::new(0.18, 0.02, 0.12),
                    colors[i % colors.len()],
                    14.0,
                );
            }
        }

        /// A burst of debris flying out from a point, in the given colors.
        pub fn shatter(&mut self, center: DVec3, count: usize, colors: &[u32], size: f64, speed: f64) {
            for _ in 0..self.count(count) {
                let angle = random() * std::f64::consts::TAU;
                let s = speed * (0.4 + random() * 0.8);
                let k = size * (0.4 + random() * 0.9);
                self.add_debris(
                    DVec3::new(center.x + spread(size), center.y + random() * size, center.z + spread(size)),
                    DVec3::new(angle.cos() * s, 2.0 + random() * 6.0, angle.sin() * s),
                    DVec3::new(k, k * (0.4 + random() * 0.7), k * (0.6 + random() * 0.6)),
                    rng::pick(colors),
                    8.0,
                );
            }
        }

        /// A fiery explosion. `size` is roughly its radius in meters.
        pub fn explosion(&mut self, at: DVec3, size: f64) {
            play(&mut self.fireballs, at, size * 0.8, 0.55);
            self.light_color = rgb(0xffa040);
            self.light_pos = DVec3::new(at.x, at.y + 2.0, at.z);
            self.light_intensity = self.light_intensity.max(40.0 * size);
            for _ in 0..self.count(16) {
                let mut v = random_direction() * (size * (3.0 + random() * 4.0));
                v.y = 2.0 + random() * size * 4.0;
                self.add_glow(ParticleOptions {
                    at,
                    velocity: v,
                    life: 0.35 + random() * 0.4,
                    size: [size * 1.6, size * 0.3],
                    color: rgb(0xffd966),
                    end_color: Some(rgb(0xff4010)),
                    drag: 3.5,
                    rise: 2.0,
                    ..Default::default()
                });
            }
            for _ in 0..self.count(12) {
                let mut v = random_direction() * (18.0 + random() * 18.0);
                v.y = 4.0 + random() * 12.0;
                self.add_glow(ParticleOptions {
                    at,
                    velocity: v,
                    life: 0.6 + random() * 0.6,
                    size: [0.3, 0.1],
                    color: rgb(0xffe6a0),
                    end_color: Some(rgb(0xff6020)),
                    rise: -14.0,
                    drag: 0.6,
                    ..Default::default()
                });
            }
            for _ in 0..self.count(14) {
                let mut v = random_direction() * (size * 1.5);
                v.y = 1.5 + random() * 2.5;
                self.add_smoke(ParticleOptions {
                    at: at + random_direction() * size,
                    velocity: v,
                    life: 2.5 + random() * 2.5,
                    size: [size * 2.0, size * 5.0],
                    color: rgb(0x303030),
                    end_color: Some(rgb(0x606060)),
                    opacity: 0.75,
                    rise: 0.6,
                    drag: 0.8,
                });
            }
            self.scorch(at.x, at.z, size * 2.6);
        }

        /// A flat blue ring rushing outwards, for the shockwave weapon.
        pub fn shockwave(&mut self, at: DVec3, radius: f64) {
            play(&mut self.rings, DVec3::new(at.x, 0.3, at.z), radius, 0.6);
            self.light_color = rgb(0x9fdcff);
            self.light_pos = DVec3::new(at.x, 2.0, at.z);
            self.light_intensity = 120.0;
            self.dust(at.x, at.z, 1.5, 1.5, 40, 0.5);
        }

        /// Sparks (and a puff of dust on walls and the road) where a bullet hits.
        pub fn bullet_hit(&mut self, at: DVec3, dusty: bool) {
            for _ in 0..self.count(5) {
                let mut v = random_direction() * (5.0 + random() * 8.0);
                v.y = 2.0 + random() * 5.0;
                self.add_glow(ParticleOptions {
                    at,
                    velocity: v,
                    life: 0.15 + random() * 0.25,
                    size: [0.2, 0.05],
                    color: rgb(0xffd98a),
                    end_color: Some(rgb(0xff6a1a)),
                    rise: -12.0,
                    ..Default::default()
                });
            }
            if dusty {
                self.add_smoke(ParticleOptions {
                    at,
                    velocity: DVec3::new(0.0, 0.8, 0.0),
                    life: 1.0,
                    size: [0.4, 1.6],
                    color: rgb(0x9a948a),
                    opacity: 0.6,
                    drag: 1.5,
                    ..Default::default()
                });
            }
        }

        /// A quick flash at the end of a gun barrel.
        pub fn muzzle_flash(&mut self, at: DVec3, direction: DVec3) {
            self.add_glow(ParticleOptions {
                at,
                velocity: direction * 4.0,
                life: 0.07,
                size: [0.5, 0.15],
                color: rgb(0xffd98a),
                ..Default::default()
            });
        }

        /// Fire and smoke rising from something burning. Call every frame.
        pub fn burn(&mut self, at: DVec3, strength: f64, dt: f64) {
            let per_second = 20.0 * strength * self.amount;
            if random() > per_second * dt {
                return;
            }
            let spot = DVec3::new(at.x + spread(strength), at.y, at.z + spread(strength));
            self.add_glow(ParticleOptions {
                at: spot,
                velocity: DVec3::new(0.0, 2.0 + random() * 2.0, 0.0),
                life: 0.5,
                size: [strength, 0.2],
                color: rgb(0xffb340),
                end_color: Some(rgb(0xff3005)),
                drag: 1.0,
                ..Default::default()
            });
            self.add_smoke(ParticleOptions {
                at: DVec3::new(spot.x, at.y + 0.6, spot.z),
                velocity: DVec3::new(0.6, 2.0 + random() * 1.5, 0.3),
                life: 3.0 + random() * 2.0,
                size: [strength, 4.0 * strength],
                color: rgb(0x202020),
                end_color: Some(rgb(0x505052)),
                opacity: 0.6,
                drag: 0.4,
                ..Default::default()
            });
        }

        /// Dust billowing out from the edges of a rectangle on the ground.
        pub fn dust(&mut self, x: f64, z: f64, half_width: f64, half_depth: f64, count: usize, scale: f64) {
            for _ in 0..self.count(count) {
                let along = random() * 2.0 - 1.0;
                let sides = [
                    (x + along * half_width, z + half_depth, 0.0, 1.0),
                    (x + along * half_width, z - half_depth, 0.0, -1.0),
                    (x + half_width, z + along * half_depth, 1.0, 0.0),
                    (x - half_width, z + along * half_depth, -1.0, 0.0),
                ];
                let (sx, sz, ox, oz) = sides[rng::index(4)];
                let speed = (3.0 + random() * 5.0) * scale;
                self.add_smoke(ParticleOptions {
                    at: DVec3::new(sx + ox, 0.5 + random() * 2.0 * scale, sz + oz),
                    velocity: DVec3::new(ox * speed + spread(2.0), 0.5 + random() * 2.0, oz * speed + spread(2.0)),
                    life: 3.0 + random() * 3.0,
                    size: [3.0 * scale, 9.0 * scale],
                    color: rgb(0x8c857a),
                    opacity: 0.8,
                    drag: 0.9,
                    ..Default::default()
                });
            }
        }

        /// A glowing streak flying from the gun to where the bullet hit.
        pub fn tracer(&mut self, from: DVec3, to: DVec3) {
            let i = self.tracers.iter().position(|t| !t.alive).unwrap_or(0);
            self.tracers[i] = Flash {
                from,
                to,
                age: 0.0,
                life: (from.distance(to) / 400.0).max(0.04),
                alive: true,
                ..Default::default()
            };
        }

        /// A dark burn mark on the ground.
        pub fn scorch(&mut self, x: f64, z: f64, size: f64) {
            let s = &mut self.scorch_marks[self.next_scorch];
            self.next_scorch = (self.next_scorch + 1) % 30;
            *s = Flash {
                pos: DVec3::new(x, 0.045, z),
                size,
                turn: random() * std::f64::consts::PI,
                age: 0.0,
                life: 35.0,
                alive: true,
                ..Default::default()
            };
        }

        /// Moves everything on by `dt` seconds.
        pub fn update(&mut self, dt: f64) {
            for p in self.glow.iter_mut().chain(self.smoke.iter_mut()) {
                if !p.alive {
                    continue;
                }
                p.age += dt;
                if p.age >= p.life {
                    p.alive = false;
                    continue;
                }
                p.velocity *= (1.0 - p.drag * dt).max(0.0);
                p.velocity.y += p.rise * dt;
                p.pos += p.velocity * dt;
                p.pos.y = p.pos.y.max(0.05);
            }
            for d in &mut self.debris {
                if !d.alive {
                    continue;
                }
                d.age += dt;
                if d.age >= d.life {
                    d.alive = false;
                    continue;
                }
                if !d.resting {
                    d.velocity.y -= 16.0 * dt;
                    d.pos += d.velocity * dt;
                    d.rot += d.spin * dt;
                    let half_height = d.scale.min_element() / 2.0;
                    if d.pos.y < half_height {
                        d.pos.y = half_height;
                        d.velocity = DVec3::new(d.velocity.x * 0.5, d.velocity.y.abs() * 0.3, d.velocity.z * 0.5);
                        d.spin *= 0.5;
                        if d.velocity.length() < 1.0 {
                            d.resting = true;
                            d.rot = DVec3::new(0.0, d.rot.y, 0.0);
                            d.pos.y = d.scale.y / 2.0;
                        }
                    }
                }
                let left = d.life - d.age;
                if left < 1.0 {
                    d.scale *= (1.0 - dt / left).max(0.0);
                }
            }
            for f in self
                .fireballs
                .iter_mut()
                .chain(self.rings.iter_mut())
                .chain(self.scorch_marks.iter_mut())
                .chain(self.tracers.iter_mut())
            {
                if f.alive {
                    f.age += dt;
                    if f.age >= f.life {
                        f.alive = false;
                    }
                }
            }
            self.light_intensity *= (-dt * 9.0).exp();
            if self.light_intensity < 0.5 {
                self.light_intensity = 0.0;
            }
        }

        /// Removes every effect at once.
        pub fn clear(&mut self) {
            for p in self.glow.iter_mut().chain(self.smoke.iter_mut()) {
                p.alive = false;
            }
            for d in &mut self.debris {
                d.alive = false;
            }
            for f in self
                .fireballs
                .iter_mut()
                .chain(self.rings.iter_mut())
                .chain(self.tracers.iter_mut())
                .chain(self.scorch_marks.iter_mut())
            {
                f.alive = false;
            }
        }
    }

    fn start(p: &mut Particle, o: ParticleOptions) {
        *p = Particle {
            pos: o.at,
            velocity: o.velocity,
            age: 0.0,
            life: o.life,
            size: o.size,
            start_color: o.color,
            end_color: o.end_color.unwrap_or(o.color),
            opacity: o.opacity,
            rise: o.rise,
            drag: o.drag,
            alive: true,
        };
    }

    fn play(pool: &mut [Flash], at: DVec3, size: f64, life: f64) {
        let i = pool.iter().position(|f| !f.alive).unwrap_or(0);
        pool[i] = Flash {
            pos: at,
            size,
            age: 0.0,
            life,
            alive: true,
            ..Default::default()
        };
    }
}

pub mod canvas {
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
}

pub mod textures {
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
                let tongue = |c: &mut Canvas, y: f32, reach: f32, thick: f32, color: Rgba| {
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
}

pub mod meshes {
    //! Helpers for building meshes from lists of points.
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
    use bevy::prelude::*;

    /// Positions, texture coordinates and triangles, built up and then turned into a mesh.
    #[derive(Default, Clone)]
    pub struct MeshData {
        pub positions: Vec<[f32; 3]>,
        pub uvs: Vec<[f32; 2]>,
        pub normals: Vec<[f32; 3]>,
        pub colors: Vec<[f32; 4]>,
        pub indices: Vec<u32>,
    }

    impl MeshData {
        pub fn vertex(&mut self, p: [f64; 3], uv: [f64; 2]) -> u32 {
            self.positions.push([p[0] as f32, p[1] as f32, p[2] as f32]);
            self.uvs.push([uv[0] as f32, uv[1] as f32]);
            self.positions.len() as u32 - 1
        }

        pub fn tri(&mut self, a: u32, b: u32, c: u32) {
            self.indices.extend([a, b, c]);
        }

        pub fn is_empty(&self) -> bool {
            self.indices.is_empty()
        }

        /// Adds another mesh's triangles.
        pub fn append(&mut self, other: &MeshData) {
            let base = self.positions.len() as u32;
            self.positions.extend_from_slice(&other.positions);
            self.uvs.extend_from_slice(&other.uvs);
            self.normals.extend_from_slice(&other.normals);
            if !self.colors.is_empty() || !other.colors.is_empty() {
                self.colors.resize(base as usize, [1.0; 4]);
                self.colors.extend_from_slice(&other.colors);
                self.colors.resize(self.positions.len(), [1.0; 4]);
            }
            self.indices.extend(other.indices.iter().map(|i| i + base));
        }

        /// Smooth normals from the triangles (like three.js computeVertexNormals).
        pub fn compute_normals(&mut self) {
            let mut normals = vec![Vec3::ZERO; self.positions.len()];
            for t in self.indices.chunks(3) {
                let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
                let (pa, pb, pc) = (
                    Vec3::from(self.positions[a]),
                    Vec3::from(self.positions[b]),
                    Vec3::from(self.positions[c]),
                );
                let n = (pb - pa).cross(pc - pa);
                normals[a] += n;
                normals[b] += n;
                normals[c] += n;
            }
            self.normals = normals.into_iter().map(|n| n.normalize_or(Vec3::Y).to_array()).collect();
        }

        /// Adds the back of every triangle too, for strips seen from both sides.
        pub fn double_sided(&mut self) {
            if self.normals.len() != self.positions.len() {
                self.compute_normals();
            }
            let count = self.positions.len() as u32;
            let positions = self.positions.clone();
            self.positions.extend(positions);
            self.uvs.extend(self.uvs.clone());
            let flipped: Vec<[f32; 3]> = self.normals.iter().map(|n| [-n[0], -n[1], -n[2]]).collect();
            self.normals.extend(flipped);
            if !self.colors.is_empty() {
                self.colors.extend(self.colors.clone());
            }
            let back: Vec<u32> = self
                .indices
                .chunks(3)
                .flat_map(|t| [t[0] + count, t[2] + count, t[1] + count])
                .collect();
            self.indices.extend(back);
        }

        /// The finished mesh, kept only on the graphics card.
        pub fn build(self) -> Mesh {
            self.build_with(RenderAssetUsages::RENDER_WORLD)
        }

        /// The finished mesh, also kept in the game's memory so it can be changed later.
        pub fn build_editable(self) -> Mesh {
            self.build_with(RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
        }

        fn build_with(mut self, usage: RenderAssetUsages) -> Mesh {
            if self.normals.len() != self.positions.len() {
                self.compute_normals();
            }
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, usage);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
            if !self.colors.is_empty() {
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
            }
            mesh.insert_indices(Indices::U32(self.indices));
            mesh
        }
    }

    impl MeshData {
        /// A Bevy mesh's triangles, moved by `transform`, in one color (linear RGBA).
        pub fn from_mesh(mesh: &Mesh, transform: &Transform, color: [f32; 4]) -> MeshData {
            let mut out = MeshData::default();
            out.add_mesh(mesh, transform, color);
            out
        }

        /// Adds a Bevy mesh's triangles, moved by `transform`, in one color (linear RGBA).
        pub fn add_mesh(&mut self, mesh: &Mesh, transform: &Transform, color: [f32; 4]) {
            use bevy::mesh::VertexAttributeValues as V;
            let base = self.positions.len() as u32;
            let matrix = transform.to_matrix();
            let normal_matrix = bevy::math::Mat3::from_mat4(matrix).inverse().transpose();
            let Some(V::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                return;
            };
            let count = positions.len();
            for p in positions {
                self.positions.push(matrix.transform_point3(Vec3::from(*p)).to_array());
            }
            match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
                Some(V::Float32x3(normals)) => {
                    for n in normals {
                        self.normals.push((normal_matrix * Vec3::from(*n)).normalize_or(Vec3::Y).to_array());
                    }
                }
                _ => self.normals.extend(std::iter::repeat_n([0.0, 1.0, 0.0], count)),
            }
            match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
                Some(V::Float32x2(uvs)) => self.uvs.extend_from_slice(uvs),
                _ => self.uvs.extend(std::iter::repeat_n([0.0, 0.0], count)),
            }
            // Keep the color list the same length as the positions once any mesh has colors.
            if self.colors.len() < base as usize {
                self.colors.resize(base as usize, [1.0; 4]);
            }
            self.colors.extend(std::iter::repeat_n(color, count));
            match mesh.indices() {
                Some(indices) => self.indices.extend(indices.iter().map(|i| i as u32 + base)),
                None => self.indices.extend((0..count as u32).map(|i| i + base)),
            }
        }

        /// Gives every triangle its own corners, with the triangle's normal: a faceted look.
        pub fn flat_shaded(self) -> MeshData {
            let mut out = MeshData::default();
            let has_colors = self.colors.len() == self.positions.len();
            for t in self.indices.chunks(3) {
                let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from(self.positions[i as usize])).collect();
                let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y).to_array();
                for &i in t {
                    out.positions.push(self.positions[i as usize]);
                    out.normals.push(n);
                    out.uvs.push(self.uvs.get(i as usize).copied().unwrap_or([0.0, 0.0]));
                    if has_colors {
                        out.colors.push(self.colors[i as usize]);
                    }
                }
                let k = out.positions.len() as u32;
                out.indices.extend([k - 3, k - 2, k - 1]);
            }
            out
        }
    }

    /// An sRGB color (as CSS writes it) in the linear values meshes use.
    pub fn linear(rgb: u32) -> [f32; 4] {
        let c = Color::srgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8).to_linear();
        [c.red, c.green, c.blue, 1.0]
    }

    /// HSL (0..1) in the linear values meshes use.
    pub fn linear_hsl(h: f64, s: f64, l: f64) -> [f32; 4] {
        let c = crate::canvas::hsl(h as f32, s as f32, l as f32);
        let lin = Color::srgb(c.0, c.1, c.2).to_linear();
        [lin.red, lin.green, lin.blue, 1.0]
    }

    /// A cylinder whose top and bottom can differ in size (like three.js CylinderGeometry), centered on the origin.
    pub fn tapered_cylinder(top: f32, bottom: f32, height: f32, segments: u32) -> Mesh {
        bevy::shape::ConicalFrustum {
            radius_top: top,
            radius_bottom: bottom,
            height,
        }
        .mesh()
        .resolution(segments)
        .build()
    }

    /// A shape drawn side-on (x across, y up) and pushed out `depth` along z, from z = 0 to z = depth.
    pub fn extrude(shape: &[(f32, f32)], depth: f32) -> MeshData {
        // Work counter-clockwise, so the sides face out.
        let area: f32 = (0..shape.len())
            .map(|i| shape[i].0 * shape[(i + 1) % shape.len()].1 - shape[(i + 1) % shape.len()].0 * shape[i].1)
            .sum();
        let owned: Vec<(f32, f32)> = if area < 0.0 {
            shape.iter().rev().copied().collect()
        } else {
            shape.to_vec()
        };
        let shape = &owned[..];
        let mut m = MeshData::default();
        let n = shape.len();
        // The sides: one flat quad per edge.
        for i in 0..n {
            let (a, b) = (shape[i], shape[(i + 1) % n]);
            let edge = Vec2::new(b.0 - a.0, b.1 - a.1);
            // Outward for a counter-clockwise shape.
            let normal = Vec3::new(edge.y, -edge.x, 0.0).normalize_or(Vec3::Y);
            let base = m.positions.len() as u32;
            for (p, z) in [(a, 0.0), (b, 0.0), (b, depth), (a, depth)] {
                m.positions.push([p.0, p.1, z]);
                m.normals.push(normal.to_array());
                m.uvs.push([z, p.1]);
            }
            m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        // The two ends: the shape cut into triangles.
        let tris = triangulate(shape);
        for (z, normal, flip) in [(0.0, -1.0, true), (depth, 1.0, false)] {
            let base = m.positions.len() as u32;
            for p in shape.iter() {
                m.positions.push([p.0, p.1, z]);
                m.normals.push([0.0, 0.0, normal]);
                m.uvs.push([p.0, p.1]);
            }
            for t in &tris {
                if flip {
                    m.indices.extend([base + t[0], base + t[2], base + t[1]]);
                } else {
                    m.indices.extend([base + t[0], base + t[1], base + t[2]]);
                }
            }
        }
        m
    }

    /// Cuts a simple polygon into triangles by ear clipping; they come back counter-clockwise.
    pub fn triangulate(shape: &[(f32, f32)]) -> Vec<[u32; 3]> {
        let mut idx: Vec<u32> = (0..shape.len() as u32).collect();
        let area: f32 = (0..shape.len())
            .map(|i| {
                let (a, b) = (shape[i], shape[(i + 1) % shape.len()]);
                a.0 * b.1 - b.0 * a.1
            })
            .sum();
        if area < 0.0 {
            idx.reverse();
        }
        let mut out = Vec::new();
        let p = |i: u32| Vec2::new(shape[i as usize].0, shape[i as usize].1);
        let mut guard = 0;
        while idx.len() > 3 && guard < 10_000 {
            guard += 1;
            let n = idx.len();
            let mut clipped = false;
            for k in 0..n {
                let (a, b, c) = (idx[(k + n - 1) % n], idx[k], idx[(k + 1) % n]);
                let (pa, pb, pc) = (p(a), p(b), p(c));
                let turn = (pb - pa).perp_dot(pc - pb);
                if turn.abs() < 1e-7 {
                    // A point on a straight edge (or a repeat): drop it, no triangle needed.
                    idx.remove(k);
                    clipped = true;
                    break;
                }
                if turn < 0.0 {
                    continue;
                }
                let inside = idx.iter().any(|&o| {
                    if o == a || o == b || o == c {
                        return false;
                    }
                    let q = p(o);
                    (pb - pa).perp_dot(q - pa) > 0.0 && (pc - pb).perp_dot(q - pb) > 0.0 && (pa - pc).perp_dot(q - pc) > 0.0
                });
                if inside {
                    continue;
                }
                out.push([a, b, c]);
                idx.remove(k);
                clipped = true;
                break;
            }
            if !clipped {
                break;
            }
        }
        if idx.len() == 3 {
            out.push([idx[0], idx[1], idx[2]]);
        }
        out
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn appending_colored_and_uncolored_meshes_preserves_vertex_alignment() {
            use super::{MeshData, Transform};
            use bevy::prelude::Cuboid;

            let cube = bevy::prelude::Mesh::from(Cuboid::from_length(1.0));
            let red = [1.0, 0.0, 0.0, 1.0];
            let colored = MeshData::from_mesh(&cube, &Transform::default(), red);
            let mut plain = colored.clone();
            plain.colors.clear();

            let mut combined = colored.clone();
            combined.append(&plain);
            combined.append(&colored);

            let count = colored.positions.len();
            assert_eq!(combined.colors.len(), count * 3);
            assert!(combined.colors[..count].iter().all(|color| *color == red));
            assert!(combined.colors[count..count * 2].iter().all(|color| *color == [1.0; 4]));
            assert!(combined.colors[count * 2..].iter().all(|color| *color == red));
        }

        #[test]
        fn triangulates_concave_shapes_with_straight_runs() {
            // An L shape with extra points along its edges.
            let shape = [
                (0.0, 0.0),
                (1.0, 0.0),
                (2.0, 0.0),
                (2.0, 1.0),
                (1.0, 1.0),
                (1.0, 2.0),
                (0.0, 2.0),
                (0.0, 1.0),
            ];
            let tris = super::triangulate(&shape);
            let area: f32 = tris
                .iter()
                .map(|t| {
                    let p = |i: u32| shape[i as usize];
                    let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                    ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)) / 2.0
                })
                .sum();
            assert!((area - 3.0).abs() < 1e-5, "covered {area}");
        }
    }
}

pub mod quality {
    //! Picks graphics settings that suit this computer on the first start:
    //! Low on small or old machines (the target is a steady 60 frames a second
    //! at 720p on an Intel UHD 620 laptop with 4 GB of memory), up to High on
    //! big ones. The player can change it any time in Garage → Settings.
    use bevy::prelude::*;
    use bevy::render::renderer::RenderAdapterInfo;

    use crate::game::Game;

    /// A first guess from the memory and processor.
    pub fn pick_preset() -> &'static str {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let gb = sys.total_memory() as f64 / 1024f64.powi(3);
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        if gb > 0.0 && gb < 6.0 || cores <= 2 {
            "Low"
        } else if gb < 12.0 || cores <= 4 {
            "Medium"
        } else {
            "High"
        }
    }

    /// Once the graphics chip is known: integrated graphics get at most Medium,
    /// and software drawing gets Low.
    pub fn refine(
        adapter: Option<Res<RenderAdapterInfo>>,
        mut game: ResMut<Game>,
        mut done: Local<bool>,
        mut storage: ResMut<crate::storage::Storage>,
    ) {
        if *done {
            return;
        }
        let Some(adapter) = adapter else { return };
        *done = true;
        if !game.first_start {
            return;
        }
        let kind = format!("{:?}", adapter.device_type);
        let cap = match kind.as_str() {
            "Cpu" | "VirtualGpu" => "Low",
            "IntegratedGpu" => {
                if game.settings.quality == "Low" {
                    "Low"
                } else {
                    "Medium"
                }
            }
            _ => return,
        };
        let order = ["Low", "Medium", "High", "Ultra"];
        let now = order.iter().position(|p| *p == game.settings.quality).unwrap_or(2);
        let limit = order.iter().position(|p| *p == cap).unwrap_or(0);
        if now > limit && storage.get("carGame.imported").is_none() {
            game.settings.apply_preset(cap);
            let s = game.settings.clone();
            crate::settings::save_settings(&mut storage, &s);
            if let Some(p) = game.popups.last_mut() {
                p.title = format!("Graphics: {cap}");
            }
        }
    }
}
