//! The on-screen displays while driving: speed, gear, revs, nitro, the skill
//! chain, boosts, weapons, the race (position, laps, standings, warnings),
//! the time trial, free-roam challenges, the minimap and messages.
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui::{Color32, RichText};

use crate::event::EventState;
use crate::game::{Game, Mode, gear_text, shift_light};
use crate::menu::{GOLD, MUTED};
use crate::text::{format_time, thousands};
use crate::world::World;

pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(12, 16, 24, 190))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(10))
}

fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn bar(ui: &mut egui::Ui, fraction: f32, color: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, Color32::from_white_alpha(40));
    let mut fill = rect;
    fill.set_width(width * fraction.clamp(0.0, 1.0));
    p.rect_filled(fill, 4.0, color);
}

pub fn hud(mut contexts: EguiContexts, game: Res<Game>, world: Res<World>) -> Result {
    let ctx = contexts.ctx_mut()?;
    if game.screen_shot {
        return Ok(());
    }
    let driving = game.mode == Mode::Drive;
    let p = &game.physics;
    let event = game.event.as_ref();
    let area = |id: &str, align: egui::Align2, offset: [f32; 2]| {
        egui::Area::new(egui::Id::new(id))
            .anchor(align, egui::vec2(offset[0], offset[1]))
            .interactable(false)
    };
    if driving {
        // Speed lines and the boost glow at the edges of the screen.
        if game.speed_lines > 0.02 || p.boosting {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("rush")));
            let r = ctx.viewport_rect();
            let alpha = (game.speed_lines * 255.0 * 0.6) as u8;
            for i in 0..40 {
                let a = (i as f32 / 40.0) * std::f32::consts::TAU
                    + (game.real_clock as f32 * 7.0).sin() * 0.05
                    + crate::rng::random() as f32 * 0.1;
                let (s, c) = a.sin_cos();
                let center = r.center();
                let far = r.width().max(r.height());
                let inner = far * (0.42 + crate::rng::random() as f32 * 0.1);
                painter.line_segment(
                    [center + egui::vec2(c, s) * inner, center + egui::vec2(c, s) * far],
                    egui::Stroke::new(2.0, Color32::from_white_alpha(alpha)),
                );
            }
            if p.boosting {
                painter.rect_stroke(
                    r.shrink(2.0),
                    0.0,
                    egui::Stroke::new(14.0, Color32::from_rgba_unmultiplied(80, 170, 255, 40)),
                    egui::StrokeKind::Inside,
                );
            }
        }
        area("speedo", egui::Align2::RIGHT_BOTTOM, [-20.0, -20.0]).show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.set_min_width(200.0);
                // Boosts: drift sparks, slipstream and turbo.
                let mut tags = Vec::new();
                if game.spark_tier > 0 {
                    let names = ["BLUE", "ORANGE", "PURPLE"];
                    tags.push((
                        format!(
                            "{}{} {}",
                            "●".repeat(game.spark_tier),
                            "○".repeat(3 - game.spark_tier),
                            names[game.spark_tier - 1]
                        ),
                        hex([0x4fb3ff, 0xff9f40, 0xc77dff][game.spark_tier - 1]),
                    ));
                }
                if p.draft > 0.3 {
                    tags.push((format!("SLIPSTREAM {}%", (p.draft * 100.0).round()), hex(0x7af0ff)));
                }
                if p.turbo > 0.0 {
                    tags.push(("TURBO".into(), hex(0xffd23f)));
                }
                if !tags.is_empty() {
                    ui.horizontal(|ui| {
                        for (t, c) in tags {
                            ui.label(RichText::new(t).size(12.0).strong().color(c));
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{}", game.speed_in_units(p.speed()).round()))
                            .size(54.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.vertical(|ui| {
                        ui.label(RichText::new(game.unit()).size(16.0).color(Color32::LIGHT_GRAY));
                        let gear_color = if game.gear_refused > 0.0 {
                            hex(0xff5050)
                        } else if p.manual {
                            GOLD
                        } else {
                            Color32::WHITE
                        };
                        ui.label(RichText::new(gear_text(p)).size(26.0).strong().color(gear_color));
                    });
                });
                let revs = p.revs.clamp(0.0, 1.0) as f32;
                let revs_color = if shift_light(p) {
                    hex(0xff2828)
                } else if revs > 0.75 {
                    hex(0xff963c)
                } else {
                    hex(0x78c8ff)
                };
                bar(ui, revs, revs_color, 180.0);
                ui.add_space(4.0);
                ui.label(RichText::new("NITRO").size(11.0).color(Color32::GRAY));
                bar(ui, p.nitro as f32, if p.boosting { hex(0x8ce6ff) } else { hex(0x4fb3ff) }, 180.0);
                if game.weapons.armed() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{} · {}", game.weapons.selected + 1, game.weapons.name()))
                            .size(12.0)
                            .color(Color32::WHITE),
                    );
                    bar(
                        ui,
                        game.weapons.readiness() as f32,
                        if game.weapons.overheated { hex(0xff4040) } else { hex(0xffb347) },
                        180.0,
                    );
                }
            });
        });
        // The skill chain.
        if game.chain.count > 0 || (game.chain_shown > 0.0 && game.chain_end.is_some()) {
            area("chain", egui::Align2::LEFT_CENTER, [16.0, 40.0]).show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    ui.set_min_width(190.0);
                    if game.chain.count > 0 {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(thousands(game.chain.total()))
                                    .size(26.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                            ui.label(RichText::new(format!("×{}", game.chain.multiplier())).size(18.0).color(GOLD));
                        });
                        bar(ui, (game.chain.timer / crate::skills::CHAIN_TIME) as f32, GOLD, 180.0);
                        for f in &game.chain.feed {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&f.label).size(13.0).color(hex(f.kind.color())));
                                ui.label(
                                    RichText::new(format!("+{}", thousands(f.points)))
                                        .size(13.0)
                                        .color(hex(f.kind.color())),
                                );
                            });
                        }
                    } else if let Some((text, banked)) = &game.chain_end {
                        ui.label(
                            RichText::new(text)
                                .size(22.0)
                                .strong()
                                .color(if *banked { GOLD } else { hex(0xff5050) }),
                        );
                    }
                });
            });
        }
        if game.drift_score >= 10.0 {
            area("drift", egui::Align2::CENTER_TOP, [0.0, 110.0]).show(ctx, |ui| {
                ui.label(
                    RichText::new(format!("DRIFT {}", thousands(game.drift_score)))
                        .size(30.0)
                        .strong()
                        .color(GOLD),
                );
            });
        }
        if let Some(e) = event {
            let h = &game.event_hud;
            area("race", egui::Align2::CENTER_TOP, [0.0, 14.0]).show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (label, value) in &h.stats {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(label).size(11.0).color(MUTED));
                                ui.label(RichText::new(value).size(22.0).strong().color(Color32::WHITE));
                            });
                            ui.add_space(12.0);
                        }
                        ui.label(
                            RichText::new(format!(
                                "{}{}",
                                "★".repeat(h.stars as usize),
                                "☆".repeat(3 - h.stars.min(3) as usize)
                            ))
                            .size(22.0)
                            .color(GOLD),
                        );
                    });
                    if let Some(health) = h.health {
                        bar(ui, health as f32, if health < 0.3 { hex(0xff4040) } else { hex(0x3bb273) }, 260.0);
                    }
                });
            });
            if e.is_race() && !h.standings.is_empty() {
                area("standings", egui::Align2::LEFT_TOP, [14.0, 14.0]).show(ctx, |ui| {
                    panel_frame().show(ui, |ui| {
                        for (i, (name, gap, me, out)) in h.standings.iter().enumerate() {
                            let color = if *me {
                                GOLD
                            } else if *out {
                                MUTED
                            } else {
                                Color32::WHITE
                            };
                            ui.label(RichText::new(format!("{}. {name}  {gap}", i + 1)).size(13.0).color(color));
                        }
                    });
                });
            }
            if !h.warning.is_empty() {
                let big = e.state == EventState::Countdown;
                area("warning", egui::Align2::CENTER_CENTER, [0.0, -40.0]).show(ctx, |ui| {
                    ui.label(
                        RichText::new(&h.warning)
                            .size(if big { 64.0 } else { 30.0 })
                            .strong()
                            .color(if big { Color32::WHITE } else { hex(0xff5050) }),
                    );
                });
            }
        } else {
            // Free roam: best scores, the time trial, the damage bill and challenges.
            let mut best = Vec::new();
            if game.best_drift > 0.0 {
                best.push(format!("Best drift {}", thousands(game.best_drift)));
            }
            if game.trial.best > 0.0 {
                best.push(format!("Best lap {}", format_time(game.trial.best)));
            }
            area("best", egui::Align2::LEFT_TOP, [16.0, 16.0]).show(ctx, |ui| {
                if !best.is_empty() {
                    ui.label(RichText::new(best.join(" · ")).size(14.0).color(Color32::WHITE));
                }
                if game.damage > 0.0 {
                    ui.label(
                        RichText::new(format!("${} damage", thousands(game.damage)))
                            .size(16.0)
                            .strong()
                            .color(hex(0xff9f40)),
                    );
                }
                if !game.help {
                    ui.label(RichText::new("H for help · Esc for the garage").size(12.0).color(MUTED));
                }
            });
            if game.trial.active {
                let t = &game.trial;
                area("trial", egui::Align2::CENTER_TOP, [0.0, 14.0]).show(ctx, |ui| {
                    panel_frame().show(ui, |ui| {
                        let best = if t.best > 0.0 {
                            format!("  ·  Best {}", format_time(t.best))
                        } else {
                            String::new()
                        };
                        ui.label(
                            RichText::new(format!("Checkpoint {}/{}  ·  {}{best}", t.next + 1, t.total, format_time(t.time)))
                                .size(18.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                    });
                });
                if t.countdown > 0.0 {
                    area("countdown", egui::Align2::CENTER_CENTER, [0.0, -40.0]).show(ctx, |ui| {
                        ui.label(
                            RichText::new(format!("{}", t.countdown.ceil()))
                                .size(80.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                    });
                }
            }
            if !game.zone_text.is_empty() {
                area("zone", egui::Align2::CENTER_BOTTOM, [0.0, -24.0]).show(ctx, |ui| {
                    panel_frame().show(ui, |ui| {
                        ui.label(RichText::new(&game.zone_text).size(14.0).color(Color32::WHITE));
                    });
                });
            }
        }
        if game.slow_motion {
            area("slowmo", egui::Align2::CENTER_BOTTOM, [0.0, -70.0]).show(ctx, |ui| {
                ui.label(RichText::new("SLOW MOTION").size(18.0).strong().color(hex(0x7af0ff)));
            });
        }
        if game.help {
            area("help", egui::Align2::LEFT_CENTER, [16.0, 0.0]).show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    for line in HELP {
                        ui.label(RichText::new(*line).size(13.0).color(Color32::WHITE));
                    }
                });
            });
        }
        if game.settings.minimap {
            let markers: Vec<(f64, f64, u32, f32)> = match event {
                Some(e) => e.map_markers(),
                None => {
                    let mut m = game.trial.map_markers();
                    m.extend(game.free_roam.map_markers(p.x, p.z));
                    m
                }
            };
            area(
                "minimap",
                egui::Align2::RIGHT_TOP,
                [-16.0, if game.settings.show_fps { 34.0 } else { 16.0 }],
            )
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(crate::minimap::SIZE, crate::minimap::SIZE), egui::Sense::hover());
                crate::minimap::draw(ui.painter(), rect, &world, p.x, p.z, p.heading, &markers);
            });
        }
    }
    if game.mode == Mode::Photo {
        area("photo", egui::Align2::CENTER_BOTTOM, [0.0, -20.0]).show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "📷 Photo mode · drag to look around, right-drag to move, scroll to zoom · Enter saves a photo · P or Esc to drive",
                    )
                    .size(14.0)
                    .color(Color32::WHITE),
                );
            });
        });
    }
    if game.toast.left > 0.0 && !game.toast.text.is_empty() {
        area("toast", egui::Align2::CENTER_CENTER, [0.0, -120.0]).show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.label(RichText::new(&game.toast.text).size(22.0).strong().color(Color32::WHITE));
            });
        });
    }
    if !game.popups.is_empty() {
        area("popups", egui::Align2::RIGHT_TOP, [-16.0, 210.0]).show(ctx, |ui| {
            for p in &game.popups {
                panel_frame().show(ui, |ui| {
                    ui.set_max_width(300.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&p.icon).size(28.0));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&p.title).strong().size(15.0).color(Color32::WHITE));
                            ui.label(RichText::new(&p.text).size(13.0).color(Color32::LIGHT_GRAY));
                        });
                    });
                });
                ui.add_space(6.0);
            }
        });
    }
    if game.settings.show_fps {
        area("fps", egui::Align2::RIGHT_TOP, [-10.0, 8.0]).show(ctx, |ui| {
            ui.add(egui::Label::new(RichText::new(format!("{} FPS", game.fps.2)).size(13.0).color(Color32::WHITE)).extend());
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
    "Controller: triggers drive, A handbrake, B nitro, RB fire",
];
