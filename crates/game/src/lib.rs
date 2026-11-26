//! The game itself: driving, the garage, the worlds, events and the career.
//!
//! Stage 0 of the 1.3.0 rebuild: a test scene that proves the engine runs on
//! every system. The real game arrives stage by stage.

use bevy::prelude::*;

pub mod dev;
pub mod packs;

/// The game's version, from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything in the game. Packs add their own plugins next to it.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(dev::DevPlugin)
            .init_resource::<packs::Packs>()
            .insert_resource(ClearColor(Color::srgb(0.55, 0.75, 0.95)))
            .add_systems(Startup, test_scene)
            .add_systems(Update, spin_test_car);
    }
}

/// Marks the stand-in car in the test scene.
#[derive(Component)]
struct TestCar;

fn test_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    packs: Res<packs::Packs>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-6.0, 3.5, 8.0).looking_at(Vec3::new(0.0, 0.6, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(8.0, 14.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // The ground.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(60.0, 60.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.32, 0.55, 0.3))),
    ));
    // A stand-in car: a red body and four black wheels.
    commands
        .spawn((TestCar, Transform::from_xyz(0.0, 0.0, 0.0), Visibility::default()))
        .with_children(|car| {
            car.spawn((
                Mesh3d(meshes.add(Cuboid::new(1.9, 0.7, 4.2))),
                MeshMaterial3d(materials.add(Color::srgb(0.84, 0.15, 0.24))),
                Transform::from_xyz(0.0, 0.75, 0.0),
            ));
            let wheel = meshes.add(Cylinder::new(0.38, 0.3));
            let rubber = materials.add(Color::srgb(0.08, 0.08, 0.08));
            for (x, z) in [(-1.0, 1.35), (1.0, 1.35), (-1.0, -1.35), (1.0, -1.35)] {
                car.spawn((
                    Mesh3d(wheel.clone()),
                    MeshMaterial3d(rubber.clone()),
                    Transform::from_xyz(x, 0.38, z).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
            }
        });
    let names: Vec<&str> = packs.list.iter().map(|p| p.name).collect();
    commands.spawn((
        Text::new(format!(
            "Car Game {VERSION}\nRebuild in progress: stage 0\nPacks: {}",
            names.join(", ")
        )),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
}

fn spin_test_car(time: Res<Time>, mut cars: Query<&mut Transform, With<TestCar>>) {
    for mut t in &mut cars {
        t.rotate_y(time.delta_secs() * 0.6);
    }
}
