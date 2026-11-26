// Controller rumble: a gamepad that can vibrate buzzes softly as the tires
// reach their limit and slide, and thumps over curbs, rough ground, heavy
// landings and crashes. (The weak motor is the high buzz, the strong one the
// heavy thump.)
import type { CarState } from './model';

/** Effects are short and sent about ten times a second, so each one runs into the next. */
const EVERY = 0.1;
const LENGTH = 130;

/** Minimal shape of the browser's gamepad haptics (not in every TypeScript DOM library). */
interface Actuator {
  playEffect?(type: 'dual-rumble', params: { duration: number; strongMagnitude: number; weakMagnitude: number }): Promise<unknown>;
  reset?(): Promise<unknown>;
}

export class Rumble {
  private wait = 0;
  private buzzing = false;

  /** Called each frame while driving. */
  update(s: CarState, dt: number): void {
    this.wait -= dt;
    if (this.wait > 0) return;
    this.wait = EVERY;
    // Tires past about 90% of their grip buzz, more as they slide.
    const slip = Math.max(...s.usage);
    const weak = clamp((slip - 0.9) * 1.6);
    // Bumps (how fast the springs are working), plus any jolt from a crash or landing.
    const strong = clamp(s.susp.shake * 0.35 - 0.05 + s.jolt);
    if (weak < 0.03 && strong < 0.03) {
      if (this.buzzing) this.stop();
      return;
    }
    this.buzzing = true;
    for (const actuator of actuators()) {
      actuator.playEffect?.('dual-rumble', { duration: LENGTH, strongMagnitude: strong, weakMagnitude: weak })?.catch(() => {});
    }
  }

  /** Stops any rumble (paused, left the drive, or switched off). */
  stop(): void {
    if (!this.buzzing) return;
    this.buzzing = false;
    this.wait = 0;
    for (const actuator of actuators()) actuator.reset?.()?.catch(() => {});
  }
}

function actuators(): Actuator[] {
  if (typeof navigator === 'undefined' || !navigator.getGamepads) return [];
  const found: Actuator[] = [];
  for (const pad of navigator.getGamepads()) {
    const actuator = (pad as (Gamepad & { vibrationActuator?: Actuator }) | null)?.vibrationActuator;
    if (actuator) found.push(actuator);
  }
  return found;
}

function clamp(x: number): number {
  return Math.max(0, Math.min(1, x));
}
