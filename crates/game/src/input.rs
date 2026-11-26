//! Keyboard, mouse and controller input.
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
}

const ACTION_KEYS: &[(KeyCode, Action)] = &[
    (KeyCode::KeyC, Action::Camera),
    (KeyCode::KeyG, Action::Garage),
    (KeyCode::KeyN, Action::Night),
    (KeyCode::KeyM, Action::Mute),
    (KeyCode::KeyR, Action::Reset),
    (KeyCode::KeyH, Action::Help),
    (KeyCode::KeyF, Action::Fullscreen),
    (KeyCode::Escape, Action::Settings),
    (KeyCode::KeyP, Action::Photo),
    (KeyCode::Enter, Action::Snapshot),
    (KeyCode::KeyB, Action::SlowMo),
    (KeyCode::KeyT, Action::Trial),
    (KeyCode::KeyQ, Action::NextWeapon),
    (KeyCode::Digit1, Action::Weapon1),
    (KeyCode::Digit2, Action::Weapon2),
    (KeyCode::Digit3, Action::Weapon3),
    (KeyCode::KeyV, Action::Horn),
    (KeyCode::KeyX, Action::ShiftUp),
    (KeyCode::KeyZ, Action::ShiftDown),
];

/// The driving controls this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub car: CarInput,
    pub fire: bool,
}

/// Reads keys and controllers. `typing` is true while a text box has the keyboard.
pub fn actions(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, typing: bool) -> Vec<Action> {
    let mut out = Vec::new();
    for (key, action) in ACTION_KEYS {
        if keys.just_pressed(*key) && (!typing || *key == KeyCode::Escape) {
            out.push(*action);
        }
    }
    // Standard layout: Y = camera, Start = garage, Back = reset, X = night,
    // LB = next weapon, D-pad down = slow motion, right stick click = horn,
    // D-pad right / left = gear up / down.
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
        ];
        for (button, action) in map {
            if pad.just_pressed(button) {
                out.push(action);
            }
        }
    }
    out
}

pub fn read(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    pads: &Query<&Gamepad>,
    typing: bool,
    mouse_on_game: bool,
) -> Controls {
    if typing {
        return Controls::default();
    }
    let k = |codes: &[KeyCode]| codes.iter().any(|c| keys.pressed(*c));
    let mut throttle: f64 = if k(&[KeyCode::KeyW, KeyCode::ArrowUp]) { 1.0 } else { 0.0 };
    let mut brake: f64 = if k(&[KeyCode::KeyS, KeyCode::ArrowDown]) { 1.0 } else { 0.0 };
    let mut steer: f64 = (if k(&[KeyCode::KeyA, KeyCode::ArrowLeft]) { 1.0 } else { 0.0 })
        - (if k(&[KeyCode::KeyD, KeyCode::ArrowRight]) { 1.0 } else { 0.0 });
    let mut handbrake = k(&[KeyCode::Space]);
    let mut boost = k(&[KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let mut fire = k(&[KeyCode::KeyE]) || (mouse_on_game && mouse.pressed(MouseButton::Left));
    let mut jump = k(&[KeyCode::KeyJ]);
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
