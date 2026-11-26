//! The game loop: the player's car, the modes (menu, garage, driving, photo),
//! fixed-step physics, drifting, crashes and the cameras. Events, the career,
//! weapons, traffic and the packs hook in from their own files.
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::audio::Sound;
use crate::camera::CameraRig;
use crate::carmodel::{CarBuildCtx, CarRig, build_car};
use crate::config::{self, CarConfig};
use crate::physics::{CarPhysics, Dynamics, GEAR_TOPS};
use crate::settings::{self, Settings};
use crate::storage::Storage;
use crate::world::World;
use crate::world_build::{Build, WorldMats};
use crate::world_render::{Focus, Graphics, MainCamera};

/// Physics runs at a fixed 120 steps per second.
pub const STEP: f64 = 1.0 / 120.0;
/// The longest frame the game catches up on, in seconds.
pub const MAX_FRAME: f64 = 0.25;
const MAX_STEPS: usize = 30;
pub const SLOW_MOTION: f64 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Menu,
    Garage,
    Drive,
    Photo,
}

/// A short message in the middle of the screen.
#[derive(Clone, Debug, Default)]
pub struct Toast {
    pub text: String,
    pub left: f64,
}

/// A bigger message that slides in at the side: trophies, level ups, finds.
#[derive(Clone, Debug)]
pub struct Popup {
    pub icon: String,
    pub title: String,
    pub text: String,
    pub left: f64,
}

/// Everything about the game that isn't the map.
#[derive(Resource)]
pub struct Game {
    pub mode: Mode,
    pub settings: Settings,
    /// The garage build: what you drive in free roam.
    pub config: CarConfig,
    /// The car being driven now: the garage build, or the car for the event.
    pub drive_config: CarConfig,
    /// True while driving the garage build (not a career or event car).
    pub driving_garage_build: bool,
    pub physics: CarPhysics,
    pub dynamics: Dynamics,
    pub car: Option<CarRig>,
    /// Set to build the player's car again (after a change in the garage).
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
    /// Seconds of game time, for animations.
    pub clock: f64,
    /// Real seconds since the start.
    pub real_clock: f64,
    /// The map to load next (set by menus, the garage and events).
    pub load_map: Option<(String, u32)>,
    pub wheel_spin: f64,
    /// The mouse is over a menu (so clicks don't fire guns or turn the camera).
    pub pointer_on_ui: bool,
    pub typing: bool,
    pub screen_shot: bool,
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

    /// How far the car's footprint reaches from its center: half its diagonal.
    pub fn car_radius(&self) -> f64 {
        self.car.as_ref().map_or(2.4, |c| c.length.hypot(c.width) / 2.0)
    }

    pub fn speed_in_units(&self, mps: f64) -> f64 {
        mps * if self.settings.is_mph() { 2.237 } else { 3.6 }
    }

    pub fn unit(&self) -> &'static str {
        if self.settings.is_mph() { "mph" } else { "km/h" }
    }

    /// Meters as feet or meters, depending on the Units setting.
    pub fn show_length(&self, m: f64) -> String {
        if self.settings.is_mph() {
            format!("{} ft", crate::text::thousands((m * 3.281).round()))
        } else if m < 10.0 {
            format!("{m:.1} m")
        } else {
            format!("{} m", crate::text::thousands(m.round()))
        }
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

    /// Builds the player's car again from `drive_config` (with the weapons it has).
    pub fn rebuild_car(&mut self) -> bool {
        if let Some(old) = self.game.car.take() {
            old.despawn(&mut self.commands);
        }
        let config = self.game.drive_config.clone();
        let night = self.world.is_night();
        let rig = build_car(&mut self.car_ctx(), &config, night, true);
        self.game.car = Some(rig);
        self.fit_to_car()
    }

    /// Sets up the physics, cameras, sound and shadows for the car as built. True if its size changed a lot.
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

    /// Shadows and view distance for the car and the settings.
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

    /// Puts the car at a spot, standing still, with the camera behind it.
    pub fn place_car(&mut self, x: f64, z: f64, heading: f64) {
        let dynamics = self.game.dynamics.clone();
        self.game.physics.reset(x, z, heading, false, &dynamics);
        self.game.physics.y = self.world.height_at(x, z);
        let p = self.game.car_position();
        self.rig.snap(p, heading as f32);
        self.game.drift_score = 0.0;
    }

    pub fn save_settings(&mut self) {
        let s = self.game.settings.clone();
        settings::save_settings(&mut self.storage, &s);
    }

    pub fn save_config(&mut self) {
        let c = self.game.config.clone();
        config::save_config(&mut self.storage, &c);
    }

    pub fn set_mode(&mut self, next: Mode) {
        let previous = self.game.mode;
        self.game.mode = next;
        if next == Mode::Garage || next == Mode::Menu {
            let p = &mut self.game.physics;
            p.vx = 0.0;
            p.vz = 0.0;
            p.vy = 0.0;
            p.y = self.world.height_at(p.x, p.z);
            p.airborne = false;
        }
        let (pos, heading) = (self.game.car_position(), self.game.physics.heading as f32);
        match next {
            Mode::Garage | Mode::Menu => self.rig.set_garage(true, pos, heading, false),
            Mode::Photo => self.rig.set_garage(true, pos, heading, true),
            Mode::Drive if previous != Mode::Drive => self.rig.set_garage(false, pos, heading, false),
            Mode::Drive => {}
        }
    }
}

pub fn setup_game(mut commands: Commands, storage: Res<Storage>) {
    let settings = settings::load_settings(&storage);
    let config = config::load_config(&storage);
    let best_drift = storage.get("carGame.bestDrift").and_then(|t| t.parse().ok()).unwrap_or(0.0);
    let mut physics = CarPhysics::new();
    physics.manual = settings.transmission == "manual";
    // For checking the game unattended (see dev.rs): start on a map, driving.
    let map = std::env::var("CAR_GAME_MAP").unwrap_or_else(|_| settings.map.clone());
    let mode = if std::env::var("CAR_GAME_MODE").as_deref() == Ok("drive") {
        Mode::Drive
    } else {
        Mode::Menu
    };
    commands.insert_resource(Game {
        mode,
        settings,
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
        load_map: Some((map, 0)),
        wheel_spin: 0.0,
        pointer_on_ui: false,
        typing: false,
        screen_shot: false,
    });
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
        Msaa::Sample4,
        Transform::from_xyz(0.0, 3.0, -8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Loads a new map when asked, and builds the player's car when needed.
pub fn load_and_build(mut ctx: Ctx) {
    if let Some((map, layout)) = ctx.game.load_map.take() {
        let mut world = std::mem::replace(&mut *ctx.world, World::empty());
        world.night = ctx.game.night;
        world.load(&map, layout, &mut ctx.build());
        *ctx.world = world;
        let s = ctx.world.spawn;
        ctx.place_car(s.x, s.z, s.heading);
        ctx.game.damage = 0.0;
        let label = crate::venues::venue(&map).label;
        if ctx.game.mode != Mode::Menu {
            ctx.game.toast(label, 1600.0);
        }
        let (pos, heading, mode) = (ctx.game.car_position(), ctx.game.physics.heading as f32, ctx.game.mode);
        ctx.rig.set_garage(mode == Mode::Garage || mode == Mode::Menu, pos, heading, false);
    }
    if ctx.game.rebuild {
        ctx.game.rebuild = false;
        ctx.rebuild_car();
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
    // Frames per second, twice a second.
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
    // Orbit camera: drag to look around, scroll to zoom.
    if on_game && (ctx.game.mode == Mode::Photo || ctx.game.mode == Mode::Garage) {
        let drag = if input.mouse.pressed(MouseButton::Left) || input.mouse.pressed(MouseButton::Right) {
            input.motion.delta
        } else {
            Vec2::ZERO
        };
        ctx.rig.drag(drag, input.scroll.delta.y, input.mouse.pressed(MouseButton::Right));
    }

    let driving = ctx.game.mode == Mode::Drive && !ctx.game.paused;
    let dt = if !driving {
        0.0
    } else if ctx.game.slow_motion {
        real_dt * SLOW_MOTION
    } else {
        real_dt
    };
    ctx.game.clock += dt;
    if driving {
        drive(&mut ctx, dt, &controls);
    }

    // The music builds up with speed.
    let intensity = if !driving {
        0.0
    } else {
        0.3 + 0.5 * ctx.game.physics.speed_fraction()
    };
    ctx.sound.music(ctx.game.settings.music, intensity);

    sync_car(&mut ctx, dt);
    let (x, z, radius) = (ctx.game.physics.x, ctx.game.physics.z, ctx.game.car_radius());
    ctx.world.effects.amount = settings::effects_amount(&ctx.game.settings.effects);
    ctx.world.update(dt, x, z, radius);
    ctx.focus.x = x;
    ctx.focus.z = z;
    let (pos, heading, speed, boosting) = (
        ctx.game.car_position(),
        ctx.game.physics.heading as f32,
        ctx.game.physics.speed() as f32,
        ctx.game.physics.boosting,
    );
    ctx.rig.base_fov = ctx.game.settings.fov as f32;
    ctx.rig.update(real_dt as f32, pos, heading, speed, driving && boosting);

    // Sound.
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

    ctx.game.gear_refused = (ctx.game.gear_refused - real_dt).max(0.0);
    if ctx.game.toast.left > 0.0 {
        ctx.game.toast.left -= real_dt;
    }
    for p in &mut ctx.game.popups {
        p.left -= real_dt;
    }
    ctx.game.popups.retain(|p| p.left > 0.0);
    if ctx.storage.dirty {
        ctx.storage.flush();
    }
}

/// Puts the camera where the rig says, with any shake.
pub fn place_camera(
    mut ctx_rig: ResMut<CameraRig>,
    mut game: ResMut<Game>,
    time: Res<Time<Real>>,
    mut camera: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut t, mut projection)) = camera.single_mut() else { return };
    let mut position = ctx_rig.position;
    let driving = game.mode == Mode::Drive && !game.paused;
    if game.shake > 0.001 && driving {
        position.x += crate::rng::centered() as f32 * game.shake as f32;
        position.y += crate::rng::centered() as f32 * game.shake as f32;
        game.shake *= (-time.delta_secs_f64().min(MAX_FRAME) * 8.0).exp();
    }
    *t = Transform::from_translation(position).looking_at(ctx_rig.look_at, Vec3::Y);
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = ctx_rig.fov.to_radians();
        let s = ctx_rig.size;
        p.near = 0.1 * (s / 5.0).max(1.0);
        p.far = ctx_rig_far(&ctx_rig, &game);
    }
    ctx_rig.fov = ctx_rig.fov.max(1.0);
}

fn ctx_rig_far(rig: &CameraRig, game: &Game) -> f32 {
    let reach = 40.0 * rig.size as f64 + game.car.as_ref().map_or(4.5, |c| c.length.max(c.height));
    (game.settings.view_distance.max(reach * 1.5) + 200.0) as f32
}

pub fn handle_action(ctx: &mut Ctx, action: crate::input::Action, windows: &mut Query<&mut Window, With<PrimaryWindow>>) {
    use crate::input::Action as A;
    let mode = ctx.game.mode;
    if mode == Mode::Menu && action != A::Mute && action != A::Fullscreen {
        return;
    }
    match action {
        A::Night => {
            ctx.game.night = !ctx.game.night;
            let n = ctx.game.night;
            ctx.world.set_night(n);
            ctx.game.toast(if n { "Night" } else { "Day" }, 1600.0);
        }
        A::Mute => {
            let muted = ctx.sound.toggle_mute();
            ctx.storage.set("carGame.muted", if muted { "1" } else { "0" });
            ctx.game.toast(if muted { "Sound off" } else { "Sound on" }, 1600.0);
        }
        A::Fullscreen => {
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
        if action == A::Garage || action == A::Settings {
            ctx.set_mode(Mode::Drive);
        }
        return;
    }
    if mode == Mode::Photo {
        if action == A::Snapshot {
            ctx.game.screen_shot = true;
        }
        if action == A::Photo || action == A::Settings {
            ctx.set_mode(Mode::Drive);
        }
        return;
    }
    match action {
        A::Garage | A::Settings => ctx.set_mode(Mode::Garage),
        A::Camera => {
            let m = ctx.rig.next_mode();
            ctx.game.toast(m.label(), 1600.0);
        }
        A::Reset => {
            let s = ctx.world.spawn;
            ctx.place_car(s.x, s.z, s.heading);
        }
        A::Help => ctx.game.help = !ctx.game.help,
        A::Photo => {
            ctx.set_mode(Mode::Photo);
            ctx.game.toast("Photo mode: drag to look around, Enter to save", 1600.0);
        }
        A::Horn => {
            let horn = ctx.game.config.horn.clone();
            ctx.sound.horn(&horn);
        }
        A::ShiftUp | A::ShiftDown => {
            if !ctx.game.physics.manual {
                ctx.game.toast("Automatic gearbox: pick Manual in Settings (Esc)", 2200.0);
            } else {
                let ok = if action == A::ShiftUp {
                    ctx.game.physics.shift_up()
                } else {
                    ctx.game.physics.shift_down()
                };
                if !ok {
                    ctx.game.gear_refused = 0.25;
                }
            }
        }
        A::SlowMo => ctx.game.slow_motion = !ctx.game.slow_motion,
        _ => {}
    }
}

/// Runs the car's physics for `dt` seconds in fixed steps, and reacts to crashes and landings.
pub fn drive(ctx: &mut Ctx, dt: f64, controls: &crate::input::Controls) {
    ctx.game.accumulator += dt;
    let mut steps = 0;
    let dynamics = ctx.game.dynamics.clone();
    while ctx.game.accumulator >= STEP && steps < MAX_STEPS {
        let was_airborne = ctx.game.physics.airborne;
        let ev = ctx.game.physics.step(STEP, &controls.car, &mut ctx.world, &dynamics);
        ctx.game.accumulator -= STEP;
        steps += 1;
        let g = &mut *ctx.game;
        if g.physics.airborne {
            g.air_time += STEP;
        } else if was_airborne {
            if g.air_time > 0.7 {
                let t = g.air_time;
                g.toast(format!("Air time {t:.1} s"), 1600.0);
                g.physics.nitro = (g.physics.nitro + t * 0.08).min(1.0);
            }
            g.air_time = 0.0;
        }
        if ev.impact > 3.0 {
            ctx.sound.crash(ev.impact);
            g.shake = (ev.impact * 0.03).min(0.6);
            if g.drift_score > 0.0 {
                g.drift_score = 0.0;
                g.toast("Crashed! Drift lost", 1600.0);
            }
            if let Some(t) = ev.hit_target
                && ev.impact > 6.0
            {
                let dir = bevy::math::DVec3::new(g.physics.vx, 0.0, g.physics.vz).normalize_or_zero();
                ctx.world.damage(t, ev.impact * 5.0, dir);
            }
        }
        if ev.smashed > 0 {
            g.shake = g.shake.max(0.15);
        }
        if g.physics.on_grass && g.physics.speed() > 10.0 {
            g.shake = g.shake.max(0.04);
        }
        if ev.cones > 0 {
            ctx.sound.crash(5.0);
        }
        if ev.landed > 6.0 {
            ctx.sound.crash(ev.landed * 0.6);
        }
    }
    if ctx.game.accumulator > STEP {
        ctx.game.accumulator = 0.0;
    }
    // Drift scoring: points for sliding sideways at speed.
    let g = &mut *ctx.game;
    let speed = g.physics.speed();
    let slip_angle = g.physics.lateral_speed.abs().atan2(g.physics.forward_speed.abs());
    let drifting = !g.physics.airborne && speed > 8.0 && slip_angle > 0.2 && g.physics.forward_speed > 0.0;
    if drifting {
        g.drift_score += speed * slip_angle * dt * 10.0;
        g.drift_timer = 0.8;
        g.physics.nitro = (g.physics.nitro + dt * 0.06).min(1.0);
    } else if g.drift_timer > 0.0 {
        g.drift_timer -= dt;
        if g.drift_timer <= 0.0 {
            let points = g.drift_score.round();
            g.drift_score = 0.0;
            if points >= 50.0 {
                if points > g.best_drift {
                    g.best_drift = points;
                    ctx.storage.set("carGame.bestDrift", points.to_string());
                    g.toast(format!("New best drift: {}!", crate::text::thousands(points)), 1600.0);
                } else {
                    g.toast(format!("Drift +{}", crate::text::thousands(points)), 1600.0);
                }
            }
        }
    }
    if !g.physics.boosting {
        g.physics.nitro = (g.physics.nitro + dt * 0.03).min(1.0);
    }
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
    let (forward, steer) = (p.forward_speed, p.steer_angle);
    let night = ctx.world.is_night();
    let braking = p.braking && ctx.game.mode == Mode::Drive;
    let boosting = p.boosting && ctx.game.mode == Mode::Drive;
    let wheels: Vec<(Entity, Entity, bool, f64)> = car.wheels.iter().map(|w| (w.steer, w.spin, w.front, w.radius)).collect();
    ctx.commands.entity(car.root).queue(move |mut e: EntityWorldMut| {
        if let Some(mut t) = e.get_mut::<Transform>() {
            *t = root;
        }
    });
    ctx.commands.entity(car.body).queue(move |mut e: EntityWorldMut| {
        if let Some(mut t) = e.get_mut::<Transform>() {
            *t = body;
        }
    });
    for (steer_e, spin_e, front, radius) in wheels {
        let turn = (forward / radius * dt) as f32;
        ctx.commands.entity(spin_e).queue(move |mut e: EntityWorldMut| {
            if let Some(mut t) = e.get_mut::<Transform>() {
                t.rotate_local_x(turn);
            }
        });
        if front {
            ctx.commands.entity(steer_e).queue(move |mut e: EntityWorldMut| {
                if let Some(mut t) = e.get_mut::<Transform>() {
                    t.rotation = Quat::from_rotation_y(steer as f32);
                }
            });
        }
    }
    let clock = ctx.game.real_clock;
    if let Some(rig) = ctx.game.car.as_mut() {
        if rig.lights != (night, braking) {
            rig.set_lights(night, braking, &mut ctx.materials, &mut ctx.commands);
        }
        rig.set_nitro(boosting, &mut ctx.commands);
        rig.animate(clock, &mut ctx.materials, &mut ctx.commands);
    }
}

/// The revs bar and gear text, for the HUD.
pub fn gear_text(p: &CarPhysics) -> String {
    if p.forward_speed < -0.5 { "R".into() } else { p.gear.to_string() }
}

pub fn shift_light(p: &CarPhysics) -> bool {
    p.manual && p.revs.clamp(0.0, 1.0) > 0.92 && p.gear < GEAR_TOPS.len()
}
