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
        for (i, n) in city.residents.iter().enumerate() {
            if n.asleep || n.in_car {
                continue;
            }
            let ground = world.height_at(n.at.x, n.at.y);
            let point = Vec3::new(n.at.x as f32, (ground + n.member.look.height + 0.22) as f32, n.at.y as f32);
            if camera_position.distance_squared(point) > 120.0_f32.powi(2) {
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
                if !window_rect.contains(screen) {
                    continue;
                }
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
    }
    if let Some(who) = clicked {
        city.open_chat(who, Contact::Beside, &mut game);
    }

    egui::Area::new(Id::new("city status"))
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
            if passenger && chat.contact == Contact::Beside && is_friend(&resident.mind) {
                ride = ui.button("Offer a ride").clicked();
            }
        });
    if send {
        city.send_chat(game, ai, ai_allowed);
    }
    if ride {
        city.offer_ride(who, game);
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
