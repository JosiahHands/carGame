// The realistic driving model: four tyres on a car body that slides, turns and
// leans. It takes the place of the game's arcade model (see CarDynamics in
// game/carPhysics.ts) for every car, the player's and the rivals'.
//
// Each step, for each wheel:
//   1. How fast the tyre's contact patch slides over the road, along the tyre
//      (slip ratio: wheelspin or locked brakes) and across it (slip angle).
//   2. The tyre force from the "Magic Formula", the curve real tyre models
//      use. Grip rises with slip to a peak, then falls away as it slides.
//      Grip is shared: a tyre that's braking hard can't also corner hard.
//   3. How much weight is on the tyre, which moves to the back under
//      acceleration, the front under braking, and the outside in corners.
//      That's what makes the car understeer and oversteer.
// The engine has a torque curve and five gears, turning the driven wheels
// through a limited-slip differential. Drag is worked out per car, so every
// car still reaches the top speed set in the garage.
//
// Numbers are per kilogram of car (forces are accelerations), so a toy car
// and a 100 ft monster drive by the same rules.
import { CarPhysics, GEAR_TOPS, MAX_REVERSE, type CarDynamics, type CarInput } from '../../game/carPhysics';
import type { World } from '../../game/world';

/** How much driving help a car gets. Rivals always get full help. */
export type Assists = 'full' | 'some' | 'off';

export interface ModelOptions {
  /** Tyre grip: 0.95 is a road tyre, 1.2 sporty, 1.45 a racing slick. */
  grip: number;
  /** The player's driving aids. */
  assists: Assists;
  /** The player's car, which uses `assists` (everyone else gets full help). */
  player: CarPhysics;
}

/** The Magic Formula's shape (B stiffness, C shape, E curvature), for slip normalised so the peak is at 1. */
const B = 10;
const C = 1.9;
const E = 0.97;
/** Where the curve peaks (slip ratio or tan of slip angle), and its height there. */
const PEAK_SLIP = 0.11;
const PEAK = magic(PEAK_SLIP);

/**
 * The weight the tyres feel, m/s². Low-gravity cars (the moon buggy) keep
 * Earth grip, or they could hardly pull away; they still fly further.
 */
function tyreGravity(car: CarPhysics): number {
  return Math.max(car.gravity, 9.81);
}

/** y = sin(C·atan(Bx − E(Bx − atan Bx))): tyre force (as a share of the most it can give) for slip x. */
function magic(x: number): number {
  const bx = B * x;
  return Math.sin(C * Math.atan(bx - E * (bx - Math.atan(bx))));
}

/** The four wheels, in order: front left, front right, rear left, rear right. */
const FRONT = [true, true, false, false];
const LEFT = [true, false, true, false];

/** What each car keeps between steps. */
export interface CarState {
  /** Turning speed, rad/s (positive is left). */
  yawRate: number;
  /** Each wheel's spin, rad/s. */
  spin: number[];
  /** Slip angles with a little lag (tyres take a moment to build up sideways force). */
  slipAngle: number[];
  /** Accelerations, smoothed like a car's suspension settling: forward and left, m/s². */
  ax: number;
  ay: number;
  // For the telemetry panel: how hard each tyre is working (0..1+, over 1 is sliding), and the aids at work.
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
    load: [0, 0, 0, 0], usage: [0, 0, 0, 0], slipRatio: [0, 0, 0, 0],
    tractionControl: false, abs: false, stability: false, rpm: 0,
  };
}

export function stateOf(car: CarPhysics): CarState {
  if (!car.dynamicsState) car.dynamicsState = newState();
  return car.dynamicsState as CarState;
}

/** The car's fixed numbers, worked out from its garage settings and size. */
interface Specs {
  /** Wheel radius, m. */
  radius: number;
  /** Front and rear axle distance from the centre, and half the distance between left and right wheels, m. */
  front: number;
  rear: number;
  halfTrack: number;
  /** Centre of mass height, m. */
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
  /** Most braking per kg (as a force at the tyres), m/s². */
  brake: number;
  /** Driver's steering lock, radians. */
  maxSteer: number;
}

const ROLLING = 0.013; // rolling resistance on tarmac (Bosch)
const GRASS_ROLLING = 0.09;
const DRIVELINE = 0.88; // share of engine power that reaches the wheels

function specsOf(car: CarPhysics): Specs {
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
  const frontDrive = body === 'hatch' ? 1 : body === 'pickup' || body === 'van' || car.allTerrain ? 0.4 : 0;
  return {
    radius,
    front: half,
    rear: half,
    halfTrack: (car.width / 2) * 0.86,
    comHeight: car.height * 0.38,
    yawInertia: (car.length ** 2 + car.width ** 2) / 12,
    wheelInertia: 0.004 * radius * radius * 8,
    power,
    drag,
    frontDrive,
    brake: car.brakeDecel * 1.4,
    maxSteer: car.maxSteer,
  };
}

/** The realistic model, ready to put in CarPhysics.dynamics. */
export class RealisticDynamics implements CarDynamics {
  private specs = new WeakMap<CarPhysics, { key: string; specs: Specs }>();

  constructor(public options: ModelOptions) {}

  private specsFor(car: CarPhysics): Specs {
    // Worked out again when the car changes (a new build or size).
    const key = `${car.accel}/${car.topSpeed}/${car.length}/${car.width}/${car.height}/${car.wheelBase}/${car.brakeDecel}/${car.maxSteer}/${car.config?.body}/${car.allTerrain}/${car.config?.wheelSize}/${car.gravity}`;
    const saved = this.specs.get(car);
    if (saved && saved.key === key) return saved.specs;
    const specs = specsOf(car);
    this.specs.set(car, { key, specs });
    return specs;
  }

  /** Peak tyre grip at this speed and surface, m/s² (the same sum the tyres use, without weight transfer). */
  cornerGrip(car: CarPhysics, speed: number, surfaceGrip: number): number {
    const mu = this.options.grip * car.gripMultiplier * surfaceGrip;
    // Downforce: the same as the arcade model, doubling the grip at 10/10 and 100 mph.
    const down = 1 + (car.downforce / 10) * (speed / 44.7) ** 2;
    // A little under the limit: weight transfer costs some grip in a real corner.
    return mu * tyreGravity(car) * down * 0.92;
  }

  reset(car: CarPhysics): void {
    car.dynamicsState = newState();
  }

  /** A crash: a blow off the car's centre spins it. */
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
  }

  drive(car: CarPhysics, dt: number, input: CarInput, world: World): number {
    // Two smaller steps: the tyres respond quickly, so this keeps them steady.
    const steps = 2;
    for (let i = 0; i < steps; i++) this.step(car, dt / steps, input, world);
    return stateOf(car).yawRate;
  }

  private step(car: CarPhysics, dt: number, input: CarInput, world: World): void {
    const s = stateOf(car);
    const sp = this.specsFor(car);
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
    const delta = car.steerAngle;

    if (car.airborne) {
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
      car.revs = u < 0 ? -u / MAX_REVERSE : u / (GEAR_TOPS[car.gear - 1] * car.topSpeed);
      s.rpm = Math.min(1.2, Math.abs(car.revs));
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
      this.setVelocity(car, u, v, fx, fz, dt);
      return;
    }

    // ---------- the gearbox and engine ----------
    // Gears are set by speed, the same steps as the arcade model, so the rev
    // counter and shift light work the same way.
    const driven = [0, 1, 2, 3].filter((i) => (FRONT[i] ? sp.frontDrive > 0 : sp.frontDrive < 1));
    const wheelSpeed = (driven.reduce((sum, i) => sum + s.spin[i], 0) / driven.length) * sp.radius;
    if (!car.manual) {
      // Changes up at the redline: by wheel speed (engine revs), so a spinning wheel changes up too.
      // (Just before the rev limiter, which cuts in at the redline.)
      const shiftSpeed = Math.max(Math.abs(u), Math.abs(wheelSpeed)) * 1.03;
      car.gear = 1;
      while (car.gear < GEAR_TOPS.length && shiftSpeed > GEAR_TOPS[car.gear - 1] * car.topSpeed) car.gear++;
    }
    const reversing = input.brake > 0 && u < 0.5 && input.throttle === 0;
    const gearTop = GEAR_TOPS[car.gear - 1] * car.topSpeed; // road speed at the redline in this gear
    const boost = car.useBoost(dt, input, u);
    // Nitro lets the engine rev past the redline (×1.35), like the arcade model's higher top speed.
    const redlineSpeed = gearTop * (boost > 0 ? 1 + 0.35 * boost : 1);
    const rpm = Math.max(0.12, Math.abs(wheelSpeed) / gearTop); // the clutch slips below 12%
    s.rpm = Math.min(1.2, Math.abs(wheelSpeed) / gearTop);
    car.revs = reversing ? Math.abs(u) / 12 : Math.abs(wheelSpeed) / gearTop;
    car.limiter = car.manual && car.revs >= 1 && input.throttle > 0;
    // Torque curve: strong in the middle of the revs, fading near idle and the redline.
    const torqueShape = Math.max(0.35, Math.min(1, 0.55 + 1.2 * rpm - 0.75 * rpm * rpm));
    // Drive force at the wheels per kg: power / speed at the redline, times the curve.
    const maxForce = (sp.power * DRIVELINE) / Math.max(4, gearTop) / 0.95;
    let throttle = input.throttle;
    if (Math.abs(wheelSpeed) >= redlineSpeed && !reversing) throttle = 0; // rev limiter
    if (car.manual && car.gear > 1 && car.revs < 0.53) throttle *= 0.12 + 0.88 * (car.revs / 0.53) ** 2; // bogging down
    let driveForce = throttle * maxForce * torqueShape * (1 + 0.6 * boost);
    if (boost > 0 && throttle === 0) driveForce = maxForce * 0.6 * boost; // nitro pushes even off the throttle
    if (reversing) driveForce = -input.brake * maxForce * 0.6 * (Math.abs(u) < 12 ? 1 : 0);
    // Throttle while rolling backwards brakes instead.
    const throttleBrakes = input.throttle > 0 && u < -0.5;
    if (throttleBrakes) driveForce = 0;

    // ---------- weight on each wheel ----------
    const down = 1 + (car.downforce / 10) * (u / 44.7) ** 2;
    const g = tyreGravity(car) * down;
    const wheelBase = sp.front + sp.rear;
    // Weight moves back under acceleration, forward under braking, outwards in corners.
    const longShift = (s.ax * sp.comHeight) / wheelBase; // per kg, to the rear
    const latShift = (s.ay * sp.comHeight) / (2 * sp.halfTrack); // per kg, to the right (outside of a left turn)
    const loads = [0, 0, 0, 0];
    for (let i = 0; i < 4; i++) {
      const axle = (FRONT[i] ? sp.rear : sp.front) / wheelBase; // static share on this axle
      let n = (g * axle) / 2 + (FRONT[i] ? -longShift : longShift) / 2;
      // The front takes more of the cornering weight shift (a stiffer front
      // anti-roll bar), which makes the car understeer gently at the limit
      // rather than spin, as real road cars are set up to.
      const share = FRONT[i] ? 0.62 : 0.38;
      n += (LEFT[i] ? -latShift : latShift) * share;
      loads[i] = Math.max(0, n);
    }

    // ---------- surface ----------
    const grassy = car.onGrass;
    let mu = this.options.grip * car.gripMultiplier * car.surfaceGrip;
    // Aquaplaning: standing water takes grip away at high speed, in the rain.
    if (world.theme.weather === 'rain' && world.map !== 'city') mu *= Math.max(0.55, 1 - 0.35 * Math.max(0, (speed - 25) / 40) ** 2);
    const drift = car.config?.tuning.drift ?? 0;
    // Wider rear tyres (another understeer setting), less so on drift builds.
    const rearGrip = 1.1 - drift * 0.02;

    // ---------- each tyre ----------
    let fxSum = 0; // forward force per kg
    let fySum = 0; // left force per kg
    let moment = 0; // turning
    const handbrake = input.handbrake;
    const braking = input.brake > 0 && u > 0.5;
    const throttleBrake = throttleBrakes ? input.throttle : 0;
    car.braking = braking || throttleBrakes || handbrake;
    s.tractionControl = false;
    s.abs = false;
    const floor = Math.max(0.5, sp.radius * 3); // slower than this, slip is measured against a floor (no divide by zero)
    for (let i = 0; i < 4; i++) {
      const x = FRONT[i] ? sp.front : -sp.rear;
      const y = LEFT[i] ? sp.halfTrack : -sp.halfTrack;
      // The contact patch's velocity, then turned into the wheel's own directions.
      const pu = u - r * y;
      const pv = v + r * x;
      const steer = FRONT[i] ? delta : 0;
      const cos = Math.cos(steer);
      const sin = Math.sin(steer);
      const wu = pu * cos + pv * sin; // along the tyre
      const wv = -pu * sin + pv * cos; // across it
      const ref = Math.max(Math.abs(wu), floor);

      // Slip angle, built up over about a tyre's width of rolling (relaxation length).
      const angleTarget = Math.atan2(wv, ref);
      const relax = Math.min(1, (Math.max(Math.abs(wu), 2) * dt) / Math.max(0.2, sp.radius));
      s.slipAngle[i] += (angleTarget - s.slipAngle[i]) * relax;

      const n = loads[i];
      // Load sensitivity: a heavily loaded tyre grips a bit less per kg.
      const static_ = g / 4;
      const sensitivity = Math.max(0.7, 1 - 0.12 * (n / Math.max(1e-6, static_) - 1));
      const peakForce = mu * n * sensitivity * (grassy ? 0.8 : 1) * (FRONT[i] ? 1 : rearGrip);

      // Torque on this wheel from the engine (through the differential) and the brakes.
      let drive = 0;
      if (driven.includes(i)) {
        const axleShare = FRONT[i] ? sp.frontDrive : 1 - sp.frontDrive;
        drive = (driveForce * axleShare) / 2;
      }
      let brake = 0;
      // 60% of the braking on the front wheels, 40% on the back.
      if (braking) brake = sp.brake * input.brake * (FRONT[i] ? 0.3 : 0.2);
      if (throttleBrakes) brake = sp.brake * throttleBrake * (FRONT[i] ? 0.3 : 0.2);
      if (handbrake && !FRONT[i]) brake = Math.max(brake, sp.brake * 0.6);

      // The tyre force for a given wheel spin, as a function, so the wheel can
      // be stepped implicitly (stable however stiff the tyre is).
      const angle = s.slipAngle[i];
      const tyre = (spin: number) => {
        const ratio = Math.max(-1.5, Math.min(1.5, (spin * sp.radius - wu) / ref));
        const sx = ratio / PEAK_SLIP;
        const sy = Math.tan(Math.max(-1.4, Math.min(1.4, angle))) / PEAK_SLIP;
        const combined = Math.hypot(sx, sy);
        if (combined < 1e-9) return { fx: 0, fy: 0, ratio, usage: 0 };
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

      // Wheel spin: inertia × d(spin)/dt = (drive − tyre force) × radius, minus brakes.
      const inertia = sp.wheelInertia;
      const h = 1e-3;
      const slope = (tyre(s.spin[i] + h).fx - now.fx) / h; // how the tyre force changes with spin
      const free = s.spin[i] + (dt * (drive - now.fx) * sp.radius) / inertia;
      // Implicit step: allows for the tyre force growing as the spin changes.
      let spin = s.spin[i] + (free - s.spin[i]) / (1 + (dt * sp.radius * Math.max(0, slope)) / inertia);
      // Brakes slow the wheel towards a standstill, but never spin it backwards.
      if (brake > 0) {
        const slowing = (dt * brake * sp.radius) / inertia;
        spin = Math.abs(spin) <= slowing ? 0 : spin - Math.sign(spin) * slowing;
      }
      s.spin[i] = spin;
      const out = tyre(spin);
      s.slipRatio[i] = out.ratio;
      s.usage[i] = out.usage;
      s.load[i] = n / Math.max(1e-6, static_);

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
      const limit = (mu * g) / Math.max(u, 1);
      const target = Math.max(-limit, Math.min(limit, wanted));
      const spinning = Math.abs(r) > Math.abs(target) + 0.1 * (1.6 - help) || (Math.sign(r) !== Math.sign(target) && Math.abs(r) > 0.15);
      if (spinning && Math.abs(sideslip) > 0.06) {
        moment -= (r - target) * sp.yawInertia * 5 * help;
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
    // The suspension settles over about a tenth of a second, which smooths the weight transfer.
    const settle = Math.min(1, dt * 10);
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
