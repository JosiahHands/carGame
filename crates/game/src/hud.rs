//! The on-screen displays while driving: speed, gear, revs, nitro, drift
//! score, messages, and the frame counter.
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::game::{Game, Mode, gear_text, shift_light};

pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(12, 16, 24, 190))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(10))
}

fn bar(ui: &mut egui::Ui, fraction: f32, color: egui::Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40));
    let mut fill = rect;
    fill.set_width(width * fraction.clamp(0.0, 1.0));
    p.rect_filled(fill, 4.0, color);
}

pub fn hud(mut contexts: EguiContexts, game: Res<Game>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let driving = game.mode == Mode::Drive;
    let p = &game.physics;
    if driving {
        egui::Area::new(egui::Id::new("speedo"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
            .show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}", game.speed_in_units(p.speed()).round()))
                                .size(54.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(game.unit()).size(16.0).color(egui::Color32::LIGHT_GRAY));
                            let refused = game.gear_refused > 0.0;
                            let gear_color = if refused {
                                egui::Color32::from_rgb(255, 80, 80)
                            } else if p.manual {
                                egui::Color32::from_rgb(255, 210, 63)
                            } else {
                                egui::Color32::WHITE
                            };
                            ui.label(egui::RichText::new(gear_text(p)).size(26.0).strong().color(gear_color));
                        });
                    });
                    let revs = p.revs.clamp(0.0, 1.0) as f32;
                    let revs_color = if shift_light(p) {
                        egui::Color32::from_rgb(255, 40, 40)
                    } else if revs > 0.75 {
                        egui::Color32::from_rgb(255, 150, 60)
                    } else {
                        egui::Color32::from_rgb(120, 200, 255)
                    };
                    bar(ui, revs, revs_color, 170.0);
                    ui.add_space(4.0);
                    let nitro_color = if p.boosting {
                        egui::Color32::from_rgb(140, 230, 255)
                    } else {
                        egui::Color32::from_rgb(79, 179, 255)
                    };
                    ui.label(egui::RichText::new("NITRO").size(11.0).color(egui::Color32::GRAY));
                    bar(ui, p.nitro as f32, nitro_color, 170.0);
                });
            });
        if game.drift_score >= 10.0 {
            egui::Area::new(egui::Id::new("drift"))
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 90.0))
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!("DRIFT {}", crate::text::thousands(game.drift_score)))
                            .size(30.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 210, 63)),
                    );
                });
        }
        let mut best = Vec::new();
        if game.best_drift > 0.0 {
            best.push(format!("Best drift {}", crate::text::thousands(game.best_drift)));
        }
        if !best.is_empty() {
            egui::Area::new(egui::Id::new("best"))
                .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 16.0))
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(best.join(" · ")).size(14.0).color(egui::Color32::WHITE));
                });
        }
        if game.help {
            egui::Area::new(egui::Id::new("help"))
                .anchor(egui::Align2::LEFT_CENTER, egui::vec2(16.0, 0.0))
                .show(ctx, |ui| {
                    panel_frame().show(ui, |ui| {
                        for line in HELP {
                            ui.label(egui::RichText::new(*line).size(13.0).color(egui::Color32::WHITE));
                        }
                    });
                });
        }
    }
    if game.toast.left > 0.0 && !game.toast.text.is_empty() {
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -120.0))
            .interactable(false)
            .show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&game.toast.text)
                            .size(22.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                });
            });
    }
    if !game.popups.is_empty() {
        egui::Area::new(egui::Id::new("popups"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 80.0))
            .interactable(false)
            .show(ctx, |ui| {
                for p in &game.popups {
                    panel_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&p.icon).size(28.0));
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&p.title).strong().size(15.0).color(egui::Color32::WHITE));
                                ui.label(egui::RichText::new(&p.text).size(13.0).color(egui::Color32::LIGHT_GRAY));
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
            });
    }
    if game.settings.show_fps {
        egui::Area::new(egui::Id::new("fps"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("{} FPS", game.fps.2))
                        .size(13.0)
                        .color(egui::Color32::WHITE),
                );
            });
    }
    Ok(())
}

pub const HELP: &[&str] = &[
    "W / ↑  accelerate        S / ↓  brake, reverse",
    "A D / ← →  steer          Space  handbrake (drift)",
    "Shift  nitro              J  jump (if the car can)",
    "E or left click  fire     Q / 1 2 3  weapons",
    "X / Z  change gear (manual)",
    "C  camera   R  reset   T  time trial   B  slow motion",
    "G  garage   P  photo mode   N  night   V  horn",
    "M  sound   F  fullscreen   H  this help   Esc  menu",
];
