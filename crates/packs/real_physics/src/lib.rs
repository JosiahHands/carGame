//! 🔬 Real Physics: realistic driving for every car (see model.rs): real
//! suspension, tires that warm up and wear, an engine with a torque curve and
//! turbo, crashes that spin the car, dent its bodywork and bend its steering,
//! a telemetry panel that shows it all at work, and controller rumble.
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::needless_range_loop)]
use std::collections::HashMap;
use std::sync::Arc;

use bevy::input::gamepad::{Gamepad, GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use game::game::{Game, Mode};
use game::packs::{PackInfo, PackOption, Packs, RegisterPack};
use game::physics::Dynamics;

pub mod model;
pub mod powertrain;
pub mod suspension;
pub mod tires;

use model::{Assists, ModelOptions, Realism, RealisticDynamics, peek_state, state_of};

const ID: &str = "physics";

pub struct RealPhysicsPack;

impl Plugin for RealPhysicsPack {
    fn build(&self, app: &mut App) {
        app.register_pack(PackInfo {
            id: ID,
            icon: "🔬",
            name: "Real Physics",
            about: "Realistic driving: a body that leans, dives and bounces on real springs and dampers, four tires with real grip curves that warm up and wear, an engine with a torque curve, turbo lag and gear changes, limited-slip differentials, brakes that fade, and different grip on every surface. Crashes spin the car, dent it and can bend its steering. A telemetry panel shows it all at work, and a controller rumbles as the tires slide.",
        });
        let on_off = || vec![("on".to_string(), "On".to_string()), ("off".to_string(), "Off".to_string())];
        app.world_mut().get_resource_or_init::<Packs>().options.insert(
            ID,
            vec![
                PackOption::Choice("grip", "Tire grip", vec![("road".into(), "Road".into()), ("sport".into(), "Sport".into()), ("race".into(), "Race".into())], "sport".into()),
                PackOption::Choice("assists", "Driving aids", vec![("full".into(), "Full".into()), ("some".into(), "Some".into()), ("off".into(), "Off".into())], "some".into()),
                PackOption::Hint("Driving aids are traction control, ABS and stability control, plus a wheel that helps catch slides. Rivals always have them.".into()),
                PackOption::Choice("realism", "Realism", vec![("friendly".into(), "Friendly".into()), ("real".into(), "Real".into()), ("sim".into(), "Simulation".into())], "real".into()),
                PackOption::Hint("Friendly: tires always warm, instant gear changes. Real: tires warm up and wear, gear changes take a moment, turbo lag, brakes fade when hot, and crashes can damage the engine, wheels and steering. Simulation: cold tires at the start and harsher damage.".into()),
                PackOption::Choice("telemetry", "Telemetry panel", on_off(), "on".into()),
                PackOption::Choice("dents", "Crash dents", on_off(), "on".into()),
                PackOption::Choice("rumble", "Controller rumble", on_off(), "on".into()),
            ],
        );
        app.init_resource::<Dents>()
            .add_systems(
                Update,
                (run.before(game::GameSet::Frame), (dents, rumble).after(game::GameSet::Frame)),
            )
            .add_systems(EguiPrimaryContextPass, telemetry.after(game::hud::hud));
    }
}

/// The pack's options as the model wants them.
fn options(packs: &Packs) -> (ModelOptions, String, bool, bool, bool) {
    let grip_name = packs.string(ID, "grip", "sport");
    let grip = match grip_name.as_str() {
        "road" => 0.95,
        "race" => 1.45,
        _ => 1.2,
    };
    let assists = match packs.string(ID, "assists", "some").as_str() {
        "full" => Assists::Full,
        "off" => Assists::Off,
        _ => Assists::Some,
    };
    let realism = match packs.string(ID, "realism", "real").as_str() {
        "friendly" => Realism::Friendly,
        "sim" => Realism::Sim,
        _ => Realism::Real,
    };
    let on = |k: &str| packs.string(ID, k, "on") == "on";
    (
        ModelOptions { grip, assists, realism },
        grip_name,
        on("telemetry"),
        on("dents"),
        on("rumble"),
    )
}

/// Switches the model in and out with the pack, keeps its options current,
/// and has the garage fix the car.
fn run(packs: Res<Packs>, mut game: ResMut<Game>, mut model: Local<Option<Arc<RealisticDynamics>>>) {
    let on = packs.is_on(ID);
    if !on {
        if game.dynamics.is_some() {
            game.dynamics = None;
            game.physics.dynamics_state = None;
            *model = None;
        }
        return;
    }
    let (opts, ..) = options(&packs);
    let m = model.get_or_insert_with(|| Arc::new(RealisticDynamics::new(opts))).clone();
    if let Ok(mut o) = m.options.write() {
        *o = opts;
    }
    if game.dynamics.is_none() {
        let d: Dynamics = Some(m.clone());
        game.dynamics = d;
        game.physics.dynamics_state = None;
    }
    if matches!(game.mode, Mode::Garage | Mode::Menu) {
        RealisticDynamics::repair(&mut game.physics);
    }
}

/// Dented parts: the mesh each one had before.
#[derive(Resource, Default)]
struct Dents {
    original: HashMap<Entity, Handle<Mesh>>,
    car: Option<Entity>,
}

/// Presses crash dents into the player's car, and the garage takes them out.
fn dents(
    packs: Res<Packs>,
    mut game: ResMut<Game>,
    mut saved: ResMut<Dents>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut parts: Query<(&mut Mesh3d, &GlobalTransform)>,
) {
    let (_, _, _, on, _) = options(&packs);
    let driving = game.mode == Mode::Drive;
    let car_root = game.car.as_ref().map(|c| c.root);
    let fresh: Vec<[f64; 5]> = if game.dynamics.is_some() {
        std::mem::take(&mut state_of(&mut game.physics).dents)
    } else {
        Vec::new()
    };
    let repair = !driving || !packs.is_on(ID) || saved.car != car_root;
    if repair && !saved.original.is_empty() {
        for (e, mesh) in saved.original.drain() {
            if let Ok((mut m, _)) = parts.get_mut(e) {
                m.0 = mesh;
            }
        }
    }
    saved.car = car_root;
    if !driving || !on || fresh.is_empty() {
        return;
    }
    let Some(car) = game.car.clone() else { return };
    let p = &game.physics;
    let rotation = Quat::from_euler(EulerRot::YXZ, p.heading as f32, p.pitch as f32, p.ground_roll as f32);
    let origin = game.car_position();
    for [along, across, nx, nz, lost] in fresh {
        let size = (car.length / 4.4).max(0.2) as f32;
        let radius = (0.55 + (lost / 25.0).min(1.0) as f32 * 0.6) * size;
        let depth = (0.03 + lost as f32 * 0.006).min(0.22) * size;
        let center = origin + rotation * Vec3::new(across as f32, car.height as f32 * 0.45, along as f32);
        let push = Vec3::new(nx as f32, -0.15, nz as f32).normalize_or_zero() * depth;
        for (e, _) in &car.lit_parts {
            let Ok((mut mesh3d, global)) = parts.get_mut(*e) else { continue };
            let Some(mesh) = meshes.get(&mesh3d.0) else { continue };
            // Only meshes kept in memory can be dented (the car's own parts are).
            if mesh.try_attribute(Mesh::ATTRIBUTE_POSITION).is_err() || mesh.count_vertices() > 20000 {
                continue;
            }
            let to_local = global.affine().inverse();
            let local_center = to_local.transform_point3(center);
            let local_push = to_local.transform_vector3(push);
            let to_world = global.affine();
            let mut copy = mesh.clone();
            let mut changed = false;
            if let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) = copy.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
                for pos in positions.iter_mut() {
                    let world = to_world.transform_point3(Vec3::from(*pos));
                    let d = world.distance(center);
                    if d >= radius {
                        continue;
                    }
                    let falloff = (1.0 - d / radius).powi(2);
                    let _ = local_center;
                    pos[0] += local_push.x * falloff;
                    pos[1] += local_push.y * falloff;
                    pos[2] += local_push.z * falloff;
                    changed = true;
                }
            }
            if changed {
                saved.original.entry(*e).or_insert_with(|| mesh3d.0.clone());
                mesh3d.0 = meshes.add(copy);
            }
        }
    }
}

/// A controller buzzes as the tires reach their limit, and thumps over bumps, landings and crashes.
fn rumble(
    packs: Res<Packs>,
    game: Res<Game>,
    time: Res<Time<Real>>,
    pads: Query<Entity, With<Gamepad>>,
    mut requests: MessageWriter<GamepadRumbleRequest>,
    mut wait: Local<f32>,
) {
    let (.., on) = options(&packs);
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = 0.1;
    let Some(s) = peek_state(&game.physics) else { return };
    let active = on && packs.is_on(ID) && game.is_driving();
    let slip = s.usage.iter().copied().fold(0.0, f64::max);
    let weak = ((slip - 0.9) * 1.6).clamp(0.0, 1.0) as f32;
    let strong = (s.susp.shake * 0.35 - 0.05 + s.jolt).clamp(0.0, 1.0) as f32;
    for pad in &pads {
        if !active || (weak < 0.03 && strong < 0.03) {
            requests.write(GamepadRumbleRequest::Stop { gamepad: pad });
        } else {
            requests.write(GamepadRumbleRequest::Add {
                gamepad: pad,
                duration: std::time::Duration::from_millis(130),
                intensity: GamepadRumbleIntensity {
                    strong_motor: strong,
                    weak_motor: weak,
                },
            });
        }
    }
}

/// The telemetry panel: the grip circle, each tire's work, temperature and
/// wear, the engine, the brakes, the driving aids and any damage.
fn telemetry(mut contexts: EguiContexts, packs: Res<Packs>, game: Res<Game>, mut trail: Local<Vec<egui::Pos2>>) -> Result {
    let (opts, grip_name, show, ..) = options(&packs);
    if !packs.is_on(ID) || !show || !game.is_driving() || game.screen_shot {
        return Ok(());
    }
    let Some(s) = peek_state(&game.physics) else { return Ok(()) };
    let car = &game.physics;
    let ctx = contexts.ctx_mut()?;
    egui::Area::new(egui::Id::new("telemetry"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(14.0, -14.0))
        .interactable(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(300.0, 172.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 12.0, egui::Color32::from_rgba_unmultiplied(8, 12, 20, 158));
            let at = |x: f32, y: f32| rect.min + egui::vec2(x, y);
            let small = egui::FontId::proportional(9.0);
            let realism = match opts.realism {
                Realism::Friendly => "FRIENDLY",
                Realism::Real => "REAL",
                Realism::Sim => "SIMULATION",
            };
            p.text(
                at(10.0, 8.0),
                egui::Align2::LEFT_TOP,
                format!("{} TIRES · {realism}", grip_name.to_uppercase()),
                small.clone(),
                egui::Color32::from_white_alpha(128),
            );
            // The g-force circle.
            let (cx, cy, r) = (60.0, 72.0, 44.0);
            let limit = (car.corner_grip(car.forward_speed.abs(), &game.dynamics) / model::CORNER_MARGIN).max(1.0);
            let stroke = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(64));
            p.circle_stroke(at(cx, cy), r, stroke);
            p.circle_stroke(at(cx, cy), r * 0.5, stroke);
            p.line_segment([at(cx - r, cy), at(cx + r, cy)], stroke);
            p.line_segment([at(cx, cy - r), at(cx, cy + r)], stroke);
            let dot = at(cx - (s.ay / limit) as f32 * r, cy + (s.ax / limit) as f32 * r);
            trail.push(dot);
            if trail.len() > 30 {
                trail.remove(0);
            }
            let n = trail.len();
            for (i, d) in trail.iter().enumerate() {
                p.circle_filled(
                    *d,
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(120, 200, 255, ((i as f32 / n as f32) * 128.0) as u8),
                );
            }
            p.circle_filled(dot, 4.5, egui::Color32::from_rgb(127, 212, 255));
            p.text(
                at(cx, cy + r + 8.0),
                egui::Align2::CENTER_TOP,
                format!("{:.2} g", s.ax.hypot(s.ay) / 9.81),
                egui::FontId::proportional(10.0),
                egui::Color32::from_white_alpha(190),
            );
            // The four tires, seen from above.
            let best = tires::best_temperature(opts.grip);
            let full = s.susp.compression.iter().copied().fold(0.05, f64::max) * 1.6;
            for i in 0..4 {
                let x = 124.0 + (i % 2) as f32 * 56.0;
                let y = 22.0 + if i < 2 { 0.0 } else { 56.0 };
                let use_ = s.usage[i];
                let color = if use_ < 0.8 {
                    egui::Color32::from_rgb(59, 178, 115)
                } else if use_ < 1.15 {
                    egui::Color32::from_rgb(255, 182, 39)
                } else {
                    egui::Color32::from_rgb(230, 57, 70)
                };
                let tire = egui::Rect::from_min_size(at(x, y), egui::vec2(16.0, 28.0));
                p.rect_filled(tire, 4.0, color);
                if s.wear[i] > 0.01 {
                    let mut worn = tire;
                    worn.set_height(28.0 * s.wear[i].min(1.0) as f32);
                    p.rect_filled(worn, 0.0, egui::Color32::from_black_alpha(115));
                }
                let bar = |dx: f32, share: f32, c: egui::Color32| {
                    let full = egui::Rect::from_min_size(at(x + dx, y), egui::vec2(4.0, 28.0));
                    p.rect_filled(full, 1.0, egui::Color32::from_white_alpha(30));
                    let h = 28.0 * share.clamp(0.0, 1.0);
                    p.rect_filled(egui::Rect::from_min_size(at(x + dx, y + 28.0 - h), egui::vec2(4.0, h)), 1.0, c);
                };
                bar(20.0, (s.load[i].min(2.0) / 2.0) as f32, egui::Color32::from_white_alpha(204));
                bar(
                    26.0,
                    (s.susp.compression[i].max(0.0) / full) as f32,
                    egui::Color32::from_rgb(155, 140, 255),
                );
                let t = s.temp[i];
                let tc = if t.is_nan() {
                    egui::Color32::from_white_alpha(100)
                } else if t - best < -15.0 {
                    egui::Color32::from_rgb(120, 180, 255)
                } else if t - best > 20.0 {
                    egui::Color32::from_rgb(255, 90, 80)
                } else {
                    egui::Color32::from_rgb(120, 230, 140)
                };
                p.text(
                    at(x, y + 30.0),
                    egui::Align2::LEFT_TOP,
                    if t.is_nan() { "--°".into() } else { format!("{}°", t.round()) },
                    small.clone(),
                    tc,
                );
            }
            // The engine, turbo, gear and brakes.
            let y = 132.0;
            let bar_w = 120.0;
            let rpm = egui::Rect::from_min_size(at(124.0, y), egui::vec2(bar_w, 6.0));
            p.rect_filled(rpm, 3.0, egui::Color32::from_white_alpha(30));
            p.rect_filled(
                egui::Rect::from_min_size(rpm.min, egui::vec2(bar_w * s.rpm.min(1.0) as f32, 6.0)),
                3.0,
                if s.rpm > 0.92 {
                    egui::Color32::from_rgb(255, 80, 80)
                } else {
                    egui::Color32::from_rgb(120, 200, 255)
                },
            );
            let turbo = egui::Rect::from_min_size(at(124.0, y + 9.0), egui::vec2(bar_w, 4.0));
            p.rect_filled(turbo, 2.0, egui::Color32::from_white_alpha(30));
            p.rect_filled(
                egui::Rect::from_min_size(turbo.min, egui::vec2(bar_w * s.boost as f32, 4.0)),
                2.0,
                egui::Color32::from_rgb(255, 210, 63),
            );
            p.text(
                at(250.0, y - 2.0),
                egui::Align2::LEFT_TOP,
                game::game::gear_text(car),
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
            let brake = if s.brake_temp.is_nan() {
                "--".into()
            } else {
                format!("{}", s.brake_temp.round())
            };
            p.text(
                at(124.0, y + 16.0),
                egui::Align2::LEFT_TOP,
                format!("BRAKES {brake}°{}", if s.fade { " FADE" } else { "" }),
                small.clone(),
                if s.fade {
                    egui::Color32::from_rgb(255, 120, 80)
                } else {
                    egui::Color32::from_white_alpha(160)
                },
            );
            // Driving aids and damage.
            let mut lights = Vec::new();
            if s.traction_control {
                lights.push("TC");
            }
            if s.abs {
                lights.push("ABS");
            }
            if s.stability {
                lights.push("ESC");
            }
            p.text(
                at(10.0, 156.0),
                egui::Align2::LEFT_TOP,
                lights.join(" "),
                small.clone(),
                egui::Color32::from_rgb(255, 182, 39),
            );
            let d = &s.damage;
            let wheel = d.wheel.iter().copied().fold(0.0, f64::max);
            if d.engine > 0.01 || wheel > 0.01 || d.toe.abs() > 0.002 {
                let text = format!(
                    "DAMAGE engine {}% · wheels {}%{}",
                    (d.engine * 100.0).round(),
                    (wheel * 100.0).round(),
                    if d.toe.abs() > 0.002 { " · steering bent" } else { "" }
                );
                p.text(
                    at(60.0, 156.0),
                    egui::Align2::LEFT_TOP,
                    text,
                    small,
                    egui::Color32::from_rgb(255, 120, 80),
                );
            }
        });
    Ok(())
}
