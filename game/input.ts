// Keyboard and gamepad input.
import type { CarInput } from './carPhysics';

export type Action = 'camera' | 'garage' | 'night' | 'mute' | 'reset' | 'help' | 'fullscreen';

const ACTION_KEYS: Record<string, Action> = {
  KeyC: 'camera',
  KeyG: 'garage',
  KeyN: 'night',
  KeyM: 'mute',
  KeyR: 'reset',
  KeyH: 'help',
  KeyF: 'fullscreen',
};

const DRIVE_KEYS = new Set(['KeyW', 'KeyA', 'KeyS', 'KeyD', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Space']);

export class Input {
  private down = new Set<string>();
  private pending: Action[] = [];
  private padButtons: boolean[] = [];

  constructor() {
    window.addEventListener('keydown', (e) => {
      if (isTyping(e.target)) return;
      // Arrows move a focused slider and Space presses a focused button,
      // so only block their default action when no control has focus.
      if (DRIVE_KEYS.has(e.code) && !isControl(e.target)) e.preventDefault();
      this.down.add(e.code);
      const action = ACTION_KEYS[e.code];
      if (action && !e.repeat) this.pending.push(action);
    });
    window.addEventListener('keyup', (e) => this.down.delete(e.code));
    window.addEventListener('blur', () => this.down.clear());
  }

  /** Actions pressed since the last call (keyboard plus gamepad buttons). */
  takeActions(): Action[] {
    const pad = firstGamepad();
    if (pad) {
      // Standard layout: Y = camera, Start = garage, Back = reset, X = night.
      const map: [number, Action][] = [[3, 'camera'], [9, 'garage'], [8, 'reset'], [2, 'night']];
      for (const [i, action] of map) {
        const pressed = !!pad.buttons[i]?.pressed;
        if (pressed && !this.padButtons[i]) this.pending.push(action);
        this.padButtons[i] = pressed;
      }
    }
    const out = this.pending;
    this.pending = [];
    return out;
  }

  read(): CarInput {
    const k = (...codes: string[]) => codes.some((c) => this.down.has(c));
    let throttle = k('KeyW', 'ArrowUp') ? 1 : 0;
    let brake = k('KeyS', 'ArrowDown') ? 1 : 0;
    let steer = (k('KeyA', 'ArrowLeft') ? 1 : 0) - (k('KeyD', 'ArrowRight') ? 1 : 0);
    let handbrake = k('Space');

    const pad = firstGamepad();
    if (pad) {
      const stick = pad.axes[0] ?? 0;
      if (Math.abs(stick) > 0.12) steer = -stick;
      throttle = Math.max(throttle, pad.buttons[7]?.value ?? 0);
      brake = Math.max(brake, pad.buttons[6]?.value ?? 0);
      handbrake = handbrake || !!pad.buttons[0]?.pressed;
    }
    return { throttle, brake, steer, handbrake };
  }
}

function firstGamepad(): Gamepad | null {
  if (!navigator.getGamepads) return null;
  for (const pad of navigator.getGamepads()) if (pad && pad.connected) return pad;
  return null;
}

function isControl(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest('input, button, select, textarea, [contenteditable]') !== null;
}

function isTyping(target: EventTarget | null): boolean {
  return target instanceof HTMLInputElement && (target.type === 'text' || target.type === 'number');
}
