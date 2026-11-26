//! Shared geometry for the residents, shops and cars.
use std::collections::HashMap;

use bevy::prelude::*;
use game::camera::CameraRig;
use game::world::World;

use crate::cast::{CastMember, Extra, HairStyle, Hat};
use crate::city::City;

#[derive(Component)]
pub struct CityRoot;

#[derive(Component)]
pub struct Body {
    index: usize,
    arms: [Entity; 2],
    legs: [Entity; 2],
}

#[derive(Component)]
pub struct CityCar(pub Option<usize>);

#[derive(Component)]
pub struct Finder;

#[derive(Resource)]
pub struct Visuals {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    colors: HashMap<u32, Handle<StandardMaterial>>,
}

impl Visuals {
    pub fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            cube: meshes.add(Cuboid::from_length(1.0)),
            sphere: meshes.add(Sphere::new(0.5).mesh().uv(12, 8)),
            cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(12)),
            colors: HashMap::new(),
        }
    }

    fn color(&mut self, materials: &mut Assets<StandardMaterial>, color: u32) -> Handle<StandardMaterial> {
        self.colors
            .entry(color)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::srgb_u8((color >> 16) as u8, (color >> 8) as u8, color as u8),
                    perceptual_roughness: 0.84,
                    ..default()
                })
            })
            .clone()
    }
}

fn part(
    commands: &mut Commands,
    parent: Entity,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    pos: [f32; 3],
    size: [f32; 3],
) -> Entity {
    commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(Vec3::from_array(pos)).with_scale(Vec3::from_array(size)),
            ChildOf(parent),
        ))
        .id()
}

pub fn spawn_resident(
    commands: &mut Commands,
    visuals: &mut Visuals,
    materials: &mut Assets<StandardMaterial>,
    member: &CastMember,
    index: usize,
) -> Entity {
    let look = member.look;
    let skin = visuals.color(materials, look.skin);
    let shirt = visuals.color(materials, look.shirt);
    let hair = visuals.color(materials, look.hair);
    let trousers = visuals.color(materials, look.trousers);
    let shoes = visuals.color(materials, look.shoes);
    let black = visuals.color(materials, 0x211f23);
    let root = commands.spawn((CityRoot, Transform::default(), Visibility::Hidden)).id();
    let width = look.build as f32;
    let cube = &visuals.cube;
    let sphere = &visuals.sphere;
    part(commands, root, cube, &shirt, [0.0, 1.05, 0.0], [0.39 * width, 0.58, 0.24]);
    part(commands, root, sphere, &skin, [0.0, 1.48, 0.0], [0.32, 0.39, 0.31]);
    if look.hair_style != HairStyle::Bald {
        let size = match look.hair_style {
            HairStyle::Long => [0.36, 0.32, 0.27],
            HairStyle::Curly => [0.38, 0.21, 0.35],
            HairStyle::Spiky => [0.32, 0.25, 0.28],
            _ => [0.34, 0.16, 0.34],
        };
        part(commands, root, sphere, &hair, [0.0, 1.62, -0.025], size);
        if look.hair_style == HairStyle::Bun {
            part(commands, root, sphere, &hair, [0.0, 1.63, -0.17], [0.14; 3]);
        }
    }
    for x in [-0.07, 0.07] {
        let size = if look.glasses { [0.12, 0.052, 0.035] } else { [0.024, 0.03, 0.022] };
        part(commands, root, cube, &black, [x, 1.51, 0.14], size);
    }
    let hat_color = match look.hat {
        Hat::None => None,
        Hat::Police => Some(0x182d49),
        Hat::Beanie => Some(0xa43255),
        Hat::Cap => Some(look.shirt),
    };
    if let Some(color) = hat_color {
        let mat = visuals.color(materials, color);
        part(commands, root, &visuals.cylinder, &mat, [0.0, 1.65, 0.0], [0.36, 0.12, 0.35]);
        if look.hat != Hat::Beanie {
            part(commands, root, &visuals.cube, &mat, [0.0, 1.61, 0.15], [0.28, 0.035, 0.22]);
        }
    }
    let accessory = match look.extra {
        Extra::None => None,
        Extra::Badge => Some((0xffcf65, [0.065, 0.078, 0.02], [0.11, 1.17, 0.13])),
        Extra::Apron => Some((0xf5e7d4, [0.29, 0.48, 0.02], [0.0, 0.99, 0.13])),
        Extra::Tie => Some((0x8c284c, [0.065, 0.32, 0.02], [0.0, 1.14, 0.13])),
        Extra::Headband => Some((0xdd3548, [0.33, 0.05, 0.06], [0.0, 1.6, 0.11])),
        Extra::Guitar => Some((0xbc783b, [0.25, 0.54, 0.12], [0.32, 0.95, -0.21])),
        Extra::Skateboard => Some((0x7ca759, [0.13, 0.62, 0.06], [0.33, 0.67, 0.09])),
        Extra::Camera => Some((0x252529, [0.16, 0.12, 0.09], [0.19, 0.94, 0.16])),
        Extra::Wrench | Extra::Spatula => Some((0xadb5bd, [0.062, 0.25, 0.04], [0.32, 0.75, 0.04])),
        Extra::Newspaper => Some((0xf5efdd, [0.12, 0.23, 0.04], [0.33, 0.74, 0.05])),
        Extra::Paint => Some((0xf5bc40, [0.10, 0.26, 0.04], [0.33, 0.75, 0.07])),
    };
    if let Some((color, size, pos)) = accessory {
        let mat = visuals.color(materials, color);
        part(commands, root, &visuals.cube, &mat, pos, size);
    }
    let arms = [-1.0, 1.0].map(|side| {
        let hinge = commands
            .spawn((
                Transform::from_xyz(side * (0.19 * width + 0.07), 1.25, 0.0),
                Visibility::Inherited,
                ChildOf(root),
            ))
            .id();
        part(commands, hinge, &visuals.cube, &shirt, [0.0, -0.12, 0.0], [0.13, 0.25, 0.14]);
        part(commands, hinge, &visuals.cube, &skin, [0.0, -0.32, 0.0], [0.105, 0.18, 0.105]);
        hinge
    });
    let legs = [-1.0, 1.0].map(|side| {
        let hinge = commands
            .spawn((
                Transform::from_xyz(side * 0.106 * width, 0.83, 0.0),
                Visibility::Inherited,
                ChildOf(root),
            ))
            .id();
        part(
            commands,
            hinge,
            &visuals.cube,
            &trousers,
            [0.0, -0.34, 0.0],
            [0.16 * width, 0.68, 0.17],
        );
        part(
            commands,
            hinge,
            &visuals.cube,
            &shoes,
            [0.0, -0.765, 0.035],
            [0.17 * width, 0.13, 0.25],
        );
        hinge
    });
    commands.entity(root).insert(Body { index, arms, legs });
    root
}

pub fn show_residents(
    city: Res<City>,
    world: Res<World>,
    camera: Res<CameraRig>,
    mut transforms: Query<&mut Transform>,
    mut bodies: Query<(Entity, &Body, &mut Visibility)>,
) {
    for (entity, body, mut visibility) in &mut bodies {
        let Some(n) = city.residents.get(body.index) else { continue };
        let ground = world.height_at(n.at.x, n.at.y) as f32;
        let at = Vec3::new(n.at.x as f32, ground, n.at.y as f32);
        let show = city.active && !n.asleep && !n.in_car && at.distance_squared(camera.position) < 150.0_f32.powi(2);
        *visibility = if show { Visibility::Inherited } else { Visibility::Hidden };
        if !show {
            continue;
        }
        let phase = (city.clock * n.speed * 3.0) as f32;
        let stride = phase.sin() * (n.speed as f32 * 0.28).min(1.15);
        if let Ok(mut t) = transforms.get_mut(entity) {
            t.translation = at + Vec3::Y * stride.abs() * 0.035;
            t.rotation = Quat::from_rotation_y(n.heading as f32);
            t.scale = Vec3::splat((n.member.look.height / 1.7) as f32);
        }
        for i in 0..2 {
            let sign = if i == 0 { 1.0 } else { -1.0 };
            if let Ok(mut arm) = transforms.get_mut(body.arms[i]) {
                arm.rotation = Quat::from_rotation_x(stride * sign);
            }
            if let Ok(mut leg) = transforms.get_mut(body.legs[i]) {
                leg.rotation = Quat::from_rotation_x(-stride * sign);
            }
        }
    }
}

pub fn spawn_place(commands: &mut Commands, visuals: &mut Visuals, materials: &mut Assets<StandardMaterial>, at: Vec3, kind: &str) {
    let color = match kind {
        "coffee" => 0xbb5838,
        "bank" => 0x43836d,
        "diner" => 0xb1344f,
        "gym" => 0x376e9e,
        "park" => 0x397444,
        "stunts" => 0xb78932,
        "garage" => 0x687581,
        "music" => 0x915ec3,
        _ => 0x3d8097,
    };
    let paint = visuals.color(materials, color);
    let wood = visuals.color(materials, 0xb99461);
    let pole = visuals.color(materials, 0x545456);
    let root = commands
        .spawn((CityRoot, Transform::from_translation(at), Visibility::Inherited))
        .id();
    if kind == "park" {
        part(commands, root, &visuals.cube, &wood, [0.0, 0.48, 0.0], [2.1, 0.12, 0.62]);
        part(commands, root, &visuals.cube, &wood, [0.0, 0.85, -0.27], [2.1, 0.38, 0.10]);
        for x in [-0.8, 0.8] {
            part(commands, root, &visuals.cube, &pole, [x, 0.23, 0.0], [0.13, 0.46, 0.53]);
        }
    } else {
        part(commands, root, &visuals.cube, &pole, [0.0, 0.91, 0.0], [0.13, 1.82, 0.13]);
        part(commands, root, &visuals.cube, &paint, [0.0, 2.03, 0.0], [1.35, 0.53, 0.14]);
        if ["coffee", "market", "news", "bakery"].contains(&kind) {
            part(commands, root, &visuals.cube, &wood, [0.0, 0.49, 0.0], [1.4, 0.98, 0.74]);
            part(commands, root, &visuals.cube, &paint, [0.0, 1.1, 0.0], [1.5, 0.17, 0.84]);
        }
    }
}

pub fn spawn_car(commands: &mut Commands, visuals: &mut Visuals, materials: &mut Assets<StandardMaterial>, driver: Option<usize>) {
    let paint = visuals.color(
        materials,
        driver.map_or(0xf4f3f3, |i| [0x8f334e, 0xc1692e, 0x378995, 0x6f7166][i % 4]),
    );
    let dark = visuals.color(materials, 0x1c2732);
    let lights = visuals.color(materials, 0xf8dfaf);
    let red = visuals.color(materials, 0xf3333d);
    let blue = visuals.color(materials, 0x368aff);
    let root = commands
        .spawn((CityRoot, CityCar(driver), Transform::default(), Visibility::Inherited))
        .id();
    part(commands, root, &visuals.cube, &paint, [0.0, 0.63, 0.0], [1.84, 0.70, 4.40]);
    part(commands, root, &visuals.cube, &dark, [0.0, 1.17, -0.24], [1.55, 0.55, 2.30]);
    for x in [-0.98, 0.98] {
        for z in [-1.4, 1.4] {
            let tire = part(commands, root, &visuals.cylinder, &dark, [x, 0.38, z], [0.69, 0.23, 0.69]);
            commands.entity(tire).insert(
                Transform::from_xyz(x, 0.38, z)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::new(0.69, 0.23, 0.69)),
            );
        }
        part(commands, root, &visuals.cube, &lights, [x * 0.73, 0.68, 2.21], [0.3, 0.2, 0.05]);
        part(commands, root, &visuals.cube, &red, [x * 0.73, 0.68, -2.21], [0.3, 0.2, 0.05]);
    }
    if driver.is_none() {
        part(commands, root, &visuals.cube, &red, [-0.40, 1.54, -0.23], [0.50, 0.17, 0.25]);
        part(commands, root, &visuals.cube, &blue, [0.40, 1.54, -0.23], [0.50, 0.17, 0.25]);
    }
}

pub fn spawn_finder(commands: &mut Commands, visuals: &Visuals, materials: &mut Assets<StandardMaterial>) {
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgba(0.46, 0.74, 1.0, 0.38),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        CityRoot,
        Finder,
        Mesh3d(visuals.cylinder.clone()),
        MeshMaterial3d(glow),
        Transform::from_scale(Vec3::new(1.25, 57.0, 1.25)),
        Visibility::Hidden,
    ));
}
