//! The garage panel: every customization option grouped into tabs, ready-made
//! and saved builds with share codes, the maps, and the Settings and Packs tabs.
use bevy_egui::egui::{self, Color32, RichText};

use crate::config::*;
use crate::menu::{MUTED, button};
use crate::packs::Packs;
use crate::settings::{PRESETS, Settings};
use crate::storage::Storage;
use crate::text::{format_time, thousands};
use crate::timetrial::best_lap_for;
use crate::venues::venues;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Builds,
    Maps,
    Body,
    Paint,
    Wheels,
    Parts,
    Lights,
    SoundFx,
    Engine,
    Physics,
    Settings,
    Packs,
}

const TABS: [(Tab, &str); 12] = [
    (Tab::Builds, "Builds"),
    (Tab::Maps, "Maps"),
    (Tab::Body, "Body"),
    (Tab::Paint, "Paint"),
    (Tab::Wheels, "Wheels"),
    (Tab::Parts, "Parts"),
    (Tab::Lights, "Lights"),
    (Tab::SoundFx, "Sound & FX"),
    (Tab::Engine, "Engine"),
    (Tab::Physics, "Physics"),
    (Tab::Settings, "Settings"),
    (Tab::Packs, "Packs"),
];

#[derive(Clone, Debug, PartialEq)]
pub enum GarageAction {
    /// The car's look changed (it needs building again).
    Look,
    /// Driving or sound changed.
    Tuning,
    SettingsChanged,
    RebuildCity,
    MapChanged,
    Drive,
    Menu,
    Honk,
    PacksChanged,
}

pub struct Garage {
    pub tab: Tab,
    build_name: String,
    paste: String,
    message: String,
    copied: bool,
}

impl Default for Garage {
    fn default() -> Self {
        Garage {
            tab: Tab::Builds,
            build_name: String::new(),
            paste: String::new(),
            message: String::new(),
            copied: false,
        }
    }
}

/// 2.5 -> "2.5×", 12 -> "12×".
fn times(v: f64) -> String {
    if v < 10.0 {
        format!("{}×", (v * 100.0).round() / 100.0)
    } else {
        format!("{}×", v.round())
    }
}

fn color32(hex: &str) -> Color32 {
    let c = crate::canvas::Rgba::css(hex);
    Color32::from_rgb((c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8)
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong().size(16.0));
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(MUTED));
}

/// Choices from a list of (value, label); true if one was picked.
fn choice(ui: &mut egui::Ui, label: &str, options: Options, value: &mut String) -> bool {
    let mut changed = false;
    ui.label(RichText::new(label).size(13.0).color(MUTED));
    ui.horizontal_wrapped(|ui| {
        for (v, l) in options {
            if ui.selectable_label(value == v, *l).clicked() && value != v {
                *value = v.to_string();
                changed = true;
            }
        }
    });
    changed
}

fn on_off(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(13.0).color(MUTED));
        for (v, l) in [(true, "On"), (false, "Off")] {
            if ui.selectable_label(*value == v, l).clicked() && *value != v {
                *value = v;
                changed = true;
            }
        }
    });
    changed
}

/// Color swatches and a picker for any color.
fn color(ui: &mut egui::Ui, label: &str, swatches: &[&str], value: &mut String) -> bool {
    let mut changed = false;
    ui.label(RichText::new(label).size(13.0).color(MUTED));
    ui.horizontal_wrapped(|ui| {
        for s in swatches {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 6.0, color32(s));
            if value == s {
                ui.painter()
                    .rect_stroke(rect, 6.0, egui::Stroke::new(2.5, Color32::WHITE), egui::StrokeKind::Outside);
            }
            if response.clicked() {
                *value = s.to_string();
                changed = true;
            }
        }
        let mut c = color32(value);
        if ui.color_edit_button_srgba(&mut c).changed() {
            *value = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
            changed = true;
        }
    });
    changed
}

/// A slider for a value between `min` and `max`, rounded to `step`. A log
/// slider gives each doubling the same room.
#[allow(clippy::too_many_arguments)]
fn slider(ui: &mut egui::Ui, label: &str, value: &mut f64, min: f64, max: f64, step: f64, fmt: &dyn Fn(f64) -> String, log: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([120.0, 18.0], egui::Label::new(RichText::new(label).size(13.0).color(MUTED)));
        let mut v = *value;
        let r = ui.add(
            egui::Slider::new(&mut v, min..=max)
                .logarithmic(log)
                .show_value(false)
                .step_by(if log { 0.0 } else { step }),
        );
        ui.label(fmt(*value));
        if r.changed() {
            let snapped = ((v / step).round() * step).clamp(min, max);
            if snapped != *value {
                *value = (snapped * 1e6).round() / 1e6;
                changed = true;
            }
        }
    });
    changed
}

fn number(ui: &mut egui::Ui, label: &str, key: &str, value: &mut f64, step: f64, fmt: &dyn Fn(f64) -> String, log: bool) -> bool {
    let (min, max) = range(key);
    slider(ui, label, value, min, max, step, fmt, log)
}

/// The garage panel for this frame.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ctx: &egui::Context,
    g: &mut Garage,
    c: &mut CarConfig,
    s: &mut Settings,
    storage: &mut Storage,
    packs: &mut Packs,
    describe: &str,
) -> Vec<GarageAction> {
    let mut out = Vec::new();
    let mut root = crate::menu::root_ui(ctx, "garage-root");
    egui::Panel::right("garage")
        .exact_size(400.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(10, 14, 22, 230))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(&mut root, |ui| {
            ui.heading(RichText::new("Garage").strong().size(26.0));
            ui.label(
                RichText::new("Build anything, then hit Drive. Your paint and parts go on your career cars too.")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.label(RichText::new(describe).size(12.0));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (t, label) in TABS {
                    if ui.selectable_label(g.tab == t, label).clicked() {
                        g.tab = t;
                        g.copied = false;
                    }
                }
            });
            ui.separator();
            egui::Panel::bottom("garage-foot").frame(egui::Frame::NONE).show(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if button(ui, "◀ Menu", false, true) {
                        out.push(GarageAction::Menu);
                    }
                    if button(ui, "Randomize", false, true) {
                        *c = random_config(c);
                        out.extend([GarageAction::Look, GarageAction::Tuning]);
                    }
                    if button(ui, "Reset", false, true) {
                        *c = CarConfig::default();
                        out.extend([GarageAction::Look, GarageAction::Tuning]);
                    }
                    if button(ui, "Drive ▶", true, true) {
                        out.push(GarageAction::Drive);
                    }
                });
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| tab(ui, g, c, s, storage, packs, &mut out));
        });
    out
}

fn tab(
    ui: &mut egui::Ui,
    g: &mut Garage,
    c: &mut CarConfig,
    s: &mut Settings,
    storage: &mut Storage,
    packs: &mut Packs,
    out: &mut Vec<GarageAction>,
) {
    let mph = s.is_mph();
    let length = move |m: f64| {
        if mph {
            format!(
                "{} ft",
                if m < 3.0 {
                    ((m * 3.281) * 10.0).round() / 10.0
                } else {
                    (m * 3.281).round()
                }
            )
        } else {
            format!("{} m", if m < 10.0 { (m * 10.0).round() / 10.0 } else { m.round() })
        }
    };
    let speed = move |v: f64| {
        if mph {
            format!("{} mph", v.round())
        } else {
            format!("{} km/h", (v * 1.609).round())
        }
    };
    let mut look = false;
    let mut tuned = false;
    let t = &mut c.tuning;
    match g.tab {
        Tab::Builds => {
            builds(ui, g, c, storage, out);
            return;
        }
        Tab::Maps => {
            for v in venues() {
                let best = best_lap_for(storage, v.value);
                let active = s.map == v.value;
                let r = egui::Frame::NONE
                    .fill(if active {
                        Color32::from_rgba_unmultiplied(200, 60, 40, 70)
                    } else {
                        Color32::from_white_alpha(12)
                    })
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(v.label).strong());
                        ui.label(RichText::new(v.about).size(12.0).color(MUTED));
                        ui.label(
                            RichText::new(if best > 0.0 {
                                format!("Best lap {}", format_time(best))
                            } else {
                                "No lap yet".into()
                            })
                            .size(12.0)
                            .italics(),
                        );
                    })
                    .response
                    .interact(egui::Sense::click());
                if r.clicked() && !active {
                    s.map = v.value.into();
                    out.push(GarageAction::MapChanged);
                }
                ui.add_space(4.0);
            }
            hint(
                ui,
                "Press T while driving for a timed lap. Beat your best and a ghost of that lap races you next time.",
            );
            return;
        }
        Tab::Body => {
            look |= choice(ui, "Body style", BODY_OPTIONS, &mut c.body);
            heading(ui, "Size and shape");
            look |= number(ui, "Size", "size", &mut c.size, 0.05, &times, true);
            look |= number(ui, "Length", "length", &mut c.length, 0.05, &times, false);
            look |= number(ui, "Width", "width", &mut c.width, 0.05, &times, false);
            look |= number(ui, "Height", "height", &mut c.height, 0.05, &times, false);
            hint(
                ui,
                "Big enough (about 10 ft tall or more) and the car flattens buildings, cars, lamps and trees in its way.",
            );
            heading(ui, "Plates and numbers");
            ui.horizontal(|ui| {
                ui.label(RichText::new("License plate").size(13.0).color(MUTED));
                let mut plate = c.plate.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut plate).char_limit(8).desired_width(120.0))
                    .changed()
                {
                    c.plate = clean_plate(&plate);
                    look = true;
                }
            });
            look |= number(
                ui,
                "Race number",
                "raceNumber",
                &mut c.race_number,
                1.0,
                &|v| if v == 0.0 { "None".into() } else { format!("#{v}") },
                false,
            );
        }
        Tab::Paint => {
            look |= color(ui, "Paint color", PAINT_SWATCHES, &mut c.paint_color);
            look |= choice(ui, "Finish", FINISH_OPTIONS, &mut c.finish);
            look |= choice(ui, "Pattern", PATTERN_OPTIONS, &mut c.pattern);
            look |= choice(ui, "Stripes", STRIPE_OPTIONS, &mut c.stripes);
            look |= color(ui, "Accent color (patterns, stripes, parts)", PAINT_SWATCHES, &mut c.accent_color);
            look |= number(
                ui,
                "Window tint",
                "windowTint",
                &mut c.window_tint,
                0.05,
                &|v| format!("{}%", (v * 100.0).round()),
                false,
            );
        }
        Tab::Wheels => {
            look |= choice(ui, "Rims", WHEEL_OPTIONS, &mut c.wheel_style);
            look |= color(
                ui,
                "Rim color",
                &[
                    "#c9ccd1", "#1a1a1a", "#ffffff", "#d4af37", "#b87333", "#d7263d", "#1b98e0", "#3bb273",
                ],
                &mut c.rim_color,
            );
            look |= color(
                ui,
                "Tire color",
                &[
                    "#1a1a1a", "#f2f2f2", "#d7263d", "#1b98e0", "#ffd23f", "#ff70a6", "#3bb273", "#7b2cbf",
                ],
                &mut c.tire_color,
            );
            heading(ui, "Size and stance");
            look |= number(ui, "Wheel size", "wheelSize", &mut c.wheel_size, 0.05, &times, false);
            look |= number(ui, "Tire width", "tireWidth", &mut c.tire_width, 0.05, &times, false);
            look |= number(ui, "Wheel spacing", "track", &mut c.track, 0.05, &times, false);
            look |= number(
                ui,
                "Camber",
                "camber",
                &mut c.camber,
                1.0,
                &|v| if v == 0.0 { "Straight".into() } else { format!("{v}°") },
                false,
            );
            look |= number(
                ui,
                "Suspension lift",
                "lift",
                &mut c.lift,
                0.01,
                &|v| {
                    if v == 0.0 {
                        "Stock".into()
                    } else if v < 0.0 {
                        "Slammed".into()
                    } else {
                        format!("+{}", length(v))
                    }
                },
                false,
            );
            hint(
                ui,
                "Wheels too big for the arches lift the body up on top of them, monster truck style.",
            );
        }
        Tab::Parts => {
            look |= choice(ui, "Spoiler", SPOILER_OPTIONS, &mut c.spoiler);
            look |= choice(ui, "Spoiler color", PART_COLOR_OPTIONS, &mut c.spoiler_color);
            look |= choice(ui, "Exhaust", EXHAUST_OPTIONS, &mut c.exhaust);
            look |= choice(ui, "Roof topper", TOPPER_OPTIONS, &mut c.topper);
            look |= choice(ui, "Antenna", ANTENNA_OPTIONS, &mut c.antenna);
            look |= on_off(ui, "Bull bar", &mut c.bull_bar);
            look |= on_off(ui, "Roof light bar", &mut c.light_bar);
            look |= on_off(ui, "Roof weapons", &mut c.roof_guns);
            hint(
                ui,
                "Roof weapons show when weapons are on (Settings → Weapons). In the career, your car carries the weapons you have bought.",
            );
        }
        Tab::Lights => {
            look |= color(ui, "Headlight color", LIGHT_SWATCHES, &mut c.headlight_color);
            look |= choice(ui, "Underglow", UNDERGLOW_OPTIONS, &mut c.underglow);
            look |= color(
                ui,
                "Underglow color",
                &[
                    "#7af0ff", "#c58bff", "#ff4fd8", "#39ff88", "#ff3b3b", "#ffd23f", "#2f6bff", "#ffffff",
                ],
                &mut c.underglow_color,
            );
            look |= color(
                ui,
                "Nitro flame color",
                &["#4fb3ff", "#ff7a1a", "#c58bff", "#39ff88", "#ff3bd0", "#ffffff"],
                &mut c.nitro_color,
            );
            hint(
                ui,
                "Press N while driving to switch between day and night. Glow paint and underglow look best at night.",
            );
        }
        Tab::SoundFx => {
            tuned |= choice(ui, "Engine sound", ENGINE_OPTIONS, &mut c.engine_sound);
            if choice(ui, "Horn", HORN_OPTIONS, &mut c.horn) {
                tuned = true;
                out.push(GarageAction::Honk);
            }
            tuned |= choice(ui, "Boost trail", TRAIL_OPTIONS, &mut c.boost_trail);
            tuned |= on_off(ui, "Tire smoke", &mut c.tire_smoke);
            tuned |= color(
                ui,
                "Tire smoke color",
                &[
                    "#e8e8e8", "#3a3a3a", "#ff4fd8", "#7af0ff", "#39ff88", "#ffd23f", "#ff3b3b", "#c58bff",
                ],
                &mut c.tire_smoke_color,
            );
            hint(ui, "Press V to sound the horn while driving. Tire smoke shows when you drift.");
        }
        Tab::Engine => {
            tuned |= number(
                ui,
                "Horsepower",
                "horsepower",
                &mut t.horsepower,
                10.0,
                &|v| format!("{} hp", thousands(v)),
                true,
            );
            tuned |= number(ui, "Top speed", "topSpeed", &mut t.top_speed, 1.0, &speed, true);
            tuned |= number(ui, "Nitro power", "nitroPower", &mut t.nitro_power, 0.05, &times, false);
            tuned |= number(
                ui,
                "Nitro bottle",
                "nitroSeconds",
                &mut t.nitro_seconds,
                0.5,
                &|v| format!("{v} s"),
                false,
            );
            tuned |= on_off(ui, "Infinite nitro", &mut t.infinite_nitro);
            tuned |= number(
                ui,
                "Downforce",
                "downforce",
                &mut t.downforce,
                0.5,
                &|v| if v == 0.0 { "None".into() } else { format!("{v}/10") },
                false,
            );
            hint(
                ui,
                "A high top speed needs lots of horsepower to reach. Downforce adds grip at speed, so a 500 mph car can still turn.",
            );
        }
        Tab::Physics => {
            tuned |= number(ui, "Grip", "grip", &mut t.grip, 0.05, &times, false);
            tuned |= number(ui, "Brakes", "brakes", &mut t.brakes, 0.05, &times, false);
            tuned |= number(ui, "Steering lock", "steering", &mut t.steering, 1.0, &|v| format!("{v}°"), false);
            tuned |= number(ui, "Drift (handbrake)", "drift", &mut t.drift, 1.0, &|v| format!("{v}/10"), false);
            tuned |= number(
                ui,
                "Jump height",
                "jump",
                &mut t.jump,
                0.5,
                &|v| if v == 0.0 { "Off".into() } else { length(v) },
                false,
            );
            tuned |= number(
                ui,
                "Air control",
                "airControl",
                &mut t.air_control,
                0.05,
                &|v| format!("{}%", (v * 100.0).round()),
                false,
            );
            let gravity = |v: f64| {
                if (v - 0.17).abs() < 0.02 {
                    "Moon".to_string()
                } else if (v - 1.0).abs() < 0.02 {
                    "Earth".into()
                } else if (v - 2.5).abs() < 0.05 {
                    "Jupiter".into()
                } else {
                    times(v)
                }
            };
            tuned |= number(ui, "Gravity", "gravity", &mut t.gravity, 0.01, &gravity, true);
            tuned |= number(
                ui,
                "Wall bounce",
                "bounce",
                &mut t.bounce,
                0.05,
                &|v| format!("{}%", (v * 100.0).round()),
                false,
            );
            hint(
                ui,
                "Press J to jump (when jump height is above zero). Air control lets you steer and boost in the air.",
            );
        }
        Tab::Settings => {
            settings_tab(ui, s, out);
            return;
        }
        Tab::Packs => {
            if crate::packs::packs_tab(ui, packs) {
                out.push(GarageAction::PacksChanged);
            }
            return;
        }
    }
    if look {
        out.push(GarageAction::Look);
    }
    if tuned {
        out.push(GarageAction::Tuning);
    }
}

fn builds(ui: &mut egui::Ui, g: &mut Garage, c: &mut CarConfig, storage: &mut Storage, out: &mut Vec<GarageAction>) {
    heading(ui, "Showcase builds");
    for b in SHOWCASE_BUILDS {
        let r = egui::Frame::NONE
            .fill(Color32::from_white_alpha(12))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(b.name).strong());
                ui.label(RichText::new(b.about).size(12.0).color(MUTED));
            })
            .response
            .interact(egui::Sense::click());
        if r.clicked() {
            *c = showcase_config(b);
            out.extend([GarageAction::Look, GarageAction::Tuning]);
        }
        ui.add_space(3.0);
    }
    heading(ui, "Your builds");
    let mut list = load_builds(storage);
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut g.build_name)
                .hint_text("Name this car")
                .char_limit(30)
                .desired_width(200.0),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if button(ui, "Save", false, true) || enter {
            let name = if g.build_name.trim().is_empty() {
                format!("Build {}", list.len() + 1)
            } else {
                g.build_name.trim().to_string()
            };
            list.insert(0, SavedBuild { name, config: c.clone() });
            list.truncate(30);
            save_builds(storage, &list);
            g.build_name.clear();
        }
    });
    if list.is_empty() {
        hint(ui, "Saved cars show up here.");
    }
    let mut remove = None;
    for (i, b) in list.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(&b.name);
            if button(ui, "Load", false, true) {
                *c = b.config.clone();
                out.extend([GarageAction::Look, GarageAction::Tuning]);
            }
            if button(ui, "Delete", false, true) {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        list.remove(i);
        save_builds(storage, &list);
    }
    heading(ui, "Share code");
    let mut code = share_code(c);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut code)
                .desired_width(260.0)
                .font(egui::TextStyle::Monospace),
        );
        if button(ui, if g.copied { "Copied" } else { "Copy" }, false, true) {
            ui.ctx().copy_text(code.clone());
            g.copied = true;
        }
    });
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut g.paste)
                .hint_text("Paste a code: CAR1-…")
                .desired_width(260.0),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if button(ui, "Load", false, true) || enter {
            match read_share_code(&g.paste) {
                Some(car) => {
                    *c = car;
                    g.message.clear();
                    out.extend([GarageAction::Look, GarageAction::Tuning]);
                }
                None => g.message = "That isn't a car code. They start with CAR1-.".into(),
            }
        }
    });
    if !g.message.is_empty() {
        hint(ui, &g.message);
    }
}

fn settings_tab(ui: &mut egui::Ui, s: &mut Settings, out: &mut Vec<GarageAction>) {
    let mut changed = false;
    ui.label(RichText::new("Quality preset").size(13.0).color(MUTED));
    ui.horizontal(|ui| {
        for p in PRESETS {
            if ui.selectable_label(s.preset() == Some(p.0), p.0).clicked() {
                s.apply_preset(p.0);
                changed = true;
            }
        }
    });
    changed |= slider(
        ui,
        "Resolution",
        &mut s.resolution,
        0.5,
        1.0,
        0.05,
        &|v| format!("{}%", (v * 100.0).round()),
        false,
    );
    let pick = |ui: &mut egui::Ui, label: &str, options: Options, value: &mut String| choice(ui, label, options, value);
    changed |= pick(
        ui,
        "Shadows",
        &[("off", "Off"), ("low", "Low"), ("high", "High"), ("ultra", "Ultra")],
        &mut s.shadows,
    );
    changed |= slider(
        ui,
        "View distance",
        &mut s.view_distance,
        150.0,
        600.0,
        10.0,
        &|v| format!("{v} m"),
        false,
    );
    changed |= pick(
        ui,
        "Explosion effects",
        &[("low", "Low"), ("medium", "Medium"), ("high", "High")],
        &mut s.effects,
    );
    let mut fps = format!("{}", s.fps_limit);
    if pick(
        ui,
        "Frame rate limit",
        &[("0", "None"), ("30", "30"), ("60", "60"), ("120", "120")],
        &mut fps,
    ) {
        s.fps_limit = fps.parse().unwrap_or(0.0);
        changed = true;
    }
    changed |= on_off(ui, "Anti-aliasing", &mut s.antialias);
    changed |= on_off(ui, "FPS counter", &mut s.show_fps);
    changed |= slider(ui, "Field of view", &mut s.fov, 50.0, 90.0, 1.0, &|v| format!("{v}°"), false);
    changed |= pick(ui, "Units", &[("mph", "mph and feet"), ("kmh", "km/h and meters")], &mut s.units);
    changed |= pick(
        ui,
        "Gearbox",
        &[("auto", "Automatic"), ("manual", "Manual (X / Z)")],
        &mut s.transmission,
    );
    changed |= on_off(ui, "Weapons", &mut s.weapons);
    changed |= on_off(ui, "Minimap", &mut s.minimap);
    changed |= pick(
        ui,
        "City traffic",
        &[("off", "Off"), ("light", "Light"), ("normal", "Normal"), ("busy", "Busy")],
        &mut s.traffic,
    );
    changed |= on_off(ui, "Pedestrians", &mut s.pedestrians);
    changed |= on_off(ui, "Music", &mut s.music);
    changed |= on_off(ui, "Announcer", &mut s.announcer);
    changed |= on_off(ui, "Speed lines", &mut s.speed_lines);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Destroyed buildings").size(13.0).color(MUTED));
        if button(ui, "Rebuild everything", false, true) {
            out.push(GarageAction::RebuildCity);
        }
    });
    hint(
        ui,
        "If the game feels slow, try the Low or Medium preset and turn on the FPS counter to see the difference.",
    );
    if changed {
        out.push(GarageAction::SettingsChanged);
    }
}
