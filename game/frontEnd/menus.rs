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

/// While an update downloads and installs: a box over everything saying so.
pub fn update_progress(ctx: &egui::Context, updater: &crate::updates::Updater) {
    use crate::updates::Status;
    let (version, line) = match updater.status() {
        Status::Downloading { version, .. } => {
            let line = match updater.progress() {
                Some((done, Some(size))) if size > 0 => format!("Downloading… {} of {} MB", done / 1_000_000, size.div_ceil(1_000_000)),
                Some((done, _)) => format!("Downloading… {} MB", done / 1_000_000),
                None => "Downloading…".to_string(),
            };
            (version, line)
        }
        Status::Installing(version) => (version, "Installing…".to_string()),
        Status::Restarting(version) => (version, "Starting the new version…".to_string()),
        _ => return,
    };
    egui::Modal::new(egui::Id::new("car-game-updating")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.label(RichText::new(format!("Updating Car Game to {version}")).strong().size(19.0));
        ui.add_space(6.0);
        if let Some((done, Some(size))) = updater.progress().filter(|p| p.1.is_some_and(|s| s > 0)) {
            ui.add(
                egui::ProgressBar::new((done as f32 / size as f32).min(1.0))
                    .desired_height(8.0)
                    .fill(GOLD),
            );
        }
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(line);
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new("Your progress is kept. The game opens again on its own when it's done.")
                .size(12.0)
                .color(MUTED),
        );
    });
    ctx.request_repaint();
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
                let speed = crate::text::speed;
                format!("Average ★ {} · ★★ {} · ★★★ {}", speed(t[0]), speed(t[1]), speed(t[2]))
            } else {
                "Target average speeds are set by a pace car at the start".into()
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
        fact(
            "Top speed in an event",
            crate::text::speed(save.stats.get("topSpeed").copied().unwrap_or(0.0)),
        );
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

pub mod garage {
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
    use crate::updates::{Status, Updater};
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
        Updates,
        Controls,
    }

    const TABS: [(Tab, &str); 14] = [
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
        (Tab::Updates, "Updates"),
        (Tab::Controls, "Controls"),
    ];

    /// How wide the garage panel is, on the right of the window.
    pub const WIDTH: f32 = 400.0;

    /// The tab with this name ("updates", "sound & fx"), any case.
    pub fn tab_named(name: &str) -> Option<Tab> {
        TABS.iter().find(|(_, label)| label.eq_ignore_ascii_case(name)).map(|(t, _)| *t)
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum GarageAction {
        /// The car's look changed (it needs building again).
        Look,
        /// Driving or sound changed.
        Tuning,
        SettingsChanged,
        RebuildCity,
        MapChanged,
        /// The map's "Start at night" changed (it also changes where you left off).
        NightChanged,
        /// The keys went back to the standard ones.
        ControlsChanged,
        Drive,
        Menu,
        Honk,
        PacksChanged,
    }

    pub struct Garage {
        pub tab: Tab,
        /// Waiting for a key for this control (id, which of its keys).
        pub listening: Option<(String, usize)>,
        build_name: String,
        paste: String,
        message: String,
        copied: bool,
    }

    impl Default for Garage {
        fn default() -> Self {
            Garage {
                tab: Tab::Builds,
                listening: None,
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

    /// Every control and its keys: click a key, then press the new one.
    fn controls_tab(ui: &mut egui::Ui, g: &mut Garage, s: &mut Settings, out: &mut Vec<GarageAction>) {
        use crate::input::{BINDINGS, Bindings, PAD_HELP, key_label};
        let bindings = Bindings::new(&s.controls);
        hint(
            ui,
            "Click a key and press the one you want instead (Esc keeps it). Each control can have two keys.",
        );
        let mut group = "";
        for b in BINDINGS {
            if b.group != group {
                group = b.group;
                heading(ui, group);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(b.label).size(13.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let keys = bindings.keys(b.id);
                    for slot in (0..2).rev() {
                        let waiting = g.listening.as_ref().is_some_and(|(id, k)| id == b.id && *k == slot);
                        let text = if waiting {
                            "press a key…".to_string()
                        } else if let Some(k) = keys.get(slot) {
                            key_label(*k).to_string()
                        } else if slot == keys.len() {
                            "+".to_string()
                        } else {
                            continue;
                        };
                        let button = egui::Button::new(RichText::new(text).monospace())
                            .selected(waiting)
                            .min_size(egui::vec2(54.0, 22.0));
                        if ui.add(button).clicked() {
                            g.listening = if waiting {
                                None
                            } else {
                                Some((b.id.to_string(), slot.min(keys.len())))
                            };
                        }
                    }
                });
            });
        }
        ui.add_space(6.0);
        if !s.controls.is_empty() && ui.button("Back to the standard keys").clicked() {
            s.controls.clear();
            g.listening = None;
            out.push(GarageAction::ControlsChanged);
        }
        heading(ui, "Controller");
        for line in PAD_HELP {
            hint(ui, line);
        }
        hint(ui, "Esc always opens the garage and the menu.");
    }

    /// The chosen map's own options. Most rebuild the map; fighting and
    /// starting where you left off only change the settings.
    fn map_options(ui: &mut egui::Ui, s: &mut Settings, out: &mut Vec<GarageAction>) {
        let map = s.map.clone();
        let label = crate::venues::venue(&map).label;
        heading(ui, &format!("{label} options"));
        let mut o = s.map_options(&map);
        let before = o.clone();
        let mut reload = false;
        if map == "city" {
            reload |= on_off(ui, "Generative map", &mut o.generative);
            hint(
                ui,
                "Off: the map is 10 km across, ringed by mountains. On: keep driving past the edge and new towns, roads and country are made as you go, forever (the same every time).",
            );
            on_off(ui, "Start where I left off", &mut o.resume);
            hint(ui, "Off: you start at the city's start line each time.");
            on_off(ui, "Fighting people", &mut o.fighting);
            hint(
                ui,
                "On foot (F gets in and out of cars), punch with E or a click, and 2 for the pistol. People are knocked out, never killed, and the police come.",
            );
        }
        let night = on_off(ui, "Start at night", &mut o.night);
        reload |= choice(
            ui,
            "Weather",
            &[("auto", "This map's own"), ("clear", "Clear"), ("rain", "Rain"), ("snow", "Snow")],
            &mut o.weather,
        );
        if o != before {
            s.maps.insert(map, o);
            out.push(if night {
                GarageAction::NightChanged
            } else if reload {
                GarageAction::MapChanged
            } else {
                GarageAction::SettingsChanged
            });
        }
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
    fn slider(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f64,
        min: f64,
        max: f64,
        step: f64,
        fmt: &dyn Fn(f64) -> String,
        log: bool,
    ) -> bool {
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
        updater: &Updater,
        describe: &str,
    ) -> Vec<GarageAction> {
        let mut out = Vec::new();
        let mut root = crate::menu::root_ui(ctx, "garage-root");
        egui::Panel::right("garage")
            .exact_size(WIDTH)
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
                            g.listening = None;
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
                    .show(ui, |ui| tab(ui, g, c, s, storage, packs, updater, &mut out));
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
        updater: &Updater,
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
                map_options(ui, s, out);
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
            Tab::Updates => {
                updates_tab(ui, s, updater, out);
                return;
            }
            Tab::Controls => {
                controls_tab(ui, g, s, out);
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

    fn updates_tab(ui: &mut egui::Ui, s: &mut Settings, updater: &Updater, out: &mut Vec<GarageAction>) {
        heading(ui, "Updates");
        ui.label(format!("You're playing Car Game v{}.", crate::VERSION));
        ui.add_space(6.0);
        if on_off(ui, "Update automatically", &mut s.auto_update) {
            out.push(GarageAction::SettingsChanged);
        }
        hint(
            ui,
            "When this is on, each time the game starts it checks for a new version. If there is one, it's \
             downloaded and installed, and the new version starts. Your progress and settings are kept.",
        );
        ui.add_space(10.0);
        let installable = crate::updates::installation().is_some();
        let busy = updater.busy();
        match updater.status() {
            Status::Idle => {}
            Status::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking for updates…");
                });
            }
            Status::UpToDate => {
                ui.label(RichText::new("✔ You have the latest version.").color(Color32::from_rgb(120, 220, 140)));
            }
            Status::Available(version) => {
                ui.label(RichText::new(format!("Version {version} is available.")).strong().size(15.0));
                if installable {
                    hint(ui, "The game closes while it updates, then opens again on the new version.");
                    if button(ui, &format!("Update to {version}"), true, !busy) {
                        updater.install(version);
                    }
                } else {
                    hint(
                        ui,
                        "This copy of the game was built from the source code, so it can't update itself. \
                         Download the new version from the game's releases page instead.",
                    );
                }
            }
            Status::Downloading { version, .. } | Status::Installing(version) | Status::Restarting(version) => {
                ui.label(format!("Updating to {version}…"));
            }
            Status::Failed(error) => {
                ui.label(RichText::new(error).color(Color32::from_rgb(240, 140, 120)));
            }
        }
        if !matches!(updater.status(), Status::Available(_)) && button(ui, "Check for updates", false, !busy) {
            updater.check(false);
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
}
