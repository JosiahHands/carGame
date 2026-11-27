//! 🌅 Photo Real: makes the game look real. A physically based sky that also
//! lights and reflects in everything, soft shadows in corners (ambient
//! occlusion), glowing lights, filmic color, roads that shine in the rain,
//! street lamps and headlights that really light the road, real car models
//! (by Daniel Zhabotinsky, CC BY 4.0, see packs/photo_real/libs/car-models/CREDITS.md)
//! and a camera lens with vignette, fringing, grain, speed blur and raindrops.
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::needless_range_loop)]
use std::collections::HashMap;

use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::visibility::VisibilityRange;
use bevy::camera::{Exposure, Hdr};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode};
use bevy::light::{Atmosphere, AtmosphereEnvironmentMapLight, NotShadowCaster, NotShadowReceiver, atmosphere::ScatteringMedium};
use bevy::pbr::{AtmosphereSettings, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::post_process::dof::DepthOfField;
use bevy::post_process::effect_stack::{ChromaticAberration, Vignette};
use bevy::post_process::motion_blur::MotionBlur;
use bevy::prelude::*;
use bevy::render::view::{ColorGrading, ColorGradingGlobal};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use game::carmodel::{CarLook, Ghost};
use game::game::{Game, Mode};
use game::packs::{PackInfo, PackOption, Packs, RegisterPack};
use game::venues::{Ground, Weather};
use game::world::{TargetKind, World};
use game::world_build::Wet;
use game::world_render::{MainCamera, SkyDome, Sun};

const ID: &str = "graphics";
/// The real car models (in this pack's libs/ folder, and the same place under assets/ in a download).
const MODELS: &str = "packs/photo_real/libs/car-models/cars.glb";

pub struct PhotoRealPack;

impl Plugin for PhotoRealPack {
    fn build(&self, app: &mut App) {
        app.register_pack(PackInfo {
            id: ID,
            icon: "🌅",
            name: "Photo Real",
            about: "Makes the game look real: a true sky that lights and reflects in everything, soft shadows in every corner, glowing lights, filmic color, roads that shine in the rain, street lamps and headlights that light the road, and a real camera lens. Every car gets a real, detailed car body with no badges or logos, painted by your garage settings. At night, headlights shine beams through the air; in the rain, drops land on the lens. Car models by Daniel Zhabotinsky (CC BY 4.0).",
        });
        let pct = |key: &'static str, label: &'static str, default: f64| PackOption::Slider(key, label, 0.0, 1.0, 0.05, default);
        let on_off = || vec![("on".to_string(), "On".to_string()), ("off".to_string(), "Off".to_string())];
        app.world_mut().get_resource_or_init::<Packs>().options.insert(
            ID,
            vec![
                PackOption::Choice("quality", "Quality", vec![("medium".into(), "Medium".into()), ("high".into(), "High".into()), ("ultra".into(), "Ultra".into())], "high".into()),
                PackOption::Hint("High adds soft shadows in every corner (ambient occlusion). Ultra also smooths edges over time: it needs a strong graphics card.".into()),
                PackOption::Choice("time", "Time of day", vec![("morning".into(), "Morning".into()), ("noon".into(), "Noon".into()), ("golden".into(), "Golden hour".into())], "morning".into()),
                PackOption::Choice("look", "Color", vec![("natural".into(), "Natural".into()), ("film".into(), "Film".into()), ("vivid".into(), "Vivid".into()), ("noir".into(), "Noir".into())], "natural".into()),
                PackOption::Choice("tone", "Tone", vec![("agx".into(), "Filmic".into()), ("aces".into(), "Punchy".into()), ("neutral".into(), "Neutral".into())], "agx".into()),
                PackOption::Slider("exposure", "Exposure", 0.6, 1.8, 0.05, 1.0),
                pct("speedBlur", "Speed blur", 0.5),
                pct("vignette", "Vignette", 0.4),
                pct("grain", "Film grain", 0.25),
                pct("aberration", "Lens fringing", 0.35),
                PackOption::Hint("Photo mode focuses on your car and blurs the background, like a real camera.".into()),
                PackOption::Choice("models", "Car models", vec![("real".into(), "Real".into()), ("game".into(), "Game".into())], "real".into()),
                PackOption::Hint("Real: detailed car bodies with no badges or logos, by Daniel Zhabotinsky (CC BY 4.0). Game: the game's own bodies.".into()),
                PackOption::Choice("lensRain", "Rain on the lens", on_off(), "on".into()),
                pct("beams", "Headlight beams", 0.5),
                PackOption::Choice("headlightShadows", "Headlight shadows", on_off(), "on".into()),
            ],
        );
        app.init_resource::<State>()
            .add_systems(Startup, setup)
            .add_systems(Update, (load_models, dress_cars, night_lights, beams).after(game::GameSet::Frame))
            .add_systems(
                PostUpdate,
                (camera, sky_and_sun, wet_roads)
                    .after(game::world_render::WorldRenderSet)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(EguiPrimaryContextPass, lens.before(game::hud::hud));
    }
}

#[derive(Clone, Debug)]
struct Options {
    on: bool,
    quality: String,
    time: String,
    look: String,
    tone: String,
    exposure: f32,
    speed_blur: f32,
    vignette: f32,
    grain: f32,
    aberration: f32,
    real_models: bool,
    lens_rain: bool,
    beams: f32,
    headlight_shadows: bool,
}

fn options(packs: &Packs) -> Options {
    let s = |k: &str, d: &str| packs.string(ID, k, d);
    let n = |k: &str, d: f64| packs.number(ID, k, d) as f32;
    Options {
        on: packs.is_on(ID),
        quality: s("quality", "high"),
        time: s("time", "morning"),
        look: s("look", "natural"),
        tone: s("tone", "agx"),
        exposure: n("exposure", 1.0),
        speed_blur: n("speedBlur", 0.5),
        vignette: n("vignette", 0.4),
        grain: n("grain", 0.25),
        aberration: n("aberration", 0.35),
        real_models: s("models", "real") == "real",
        lens_rain: s("lensRain", "on") == "on",
        beams: n("beams", 0.5),
        headlight_shadows: s("headlightShadows", "on") == "on",
    }
}

/// A part of a real car model: its mesh, material and where it sits.
#[derive(Clone)]
struct Part {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    kind: &'static str,
    transform: Transform,
}

/// One body type's model: near and far versions, and its measurements.
#[derive(Clone)]
struct Model {
    near: Vec<Part>,
    far: Vec<Part>,
    axle_front: f32,
    axle_rear: f32,
    wheel_radius: f32,
    body_width: f32,
    arch_radius: f32,
    max_y: f32,
}

#[derive(Resource, Default)]
struct State {
    gltf: Option<Handle<Gltf>>,
    models: HashMap<String, Model>,
    failed: bool,
    /// Cars dressed in a real body: car root -> the body's entity.
    dressed: HashMap<Entity, Entity>,
    /// Materials made wet, with how they were dry.
    wet: HashMap<AssetId<StandardMaterial>, (f32, Color, f32)>,
    wet_generation: u64,
    atmosphere: Option<Entity>,
    medium: Option<Handle<ScatteringMedium>>,
    lamps: Vec<Entity>,
    lamp_timer: f32,
    beams: Vec<Entity>,
    beam_car: Option<Entity>,
    lens_rain: f32,
    drops: Vec<(f32, f32, f32, f32)>,
}

fn setup(mut commands: Commands, mut state: ResMut<State>, mut media: ResMut<Assets<ScatteringMedium>>) {
    state.medium = Some(media.add(ScatteringMedium::earth(256, 256)));
    for _ in 0..6 {
        state.lamps.push(
            commands
                .spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.79, 0.54),
                        intensity: 0.0,
                        range: 26.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::default(),
                ))
                .id(),
        );
    }
}

/// Starts loading the car models the first time the pack is on.
fn load_models(
    packs: Res<Packs>,
    mut state: ResMut<State>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let o = options(&packs);
    if !o.on || !o.real_models || state.failed || !state.models.is_empty() {
        return;
    }
    let handle = state.gltf.get_or_insert_with(|| assets.load(MODELS)).clone();
    if let Some(bevy::asset::LoadState::Failed(_)) = assets.get_load_state(&handle) {
        state.failed = true;
        return;
    }
    let Some(gltf) = gltfs.get(&handle) else { return };
    // Material names, from the file itself (several share a name, so the map in the asset can't tell them apart).
    let names = material_names(&assets, &handle);
    let mut standard: HashMap<usize, Handle<StandardMaterial>> = HashMap::new();
    for kind in ["sports", "muscle", "hatch", "pickup", "van", "wedge"] {
        let Some(node) = gltf.named_nodes.get(format!("car-{kind}").as_str()).and_then(|h| nodes.get(h)) else {
            continue;
        };
        let far = gltf
            .named_nodes
            .get(format!("car-{kind}-far").as_str())
            .and_then(|h| nodes.get(h))
            .unwrap_or(node);
        let Some(extras) = node
            .extras
            .as_ref()
            .and_then(|e| serde_json::from_str::<serde_json::Value>(&e.value).ok())
        else {
            continue;
        };
        let num = |k: &str| extras.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32;
        let max_y = extras
            .get("max")
            .and_then(|m| m.get(1))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1.4) as f32;
        let mut collect = |root: &GltfNode| {
            let mut out = Vec::new();
            let mut stack = vec![(root.clone(), Transform::IDENTITY)];
            while let Some((n, parent)) = stack.pop() {
                // The car's own node sits at its place in the file; its parts are measured from the car.
                let here = if std::ptr::eq(&n, root) {
                    Transform::IDENTITY
                } else {
                    parent * n.transform
                };
                if let Some(mesh) = n.mesh.as_ref().and_then(|h| meshes.get(h)) {
                    for p in &mesh.primitives {
                        let index = p.material.as_ref().and_then(|m| gltf.materials.iter().position(|x| x == m));
                        let kind: &'static str = match index.and_then(|i| names.get(i)).map(String::as_str) {
                            Some("paint") => "paint",
                            Some("glass") => "glass",
                            Some("lamp-front") => "head",
                            Some("lamp-rear") => "tail",
                            _ => "other",
                        };
                        let material = match index {
                            Some(i) => standard
                                .entry(i)
                                .or_insert_with(|| {
                                    let m = gltf.materials[i].clone();
                                    let std = gltf_materials
                                        .get(&m)
                                        .map(bevy::pbr::gltf::standard_material_from_gltf_material)
                                        .unwrap_or_default();
                                    materials.add(std)
                                })
                                .clone(),
                            None => materials.add(StandardMaterial::default()),
                        };
                        out.push(Part {
                            mesh: p.mesh.clone(),
                            material,
                            kind,
                            transform: here,
                        });
                    }
                }
                for c in &n.children {
                    if let Some(child) = nodes.get(c) {
                        stack.push((child.clone(), here));
                    }
                }
            }
            out
        };
        let near = collect(node);
        let far = collect(far);
        if near.is_empty() {
            continue;
        }
        state.models.insert(
            kind.to_string(),
            Model {
                near,
                far,
                axle_front: num("axleFront"),
                axle_rear: num("axleRear"),
                wheel_radius: num("wheelRadius"),
                body_width: num("bodyWidth"),
                arch_radius: num("archRadius"),
                max_y,
            },
        );
    }
    if state.models.is_empty() {
        state.failed = true;
    }
}

/// The material names in the glTF file, in order.
fn material_names(assets: &AssetServer, handle: &Handle<Gltf>) -> Vec<String> {
    let Some(path) = assets.get_path(handle.id()) else {
        return Vec::new();
    };
    // The file is read from the same folder the asset server uses.
    let candidates = [game::assets_folder().join(path.path()), std::path::PathBuf::from(path.path())];
    for file in candidates {
        let Ok(bytes) = std::fs::read(&file) else { continue };
        if bytes.len() < 20 {
            continue;
        }
        let length = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes[20..20 + length]) {
            return json["materials"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|m| m["name"].as_str().unwrap_or("").to_string())
                .collect();
        }
    }
    Vec::new()
}

/// Puts real bodies on cars (and takes them off when switched off).
fn dress_cars(
    packs: Res<Packs>,
    mut state: ResMut<State>,
    cars: Query<(Entity, &CarLook), Without<Ghost>>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let o = options(&packs);
    let want = o.on && o.real_models && !state.models.is_empty();
    // Undress cars that are gone or shouldn't be dressed.
    let dressed: Vec<(Entity, Entity)> = state.dressed.iter().map(|(a, b)| (*a, *b)).collect();
    for (car, body) in dressed {
        let exists = cars.get(car).is_ok();
        if exists && want {
            continue;
        }
        state.dressed.remove(&car);
        if let Ok(mut e) = commands.get_entity(body) {
            e.despawn();
        }
        if let Ok((_, look)) = cars.get(car) {
            for e in &look.0.shell {
                if let Ok(mut ec) = commands.get_entity(*e) {
                    ec.insert(Visibility::Inherited);
                }
            }
        }
    }
    if !want {
        return;
    }
    for (car, look) in &cars {
        if state.dressed.contains_key(&car) {
            continue;
        }
        let rig = &look.0;
        let Some(model) = state.models.get(&rig.config.body).cloned() else {
            continue;
        };
        let spec = &rig.shape.spec;
        let sz = (spec.axle_front - spec.axle_rear) / (model.axle_front - model.axle_rear);
        let sx = spec.width / model.body_width;
        let roof = spec.cabin.iter().map(|p| p.1).fold(f32::MIN, f32::max) + 0.045;
        let even = (sx + sz) / 2.0 * rig.config.height as f32;
        let fit_y = (roof - spec.wheel_radius) / (model.max_y - model.wheel_radius);
        let sy = fit_y.clamp(even * 0.94, even * 1.06);
        let mut y = spec.wheel_radius - model.wheel_radius * sy;
        let tire_top = rig.shape.wheel_y + spec.wheel_radius * rig.config.wheel_size as f32 + 0.015;
        let arch_top = spec.wheel_radius + model.arch_radius * sy;
        y += (tire_top - arch_top).max(0.0);
        let place = Transform::from_xyz(0.0, y, (spec.axle_front + spec.axle_rear) / 2.0).with_scale(Vec3::new(sx, sy, sz));
        // The paint pattern, moved and stretched onto the model (its paint is laid out side-on in its own meters).
        let paint = match materials.get(&rig.mats.body_paint).cloned() {
            Some(mut m) if m.base_color_texture.is_some() => {
                let fit = bevy::math::Affine2::from_scale_angle_translation(
                    Vec2::new(sz, -sy),
                    0.0,
                    Vec2::new(place.translation.z, -place.translation.y),
                );
                m.uv_transform *= fit;
                materials.add(m)
            }
            _ => rig.mats.body_paint.clone(),
        };
        let body = commands.spawn((place, Visibility::default(), ChildOf(rig.body))).id();
        for (parts, range) in [(&model.near, 0.0..45.0), (&model.far, 45.0..100_000.0)] {
            for p in parts.iter() {
                let material = match p.kind {
                    "paint" => paint.clone(),
                    "glass" => rig.mats.glass.clone(),
                    "head" => rig.mats.head.clone(),
                    "tail" => rig.mats.tail.clone(),
                    _ => p.material.clone(),
                };
                let mut e = commands.spawn((
                    Mesh3d(p.mesh.clone()),
                    MeshMaterial3d(material),
                    p.transform,
                    VisibilityRange {
                        start_margin: range.start..range.start,
                        end_margin: range.end..range.end,
                        use_aabb: false,
                    },
                    ChildOf(body),
                ));
                if p.kind == "glass" {
                    e.insert(NotShadowCaster);
                }
            }
        }
        for e in &rig.shell {
            if let Ok(mut ec) = commands.get_entity(*e) {
                ec.insert(Visibility::Hidden);
            }
        }
        state.dressed.insert(car, body);
    }
}

/// The camera: HDR, the sky's light, bloom, filmic color, lens effects and ambient occlusion.
#[allow(clippy::type_complexity)]
fn camera(
    packs: Res<Packs>,
    game: Res<Game>,
    mut commands: Commands,
    mut cam: Query<(Entity, &mut Transform, Option<&mut Tonemapping>, Option<&Hdr>), With<MainCamera>>,
    mut was_on: Local<Option<String>>,
) {
    let o = options(&packs);
    let Ok((entity, transform, _, _)) = cam.single_mut() else { return };
    let key = format!("{:?}", (o.on, &o.quality, &o.tone, &o.look, game.settings.antialias));
    let mut e = commands.entity(entity);
    if !o.on {
        if was_on.take().is_some() {
            e.remove::<(
                Hdr,
                AtmosphereSettings,
                AtmosphereEnvironmentMapLight,
                Bloom,
                Vignette,
                ChromaticAberration,
                MotionBlur,
                ScreenSpaceAmbientOcclusion,
                TemporalAntiAliasing,
                DepthOfField,
                ColorGrading,
                Exposure,
            )>();
            e.insert((
                Tonemapping::AcesFitted,
                if game.settings.antialias { Msaa::Sample4 } else { Msaa::Off },
            ));
        }
        return;
    }
    if was_on.as_deref() != Some(key.as_str()) {
        *was_on = Some(key);
        let tone = match o.tone.as_str() {
            "aces" => Tonemapping::AcesFitted,
            "neutral" => Tonemapping::TonyMcMapface,
            _ => Tonemapping::AgX,
        };
        let grading = match o.look.as_str() {
            "film" => ColorGrading {
                global: ColorGradingGlobal {
                    post_saturation: 0.85,
                    temperature: 0.08,
                    ..default()
                },
                ..default()
            },
            "vivid" => ColorGrading {
                global: ColorGradingGlobal {
                    post_saturation: 1.3,
                    ..default()
                },
                ..default()
            },
            "noir" => ColorGrading {
                global: ColorGradingGlobal {
                    post_saturation: 0.0,
                    ..default()
                },
                ..default()
            },
            _ => ColorGrading::default(),
        };
        e.insert((
            Hdr,
            tone,
            AtmosphereSettings::default(),
            AtmosphereEnvironmentMapLight::default(),
            Bloom::NATURAL,
            grading,
        ));
        if o.quality == "medium" {
            e.remove::<(ScreenSpaceAmbientOcclusion, TemporalAntiAliasing)>();
            e.insert(if game.settings.antialias { Msaa::Sample4 } else { Msaa::Off });
        } else {
            // Ambient occlusion needs the scene without multisampling.
            e.insert((Msaa::Off, ScreenSpaceAmbientOcclusion::default()));
            if o.quality == "ultra" {
                e.insert(TemporalAntiAliasing::default());
            } else {
                e.remove::<TemporalAntiAliasing>();
            }
        }
    }
    // Things that change every frame.
    let night = game.mode == Mode::Drive && false;
    let _ = night;
    let speed = if game.mode == Mode::Drive {
        (game.physics.speed() / 55.0).min(1.0) as f32
    } else {
        0.0
    };
    e.insert((
        Vignette {
            intensity: o.vignette * 0.8,
            ..default()
        },
        ChromaticAberration {
            intensity: o.aberration * 0.012 * (1.0 + speed),
            ..default()
        },
        MotionBlur {
            shutter_angle: o.speed_blur * speed * 0.9,
            samples: 4,
        },
    ));
    if game.mode == Mode::Photo {
        let focus = transform.translation.distance(game.car_position() + Vec3::Y * 0.8);
        e.insert(DepthOfField {
            focal_distance: focus,
            aperture_f_stops: 1.4,
            ..default()
        });
    } else {
        e.remove::<DepthOfField>();
    }
}

/// Sun heights for the times of day, degrees.
fn sun_height(time: &str) -> f32 {
    match time {
        "noon" => 58.0,
        "golden" => 9.0,
        _ => 24.0,
    }
}

/// The real sky, and the sun where it shows it, as bright as real sunlight.
#[allow(clippy::type_complexity)]
fn sky_and_sun(
    packs: Res<Packs>,
    world: Res<World>,
    mut state: ResMut<State>,
    mut commands: Commands,
    mut sky: Query<&mut Visibility, With<SkyDome>>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut camera: Query<(Entity, Option<&mut AmbientLight>), With<MainCamera>>,
) {
    let o = options(&packs);
    if let Ok(mut v) = sky.single_mut() {
        let want = if o.on { Visibility::Hidden } else { Visibility::Inherited };
        if *v != want {
            *v = want;
        }
    }
    if !o.on {
        if let Some(a) = state.atmosphere.take()
            && let Ok(mut e) = commands.get_entity(a)
        {
            e.despawn();
        }
        if let Ok((cam, _)) = camera.single() {
            commands.entity(cam).remove::<Exposure>();
        }
        return;
    }
    if state.atmosphere.is_none()
        && let Some(medium) = state.medium.clone()
    {
        state.atmosphere = Some(commands.spawn(Atmosphere::earth(medium)).id());
    }
    let night = world.is_night();
    let theme = if world.map == "city" { None } else { Some(world.theme()) };
    let weather = world.weather_kind();
    let desert = theme.is_some_and(|t| t.ground == Ground::Sand);
    let sun_share = match weather {
        Weather::Rain => 0.45,
        Weather::Snow => 0.6,
        Weather::Clear if desert => 1.05,
        Weather::Clear => 1.0,
    };
    if let Ok((mut light, mut t)) = sun.single_mut() {
        let height = sun_height(&o.time).to_radians();
        let around = 40f32.atan2(25.0);
        let dir = Vec3::new(around.sin() * height.cos(), height.sin(), around.cos() * height.cos());
        if !night {
            light.illuminance = bevy::light::light_consts::lux::RAW_SUNLIGHT * sun_share * if o.time == "golden" { 0.85 } else { 1.0 };
            let target = t.translation - Vec3::new(40.0, 80.0, 25.0);
            *t = Transform::from_translation(target + dir * 100.0).looking_at(target, Vec3::Y);
            if o.time == "golden" {
                light.color = Color::srgb(1.0, 0.75, 0.5);
            }
        }
    }
    if let Ok((cam, ambient)) = camera.single_mut() {
        // Daylight is real sunlight, so the camera's exposure matches it; at night the game's own lights stay.
        let ev = if night { 9.7 } else { 13.0 } - (o.exposure - 1.0) * 2.0;
        commands.entity(cam).insert(Exposure { ev100: ev });
        if let Some(mut a) = ambient
            && !night
        {
            a.brightness = 0.0; // the sky lights everything now
        }
    }
}

/// Rain soaks the roads and ground: darker, and shiny.
fn wet_roads(
    packs: Res<Packs>,
    world: Res<World>,
    mut state: ResMut<State>,
    ground: Query<&MeshMaterial3d<StandardMaterial>, With<Wet>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let o = options(&packs);
    let weather = world.weather_kind();
    let wetness = match weather {
        Weather::Rain => 1.0,
        Weather::Snow => 0.35,
        Weather::Clear => 0.0,
    };
    let want = o.on && wetness > 0.0;
    if state.wet_generation != world.generation {
        state.wet_generation = world.generation;
        state.wet.clear(); // the old map's materials are gone
    }
    if !want {
        for (id, (rough, color, reflect)) in state.wet.drain() {
            if let Some(mut m) = materials.get_mut(id) {
                m.perceptual_roughness = rough;
                m.base_color = color;
                m.reflectance = reflect;
            }
        }
        return;
    }
    for m in &ground {
        let id = m.0.id();
        if state.wet.contains_key(&id) {
            continue;
        }
        if let Some(mut mat) = materials.get_mut(id) {
            state.wet.insert(id, (mat.perceptual_roughness, mat.base_color, mat.reflectance));
            mat.perceptual_roughness *= 1.0 - 0.7 * wetness;
            mat.reflectance = 0.5 + 0.3 * wetness;
            let c = mat.base_color.to_linear() * (1.0 - 0.3 * wetness);
            mat.base_color = Color::LinearRgba(c.with_alpha(1.0));
        }
    }
}

/// At night the street lamps nearest the camera really light the road, and
/// the player's headlights throw shadows.
fn night_lights(
    packs: Res<Packs>,
    game: Res<Game>,
    world: Res<World>,
    time: Res<Time<Real>>,
    mut state: ResMut<State>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut lights: Query<(&mut PointLight, &mut Transform)>,
    mut spots: Query<&mut SpotLight>,
) {
    let o = options(&packs);
    let night = o.on && world.is_night();
    state.lamp_timer -= time.delta_secs();
    if state.lamp_timer <= 0.0 {
        state.lamp_timer = 0.25;
        let cam = camera.single().map(|c| c.translation()).unwrap_or_default();
        let mut near: Vec<(f32, f64, f64, f64)> = if night {
            world
                .targets
                .iter()
                .filter(|t| t.kind == TargetKind::Lamp && t.alive)
                .map(|t| (((t.x as f32 - cam.x).powi(2) + (t.z as f32 - cam.z).powi(2)), t.x, t.z, t.size))
                .collect()
        } else {
            Vec::new()
        };
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (i, &e) in state.lamps.iter().enumerate() {
            let Ok((mut light, mut t)) = lights.get_mut(e) else { continue };
            match near.get(i) {
                Some(&(_, x, z, size)) => {
                    t.translation = Vec3::new(x as f32, 6.4 * size as f32, z as f32);
                    light.range = 26.0 * size as f32;
                    light.intensity = 24.0 * size as f32 * game::world_render::LUMENS;
                }
                None => light.intensity = 0.0,
            }
        }
    }
    if let Some(beam) = game.car.as_ref().and_then(|c| c.beam)
        && let Ok(mut spot) = spots.get_mut(beam)
    {
        let want = night && o.headlight_shadows;
        if spot.shadow_maps_enabled != want {
            spot.shadow_maps_enabled = want;
        }
    }
}

/// Headlight beams in the night air: soft glowing cones from the player's headlights.
fn beams(
    packs: Res<Packs>,
    game: Res<Game>,
    world: Res<World>,
    mut state: ResMut<State>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let o = options(&packs);
    let car = game.car.as_ref();
    let show = o.on && o.beams > 0.0 && world.is_night() && game.mode == Mode::Drive;
    if state.beam_car != car.map(|c| c.root) || !show {
        for e in state.beams.drain(..) {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.despawn();
            }
        }
        state.beam_car = None;
    }
    if !show || !state.beams.is_empty() {
        return;
    }
    let Some(car) = car else { return };
    let weather = world.weather_kind();
    let strength = o.beams
        * match weather {
            Weather::Rain => 1.8,
            Weather::Snow => 1.5,
            Weather::Clear => 1.0,
        };
    let c = game::world_build::css_color(&car.config.headlight_color).to_linear();
    let a = 0.08 * strength;
    let mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(c.red * a, c.green * a, c.blue * a, a)),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        double_sided: true,
        fog_enabled: true,
        ..default()
    });
    let cone = meshes.add(bevy::shape::Cone { radius: 3.2, height: 16.0 }.mesh().resolution(16).build());
    let spec = &car.shape.spec;
    for side in [-1.0f32, 1.0] {
        let t = Transform::from_xyz(side * (spec.width / 2.0 - 0.3), spec.headlight.1, car.shape.front_z + 8.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2 - 0.06));
        state.beams.push(
            commands
                .spawn((
                    Mesh3d(cone.clone()),
                    MeshMaterial3d(mat.clone()),
                    t,
                    NotShadowCaster,
                    NotShadowReceiver,
                    ChildOf(car.body),
                ))
                .id(),
        );
    }
    state.beam_car = Some(car.root);
}

/// The lens: film grain, and raindrops on the lens in the rain.
fn lens(
    mut contexts: EguiContexts,
    packs: Res<Packs>,
    game: Res<Game>,
    world: Res<World>,
    time: Res<Time<Real>>,
    mut state: ResMut<State>,
) -> Result {
    let o = options(&packs);
    if !o.on || game.screen_shot {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let dt = time.delta_secs();
    let raining = o.lens_rain && game.mode == Mode::Drive && world.weather_kind() == Weather::Rain;
    state.lens_rain = (state.lens_rain + if raining { dt / 4.0 } else { -dt / 2.0 }).clamp(0.0, 1.0);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("lens")));
    let r = ctx.viewport_rect();
    // Raindrops: they land, sit, and run down the glass.
    let want = (state.lens_rain * 40.0) as usize;
    while state.drops.len() < want {
        state.drops.push((
            game::rng::random() as f32,
            game::rng::random() as f32,
            3.0 + game::rng::random() as f32 * 9.0,
            0.0,
        ));
    }
    state.drops.truncate(want);
    let speed = (game.physics.speed() / 30.0) as f32;
    for d in &mut state.drops {
        d.3 += dt;
        d.1 += dt * (0.02 + speed * 0.1) * if d.2 > 8.0 { 1.0 } else { 0.2 };
        if d.1 > 1.05 || d.3 > 6.0 {
            *d = (game::rng::random() as f32, -0.02, 3.0 + game::rng::random() as f32 * 9.0, 0.0);
        }
        let at = egui::pos2(r.left() + d.0 * r.width(), r.top() + d.1 * r.height());
        painter.circle(
            at,
            d.2,
            egui::Color32::from_white_alpha(18),
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)),
        );
    }
    // Grain: faint specks that change every frame.
    if o.grain > 0.0 {
        let count = (o.grain * 900.0) as usize;
        for _ in 0..count {
            let at = egui::pos2(
                r.left() + game::rng::random() as f32 * r.width(),
                r.top() + game::rng::random() as f32 * r.height(),
            );
            let v = if game::rng::random() < 0.5 { 255 } else { 0 };
            painter.rect_filled(
                egui::Rect::from_min_size(at, egui::vec2(1.5, 1.5)),
                0.0,
                egui::Color32::from_rgba_unmultiplied(v, v, v, (o.grain * 40.0) as u8),
            );
        }
    }
    Ok(())
}
