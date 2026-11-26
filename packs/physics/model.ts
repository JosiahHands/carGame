// The realistic driving model: four tires on a car body that rides on real
// suspension. It takes the place of the game's arcade model (see CarDynamics
// in game/carPhysics.ts) for every car, the player's and the rivals'.
//
// Each step (four times per physics step: 480 times a second):
//   1. The suspension (suspension.ts): the body rises, dips and leans on four
//      springs and dampers, with anti-roll bars. The springs' push is the
//      weight on each tire: it moves back under acceleration, forward under
//      braking, outwards in corners, and jumps about over bumps and curbs.
//   2. The engine and gearbox (powertrain.ts): a torque curve for each kind
//      of engine, turbo lag, a slipping clutch when pulling away, a moment
//      with the power cut while changing gear, and engine braking. The
//      differential shares the push between the driven wheels.
//   3. Each tire (tyres.ts): how fast its contact patch slides along the
//      road (wheelspin or locked brakes) and across it (slip angle), and the
//      force from the "Magic Formula" curve real tire models use. Grip rises
//      with slip to a peak, then falls away as it slides, and is shared: a
//      tire braking hard can't also corner hard. Grip grows with the weight
//      on the tire, and depends on its temperature and wear.
//   4. Air drag and downforce, then the driving aids (traction control, ABS
//      and stability control), and the car moves.
//
// How much of this you feel is set by the realism level: Friendly keeps the
// tires at their best and the gearbox instant; Real adds tire warm-up and
// wear, gear-change time, turbo lag, brake fade and damage that changes how
// the car drives; Simulation starts on cold tires and is harsher still.
//
// Numbers are per kilogram of car (forces are accelerations), so a toy car
// and a 100 ft monster drive by the same rules.
import { CarPhysics, GEAR_TOPS, MAX_REVERSE, type CarDynamics, type CarInput } from '../../game/carPhysics';
import type { World } from '../../game/world';
import { autoGear, differential, engineFor, spoolTurbo, torqueAt, turboShare, type Engine } from './powertrain';
import { FRONT, LEFT, newSuspension, relaxSuspension, roughness, stepSuspension, suspensionSpecs, type SuspensionSpecs, type SuspensionState } from './suspension';
import { ambientTemperature, bestTemperature, heatTyre, magic, PEAK, PEAK_SLIP, temperatureGrip, wearGrip } from './tyres';

/** How much driving help a car gets. Rivals always get full help. */
export type Assists = 'full' | 'some' | 'off';

/** How much of real life the player's car has to deal with. */
export type Realism = 'friendly' | 'real' | 'sim';

export interface ModelOptions {
  /** Tire grip: 0.95 is a road tire, 1.2 sporty, 1.45 a racing slick. */
  grip: number;
  /** The player's driving aids. */
  assists: Assists;
  /** The player's realism level (rivals always drive on 'real' rules, without damage or wear). */
  realism: Realism;
  /** The player's car, which uses `assists` and `realism`. */
  player: CarPhysics;
}

/** What each realism level switches on. */
interface Rules {
  /** Tires warm up and cool down (otherwise they're always at their best). */
  tyreHeat: boolean;
  /** Where the tires start: this many °C under their best (null: at the air temperature, stone cold). */
  startBelowBest: number | null;
  /** The least grip a cold or overheated tire keeps. */
  tempFloor: number;
  /** How fast the tires wear (0: never). */
  wear: number;
  /** Seconds the power is cut for each gear change. */
  shift: number;
  turboLag: boolean;
  /** How much crashes change the way the car drives (0: not at all). */
  damage: number;
  /** Brakes that get too hot lose some of their bite. */
  brakeFade: boolean;
  /** How rough the road feels (bumps in the surface). */
  road: number;
}

const RULES: Record<Realism | 'rival', Rules> = {
  friendly: { tyreHeat: false, startBelowBest: 0, tempFloor: 1, wear: 0, shift: 0, turboLag: false, damage: 0, brakeFade: false, road: 0.5 },
  real: { tyreHeat: true, startBelowBest: 20, tempFloor: 0.88, wear: 0.5, shift: 0.15, turboLag: true, damage: 1, brakeFade: true, road: 1 },
  sim: { tyreHeat: true, startBelowBest: null, tempFloor: 0.8, wear: 1, shift: 0.2, turboLag: true, damage: 1.6, brakeFade: true, road: 1.25 },
  rival: { tyreHeat: false, startBelowBest: 0, tempFloor: 1, wear: 0, shift: 0.15, turboLag: true, damage: 0, brakeFade: false, road: 1 },
};

/**
 * The weight the tires feel, m/s². Low-gravity cars (the moon buggy) keep
 * Earth grip, or they could hardly pull away; they still fly further.
 */
function tyreGravity(car: CarPhysics): number {
  return Math.max(car.gravity, 9.81);
}

/** Damage from crashes that changes how the car drives (Real and Simulation only, the player's car only). */
export interface Damage {
  /** 0..0.6: power lost from a damaged engine (front hits). */
  engine: number;
  /** Bent steering, radians: the car pulls to one side. */
  toe: number;
  /** 0..0.4 for each wheel: grip lost from a bent wheel or suspension. */
  wheel: number[];
}

/** What each car keeps between steps. */
export interface CarState {
  /** Turning speed, rad/s (positive is left). */
  yawRate: number;
  /** Each wheel's spin, rad/s. */
  spin: number[];
  /** Slip angles with a little lag (tires take a moment to build up sideways force). */
  slipAngle: number[];
  /** Accelerations, a little smoothed: forward and left, m/s². */
  ax: number;
  ay: number;
  susp: SuspensionState;
  /** Each tire's temperature (°C) and wear (0 new .. 1 bald). */
  temp: number[];
  wear: number[];
  /** Grip from temperature and wear, averaged over the tires (the AI plans its corners with it). */
  gripFactor: number;
  /** Turbo boost, 0..1. */
  boost: number;
  /** Engine revs 0..1 of the redline (above the wheels' revs while the clutch slips). */
  engineRevs: number;
  /** Seconds left of a gear change (power cut), and the gear before it. */
  shiftCut: number;
  lastGear: number;
  /** Seconds since the last gear change. */
  sinceShift: number;
  /** Brake temperature, °C, and whether they've started to fade. */
  brakeTemp: number;
  fade: boolean;
  damage: Damage;
  /** Falling speed just before landing, for the springs to soak up. */
  airVy: number;
  wasAirborne: boolean;
  /** A sudden jolt (a crash or a heavy landing), 0..1, fading: for the controller's rumble. */
  jolt: number;
  // For the telemetry panel: the weight on each tire (1 is its share standing still), how hard each tire is working (0..1+, over 1 is sliding), and the aids at work.
  load: number[];
  usage: number[];
  slipRatio: number[];
  tractionControl: boolean;
  abs: boolean;
  stability: boolean;
  /** Engine speed, 0..1 of the redline. */
  rpm: number;
}

function newState(): CarState {
  return {
    yawRate: 0, spin: [0, 0, 0, 0], slipAngle: [0, 0, 0, 0], ax: 0, ay: 0,
    susp: newSuspension(), temp: [NaN, NaN, NaN, NaN], wear: [0, 0, 0, 0], gripFactor: 1,
    boost: 0, engineRevs: 0, shiftCut: 0, lastGear: 1, sinceShift: 9, brakeTemp: NaN, fade: false,
    damage: { engine: 0, toe: 0, wheel: [0, 0, 0, 0] }, airVy: 0, wasAirborne: false, jolt: 0,
    load: [0, 0, 0, 0], usage: [0, 0, 0, 0], slipRatio: [0, 0, 0, 0],
    tractionControl: false, abs: false, stability: false, rpm: 0,
  };
}

export function stateOf(car: CarPhysics): CarState {
  if (!car.dynamicsState) car.dynamicsState = newState();
  return car.dynamicsState as CarState;
}

/** The car's fixed numbers, worked out from its garage settings and size. */
export interface Specs {
  /** Wheel radius, m. */
  radius: number;
  /** Front and rear axle distance from the center, and half the distance between left and right wheels, m. */
  front: number;
  rear: number;
  halfTrack: number;
  /** Center of mass height, m. */
  comHeight: number;
  /** Yaw inertia per kg, m². */
  yawInertia: number;
  /** Wheel inertia per kg of car, m² (kept a bit heavy for steadiness). */
  wheelInertia: number;
  /** Engine power per kg at its peak, W/kg. */
  power: number;
  /** Air drag per kg: force = drag × v². */
  drag: number;
  /** Which wheels the engine drives: share of torque to the front. */
  frontDrive: number;
  /** Differential locking, front and rear (0 open .. 1 locked). */
  lsdFront: number;
  lsdRear: number;
  /** Most braking per kg (as a force at the tires), m/s². */
  brake: number;
  /** Driver's steering lock, radians. */
  maxSteer: number;
  engine: Engine;
  suspension: SuspensionSpecs;
  /** The car's size compared to a normal car (1). */
  size: number;
  /** Where each wheel is: forwards and left of the center, m. */
  wheelX: number[];
  wheelY: number[];
}

const ROLLING = 0.013; // rolling resistance on tarmac (Bosch)
const GRASS_ROLLING = 0.09;
const DRIVELINE = 0.88; // share of engine power that reaches the wheels
/** Share of the downforce on the front axle (a little more on the back keeps a car stable at speed). */
const AERO_FRONT = 0.45;
/** How much the body's lean and dive is shown on the model (twice the real angles, so you can see it). */
const SHOW_LEAN = 2;
/** Share of the tires' grip the AI drivers plan their corners with. */
export const CORNER_MARGIN = 0.82;

export function specsOf(car: CarPhysics): Specs {
  const size = Math.sqrt((car.length * car.height) / (4.4 * 1.3));
  const radius = 0.33 * Math.max(0.15, size) * (car.config?.wheelSize ?? 1) ** 0.5;
  const half = car.wheelBase / 2;
  // 400 hp gives the arcade model's 10 m/s² pull; the same here at about 20 m/s.
  const power = car.accel * 20;
  const top = car.topSpeed;
  // Drag set so full power (at the redline in top gear) just holds the top speed.
  const atTop = power * DRIVELINE * 0.95;
  const drag = Math.max(1e-7, (atTop / top - ROLLING * car.gravity) / (top * top));
  // Drive layout from the body: hatchbacks are front-wheel drive, pickups and vans four-wheel drive.
  const body = car.config?.body;
  const fourWheel = body === 'pickup' || body === 'van' || car.allTerrain;
  const frontDrive = body === 'hatch' ? 1 : fourWheel ? 0.4 : 0;
  const drift = car.config?.tuning.drift ?? 5;
  // Differentials: rear-drive performance cars have a limited-slip diff (more locking on drift builds),
  // four-wheel drives a locking one on the back, hot hatches an open front.
  const lsdRear = Math.min(1, (fourWheel ? 0.6 : 0.4) + Math.max(0, drift - 5) * 0.08);
  const lsdFront = fourWheel ? 0.3 : 0;
  // Springs: firm on sports cars, softer on the big ones, firmer with aero.
  const frequency =
    (car.allTerrain ? 1.3 : body === 'wedge' ? 2.4 : body === 'sports' ? 2.1 : body === 'hatch' ? 1.9 : body === 'muscle' ? 1.7 : body === 'pickup' ? 1.4 : body === 'van' ? 1.3 : 1.8) +
    car.downforce * 0.08;
  const halfTrack = (car.width / 2) * 0.86;
  return {
    radius,
    front: half,
    rear: half,
    halfTrack,
    comHeight: car.height * 0.38,
    yawInertia: (car.length ** 2 + car.width ** 2) / 12,
    wheelInertia: 0.004 * radius * radius * 8,
    power,
    drag,
    frontDrive,
    lsdFront,
    lsdRear,
    brake: car.brakeDecel * 1.4,
    maxSteer: car.maxSteer,
    engine: engineFor(body),
    suspension: suspensionSpecs({
      frequency,
      size,
      gravity: tyreGravity(car),
      wheelBase: car.wheelBase,
      halfTrack,
      rearBar: Math.max(0, drift - 5) * 0.06,
    }),
    size,
    wheelX: FRONT.map((f) => (f ? half : -half)),
    wheelY: LEFT.map((l) => (l ? halfTrack : -halfTrack)),
  };
}

/** The realistic model, ready to put in CarPhysics.dynamics. */
export class RealisticDynamics implements CarDynamics {
  private specs = new WeakMap<CarPhysics, { key: string; specs: Specs }>();

  constructor(public options: ModelOptions) {}

  specsFor(car: CarPhysics): Specs {
    // Worked out again when the car changes (a new build or size).
    const key = `${car.accel}/${car.topSpeed}/${car.length}/${car.width}/${car.height}/${car.wheelBase}/${car.brakeDecel}/${car.maxSteer}/${car.config?.body}/${car.allTerrain}/${car.config?.wheelSize}/${car.gravity}/${car.downforce}/${car.config?.tuning.drift}`;
    const saved = this.specs.get(car);
    if (saved && saved.key === key) return saved.specs;
    const specs = specsOf(car);
    this.specs.set(car, { key, specs });
    return specs;
  }

  private rulesFor(car: CarPhysics): Rules {
    return car === this.options.player ? RULES[this.options.realism] : RULES.rival;
  }

  /** Peak tire grip at this speed and surface, m/s² (the same sum the tires use, without weight transfer). */
  cornerGrip(car: CarPhysics, speed: number, surfaceGrip: number): number {
    const mu = this.options.grip * car.gripMultiplier * surfaceGrip;
    // Downforce: the same as the arcade model, doubling the grip at 10/10 and 100 mph.
    const down = 1 + (car.downforce / 10) * (speed / 44.7) ** 2;
    // Cold, hot or worn tires grip less.
    const tyres = (car.dynamicsState as CarState | null)?.gripFactor ?? 1;
    // Under the limit: weight transfer costs some grip in a real corner, and
    // the drivers that plan with this (rivals, the city's drivers) need some
    // in hand to brake and turn at once. (0.82 kept rivals on the road about
    // a third more of the time than 0.92, for the same lap times.)
    return mu * tyreGravity(car) * down * tyres * CORNER_MARGIN;
  }

  /**
   * Put back on the road (a respawn): the car stops moving, but keeps its
   * damage, tire wear and heat. A fresh start (an event's grid, a retry)
   * begins with a sound car instead.
   */
  reset(car: CarPhysics, fresh = false): void {
    const old = car.dynamicsState as CarState | null;
    const s = newState();
    if (old && !fresh) {
      s.damage = old.damage;
      s.temp = old.temp;
      s.wear = old.wear;
      s.brakeTemp = old.brakeTemp;
    }
    car.dynamicsState = s;
  }

  /** Back from the garage: crash damage fixed, fresh tires, cool brakes. */
  repair(car: CarPhysics): void {
    const s = stateOf(car);
    s.damage = { engine: 0, toe: 0, wheel: [0, 0, 0, 0] };
    s.wear.fill(0);
    s.temp.fill(NaN);
    s.brakeTemp = NaN;
  }

  /** A crash: a blow off the car's center spins it, rocks the body on its springs, and (on Real and Simulation) damages the car. */
  impact(car: CarPhysics, nx: number, nz: number, along: number, across: number, lost: number): void {
    const s = stateOf(car);
    const sp = this.specsFor(car);
    const fx = Math.sin(car.heading);
    const fz = Math.cos(car.heading);
    // The push, in the car's own directions (forwards and to the left).
    const pf = (nx * fx + nz * fz) * lost;
    const pl = (nx * fz - nz * fx) * lost;
    // Turning effect of a push at (along, across): along × left-push − across × forward-push.
    s.yawRate += ((along * pl - across * pf) / sp.yawInertia) * 0.5;
    s.yawRate = Math.max(-8, Math.min(8, s.yawRate));
    // The body rocks: a hit from the side leans it away, a hit from the front lifts the nose.
    s.susp.rollRate -= pl * 0.04;
    s.susp.pitchRate -= pf * 0.03;
    s.jolt = Math.max(s.jolt, Math.min(1, lost / 18));
    const rules = this.rulesFor(car);
    if (rules.damage <= 0 || lost < 3) return;
    const hurt = (lost - 3) * rules.damage;
    const d = s.damage;
    const front = along > car.length * 0.15;
    const leftSide = across > 0;
    // A hit on the nose damages the engine.
    if (front) d.engine = Math.min(0.6, d.engine + hurt / 160);
    // The wheel nearest the hit gets bent: less grip, and a bent front wheel pulls the steering.
    const wheel = (front ? 0 : 2) + (leftSide ? 0 : 1);
    if (Math.abs(along) > car.length * 0.15 || Math.abs(across) > car.width * 0.3) {
      d.wheel[wheel] = Math.min(0.4, d.wheel[wheel] + hurt / 220);
      if (front) d.toe = Math.max(-0.035, Math.min(0.035, d.toe + (leftSide ? -1 : 1) * hurt * 0.0006));
    }
  }

  /** Shows the body's lean and dive on the model (called after each physics step). */
  pose(car: CarPhysics): void {
    if (car.airborne) return; // the game tips the car in the air
    const s = stateOf(car).susp;
    car.roll = s.roll * SHOW_LEAN;
    car.bodyPitch = s.pitch * SHOW_LEAN;
  }

  drive(car: CarPhysics, dt: number, input: CarInput, world: World): number {
    // Four smaller steps (480 a second): the tires and springs respond quickly, so this keeps them steady.
    const steps = 4;
    const s = stateOf(car);
    if (car.airborne) {
      s.wasAirborne = true;
      s.airVy = Math.min(s.airVy, car.vy);
    }
    for (let i = 0; i < steps; i++) this.step(car, dt / steps, input, world);
    s.jolt *= Math.max(0, 1 - dt * 4);
    return s.yawRate;
  }

  private step(car: CarPhysics, dt: number, input: CarInput, world: World): void {
    const s = stateOf(car);
    const sp = this.specsFor(car);
    const rules = this.rulesFor(car);
    const isPlayer = car === this.options.player;
    const assists: Assists = isPlayer ? this.options.assists : 'full';
    const help = assists === 'full' ? 1 : assists === 'some' ? 0.5 : 0;
    car.feelSurface(world);

    // Velocity in the car's own directions: u forwards, v to the left.
    const fx = Math.sin(car.heading);
    const fz = Math.cos(car.heading);
    let u = car.vx * fx + car.vz * fz;
    let v = car.vx * fz - car.vz * fx;
    let r = s.yawRate;
    const speed = Math.hypot(u, v);
    // A car that was already moving when this model took over (the pack was
    // just switched on): its wheels are rolling, not standing still.
    if (Math.abs(u) > 2 && !s.spin.some((w) => w !== 0)) s.spin.fill(u / sp.radius);

    // Tires and brakes start at the temperature the realism level says.
    const ambient = ambientTemperature(world.theme.weather, world.isNight);
    const best = bestTemperature(this.options.grip);
    if (Number.isNaN(s.temp[0])) s.temp.fill(rules.startBelowBest === null ? ambient : best - rules.startBelowBest);
    if (Number.isNaN(s.brakeTemp)) s.brakeTemp = ambient + 40;

    // ---------- steering ----------
    // Less lock at speed (as in the arcade model), but steering into a slide
    // (counter-steer) is never held back. With help, the wheel also catches
    // slides by itself a little.
    const sideslip = speed > 3 ? Math.atan2(v, Math.abs(u)) : 0;
    const lock = sp.maxSteer / (1 + Math.abs(u) / 28);
    let target = input.steer * lock;
    const counter = Math.sign(input.steer) === Math.sign(sideslip) && Math.abs(sideslip) > 0.05;
    if (counter) target = input.steer * Math.max(lock, Math.min(sp.maxSteer, Math.abs(sideslip) * 1.2));
    target += sideslip * 0.5 * help * (u > 0 ? 1 : 0);
    target = Math.max(-sp.maxSteer, Math.min(sp.maxSteer, target));
    car.steerAngle += (target - car.steerAngle) * Math.min(1, dt * 12);
    // Bent steering from a crash adds a pull to one side.
    const delta = car.steerAngle + s.damage.toe;

    if (car.airborne) {
      relaxSuspension(s.susp, sp.suspension, dt);
      // In the air: no grip. Steer, throttle, brake and nitro still nudge the
      // car (air control), the same amounts as the arcade model.
      const control = car.airControl;
      r += (input.steer * 2.5 * control - r) * Math.min(1, dt * 2);
      // The gearbox keeps working in the air, as in the arcade model: an
      // automatic changes gear with speed, and a manual's rev limiter cuts the
      // power at the redline (and a high gear bogs down at low revs).
      if (!car.manual) {
        car.gear = 1;
        while (car.gear < GEAR_TOPS.length && Math.abs(u) > GEAR_TOPS[car.gear - 1] * car.topSpeed) car.gear++;
      }
      s.lastGear = car.gear;
      car.revs = u < 0 ? -u / MAX_REVERSE : u / (GEAR_TOPS[car.gear - 1] * car.topSpeed);
      s.rpm = Math.min(1.2, Math.abs(car.revs));
      s.engineRevs = Math.abs(car.revs);
      let pull = 1;
      if (car.manual) {
        if (car.revs >= 1) pull = 0;
        else if (car.gear > 1) pull = 0.12 + 0.88 * Math.min(1, car.revs / 0.53) ** 2;
      }
      car.limiter = car.manual && car.revs >= 1 && input.throttle > 0;
      car.braking = false;
      if (input.throttle > 0) {
        if (u < -0.5) {
          u = Math.min(0, u + car.brakeDecel * input.throttle * dt * control);
          car.braking = true;
        } else u += car.accel * input.throttle * Math.max(0, 1 - (u / (car.topSpeed * (1 + 0.08 * car.draft))) ** 2) * pull * dt * control;
      }
      const power = car.useBoost(dt, input, u);
      if (power > 0) u += car.accel * 1.6 * power * Math.max(0, 1 - (u / (car.topSpeed * (1 + 0.35 * power))) ** 2) * dt * Math.max(0.4, control);
      if (input.brake > 0) {
        if (u > 0.5) {
          u = Math.max(0, u - car.brakeDecel * input.brake * dt * control);
          car.braking = true;
        } else if (u > -MAX_REVERSE) u -= car.accel * 0.6 * input.brake * dt * control;
      }
      s.yawRate = r;
      for (let i = 0; i < 4; i++) s.spin[i] += (u / sp.radius - s.spin[i]) * Math.min(1, dt * 2);
      s.load.fill(0);
      this.setVelocity(car, u, v, fx, fz, dt);
      return;
    }

    // Just landed: the springs soak up the fall.
    if (s.wasAirborne) {
      s.wasAirborne = false;
      const fall = Math.min(0, s.airVy, car.vy);
      // (A big drop bottoms the springs out: the tires and bump stops take the rest of the blow.)
      s.susp.heaveRate += Math.max(fall * 0.85, -4);
      s.jolt = Math.max(s.jolt, Math.min(1, -fall / 12));
      s.airVy = 0;
    }

    // ---------- the gearbox and engine ----------
    const driven = [0, 1, 2, 3].filter((i) => (FRONT[i] ? sp.frontDrive > 0 : sp.frontDrive < 1));
    const wheelSpeed = (driven.reduce((sum, i) => sum + s.spin[i], 0) / driven.length) * sp.radius;
    const tops = GEAR_TOPS.map((t) => t * car.topSpeed);
    s.sinceShift += dt;
    if (!car.manual && s.sinceShift > 0.5) {
      // Changes up just before the redline, by road speed (so a wheelspin
      // doesn't send it hunting up and down), and never twice in half a second.
      car.gear = autoGear(car.gear, Math.abs(u) * 1.06, tops);
    }
    // A gear change takes a moment: the clutch is out and the power is cut. (A manual is quicker.)
    if (car.gear !== s.lastGear) {
      s.sinceShift = 0;
      if (car.gear > s.lastGear && rules.shift > 0) s.shiftCut = rules.shift * (car.manual ? 0.7 : 1);
      s.lastGear = car.gear;
    }
    s.shiftCut = Math.max(0, s.shiftCut - dt);
    const reversing = input.brake > 0 && u < 0.5 && input.throttle === 0;
    const gearTop = tops[car.gear - 1]; // road speed at the redline in this gear
    const boost = car.useBoost(dt, input, u);
    // Nitro lets the engine rev past the redline (×1.35), like the arcade model's higher top speed.
    const redlineSpeed = gearTop * (boost > 0 ? 1 + 0.35 * boost : 1);
    const wheelRevs = Math.abs(wheelSpeed) / gearTop;
    // Pulling away, the clutch slips: the engine revs up ahead of the wheels.
    const slipping = car.gear === 1 && wheelRevs < 0.35 && input.throttle > 0 && !reversing;
    const engineTarget = slipping ? Math.max(wheelRevs, 0.22 + 0.33 * input.throttle) : wheelRevs;
    s.engineRevs += (engineTarget - s.engineRevs) * Math.min(1, dt * (slipping ? 8 : 25));
    s.rpm = Math.min(1.2, s.engineRevs);
    car.revs = reversing ? Math.abs(u) / 12 : s.engineRevs;
    car.limiter = car.manual && car.revs >= 1 && input.throttle > 0;
    // The turbo builds boost with revs, after a moment.
    s.boost = spoolTurbo(s.boost, input.throttle, s.engineRevs, dt, rules.turboLag);
    // Drive force at the wheels per kg: power / speed at the redline, times the torque curve.
    const maxForce = ((sp.power * DRIVELINE) / Math.max(4, gearTop) / 0.95) * (1 - s.damage.engine);
    const torque = torqueAt(sp.engine, Math.max(0.12, s.engineRevs)) * turboShare(sp.engine, s.boost);
    let throttle = input.throttle;
    if (Math.abs(wheelSpeed) >= redlineSpeed && !reversing) throttle = 0; // rev limiter
    if (car.manual && car.gear > 1 && car.revs < 0.53) throttle *= 0.12 + 0.88 * (car.revs / 0.53) ** 2; // bogging down
    let driveForce = throttle * maxForce * torque * (1 + 0.6 * boost);
    if (boost > 0 && throttle === 0) driveForce = maxForce * 0.6 * boost; // nitro pushes even off the throttle
    if (s.shiftCut > 0) driveForce = 0;
    // Off the throttle in gear, the engine holds the car back (more at high revs and in low gears).
    else if (input.throttle === 0 && boost === 0 && !reversing && u > 1 && input.brake === 0) driveForce = -maxForce * (0.06 + 0.12 * Math.min(1, wheelRevs));
    if (reversing) driveForce = -input.brake * maxForce * 0.6 * (Math.abs(u) < 12 ? 1 : 0);
    // Throttle while rolling backwards brakes instead.
    const throttleBrakes = input.throttle > 0 && u < -0.5;
    if (throttleBrakes) driveForce = 0;

    // ---------- the suspension: weight on each wheel ----------
    const g0 = tyreGravity(car);
    const down = 1 + (car.downforce / 10) * (u / 44.7) ** 2;
    const g = g0 * down;
    const wheelBase = sp.front + sp.rear;
    // The ground under each wheel, compared to the plane the car sits on: bumps, curbs, crests.
    const roughAmount = rules.road * (car.onGrass ? 0.03 : car.surfaceGrip < 0.85 ? 0.012 : 0.003);
    const road = [0, 0, 0, 0];
    const limit = sp.suspension.sag + sp.suspension.travel;
    for (let i = 0; i < 4; i++) {
      const x = sp.wheelX[i];
      const y = sp.wheelY[i];
      const wx = car.x + fx * x + fz * y;
      const wz = car.z + fz * x - fx * y;
      const plane = car.y - car.pitch * x + car.groundRoll * y;
      const bump = world.heightAt(wx, wz) - plane + roughness(wx, wz, roughAmount);
      road[i] = Math.max(-limit, Math.min(limit * 1.5, bump));
    }
    const extra = g - g0; // downforce
    stepSuspension(s.susp, sp.suspension, dt, {
      road,
      x: sp.wheelX,
      y: sp.wheelY,
      gravity: g,
      aeroPitch: extra * (AERO_FRONT * sp.front - (1 - AERO_FRONT) * sp.rear),
      ax: s.ax,
      ay: s.ay,
      comHeight: sp.comHeight,
    });
    const loads = s.susp.load;

    // ---------- surface ----------
    const grassy = car.onGrass;
    let mu = this.options.grip * car.gripMultiplier * car.surfaceGrip;
    // Aquaplaning: standing water takes grip away at high speed, in the rain.
    if (world.theme.weather === 'rain' && world.map !== 'city') mu *= Math.max(0.55, 1 - 0.35 * Math.max(0, (speed - 25) / 40) ** 2);
    const drift = car.config?.tuning.drift ?? 0;
    // Wider rear tires (another understeer setting), less so on drift builds.
    const rearGrip = 1.1 - drift * 0.02;

    // ---------- the brakes ----------
    const handbrake = input.handbrake;
    const braking = input.brake > 0 && u > 0.5;
    const throttleBrake = throttleBrakes ? input.throttle : 0;
    car.braking = braking || throttleBrakes || handbrake;
    // Hot brakes fade: past about 450 °C they bite less and less, down to 40% at 700 °C.
    const fade = rules.brakeFade ? 1 - 0.6 * Math.max(0, Math.min(1, (s.brakeTemp - 450) / 250)) : 1;
    s.fade = fade < 0.97;

    // ---------- the differentials ----------
    const push = [0, 0, 0, 0];
    for (const [l, rt, share, lsd] of [[0, 1, sp.frontDrive, sp.lsdFront], [2, 3, 1 - sp.frontDrive, sp.lsdRear]] as const) {
      if (share <= 0) continue;
      [push[l], push[rt]] = differential(driveForce * share, lsd, s.spin[l] * sp.radius, s.spin[rt] * sp.radius, u);
    }

    // ---------- each tire ----------
    let fxSum = 0; // forward force per kg
    let fySum = 0; // left force per kg
    let moment = 0; // turning
    let brakePower = 0;
    let gripSum = 0;
    s.tractionControl = false;
    s.abs = false;
    const floor = Math.max(0.5, sp.radius * 3); // slower than this, slip is measured against a floor (no divide by zero)
    const staticLoad = g / 4;
    for (let i = 0; i < 4; i++) {
      const x = sp.wheelX[i];
      const y = sp.wheelY[i];
      // The contact patch's velocity, then turned into the wheel's own directions.
      const pu = u - r * y;
      const pv = v + r * x;
      const steer = FRONT[i] ? delta : 0;
      const cos = Math.cos(steer);
      const sin = Math.sin(steer);
      const wu = pu * cos + pv * sin; // along the tire
      const wv = -pu * sin + pv * cos; // across it
      const ref = Math.max(Math.abs(wu), floor);

      // Slip angle, built up over about a tire's width of rolling (relaxation length).
      const angleTarget = Math.atan2(wv, ref);
      const relax = Math.min(1, (Math.max(Math.abs(wu), 2) * dt) / Math.max(0.2, sp.radius));
      s.slipAngle[i] += (angleTarget - s.slipAngle[i]) * relax;

      const n = loads[i];
      // Load sensitivity: a heavily loaded tire grips a bit less per kg.
      const sensitivity = Math.max(0.7, 1 - 0.12 * (n / Math.max(1e-6, staticLoad) - 1));
      // Temperature and wear (always at their best on Friendly), a bent wheel, and the tire leaning over as the body rolls.
      const condition = rules.tyreHeat ? temperatureGrip(s.temp[i], best, rules.tempFloor) * wearGrip(s.wear[i]) : 1;
      gripSum += condition;
      const camber = 1 - Math.min(0.05, Math.abs(s.susp.roll) * 0.6);
      const peakForce = mu * n * sensitivity * condition * camber * (1 - s.damage.wheel[i] * 0.5) * (grassy ? 0.8 : 1) * (FRONT[i] ? 1 : rearGrip);

      let drive = push[i];
      let brake = 0;
      // 60% of the braking on the front wheels, 40% on the back.
      if (braking) brake = sp.brake * input.brake * (FRONT[i] ? 0.3 : 0.2) * fade;
      if (throttleBrakes) brake = sp.brake * throttleBrake * (FRONT[i] ? 0.3 : 0.2) * fade;
      if (handbrake && !FRONT[i]) brake = Math.max(brake, sp.brake * 0.6);

      // The tire force for a given wheel spin, as a function, so the wheel can
      // be stepped implicitly (stable however stiff the tire is).
      const angle = s.slipAngle[i];
      const tyre = (spin: number) => {
        const ratio = Math.max(-1.5, Math.min(1.5, (spin * sp.radius - wu) / ref));
        const sx = ratio / PEAK_SLIP;
        const sy = Math.tan(Math.max(-1.4, Math.min(1.4, angle))) / PEAK_SLIP;
        const combined = Math.hypot(sx, sy);
        if (combined < 1e-9 || peakForce <= 0) return { fx: 0, fy: 0, ratio, usage: 0 };
        const force = (peakForce * magic(combined * PEAK_SLIP)) / PEAK;
        return { fx: (force * sx) / combined, fy: (-force * sy) / combined, ratio, usage: combined };
      };

      // Traction control and ABS: ease off when the wheel spins or locks.
      const now = tyre(s.spin[i]);
      // (Traction control watches the whole grip budget, so it also stops
      // wheelspin from pushing the back out mid-corner.)
      if (help > 0 && drive > 0 && (now.ratio > 0.2 + (1 - help) * 0.25 || now.usage > 1.05 + (1 - help) * 0.3)) {
        drive *= 0.5;
        s.tractionControl = true;
      }
      if (help > 0 && brake > 0 && !handbrake && now.ratio < -0.12 - (1 - help) * 0.2) {
        brake *= 0.4;
        s.abs = true;
      }

      // Wheel spin: inertia × d(spin)/dt = (drive − tire force) × radius, minus brakes.
      const inertia = sp.wheelInertia;
      const h = 1e-3;
      const slope = (tyre(s.spin[i] + h).fx - now.fx) / h; // how the tire force changes with spin
      const free = s.spin[i] + (dt * (drive - now.fx) * sp.radius) / inertia;
      // Implicit step: allows for the tire force growing as the spin changes.
      let spin = s.spin[i] + (free - s.spin[i]) / (1 + (dt * sp.radius * Math.max(0, slope)) / inertia);
      // Brakes slow the wheel towards a standstill, but never spin it backwards.
      if (brake > 0) {
        const slowing = (dt * brake * sp.radius) / inertia;
        spin = Math.abs(spin) <= slowing ? 0 : spin - Math.sign(spin) * slowing;
        brakePower += brake * Math.abs(spin * sp.radius);
      }
      s.spin[i] = spin;
      const out = tyre(spin);
      s.slipRatio[i] = out.ratio;
      s.usage[i] = out.usage;
      s.load[i] = n / Math.max(1e-6, staticLoad);

      // The tire heats up as it slides (and a little as it rolls), and wears.
      if (rules.tyreHeat) {
        const sliding = Math.abs(out.fx * (spin * sp.radius - wu)) + Math.abs(out.fy * wv);
        const rolling = 0.012 * (n / Math.max(1e-6, staticLoad)) * Math.abs(wu);
        s.temp[i] = heatTyre(s.temp[i], sliding + rolling, speed, ambient, dt);
        s.wear[i] = Math.min(1, s.wear[i] + sliding * 2.5e-5 * rules.wear * dt); // bald after about 15 minutes sliding flat out on Simulation
      }

      // Back into the car's directions.
      let tfx = out.fx * cos - out.fy * sin;
      let tfy = out.fx * sin + out.fy * cos;
      // Rolling resistance (much more on grass).
      const rolling = (grassy ? GRASS_ROLLING : ROLLING) * n;
      tfx -= Math.sign(pu) * Math.min(Math.abs(pu) / dt / 4, rolling);
      // Never push the patch past standing still in one step (low-speed jitter).
      const maxLat = Math.abs(pv) / dt / 4 + peakForce * 0.02;
      tfy = Math.max(-maxLat, Math.min(maxLat, tfy));
      fxSum += tfx;
      fySum += tfy;
      moment += x * tfy - y * tfx;
    }
    s.gripFactor = gripSum / 4;
    // The brakes heat up with the work they do, and cool in the air rushing past.
    s.brakeTemp += (0.1 * brakePower - 0.02 * (s.brakeTemp - ambient) * (1 + speed / 30)) * dt;

    // Air drag (less when tucked in behind another car: the slipstream).
    const air = sp.drag * speed * (1 - 0.22 * car.draft);
    fxSum -= air * u;
    fySum -= air * v;

    // ---------- stability control ----------
    // When the car turns faster than the wheels are asking (the back is
    // stepping out: oversteer), it's turned back towards the way they point.
    s.stability = false;
    if (help > 0 && !handbrake && u > 5) {
      const wanted = (u * Math.tan(delta)) / wheelBase;
      const most = (mu * g) / Math.max(u, 1);
      const aim = Math.max(-most, Math.min(most, wanted));
      const spinning = Math.abs(r) > Math.abs(aim) + 0.1 * (1.6 - help) || (Math.sign(r) !== Math.sign(aim) && Math.abs(r) > 0.15);
      if (spinning && Math.abs(sideslip) > 0.06) {
        moment -= (r - aim) * sp.yawInertia * 5 * help;
        s.stability = true;
      }
    }

    // ---------- move ----------
    // The forces are in the car's directions at the start of the step, and the
    // velocity is written back to the world in those same directions. (The
    // car's turning is then allowed for when the next step measures u and v
    // against its new heading, so there's no extra turning term here.)
    u += fxSum * dt;
    v += fySum * dt;
    r += (moment / sp.yawInertia) * dt;
    // A car sitting still with no input stays still.
    if (speed < 0.3 && input.throttle === 0 && input.brake === 0 && boost === 0) {
      u *= 0.9;
      v *= 0.9;
      r *= 0.9;
    }
    s.yawRate = r;
    // The accelerations that throw the body about (lightly smoothed: the springs do the rest).
    const settle = Math.min(1, dt * 30);
    s.ax += (fxSum - s.ax) * settle;
    s.ay += (fySum - s.ay) * settle;
    this.setVelocity(car, u, v, fx, fz, dt);
  }

  /** Writes the car-frame velocity (u forwards, v left) back to the world, and turns the car. */
  private setVelocity(car: CarPhysics, u: number, v: number, fx: number, fz: number, dt: number): void {
    car.vx = fx * u + fz * v;
    car.vz = fz * u - fx * v;
    car.heading += stateOf(car).yawRate * dt;
  }
}
