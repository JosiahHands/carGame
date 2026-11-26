//! The game loop: the player's car, the modes (menu, garage, driving, photo),
//! fixed-step physics, drifting, crashes, the cameras, and everything that
//! hangs off them: events and the career, weapons, the time trial, free roam,
//! traffic and people, skill chains, XP and trophies.
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::audio::Sound;
use crate::camera::CameraRig;
use crate::career::{self, CHASSIS, CareerSave, LEVELS, Level, Loadout};
use crate::carmodel::{CarBuildCtx, CarRig, build_car};
use crate::config::{self, CarConfig};
use crate::event::{EventHud, EventRun, EventState, EventVisual};
use crate::freeroam::{FreeRoam, FreeRoamEvent, zone_stars};
use crate::garage::Garage;
use crate::input::{Action, Controls};
use crate::menu::{Menu, MenuAction, Screen};
use crate::pedestrians::{CarNear, Pedestrians};
use crate::physics::{CarInput, CarPhysics, Dynamics, GEAR_TOPS};
use crate::rewards::{add_xp, bump, check_trophies, driver_level, record, trophies};
use crate::rng;
use crate::settings::{self, Settings};
use crate::skills::{ChainEnd, SkillChain, SkillKind};
use crate::spec::{EventKind, EventSpec};
use crate::storage::Storage;
use crate::text::{format_time, thousands};
use crate::timetrial::{CarPose, TimeTrial, TrialEvent};
use crate::traffic::{Traffic, TrafficPlayer};
use crate::weapons::{CarFrame, Weapons};
use crate::world::{TargetKind, World};
use crate::world_build::{Build, WorldMats};
use crate::world_render::{Focus, Graphics, MainCamera};

pub const STEP: f64 = 1.0 / 120.0;
pub const MAX_FRAME: f64 = 0.25;
const MAX_STEPS: usize = 30;
pub const SLOW_MOTION: f64 = 0.3;
const SPARK_COLORS: [u32; 3] = [0x4fb3ff, 0xff9f40, 0xc77dff];
const SPARK_NAMES: [&str; 3] = ["Blue", "Orange", "Purple"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Menu,
    Garage,
    Drive,
    Photo,
}

#[derive(Clone, Debug, Default)]
pub struct Toast {
    pub text: String,
    pub left: f64,
}

#[derive(Clone, Debug)]
pub struct Popup {
    pub icon: String,
    pub title: String,
    pub text: String,
    pub left: f64,
}

/// How to start the current event again (Restart and Retry).
#[derive(Clone, Debug)]
pub enum Again {
    /// A career level (0 for the daily challenge).
    Level(usize),
    Quick(Box<EventSpec>, Box<CarConfig>),
}

/// Everything about the game that isn't the map.
#[derive(Resource)]
pub struct Game {
    pub mode: Mode,
    pub settings: Settings,
    /// The garage build: what you drive in free roam.
    pub config: CarConfig,
    /// The car being driven now.
    pub drive_config: CarConfig,
    pub driving_garage_build: bool,
    pub physics: CarPhysics,
    pub dynamics: Dynamics,
    pub car: Option<CarRig>,
    pub rebuild: bool,
    pub night: bool,
    pub slow_motion: bool,
    pub paused: bool,
    pub accumulator: f64,
    pub shake: f64,
    pub drift_score: f64,
    pub drift_timer: f64,
    pub best_drift: f64,
    pub air_time: f64,
    pub gear_refused: f64,
    pub help: bool,
    pub toast: Toast,
    pub popups: Vec<Popup>,
    pub fps: (u32, f64, u32),
    pub damage: f64,
    pub clock: f64,
    pub real_clock: f64,
    pub load_map: Option<(String, u32)>,
    pub pointer_on_ui: bool,
    pub typing: bool,
    /// Save a photo on the next frame (photo mode).
    pub screen_shot: bool,
    pub career: CareerSave,
    pub menu: Menu,
    pub garage: Garage,
    pub event: Option<EventRun>,
    pub again: Option<Again>,
    pub playing: Option<Level>,
    pub ended_for: f64,
    pub results_shown: bool,
    pub event_hud: EventHud,
    event_hud_timer: f64,
    pub trial: TimeTrial,
    pub ghost: Option<CarRig>,
    pub free_roam: FreeRoam,
    pub traffic: Traffic,
    pub people: Pedestrians,
    pub weapons: Weapons,
    pub chain: SkillChain,
    pub chain_shown: f64,
    pub chain_end: Option<(String, bool)>,
    pub drift_held: f64,
    drift_gap: f64,
    pub spark_tier: usize,
    draft_tick: f64,
    speed_tick: f64,
    pub roaming_in_career_car: bool,
    throttle_at: Option<f64>,
    was_countdown: bool,
    pub wheel_spin: f64,
    celebrated: bool,
    fireworks: f64,
    firework_due: f64,
    smoke_due: f64,
    sparks_due: f64,
    grass_due: f64,
    trail_due: f64,
    odometer: f64,
    last_tips: Vec<DVec3>,
    pub zone_text: String,
    npc_settings: String,
    pub speed_lines: f32,
    /// Rival car models still to remove, and other entities of the event (speed camera gantries).
    pub event_entities: Vec<Entity>,
    /// The quality preset was just picked for this computer (shown once).
    pub first_start: bool,
    /// The new-map toast is shown once the first map is up.
    loaded_once: bool,
    pub controls: Controls,
    /// What happened this frame, for the packs (cleared at the start of each frame).
    pub happened: Vec<crate::packs::Happening>,
}

impl Game {
    pub fn toast(&mut self, text: impl Into<String>, ms: f64) {
        self.toast = Toast {
            text: text.into(),
            left: ms / 1000.0,
        };
    }

    pub fn popup(&mut self, icon: &str, title: impl Into<String>, text: impl Into<String>) {
        self.popups.push(Popup {
            icon: icon.into(),
            title: title.into(),
            text: text.into(),
            left: 4.6,
        });
    }

    pub fn car_position(&self) -> Vec3 {
        Vec3::new(self.physics.x as f32, self.physics.y as f32, self.physics.z as f32)
    }

    pub fn car_radius(&self) -> f64 {
        self.car.as_ref().map_or(2.4, |c| c.length.hypot(c.width) / 2.0)
    }

    pub fn speed_in_units(&self, mps: f64) -> f64 {
        mps * if self.settings.is_mph() { 2.237 } else { 3.6 }
    }

    pub fn unit(&self) -> &'static str {
        if self.settings.is_mph() { "mph" } else { "km/h" }
    }

    pub fn show_length(&self, m: f64) -> String {
        if self.settings.is_mph() {
            format!("{} ft", thousands((m * 3.281).round()))
        } else if m < 10.0 {
            format!("{m:.1} m")
        } else {
            format!("{} m", thousands(m.round()))
        }
    }

    pub fn is_driving(&self) -> bool {
        self.mode == Mode::Drive && !self.paused
    }

    /// The announcer, if it's on and the sound isn't muted.
    pub fn speak(&self, text: &str, sound: &Sound) {
        if self.settings.announcer && !sound.is_muted() {
            crate::speak::speak(text);
        }
    }

    /// The weapons the car has: in events with weapons, and in free roam if the setting is on.
    pub fn loadout(&self, spec: Option<&EventSpec>) -> Loadout {
        let career_car_now = if spec.is_some() {
            self.playing.is_some()
        } else {
            !self.driving_garage_build
        };
        let on = spec.map_or(self.settings.weapons, |s| s.weapons);
        career::car_loadout(on, if career_car_now { Some(&self.career.arsenal) } else { None })
    }

    pub fn describe_car(&self) -> String {
        let Some(car) = &self.car else { return String::new() };
        let top = self.speed_in_units(self.config.tuning.top_speed * 0.44704);
        let giant = if car.height >= 3.0 {
            format!(" · Flattens anything under {} tall", self.show_length(car.height * 2.0))
        } else {
            String::new()
        };
        format!(
            "{} tall · {} long · top speed {} {}{giant}",
            self.show_length(car.height),
            self.show_length(car.length),
            top.round(),
            self.unit()
        )
    }
}

/// What most of the game's systems need.
#[derive(SystemParam)]
pub struct Ctx<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub mats: Res<'w, WorldMats>,
    pub world: ResMut<'w, World>,
    pub game: ResMut<'w, Game>,
    pub rig: ResMut<'w, CameraRig>,
    pub sound: ResMut<'w, Sound>,
    pub storage: ResMut<'w, Storage>,
    pub graphics: ResMut<'w, Graphics>,
    pub focus: ResMut<'w, Focus>,
    pub packs: ResMut<'w, crate::packs::Packs>,
}

impl<'w, 's> Ctx<'w, 's> {
    pub fn build(&mut self) -> Build<'_, 'w, 's> {
        Build {
            commands: &mut self.commands,
            meshes: &mut self.meshes,
            materials: &mut self.materials,
            images: &mut self.images,
            mats: &self.mats,
        }
    }

    pub fn car_ctx(&mut self) -> CarBuildCtx<'_, 'w, 's> {
        CarBuildCtx {
            commands: &mut self.commands,
            meshes: &mut self.meshes,
            materials: &mut self.materials,
            images: &mut self.images,
        }
    }

    fn toast(&mut self, text: impl Into<String>, ms: f64) {
        self.game.toast(text, ms);
    }

    /// Builds the player's car again, with the weapons it has for `spec`. True if its size changed a lot.
    pub fn rebuild_car(&mut self, spec: Option<&EventSpec>) -> bool {
        if let Some(old) = self.game.car.take() {
            old.despawn(&mut self.commands);
        }
        let loadout = self.game.loadout(spec.or(self.game.event.as_ref().map(|e| &e.spec)));
        self.game.weapons.set_loadout(loadout);
        let mut config = self.game.drive_config.clone();
        config.armed = config::Armed {
            gun: loadout[0] > 0,
            rockets: loadout[1] > 0,
        };
        let night = self.world.is_night();
        let rig = build_car(&mut self.car_ctx(), &config, night, true);
        self.game.car = Some(rig);
        let resized = self.fit_to_car();
        sync_car(self, 0.0);
        resized
    }

    pub fn fit_to_car(&mut self) -> bool {
        let Some(car) = &self.game.car else { return false };
        let dims = car.dims();
        let (length, height, eye) = (car.length, car.height, car.eye_height);
        let config = self.game.drive_config.clone();
        self.game.physics.configure(&config, dims);
        self.rig.eye_height = eye as f32;
        let size = (length / 4.5).max(height / 1.4) as f32;
        let resized = (size / self.rig.size).ln().abs() > 0.1;
        self.rig.size = size;
        self.sound.set_engine(&config.engine_sound);
        self.apply_view();
        resized
    }

    pub fn apply_view(&mut self) {
        let s = self.rig.size;
        self.graphics.shadows = self.game.settings.shadows.clone();
        self.graphics.shadow_spread = s.clamp(1.0, 8.0);
        let (length, height) = self.game.car.as_ref().map_or((4.5, 1.4), |c| (c.length, c.height));
        let reach = 40.0 * s as f64 + length.max(height);
        let distance = self.game.settings.view_distance.max(reach * 1.5);
        self.graphics.view_distance = distance as f32;
        self.world.set_view_distance(distance);
    }

    pub fn apply_settings(&mut self) {
        self.apply_view();
        self.rig.base_fov = self.game.settings.fov as f32;
        self.world.effects.amount = settings::effects_amount(&self.game.settings.effects);
        self.game.physics.manual = self.game.settings.transmission == "manual";
    }

    pub fn place_car(&mut self, x: f64, z: f64, heading: f64) {
        let dynamics = self.game.dynamics.clone();
        self.game.physics.reset(x, z, heading, false, &dynamics);
        self.game.physics.y = self.world.height_at(x, z);
        self.game.weapons.clear();
        let p = self.game.car_position();
        self.rig.snap(p, heading as f32);
        let g = &mut *self.game;
        g.drift_score = 0.0;
        g.drift_held = 0.0;
        g.chain.clear();
    }

    pub fn save_settings(&mut self) {
        let s = self.game.settings.clone();
        settings::save_settings(&mut self.storage, &s);
    }

    pub fn save_config(&mut self) {
        let c = self.game.config.clone();
        config::save_config(&mut self.storage, &c);
    }

    pub fn save_career(&mut self) {
        let c = self.game.career.clone();
        career::save_career(&mut self.storage, &c);
    }

    pub fn set_mode(&mut self, next: Mode) {
        let previous = self.game.mode;
        self.game.mode = next;
        if next != Mode::Menu && !self.game.menu.is_overlay() {
            self.game.menu.screen = Screen::Hidden;
        }
        if next == Mode::Garage || next == Mode::Menu {
            let p = &mut self.game.physics;
            p.vx = 0.0;
            p.vz = 0.0;
            p.vy = 0.0;
            p.y = self.world.height_at(p.x, p.z);
            p.airborne = false;
            self.game.trial.stop();
        }
        let (pos, heading) = (self.game.car_position(), self.game.physics.heading as f32);
        match next {
            Mode::Garage | Mode::Menu => self.rig.set_garage(true, pos, heading, false),
            Mode::Photo => self.rig.set_garage(true, pos, heading, true),
            Mode::Drive if previous != Mode::Drive => self.rig.set_garage(false, pos, heading, false),
            Mode::Drive => {}
        }
    }

    /// Builds a map right now.
    pub fn load_world(&mut self, map: &str, layout: u32) {
        let mut world = std::mem::replace(&mut *self.world, World::empty());
        world.night = self.game.night;
        world.effects.amount = settings::effects_amount(&self.game.settings.effects);
        world.view_distance = self.world.view_distance;
        world.generation = self.world.generation;
        world.load(map, layout, &mut self.build());
        *self.world = world;
        self.game.traffic = Traffic::default();
    }

    /// Switches to the map chosen in the settings, and puts the car at its start (free roam).
    pub fn load_map(&mut self) {
        self.game.trial.stop();
        self.game.weapons.clear();
        let map = self.game.settings.map.clone();
        self.load_world(&map, 0);
        self.after_map();
        let label = crate::venues::venue(&map).label;
        if self.game.loaded_once {
            self.toast(label, 1600.0);
        }
        self.game.loaded_once = true;
    }

    /// Free roam things for the map just built: wrenches, the lap, traffic and people.
    fn after_map(&mut self) {
        let found = self.game.career.found.clone();
        self.game.free_roam.load(&self.world, &found);
        let course = self.world.course.clone();
        let map = self.world.map.clone();
        let storage = std::mem::take(&mut *self.storage);
        self.game.trial.set_course(&course, &map, &storage);
        *self.storage = storage;
        if let Some(g) = self.game.ghost.take() {
            g.despawn(&mut self.commands);
        }
        self.game.damage = 0.0;
        let s = self.world.spawn;
        self.place_car(s.x, s.z, s.heading);
        self.load_npcs();
        sync_car(self, 0.0);
        if matches!(self.game.mode, Mode::Garage | Mode::Menu) {
            let (pos, heading) = (self.game.car_position(), self.game.physics.heading as f32);
            self.rig.set_garage(true, pos, heading, false);
        }
    }

    /// Traffic and people in the city for free roam, as many as the settings say.
    pub fn load_npcs(&mut self) {
        let s = self.game.settings.clone();
        self.game.npc_settings = format!("{}/{}", s.traffic, s.pedestrians);
        let count = settings::traffic_count(&s.traffic);
        let people = s.pedestrians;
        let (x, z) = (self.game.physics.x, self.game.physics.z);
        let mut traffic = std::mem::take(&mut self.game.traffic);
        let mut world = std::mem::replace(&mut *self.world, World::empty());
        traffic.load(&mut world, &mut self.build(), count, x, z);
        *self.world = world;
        self.game.traffic = traffic;
        self.game.people.load(&self.world, people);
    }

    fn clear_traffic(&mut self) {
        let mut traffic = std::mem::take(&mut self.game.traffic);
        traffic.clear(&mut self.world);
    }

    // ---------- events ----------

    fn dispose_event(&mut self) {
        if let Some(e) = self.game.event.take() {
            for r in &e.racers {
                if let Some(rig) = r.rival.as_ref().and_then(|v| v.rig.as_ref()) {
                    rig.despawn(&mut self.commands);
                }
            }
        }
        for e in std::mem::take(&mut self.game.event_entities) {
            if let Ok(mut ec) = self.commands.get_entity(e) {
                ec.despawn();
            }
        }
        self.world.hit_boxes.clear();
    }

    /// Plays a career level or the daily challenge (number 0), if it's unlocked.
    pub fn play_level(&mut self, mut level: Level) {
        let reason = if level.number > 0 && level.number <= LEVELS {
            career::locked_reason(&self.game.career, level.number)
        } else {
            String::new()
        };
        if !reason.is_empty() {
            self.toast(reason, 1600.0);
            return;
        }
        level.spec.armor = career::armor_for(self.game.career.upgrades.get("armor").copied().unwrap_or(0));
        let c = &self.game.career;
        let car = career::career_car(&self.game.config, c.car, &c.upgrades);
        let all_terrain = CHASSIS[c.car].all_terrain;
        self.game.again = Some(Again::Level(level.number));
        let spec = level.spec.clone();
        self.start_event(spec, car, Some(level), all_terrain);
    }

    pub fn restart(&mut self) {
        match self.game.again.clone() {
            Some(Again::Level(0)) => {
                let level = career::daily_level(&self.game.career);
                self.play_level(level);
            }
            Some(Again::Level(n)) => self.play_level(career::make_level(n)),
            Some(Again::Quick(spec, car)) => self.start_event(*spec, *car, None, false),
            None => {}
        }
    }

    /// Loads the venue, puts everyone on the grid and starts the countdown.
    pub fn start_event(&mut self, spec: EventSpec, car: CarConfig, level: Option<Level>, all_terrain: bool) {
        self.dispose_event();
        self.resume_event();
        self.clear_traffic();
        self.game.people.load(&self.world, false);
        self.game.playing = level;
        self.game.trial.stop();
        if let Some(g) = self.game.ghost.take() {
            g.despawn(&mut self.commands);
        }
        self.game.weapons.clear();
        self.world.effects.clear();
        self.game.night = spec.night;
        if self.world.map != spec.map || self.world.layout != spec.layout {
            self.load_world(&spec.map, spec.layout);
            let found = self.game.career.found.clone();
            self.game.free_roam.load(&self.world, &found);
        } else {
            self.world.rebuild_all(1e6, 1e6, 0.0);
        }
        self.world.destroyed.clear();
        self.world.set_night(spec.night);
        self.game.drive_config = car;
        self.game.driving_garage_build = false;
        self.rebuild_car(Some(&spec));
        self.game.physics.all_terrain = all_terrain;
        let g = &mut *self.game;
        g.damage = 0.0;
        g.drift_score = 0.0;
        g.air_time = 0.0;
        g.slow_motion = false;
        g.ended_for = 0.0;
        g.results_shown = false;
        g.chain.clear();
        g.drift_held = 0.0;
        g.throttle_at = None;
        g.was_countdown = false;
        g.wheel_spin = 0.0;
        g.celebrated = false;
        g.fireworks = 0.0;
        g.firework_due = 0.0;
        // The rivals' cars.
        let night = self.world.is_night();
        let mut rigs = Vec::new();
        for r in &spec.rivals {
            let rig = build_car(&mut self.car_ctx(), &r.config, night, false);
            rigs.push(Some(rig));
        }
        let dynamics = self.game.dynamics.clone();
        let mut physics = std::mem::take(&mut self.game.physics);
        let mut event = EventRun::new(spec, &mut self.world, &mut physics, rigs, dynamics);
        self.game.physics = physics;
        let nemesis = event
            .rival_names()
            .into_iter()
            .find(|n| self.game.career.nemesis.get(n).copied().unwrap_or(0.0) >= 2.0);
        if let Some(n) = nemesis {
            event.notes.push(format!("😈 Your nemesis {n} is here. Beat them for revenge!"));
            event.callouts.push(format!("Your nemesis, {n}, is here."));
        }
        // Speed camera gantries.
        if let Some(track) = event.track.clone() {
            let post = self.materials.add(StandardMaterial {
                base_color: crate::world_build::hex_color(0x30343a),
                metallic: 0.6,
                perceptual_roughness: 0.4,
                ..default()
            });
            let glow = self.materials.add(StandardMaterial {
                base_color: crate::world_build::hex_color(0xffd23f),
                emissive: crate::world_build::glow(crate::world_build::hex_color(0xffb300), 1.4),
                ..default()
            });
            let half = (track.style.width / 2.0 + 1.0) as f32;
            for &along in &event.traps {
                let p = track.pose(along, 0.0);
                let root = self
                    .commands
                    .spawn((
                        Transform::from_xyz(p.x as f32, p.y as f32, p.z as f32).with_rotation(Quat::from_rotation_y(p.heading as f32)),
                        Visibility::default(),
                    ))
                    .id();
                for side in [-1.0f32, 1.0] {
                    let m = self.meshes.add(bevy::shape::Cuboid::new(0.3, 6.0, 0.3).mesh().build());
                    self.commands.spawn((
                        Mesh3d(m),
                        MeshMaterial3d(post.clone()),
                        Transform::from_xyz(side * half, 3.0, 0.0),
                        ChildOf(root),
                    ));
                }
                let m = self.meshes.add(bevy::shape::Cuboid::new(half * 2.0, 0.5, 0.5).mesh().build());
                self.commands.spawn((
                    Mesh3d(m),
                    MeshMaterial3d(glow.clone()),
                    Transform::from_xyz(0.0, 6.0, 0.0),
                    ChildOf(root),
                ));
                self.game.event_entities.push(root);
            }
        }
        self.game.event = Some(event);
        sync_car(self, 0.0);
        self.set_mode(Mode::Drive);
        let (pos, heading) = (self.game.car_position(), self.game.physics.heading as f32);
        self.rig.set_garage(false, pos, heading, false);
        self.rig.snap(pos, heading);
    }

    pub fn pause_event(&mut self) {
        let Some(e) = &self.game.event else { return };
        if e.state == EventState::Done {
            return;
        }
        let title = e.spec.title.clone();
        self.game.paused = true;
        self.game.menu.screen = Screen::Pause(title);
    }

    pub fn resume_event(&mut self) {
        self.game.paused = false;
        self.game.menu.screen = Screen::Hidden;
    }

    /// Ends any event and goes back to the garage build, on the map chosen in the settings.
    pub fn leave_events(&mut self) {
        self.dispose_event();
        let g = &mut *self.game;
        g.playing = None;
        g.paused = false;
        g.physics.all_terrain = false;
        g.roaming_in_career_car = false;
        let loadout_changed = g.loadout(None) != g.weapons.loadout();
        if !g.driving_garage_build || loadout_changed {
            g.drive_config = g.config.clone();
            g.driving_garage_build = true;
            self.rebuild_car(None);
        }
        if self.game.night {
            self.game.night = false;
            self.world.set_night(false);
        }
        if self.world.map != self.game.settings.map || self.world.layout != 0 {
            self.load_map();
        } else {
            self.world.rebuild_all(1e6, 1e6, 0.0);
            self.after_map();
        }
    }

    /// Quits an event (or finishes with one) and goes to the main menu.
    pub fn quit_event(&mut self) {
        self.dispose_event();
        self.game.paused = false;
        self.game.physics.all_terrain = false;
        let s = self.world.spawn;
        self.place_car(s.x, s.z, s.heading);
        self.set_mode(Mode::Menu);
        if !matches!(self.game.menu.screen, Screen::Career) {
            self.game.menu.screen = Screen::Main;
        }
    }

    /// Shows the career car behind the menu (rebuilt only when it looks different).
    pub fn show_career_car(&mut self) {
        self.save_career();
        if self.game.mode != Mode::Menu || self.game.event.is_some() {
            return;
        }
        let c = &self.game.career;
        let next = career::career_car(&self.game.config, c.car, &c.upgrades);
        let looks_changed = next != self.game.drive_config;
        self.game.drive_config = next;
        self.game.driving_garage_build = false;
        if looks_changed || self.game.loadout(None) != self.game.weapons.loadout() {
            self.rebuild_car(None);
            let (pos, heading) = (self.game.car_position(), self.game.physics.heading as f32);
            self.rig.set_garage(true, pos, heading, false);
        }
    }

    pub fn menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::PlayLevel(level) => self.play_level(*level),
            MenuAction::PlayQuick(spec, car) => {
                self.game.again = Some(Again::Quick(spec.clone(), car.clone()));
                self.start_event(*spec, *car, None, false);
            }
            MenuAction::FreeRoam => {
                self.leave_events();
                self.set_mode(Mode::Drive);
            }
            MenuAction::FreeRoamCareer => {
                self.leave_events();
                let c = &self.game.career;
                self.game.drive_config = career::career_car(&self.game.config, c.car, &c.upgrades);
                self.game.driving_garage_build = false;
                self.rebuild_car(None);
                self.game.physics.all_terrain = CHASSIS[self.game.career.car].all_terrain;
                self.game.roaming_in_career_car = true;
                self.set_mode(Mode::Drive);
                self.toast("Your career car: challenge stars count", 2500.0);
            }
            MenuAction::Garage => {
                self.leave_events();
                self.set_mode(Mode::Garage);
            }
            MenuAction::CarChanged => self.show_career_car(),
            MenuAction::Resume => self.resume_event(),
            MenuAction::Restart => self.restart(),
            MenuAction::Quit => self.quit_event(),
            MenuAction::ChooseSandbox => {
                self.game.career.mode = "sandbox".into();
                self.game.career.finished = true;
                self.save_career();
                if self.game.event.is_some() {
                    self.quit_event();
                }
                self.game.menu.chapter = 11;
                self.game.menu.tab = crate::menu::CareerTab::Levels;
                self.game.menu.screen = Screen::Career;
            }
            MenuAction::ChooseReset => {
                self.game.career = career::reset_career(&self.game.career, true);
                self.save_career();
                let found = self.game.career.found.clone();
                self.game.free_roam.load(&self.world, &found);
                if self.game.event.is_some() {
                    self.quit_event();
                }
                self.game.menu.chapter = 1;
                self.game.menu.tab = crate::menu::CareerTab::Levels;
                self.game.menu.screen = Screen::Career;
            }
        }
    }

    pub fn garage_action(&mut self, action: crate::garage::GarageAction) {
        use crate::garage::GarageAction as G;
        match action {
            G::Look | G::Tuning => {
                let switched = !self.game.driving_garage_build;
                self.game.drive_config = self.game.config.clone();
                self.game.driving_garage_build = true;
                self.game.roaming_in_career_car = false;
                let resized = if action == G::Look || switched {
                    self.rebuild_car(None)
                } else {
                    self.fit_to_car()
                };
                if resized && self.game.mode == Mode::Garage {
                    let (pos, heading) = (self.game.car_position(), self.game.physics.heading as f32);
                    self.rig.set_garage(true, pos, heading, false);
                }
                self.save_config();
            }
            G::SettingsChanged => {
                self.save_settings();
                if self.game.event.is_none() && self.game.loadout(None) != self.game.weapons.loadout() {
                    self.rebuild_car(None);
                }
                self.apply_settings();
                let s = &self.game.settings;
                let npcs = format!("{}/{}", s.traffic, s.pedestrians);
                if npcs != self.game.npc_settings && self.game.event.is_none() {
                    self.load_npcs();
                }
            }
            G::RebuildCity => {
                let (x, z, r) = (self.game.physics.x, self.game.physics.z, self.game.car_radius());
                self.world.rebuild_all(x, z, r);
                self.game.damage = 0.0;
                self.toast("Everything has been rebuilt", 1600.0);
            }
            G::MapChanged => {
                self.save_settings();
                self.load_map();
            }
            G::Drive => self.set_mode(Mode::Drive),
            G::Menu => {
                self.set_mode(Mode::Menu);
                self.game.menu.screen = Screen::Main;
            }
            G::Honk => {
                let horn = self.game.config.horn.clone();
                self.sound.horn(&horn);
            }
            G::PacksChanged => {
                self.packs.save(&mut self.storage);
            }
        }
    }

    // ---------- rewards ----------

    /// Adds a skill to the chain.
    pub fn skill(&mut self, kind: SkillKind, points: f64, label: Option<&str>) {
        if self.game.event.as_ref().is_some_and(|e| e.state == EventState::Done) && self.game.celebrated {
            return;
        }
        self.game.chain.add(kind, points, label);
        let stat = kind.stat();
        if !stat.is_empty() {
            bump(&mut self.game.career, stat, 1.0);
        }
    }

    fn chain_ended(&mut self, end: ChainEnd) {
        match end {
            ChainEnd::Banked(points) => {
                let p = &mut self.game.physics;
                p.nitro = (p.nitro + (points / 10000.0).min(0.4)).min(1.0);
                record(&mut self.game.career, "bestChain", points);
                self.show_chain_end(format!("+{} XP", thousands(points)), true);
                self.gain_xp(points);
            }
            ChainEnd::Broken(lost) => {
                self.show_chain_end(format!("Chain lost −{}", thousands(lost)), false);
                if lost > 1000.0 {
                    self.game.speak("Chain lost!", &self.sound);
                }
            }
        }
    }

    fn show_chain_end(&mut self, text: String, banked: bool) {
        self.game.chain_shown = 1.4;
        self.game.chain_end = Some((text, banked));
    }

    /// Adds XP: a new driver level earns a prize spin. Also awards any trophies just won.
    pub fn gain_xp(&mut self, xp: f64) -> Vec<String> {
        let mut lines = Vec::new();
        let levels = add_xp(&mut self.game.career, xp);
        if levels > 0 {
            let level = driver_level(self.game.career.xp).0;
            self.game.popup(
                "🎖",
                format!("Driver level {level}!"),
                format!("+{levels} prize spin{}: spin from the main menu", if levels > 1 { "s" } else { "" }),
            );
            self.sound.chime(true);
            self.game.speak("Level up!", &self.sound);
            lines.push(format!("🎖 Driver level {level}! Prize wheel spin earned"));
        }
        for i in check_trophies(&mut self.game.career) {
            let t = &trophies()[i];
            self.game.popup(
                &t.icon,
                format!("Trophy: {}", t.name),
                format!("{} · +💰 {}", t.about, thousands(t.reward)),
            );
            self.sound.chime(true);
            lines.push(format!("{} Trophy: {} +💰 {}", t.icon, t.name, thousands(t.reward)));
        }
        self.save_career();
        lines
    }

    fn bank_drift(&mut self) {
        let points = self.game.drift_score.round();
        self.game.drift_score = 0.0;
        if points < 50.0 {
            return;
        }
        if let Some(e) = self.game.event.as_mut() {
            e.add_drift(points);
        } else {
            self.game.free_roam.add_drift(points);
        }
        let (x, z) = (self.game.physics.x, self.game.physics.z);
        self.game.happened.push(crate::packs::Happening::Drift { x, z, points });
        self.skill(SkillKind::Drift, points / 2.0, Some(&format!("Drift {}", thousands(points))));
        if points > self.game.best_drift {
            self.game.best_drift = points;
            self.storage.set("carGame.bestDrift", points.to_string());
            self.toast(format!("New best drift: {}!", thousands(points)), 1600.0);
        } else {
            self.toast(format!("Drift +{}", thousands(points)), 1600.0);
        }
    }

    /// Something destroyed: the score, sounds and nitro.
    fn on_destroyed(&mut self, i: usize) {
        let t = self.world.targets[i].clone();
        self.game.happened.push(crate::packs::Happening::Destroyed {
            x: t.x,
            z: t.z,
            kind: t.kind,
        });
        self.game.damage += t.value;
        if let Some(e) = self.game.event.as_mut() {
            e.add_damage(t.value);
        }
        let distance = (t.x - self.game.physics.x).hypot(t.z - self.game.physics.z);
        if !matches!(t.kind, TargetKind::Lamp | TargetKind::Tree) {
            self.game.people.panic(t.x, t.z, 40.0);
        }
        let p = &mut self.game.physics;
        match t.kind {
            TargetKind::Building => {
                p.nitro = (p.nitro + 0.25).min(1.0);
                self.sound.explosion(2.5, distance);
                self.game.shake = self.game.shake.max((12.0 / (distance + 10.0)).min(0.5));
                self.toast(format!("Building destroyed +${}", thousands(t.value)), 1600.0);
                self.skill(SkillKind::Smash, 300.0, Some("Building smash"));
            }
            TargetKind::Car => {
                p.nitro = (p.nitro + 0.1).min(1.0);
                self.sound.explosion(1.4, distance);
                self.toast(format!("Car destroyed +${}", thousands(t.value)), 1600.0);
                self.skill(SkillKind::Smash, 150.0, Some("Car smash"));
            }
            _ => {
                p.nitro = (p.nitro + 0.05).min(1.0);
                self.sound.crash(6.0);
                self.skill(SkillKind::Smash, 25.0, None);
            }
        }
    }

    // ---------- free roam ----------

    fn free_roam_event(&mut self, f: FreeRoamEvent) {
        match f {
            FreeRoamEvent::Wrench(id) => {
                if self.game.career.found.contains(&id) {
                    return;
                }
                self.game.career.found.push(id);
                self.game.career.coins += 500.0;
                self.game.career.earned += 500.0;
                self.skill(SkillKind::Collect, 500.0, Some("Gold wrench"));
                let n = self.game.career.found.len();
                self.game.popup("🔧", format!("Gold wrench {n} found!"), "+💰 500 · +300 XP");
                self.sound.chime(true);
                self.gain_xp(300.0);
            }
            FreeRoamEvent::Zone { zone, score, text } => {
                let stars = zone_stars(&zone, score);
                let star_text = format!("{}{}", "★".repeat(stars as usize), "☆".repeat(3 - stars as usize));
                if !self.game.roaming_in_career_car {
                    self.toast(
                        format!("{}: {text} {star_text} · stars only count in your career car", zone.name),
                        3000.0,
                    );
                    return;
                }
                let had = self.game.career.zones.get(&zone.id).copied().unwrap_or(0.0) as u32;
                if stars > had {
                    let coins = 400.0 * (stars - had) as f64;
                    let c = &mut self.game.career;
                    c.zones.insert(zone.id.clone(), stars as f64);
                    c.coins += coins;
                    c.earned += coins;
                    self.game.popup(
                        "🗺",
                        format!("{}: {star_text}", zone.name),
                        format!("{text} · +💰 {}", thousands(coins)),
                    );
                    self.sound.chime(true);
                    self.gain_xp(250.0 * (stars - had) as f64);
                } else {
                    let best = if had > 0 {
                        format!(" (best {})", "★".repeat(had as usize))
                    } else {
                        String::new()
                    };
                    self.toast(format!("{}: {text} {star_text}{best}", zone.name), 2500.0);
                }
            }
        }
    }

    pub fn start_time_trial(&mut self) {
        let s = self.game.trial.start;
        self.place_car(s.x, s.z, s.heading);
        self.game.trial.begin();
        if let Some(g) = self.game.ghost.take() {
            g.despawn(&mut self.commands);
        }
        if self.game.trial.has_ghost() {
            let config = self.game.config.clone();
            let rig = build_car(&mut self.car_ctx(), &config, false, false);
            let ghost = self.materials.add(StandardMaterial {
                base_color: Color::srgba(0.62, 0.83, 1.0, 0.35),
                emissive: crate::world_build::glow(crate::world_build::hex_color(0x3a7bd5), 0.7),
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            for (e, _) in &rig.lit_parts {
                self.commands
                    .entity(*e)
                    .insert((MeshMaterial3d(ghost.clone()), bevy::light::NotShadowCaster));
            }
            self.commands.entity(rig.root).insert((Visibility::Hidden, crate::carmodel::Ghost));
            self.game.ghost = Some(rig);
        }
    }
}

pub fn setup_game(mut commands: Commands, mut storage: ResMut<Storage>, mut packs: ResMut<crate::packs::Packs>) {
    // Bring the 1.2.x game's progress over, the first time.
    let mut imported = false;
    if storage.is_new()
        && let Some(old) = crate::import::read_old_saves()
    {
        for (k, v) in old {
            storage.set(&k, v);
        }
        storage.set("carGame.imported", "1");
        storage.flush();
        imported = true;
    }
    let mut settings = settings::load_settings(&storage);
    let first_start = settings.quality.is_empty();
    if first_start {
        let preset = crate::quality::pick_preset();
        // An old game's own choices are kept; a new player gets the preset for this computer.
        if !imported {
            settings.apply_preset(preset);
        } else {
            settings.quality = preset.into();
        }
        settings::save_settings(&mut storage, &settings);
    }
    packs.load(&storage);
    let config = config::load_config(&storage);
    let career = career::load_career(&storage);
    let best_drift = storage.get("carGame.bestDrift").and_then(|t| t.parse().ok()).unwrap_or(0.0);
    let mut physics = CarPhysics::new();
    physics.player = true;
    physics.manual = settings.transmission == "manual";
    let map = std::env::var("CAR_GAME_MAP").unwrap_or_else(|_| settings.map.clone());
    let mode = if std::env::var("CAR_GAME_MODE").as_deref() == Ok("drive") {
        Mode::Drive
    } else {
        Mode::Menu
    };
    let mut menu = Menu::new(&career);
    if mode == Mode::Menu {
        menu.screen = Screen::Main;
    }
    let msaa = if settings.antialias { Msaa::Sample4 } else { Msaa::Off };
    let mut game = Game {
        mode,
        drive_config: config.clone(),
        config,
        driving_garage_build: true,
        physics,
        dynamics: None,
        car: None,
        rebuild: true,
        night: false,
        slow_motion: false,
        paused: false,
        accumulator: 0.0,
        shake: 0.0,
        drift_score: 0.0,
        drift_timer: 0.0,
        best_drift,
        air_time: 0.0,
        gear_refused: 0.0,
        help: false,
        toast: Toast::default(),
        popups: Vec::new(),
        fps: (0, 0.0, 0),
        damage: 0.0,
        clock: 0.0,
        real_clock: 0.0,
        load_map: Some((map.clone(), 0)),
        pointer_on_ui: false,
        typing: false,
        screen_shot: false,
        career,
        menu,
        garage: Garage::default(),
        event: None,
        again: None,
        playing: None,
        ended_for: 0.0,
        results_shown: false,
        event_hud: EventHud::default(),
        event_hud_timer: 0.0,
        trial: TimeTrial::default(),
        ghost: None,
        free_roam: FreeRoam::default(),
        traffic: Traffic::default(),
        people: Pedestrians::default(),
        weapons: Weapons::default(),
        chain: SkillChain::default(),
        chain_shown: 0.0,
        chain_end: None,
        drift_held: 0.0,
        drift_gap: 0.0,
        spark_tier: 0,
        draft_tick: 0.0,
        speed_tick: 0.0,
        roaming_in_career_car: false,
        throttle_at: None,
        was_countdown: false,
        wheel_spin: 0.0,
        celebrated: false,
        fireworks: 0.0,
        firework_due: 0.0,
        smoke_due: 0.0,
        sparks_due: 0.0,
        grass_due: 0.0,
        trail_due: 0.0,
        odometer: 0.0,
        last_tips: Vec::new(),
        zone_text: String::new(),
        npc_settings: String::new(),
        speed_lines: 0.0,
        event_entities: Vec::new(),
        first_start,
        loaded_once: false,
        controls: Controls::default(),
        happened: Vec::new(),
        settings,
    };
    if imported {
        game.popup(
            "📦",
            "Progress brought over",
            "Your career, cars and settings from Car Game 1.2 are here.",
        );
    } else if first_start {
        let q = game.settings.quality.clone();
        game.popup(
            "⚙",
            format!("Graphics: {q}"),
            "Picked for this computer. Change it in Garage → Settings.",
        );
    }
    commands.insert_resource(game);
    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 62f32.to_radians(),
            near: 0.1,
            far: 1200.0,
            ..default()
        }),
        bevy::core_pipeline::tonemapping::Tonemapping::AcesFitted,
        msaa,
        Transform::from_xyz(0.0, 3.0, -8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Loads the first map and builds the player's car.
pub fn load_and_build(mut ctx: Ctx) {
    if let Some((map, layout)) = ctx.game.load_map.take() {
        ctx.apply_settings();
        ctx.load_world(&map, layout);
        ctx.after_map();
        ctx.game.loaded_once = true;
        if ctx.game.mode == Mode::Menu {
            ctx.show_career_car();
        }
        // For checking the game unattended: start a career level (CAR_GAME_LEVEL=12) or a quick race (CAR_GAME_LEVEL=quick:race:oval).
        if let Ok(level) = std::env::var("CAR_GAME_LEVEL") {
            if let Some(rest) = level.strip_prefix("quick:") {
                let mut parts = rest.split(':');
                let kind = parts.next().and_then(EventKind::from_id).unwrap_or(EventKind::Race);
                let map = parts.next().unwrap_or("circuit").to_string();
                let car = ctx.game.config.clone();
                let spec = career::quick_race_spec(&career::QuickRaceOptions {
                    kind,
                    map,
                    layout: 0,
                    laps: 2,
                    rivals: 5,
                    difficulty: 1,
                    car: car.clone(),
                    rival_top_speed: 0.0,
                    night: false,
                    weapons: kind == EventKind::Takedown,
                });
                ctx.menu_action(MenuAction::PlayQuick(Box::new(spec), Box::new(car)));
            } else if let Ok(n) = level.parse::<usize>() {
                let mut l = career::make_level(n);
                l.number = n;
                ctx.game.again = Some(Again::Level(n));
                let spec = l.spec.clone();
                let c = &ctx.game.career;
                let car = career::career_car(&ctx.game.config, c.car, &c.upgrades);
                ctx.start_event(spec, car, Some(l), false);
            }
        }
    }
    if ctx.game.rebuild {
        ctx.game.rebuild = false;
        ctx.rebuild_car(None);
    }
}

#[derive(SystemParam)]
pub struct Inputs<'w, 's> {
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    pub mouse: Res<'w, ButtonInput<MouseButton>>,
    pub pads: Query<'w, 's, &'static bevy::input::gamepad::Gamepad>,
    pub motion: Res<'w, bevy::input::mouse::AccumulatedMouseMotion>,
    pub scroll: Res<'w, bevy::input::mouse::AccumulatedMouseScroll>,
    pub time: Res<'w, Time<Real>>,
}

/// One frame of the game.
pub fn frame(mut ctx: Ctx, input: Inputs, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    let elapsed = input.time.delta_secs_f64();
    let real_dt = elapsed.min(MAX_FRAME);
    ctx.game.real_clock += real_dt;
    ctx.game.happened.clear();
    let fps = &mut ctx.game.fps;
    fps.0 += 1;
    fps.1 += elapsed;
    if fps.1 >= 0.5 {
        fps.2 = (fps.0 as f64 / fps.1).round() as u32;
        fps.0 = 0;
        fps.1 = 0.0;
    }
    let typing = ctx.game.typing;
    for action in crate::input::actions(&input.keys, &input.pads, typing) {
        handle_action(&mut ctx, action, &mut windows);
    }
    let on_game = !ctx.game.pointer_on_ui;
    let mut controls = crate::input::read(&input.keys, &input.mouse, &input.pads, typing, on_game);
    if let Some(auto) = crate::dev::autopilot() {
        controls.car.throttle = 1.0;
        controls.car.steer = auto;
    }
    ctx.game.controls = controls;
    if on_game && matches!(ctx.game.mode, Mode::Photo | Mode::Garage | Mode::Menu) && !ctx.game.menu.is_overlay() {
        let drag = if input.mouse.pressed(MouseButton::Left) || input.mouse.pressed(MouseButton::Right) {
            input.motion.delta
        } else {
            Vec2::ZERO
        };
        ctx.rig.drag(drag, input.scroll.delta.y, input.mouse.pressed(MouseButton::Right));
    }

    let driving = ctx.game.is_driving();
    let dt = if !driving {
        0.0
    } else if ctx.game.slow_motion {
        real_dt * SLOW_MOTION
    } else {
        real_dt
    };
    ctx.game.clock += dt;
    if driving && ctx.game.event.is_some() {
        let (state, wrecked) = {
            let e = ctx.game.event.as_ref().unwrap();
            (e.state, e.player_wrecked)
        };
        let held = state != EventState::Running || wrecked > 0.0 || ctx.game.wheel_spin > 0.0;
        if ctx.game.event.as_ref().unwrap().spec.no_nitro {
            controls.car.boost = false;
        }
        launch(&mut ctx, controls.car.throttle);
        ctx.game.wheel_spin = (ctx.game.wheel_spin - dt).max(0.0);
        let stopped = Controls::default();
        let use_controls = if state == EventState::Done {
            Controls {
                car: CarInput {
                    handbrake: true,
                    ..Default::default()
                },
                fire: false,
            }
        } else if held {
            stopped
        } else {
            controls
        };
        drive(&mut ctx, dt, &use_controls);
        if ctx.game.weapons.armed() && !held {
            use_weapons(&mut ctx, dt, controls.fire);
        }
        {
            let Ctx { game, world, sound, .. } = &mut ctx;
            let g = &mut **game;
            if let Some(e) = g.event.as_mut() {
                e.with_player(&mut g.physics, |e| e.update(real_dt, world, sound));
            }
        }
        update_event(&mut ctx, real_dt);
    } else if driving {
        let waiting = ctx.game.trial.active && ctx.game.trial.countdown > 0.0;
        if !waiting {
            drive(&mut ctx, dt, &controls);
        }
        if ctx.game.weapons.armed() && !waiting {
            use_weapons(&mut ctx, dt, controls.fire);
        }
        update_free_roam(&mut ctx, dt);
        update_npcs(&mut ctx, dt);
        update_time_trial(&mut ctx, real_dt);
    }
    for i in std::mem::take(&mut ctx.world.destroyed) {
        ctx.on_destroyed(i);
    }
    if driving && let Some(end) = ctx.game.chain.update(dt) {
        ctx.chain_ended(end);
    }
    if ctx.game.chain.count == 0 {
        ctx.game.chain_shown -= real_dt;
    }
    let p = &ctx.game.physics;
    let intensity = if !driving {
        0.0
    } else {
        match &ctx.game.event {
            Some(e) if e.state == EventState::Countdown => 0.2,
            Some(_) => 0.55 + 0.45 * p.speed_fraction(),
            None => 0.3 + 0.5 * p.speed_fraction(),
        }
    };
    ctx.sound.music(ctx.game.settings.music, intensity);
    let rush = if driving && ctx.game.settings.speed_lines {
        ((p.speed_fraction() - 0.65) / 0.35).max(0.0) * 0.5 + p.draft * 0.4 + if p.boosting { 0.35 } else { 0.0 }
    } else {
        0.0
    };
    ctx.game.speed_lines = rush.min(0.8) as f32;

    sync_car(&mut ctx, dt);
    sync_rivals(&mut ctx, dt);
    if driving {
        car_effects(&mut ctx, dt, controls.car.throttle);
    }
    let lights = match &ctx.game.event {
        Some(e) => e.start_lights(),
        None => ctx.game.trial.start_lights(),
    };
    ctx.world.set_start_lights(lights);
    let (x, z, radius) = (ctx.game.physics.x, ctx.game.physics.z, ctx.game.car_radius());
    ctx.world.update(dt, x, z, radius);
    ctx.focus.x = x;
    ctx.focus.z = z;
    // The orbiting camera at the finish follows the car as it stops.
    if ctx.game.event.as_ref().is_some_and(|e| e.state == EventState::Done) && ctx.game.celebrated {
        let target = ctx.game.car_position() + Vec3::Y * 0.7 * ctx.rig.size;
        let o = &mut ctx.rig.orbit;
        o.target = o.target.lerp(target, (real_dt * 4.0).min(1.0) as f32);
    }
    let (pos, heading, speed, boosting) = (
        ctx.game.car_position(),
        ctx.game.physics.heading as f32,
        ctx.game.physics.speed() as f32,
        ctx.game.physics.boosting,
    );
    ctx.rig.update(real_dt as f32, pos, heading, speed, driving && boosting);

    let p = &ctx.game.physics;
    let slip = if p.airborne { 0.0 } else { (p.lateral_speed.abs() / 7.0).min(1.0) };
    let throttle = if driving { controls.car.throttle } else { 0.0 };
    ctx.sound.update(
        p.revs,
        p.speed_fraction(),
        throttle,
        if driving { slip } else { 0.0 },
        driving,
        driving && p.limiter,
    );
    while ctx.game.traffic.honks > 0 {
        ctx.game.traffic.honks -= 1;
        ctx.sound.traffic_horn();
    }

    let g = &mut *ctx.game;
    g.gear_refused = (g.gear_refused - real_dt).max(0.0);
    if g.toast.left > 0.0 {
        g.toast.left -= real_dt;
    }
    for p in &mut g.popups {
        p.left -= real_dt;
    }
    g.popups.retain(|p| p.left > 0.0);
    ctx.packs.save(&mut ctx.storage);
    if ctx.storage.dirty {
        ctx.storage.flush();
    }
}

pub fn place_camera(
    rig: Res<CameraRig>,
    mut game: ResMut<Game>,
    time: Res<Time<Real>>,
    mut camera: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut t, mut projection)) = camera.single_mut() else { return };
    let mut position = rig.position;
    if game.shake > 0.001 && game.is_driving() {
        position.x += rng::centered() as f32 * game.shake as f32;
        position.y += rng::centered() as f32 * game.shake as f32;
        game.shake *= (-time.delta_secs_f64().min(MAX_FRAME) * 8.0).exp();
    }
    *t = Transform::from_translation(position).looking_at(rig.look_at, Vec3::Y);
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = rig.fov.max(1.0).to_radians();
        p.near = 0.1 * (rig.size / 5.0).max(1.0);
        let reach = 40.0 * rig.size as f64 + game.car.as_ref().map_or(4.5, |c| c.length.max(c.height));
        p.far = (game.settings.view_distance.max(reach * 1.5) + 200.0) as f32;
    }
}

pub fn handle_action(ctx: &mut Ctx, action: Action, windows: &mut Query<&mut Window, With<PrimaryWindow>>) {
    let mode = ctx.game.mode;
    if mode == Mode::Menu && action != Action::Mute && action != Action::Fullscreen {
        return;
    }
    if ctx.game.event.is_some() && ctx.game.menu.visible() && action != Action::Mute && action != Action::Fullscreen {
        if action == Action::Settings && ctx.game.paused {
            ctx.resume_event();
        }
        return;
    }
    match action {
        Action::Night if ctx.game.event.is_none() => {
            ctx.game.night = !ctx.game.night;
            let n = ctx.game.night;
            ctx.world.set_night(n);
            ctx.toast(if n { "Night" } else { "Day" }, 1600.0);
        }
        Action::Mute => {
            let muted = ctx.sound.toggle_mute();
            ctx.storage.set("carGame.muted", if muted { "1" } else { "0" });
            ctx.toast(if muted { "Sound off" } else { "Sound on" }, 1600.0);
        }
        Action::Fullscreen => {
            if let Ok(mut w) = windows.single_mut() {
                w.mode = if matches!(w.mode, WindowMode::Windowed) {
                    WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                } else {
                    WindowMode::Windowed
                };
            }
        }
        _ => {}
    }
    if mode == Mode::Garage {
        if action == Action::Garage || action == Action::Settings {
            ctx.set_mode(Mode::Drive);
        }
        return;
    }
    if mode == Mode::Photo {
        if action == Action::Snapshot {
            ctx.game.screen_shot = true;
        }
        if action == Action::Photo || action == Action::Settings {
            ctx.set_mode(Mode::Drive);
        }
        return;
    }
    if let Some(e) = &ctx.game.event {
        match action {
            Action::Settings => {
                ctx.pause_event();
                return;
            }
            Action::Reset => {
                if e.state == EventState::Running && e.player_wrecked <= 0.0 {
                    let Ctx { game, world, .. } = ctx;
                    let g = &mut **game;
                    if let Some(e) = g.event.as_mut() {
                        e.with_player(&mut g.physics, |e| e.recover(world));
                    }
                }
                return;
            }
            Action::Garage | Action::Trial | Action::SlowMo => {
                ctx.toast("Not during an event · Esc to pause", 1600.0);
                return;
            }
            _ => {}
        }
    }
    match action {
        Action::Garage => ctx.set_mode(Mode::Garage),
        Action::Settings => {
            ctx.set_mode(Mode::Garage);
            ctx.game.garage.tab = crate::garage::Tab::Settings;
        }
        Action::Camera => {
            let m = ctx.rig.next_mode();
            ctx.toast(m.label(), 1600.0);
        }
        Action::Reset => {
            if ctx.game.trial.active {
                ctx.start_time_trial();
            } else {
                let s = ctx.world.spawn;
                ctx.place_car(s.x, s.z, s.heading);
            }
        }
        Action::Help => ctx.game.help = !ctx.game.help,
        Action::Photo => {
            ctx.set_mode(Mode::Photo);
            ctx.toast("Photo mode: drag to look around, Enter to save", 1600.0);
        }
        Action::Horn => {
            let horn = ctx.game.config.horn.clone();
            ctx.sound.horn(&horn);
            let (x, z) = (ctx.game.physics.x, ctx.game.physics.z);
            ctx.game.happened.push(crate::packs::Happening::Horn { x, z });
        }
        Action::ShiftUp | Action::ShiftDown => {
            if !ctx.game.physics.manual {
                ctx.toast("Automatic gearbox: pick Manual in Settings (Esc)", 2200.0);
            } else {
                let ok = if action == Action::ShiftUp {
                    ctx.game.physics.shift_up()
                } else {
                    ctx.game.physics.shift_down()
                };
                if !ok {
                    ctx.game.gear_refused = 0.25;
                }
            }
        }
        Action::SlowMo => ctx.game.slow_motion = !ctx.game.slow_motion,
        Action::Trial => {
            if ctx.game.trial.active {
                ctx.game.trial.stop();
                ctx.toast("Time trial canceled", 1600.0);
            } else {
                ctx.start_time_trial();
            }
        }
        Action::NextWeapon | Action::Weapon1 | Action::Weapon2 | Action::Weapon3 => {
            if !ctx.game.weapons.armed() {
                return;
            }
            if action == Action::NextWeapon {
                ctx.game.weapons.select_next();
            } else {
                let index = match action {
                    Action::Weapon1 => 0,
                    Action::Weapon2 => 1,
                    _ => 2,
                };
                if !ctx.game.weapons.select(index) {
                    let career_car = ctx.game.playing.is_some() || !ctx.game.driving_garage_build;
                    ctx.toast(
                        if career_car {
                            "Not bought yet: weapons are under Cars & upgrades"
                        } else {
                            "Not fitted"
                        },
                        1600.0,
                    );
                    return;
                }
            }
            let name = ctx.game.weapons.name();
            ctx.toast(name, 1600.0);
        }
        _ => {}
    }
}

/// Runs the car's physics for `dt` seconds in fixed steps, and reacts to crashes and landings.
pub fn drive(ctx: &mut Ctx, dt: f64, controls: &Controls) {
    ctx.game.accumulator += dt;
    let mut steps = 0;
    let dynamics = ctx.game.dynamics.clone();
    while ctx.game.accumulator >= STEP && steps < MAX_STEPS {
        let was_airborne = ctx.game.physics.airborne;
        let ev = {
            let Ctx { game, world, .. } = &mut *ctx;
            game.physics.step(STEP, &controls.car, world, &dynamics)
        };
        ctx.game.accumulator -= STEP;
        steps += 1;
        if ctx.game.physics.airborne {
            ctx.game.air_time += STEP;
        } else if was_airborne {
            let t = ctx.game.air_time;
            if t > 0.5
                && let Some(e) = ctx.game.event.as_mut()
            {
                e.add_air(t);
            }
            if t > 0.7 {
                let (x, z) = (ctx.game.physics.x, ctx.game.physics.z);
                ctx.game.happened.push(crate::packs::Happening::Jump { x, z, air_time: t });
                ctx.toast(format!("Air time {t:.1} s"), 1600.0);
                let p = &mut ctx.game.physics;
                p.nitro = (p.nitro + t * 0.08).min(1.0);
                ctx.skill(SkillKind::Air, t * 300.0, Some(&format!("Air {t:.1} s")));
                if t > 1.2 {
                    let (x, z) = (ctx.game.physics.x, ctx.game.physics.z);
                    ctx.game.people.cheer(x, z, 45.0);
                }
                bump(&mut ctx.game.career, "air", t);
            }
            ctx.game.air_time = 0.0;
        }
        if ev.impact > 3.0 {
            let (x, z) = (ctx.game.physics.x, ctx.game.physics.z);
            ctx.game.happened.push(crate::packs::Happening::Crash { x, z, impact: ev.impact });
            ctx.sound.crash(ev.impact);
            ctx.game.shake = (ev.impact * 0.03).min(0.6);
            if ctx.game.drift_score > 0.0 {
                ctx.game.drift_score = 0.0;
                ctx.game.drift_held = 0.0;
                ctx.toast("Crashed! Drift lost", 1600.0);
            }
            if ev.impact > 7.0
                && let Some(end) = ctx.game.chain.break_chain()
            {
                ctx.chain_ended(end);
            }
            if let Some(t) = ev.hit_target {
                if ctx.game.traffic.owns(t) {
                    ctx.game.traffic.hit(t);
                }
                if ev.impact > 6.0 {
                    let p = &ctx.game.physics;
                    let dir = DVec3::new(p.vx, 0.0, p.vz).normalize_or_zero();
                    ctx.world.damage(t, ev.impact * 5.0, dir);
                }
            }
        }
        if ev.smashed > 0 {
            ctx.game.shake = ctx.game.shake.max(0.15);
        }
        if ctx.game.physics.on_grass && ctx.game.physics.speed() > 10.0 {
            ctx.game.shake = ctx.game.shake.max(0.04);
        }
        if ev.cones > 0 {
            ctx.sound.crash(5.0);
        }
        if ev.landed > 6.0 {
            ctx.sound.crash(ev.landed * 0.6);
        }
        if ctx.game.event.is_some() {
            let hit = {
                let Ctx { game, world, sound, .. } = &mut *ctx;
                let g = &mut **game;
                let e = g.event.as_mut().unwrap();
                e.with_player(&mut g.physics, |e| e.step(STEP, world, sound))
            };
            if hit > 3.0 {
                ctx.sound.crash(hit);
                ctx.game.shake = ctx.game.shake.max((hit * 0.03).min(0.5));
            }
            if hit > 7.0
                && let Some(end) = ctx.game.chain.break_chain()
            {
                ctx.chain_ended(end);
            }
        }
    }
    if ctx.game.accumulator > STEP {
        ctx.game.accumulator = 0.0;
    }
    let p = &ctx.game.physics;
    let speed = p.speed();
    let slip_angle = p.lateral_speed.abs().atan2(p.forward_speed.abs());
    let drifting = !p.airborne && speed > 8.0 && slip_angle > 0.2 && p.forward_speed > 0.0;
    if drifting {
        let g = &mut *ctx.game;
        g.drift_score += speed * slip_angle * dt * 10.0;
        g.drift_timer = 0.8;
        g.physics.nitro = (g.physics.nitro + dt * 0.06).min(1.0);
    } else if ctx.game.drift_timer > 0.0 {
        ctx.game.drift_timer -= dt;
        if ctx.game.drift_timer <= 0.0 {
            ctx.bank_drift();
        }
    }
    mini_turbo(ctx, dt, drifting);
    steady_skills(ctx, dt);
    let g = &mut *ctx.game;
    if let Some(e) = g.event.as_mut() {
        e.unbanked_drift = if g.drift_score >= 50.0 { g.drift_score.round() } else { 0.0 };
        e.unbanked_air = if g.physics.airborne && g.air_time > 0.5 { g.air_time } else { 0.0 };
    }
    if !g.physics.boosting {
        g.physics.nitro = (g.physics.nitro + dt * 0.03).min(1.0);
    }
}

/// The drift mini-turbo: a drift held 1, 2 or 3 seconds charges a blue, orange or purple boost.
fn mini_turbo(ctx: &mut Ctx, dt: f64, drifting: bool) {
    if drifting {
        ctx.game.drift_held += dt;
        ctx.game.drift_gap = 0.0;
    } else if ctx.game.drift_held > 0.0 {
        ctx.game.drift_gap += dt;
        if ctx.game.drift_gap > 0.15 {
            let tier = ctx.game.spark_tier;
            if tier > 0 && ctx.game.physics.forward_speed > 5.0 {
                ctx.game.physics.give_turbo([0.6, 1.0, 1.5][tier - 1], [1.15, 1.25, 1.35][tier - 1]);
                ctx.skill(
                    SkillKind::DriftBoost,
                    150.0 * tier as f64,
                    Some(&format!("{} boost", SPARK_NAMES[tier - 1])),
                );
                ctx.sound.chime(false);
            }
            ctx.game.drift_held = 0.0;
        }
    }
    let h = ctx.game.drift_held;
    ctx.game.spark_tier = if h >= 3.0 {
        3
    } else if h >= 2.0 {
        2
    } else if h >= 1.0 {
        1
    } else {
        0
    };
}

fn steady_skills(ctx: &mut Ctx, dt: f64) {
    let p = &ctx.game.physics;
    if p.draft > 0.5 && p.speed() > 15.0 {
        ctx.game.draft_tick += dt;
        bump(&mut ctx.game.career, "draft", dt);
        if ctx.game.draft_tick >= 1.0 {
            ctx.game.draft_tick -= 1.0;
            ctx.skill(SkillKind::Draft, 60.0, None);
        }
    }
    let p = &ctx.game.physics;
    if p.speed_fraction() > 0.9 && !p.airborne {
        ctx.game.speed_tick += dt;
        if ctx.game.speed_tick >= 1.0 {
            ctx.game.speed_tick -= 1.0;
            ctx.skill(SkillKind::Speed, 80.0, None);
        }
    } else {
        ctx.game.speed_tick = 0.0;
    }
    if ctx.game.event.is_some() {
        let mph = (ctx.game.physics.speed() * 2.237).round();
        record(&mut ctx.game.career, "topSpeed", mph);
    }
}

/// Fires weapons, and shakes and pushes the car when explosions go off near it.
fn use_weapons(ctx: &mut Ctx, dt: f64, firing: bool) {
    let Some(car) = &ctx.game.car else { return };
    let p = &ctx.game.physics;
    let frame = CarFrame {
        position: DVec3::new(p.x, p.y, p.z),
        rotation: bevy::math::DQuat::from_euler(EulerRot::YXZ, p.heading, p.pitch, p.ground_roll),
        scale: car.config.size,
        gun_muzzle: car.gun_muzzle.as_dvec3(),
        rocket_muzzle: car.rocket_muzzle.as_dvec3(),
    };
    let velocity = DVec3::new(p.vx, p.vy, p.vz);
    let blasts = {
        let Ctx { game, world, sound, .. } = &mut *ctx;
        game.weapons.update(dt, firing, &frame, velocity, world, sound)
    };
    if ctx.game.event.is_some() {
        let Ctx { game, world, sound, .. } = &mut *ctx;
        let g = &mut **game;
        if let Some(e) = g.event.as_mut() {
            e.with_player(&mut g.physics, |e| e.take_hits(world, sound));
        }
    }
    for blast in blasts {
        let car_pos = DVec3::new(ctx.game.physics.x, ctx.game.physics.y, ctx.game.physics.z);
        let distance = blast.at.distance(car_pos);
        ctx.sound.explosion(1.5, distance);
        ctx.game.happened.push(crate::packs::Happening::Explosion {
            x: blast.at.x,
            z: blast.at.z,
        });
        ctx.game.people.panic(blast.at.x, blast.at.z, blast.radius * 3.0 + 12.0);
        ctx.game.shake = ctx.game.shake.max((6.0 / (distance + 4.0)).min(0.7));
        let reach = blast.radius * 1.3;
        if distance < reach {
            let strength = 1.0 - distance / reach;
            let away = DVec3::new(car_pos.x - blast.at.x, 0.0, car_pos.z - blast.at.z).normalize_or_zero();
            ctx.game
                .physics
                .push(away.x * 12.0 * strength, 7.0 * strength, away.z * 12.0 * strength);
        }
        if blast.radius == 0.0 {
            ctx.game.physics.push(0.0, 3.0, 0.0);
        }
    }
}

fn update_time_trial(ctx: &mut Ctx, dt: f64) {
    let p = &ctx.game.physics;
    let pose = CarPose {
        x: p.x,
        y: p.y,
        z: p.z,
        heading: p.heading,
        pitch: p.pitch,
        ground_roll: p.ground_roll,
    };
    let clock = ctx.game.real_clock;
    let event = {
        let Ctx { game, storage, .. } = &mut *ctx;
        game.trial.update(dt, pose, clock, storage)
    };
    match event {
        Some(TrialEvent::Go) => {
            ctx.toast("GO!", 900.0);
            ctx.sound.chime(false);
        }
        Some(TrialEvent::Checkpoint) => ctx.sound.chime(false),
        Some(TrialEvent::Finish) => {
            ctx.sound.chime(true);
            let t = &ctx.game.trial;
            let text = if t.new_best {
                format!("New best lap: {}!", format_time(t.time))
            } else {
                format!("Lap {}  (best {})", format_time(t.time), format_time(t.best))
            };
            ctx.toast(text, 4000.0);
        }
        None => {}
    }
}

fn update_free_roam(ctx: &mut Ctx, dt: f64) {
    let p = &ctx.game.physics;
    let (pos, speed, airborne) = (DVec3::new(p.x, p.y, p.z), p.speed(), p.airborne);
    let (drift, air, reach) = (ctx.game.drift_score, ctx.game.air_time, ctx.game.car_radius());
    let found = ctx.game.free_roam.update(dt, pos, speed, airborne, drift, air, reach);
    for f in found {
        ctx.free_roam_event(f);
    }
    ctx.game.zone_text = ctx.game.free_roam.hud(pos.x, pos.z, drift);
}

fn update_npcs(ctx: &mut Ctx, dt: f64) {
    let p = &ctx.game.physics;
    let (width, length) = ctx.game.car.as_ref().map_or((1.9, 4.4), |c| (c.width, c.length));
    let player = TrafficPlayer {
        x: p.x,
        y: p.y,
        z: p.z,
        vx: p.vx,
        vz: p.vz,
        width,
        length,
    };
    let near = CarNear {
        x: p.x,
        z: p.z,
        vx: p.vx,
        vz: p.vz,
        reach: ctx.game.car_radius(),
    };
    let misses = {
        let Ctx { game, world, .. } = &mut *ctx;
        game.traffic.update(dt, player, world)
    };
    for points in misses {
        let (x, z) = (ctx.game.physics.x, ctx.game.physics.z);
        ctx.game.happened.push(crate::packs::Happening::NearMiss { x, z });
        ctx.skill(SkillKind::NearMiss, points, Some("Near miss: traffic"));
        ctx.toast("Near miss!", 700.0);
    }
    ctx.game.people.update(dt, near);
}

/// Launch control: throttle in the last 0.6 s of the countdown is a perfect start; too early spins the wheels.
fn launch(ctx: &mut Ctx, throttle: f64) {
    let Some(e) = &ctx.game.event else { return };
    if e.state == EventState::Countdown {
        let countdown = e.countdown;
        ctx.game.was_countdown = true;
        if throttle > 0.5 {
            ctx.game.throttle_at.get_or_insert(countdown);
        } else {
            ctx.game.throttle_at = None;
        }
        return;
    }
    if !ctx.game.was_countdown {
        return;
    }
    ctx.game.was_countdown = false;
    match ctx.game.throttle_at.take() {
        Some(t) if t <= 0.6 => {
            ctx.game.physics.give_turbo(1.2, 1.3);
            ctx.skill(SkillKind::PerfectStart, 500.0, None);
            ctx.toast("PERFECT START!", 1200.0);
            ctx.game.speak("Perfect start!", &ctx.sound);
        }
        Some(t) if t > 1.2 => {
            ctx.game.wheel_spin = 0.7;
            ctx.toast("Wheelspin! Too early", 1200.0);
        }
        _ => {}
    }
}

/// Each frame of an event: messages, the HUD, and the results at the end.
fn update_event(ctx: &mut Ctx, dt: f64) {
    let (notes, skills, callouts) = {
        let e = ctx.game.event.as_mut().unwrap();
        (
            std::mem::take(&mut e.notes),
            std::mem::take(&mut e.skills),
            std::mem::take(&mut e.callouts),
        )
    };
    for note in notes {
        let ms = if note == "GO!" {
            900.0
        } else if note.starts_with('😈') {
            3000.0
        } else {
            1800.0
        };
        ctx.toast(note, ms);
    }
    for (kind, points, label) in skills {
        ctx.skill(kind, points, label.as_deref());
    }
    for line in callouts {
        ctx.game.speak(&line, &ctx.sound);
    }
    let done = ctx.game.event.as_ref().unwrap().state == EventState::Done;
    if done && !ctx.game.celebrated {
        celebrate(ctx);
    }
    if ctx.game.fireworks > 0.0 {
        ctx.game.fireworks -= dt;
        ctx.game.firework_due -= dt;
        if ctx.game.firework_due <= 0.0 {
            ctx.game.firework_due = 0.3 + rng::random() * 0.3;
            let colors = [0xff3b3b, 0xffd23f, 0x3bb273, 0x1b98e0, 0xc62f9e, 0x7af0ff];
            let a = rng::random() * std::f64::consts::TAU;
            let s = ctx.rig.size as f64;
            let p = &ctx.game.physics;
            let at = DVec3::new(
                p.x + a.sin() * 12.0 * s,
                p.y + (10.0 + rng::random() * 6.0) * s,
                p.z + a.cos() * 12.0 * s,
            );
            ctx.world.effects.firework(at, rng::pick(&colors), s);
            ctx.sound.explosion(0.4, 30.0);
        }
    }
    ctx.game.event_hud_timer -= dt;
    if ctx.game.event_hud_timer <= 0.0 {
        ctx.game.event_hud_timer = 0.1;
        let hud = ctx.game.event.as_ref().unwrap().hud();
        ctx.game.event_hud = hud;
    }
    if done && !ctx.game.results_shown {
        ctx.game.ended_for += dt;
        if ctx.game.ended_for > 1.5 {
            show_results(ctx);
        }
    }
}

fn celebrate(ctx: &mut Ctx) {
    ctx.game.celebrated = true;
    if let Some(end) = ctx.game.chain.bank() {
        ctx.chain_ended(end);
    }
    let Some(r) = ctx.game.event.as_ref().and_then(|e| e.result.clone()) else {
        return;
    };
    if r.stars == 0 {
        return;
    }
    ctx.game.fireworks = if r.stars == 3 || r.position == 1 { 4.0 } else { 2.0 };
    ctx.game.firework_due = 0.0;
    let p = ctx.game.car_position().as_dvec3();
    ctx.world.effects.confetti(p, 60);
    let (pos, heading) = (ctx.game.car_position(), ctx.game.physics.heading as f32);
    ctx.rig.set_garage(true, pos, heading, false);
}

/// Pays out and shows the results (or, after level 100, the end of the career).
fn show_results(ctx: &mut Ctx) {
    ctx.game.results_shown = true;
    let e = ctx.game.event.as_ref().unwrap();
    let r = e.result.clone().unwrap();
    let spec = e.spec.clone();
    let lower = e.lower_is_better();
    let is_race = e.is_race();
    if r.stars > 0 {
        ctx.sound.chime(true);
    }
    let extras = reward_event(ctx, &r, is_race);
    let Some(level) = ctx.game.playing.clone() else {
        ctx.game.menu.screen = Screen::Results {
            spec: Box::new(spec),
            result: Box::new(r),
            payout: None,
            next: None,
            extras,
        };
        return;
    };
    let payout = if level.number == 0 {
        career::pay_daily(&mut ctx.game.career, &level, r.stars)
    } else {
        career::pay_level(
            &mut ctx.game.career,
            &level,
            &career::LevelResult {
                stars: r.stars,
                score: r.score,
                score_text: r.score_text.clone(),
                position: r.position,
                lower_is_better: lower,
            },
        )
    };
    ctx.save_career();
    let mut next = None;
    if level.number > 0 && r.passed {
        if level.number == LEVELS && career::awaiting_choice(&ctx.game.career) {
            ctx.game.menu.screen = Screen::CareerComplete;
            return;
        }
        let c = &ctx.game.career;
        let n = if level.number >= LEVELS {
            if c.mode == "sandbox" { c.sandbox_level } else { 0 }
        } else {
            level.number + 1
        };
        if n > LEVELS || (n > 0 && career::locked_reason(c, n).is_empty()) {
            next = Some(Box::new(career::make_level(n)));
        }
    }
    ctx.game.menu.screen = Screen::Results {
        spec: Box::new(spec),
        result: Box::new(r),
        payout: Some(payout),
        next,
        extras,
    };
}

/// XP for an event, and the nemesis: lines for the results screen.
fn reward_event(ctx: &mut Ctx, r: &crate::event::EventResult, is_race: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut xp = 1000.0 + 1000.0 * r.stars as f64;
    if is_race && r.standings.len() > 1 {
        let mine = r.standings.iter().position(|s| s.me).unwrap_or(0);
        for (i, s) in r.standings.iter().enumerate() {
            if s.me || ctx.game.career.nemesis.get(&s.name).copied().unwrap_or(0.0) < 2.0 || i < mine {
                continue;
            }
            ctx.game.career.nemesis.insert(s.name.clone(), 0.0);
            bump(&mut ctx.game.career, "revenge", 1.0);
            xp += 1500.0;
            lines.push(format!("😤 Revenge on {}! +1,500 XP", s.name));
            ctx.game.speak(&format!("Revenge on {}!", s.name), &ctx.sound);
        }
        for s in r.standings.iter().take(mine) {
            let wins = ctx.game.career.nemesis.get(&s.name).copied().unwrap_or(0.0) + 1.0;
            ctx.game.career.nemesis.insert(s.name.clone(), wins);
            if wins == 2.0 {
                lines.push(format!("😈 {} has beaten you twice: they're your nemesis now", s.name));
            }
        }
    }
    lines.insert(0, format!("+{} XP", thousands(xp)));
    lines.extend(ctx.gain_xp(xp));
    lines
}

/// Moves the car's model to match the physics.
pub fn sync_car(ctx: &mut Ctx, dt: f64) {
    let Some(car) = ctx.game.car.clone() else { return };
    let p = &ctx.game.physics;
    let root = Transform::from_translation(ctx.game.car_position())
        .with_rotation(Quat::from_euler(
            EulerRot::YXZ,
            p.heading as f32,
            p.pitch as f32,
            p.ground_roll as f32,
        ))
        .with_scale(Vec3::splat(car.config.size as f32));
    let body =
        Transform::from_xyz(0.0, car.body_y, 0.0).with_rotation(Quat::from_euler(EulerRot::XYZ, p.body_pitch as f32, 0.0, p.roll as f32));
    let night = ctx.world.is_night();
    let driving = ctx.game.mode == Mode::Drive;
    let braking = p.braking && driving;
    let boosting = p.boosting && driving;
    let wrecked = ctx.game.event.as_ref().is_some_and(|e| e.player_wrecked > 0.0);
    set_transform(&mut ctx.commands, car.root, root);
    set_transform(&mut ctx.commands, car.body, body);
    spin_wheels(&mut ctx.commands, &car, p.forward_speed, p.steer_angle, dt);
    let clock = ctx.game.real_clock;
    let Ctx {
        game, materials, commands, ..
    } = ctx;
    if let Some(rig) = game.car.as_mut() {
        rig.set_wrecked(wrecked, materials, commands);
        if rig.lights != (night, braking) {
            rig.set_lights(night, braking, materials, commands);
        }
        rig.set_nitro(boosting, commands);
        rig.animate(clock, materials, commands);
    }
}

pub fn set_transform(commands: &mut Commands, e: Entity, t: Transform) {
    commands.entity(e).queue(move |mut e: EntityWorldMut| {
        if let Some(mut tr) = e.get_mut::<Transform>() {
            *tr = t;
        }
    });
}

pub fn spin_wheels(commands: &mut Commands, car: &CarRig, forward: f64, steer: f64, dt: f64) {
    for w in &car.wheels {
        let turn = (forward / w.radius * dt) as f32;
        commands.entity(w.spin).queue(move |mut e: EntityWorldMut| {
            if let Some(mut t) = e.get_mut::<Transform>() {
                t.rotate_local_x(turn);
            }
        });
        if w.front {
            commands.entity(w.steer).queue(move |mut e: EntityWorldMut| {
                if let Some(mut t) = e.get_mut::<Transform>() {
                    t.rotation = Quat::from_rotation_y(steer as f32);
                }
            });
        }
    }
}

/// Moves the rivals', ghost's and traffic's models.
fn sync_rivals(ctx: &mut Ctx, dt: f64) {
    let night = ctx.world.is_night();
    let clock = ctx.game.real_clock;
    let Ctx {
        game,
        materials,
        commands,
        world,
        ..
    } = ctx;
    let g = &mut **game;
    if let Some(e) = g.event.as_mut() {
        for v in std::mem::take(&mut e.visuals) {
            match v {
                EventVisual::Wreck(i, on) => {
                    if let Some(rig) = e.racers[i].rival.as_mut().and_then(|r| r.rig.as_mut()) {
                        rig.set_wrecked(on, materials, commands);
                    }
                }
                EventVisual::Hide(i) => {
                    if let Some(rig) = e.racers[i].rival.as_ref().and_then(|r| r.rig.as_ref()) {
                        commands.entity(rig.root).insert(Visibility::Hidden);
                    }
                }
            }
        }
        for r in e.racers.iter_mut().skip(1) {
            let p = &r.physics;
            let Some(rig) = r.rival.as_mut().and_then(|v| v.rig.as_mut()) else {
                continue;
            };
            let root = Transform::from_xyz(p.x as f32, p.y as f32, p.z as f32)
                .with_rotation(Quat::from_euler(
                    EulerRot::YXZ,
                    p.heading as f32,
                    p.pitch as f32,
                    p.ground_roll as f32,
                ))
                .with_scale(Vec3::splat(rig.config.size as f32));
            set_transform(commands, rig.root, root);
            set_transform(
                commands,
                rig.body,
                Transform::from_xyz(0.0, rig.body_y, 0.0).with_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    p.body_pitch as f32,
                    0.0,
                    p.roll as f32,
                )),
            );
            spin_wheels(commands, rig, p.forward_speed, p.steer_angle, dt);
            if rig.lights != (night, p.braking) {
                rig.set_lights(night, p.braking, materials, commands);
            }
            rig.set_nitro(p.boosting, commands);
            rig.animate(clock, materials, commands);
        }
    }
    // The ghost of the best lap.
    if let Some(ghost) = &g.ghost {
        match g.trial.ghost_pose() {
            Some(pose) if g.mode != Mode::Garage => {
                let t = Transform::from_xyz(pose.x as f32, pose.y as f32, pose.z as f32).with_rotation(Quat::from_euler(
                    EulerRot::YXZ,
                    pose.heading as f32,
                    pose.pitch as f32,
                    pose.ground_roll as f32,
                ));
                set_transform(commands, ghost.root, t.with_scale(Vec3::splat(ghost.config.size as f32)));
                commands.entity(ghost.root).insert(Visibility::Inherited);
            }
            _ => {
                commands.entity(ghost.root).insert(Visibility::Hidden);
            }
        }
    }
    // Traffic: wheels and brake lights.
    for (t, speed, steer, braking) in g.traffic.visuals() {
        let target = &mut world.targets[t];
        if let Some(rig) = target.rig.as_mut() {
            spin_wheels(commands, rig, speed, steer, dt);
            if rig.lights != (night, braking) && target.alive {
                rig.set_lights(night, braking, materials, commands);
            }
        }
    }
}

/// Tire smoke, mini-turbo sparks, thrown-up grass and the boost trail.
fn car_effects(ctx: &mut Ctx, dt: f64, throttle: f64) {
    use crate::effects::{ParticleOptions, rgb};
    let Some(car) = ctx.game.car.clone() else { return };
    let size = ctx.rig.size as f64;
    let g = &mut *ctx.game;
    let p = &g.physics;
    let amount = ctx.world.effects.amount;
    let root = bevy::math::DQuat::from_euler(EulerRot::YXZ, p.heading, p.pitch, p.ground_roll);
    let origin = DVec3::new(p.x, p.y, p.z);
    let scale = car.config.size;
    let to_world = |local: DVec3| origin + root * (local * scale);
    let wheel_at = |i: usize| {
        let s = &car.shape;
        let front = i < 2;
        let side = if i.is_multiple_of(2) { -1.0 } else { 1.0 };
        let z = if front { s.spec.axle_front } else { s.spec.axle_rear } as f64;
        to_world(DVec3::new(side * s.wheel_x as f64, 0.0, z))
    };
    let sliding = !p.airborne && p.lateral_speed.abs() > 4.0 && p.speed() > 4.0;
    let burnout = !p.airborne && throttle > 0.9 && p.speed() < 6.0 && p.forward_speed >= 0.0;
    g.smoke_due += if g.drive_config.tire_smoke && (sliding || burnout) {
        dt * 50.0 * amount
    } else {
        0.0
    };
    let smoke_color = crate::canvas::Rgba::css(&g.drive_config.tire_smoke_color);
    while g.smoke_due >= 1.0 {
        g.smoke_due -= 1.0;
        let mut spot = wheel_at(2 + rng::index(2));
        spot.y = p.y + 0.15 * size;
        let v = DVec3::new(rng::centered() * 2.0, 0.6 + rng::random(), rng::centered() * 2.0) * size;
        ctx.world.effects.add_smoke(ParticleOptions {
            at: spot,
            velocity: v,
            life: 1.6,
            size: [0.5 * size, 2.6 * size],
            color: [smoke_color.0, smoke_color.1, smoke_color.2],
            opacity: 0.55,
            drag: 1.5,
            ..Default::default()
        });
    }
    if g.spark_tier > 0 && g.drift_held > 0.0 {
        g.sparks_due += dt * 70.0 * amount;
        while g.sparks_due >= 1.0 {
            g.sparks_due -= 1.0;
            let mut spot = wheel_at(2 + rng::index(2));
            spot.y = p.y + 0.1 * size;
            let v = DVec3::new(
                -p.vx * 0.2 + rng::centered() * 4.0,
                1.0 + rng::random() * 3.0,
                -p.vz * 0.2 + rng::centered() * 4.0,
            ) * size;
            ctx.world.effects.add_glow(ParticleOptions {
                at: spot,
                velocity: v,
                life: 0.35,
                size: [0.22 * size, 0.04 * size],
                color: rgb(SPARK_COLORS[g.spark_tier - 1]),
                rise: -12.0,
                ..Default::default()
            });
        }
    }
    g.grass_due += if p.on_grass && p.speed() > 5.0 {
        dt * p.speed().min(60.0) * amount
    } else {
        0.0
    };
    while g.grass_due >= 1.0 {
        g.grass_due -= 1.0;
        let mut spot = wheel_at(rng::index(4));
        spot.y = p.y + 0.2 * size;
        let v = DVec3::new(
            -p.vx * 0.15 + rng::centered() * 2.0,
            1.0 + rng::random() * 2.0,
            -p.vz * 0.15 + rng::centered() * 2.0,
        ) * size;
        let color = if rng::random() < 0.5 { 0x7a6a48 } else { 0x5f7d3a };
        ctx.world.effects.add_smoke(ParticleOptions {
            at: spot,
            velocity: v,
            life: 0.9,
            size: [0.4 * size, 1.6 * size],
            color: rgb(color),
            opacity: 0.6,
            drag: 2.0,
            ..Default::default()
        });
    }
    g.trail_due += if p.boosting {
        dt * 40.0 * (p.speed() / 25.0).max(1.0) * amount
    } else {
        0.0
    };
    g.trail_due = g.trail_due.min(40.0);
    let start_distance = g.odometer;
    g.odometer += p.speed() * dt;
    let tips: Vec<DVec3> = car.exhausts.iter().map(|e| to_world(e.as_dvec3())).collect();
    if g.last_tips.len() != tips.len() {
        g.last_tips = tips.clone();
    }
    let count = g.trail_due.floor() as usize;
    g.trail_due -= count as f64;
    let nitro = crate::canvas::Rgba::css(&g.drive_config.nitro_color);
    let trail = g.drive_config.boost_trail.clone();
    for k in 1..=count {
        for (i, tip) in tips.iter().enumerate() {
            let mut at = g.last_tips[i].lerp(*tip, k as f64 / count as f64);
            let fx = &mut ctx.world.effects;
            match trail.as_str() {
                "rainbow" => {
                    at.y += 0.15 * size;
                    let hue = ((start_distance + (g.odometer - start_distance) * (k as f64 / count as f64)) / 4.0) % 1.0;
                    let c = crate::canvas::hsl(hue as f32, 1.0, 0.5);
                    fx.add_smoke(ParticleOptions {
                        at,
                        life: 0.8,
                        size: [0.4 * size, 0.25 * size],
                        color: [c.0, c.1, c.2],
                        opacity: 0.9,
                        ..Default::default()
                    });
                }
                "sparkles" => {
                    let v = DVec3::new(rng::centered() * 6.0, rng::random() * 4.0, rng::centered() * 6.0) * size;
                    let color = if rng::random() < 0.5 { 0xffffff } else { 0xffd23f };
                    fx.add_glow(ParticleOptions {
                        at,
                        velocity: v,
                        life: 0.6,
                        size: [0.18 * size, 0.02 * size],
                        color: rgb(color),
                        rise: -6.0 * size,
                        ..Default::default()
                    });
                }
                "smoke" => {
                    fx.add_smoke(ParticleOptions {
                        at,
                        velocity: DVec3::new(0.0, 0.5 * size, 0.0),
                        life: 1.4,
                        size: [0.4 * size, 2.0 * size],
                        color: rgb(0x3a3a3a),
                        opacity: 0.6,
                        drag: 1.0,
                        ..Default::default()
                    });
                }
                _ => {
                    let v = DVec3::new(rng::centered() * 2.0, rng::random(), rng::centered() * 2.0) * size;
                    fx.add_glow(ParticleOptions {
                        at,
                        velocity: v,
                        life: 0.3,
                        size: [0.25 * size, 0.05 * size],
                        color: [nitro.0, nitro.1, nitro.2],
                        ..Default::default()
                    });
                }
            }
        }
    }
    g.last_tips = tips;
}

pub fn gear_text(p: &CarPhysics) -> String {
    if p.forward_speed < -0.5 { "R".into() } else { p.gear.to_string() }
}

pub fn shift_light(p: &CarPhysics) -> bool {
    p.manual && p.revs.clamp(0.0, 1.0) > 0.92 && p.gear < GEAR_TOPS.len()
}

/// The menus and the garage, drawn with egui, and what they ask for.
pub fn menus(mut ctx: Ctx, mut contexts: bevy_egui::EguiContexts) -> Result {
    let egui_ctx = contexts.ctx_mut()?.clone();
    if ctx.game.screen_shot {
        return Ok(()); // no menus in a photo
    }
    let mph = ctx.game.settings.is_mph();
    let clock = ctx.game.real_clock;
    let looks = ctx.game.config.clone();
    let actions = {
        let g = &mut *ctx.game;
        crate::menu::show(&egui_ctx, &mut g.menu, &mut g.career, &looks, mph, clock)
    };
    for a in actions {
        ctx.menu_action(a);
    }
    if ctx.game.mode == Mode::Garage {
        let describe = ctx.game.describe_car();
        let actions = {
            let Ctx { game, storage, packs, .. } = &mut ctx;
            let g = &mut **game;
            crate::garage::show(&egui_ctx, &mut g.garage, &mut g.config, &mut g.settings, storage, packs, &describe)
        };
        for a in actions {
            ctx.garage_action(a);
        }
    }
    ctx.game.pointer_on_ui = egui_ctx.is_pointer_over_egui();
    ctx.game.typing = egui_ctx.egui_wants_keyboard_input();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarting_or_advancing_an_event_returns_control_to_the_player() {
        let mut storage = Storage::in_memory();
        storage.set("carGame.settings.v1", r#"{"quality":"Low","announcer":false}"#);
        storage.set("carGame.muted", "1");
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            crate::audio::AudioPlugin,
            crate::world_render::WorldRenderPlugin,
        ))
        .insert_resource(storage)
        .insert_resource(World::empty())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<CameraRig>()
        .init_resource::<crate::packs::Packs>()
        .add_systems(Startup, setup_game);
        app.world_mut().run_schedule(Startup);
        app.world_mut()
            .run_system_cached(|mut ctx: Ctx, mut windows: Query<&mut Window, With<PrimaryWindow>>| {
                fn check_countdown(ctx: &mut Ctx, windows: &mut Query<&mut Window, With<PrimaryWindow>>, level: usize) {
                    assert_eq!(ctx.game.playing.as_ref().unwrap().number, level);
                    assert_eq!(ctx.game.event.as_ref().unwrap().state, EventState::Countdown);
                    assert!(!ctx.game.paused);
                    assert!(!ctx.game.menu.visible());
                    let previous_camera = ctx.rig.mode;
                    handle_action(ctx, Action::Camera, windows);
                    assert_ne!(ctx.rig.mode, previous_camera);
                }

                ctx.menu_action(MenuAction::PlayLevel(Box::new(career::make_level(1))));
                ctx.pause_event();
                assert!(matches!(ctx.game.menu.screen, Screen::Pause(_)));
                ctx.menu_action(MenuAction::Restart);
                check_countdown(&mut ctx, &mut windows, 1);

                for advance in [false, true] {
                    let event = ctx.game.event.as_mut().unwrap();
                    event.state = EventState::Done;
                    event.result = Some(crate::event::EventResult {
                        stars: 3,
                        position: 1,
                        score_text: "1st".into(),
                        score: 1.0,
                        passed: true,
                        standings: Vec::new(),
                        best_lap: 0.0,
                    });
                    show_results(&mut ctx);
                    let Screen::Results { next: Some(next), .. } = &ctx.game.menu.screen else {
                        panic!("Level 2 should be unlocked on the results screen");
                    };
                    let action = if advance {
                        MenuAction::PlayLevel(next.clone())
                    } else {
                        MenuAction::Restart
                    };
                    ctx.menu_action(action);
                    check_countdown(&mut ctx, &mut windows, if advance { 2 } else { 1 });
                }
            })
            .unwrap();
    }
}
