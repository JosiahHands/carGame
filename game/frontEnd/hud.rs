//! The on-screen displays while driving: speed, gear, revs, nitro, the skill
//! chain, boosts, weapons, the race (position, laps, standings, warnings),
//! the time trial, free-roam challenges, the minimap and messages.
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui::{Color32, RichText};

use crate::event::EventState;
use crate::game::{Game, Mode, Popup, gear_text, shift_light};
use crate::menu::{GOLD, MUTED};
use crate::text::{format_time, thousands};
use crate::world::World;

pub const CITY_STATUS_AREA: &str = "city status";

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

fn popup_card(ui: &mut egui::Ui, popup: &Popup, width: f32) {
    let frame = panel_frame();
    let content_width = (width - frame.inner_margin.sum().x).max(1.0);
    frame.show(ui, |ui| {
        ui.set_width(content_width);
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 6.0);
        ui.horizontal_top(|ui| {
            if !popup.icon.is_empty() && content_width >= 130.0 {
                ui.add_sized([32.0, 0.0], egui::Label::new(RichText::new(&popup.icon).size(26.0)).extend());
            }
            let text_width = ui.available_width().max(1.0);
            ui.vertical(|ui| {
                ui.set_width(text_width);
                ui.add(
                    egui::Label::new(
                        RichText::new(&popup.title)
                            .strong()
                            .size(16.0)
                            .line_height(Some(21.0))
                            .color(Color32::WHITE),
                    )
                    .wrap(),
                );
                if !popup.text.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&popup.text)
                                .size(14.0)
                                .line_height(Some(19.0))
                                .color(Color32::LIGHT_GRAY),
                        )
                        .wrap(),
                    );
                }
            });
        });
    });
}

#[cfg(test)]
#[test]
fn popup_text_wraps_with_room_between_title_and_description() {
    let popup = Popup {
        icon: "🏆".into(),
        title: "Trophy unlocked for finishing every city driving challenge".into(),
        text: "Your new trophy and coin reward are waiting in the career menu.".into(),
        left: 4.6,
    };
    for width in [168.0, 340.0] {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| popup_card(ui, &popup, width));
        output.textures_delta.clear();
        let find = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                    _ => None,
                })
                .unwrap()
        };
        let title = find(&popup.title);
        let body = find(&popup.text);
        assert!(title.galley.rows.len() > 1 && body.galley.rows.len() > 1);
        assert!((title.pos.x - body.pos.x).abs() < 1.0);
        assert!(body.pos.y >= title.pos.y + title.galley.size().y + 5.0);
        assert!(title.galley.size().x <= width - 20.0 && body.galley.size().x <= width - 20.0);
    }
}

pub fn hud(mut contexts: EguiContexts, game: Res<Game>, world: Res<World>) -> Result {
    let ctx = contexts.ctx_mut()?;
    if game.screen_shot {
        return Ok(());
    }
    // The game's version, in the top left corner of every screen (left out of saved photos).
    egui::Area::new(egui::Id::new("version"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(8.0, 4.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(12, 16, 24, 150))
                .corner_radius(5.0)
                .inner_margin(egui::Margin::symmetric(5, 1))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("v{}", crate::VERSION))
                            .size(11.0)
                            .color(Color32::from_white_alpha(210)),
                    );
                });
        });
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
        // Wanted by the police.
        let stars = game.police.stars();
        if stars > 0 {
            area("wanted", egui::Align2::CENTER_TOP, [0.0, 14.0]).show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("🚓 {}{}", "★".repeat(stars), "☆".repeat(5 - stars.min(5))))
                            .size(20.0)
                            .strong()
                            .color(GOLD),
                    );
                });
            });
        }
        if let Some(me) = &game.on_foot {
            area("onfoot", egui::Align2::RIGHT_BOTTOM, [-20.0, -20.0]).show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    ui.set_min_width(200.0);
                    ui.label(RichText::new("ON FOOT").size(12.0).strong().color(Color32::LIGHT_GRAY));
                    ui.label(RichText::new("HEALTH").size(11.0).color(Color32::GRAY));
                    let health = (me.hp / crate::walk::HEALTH) as f32;
                    bar(ui, health, if health < 0.3 { hex(0xff4040) } else { hex(0x5fd16a) }, 180.0);
                    let fighting = game.settings.map_options(&world.map).fighting;
                    ui.label(
                        RichText::new(if fighting {
                            format!("{} · 1 fists · 2 pistol", me.weapon.name())
                        } else {
                            "Fighting is off".into()
                        })
                        .size(12.0)
                        .color(Color32::WHITE),
                    );
                });
            });
            let near_car = world.targets.iter().any(|t| {
                t.kind == crate::world::TargetKind::Car
                    && t.alive
                    && t.rig
                        .as_ref()
                        .is_some_and(|r| (t.x - me.pos.x).hypot(t.z - me.pos.z) < r.length.max(r.width) / 2.0 + 1.8)
            }) || game.other_cars.iter().any(|c| (c.at.x - me.pos.x).hypot(c.at.z - me.pos.z) < 4.0);
            if near_car && me.knocked <= 0.0 {
                area("getin", egui::Align2::CENTER_BOTTOM, [0.0, -70.0]).show(ctx, |ui| {
                    panel_frame().show(ui, |ui| {
                        ui.label(RichText::new("F · Get in").size(16.0).strong().color(Color32::WHITE));
                    });
                });
            }
            if me.knocked > 0.0 {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("knocked")));
                let alpha = ((3.5 - me.knocked).clamp(0.0, 1.0) * 220.0) as u8;
                painter.rect_filled(ctx.viewport_rect(), 0.0, Color32::from_rgba_unmultiplied(10, 10, 14, alpha));
            }
        } else {
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
        }
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
                area("standings", egui::Align2::LEFT_TOP, [14.0, 28.0]).show(ctx, |ui| {
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
            area("best", egui::Align2::LEFT_TOP, [16.0, 28.0]).show(ctx, |ui| {
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
        let width = (ctx.viewport_rect().width() - 40.0).clamp(1.0, 520.0);
        area("toast", egui::Align2::CENTER_CENTER, [0.0, -120.0])
            .default_width(width)
            .show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    ui.set_max_width(width);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&game.toast.text)
                                .size(22.0)
                                .line_height(Some(28.0))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .wrap(),
                    );
                });
            });
    }
    if !game.popups.is_empty() {
        let viewport = ctx.viewport_rect();
        // Beside the garage panel, not over it.
        let right = if game.mode == Mode::Garage { crate::garage::WIDTH } else { 0.0 };
        let width = (viewport.width() - right - 32.0).clamp(1.0, 340.0);
        let mut top = if driving && game.settings.minimap {
            crate::minimap::SIZE + if game.settings.show_fps { 34.0 } else { 16.0 } + 12.0
        } else {
            32.0
        };
        if driving
            && let Some(rect) = ctx.memory(|memory| {
                let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new(CITY_STATUS_AREA));
                memory
                    .areas()
                    .visible_last_frame(&layer)
                    .then(|| memory.area_rect(layer.id))
                    .flatten()
            })
        {
            top = top.max(rect.bottom() - viewport.top() + 12.0);
        }
        area("popups", egui::Align2::RIGHT_TOP, [-16.0 - right, top])
            .default_width(width)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 8.0;
                for popup in &game.popups {
                    popup_card(ui, popup, width);
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
    "M  sound   F11  fullscreen   H  this help   Esc  menu",
    "F  get out of the car, or into the nearest one (free roam)",
    "On foot: W S walk, A D turn, Shift run, Space jump",
    "   E or click  punch or shoot   1 fists   2 pistol",
    "Controller: triggers drive, A handbrake, B nitro, RB fire, D-pad up get in/out",
];

pub mod minimap {
    //! The minimap in the top-right corner. It turns with the car, so the way you
    //! are driving is always up.
    use bevy_egui::egui::{self, Color32, Pos2, Shape, Stroke, Vec2};

    use crate::world::{BlockKind, BuildingState, HALF_SIZE, ROAD_LINES, ROAD_WIDTH, TargetKind, World};

    pub const SIZE: f32 = 180.0;

    fn hex(rgb: u32) -> Color32 {
        Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// Draws the map around (x, z) into `rect`. Markers are (x, z, color, size).
    pub fn draw(painter: &egui::Painter, rect: egui::Rect, world: &World, x: f64, z: f64, heading: f64, markers: &[(f64, f64, u32, f32)]) {
        let painter = painter.with_clip_rect(rect);
        let center = rect.center();
        let range = world.track.as_ref().map_or(130.0, |t| (t.length / 10.0).clamp(120.0, 300.0));
        let ppm = (SIZE / 2.0) as f64 / range;
        let (s, c) = heading.sin_cos();
        // World to screen: forward is up, +x is to the left (as seen from behind the car).
        let to_screen = |wx: f64, wz: f64| {
            let (u, v) = (-(wx - x) * ppm, -(wz - z) * ppm);
            Pos2::new(center.x + (u * c - v * s) as f32, center.y + (u * s + v * c) as f32)
        };
        let quad = |cx: f64, cz: f64, w: f64, d: f64, turn: f64, color: Color32| {
            let (ts, tc) = turn.sin_cos();
            let corner = |a: f64, b: f64| to_screen(cx + a * tc + b * ts, cz - a * ts + b * tc);
            Shape::convex_polygon(
                vec![
                    corner(-w / 2.0, -d / 2.0),
                    corner(w / 2.0, -d / 2.0),
                    corner(w / 2.0, d / 2.0),
                    corner(-w / 2.0, d / 2.0),
                ],
                color,
                Stroke::NONE,
            )
        };
        painter.rect_filled(rect, 22.0, hex(0x3d5a35));
        let near = |px: f64, pz: f64, size: f64| (px - x).abs() < range * 1.5 + size && (pz - z).abs() < range * 1.5 + size;
        if world.map == "city" {
            let inner = 80.0 - ROAD_WIDTH;
            for b in &world.blocks {
                if !near(b.x, b.z, 40.0) {
                    continue;
                }
                let color = match b.kind {
                    BlockKind::Park => 0x4f7d3f,
                    BlockKind::Lot => 0x45474c,
                    _ => 0x8b8f96,
                };
                painter.add(quad(b.x, b.z, inner, inner, 0.0, hex(color)));
                if b.kind == BlockKind::Stunt {
                    painter.add(quad(b.x, b.z, inner - 6.0, inner - 6.0, 0.0, hex(0x9a9488)));
                }
            }
            let len = HALF_SIZE * 2.0 - 16.0;
            for &line in &ROAD_LINES {
                painter.add(quad(line, 0.0, ROAD_WIDTH, len, 0.0, hex(0x2b2d31)));
                painter.add(quad(0.0, line, len, ROAD_WIDTH, 0.0, hex(0x2b2d31)));
            }
        }
        if let Some(land) = world.land.as_deref() {
            // The open world: highways, and the towns' streets.
            let asphalt = hex(0x2b2d31);
            for r in land.roads_near(x, z, range * 1.6) {
                let road = &land.roads[r];
                let pts: Vec<Pos2> = road
                    .points
                    .iter()
                    .step_by(3)
                    .filter(|p| near(p.x, p.z, 30.0))
                    .map(|p| to_screen(p.x, p.z))
                    .collect();
                if pts.len() > 1 {
                    painter.add(Shape::line(pts, Stroke::new((crate::land::ROAD_WIDTH * ppm) as f32, asphalt)));
                }
            }
            for t in &land.towns {
                if t.size == crate::land::Size::City || !near(t.at.x, t.at.y, t.half + 20.0) {
                    continue;
                }
                for &l in &t.lines {
                    painter.add(quad(t.at.x + l, t.at.y, ROAD_WIDTH, t.half * 2.0, 0.0, asphalt));
                    painter.add(quad(t.at.x, t.at.y + l, t.half * 2.0, ROAD_WIDTH, 0.0, asphalt));
                }
            }
        }
        if let Some(track) = &world.track {
            let mut pts: Vec<Pos2> = track.points.iter().map(|p| to_screen(p.x, p.z)).collect();
            pts.push(pts[0]);
            painter.add(Shape::line(pts, Stroke::new((track.style.width * ppm) as f32, hex(0x2b2d31))));
            let start = track.pose(0.0, 0.0);
            painter.add(quad(
                start.x,
                start.z,
                track.style.width,
                3.0,
                start.heading - std::f64::consts::FRAC_PI_2 + std::f64::consts::FRAC_PI_2,
                hex(0xf4f4f4),
            ));
        }
        let wall = Stroke::new(1.5, hex(0xb8bcc2));
        for w in &world.walls {
            if near(w.ax, w.az, 10.0) || near(w.bx, w.bz, 10.0) {
                painter.line_segment([to_screen(w.ax, w.az), to_screen(w.bx, w.bz)], wall);
            }
        }
        for r in &world.ramps {
            if near(r.x, r.z, 20.0) {
                painter.add(quad(r.x, r.z, r.width, r.length, r.angle, hex(0xe0a03a)));
            }
        }
        // Only what's near (the open world has thousands of things), via the box grid.
        let mut seen: Vec<usize> = world
            .boxes_near(x, z, range * 1.6)
            .into_iter()
            .filter_map(|b| world.boxes.get(b).and_then(|b| b.target))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        for t in seen.into_iter().filter_map(|i| world.targets.get(i)) {
            if !near(t.x, t.z, 60.0) {
                continue;
            }
            match t.kind {
                TargetKind::Building => {
                    let color = if t.alive {
                        0xd8d2c6
                    } else if t.state == BuildingState::Rising {
                        0xa8a296
                    } else {
                        0x6a5a4a
                    };
                    painter.add(quad(t.x, t.z, t.width, t.depth, 0.0, hex(color)));
                }
                TargetKind::Car if t.rig.is_some() => {
                    painter.add(quad(t.x, t.z, 2.4, 2.4, 0.0, hex(if t.alive { 0xb9c3d6 } else { 0x1a1a1a })));
                }
                _ => {}
            }
        }
        let h = world.half;
        let edge = [
            to_screen(-h, -h),
            to_screen(h, -h),
            to_screen(h, h),
            to_screen(-h, h),
            to_screen(-h, -h),
        ];
        painter.add(Shape::line(edge.to_vec(), Stroke::new(2.0, hex(0xb8bcc2))));
        for &(mx, mz, color, size) in markers {
            let (mut dx, mut dz) = (mx - x, mz - z);
            let d = dx.hypot(dz);
            let max = range - 8.0;
            if d > max {
                dx *= max / d;
                dz *= max / d;
            }
            painter.circle_filled(to_screen(x + dx, z + dz), size, hex(color));
        }
        // The player: an arrow in the middle, always pointing up.
        let arrow = [Vec2::new(0.0, -8.0), Vec2::new(6.0, 6.0), Vec2::new(0.0, 3.0), Vec2::new(-6.0, 6.0)].map(|v| center + v);
        painter.add(Shape::convex_polygon(
            vec![arrow[0], arrow[1], arrow[2]],
            hex(0xff5a36),
            Stroke::NONE,
        ));
        painter.add(Shape::convex_polygon(
            vec![arrow[0], arrow[2], arrow[3]],
            hex(0xff5a36),
            Stroke::NONE,
        ));
        painter.add(Shape::closed_line(arrow.to_vec(), Stroke::new(1.5, Color32::WHITE)));
        painter.rect_stroke(
            rect,
            22.0,
            Stroke::new(2.0, Color32::from_white_alpha(90)),
            egui::StrokeKind::Inside,
        );
    }
}
