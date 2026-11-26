//! Living City: residents, conversations, traffic and the local AI.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod ai;
mod brain;
mod cast;
mod city;
mod map;
mod mind;
mod protocol;
mod roads;
mod ui;
mod visual;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use game::GameSet;
use game::audio::Sound;
use game::game::{Game, Mode};
use game::packs::{PackInfo, PackOption, Packs, RegisterPack};
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
            icon: "🧠",
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
        .add_systems(Last, save_at_exit);
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
        PackOption::Hint("City free roam · K to talk · L for the journal".into()),
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
        return;
    }

    if !city.active {
        city.active = true;
        city.generation = world.generation;
        city.map = map::CityMap::build(&world);
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
    if !game.typing && keys.just_pressed(KeyCode::KeyL) {
        city.journal = !city.journal;
    }
    if keys.just_pressed(KeyCode::KeyK) && !game.typing {
        if let Some((who, _)) = city.passenger {
            city.open_chat(who, Contact::Passenger, &mut game);
        } else if let Some(who) = city.nearest(position(&game), 14.0) {
            city.open_chat(who, Contact::Beside, &mut game);
        } else {
            game.toast("Pull up near someone to talk · L opens the journal", 2000.0);
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
