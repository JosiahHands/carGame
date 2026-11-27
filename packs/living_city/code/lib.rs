//! Living City: residents, conversations, traffic and the local AI.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod brain;
#[path = "people.rs"]
mod cast;
mod city;

use brain::{ai, protocol};
use cast::mind;
use city::{map, roads, visual};

use bevy::app::AppExit;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use game::GameSet;
use game::audio::Sound;
use game::game::{Game, Mode};
use game::packs::{Happening, Other, OtherCar, PackInfo, PackOption, Packs, RegisterPack};
use game::storage::Storage;
use game::world::{HitBox, World};

use crate::ai::LocalAi;
use crate::brain::Contact;
use crate::city::{City, POLICE_OWNER, position};
use crate::protocol::ModelChoice;
use crate::visual::{CityCar, CityRoot, Finder, Visuals};

const ID: &str = "npc";

pub struct LivingCityPack;

impl Plugin for LivingCityPack {
    fn build(&self, app: &mut App) {
        app.register_pack(PackInfo {
            id: ID,
            icon: "💭",
            name: "Living City",
            about: "People who live their own lives, remember you and talk, with optional local AI.",
        })
        .init_resource::<LocalAi>()
        .add_systems(Startup, setup.after(game::game::setup_game))
        .add_systems(
            Update,
            (configure, simulate, sync_vehicles, visual::show_residents)
                .chain()
                .after(GameSet::Frame)
                .before(GameSet::Camera),
        )
        .add_systems(EguiPrimaryContextPass, ui::overlays.after(game::hud::hud))
        .add_systems(Last, save_at_exit.in_set(bevy::app::OnAppExitSystems));
    }
}

fn setup(mut commands: Commands, storage: Res<Storage>, mut meshes: ResMut<Assets<Mesh>>, mut packs: ResMut<Packs>) {
    let mut city = City::restore(&storage);
    let mut hardware = sysinfo::System::new();
    hardware.refresh_memory();
    city.ai_memory = hardware.total_memory();
    if let Some(legacy) = storage.get_json("carGame.pack.npc.options") {
        for key in ["think", "tags", "chases"] {
            if packs.value(ID, key).is_none()
                && let Some(value) = legacy.get(key)
            {
                let option = match value.as_bool() {
                    Some(true) => serde_json::json!("on"),
                    Some(false) => serde_json::json!("off"),
                    None => value.clone(),
                };
                packs.set(ID, key, option);
            }
        }
    }
    if let Some(legacy) = storage.get_json("carGame.pack.npc.ai") {
        for (from, to) in [("model", "model"), ("enabled", "local"), ("gpu", "gpu")] {
            if packs.value(ID, to).is_none()
                && let Some(value) = legacy.get(from)
            {
                let option = match value.as_bool() {
                    Some(true) => serde_json::json!("on"),
                    Some(false) => serde_json::json!("off"),
                    None => value.clone(),
                };
                packs.set(ID, to, option);
            }
        }
    }
    commands.insert_resource(city);
    commands.insert_resource(Visuals::new(&mut meshes));
}

fn choice(key: &'static str, label: &'static str, items: &[(&str, &str)], default: &str) -> PackOption {
    PackOption::Choice(
        key,
        label,
        items.iter().map(|(id, name)| ((*id).into(), (*name).into())).collect(),
        default.into(),
    )
}

fn configure(
    time: Res<Time<Real>>,
    mut due: Local<f64>,
    mut selection: Local<Option<(ModelChoice, bool, bool)>>,
    mut packs: ResMut<Packs>,
    mut city: ResMut<City>,
    mut ai: ResMut<LocalAi>,
) {
    if !packs.is_on(ID) {
        if selection.take().is_some() {
            ai.stop();
        }
        return;
    }
    let model_default = ModelChoice::default_for(city.ai_memory);
    let model = ModelChoice::from_id(&packs.string(ID, "model", model_default.id())).unwrap_or(model_default);
    let local_default = if city.ai_memory >= 7 * 1024 * 1024 * 1024 { "on" } else { "off" };
    let enabled =
        cfg!(feature = "local-ai") && packs.string(ID, "local", local_default) == "on" && packs.string(ID, "think", "everywhere") != "off";
    let gpu = cfg!(target_os = "macos") && packs.string(ID, "gpu", "on") == "on";
    if *selection != Some((model, enabled, gpu)) {
        *selection = Some((model, enabled, gpu));
        city.ai_choice = Some(model);
        if !enabled {
            ai.stop();
        }
        *due = 0.0;
    }
    if packs.take_press(ID, "download") && cfg!(feature = "local-ai") {
        ai.start_download(model);
        *due = 0.0;
    }
    if packs.take_press(ID, "cancel") {
        ai.cancel_download();
        *due = 0.0;
    }
    if packs.take_press(ID, "remove") {
        ai.remove_model(model);
        *due = 0.0;
    }
    if packs.take_press(ID, "reset") {
        city.reset_prompt = true;
    }
    if time.elapsed_secs_f64() < *due {
        return;
    }
    *due = time.elapsed_secs_f64() + 0.7;
    ai.poll_download();
    ai.select(model, enabled, gpu);
    let mut options = vec![
        PackOption::Hint("City free roam · K to talk · L for the journal (Garage → Controls)".into()),
        choice(
            "think",
            "Use local AI",
            &[("everywhere", "Everywhere"), ("chat", "Player chats only"), ("off", "Never")],
            "everywhere",
        ),
        choice("tags", "Name tags", &[("on", "On"), ("off", "Off")], "on"),
        choice("chases", "Police chases", &[("on", "On"), ("off", "Off")], "on"),
    ];
    if cfg!(feature = "local-ai") {
        options.push(PackOption::Heading("Local language model".into()));
        options.push(choice("local", "Local model", &[("on", "On"), ("off", "Off")], local_default));
        if cfg!(target_os = "macos") {
            options.push(choice("gpu", "Compute device", &[("on", "Metal"), ("off", "CPU")], "on"));
        }
        let choices: Vec<_> = ModelChoice::ALL.into_iter().map(|m| (m.id(), m.info().label)).collect();
        options.push(choice("model", "Model", &choices, model_default.id()));
        let info = model.info();
        options.push(PackOption::Hint(format!(
            "{} · {:.1} GB download · {} GB RAM",
            info.about, info.size, info.memory
        )));
        if ai.downloading() {
            options.push(PackOption::Button("cancel", "Cancel download"));
        } else if LocalAi::model(model).is_none() {
            options.push(PackOption::Button("download", "Download selected model"));
        } else if LocalAi::owned_model(model).is_some() {
            options.push(PackOption::Button("remove", "Remove downloaded model"));
        } else {
            options.push(PackOption::Hint("Using the model from Car Game 1.2.".into()));
        }
        if !ai.status.is_empty() {
            options.push(PackOption::Hint(ai.status.clone()));
        }
    } else {
        options.push(PackOption::Hint("This build uses the built-in brain.".into()));
    }
    options.push(PackOption::Button("reset", "Reset city…"));
    packs.options.insert(ID, options);
}

fn simulate(
    mut commands: Commands,
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut city: ResMut<City>,
    packs: Res<Packs>,
    mut ai: ResMut<LocalAi>,
    mut game: ResMut<Game>,
    mut world: ResMut<World>,
    sound: Res<Sound>,
    mut storage: ResMut<Storage>,
    roots: Query<Entity, With<CityRoot>>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wanted = packs.is_on(ID)
        && world.map == "city"
        && game.event.is_none()
        && matches!(game.mode, Mode::Drive | Mode::Photo)
        && !world.blocks.is_empty();
    if city.active && (!wanted || city.generation != world.generation) {
        city.abandon(&mut game);
        city.save(&mut storage);
        game::career::save_career(&mut storage, &game.career);
        world.hit_boxes.retain(|h| h.owner != POLICE_OWNER);
        world.hit_box_hits.retain(|(owner, _, _)| *owner != POLICE_OWNER);
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        city.active = false;
    }
    if !wanted {
        game.others.retain(|o| o.owner != ID);
        game.other_cars.retain(|o| o.owner != ID);
        return;
    }

    if !city.active {
        city.active = true;
        city.generation = world.generation;
        city.map = map::CityMap::build(&world);
        city.towns = out_of_town(&world);
        city.place_residents();
        for (i, n) in city.residents.iter_mut().enumerate() {
            n.entity = Some(visual::spawn_resident(&mut commands, &mut visuals, &mut materials, n.member, i));
        }
        for place in &city.map.places {
            let at = Vec3::new(place.at.x as f32, world.height_at(place.at.x, place.at.y) as f32, place.at.y as f32);
            visual::spawn_place(&mut commands, &mut visuals, &mut materials, at, place.kind);
        }
        visual::spawn_finder(&mut commands, &visuals, &mut materials);
    }
    if let Some(answer) = ai.poll() {
        city.receive(answer.ok(), &mut game);
    } else if !ai.busy() {
        city.receive(None, &mut game);
    }
    if !game.is_driving() {
        city.chat = None;
        return;
    }
    if city.chat.as_ref().is_some_and(|chat| {
        chat.contact == Contact::Beside
            && (city.residents[chat.who].at.distance_squared(position(&game)) > 60.0_f64.powi(2)
                || game.physics.vx.hypot(game.physics.vz) > 9.0)
    }) {
        city.chat = None;
    }
    if !game.typing && game.bindings.just_pressed(&keys, "journal") {
        city.journal = !city.journal;
    }
    if game.bindings.just_pressed(&keys, "talk") && !game.typing {
        if let Some((who, _)) = city.passenger {
            city.open_chat(who, Contact::Passenger, &mut game);
        } else if let Some(who) = city.nearest(position(&game), 14.0) {
            city.open_chat(who, Contact::Beside, &mut game);
        } else {
            game.toast("Pull up near someone to talk · L opens the journal", 2000.0);
        }
    }
    // The driver, on foot, hit someone, or took a resident's car.
    for event in game.happened.clone() {
        match event {
            Happening::Struck {
                owner: ID,
                index,
                damage,
                gun,
                ..
            } if index < city.residents.len() => {
                city.struck(index, damage, gun, &mut game);
            }
            Happening::CarTaken {
                owner: Some(ID), index, ..
            } => city.car_taken(index, &mut game),
            _ => {}
        }
    }
    let dt = time.delta_secs_f64().min(0.1) * if game.slow_motion { game::game::SLOW_MOTION } else { 1.0 };
    let everywhere = packs.string(ID, "think", "everywhere") == "everywhere";
    let chases = packs.string(ID, "chases", "on") == "on";
    for (_, damage, _) in world.hit_box_hits.extract_if(.., |hit| hit.0 == POLICE_OWNER) {
        if let Some(chase) = &mut city.chase {
            chase.hurt += damage * 0.01;
        }
    }
    city.tick(dt, &mut game, &mut storage, &mut ai, everywhere);
    world.hit_boxes.retain(|h| h.owner != POLICE_OWNER);
    city.police(dt, &mut game, &sound, &world, &mut ai, everywhere, chases);
    // The residents and their cars, for the driver's fists and bullets, and to take.
    let now = city.clock;
    game.others.retain(|o| o.owner != ID);
    game.other_cars.retain(|o| o.owner != ID);
    for (i, n) in city.residents.iter().enumerate() {
        if !n.hidden(now) {
            game.others.push(Other {
                owner: ID,
                index: i,
                at: DVec3::new(n.at.x, world.height_at(n.at.x, n.at.y), n.at.y),
                down: n.down_until > now,
            });
        }
    }
    for t in &city.trips {
        game.other_cars.push(OtherCar {
            owner: ID,
            index: t.driver,
            at: DVec3::new(t.at.x, world.height_at(t.at.x, t.at.y), t.at.y),
            heading: t.heading,
            config: visual::car_config(t.driver),
        });
    }
    if let Some(chase) = &city.chase {
        world.hit_boxes.push(HitBox {
            x: chase.at.x,
            y: 0.0,
            z: chase.at.y,
            half: 1.25,
            height: 1.65,
            alive: true,
            owner: POLICE_OWNER,
        });
    }
    if storage.dirty {
        storage.flush();
    }
}

/// The towns residents can drive out to, and the highways there (the open world only).
fn out_of_town(world: &World) -> Vec<city::OutOfTown> {
    let Some(land) = world.land.as_deref() else { return Vec::new() };
    land.towns
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            matches!(
                t.size,
                game::land::Size::Town | game::land::Size::Village | game::land::Size::Metropolis
            )
        })
        .filter(|(_, t)| t.at.length() < game::land::MAP_HALF)
        .filter_map(|(i, t)| {
            let route = land.route(0, i)?;
            Some(city::OutOfTown {
                name: t.name.clone(),
                at: t.at,
                half: t.half,
                route,
            })
        })
        .collect()
}

fn sync_vehicles(
    mut commands: Commands,
    city: Res<City>,
    world: Res<World>,
    mut cars: Query<(Entity, &CityCar, &mut Transform), Without<Finder>>,
    mut finder: Query<(&mut Transform, &mut Visibility), With<Finder>>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !city.active {
        return;
    }
    let poses = city
        .trips
        .iter()
        .map(|t| (Some(t.driver), (t.at, t.heading)))
        .chain(city.chase.iter().map(|c| (None, (c.at, c.heading))));
    for (entity, car, mut transform) in &mut cars {
        if let Some((_, (at, heading))) = poses.clone().find(|(id, _)| *id == car.0) {
            transform.translation = Vec3::new(at.x as f32, world.height_at(at.x, at.y) as f32, at.y as f32);
            transform.rotation = Quat::from_rotation_y(heading as f32);
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (id, _) in poses {
        if !cars.iter().any(|(_, car, _)| car.0 == id) {
            visual::spawn_car(&mut commands, &mut visuals, &mut materials, id);
        }
    }
    if let Ok((mut t, mut v)) = finder.single_mut() {
        if let Some((i, until)) = city.find
            && city.clock <= until
        {
            let at = city.residents[i].at;
            t.translation = Vec3::new(at.x as f32, world.height_at(at.x, at.y) as f32 + 28.5, at.y as f32);
            *v = Visibility::Inherited;
        } else {
            *v = Visibility::Hidden;
        }
    }
}

fn save_at_exit(mut exits: MessageReader<AppExit>, mut city: ResMut<City>, mut storage: ResMut<Storage>, mut game: ResMut<Game>) {
    if exits.read().next().is_some() && city.active {
        city.receive(None, &mut game);
        city.save(&mut storage);
        game::career::save_career(&mut storage, &game.career);
        storage.flush();
    }
}

mod ui {
    //! In-car chat, projected name tags and the city journal.
    use bevy::prelude::*;
    use bevy_egui::{
        EguiContexts,
        egui::{self, Align2, Color32, Id, RichText},
    };
    use game::game::Game;
    use game::packs::Packs;
    use game::storage::Storage;
    use game::world::World;
    use game::world_render::MainCamera;

    use crate::ai::LocalAi;
    use crate::brain::Contact;
    use crate::cast::name_of;
    use crate::city::{City, SAVE_KEY, position};
    use crate::mind::{friendship, is_friend, mood_word};

    pub fn overlays(
        mut contexts: EguiContexts,
        mut city: ResMut<City>,
        mut game: ResMut<Game>,
        world: Res<World>,
        packs: Res<Packs>,
        mut ai: ResMut<LocalAi>,
        mut storage: ResMut<Storage>,
        camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    ) -> Result {
        let ctx = contexts.ctx_mut()?.clone();
        if city.reset_prompt {
            let mut dismiss = false;
            let mut reset = false;
            egui::Window::new("Reset Living City?")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.label("This clears residents’ memories, friendships, messages and your police record.");
                    ui.horizontal(|ui| {
                        dismiss = ui.button("Cancel").clicked();
                        reset = ui.button("Reset city").clicked();
                    });
                });
            if reset {
                storage.remove(SAVE_KEY);
                let memory = city.ai_memory;
                let choice = city.ai_choice;
                let mut fresh = City::restore(&storage);
                fresh.ai_memory = memory;
                fresh.ai_choice = choice;
                fresh.save(&mut storage);
                storage.flush();
                *city = fresh;
            } else if dismiss {
                city.reset_prompt = false;
            }
        }
        if !city.active || !game.is_driving() || game.screen_shot {
            return Ok(());
        }

        let at = position(&game);
        let tags = packs.string(super::ID, "tags", "on") == "on";
        let mut clicked = None;
        if let Ok((camera, transform)) = camera.single() {
            let extent = camera.logical_viewport_size().unwrap_or(Vec2::ONE).max(Vec2::ONE);
            let window_rect = ctx.content_rect();
            let camera_position = transform.translation();
            // Nearest first, so the people closest to you always get their tags.
            let mut shown: Vec<(f32, usize, egui::Pos2, bool, bool)> = Vec::new();
            for (i, n) in city.residents.iter().enumerate() {
                if n.asleep || n.in_car {
                    continue;
                }
                let ground = world.height_at(n.at.x, n.at.y);
                let point = Vec3::new(n.at.x as f32, (ground + n.member.look.height + 0.22) as f32, n.at.y as f32);
                let distance = camera_position.distance_squared(point);
                if distance > 120.0_f32.powi(2) {
                    continue;
                }
                let speech = city.clock < n.speech_until && at.distance_squared(n.at) < 120.0_f64.powi(2);
                let tag = tags && at.distance_squared(n.at) < 90.0_f64.powi(2);
                if !speech && !tag {
                    continue;
                }
                if let Ok(screen) = camera.world_to_viewport(transform, point) {
                    let screen = window_rect.min
                        + egui::vec2(
                            screen.x / extent.x * window_rect.width(),
                            screen.y / extent.y * window_rect.height(),
                        );
                    if window_rect.contains(screen) {
                        shown.push((distance, i, screen, speech, tag));
                    }
                }
            }
            shown.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Where tags already are: one that would cover another is lifted
            // above it if it's saying something, and left out if it's only a name.
            let mut taken: Vec<egui::Rect> = Vec::new();
            for (_, i, mut screen, speech, tag) in shown {
                let n = &city.residents[i];
                let width = if speech {
                    165.0
                } else {
                    (n.member.name.chars().count() as f32 * 7.5 + 18.0).min(165.0)
                };
                let height = if speech {
                    22.0 + 18.0 * (n.speech.chars().count() as f32 / 24.0).ceil().max(1.0)
                } else {
                    22.0
                };
                let rect_at = |p: egui::Pos2| egui::Rect::from_min_size(p - egui::vec2(width / 2.0, height), egui::vec2(width, height));
                let mut rect = rect_at(screen);
                let mut tries = 0;
                while let Some(other) = taken.iter().find(|r| r.intersects(rect)) {
                    if !speech || tries >= 4 {
                        break;
                    }
                    screen.y = other.min.y - 2.0;
                    rect = rect_at(screen);
                    tries += 1;
                }
                if taken.iter().any(|r| r.intersects(rect)) && !speech {
                    continue;
                }
                taken.push(rect);
                egui::Area::new(Id::new(("city tag", i)))
                    .fixed_pos(screen)
                    .pivot(Align2::CENTER_BOTTOM)
                    .order(egui::Order::Background)
                    .show(&ctx, |ui| {
                        ui.set_max_width(165.0);
                        if speech {
                            game::hud::panel_frame().show(ui, |ui| {
                                ui.label(&n.speech);
                            });
                        }
                        if tag && ui.small_button(&n.member.name).clicked() {
                            clicked = Some(i);
                        }
                    });
            }
        }
        if let Some(who) = clicked {
            city.open_chat(who, Contact::Beside, &mut game);
        }

        egui::Area::new(Id::new(game::hud::CITY_STATUS_AREA))
            .anchor(Align2::RIGHT_TOP, [-18.0, 225.0])
            .order(egui::Order::Middle)
            .show(&ctx, |ui| {
                game::hud::panel_frame().show(ui, |ui| {
                    ui.label(city.clock_text());
                    if city.chase.is_some() {
                        ui.colored_label(Color32::LIGHT_RED, "Police pursuit");
                    }
                    if city.heat >= 0.5 {
                        ui.colored_label(Color32::GOLD, "★".repeat(city.heat.ceil() as usize));
                    }
                    if let Some((who, place)) = city.passenger {
                        ui.label(format!(
                            "{} → {} · {}",
                            city.residents[who].member.name,
                            place.label,
                            game.show_length(at.distance(place.at))
                        ));
                        if game.physics.vx.hypot(game.physics.vz) < 3.0 && ui.small_button("Drop off here").clicked() {
                            city.drop_passenger(&mut game, false);
                        }
                    } else if let Some(who) = city.nearest(at, 14.0) {
                        ui.label(format!("K · Talk to {}", city.residents[who].member.name));
                    }
                    if ui.small_button("L · City journal").clicked() {
                        city.journal = !city.journal;
                    }
                });
            });

        if city.journal {
            journal(&ctx, &mut city, &mut game, &ai);
        }
        if city.chat.is_some() {
            let ai_allowed = packs.string(super::ID, "think", "everywhere") != "off";
            chat_window(&ctx, &mut city, &mut game, &mut ai, ai_allowed);
        }
        game.pointer_on_ui = ctx.is_pointer_over_egui();
        game.typing = ctx.egui_wants_keyboard_input();
        Ok(())
    }

    fn chat_window(ctx: &egui::Context, city: &mut City, game: &mut Game, ai: &mut LocalAi, ai_allowed: bool) {
        let who = city.chat.as_ref().expect("chat is open").who;
        let resident = &city.residents[who];
        let mut open = true;
        let mut send = false;
        let mut ride = false;
        let mut lend = false;
        let passenger = city.passenger.is_none();
        let thinking = city.chat_pending(who);
        let chat = city.chat.as_mut().expect("chat is open");
        let contact = match chat.contact {
            Contact::Beside => "Beside your car",
            Contact::Phone => "On the phone",
            Contact::Passenger => "Riding with you",
        };
        egui::Window::new(&resident.member.name)
            .id(Id::new("living city chat"))
            .anchor(Align2::LEFT_BOTTOM, [18.0, -25.0])
            .default_width(365.0)
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(format!("{} · {} · {}", resident.member.role, friendship(&resident.mind), contact));
                egui::ScrollArea::vertical().max_height(210.0).stick_to_bottom(true).show(ui, |ui| {
                    ui.label(&chat.greeting);
                    for line in resident.mind.chats.iter().rev().take(10).rev() {
                        ui.label(format!("You: {}", line.you));
                        ui.label(RichText::new(format!("{}: {}", resident.member.name, line.them)).strong());
                    }
                });
                if thinking {
                    ui.spinner();
                }
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut chat.input)
                            .char_limit(220)
                            .hint_text("Say something…")
                            .desired_width(255.0),
                    );
                    if std::mem::take(&mut chat.focus) {
                        response.request_focus();
                    }
                    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    send = ui.add_enabled(!thinking, egui::Button::new("Send")).clicked() || (enter && !thinking);
                });
                if chat.contact == Contact::Beside {
                    ui.horizontal(|ui| {
                        if passenger && is_friend(&resident.mind) && game.on_foot.is_none() {
                            ride = ui.button("Offer a ride").clicked();
                        }
                        if game.on_foot.is_some() {
                            lend = ui.button("Ask to borrow a car").clicked();
                        }
                    });
                }
            });
        if send {
            city.send_chat(game, ai, ai_allowed);
        }
        if ride {
            city.offer_ride(who, game);
        }
        if lend {
            city.lend_car(who, game);
        }
        if !open {
            city.chat = None;
        }
    }

    fn journal(ctx: &egui::Context, city: &mut City, game: &mut Game, ai: &LocalAi) {
        let mut open = city.journal;
        let mut find = None;
        let mut call = None;
        egui::Window::new("Living City journal")
            .default_width(536.0)
            .default_height(550.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(city.clock_text());
                ui.label(format!(
                    "Police record · {} tickets · {} pursuits · {} escapes · {} busted · {} called off · {} coins in fines",
                    city.tickets, city.record.chases, city.record.escapes, city.record.busts, city.record.called_off, city.record.fines
                ));
                if !ai.status.is_empty() {
                    ui.label(&ai.status);
                }
                egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                    if !city.messages.is_empty() {
                        ui.collapsing("Messages", |ui| {
                            for message in &city.messages {
                                ui.label(format!("{}: {}", name_of(&message.from), message.text));
                            }
                        });
                    }
                    if !city.news.is_empty() {
                        ui.collapsing("City news", |ui| {
                            for news in &city.news {
                                ui.label(news);
                            }
                        });
                    }
                    if !city.talks.is_empty() {
                        ui.collapsing("Overheard conversations", |ui| {
                            for talk in &city.talks {
                                ui.label(RichText::new(&talk.place).strong());
                                for line in &talk.lines {
                                    ui.label(format!("{}: {}", line.name, line.text));
                                }
                                ui.separator();
                            }
                        });
                    }
                    if !city.meetups.is_empty() {
                        ui.collapsing("Meetups", |ui| {
                            for meetup in &city.meetups {
                                let minutes = ((meetup.at - city.clock).max(0.0) / 60.0).ceil();
                                let place = city
                                    .map
                                    .place(&meetup.place_id, position(game))
                                    .map_or(meetup.kind.as_str(), |p| p.label);
                                ui.label(format!(
                                    "{} and {} at {place} · in {minutes:.0} city hours",
                                    name_of(&meetup.ids[0]),
                                    name_of(&meetup.ids[1])
                                ));
                            }
                        });
                    }
                    ui.heading("Residents");
                    for (i, n) in city.residents.iter().enumerate() {
                        ui.push_id(&n.member.id, |ui| {
                            ui.collapsing(
                                format!(
                                    "{}{} · {} · {}",
                                    if n.member.star { "★ " } else { "" },
                                    n.member.name,
                                    n.member.role,
                                    friendship(&n.mind)
                                ),
                                |ui| {
                                    ui.label(&n.member.bio);
                                    ui.label(format!("Mood: {}", mood_word(&n.mind)));
                                    if !n.mind.goal.is_empty() {
                                        ui.label(format!("Goal: {}", n.mind.goal));
                                    }
                                    if !n.mind.story.is_empty() {
                                        ui.label(&n.mind.story);
                                    }
                                    for (label, entries) in [
                                        ("Standing decisions", &n.mind.resolutions),
                                        ("Insights", &n.mind.insights),
                                        ("Habits", &n.mind.habits),
                                        ("Thoughts", &n.mind.thoughts),
                                    ] {
                                        if !entries.is_empty() {
                                            ui.collapsing(label, |ui| {
                                                for entry in entries {
                                                    ui.label(entry);
                                                }
                                            });
                                        }
                                    }
                                    ui.collapsing("Memories", |ui| {
                                        for memory in n.mind.memories.iter().rev().take(15) {
                                            ui.label(&memory.text);
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Find on map").clicked() {
                                            find = Some(i);
                                        }
                                        if is_friend(&n.mind) && ui.button("Call").clicked() {
                                            call = Some(i);
                                        }
                                    });
                                },
                            );
                        });
                    }
                });
            });
        city.journal = open;
        if let Some(i) = find {
            let n = &city.residents[i];
            city.find = Some((i, city.clock + 12.0));
            game.toast(
                format!("{} · {} away", n.member.name, game.show_length(n.at.distance(position(game)))),
                2500.0,
            );
            city.journal = false;
        }
        if let Some(i) = call {
            city.open_chat(i, Contact::Phone, game);
            if city.chat.is_some() {
                city.journal = false;
            }
        }
    }
}
