// The suspension: each wheel hangs on a spring and a damper, and an anti-roll
// bar links the two wheels on each axle. The body is a mass on those four
// springs that can rise and fall (heave), dip its nose (pitch) and lean
// (roll).
//
// This is where the weight on each tyre comes from. Brake hard and the body
// pitches forward, the front springs squash and the front tyres get more
// weight (and grip). Corner and the body leans, the outside springs squash.
// Because the springs take a moment to squash, the weight shifts the way it
// does in a real car: quickly, but not instantly, and with a little overshoot
// on a sudden change. Bumps and kerbs push each wheel up, so the weight on it
// (and its grip) jumps about on rough ground, and a crest goes light.
//
// Numbers are per kilogram of car, like the rest of the model: a spring rate
// is newtons per metre per kilogram, and a force is an acceleration.

/** The four wheels, in order: front left, front right, rear left, rear right. */
export const FRONT = [true, true, false, false];
export const LEFT = [true, false, true, false];

/** The suspension's fixed numbers for one car. */
export interface SuspensionSpecs {
  /** Spring rate at each wheel, per kg of car. */
  spring: number;
  /** Damper rates: squashing (bump) and stretching (rebound). */
  bump: number;
  rebound: number;
  /** Anti-roll bar rates, front and rear. */
  barFront: number;
  barRear: number;
  /** How far each spring is squashed with the car standing still, m. */
  sag: number;
  /** How much further it can squash before the bump stop, m. */
  travel: number;
  /** Damper speed where the damper's valve opens and it softens, m/s. */
  knee: number;
  /** Pitch and roll inertia per kg, m². */
  pitchInertia: number;
  rollInertia: number;
}

/** The body's motion on its springs, kept from step to step. */
export interface SuspensionState {
  /** Height above its resting height, m (up is positive), and how fast it's moving. */
  heave: number;
  heaveRate: number;
  /** Nose down is positive, radians. */
  pitch: number;
  pitchRate: number;
  /** Left side up (leaning to the right) is positive, radians. */
  roll: number;
  rollRate: number;
  /** How far each spring is squashed, m (0 is fully stretched; below that the wheel is off the ground). */
  compression: number[];
  /** The weight on each tyre, per kg of car, m/s². */
  load: number[];
  /** How hard the wheels are being shaken by the road, for the controller's rumble. */
  shake: number;
  /** The road under each wheel as the spring feels it, m: the tyre's own give smooths sharp edges. */
  road: number[];
}

export function newSuspension(): SuspensionState {
  return { heave: 0, heaveRate: 0, pitch: 0, pitchRate: 0, roll: 0, rollRate: 0, compression: [0, 0, 0, 0], load: [0, 0, 0, 0], shake: 0, road: [0, 0, 0, 0] };
}

/**
 * Springs for a car: `frequency` is how fast the body bounces on them, in Hz
 * (a family car about 1.4, a sports car 2, a race car 3). A bigger car
 * bounces more slowly, as a scaled-up real one would. `gravity` sets how far
 * the springs sag under the car's weight.
 */
export function suspensionSpecs(opts: {
  frequency: number;
  size: number;
  gravity: number;
  wheelBase: number;
  halfTrack: number;
  /** Extra rear anti-roll bar (drift builds like the back to step out). */
  rearBar: number;
}): SuspensionSpecs {
  const f = opts.frequency / Math.sqrt(Math.max(0.15, opts.size));
  const omega = 2 * Math.PI * f;
  // Four springs carry the car: together they bounce it at `f`.
  const spring = (omega * omega) / 4;
  // Dampers: about half of critical, a bit softer squashing than stretching (as real dampers are set).
  const critical = Math.sqrt(spring);
  return {
    spring,
    bump: 0.45 * critical,
    rebound: 0.7 * critical,
    // The front bar is stiffer, so the front takes more of the cornering load:
    // the car runs a little wide at the limit rather than spinning, like a road car.
    barFront: 0.9 * spring,
    barRear: (0.35 + opts.rearBar) * spring,
    sag: opts.gravity / (4 * spring),
    travel: 0.09 * Math.max(0.15, opts.size),
    knee: 0.15 * Math.sqrt(Math.max(0.15, opts.size)),
    pitchInertia: 0.95 * (opts.wheelBase / 2) ** 2,
    rollInertia: 0.8 * opts.halfTrack ** 2,
  };
}

/**
 * One step of the body on its springs. `road[i]` is how far the ground under
 * each wheel sits above the plane the car is driving on (bumps, kerbs, a
 * crest), `x[i]` and `y[i]` where each wheel is (forwards and left of the
 * centre), `gravity` the weight pressing down (with downforce), `aeroPitch`
 * the nose-down turning from downforce, and `ax`, `ay` the car's
 * acceleration (forwards and left), which throws the body about.
 */
export function stepSuspension(
  s: SuspensionState,
  sp: SuspensionSpecs,
  dt: number,
  input: { road: number[]; x: number[]; y: number[]; gravity: number; aeroPitch: number; ax: number; ay: number; comHeight: number },
): void {
  const forces = [0, 0, 0, 0];
  const squash = [0, 0, 0, 0];
  let shake = 0;
  // The tyre and wheel are a small spring and mass of their own: a sharp edge
  // reaches the suspension over a few hundredths of a second, not instantly.
  const give = Math.min(1, dt / 0.012);
  for (let i = 0; i < 4; i++) {
    s.road[i] += (input.road[i] - s.road[i]) * give;
    // How far the body has moved up at this wheel, from heave, pitch and roll.
    const lift = s.heave - s.pitch * input.x[i] + s.roll * input.y[i];
    const c = sp.sag - lift + s.road[i];
    const rate = (c - s.compression[i]) / dt;
    s.compression[i] = c;
    squash[i] = c;
    if (c <= 0) continue; // wheel off the ground
    let f = sp.spring * c;
    // The bump stop: a stiff rubber block at the end of the travel.
    // (Rubber soaks up much of the blow: it pushes back far less than it took.)
    const over = c - (sp.sag + sp.travel);
    if (over > 0) f += sp.spring * 12 * over * (rate > 0 ? 1 : 0.25);
    // The damper. Real ones are "digressive": firm for the body's slow
    // movements, softer for fast hits (a valve opens), so a pothole doesn't
    // slam the car.
    const fast = Math.abs(rate) > sp.knee ? Math.sign(rate) * (sp.knee + 0.3 * (Math.abs(rate) - sp.knee)) : rate;
    f += (rate > 0 ? sp.bump : sp.rebound) * fast;
    forces[i] = f;
    shake += Math.abs(rate);
  }
  // Anti-roll bars: each pushes down the wheel that's squashed less and up
  // the one squashed more, so the body leans less in corners.
  for (const [l, r, bar] of [[0, 1, sp.barFront], [2, 3, sp.barRear]] as const) {
    const twist = bar * (Math.max(0, squash[l]) - Math.max(0, squash[r]));
    if (squash[l] > 0) forces[l] += twist;
    if (squash[r] > 0) forces[r] -= twist;
  }
  let up = 0;
  let pitch = 0;
  let roll = 0;
  for (let i = 0; i < 4; i++) {
    const f = Math.max(0, forces[i]); // a spring can push but never pull the road
    s.load[i] = f;
    up += f;
    pitch -= f * input.x[i]; // a push at the front lifts the nose
    roll += f * input.y[i]; // a push on the left lifts the left
  }
  // Braking throws the body forward (nose down); cornering throws it to the outside.
  pitch -= input.ax * input.comHeight;
  roll += input.ay * input.comHeight;
  pitch += input.aeroPitch;
  // Semi-implicit Euler: the speeds first, then the positions with the new speeds (steady with springs).
  s.heaveRate += (up - input.gravity) * dt;
  s.pitchRate += (pitch / sp.pitchInertia) * dt;
  s.rollRate += (roll / sp.rollInertia) * dt;
  s.heave += s.heaveRate * dt;
  s.pitch += s.pitchRate * dt;
  s.roll += s.rollRate * dt;
  // Never fold through the ground (a huge landing): the body stops on the
  // bump stops. Nor rise off the wheels: past full droop the wheels would
  // leave the ground, and the game handles flying.
  const lowest = -(sp.travel * 2.5);
  const highest = sp.sag + sp.travel * 0.6;
  if (s.heave < lowest) {
    s.heave = lowest;
    s.heaveRate = Math.max(0, s.heaveRate);
  } else if (s.heave > highest) {
    s.heave = highest;
    s.heaveRate = Math.min(0, s.heaveRate);
  }
  [s.pitch, s.pitchRate] = limit(s.pitch, s.pitchRate, 0.25);
  [s.roll, s.rollRate] = limit(s.roll, s.rollRate, 0.25);
  s.shake += (shake / 4 - s.shake) * Math.min(1, dt * 20);
}

/** Keeps an angle within ±`most`, stopping it there (not still turning on) at the end. */
function limit(angle: number, rate: number, most: number): [number, number] {
  if (angle > most) return [most, Math.min(0, rate)];
  if (angle < -most) return [-most, Math.max(0, rate)];
  return [angle, rate];
}

/** In the air: the body settles back to its resting position, and no tyre carries any weight. */
export function relaxSuspension(s: SuspensionState, sp: SuspensionSpecs, dt: number): void {
  const k = Math.min(1, dt * 6);
  s.heave -= s.heave * k;
  s.pitch -= s.pitch * k;
  s.roll -= s.roll * k;
  s.heaveRate = s.pitchRate = s.rollRate = 0;
  s.load.fill(0);
  s.road.fill(0);
  // The wheels hang down: springs at full stretch (so a landing squashes them from there).
  s.compression.fill(0);
  s.shake *= 1 - k;
  void sp;
}

/**
 * Small bumps in the road surface under a point, m: a fixed pattern (the same
 * every time you drive over the same spot), rougher off the tarmac.
 */
export function roughness(x: number, z: number, amplitude: number): number {
  if (amplitude <= 0) return 0;
  const cell = 0.8;
  const gx = x / cell;
  const gz = z / cell;
  const ix = Math.floor(gx);
  const iz = Math.floor(gz);
  const fx = smooth(gx - ix);
  const fz = smooth(gz - iz);
  const a = hash(ix, iz);
  const b = hash(ix + 1, iz);
  const c = hash(ix, iz + 1);
  const d = hash(ix + 1, iz + 1);
  const value = a + (b - a) * fx + (c - a) * fz + (a - b - c + d) * fx * fz;
  return (value - 0.5) * 2 * amplitude;
}

function smooth(t: number): number {
  return t * t * (3 - 2 * t);
}

/** A repeatable random number 0..1 for a grid cell. */
function hash(x: number, z: number): number {
  let h = Math.imul(x, 374761393) + Math.imul(z, 668265263);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}
