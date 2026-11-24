// The engine, turbo, gearbox and differentials.
//
// - Each kind of car has its own engine: a big V8 pulls hard from low revs,
//   a sports car's engine comes alive near the top, a turbo four has a fat
//   middle once the turbo has spooled up.
// - A turbo needs revs and a moment to build boost (turbo lag).
// - The clutch slips when pulling away, so the engine revs up first.
// - Gear changes take a moment, with the power cut while the clutch is out.
// - Lifting off slows the car through the engine (engine braking).
// - A limited-slip differential sends the power to the wheel with grip
//   instead of letting the inside wheel spin away; an open one doesn't.

/** How an engine's torque changes with revs, and whether it has a turbo. */
export interface Engine {
  /** Torque at idle, as a share of the most it makes. */
  low: number;
  /** Where the torque peaks, as a share of the redline. */
  peak: number;
  /** Torque left at the redline (at least 0.95, so every car still reaches its top speed). */
  redline: number;
  turbo: boolean;
}

/** The engine for each body style. */
export function engineFor(body: string | undefined): Engine {
  switch (body) {
    case 'muscle':
      return { low: 0.8, peak: 0.5, redline: 0.95, turbo: false }; // big V8: all the torque, right away
    case 'pickup':
    case 'van':
      return { low: 0.85, peak: 0.35, redline: 0.95, turbo: false }; // lazy and torquey
    case 'hatch':
      return { low: 0.6, peak: 0.45, redline: 0.95, turbo: true }; // turbo four
    case 'wedge':
      return { low: 0.6, peak: 0.8, redline: 0.96, turbo: true }; // twin-turbo supercar
    default:
      return { low: 0.55, peak: 0.85, redline: 0.97, turbo: true }; // sports car: wants revs
  }
}

/** Torque at these revs (0..1 of the redline), as a share of the most the engine makes. */
export function torqueAt(engine: Engine, revs: number): number {
  const r = Math.max(0, Math.min(1.05, revs));
  if (r <= engine.peak) return engine.low + (1 - engine.low) * Math.sin((Math.PI / 2) * (r / engine.peak));
  const past = (r - engine.peak) / Math.max(0.05, 1 - engine.peak);
  return 1 - (1 - engine.redline) * past * past;
}

/**
 * Turbo boost, 0..1, one step on: it needs revs (it starts to build around
 * a third of the redline) and a moment to spool up. It drops quickly when
 * you lift. `lag` false makes it instant.
 */
export function spoolTurbo(boost: number, throttle: number, revs: number, dt: number, lag: boolean): number {
  const ready = Math.max(0, Math.min(1, (revs - 0.28) / 0.3));
  const target = throttle * (lag ? ready : 1);
  if (!lag) return target;
  const rate = target > boost ? 1.6 : 6;
  return boost + (target - boost) * Math.min(1, dt * rate);
}

/** How much of the engine's torque a turbo engine makes at this boost (all of it at full boost). */
export function turboShare(engine: Engine, boost: number): number {
  return engine.turbo ? 0.68 + 0.32 * boost : 1;
}

/**
 * The automatic gearbox: changes up at the redline and down when the revs
 * fall well under the lower gear's redline (so it doesn't hunt between two
 * gears at one speed). `tops` are each gear's road speed at the redline.
 */
export function autoGear(gear: number, speed: number, tops: number[]): number {
  let g = Math.max(1, Math.min(tops.length, gear));
  while (g < tops.length && speed > tops[g - 1]) g++;
  while (g > 1 && speed < tops[g - 2] * 0.86) g--;
  return g;
}

/**
 * How the two driven wheels on an axle share the push. Open (`lock` 0):
 * equally, so a wheel with no grip spins and the other gets nothing extra.
 * Limited-slip (0..1): some of the push moves from the faster-spinning wheel
 * to the slower one. Locked (1): the wheels are held almost together.
 * Returns the [left, right] push.
 */
export function differential(push: number, lock: number, leftSpeed: number, rightSpeed: number, roadSpeed: number): [number, number] {
  const half = push / 2;
  if (lock <= 0) return [half, half];
  const most = lock * Math.abs(half) + lock * 0.6; // a little preload, so it works off the throttle too
  const difference = (leftSpeed - rightSpeed) / (0.4 + 0.04 * Math.abs(roadSpeed));
  const moved = most * Math.max(-1, Math.min(1, difference));
  return [half - moved, half + moved];
}
