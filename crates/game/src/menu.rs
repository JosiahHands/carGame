//! The menus: the main menu, the career (levels, cars and upgrades, the daily
//! challenge, trophies, stats), quick race setup, the prize wheel, and the
//! screens over the game: pause, results, and the end of the career.
use bevy_egui::egui::{self, Color32, RichText};

use crate::career::*;
use crate::config::{CarConfig, SHOWCASE_BUILDS, Tuning, showcase_config};
use crate::event::EventResult;
use crate::rewards::{PRIZES, WRENCHES, ZONE_STARS, driver_level, spin_wheel, trophies};
use crate::spec::{EventKind, EventSpec};
use crate::text::{format_time, thousands};
use crate::venues::{self, Weather, venues};

pub const GOLD: Color32 = Color32::from_rgb(255, 210, 63);
pub const ACCENT: Color32 = Color32::from_rgb(255, 90, 54);
pub const MUTED: Color32 = Color32::from_rgb(160, 168, 184);

/// What the menus ask the game to do.
#[derive(Clone, Debug)]
pub enum MenuAction {
    PlayLevel(Box<Level>),
    PlayQuick(Box<EventSpec>, Box<CarConfig>),
    FreeRoam,
    FreeRoamCareer,
    Garage,
    /// The career changed (something bought or chosen): save it and show the car.
    CarChanged,
    Resume,
    Restart,
    Quit,
    ChooseSandbox,
    ChooseReset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CareerTab {
    Levels,
    Garage,
    Daily,
    Trophies,
    Stats,
}

#[derive(Clone, Debug)]
pub enum Screen {
    Hidden,
    Main,
    Wheel,
    Career,
    Level(Box<Level>, String),
    Quick,
    Pause(String),
    Results {
        spec: Box<EventSpec>,
        result: Box<EventResult>,
        payout: Option<Payout>,
        next: Option<Box<Level>>,
        extras: Vec<String>,
    },
    CareerComplete,
    ConfirmReset,
}

#[derive(Clone, Debug)]
pub struct Quick {
    pub kind: EventKind,
    pub map: String,
    pub layout: u32,
    pub laps: u32,
    pub rivals: usize,
    pub difficulty: usize,
    pub car: String,
    pub night: bool,
    pub weapons: bool,
    pub rival_top_speed: f64,
}

pub struct Menu {
    pub screen: Screen,
    pub tab: CareerTab,
    pub chapter: usize,
    pub quick: Quick,
    /// The prize wheel: its angle (degrees), where it's heading, when the spin started, and what was won.
    wheel: (f64, f64, f64, String),
}

impl Menu {
    pub fn new(save: &CareerSave) -> Self {
        Menu {
            screen: Screen::Hidden,
            tab: CareerTab::Levels,
            chapter: 10.min(next_level(save).div_ceil(10)).max(1),
            quick: Quick {
                kind: EventKind::Race,
                map: "circuit".into(),
                layout: 0,
                laps: 3,
                rivals: 5,
                difficulty: 1,
                car: "garage".into(),
                night: false,
                weapons: false,
                rival_top_speed: 0.0,
            },
            wheel: (0.0, 0.0, -10.0, String::new()),
        }
    }

    pub fn visible(&self) -> bool {
        !matches!(self.screen, Screen::Hidden)
    }

    /// The menu is over a running event (pause, results): the game is behind it.
    pub fn is_overlay(&self) -> bool {
        matches!(self.screen, Screen::Pause(_) | Screen::Results { .. } | Screen::CareerComplete)
    }
}

/// A Ui covering the whole window, for the side panels.
pub fn root_ui(ctx: &egui::Context, id: &'static str) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        id.into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    )
}

pub fn big_button(ui: &mut egui::Ui, label: &str, sub: &str, primary: bool) -> bool {
    let fill = if primary {
        Color32::from_rgb(200, 60, 40)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 18)
    };
    let text = egui::text::LayoutJob::default();
    let _ = text;
    let response = egui::Frame::NONE
        .fill(fill)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(label).size(19.0).strong().color(Color32::WHITE));
            if !sub.is_empty() {
                ui.label(RichText::new(sub).size(13.0).color(Color32::from_gray(215)));
            }
        })
        .response
        .interact(egui::Sense::click());
    let clicked = response.clicked();
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(6.0);
    clicked
}

pub fn button(ui: &mut egui::Ui, text: &str, primary: bool, enabled: bool) -> bool {
    let mut b = egui::Button::new(RichText::new(text).size(15.0).color(Color32::WHITE))
        .corner_radius(8.0)
        .min_size(egui::vec2(0.0, 30.0));
    if primary {
        b = b.fill(Color32::from_rgb(200, 60, 40));
    }
    ui.add_enabled(enabled, b).clicked()
}

/// A row of choices; returns the one clicked.
pub fn choices<T: PartialEq + Clone>(ui: &mut egui::Ui, label: &str, options: &[(T, String)], current: &T) -> Option<T> {
    let mut picked = None;
    ui.label(RichText::new(label).size(13.0).color(MUTED));
    ui.horizontal_wrapped(|ui| {
        for (value, text) in options {
            if ui.selectable_label(value == current, RichText::new(text).size(14.0)).clicked() {
                picked = Some(value.clone());
            }
        }
    });
    ui.add_space(6.0);
    picked
}

fn stars(n: u32) -> String {
    format!("{}{}", "★".repeat(n as usize), "☆".repeat(3 - n.min(3) as usize))
}

fn speed_text(mph: f64, mph_units: bool) -> String {
    if mph_units {
        format!("{} mph", mph.round())
    } else {
        format!("{} km/h", (mph * 1.609).round())
    }
}

/// The menu panel for this frame. Returns what the game should do.
pub fn show(ctx: &egui::Context, menu: &mut Menu, save: &mut CareerSave, looks: &CarConfig, mph: bool, clock: f64) -> Vec<MenuAction> {
    let mut out = Vec::new();
    match menu.screen.clone() {
        Screen::Hidden => {}
        Screen::Main | Screen::Career | Screen::Level(..) | Screen::Quick => {
            let mut root = root_ui(ctx, "menu-root");
            egui::Panel::left("menu")
                .exact_size(430.0)
                .resizable(false)
                .frame(
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(10, 14, 22, 225))
                        .inner_margin(egui::Margin::same(18)),
                )
                .show(&mut root, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match menu.screen.clone() {
                            Screen::Main => main_menu(ui, menu, save, &mut out),
                            Screen::Career => career(ui, menu, save, mph, &mut out),
                            Screen::Level(level, locked) => level_screen(ui, menu, save, &level, &locked, &mut out),
                            Screen::Quick => quick(ui, menu, save, looks, mph, &mut out),
                            _ => {}
                        });
                });
        }
        _ => {
            egui::Area::new(egui::Id::new("overlay-dim"))
                .fixed_pos(egui::pos2(0.0, 0.0))
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    ui.painter().rect_filled(ctx.viewport_rect(), 0.0, Color32::from_black_alpha(140));
                });
            egui::Window::new("overlay")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .frame(
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(14, 18, 28, 240))
                        .corner_radius(14.0)
                        .inner_margin(egui::Margin::same(22)),
                )
                .show(ctx, |ui| {
                    ui.set_width(440.0);
                    match menu.screen.clone() {
                        Screen::Wheel => wheel(ui, menu, save, clock, &mut out),
                        Screen::Pause(title) => {
                            ui.vertical_centered(|ui| {
                                ui.heading(RichText::new("Paused").size(28.0).strong());
                                ui.label(RichText::new(title).color(MUTED));
                                ui.add_space(12.0);
                                if big_button(ui, "Resume", "", true) {
                                    out.push(MenuAction::Resume);
                                }
                                if big_button(ui, "Restart", "", false) {
                                    out.push(MenuAction::Restart);
                                }
                                if big_button(ui, "Quit to menu", "", false) {
                                    out.push(MenuAction::Quit);
                                }
                            });
                        }
                        Screen::Results {
                            spec,
                            result,
                            payout,
                            next,
                            extras,
                        } => results(ui, menu, save, &spec, &result, payout.as_ref(), next.as_deref(), &extras, &mut out),
                        Screen::CareerComplete => {
                            ui.vertical_centered(|ui| {
                                ui.heading(RichText::new("🏆 CAREER COMPLETE! 🏆").size(26.0).strong().color(GOLD));
                                ui.label(format!("You beat The Phantom and all {LEVELS} levels. Legend!"));
                                ui.label(RichText::new("What now?").color(MUTED));
                            });
                            ui.add_space(10.0);
                            if big_button(
                                ui,
                                "∞ Sandbox",
                                "Keep your car, coins and upgrades. Endless levels that get harder every time. Upgrades go up to 20.",
                                true,
                            ) {
                                out.push(MenuAction::ChooseSandbox);
                            }
                            let more = ((prestige(save) + 0.25 - 1.0) * 100.0).round();
                            if big_button(
                                ui,
                                "↺ Reset",
                                &format!("Start again from level 1 with nothing, but every coin pays {more}% more."),
                                false,
                            ) {
                                out.push(MenuAction::ChooseReset);
                            }
                        }
                        Screen::ConfirmReset => {
                            ui.heading("Reset your career?");
                            ui.label("Coins, cars, upgrades, weapons and stars will be lost.");
                            ui.horizontal(|ui| {
                                if button(ui, "Reset", true, true) {
                                    *save = reset_career(save, save.stars[LEVELS - 1] > 0);
                                    menu.chapter = 1;
                                    menu.tab = CareerTab::Levels;
                                    menu.screen = Screen::Career;
                                    out.push(MenuAction::CarChanged);
                                }
                                if button(ui, "Cancel", false, true) {
                                    menu.screen = Screen::Career;
                                }
                            });
                        }
                        _ => {}
                    }
                });
        }
    }
    out
}

fn main_menu(ui: &mut egui::Ui, menu: &mut Menu, save: &mut CareerSave, out: &mut Vec<MenuAction>) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("CAR GAME").size(44.0).strong().color(Color32::WHITE));
        ui.label(RichText::new("Race. Drift. Smash. Upgrade. Repeat.").color(MUTED));
    });
    ui.add_space(10.0);
    driver_card(ui, save);
    ui.add_space(8.0);
    if save.spins > 0
        && big_button(
            ui,
            &format!("🎰 Prize wheel × {}", save.spins),
            "You leveled up! Spin for coins, upgrades and coin rushes",
            true,
        )
    {
        menu.wheel.3.clear();
        menu.screen = Screen::Wheel;
    }
    let waiting = awaiting_choice(save);
    if waiting && big_button(ui, "🏆 Career complete!", "Choose what happens next: Sandbox or Reset", true) {
        menu.screen = Screen::CareerComplete;
    }
    let level = if save.mode == "sandbox" {
        format!("Sandbox level {}", save.sandbox_level)
    } else {
        format!("Level {} of {LEVELS}", next_level(save))
    };
    if big_button(
        ui,
        "🏆 Career",
        &format!("{level} · {} ★ · 💰 {}", total_stars(save), thousands(save.coins)),
        !waiting && save.spins == 0,
    ) {
        menu.screen = Screen::Career;
        out.push(MenuAction::CarChanged);
    }
    if big_button(ui, "🏁 Quick race", "Any car, any track, any difficulty", false) {
        menu.screen = Screen::Quick;
    }
    if big_button(
        ui,
        "🗺 Free roam",
        &format!(
            "Your garage build. Weapons, stunts, time trials · 🔧 {}/{WRENCHES} gold wrenches",
            save.found.len()
        ),
        false,
    ) {
        out.push(MenuAction::FreeRoam);
    }
    let zone_stars: f64 = save.zones.values().sum::<f64>() + 0.0;
    if big_button(
        ui,
        "🚗 Free roam in your career car",
        &format!("Speed cameras, drift and jump zones · {zone_stars}/{ZONE_STARS} ★"),
        false,
    ) {
        out.push(MenuAction::FreeRoamCareer);
    }
    if big_button(ui, "🔧 Garage & settings", "Customize your car and the game", false) {
        out.push(MenuAction::Garage);
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(if save.daily == today() {
            "Daily challenge done ✔"
        } else {
            "🎁 A new daily challenge is waiting"
        })
        .color(MUTED),
    );
    ui.label(RichText::new(format!("Version {}", crate::VERSION)).size(11.0).color(MUTED));
}

fn driver_card(ui: &mut egui::Ui, save: &CareerSave) {
    let (level, into, need) = driver_level(save.xp);
    ui.label(RichText::new(format!("Driver level {level}")).strong().size(16.0));
    ui.add(egui::ProgressBar::new((into / need) as f32).desired_height(8.0).fill(GOLD));
    let mut extra = vec![format!("🏅 {}/{} trophies", save.trophies.len(), trophies().len())];
    if save.coin_rush > 0 {
        extra.push(format!("⚡ Coin rush × {}", save.coin_rush));
    }
    ui.label(
        RichText::new(format!("{} / {} XP · {}", thousands(into), thousands(need), extra.join(" · ")))
            .size(12.0)
            .color(MUTED),
    );
}

fn wheel(ui: &mut egui::Ui, menu: &mut Menu, save: &mut CareerSave, clock: f64, out: &mut Vec<MenuAction>) {
    ui.vertical_centered(|ui| {
        ui.heading(RichText::new("🎰 Prize wheel").size(26.0).color(GOLD));
        ui.label(RichText::new("You earn a spin every time your driver level goes up.").color(MUTED));
    });
    let (angle, target, started, won) = menu.wheel.clone();
    let spinning = clock - started < 4.2;
    // Ease out over 4.2 seconds.
    let t = ((clock - started) / 4.2).clamp(0.0, 1.0);
    let shown = if spinning {
        angle + (target - angle) * (1.0 - (1.0 - t).powi(3))
    } else {
        target.max(angle)
    };
    let size = 260.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), size + 20.0), egui::Sense::hover());
    let center = egui::pos2(rect.center().x, rect.top() + 10.0 + size / 2.0);
    let painter = ui.painter();
    let slice = 360.0 / PRIZES.len() as f64;
    let radius = size / 2.0;
    for (i, p) in PRIZES.iter().enumerate() {
        let c = crate::canvas::Rgba::css(p.color);
        let color = Color32::from_rgb((c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8);
        let mut pts = vec![center];
        for k in 0..=12 {
            let a = ((i as f64 * slice + slice * k as f64 / 12.0 + shown - 90.0).to_radians()) as f32;
            pts.push(center + egui::vec2(a.cos(), a.sin()) * radius);
        }
        // Each slice as a fan of triangles (they're convex).
        for k in 1..pts.len() - 1 {
            painter.add(egui::Shape::convex_polygon(
                vec![pts[0], pts[k], pts[k + 1]],
                color,
                egui::Stroke::NONE,
            ));
        }
        let mid = ((i as f64 * slice + slice / 2.0 + shown - 90.0).to_radians()) as f32;
        painter.text(
            center + egui::vec2(mid.cos(), mid.sin()) * radius * 0.66,
            egui::Align2::CENTER_CENTER,
            p.label,
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
    painter.circle_stroke(center, radius, egui::Stroke::new(3.0, Color32::WHITE));
    painter.text(
        egui::pos2(center.x, center.y - radius - 4.0),
        egui::Align2::CENTER_CENTER,
        "▼",
        egui::FontId::proportional(24.0),
        Color32::WHITE,
    );
    ui.vertical_centered(|ui| {
        let text = if spinning {
            "Spinning…".to_string()
        } else if won.is_empty() {
            format!("Spins left: {}", save.spins)
        } else {
            won.clone()
        };
        ui.label(
            RichText::new(text)
                .size(17.0)
                .strong()
                .color(if !spinning && !won.is_empty() { GOLD } else { Color32::WHITE }),
        );
        ui.horizontal(|ui| {
            let label = if !won.is_empty() && save.spins > 0 {
                format!("SPIN AGAIN ({})", save.spins)
            } else if save.spins > 0 {
                "SPIN!".into()
            } else {
                "No spins left".into()
            };
            if button(ui, &label, true, !spinning && save.spins > 0) {
                let (index, text) = spin_wheel(save);
                let center_angle = index as f64 * slice + slice / 2.0;
                let jitter = crate::rng::centered() * slice * 0.6;
                let base = shown.rem_euclid(360.0);
                let to = shown + 360.0 * 5.0 + ((360.0 - center_angle - base) + 720.0).rem_euclid(360.0) + jitter;
                menu.wheel = (shown, to, clock, text);
                out.push(MenuAction::CarChanged);
            }
            if button(ui, "Done", false, !spinning) {
                menu.screen = Screen::Main;
            }
        });
    });
    ui.ctx().request_repaint();
}

fn career(ui: &mut egui::Ui, menu: &mut Menu, save: &mut CareerSave, mph: bool, out: &mut Vec<MenuAction>) {
    ui.horizontal(|ui| {
        if button(ui, "◀", false, true) {
            menu.screen = Screen::Main;
        }
        ui.heading(RichText::new(if save.mode == "sandbox" { "Career · Sandbox" } else { "Career" }).strong());
    });
    if save.completions > 0 {
        ui.label(RichText::new(format!("Prestige {} · coins ×{:.2}", save.completions, prestige(save))).color(GOLD));
    }
    ui.label(
        RichText::new(format!("💰 {}   ★ {} / {}", thousands(save.coins), total_stars(save), LEVELS * 3))
            .size(16.0)
            .strong(),
    );
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in [
            (CareerTab::Levels, "Levels"),
            (CareerTab::Garage, "Cars & upgrades"),
            (CareerTab::Daily, "Daily"),
            (CareerTab::Trophies, "Trophies"),
            (CareerTab::Stats, "Stats"),
        ] {
            if ui.selectable_label(menu.tab == tab, RichText::new(label).size(15.0)).clicked() {
                menu.tab = tab;
            }
        }
    });
    ui.separator();
    match menu.tab {
        CareerTab::Levels => levels_tab(ui, menu, save),
        CareerTab::Garage => garage_tab(ui, save, mph, out),
        CareerTab::Daily => {
            let level = daily_level(save);
            let done = save.daily == today();
            ui.heading(format!(
                "{} Today: {} at {}",
                level.spec.kind.icon(),
                level.spec.kind.label(),
                venues::venue(&level.spec.map).label
            ));
            ui.label(RichText::new("A new challenge every day, pitched at how far you have got. It pays double, and a streak of days in a row pays up to twice as much again.").color(MUTED));
            let days = if save.streak == 1 { "day" } else { "days" };
            ui.label(if done {
                format!("✔ Done today. Come back tomorrow! Streak: {} {days}.", save.streak)
            } else {
                format!("Streak: {} {days}", save.streak)
            });
            if button(ui, if done { "Play again (no reward)" } else { "Play ▶" }, true, true) {
                menu.screen = Screen::Level(Box::new(level), String::new());
            }
        }
        CareerTab::Trophies => {
            let list = trophies();
            ui.label(RichText::new(format!("{} of {} won. Each one pays coins.", save.trophies.len(), list.len())).color(MUTED));
            let mut sorted: Vec<&crate::rewards::Trophy> = list.iter().collect();
            let weight = |t: &crate::rewards::Trophy| {
                if save.trophies.contains(&t.id) {
                    2.0
                } else {
                    ((t.value)(save) / t.need).min(1.0)
                }
            };
            sorted.sort_by(|a, b| weight(b).total_cmp(&weight(a)));
            for t in sorted {
                let won = save.trophies.contains(&t.id);
                let value = (t.value)(save).floor().min(t.need);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&t.icon).size(22.0));
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&t.name).strong().color(if won { GOLD } else { Color32::WHITE }));
                        ui.label(RichText::new(&t.about).size(12.0).color(MUTED));
                        ui.add(
                            egui::ProgressBar::new((value / t.need) as f32)
                                .desired_height(5.0)
                                .desired_width(220.0),
                        );
                        let reward = if won {
                            format!("Won · 💰 {}", thousands(t.reward))
                        } else {
                            format!("{} / {} · 💰 {}", thousands(value), thousands(t.need), thousands(t.reward))
                        };
                        ui.label(RichText::new(reward).size(12.0));
                    });
                });
                ui.add_space(4.0);
            }
        }
        CareerTab::Stats => stats_tab(ui, menu, save),
    }
}

fn levels_tab(ui: &mut egui::Ui, menu: &mut Menu, save: &CareerSave) {
    egui::Grid::new("chapters").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
        for (i, ch) in CHAPTERS.iter().enumerate() {
            let n = i + 1;
            let open = locked_reason(save, (n - 1) * 10 + 1).is_empty();
            let stars: u32 = (0..10).map(|k| save.stars[i * 10 + k]).sum();
            let label = format!("{} {}  {stars}/30 ★", if open { n.to_string() } else { "🔒".into() }, ch.name);
            if ui.selectable_label(menu.chapter == n, RichText::new(label).size(13.0)).clicked() {
                menu.chapter = n;
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
        if save.mode == "sandbox"
            && ui
                .selectable_label(
                    menu.chapter == 11,
                    RichText::new(format!("∞ Sandbox  Lv {}", save.sandbox_level)).size(13.0),
                )
                .clicked()
        {
            menu.chapter = 11;
        }
    });
    ui.add_space(8.0);
    let tile = |ui: &mut egui::Ui, menu: &mut Menu, level: Level, stars_won: u32, best: String, locked: String| {
        let spec = &level.spec;
        let venue = venues::venue(&spec.map);
        let kind = if level.is_boss {
            spec.boss.clone().unwrap_or("Boss".into())
        } else {
            spec.kind.label().into()
        };
        let night = if spec.night && spec.map != "neon" { " · night" } else { "" };
        let icon = if locked.is_empty() { spec.kind.icon() } else { "🔒" };
        let title = format!("{}  {icon} {kind}", level.number);
        let fill = if level.is_boss {
            Color32::from_rgba_unmultiplied(120, 90, 20, 120)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 14)
        };
        let r = egui::Frame::NONE
            .fill(fill)
            .corner_radius(8.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(title)
                            .strong()
                            .color(if locked.is_empty() { Color32::WHITE } else { MUTED }),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(stars(stars_won)).color(GOLD));
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "{}{night}{}",
                        venue.label,
                        if best.is_empty() {
                            String::new()
                        } else {
                            format!(" · best {best}")
                        }
                    ))
                    .size(12.0)
                    .color(MUTED),
                );
            })
            .response
            .interact(egui::Sense::click());
        if r.clicked() {
            menu.screen = Screen::Level(Box::new(level), locked);
        }
        ui.add_space(3.0);
    };
    if menu.chapter == 11 {
        ui.heading("Sandbox: endless levels");
        ui.label(RichText::new("Every level is a surprise, and each one is harder than the last: smarter rivals, faster cars, tougher targets. Upgrades go up to level 20 here. How far can you go?").color(MUTED));
        let first = (LEVELS + 1).max(save.sandbox_level.saturating_sub(6));
        for n in first..=save.sandbox_level {
            let done = n < save.sandbox_level;
            tile(
                ui,
                menu,
                make_level(n),
                if done { 3 } else { 0 },
                if done { "Cleared".into() } else { "Next".into() },
                String::new(),
            );
        }
        return;
    }
    let ch = &CHAPTERS[menu.chapter - 1];
    ui.heading(format!("Chapter {}: {}", menu.chapter, ch.name));
    ui.label(RichText::new(ch.about).color(MUTED));
    let have = chapter_stars(save, menu.chapter);
    ui.label(
        RichText::new(format!(
            "Boss: {} · needs {BOSS_STARS} ★ from levels {}-{} (you have {have})",
            ch.boss,
            (menu.chapter - 1) * 10 + 1,
            menu.chapter * 10 - 1
        ))
        .size(12.0)
        .color(MUTED),
    );
    ui.add_space(4.0);
    for i in 0..10 {
        let n = (menu.chapter - 1) * 10 + i + 1;
        tile(
            ui,
            menu,
            make_level(n),
            save.stars[n - 1],
            save.best[n - 1].clone(),
            locked_reason(save, n),
        );
    }
}

/// What each star needs.
pub fn targets_text(spec: &EventSpec, started: bool) -> String {
    let t = spec.targets;
    match spec.kind {
        EventKind::Race | EventKind::Elimination => "★ 3rd · ★★ 2nd · ★★★ 1st".into(),
        EventKind::Duel => "★★★ win".into(),
        EventKind::TimeAttack => {
            if started {
                format!("★ {} · ★★ {} · ★★★ {}", format_time(t[0]), format_time(t[1]), format_time(t[2]))
            } else {
                "Target times are set by a pace car at the start".into()
            }
        }
        EventKind::SpeedTrap => {
            if started {
                format!("★ {} mph · ★★ {} mph · ★★★ {} mph", t[0], t[1], t[2])
            } else {
                "Target speeds are set by a pace car at the start".into()
            }
        }
        kind => {
            let f = |v: f64| match kind {
                EventKind::Destruction => format!("${}", thousands(v)),
                EventKind::Stunt => format!("{v} s"),
                EventKind::Takedown => format!("{v}"),
                _ => thousands(v),
            };
            format!("★ {} · ★★ {} · ★★★ {}", f(t[0]), f(t[1]), f(t[2]))
        }
    }
}

fn rival_car_name(config: &CarConfig) -> String {
    match CHASSIS.iter().find(|c| c.body == config.body) {
        Some(c) => {
            let name = c.name.to_lowercase();
            if name.ends_with('h') {
                format!("{name}es")
            } else {
                format!("{name}s")
            }
        }
        None => "cars".into(),
    }
}

fn level_screen(ui: &mut egui::Ui, menu: &mut Menu, save: &CareerSave, level: &Level, locked: &str, out: &mut Vec<MenuAction>) {
    let spec = &level.spec;
    let kind = spec.kind;
    ui.horizontal(|ui| {
        if button(ui, "◀", false, true) {
            if level.number == 0 {
                menu.tab = CareerTab::Daily;
            }
            menu.screen = Screen::Career;
        }
        ui.heading(RichText::new(format!("{} {}", kind.icon(), spec.title)).strong());
    });
    ui.label(
        RichText::new(if level.number > 0 {
            format!("Level {}", level.number)
        } else {
            "Daily challenge".into()
        })
        .color(GOLD),
    );
    ui.add_space(6.0);
    ui.label(if level.is_boss {
        format!("{} is waiting. {}", spec.boss.clone().unwrap_or_default(), kind.rules())
    } else {
        kind.rules().into()
    });
    ui.add_space(6.0);
    let venue = venues::venue(&spec.map);
    egui::Grid::new("facts").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let mut fact = |k: &str, v: String| {
            ui.label(RichText::new(k).color(MUTED));
            ui.label(v);
            ui.end_row();
        };
        fact("Event", kind.label().into());
        let track = if spec.map == "city" {
            String::new()
        } else if spec.layout == 0 {
            " · main track".into()
        } else {
            format!(" · track #{}", spec.layout)
        };
        fact("Where", format!("{}{track}", venue.label));
        match kind {
            EventKind::Elimination => fact("Knockouts", format!("last place out every {} s", spec.time_limit)),
            EventKind::Race | EventKind::Duel | EventKind::TimeAttack | EventKind::SpeedTrap => fact("Laps", spec.laps.to_string()),
            _ => fact("Time limit", format_time(spec.time_limit)),
        }
        if let Some(last) = spec.rivals.last() {
            let cars = if last.config.body == "wedge" {
                "supercars".to_string()
            } else {
                rival_car_name(&last.config)
            };
            fact("Rivals", format!("{} · {cars}", spec.rivals.len()));
        }
        let mut twists = Vec::new();
        if spec.night {
            twists.push("🌙 Night".to_string());
        }
        if spec.no_nitro {
            twists.push("🚫 No nitro".into());
        }
        if spec.weapons && kind == EventKind::Race {
            twists.push("🔫 Combat race: weapons on".into());
        }
        match venue.theme.weather {
            Weather::Rain => twists.push("☔ Rain".into()),
            Weather::Snow => twists.push("❄ Snow".into()),
            Weather::Clear => {}
        }
        if !twists.is_empty() {
            fact("Twists", twists.join(" · "));
        }
        if spec.weapons {
            let mine: Vec<String> = WEAPON_SHOP
                .iter()
                .filter(|w| save.arsenal.get(w.0).copied().unwrap_or(0) > 0)
                .map(|w| format!("{} {} {}", w.3, w.1, save.arsenal[w.0]))
                .collect();
            fact(
                "Your weapons",
                if mine.is_empty() {
                    "None yet: ram them! Buy weapons under Cars & upgrades".into()
                } else {
                    mine.join(" · ")
                },
            );
        }
        fact("Stars", targets_text(spec, false));
        let best = if level.number == 0 { 1.0 } else { 1.0 + 0.5 + 0.6 };
        fact(
            "Reward",
            format!("up to 💰 {}", thousands((level.reward * best * prestige(save)).round())),
        );
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!(
            "Your car: {}. Change it or upgrade it under Cars & upgrades.",
            CHASSIS[save.car].name
        ))
        .color(MUTED),
    );
    ui.add_space(10.0);
    if !locked.is_empty() {
        ui.label(RichText::new(format!("🔒 {locked}")).color(ACCENT));
    } else if big_button(ui, "Start ▶", "", true) {
        out.push(MenuAction::PlayLevel(Box::new(level.clone())));
    }
    if button(ui, "Cars & upgrades", false, true) {
        menu.tab = CareerTab::Garage;
        menu.screen = Screen::Career;
    }
}

fn stat_bars(ui: &mut egui::Ui, t: &Tuning, compare: &Tuning, mph: bool) {
    let mut maxed = no_upgrades();
    for k in ["engine", "gearbox", "tires", "brakes", "aero"] {
        maxed.insert(k.into(), 10);
    }
    let max = career_tuning(CHASSIS.len() - 1, &maxed);
    let rows = [
        (
            "Power",
            t.horsepower,
            compare.horsepower,
            max.horsepower,
            format!("{} hp", t.horsepower.round()),
        ),
        (
            "Top speed",
            t.top_speed,
            compare.top_speed,
            max.top_speed,
            speed_text(t.top_speed, mph),
        ),
        (
            "Grip",
            t.grip + t.downforce * 0.05,
            compare.grip + compare.downforce * 0.05,
            max.grip + max.downforce * 0.05,
            format!("{:.2}×", t.grip),
        ),
        ("Brakes", t.brakes, compare.brakes, max.brakes, format!("{:.2}×", t.brakes)),
    ];
    for (label, value, other, top, text) in rows {
        ui.horizontal(|ui| {
            ui.add_sized([70.0, 14.0], egui::Label::new(RichText::new(label).size(12.0).color(MUTED)));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 8.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 4.0, Color32::from_white_alpha(30));
            let mut fill = rect;
            fill.set_width(rect.width() * (value / top).min(1.0) as f32);
            p.rect_filled(fill, 4.0, ACCENT);
            let x = rect.left() + rect.width() * (other / top).min(1.0) as f32;
            p.line_segment(
                [egui::pos2(x, rect.top() - 2.0), egui::pos2(x, rect.bottom() + 2.0)],
                egui::Stroke::new(2.0, Color32::WHITE),
            );
            ui.label(RichText::new(text).size(12.0));
        });
    }
}

fn garage_tab(ui: &mut egui::Ui, save: &mut CareerSave, mph: bool, out: &mut Vec<MenuAction>) {
    let current = career_tuning(save.car, &save.upgrades);
    ui.heading("Cars");
    for (i, c) in CHASSIS.iter().enumerate() {
        let owned = save.owned.contains(&i);
        egui::Frame::NONE
            .fill(if i == save.car {
                Color32::from_rgba_unmultiplied(200, 60, 40, 60)
            } else {
                Color32::from_white_alpha(12)
            })
            .corner_radius(8.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(c.name).strong().size(16.0));
                ui.label(RichText::new(c.perk).size(12.0).color(MUTED));
                stat_bars(ui, &career_tuning(i, &save.upgrades), &current, mph);
                if i == save.car {
                    ui.label(RichText::new("Driving").color(GOLD));
                } else if owned {
                    if button(ui, "Drive this", false, true) {
                        save.car = i;
                        out.push(MenuAction::CarChanged);
                    }
                } else if button(ui, &format!("Buy 💰 {}", thousands(c.price)), true, save.coins >= c.price) {
                    save.coins -= c.price;
                    save.owned.push(i);
                    save.car = i;
                    out.push(MenuAction::CarChanged);
                }
            });
        ui.add_space(4.0);
    }
    let cap = upgrade_cap(save);
    ui.heading("Upgrades");
    ui.label(
        RichText::new(format!(
            "Upgrades fit every car you own. Up to level {cap}{}.",
            if cap < 20 { " (20 in the sandbox)" } else { "" }
        ))
        .color(MUTED),
    );
    for u in UPGRADES {
        let level = save.upgrades.get(u.0).copied().unwrap_or(0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("{} {} · {level}", u.3, u.1)).strong());
                ui.label(RichText::new(u.2).size(12.0).color(MUTED));
                ui.label(
                    RichText::new(format!(
                        "{}{}",
                        "●".repeat(level as usize),
                        "○".repeat(cap.saturating_sub(level) as usize)
                    ))
                    .size(10.0)
                    .color(GOLD),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if level >= cap {
                    ui.label("MAX");
                } else {
                    let cost = upgrade_cost(level);
                    if button(ui, &format!("💰 {}", thousands(cost)), true, save.coins >= cost) {
                        save.coins -= cost;
                        save.upgrades.insert(u.0.into(), level + 1);
                        out.push(MenuAction::CarChanged);
                    }
                }
            });
        });
    }
    ui.heading("Weapons");
    ui.label(
        RichText::new(format!(
            "Your car carries the weapons you buy, in events with weapons and in free roam. {WEAPON_UPGRADE_ABOUT}"
        ))
        .color(MUTED),
    );
    for w in WEAPON_SHOP {
        let level = save.arsenal.get(w.0).copied().unwrap_or(0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} {}{}",
                        w.3,
                        w.1,
                        if level > 0 { format!(" · {level}") } else { String::new() }
                    ))
                    .strong(),
                );
                ui.label(RichText::new(w.2).size(12.0).color(MUTED));
                ui.label(
                    RichText::new(format!(
                        "{}{}",
                        "●".repeat(level as usize),
                        "○".repeat((MAX_WEAPON_LEVEL - level) as usize)
                    ))
                    .size(10.0)
                    .color(GOLD),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let locked = weapon_locked_reason(save, w.0);
                if !locked.is_empty() && level == 0 {
                    ui.label(format!("🔒 {locked}"));
                } else if level >= MAX_WEAPON_LEVEL {
                    ui.label("MAX");
                } else {
                    let cost = weapon_cost(w.0, level);
                    if button(
                        ui,
                        &format!("{}💰 {}", if level > 0 { "" } else { "Buy " }, thousands(cost)),
                        true,
                        save.coins >= cost,
                    ) && buy_weapon(save, w.0)
                    {
                        out.push(MenuAction::CarChanged);
                    }
                }
            });
        });
    }
}

fn stats_tab(ui: &mut egui::Ui, menu: &mut Menu, save: &CareerSave) {
    let stat = |k: &str| thousands(save.stats.get(k).copied().unwrap_or(0.0));
    egui::Grid::new("stats").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let mut fact = |k: &str, v: String| {
            ui.label(RichText::new(k).color(MUTED));
            ui.label(v);
            ui.end_row();
        };
        fact("Driver level", format!("{} ({} XP)", driver_level(save.xp).0, thousands(save.xp)));
        fact(
            "Levels cleared",
            format!("{} / {LEVELS}", save.stars.iter().filter(|x| **x > 0).count()),
        );
        fact("Stars", format!("{} / {}", total_stars(save), LEVELS * 3));
        fact("Events", thousands(save.events as f64));
        fact("Wins", thousands(save.wins as f64));
        fact("Coins earned", thousands(save.earned));
        fact("Cars owned", format!("{} / {}", save.owned.len(), CHASSIS.len()));
        fact("Careers finished", save.completions.to_string());
        if save.sandbox_best > 0 {
            fact("Best sandbox level", save.sandbox_best.to_string());
        }
        fact("Best skill chain", stat("bestChain"));
        fact("Near misses", stat("nearMiss"));
        fact("Overtakes", stat("overtake"));
        fact("Takedowns", stat("takedown"));
        fact("Perfect starts", stat("perfectStart"));
        fact("Drift boosts", stat("driftBoost"));
        fact("Seconds in a slipstream", stat("draft"));
        fact("Seconds in the air", stat("air"));
        fact("Top speed in an event", format!("{} mph", stat("topSpeed")));
        fact("Gold wrenches", format!("{} / {WRENCHES}", save.found.len()));
        fact(
            "Free-roam challenge stars",
            format!("{} / {ZONE_STARS}", save.zones.values().sum::<f64>() + 0.0),
        );
        if let Some((name, wins)) = save.nemesis.iter().max_by(|a, b| a.1.total_cmp(b.1))
            && *wins >= 2.0
        {
            fact("Nemesis", format!("{name} (beat you {wins} times)"));
        }
    });
    ui.add_space(10.0);
    ui.heading("Start again");
    ui.label(RichText::new("Resetting wipes your coins, cars, upgrades, weapons and stars. If you have finished the career, it counts as a completion: coins pay 25% more for each one.").color(MUTED));
    if button(ui, "Reset career…", false, true) {
        menu.screen = Screen::ConfirmReset;
    }
}

/// Cars for a quick race: (id, label, car).
fn quick_cars(looks: &CarConfig, save: &CareerSave) -> Vec<(String, String, CarConfig)> {
    let mut maxed = no_upgrades();
    for k in UPGRADE_IDS {
        maxed.insert(k.into(), 10);
    }
    let mut out = vec![
        ("garage".to_string(), "🔧 My garage build".to_string(), looks.clone()),
        (
            "career".into(),
            "🏆 My career car".into(),
            career_car(looks, save.car, &save.upgrades),
        ),
    ];
    for (i, c) in CHASSIS.iter().enumerate() {
        out.push((format!("stock{i}"), c.name.into(), career_car(looks, i, &no_upgrades())));
        out.push((format!("max{i}"), format!("{} (maxed)", c.name), career_car(looks, i, &maxed)));
    }
    for (i, b) in SHOWCASE_BUILDS.iter().enumerate() {
        out.push((format!("show{i}"), format!("⭐ {}", b.name), showcase_config(b)));
    }
    out
}

fn quick(ui: &mut egui::Ui, menu: &mut Menu, save: &CareerSave, looks: &CarConfig, mph: bool, out: &mut Vec<MenuAction>) {
    ui.horizontal(|ui| {
        if button(ui, "◀", false, true) {
            menu.screen = Screen::Main;
        }
        ui.heading(RichText::new("🏁 Quick race").strong());
    });
    ui.add_space(6.0);
    let q = &mut menu.quick;
    let kinds: Vec<(EventKind, String)> = EventKind::ALL.iter().map(|k| (*k, format!("{} {}", k.icon(), k.label()))).collect();
    if let Some(k) = choices(ui, "Event", &kinds, &q.kind) {
        let city = matches!(k, EventKind::Destruction | EventKind::Stunt);
        if city != (q.map == "city") {
            q.map = if city { "city".into() } else { "circuit".into() };
        }
        q.kind = k;
    }
    ui.label(RichText::new(q.kind.rules()).size(12.0).color(MUTED));
    ui.add_space(4.0);
    let city = matches!(q.kind, EventKind::Destruction | EventKind::Stunt);
    let maps: Vec<(String, String)> = venues()
        .iter()
        .filter(|v| (v.value == "city") == city)
        .map(|v| (v.value.to_string(), v.label.to_string()))
        .collect();
    if let Some(m) = choices(ui, "Venue", &maps, &q.map) {
        q.map = m;
    }
    if !city {
        let random = if q.layout > 0 {
            format!("Track #{} 🎲", q.layout)
        } else {
            "New random track 🎲".into()
        };
        if let Some(l) = choices(
            ui,
            "Track",
            &[(0u32, "Main track".into()), (1u32, random)],
            &if q.layout > 0 { 1 } else { 0 },
        ) {
            q.layout = if l == 0 { 0 } else { 1 + crate::rng::index(9999) as u32 };
        }
    }
    if matches!(
        q.kind,
        EventKind::Race | EventKind::Duel | EventKind::TimeAttack | EventKind::SpeedTrap
    ) {
        let laps: Vec<(u32, String)> = [1, 2, 3, 4, 5, 7, 10].iter().map(|n| (*n, n.to_string())).collect();
        if let Some(v) = choices(ui, "Laps", &laps, &q.laps) {
            q.laps = v;
        }
    }
    if matches!(q.kind, EventKind::Race | EventKind::Elimination | EventKind::Takedown) {
        let rivals: Vec<(usize, String)> = (1..=9).map(|n| (n, n.to_string())).collect();
        if let Some(v) = choices(ui, "Rivals", &rivals, &q.rivals) {
            q.rivals = v;
        }
    }
    if !city && q.kind != EventKind::Drift {
        let levels: Vec<(usize, String)> = DIFFICULTIES.iter().enumerate().map(|(i, d)| (i, d.0.to_string())).collect();
        if let Some(v) = choices(ui, "Difficulty", &levels, &q.difficulty) {
            q.difficulty = v;
        }
    }
    let cars = quick_cars(looks, save);
    let options: Vec<(String, String)> = cars.iter().map(|c| (c.0.clone(), c.1.clone())).collect();
    if let Some(v) = choices(ui, "Your car", &options, &q.car) {
        q.car = v;
    }
    let rivals = RIVAL_SPEED_KINDS.contains(&q.kind);
    if rivals {
        let speeds: Vec<(i64, String)> = RIVAL_TOP_SPEEDS
            .iter()
            .map(|s| (*s as i64, if *s > 0.0 { speed_text(*s, mph) } else { "Same as my car".into() }))
            .collect();
        if let Some(v) = choices(ui, "Rivals' top speed", &speeds, &(q.rival_top_speed as i64)) {
            q.rival_top_speed = v as f64;
        }
    }
    if let Some(v) = choices(ui, "Time of day", &[(false, "☀ Day".into()), (true, "🌙 Night".into())], &q.night) {
        q.night = v;
    }
    if matches!(q.kind, EventKind::Race | EventKind::Duel | EventKind::Elimination)
        && let Some(v) = choices(
            ui,
            "Weapons",
            &[(false, "Off".into()), (true, "🔫 On (combat race)".into())],
            &q.weapons,
        )
    {
        q.weapons = v;
    }
    if rivals {
        ui.label(
            RichText::new(if q.rival_top_speed > 0.0 {
                "Rivals drive cars like yours with the top speed you picked. Difficulty sets how well they drive."
            } else {
                "Rivals drive cars as quick as yours, so any car makes a close race. Difficulty sets how well they drive."
            })
            .size(12.0)
            .color(MUTED),
        );
    }
    ui.add_space(8.0);
    if big_button(ui, "Start ▶", "", true) {
        let car = cars.iter().find(|c| c.0 == q.car).unwrap_or(&cars[0]).2.clone();
        let spec = quick_race_spec(&QuickRaceOptions {
            kind: q.kind,
            map: q.map.clone(),
            layout: if city { 0 } else { q.layout },
            laps: q.laps,
            rivals: q.rivals,
            difficulty: q.difficulty,
            car: car.clone(),
            rival_top_speed: if rivals { q.rival_top_speed } else { 0.0 },
            night: q.night,
            weapons: q.weapons,
        });
        out.push(MenuAction::PlayQuick(Box::new(spec), Box::new(car)));
    }
}

#[allow(clippy::too_many_arguments)]
fn results(
    ui: &mut egui::Ui,
    menu: &mut Menu,
    save: &CareerSave,
    spec: &EventSpec,
    r: &EventResult,
    payout: Option<&Payout>,
    next: Option<&Level>,
    extras: &[String],
    out: &mut Vec<MenuAction>,
) {
    let won = r.position == 1 || (r.position == 0 && r.stars == 3);
    let heading = if r.passed {
        if won {
            match &spec.boss {
                Some(b) => format!("{b} defeated!"),
                None => "Victory!".into(),
            }
        } else {
            "Finished!".into()
        }
    } else if spec.kind == EventKind::Elimination && r.position > 0 {
        "Knocked out!".into()
    } else {
        "Not this time".into()
    };
    ui.vertical_centered(|ui| {
        ui.heading(
            RichText::new(heading)
                .size(28.0)
                .strong()
                .color(if won { GOLD } else { Color32::WHITE }),
        );
        ui.label(RichText::new(&spec.title).color(MUTED));
        ui.label(RichText::new(stars(r.stars)).size(40.0).color(GOLD));
        ui.label(RichText::new(&r.score_text).size(22.0).strong());
        if r.best_lap > 0.0 {
            ui.label(RichText::new(format!("Best lap {}", format_time(r.best_lap))).color(MUTED));
        }
        if !matches!(spec.kind, EventKind::Race | EventKind::Elimination | EventKind::Duel) {
            ui.label(RichText::new(format!("Targets: {}", targets_text(spec, true))).color(MUTED));
        }
    });
    if r.standings.len() > 1 {
        ui.add_space(6.0);
        for (i, s) in r.standings.iter().enumerate() {
            ui.label(RichText::new(format!("{}. {} · {}", i + 1, s.name, s.text)).color(if s.me { GOLD } else { Color32::WHITE }));
        }
    }
    if let Some(p) = payout {
        ui.add_space(6.0);
        ui.label(RichText::new(format!("+💰 {}", thousands(p.coins))).size(20.0).strong().color(GOLD));
        for line in &p.lines {
            ui.label(RichText::new(line).size(13.0));
        }
        ui.label(RichText::new(format!("Balance: 💰 {}", thousands(save.coins))).color(MUTED));
    }
    for line in extras {
        ui.label(RichText::new(line).size(13.0));
    }
    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        if let Some(n) = next
            && button(ui, &format!("Next: level {} ▶", n.number), true, true)
        {
            out.push(MenuAction::PlayLevel(Box::new(n.clone())));
        }
        if button(ui, "Retry", next.is_none(), true) {
            out.push(MenuAction::Restart);
        }
        if payout.is_some() && button(ui, "Upgrades", false, true) {
            out.push(MenuAction::Quit);
            menu.tab = CareerTab::Garage;
            menu.screen = Screen::Career;
        }
        if button(ui, "Menu", false, true) {
            out.push(MenuAction::Quit);
        }
    });
}
