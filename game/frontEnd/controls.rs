//! The keyboard, mouse and controllers, and the camera that follows the car.

pub mod input {
    //! Keyboard, mouse and controller input. Every key can be changed in
    //! the garage's Controls tab (see `Bindings`); the controller layout is fixed.
    use std::collections::{BTreeMap, HashMap};

    use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
    use bevy::prelude::*;

    use crate::physics::CarInput;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Action {
        Camera,
        Garage,
        Night,
        Mute,
        Reset,
        Help,
        Fullscreen,
        Settings,
        Photo,
        Snapshot,
        SlowMo,
        Trial,
        NextWeapon,
        Weapon1,
        Weapon2,
        Weapon3,
        Horn,
        ShiftUp,
        ShiftDown,
        EnterExit,
        Map,
    }

    /// Everything that can be put on a key: id (saved), what it does, its
    /// group on the Controls tab, the keys it starts on, and the action it
    /// sends when pressed (the driving controls are held instead).
    pub struct Binding {
        pub id: &'static str,
        pub label: &'static str,
        pub group: &'static str,
        pub keys: &'static [KeyCode],
        pub action: Option<Action>,
    }

    const fn bind(id: &'static str, label: &'static str, group: &'static str, keys: &'static [KeyCode], action: Option<Action>) -> Binding {
        Binding {
            id,
            label,
            group,
            keys,
            action,
        }
    }

    use KeyCode as K;
    pub const BINDINGS: &[Binding] = &[
        bind("accelerate", "Accelerate (on foot: walk)", "Driving", &[K::KeyW, K::ArrowUp], None),
        bind(
            "brake",
            "Brake and reverse (on foot: back up)",
            "Driving",
            &[K::KeyS, K::ArrowDown],
            None,
        ),
        bind("left", "Steer left (on foot: turn)", "Driving", &[K::KeyA, K::ArrowLeft], None),
        bind("right", "Steer right (on foot: turn)", "Driving", &[K::KeyD, K::ArrowRight], None),
        bind("handbrake", "Handbrake, drift (on foot: jump)", "Driving", &[K::Space], None),
        bind("nitro", "Nitro (on foot: run)", "Driving", &[K::ShiftLeft, K::ShiftRight], None),
        bind("jump", "Jump (if the car can)", "Driving", &[K::KeyJ], None),
        bind("shiftUp", "Gear up (manual)", "Driving", &[K::KeyX], Some(Action::ShiftUp)),
        bind("shiftDown", "Gear down (manual)", "Driving", &[K::KeyZ], Some(Action::ShiftDown)),
        bind("horn", "Horn", "Driving", &[K::KeyV], Some(Action::Horn)),
        bind(
            "enterExit",
            "Get in or out of a car",
            "On foot and fighting",
            &[K::KeyF],
            Some(Action::EnterExit),
        ),
        bind(
            "fire",
            "Fire, punch or shoot (and left click)",
            "On foot and fighting",
            &[K::KeyE],
            None,
        ),
        bind(
            "nextWeapon",
            "Next weapon",
            "On foot and fighting",
            &[K::KeyQ],
            Some(Action::NextWeapon),
        ),
        bind(
            "weapon1",
            "Weapon 1 (on foot: fists)",
            "On foot and fighting",
            &[K::Digit1],
            Some(Action::Weapon1),
        ),
        bind(
            "weapon2",
            "Weapon 2 (on foot: pistol)",
            "On foot and fighting",
            &[K::Digit2],
            Some(Action::Weapon2),
        ),
        bind("weapon3", "Weapon 3", "On foot and fighting", &[K::Digit3], Some(Action::Weapon3)),
        bind("map", "Map", "View", &[K::KeyM], Some(Action::Map)),
        bind("camera", "Change camera", "View", &[K::KeyC], Some(Action::Camera)),
        bind("photo", "Photo mode", "View", &[K::KeyP], Some(Action::Photo)),
        bind("snapshot", "Save a photo (photo mode)", "View", &[K::Enter], Some(Action::Snapshot)),
        bind("night", "Day or night", "View", &[K::KeyN], Some(Action::Night)),
        bind("fullscreen", "Fullscreen", "View", &[K::F11], Some(Action::Fullscreen)),
        bind("garage", "Garage", "Game", &[K::KeyG], Some(Action::Garage)),
        bind("reset", "Reset the car", "Game", &[K::KeyR], Some(Action::Reset)),
        bind("trial", "Time trial", "Game", &[K::KeyT], Some(Action::Trial)),
        bind("slowmo", "Slow motion", "Game", &[K::KeyB], Some(Action::SlowMo)),
        bind("mute", "Sound on or off", "Game", &[K::KeyU], Some(Action::Mute)),
        bind("help", "Help", "Game", &[K::KeyH], Some(Action::Help)),
        bind("talk", "Talk to someone (Living City)", "Living City", &[K::KeyK], None),
        bind("journal", "City journal (Living City)", "Living City", &[K::KeyL], None),
    ];

    /// The keys that can be used, with how they're saved and shown.
    const KEYS: &[(KeyCode, &str, &str)] = &[
        (K::KeyA, "KeyA", "A"),
        (K::KeyB, "KeyB", "B"),
        (K::KeyC, "KeyC", "C"),
        (K::KeyD, "KeyD", "D"),
        (K::KeyE, "KeyE", "E"),
        (K::KeyF, "KeyF", "F"),
        (K::KeyG, "KeyG", "G"),
        (K::KeyH, "KeyH", "H"),
        (K::KeyI, "KeyI", "I"),
        (K::KeyJ, "KeyJ", "J"),
        (K::KeyK, "KeyK", "K"),
        (K::KeyL, "KeyL", "L"),
        (K::KeyM, "KeyM", "M"),
        (K::KeyN, "KeyN", "N"),
        (K::KeyO, "KeyO", "O"),
        (K::KeyP, "KeyP", "P"),
        (K::KeyQ, "KeyQ", "Q"),
        (K::KeyR, "KeyR", "R"),
        (K::KeyS, "KeyS", "S"),
        (K::KeyT, "KeyT", "T"),
        (K::KeyU, "KeyU", "U"),
        (K::KeyV, "KeyV", "V"),
        (K::KeyW, "KeyW", "W"),
        (K::KeyX, "KeyX", "X"),
        (K::KeyY, "KeyY", "Y"),
        (K::KeyZ, "KeyZ", "Z"),
        (K::Digit0, "Digit0", "0"),
        (K::Digit1, "Digit1", "1"),
        (K::Digit2, "Digit2", "2"),
        (K::Digit3, "Digit3", "3"),
        (K::Digit4, "Digit4", "4"),
        (K::Digit5, "Digit5", "5"),
        (K::Digit6, "Digit6", "6"),
        (K::Digit7, "Digit7", "7"),
        (K::Digit8, "Digit8", "8"),
        (K::Digit9, "Digit9", "9"),
        (K::F1, "F1", "F1"),
        (K::F2, "F2", "F2"),
        (K::F3, "F3", "F3"),
        (K::F4, "F4", "F4"),
        (K::F5, "F5", "F5"),
        (K::F6, "F6", "F6"),
        (K::F7, "F7", "F7"),
        (K::F8, "F8", "F8"),
        (K::F9, "F9", "F9"),
        (K::F10, "F10", "F10"),
        (K::F11, "F11", "F11"),
        (K::F12, "F12", "F12"),
        (K::ArrowUp, "ArrowUp", "↑"),
        (K::ArrowDown, "ArrowDown", "↓"),
        (K::ArrowLeft, "ArrowLeft", "←"),
        (K::ArrowRight, "ArrowRight", "→"),
        (K::Space, "Space", "Space"),
        (K::Enter, "Enter", "Enter"),
        (K::Tab, "Tab", "Tab"),
        (K::Backspace, "Backspace", "Backspace"),
        (K::ShiftLeft, "ShiftLeft", "Left Shift"),
        (K::ShiftRight, "ShiftRight", "Right Shift"),
        (K::ControlLeft, "ControlLeft", "Left Ctrl"),
        (K::ControlRight, "ControlRight", "Right Ctrl"),
        (K::AltLeft, "AltLeft", "Left Alt"),
        (K::AltRight, "AltRight", "Right Alt"),
        (K::Comma, "Comma", ","),
        (K::Period, "Period", "."),
        (K::Slash, "Slash", "/"),
        (K::Semicolon, "Semicolon", ";"),
        (K::Quote, "Quote", "'"),
        (K::BracketLeft, "BracketLeft", "["),
        (K::BracketRight, "BracketRight", "]"),
        (K::Backslash, "Backslash", "\\"),
        (K::Minus, "Minus", "-"),
        (K::Equal, "Equal", "="),
        (K::Backquote, "Backquote", "`"),
        (K::Home, "Home", "Home"),
        (K::End, "End", "End"),
        (K::PageUp, "PageUp", "Page Up"),
        (K::PageDown, "PageDown", "Page Down"),
        (K::Insert, "Insert", "Insert"),
        (K::Delete, "Delete", "Delete"),
        (K::Numpad0, "Numpad0", "Num 0"),
        (K::Numpad1, "Numpad1", "Num 1"),
        (K::Numpad2, "Numpad2", "Num 2"),
        (K::Numpad3, "Numpad3", "Num 3"),
        (K::Numpad4, "Numpad4", "Num 4"),
        (K::Numpad5, "Numpad5", "Num 5"),
        (K::Numpad6, "Numpad6", "Num 6"),
        (K::Numpad7, "Numpad7", "Num 7"),
        (K::Numpad8, "Numpad8", "Num 8"),
        (K::Numpad9, "Numpad9", "Num 9"),
    ];

    pub fn key_id(key: KeyCode) -> Option<&'static str> {
        KEYS.iter().find(|k| k.0 == key).map(|k| k.1)
    }

    pub fn key_from(id: &str) -> Option<KeyCode> {
        KEYS.iter().find(|k| k.1 == id).map(|k| k.0)
    }

    pub fn key_label(key: KeyCode) -> &'static str {
        KEYS.iter().find(|k| k.0 == key).map_or("?", |k| k.2)
    }

    /// Which keys do what now: the defaults, with the player's changes
    /// (`Settings::controls`, binding id -> key ids) on top.
    #[derive(Clone, Debug)]
    pub struct Bindings(HashMap<&'static str, Vec<KeyCode>>);

    impl Default for Bindings {
        fn default() -> Self {
            Bindings(BINDINGS.iter().map(|b| (b.id, b.keys.to_vec())).collect())
        }
    }

    impl Bindings {
        pub fn new(changes: &BTreeMap<String, Vec<String>>) -> Self {
            let mut b = Bindings::default();
            for (id, keys) in changes {
                if let Some(binding) = BINDINGS.iter().find(|x| x.id == id) {
                    b.0.insert(binding.id, keys.iter().filter_map(|k| key_from(k)).take(2).collect());
                }
            }
            b
        }

        pub fn keys(&self, id: &str) -> &[KeyCode] {
            self.0.get(id).map_or(&[], |v| v.as_slice())
        }

        pub fn pressed(&self, keys: &ButtonInput<KeyCode>, id: &str) -> bool {
            self.keys(id).iter().any(|k| keys.pressed(*k))
        }

        pub fn just_pressed(&self, keys: &ButtonInput<KeyCode>, id: &str) -> bool {
            self.keys(id).iter().any(|k| keys.just_pressed(*k))
        }

        /// The keys for `id`, as shown (like "W / ↑").
        pub fn label(&self, id: &str) -> String {
            let keys = self.keys(id);
            if keys.is_empty() {
                return "(none)".into();
            }
            keys.iter().map(|k| key_label(*k)).collect::<Vec<_>>().join(" / ")
        }

        /// Puts `key` on binding `id` (in place of its key number `slot`),
        /// taking it off anything else that had it. Returns what lost it.
        pub fn set(&mut self, id: &str, slot: usize, key: KeyCode) -> Vec<&'static str> {
            let Some(binding) = BINDINGS.iter().find(|b| b.id == id) else {
                return Vec::new();
            };
            let keys = self.0.entry(binding.id).or_default();
            if let Some(old_slot) = keys.iter().position(|&k| k == key) {
                if slot < keys.len() {
                    keys.swap(old_slot, slot);
                }
                return Vec::new();
            }
            let mut moved = Vec::new();
            for (other, keys) in &mut self.0 {
                if *other != id && keys.contains(&key) {
                    keys.retain(|k| *k != key);
                    moved.push(*other);
                }
            }
            let keys = self.0.entry(binding.id).or_default();
            if let Some(current) = keys.get_mut(slot) {
                *current = key;
            } else if keys.len() < 2 {
                keys.push(key);
            }
            moved
        }

        /// The changes from the defaults, for saving.
        pub fn changes(&self) -> BTreeMap<String, Vec<String>> {
            BINDINGS
                .iter()
                .filter(|b| self.keys(b.id) != b.keys)
                .map(|b| {
                    (
                        b.id.to_string(),
                        self.keys(b.id).iter().filter_map(|k| key_id(*k)).map(str::to_string).collect(),
                    )
                })
                .collect()
        }
    }

    /// The driving controls this frame.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Controls {
        pub car: CarInput,
        pub fire: bool,
    }

    /// Reads keys and controllers. `typing` is true while a text box has the keyboard.
    pub fn actions(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, typing: bool, bindings: &Bindings) -> Vec<Action> {
        let mut out = Vec::new();
        if keys.just_pressed(KeyCode::Escape) {
            out.push(Action::Settings);
        }
        if !typing {
            for b in BINDINGS {
                if let Some(action) = b.action
                    && bindings.just_pressed(keys, b.id)
                {
                    out.push(action);
                }
            }
        }
        // Standard layout: Y = camera, Start = garage, Back = reset, X = night,
        // LB = next weapon, D-pad down = slow motion, right stick click = horn,
        // D-pad right / left = gear up / down, D-pad up = get in or out of a car.
        if let Some(pad) = pads.iter().next() {
            let map = [
                (GamepadButton::North, Action::Camera),
                (GamepadButton::Start, Action::Garage),
                (GamepadButton::Select, Action::Reset),
                (GamepadButton::West, Action::Night),
                (GamepadButton::LeftTrigger, Action::NextWeapon),
                (GamepadButton::DPadDown, Action::SlowMo),
                (GamepadButton::RightThumb, Action::Horn),
                (GamepadButton::DPadRight, Action::ShiftUp),
                (GamepadButton::DPadLeft, Action::ShiftDown),
                (GamepadButton::DPadUp, Action::EnterExit),
            ];
            for (button, action) in map {
                if pad.just_pressed(button) {
                    out.push(action);
                }
            }
        }
        out
    }

    /// The controller's layout, for the Controls tab.
    pub const PAD_HELP: &[&str] = &[
        "Right trigger accelerate · left trigger brake · left stick steer",
        "A handbrake · B nitro · right bumper fire · left stick click jump",
        "Y camera · X day/night · left bumper next weapon · right stick click horn",
        "D-pad up get in or out · down slow motion · left/right change gear",
        "Start garage · Back reset",
    ];

    pub fn read(
        keys: &ButtonInput<KeyCode>,
        mouse: &ButtonInput<MouseButton>,
        pads: &Query<&Gamepad>,
        typing: bool,
        mouse_on_game: bool,
        bindings: &Bindings,
    ) -> Controls {
        if typing {
            return Controls::default();
        }
        let k = |id: &str| bindings.pressed(keys, id);
        let mut throttle: f64 = if k("accelerate") { 1.0 } else { 0.0 };
        let mut brake: f64 = if k("brake") { 1.0 } else { 0.0 };
        let mut steer: f64 = (if k("left") { 1.0 } else { 0.0 }) - (if k("right") { 1.0 } else { 0.0 });
        let mut handbrake = k("handbrake");
        let mut boost = k("nitro");
        let mut fire = k("fire") || (mouse_on_game && mouse.pressed(MouseButton::Left));
        let mut jump = k("jump");
        if let Some(pad) = pads.iter().next() {
            let stick = pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0) as f64;
            if stick.abs() > 0.12 {
                steer = -stick;
            }
            throttle = throttle.max(pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0) as f64);
            brake = brake.max(pad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0) as f64);
            handbrake |= pad.pressed(GamepadButton::South);
            boost |= pad.pressed(GamepadButton::East);
            fire |= pad.pressed(GamepadButton::RightTrigger);
            jump |= pad.pressed(GamepadButton::LeftThumb);
        }
        Controls {
            car: CarInput {
                throttle,
                brake,
                steer,
                handbrake,
                boost,
                jump,
            },
            fire,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_key_moves_from_one_control_to_another_and_is_saved() {
            let mut b = Bindings::default();
            assert_eq!(b.keys("map"), &[KeyCode::KeyM]);
            let moved = b.set("mute", 0, KeyCode::KeyM);
            assert_eq!(moved, vec!["map"]);
            assert!(b.keys("map").is_empty());
            let saved = b.changes();
            assert_eq!(saved.get("mute"), Some(&vec!["KeyM".to_string()]));
            let again = Bindings::new(&saved);
            assert_eq!(again.keys("mute"), &[KeyCode::KeyM]);
            assert!(again.keys("map").is_empty());
            assert_eq!(again.keys("accelerate"), &[KeyCode::KeyW, KeyCode::ArrowUp]);
            // Pressing the key a control already has, in its place, changes nothing.
            let mut same = Bindings::default();
            assert!(same.set("accelerate", 0, KeyCode::KeyW).is_empty());
            assert_eq!(same.keys("accelerate"), &[KeyCode::KeyW, KeyCode::ArrowUp]);
            assert!(same.set("accelerate", 0, KeyCode::ArrowUp).is_empty());
            assert_eq!(same.keys("accelerate"), &[KeyCode::ArrowUp, KeyCode::KeyW]);
            // Every default key has a name to be saved under.
            for binding in BINDINGS {
                for key in binding.keys {
                    assert!(key_id(*key).is_some(), "{}", binding.id);
                }
            }
        }
    }
}

pub mod camera {
    //! Chase, far-chase and hood cameras for driving; an orbit camera in the
    //! garage and in photo mode (drag to look around, scroll to zoom).
    use bevy::prelude::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CameraMode {
        Chase,
        Far,
        Hood,
    }

    impl CameraMode {
        pub fn label(self) -> &'static str {
            match self {
                CameraMode::Chase => "Chase camera",
                CameraMode::Far => "Far camera",
                CameraMode::Hood => "Hood camera",
            }
        }
    }

    /// An orbit around a point: the camera's direction and distance from it.
    #[derive(Clone, Copy, Debug)]
    pub struct Orbit {
        pub target: Vec3,
        pub yaw: f32,
        pub pitch: f32,
        pub distance: f32,
        pub min_distance: f32,
        pub max_distance: f32,
        pub auto_rotate: bool,
        pub pan: bool,
        /// Smoothed changes still to apply (like OrbitControls' damping).
        spin: Vec2,
        zoom: f32,
    }

    #[derive(Resource)]
    pub struct CameraRig {
        pub mode: CameraMode,
        pub eye_height: f32,
        /// How big the car is compared to a normal one.
        pub size: f32,
        pub base_fov: f32,
        pub fov: f32,
        pub garage: bool,
        pub orbit: Orbit,
        pub position: Vec3,
        pub look_at: Vec3,
        desired: Vec3,
    }

    impl Default for CameraRig {
        fn default() -> Self {
            CameraRig {
                mode: CameraMode::Chase,
                eye_height: 1.1,
                size: 1.0,
                base_fov: 62.0,
                fov: 62.0,
                garage: false,
                orbit: Orbit {
                    target: Vec3::ZERO,
                    yaw: 0.0,
                    pitch: 0.3,
                    distance: 7.0,
                    min_distance: 4.0,
                    max_distance: 14.0,
                    auto_rotate: true,
                    pan: false,
                    spin: Vec2::ZERO,
                    zoom: 0.0,
                },
                position: Vec3::new(0.0, 3.0, -7.0),
                look_at: Vec3::ZERO,
                desired: Vec3::ZERO,
            }
        }
    }

    impl CameraRig {
        pub fn next_mode(&mut self) -> CameraMode {
            self.mode = match self.mode {
                CameraMode::Chase => CameraMode::Far,
                CameraMode::Far => CameraMode::Hood,
                CameraMode::Hood => CameraMode::Chase,
            };
            self.mode
        }

        /// Switches the orbit camera on (garage, photo mode) or back to following the car.
        pub fn set_garage(&mut self, on: bool, car: Vec3, heading: f32, photo: bool) {
            self.garage = on;
            let s = self.size;
            let o = &mut self.orbit;
            o.auto_rotate = !photo;
            o.pan = photo;
            o.min_distance = if photo { 2.0 } else { 4.0 } * s;
            o.max_distance = if photo { 40.0 } else { 14.0 } * s;
            if on {
                o.target = Vec3::new(car.x, car.y + 0.7 * s, car.z);
                if !photo {
                    let a = heading + std::f32::consts::PI * 0.8;
                    self.position = Vec3::new(car.x + a.sin() * 7.0 * s, car.y + 2.2 * s, car.z + a.cos() * 7.0 * s);
                }
                // Read the orbit back from where the camera is.
                let off = self.position - o.target;
                o.distance = off.length().clamp(o.min_distance, o.max_distance);
                o.yaw = off.x.atan2(off.z);
                o.pitch = (off.y / off.length().max(0.001)).clamp(-1.0, 1.0).asin();
                o.spin = Vec2::ZERO;
                o.zoom = 0.0;
            } else {
                self.snap(car, heading);
            }
        }

        /// Jumps straight to the follow position.
        pub fn snap(&mut self, car: Vec3, heading: f32) {
            self.target(car, heading);
            self.position = self.desired;
        }

        fn target(&mut self, car: Vec3, heading: f32) {
            let (fx, fz) = (heading.sin(), heading.cos());
            let s = self.size;
            let (dist, height, ahead) = match self.mode {
                CameraMode::Far => (11.0 * s, 4.2 * s, 4.0 * s),
                CameraMode::Hood => (-0.3 * s, self.eye_height, 12.0 * s),
                CameraMode::Chase => (6.5 * s, 2.4 * s, 3.0 * s),
            };
            self.desired = Vec3::new(car.x - fx * dist, car.y + height, car.z - fz * dist);
            let look_y = if self.mode == CameraMode::Hood {
                self.eye_height - 0.2 * s
            } else {
                1.0 * s
            };
            self.look_at = Vec3::new(car.x + fx * ahead, car.y + look_y, car.z + fz * ahead);
        }

        /// Mouse drag (pixels) and scroll for the orbit camera.
        pub fn drag(&mut self, delta: Vec2, scroll: f32, panning: bool) {
            if !self.garage {
                return;
            }
            if panning && self.orbit.pan {
                let right = Vec3::new(self.orbit.yaw.cos(), 0.0, -self.orbit.yaw.sin());
                let up = Vec3::Y;
                let k = self.orbit.distance * 0.0015;
                self.orbit.target += (-right * delta.x + up * delta.y) * k;
            } else {
                self.orbit.spin += Vec2::new(-delta.x, delta.y) * 0.005;
            }
            self.orbit.zoom += -scroll * 0.1;
            if delta != Vec2::ZERO {
                self.orbit.auto_rotate = false;
            }
        }

        pub fn update(&mut self, dt: f32, car: Vec3, heading: f32, speed: f32, boosting: bool) {
            if self.garage {
                let o = &mut self.orbit;
                if o.auto_rotate {
                    // OrbitControls' autoRotateSpeed 0.8: a turn every 75 seconds.
                    o.yaw += std::f32::consts::TAU / 75.0 * dt;
                }
                let k = 1.0 - (-dt * 12.0).exp();
                o.yaw += o.spin.x * k;
                o.pitch = (o.pitch + o.spin.y * k).clamp(0.05, std::f32::consts::FRAC_PI_2 - 0.05);
                o.spin *= 1.0 - k;
                o.distance = (o.distance * (1.0 + o.zoom * k)).clamp(o.min_distance, o.max_distance);
                o.zoom *= 1.0 - k;
                let dir = Vec3::new(o.pitch.cos() * o.yaw.sin(), o.pitch.sin(), o.pitch.cos() * o.yaw.cos());
                self.position = o.target + dir * o.distance;
                self.look_at = o.target;
                self.fov += (self.base_fov - self.fov) * (dt * 6.0).min(1.0);
                return;
            }
            self.target(car, heading);
            if self.mode == CameraMode::Hood {
                self.position = self.desired;
            } else {
                let stiffness = 6.0 * (speed / 30.0).max(1.0);
                self.position = self.position.lerp(self.desired, 1.0 - (-dt * stiffness).exp());
                self.position.y = self.position.y.max(car.y + 0.6 * self.size);
            }
            let fov = self.base_fov + (speed * 0.22).min(14.0) + if boosting { 8.0 } else { 0.0 };
            self.fov += (fov - self.fov) * (dt * 3.0).min(1.0);
        }
    }
}
