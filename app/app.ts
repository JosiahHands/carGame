// Game entry point: sets up the 3D scene, the HUD and the game loop.
import * as THREE from 'three';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { loadConfig, saveConfig, type CarConfig } from '../game/carConfig';
import { buildCar, type CarRig } from '../game/carModel';
import { CarPhysics } from '../game/carPhysics';
import { CameraRig } from '../game/cameraRig';
import { Effects } from '../game/effects';
import { EngineAudio } from '../game/engineAudio';
import { GarageUI } from '../game/garageUI';
import { Input, type Action } from '../game/input';
import { Minimap } from '../game/minimap';
import { EFFECTS_AMOUNT, SHADOW_SIZES, loadSettings, saveSettings } from '../game/settings';
import { TimeTrial, formatTime } from '../game/timeTrial';
import { Weapons } from '../game/weapons';
import { World } from '../game/world';

const STEP = 1 / 120; // physics runs at a fixed 120 steps per second
const SLOW_MOTION = 0.3; // speed of time in slow motion

const settings = loadSettings();

// ---------- renderer, scene, camera ----------
const container = document.getElementById('game')!;
// Anti-aliasing can only be chosen when the renderer is made, which is why
// that setting only changes when the game starts.
const renderer = new THREE.WebGLRenderer({ antialias: settings.antialias, powerPreference: 'high-performance' });
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = THREE.PCFShadowMap;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.outputColorSpace = THREE.SRGBColorSpace;
container.append(renderer.domElement);

const scene = new THREE.Scene();
const pmrem = new THREE.PMREMGenerator(renderer);
scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;

const camera = new THREE.PerspectiveCamera(62, window.innerWidth / window.innerHeight, 0.1, 1200);
const cameraRig = new CameraRig(camera, renderer.domElement);
const effects = new Effects(scene);
const world = new World(scene, effects);
const physics = new CarPhysics();
const input = new Input(renderer.domElement);
const audio = new EngineAudio();
const weapons = new Weapons(scene, world, effects, audio);
const trial = new TimeTrial(scene);
const minimap = new Minimap();
document.body.append(minimap.canvas);

// ---------- the player's car ----------
const config: CarConfig = loadConfig();
let night = false;
let car: CarRig = buildCar(config, night);
scene.add(car.root);
physics.configure(config, car);
physics.reset(world.spawn.x, world.spawn.z, world.spawn.heading);
cameraRig.eyeHeight = car.eyeHeight;

function rebuildCar(): void {
  car.dispose();
  car = buildCar(config, night);
  scene.add(car.root);
  physics.configure(config, car);
  cameraRig.eyeHeight = car.eyeHeight;
  syncCar(0);
}

// ---------- HUD ----------
const $ = (id: string) => document.getElementById(id)!;
const hud = $('hud');
const speedEl = $('speed');
const gearEl = $('gear');
const driftEl = $('drift');
const bestEl = $('best');
const nitroBar = $('nitro-fill');
const helpEl = $('help');
const hintEl = $('hint');
const toastEl = $('toast');
const fpsEl = $('fps');
const weaponPanel = $('weapon');
const weaponName = $('weapon-name');
const weaponBar = $('weapon-fill');
const scoreEl = $('score');
const trialEl = $('trial');
const countdownEl = $('countdown');
const slowMotionEl = $('slowmo');
const boostGlow = $('boostfx');
const photoBar = $('photo-bar');
let toastTimer = 0;

function toast(text: string, ms = 1600): void {
  toastEl.textContent = text;
  toastEl.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toastEl.classList.remove('show'), ms);
}

let bestDrift = 0;
try {
  bestDrift = Number(localStorage.getItem('carGame.bestDrift')) || 0;
} catch {
  // ignore
}
function showBest(): void {
  const parts = [];
  if (bestDrift > 0) parts.push(`Best drift ${bestDrift.toLocaleString()}`);
  if (trial.best > 0) parts.push(`Best lap ${formatTime(trial.best)}`);
  bestEl.textContent = parts.join(' · ');
}
showBest();

// ---------- destruction score ----------
// What all the damage would cost, in dollars. Destroying things also refills nitro.
let damage = 0;

world.onDestroyed = (target) => {
  damage += target.value;
  scoreEl.textContent = `$${damage.toLocaleString()} damage`;
  const distance = Math.hypot(target.x - physics.x, target.z - physics.z);
  if (target.kind === 'building') {
    physics.nitro = Math.min(1, physics.nitro + 0.25);
    audio.explosion(2.5, distance);
    shake = Math.max(shake, Math.min(0.5, 12 / (distance + 10)));
    toast(`Building destroyed +$${target.value.toLocaleString()}`);
  } else if (target.kind === 'car') {
    physics.nitro = Math.min(1, physics.nitro + 0.1);
    audio.explosion(1.4, distance);
    toast(`Car destroyed +$${target.value.toLocaleString()}`);
  } else {
    physics.nitro = Math.min(1, physics.nitro + 0.05);
    audio.crash(6);
  }
};

// ---------- settings ----------
function applySettings(): void {
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2) * settings.resolution);
  renderer.setSize(window.innerWidth, window.innerHeight);
  if (settings.shadows === 'off') {
    world.setShadows(false, SHADOW_SIZES.low.mapSize, SHADOW_SIZES.low.area);
  } else {
    world.setShadows(true, SHADOW_SIZES[settings.shadows].mapSize, SHADOW_SIZES[settings.shadows].area);
  }
  world.setViewDistance(settings.viewDistance);
  camera.far = settings.viewDistance + 200; // a bit past the fog, for the hills on the horizon
  camera.updateProjectionMatrix();
  cameraRig.baseFov = settings.fov;
  effects.amount = EFFECTS_AMOUNT[settings.effects];
  updateHud();
}

// ---------- garage, driving and photo modes ----------
let mode: 'garage' | 'drive' | 'photo' = 'garage';
let slowMotion = false;

const garage = new GarageUI(config, settings, {
  carChanged(change) {
    if (change === 'look') rebuildCar();
    else physics.configure(config, car);
    // Save right away: a delayed save is lost if the window closes first.
    saveConfig(config);
  },
  settingsChanged() {
    saveSettings(settings);
    applySettings();
  },
  rebuildCity() {
    world.rebuildAll();
    damage = 0;
    scoreEl.textContent = '';
    toast('The city has been rebuilt');
  },
  drive: () => setMode('drive'),
});
document.body.append(garage.root);

function carPosition(): THREE.Vector3 {
  return new THREE.Vector3(physics.x, physics.y, physics.z);
}

/** Shows the parts of the HUD that belong to the current mode and settings. */
function updateHud(): void {
  const driving = mode === 'drive';
  hud.classList.toggle('hidden', !driving);
  hintEl.classList.toggle('hidden', !driving);
  scoreEl.classList.toggle('hidden', !driving);
  weaponPanel.classList.toggle('hidden', !driving || !settings.weapons);
  minimap.setVisible(driving && settings.minimap);
  trialEl.classList.toggle('hidden', !driving || !trial.active);
  slowMotionEl.classList.toggle('hidden', !driving || !slowMotion);
  photoBar.classList.toggle('hidden', mode !== 'photo');
  fpsEl.classList.toggle('hidden', !settings.showFps);
  if (!driving) helpEl.classList.add('hidden');
}

function setMode(next: 'garage' | 'drive' | 'photo'): void {
  const previous = mode;
  mode = next;
  if (next === 'garage') {
    // Park the car where it is, and give up any time trial.
    physics.vx = physics.vz = physics.vy = 0;
    physics.y = world.heightAt(physics.x, physics.z);
    physics.airborne = false;
    trial.stop();
  }
  garage.setVisible(next === 'garage');
  // The garage and photo mode both use the orbit camera; driving follows the car.
  if (next === 'garage') cameraRig.setGarage(true, carPosition(), physics.heading);
  if (next === 'photo') cameraRig.setGarage(true, carPosition(), physics.heading, true);
  if (next === 'drive' && previous !== 'drive') cameraRig.setGarage(false, carPosition(), physics.heading);
  updateViewOffset();
  updateHud();
  (document.activeElement as HTMLElement | null)?.blur();
}

/** In the garage, shift the view so the car sits left of the panel. */
function updateViewOffset(): void {
  const w = window.innerWidth;
  const h = window.innerHeight;
  if (mode === 'garage' && w > 820) camera.setViewOffset(w, h, Math.min(380, w * 0.3) / 2, 0, w, h);
  else camera.clearViewOffset();
}

/** Saves the picture on screen (without the HUD, which is HTML) as a PNG download. */
function savePhoto(): void {
  // Draw a fresh frame first: the canvas is cleared after each frame is shown.
  renderer.render(scene, camera);
  const link = document.createElement('a');
  const stamp = new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '-');
  link.href = renderer.domElement.toDataURL('image/png');
  link.download = `car-game-${stamp}.png`;
  link.click();
  toast('Photo saved');
}
$('photo-save').addEventListener('click', savePhoto);

/** Puts the car at a spot, standing still, with the camera behind it. */
function placeCar(x: number, z: number, heading: number): void {
  physics.reset(x, z, heading);
  weapons.clear();
  cameraRig.snap(carPosition(), physics.heading);
  driftScore = 0;
}

function startTimeTrial(): void {
  placeCar(trial.start.x, trial.start.z, trial.start.heading);
  trial.begin();
  updateHud();
}

function handleAction(action: Action): void {
  // Keys that work in every mode.
  if (action === 'night') {
    night = !night;
    world.setNight(night);
    scene.environmentIntensity = night ? 0.2 : 1;
    toast(night ? 'Night' : 'Day');
  } else if (action === 'mute') {
    toast(audio.toggleMute() ? 'Sound off' : 'Sound on');
  } else if (action === 'fullscreen') {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen().catch(() => toast('Fullscreen not available'));
  }

  // Garage: G or Esc goes back to driving.
  if (mode === 'garage') {
    if (action === 'garage' || action === 'settings') setMode('drive');
    return;
  }

  // Photo mode: Enter saves a photo; P or Esc goes back to driving.
  if (mode === 'photo') {
    if (action === 'snapshot') savePhoto();
    if (action === 'photo' || action === 'settings') setMode('drive');
    return;
  }

  // Driving.
  switch (action) {
    case 'garage':
      setMode('garage');
      break;
    case 'settings':
      setMode('garage');
      garage.show('Settings');
      break;
    case 'camera': {
      const names = { chase: 'Chase camera', far: 'Far camera', hood: 'Bonnet camera' };
      toast(names[cameraRig.nextMode()]);
      break;
    }
    case 'reset':
      if (trial.active) startTimeTrial(); // restart the lap
      else placeCar(world.spawn.x, world.spawn.z, world.spawn.heading);
      break;
    case 'help':
      helpEl.classList.toggle('hidden');
      break;
    case 'photo':
      setMode('photo');
      toast('Photo mode: drag to look around, Enter to save');
      break;
    case 'slowmo':
      slowMotion = !slowMotion;
      updateHud();
      break;
    case 'trial':
      if (trial.active) {
        trial.stop();
        toast('Time trial cancelled');
        updateHud();
      } else {
        startTimeTrial();
      }
      break;
    case 'nextWeapon':
    case 'weapon1':
    case 'weapon2':
    case 'weapon3':
      if (!settings.weapons) break;
      if (action === 'nextWeapon') weapons.select(weapons.selected + 1);
      else weapons.select(Number(action.slice(-1)) - 1); // 'weapon2' -> 1
      weaponName.textContent = `${weapons.selected + 1} · ${weapons.name}`;
      toast(weapons.name);
      break;
  }
}

// ---------- per-frame updates ----------
function syncCar(dt: number): void {
  car.root.position.set(physics.x, physics.y, physics.z);
  car.root.rotation.set(physics.pitch, physics.heading, 0, 'YXZ');
  car.body.rotation.set(physics.bodyPitch, 0, physics.roll);
  for (const w of car.wheels) {
    w.spin.rotation.x += (physics.forwardSpeed / w.radius) * dt;
    if (w.front) w.steer.rotation.y = physics.steerAngle;
  }
  car.setLights({ night, braking: physics.braking && mode === 'drive' });
  car.setNitro(physics.boosting && mode === 'drive');
}

let driftScore = 0;
let driftTimer = 0;
let airTime = 0;
let shake = 0;

function bankDrift(): void {
  const points = Math.round(driftScore);
  driftScore = 0;
  if (points < 50) return;
  if (points > bestDrift) {
    bestDrift = points;
    try {
      localStorage.setItem('carGame.bestDrift', String(points));
    } catch {
      // ignore
    }
    toast(`New best drift: ${points.toLocaleString()}!`);
    showBest();
  } else {
    toast(`Drift +${points.toLocaleString()}`);
  }
}

/** Runs the car's physics for `dt` seconds, in fixed steps, and reacts to crashes and landings. */
function drive(dt: number, controls: ReturnType<Input['read']>): void {
  accumulator += dt;
  let steps = 0;
  while (accumulator >= STEP && steps < 10) {
    const wasAirborne = physics.airborne;
    const ev = physics.step(STEP, controls, world);
    accumulator -= STEP;
    steps++;
    if (physics.airborne) airTime += STEP;
    else if (wasAirborne) {
      if (airTime > 0.7) {
        toast(`Air time ${airTime.toFixed(1)} s`);
        physics.nitro = Math.min(1, physics.nitro + airTime * 0.08); // jumps fill the nitro
      }
      airTime = 0;
    }
    if (ev.impact > 3) {
      audio.crash(ev.impact);
      shake = Math.min(0.6, ev.impact * 0.03);
      if (driftScore > 0) {
        driftScore = 0;
        toast('Crashed! Drift lost');
      }
      // Ramming a parked car hard enough wrecks it.
      if (ev.hitTarget && ev.impact > 6) {
        world.damage(ev.hitTarget, ev.impact * 5, new THREE.Vector3(physics.vx, 0, physics.vz).normalize());
      }
    }
    if (ev.smashed > 0) shake = Math.max(shake, 0.15);
    if (ev.cones > 0) audio.crash(5);
    if (ev.landed > 6) audio.crash(ev.landed * 0.6);
  }
  if (accumulator > STEP) accumulator = 0; // fell behind: drop the backlog

  // Drift scoring: points for sliding sideways at speed.
  const speed = physics.speed;
  const slipAngle = Math.atan2(Math.abs(physics.lateralSpeed), Math.abs(physics.forwardSpeed));
  const drifting = !physics.airborne && speed > 8 && slipAngle > 0.2 && physics.forwardSpeed > 0;
  if (drifting) {
    driftScore += speed * slipAngle * dt * 10;
    driftTimer = 0.8;
    physics.nitro = Math.min(1, physics.nitro + dt * 0.06); // drifting fills the nitro
  } else if (driftTimer > 0) {
    driftTimer -= dt;
    if (driftTimer <= 0) bankDrift();
  }
  // The nitro bottle also slowly refills on its own.
  if (!physics.boosting) physics.nitro = Math.min(1, physics.nitro + dt * 0.03);
}

/** Fires weapons, and shakes and pushes the car when explosions go off near it. */
function useWeapons(dt: number, firing: boolean): void {
  const blasts = weapons.update(dt, firing, car, new THREE.Vector3(physics.vx, physics.vy, physics.vz));
  for (const blast of blasts) {
    const distance = blast.at.distanceTo(carPosition());
    audio.explosion(1.5, distance);
    shake = Math.max(shake, Math.min(0.7, 6 / (distance + 4)));
    // Close explosions throw the car up and away (rocket jumps!).
    const reach = blast.radius * 1.3;
    if (distance < reach) {
      const strength = 1 - distance / reach;
      const away = carPosition().sub(blast.at).setY(0).normalize();
      physics.push(away.x * 12 * strength, 7 * strength, away.z * 12 * strength);
    }
    if (blast.radius === 0) physics.push(0, 3, 0); // the shockwave makes the car hop
  }
}

function updateTimeTrial(dt: number): void {
  const event = trial.update(dt, physics.x, physics.y, physics.z);
  if (event === 'go') {
    toast('GO!', 900);
    audio.chime();
  } else if (event === 'checkpoint') {
    audio.chime();
  } else if (event === 'finish') {
    audio.chime(true);
    toast(trial.newBest ? `New best lap: ${formatTime(trial.time)}!` : `Lap ${formatTime(trial.time)}  (best ${formatTime(trial.best)})`, 4000);
    showBest();
    updateHud();
  }
  if (trial.active) {
    const best = trial.best ? `  ·  Best ${formatTime(trial.best)}` : '';
    trialEl.textContent = `Checkpoint ${trial.next + 1}/${trial.total}  ·  ${formatTime(trial.time)}${best}`;
  }
  countdownEl.textContent = trial.active && trial.countdown > 0 ? String(Math.ceil(trial.countdown)) : '';
}

let accumulator = 0;
let lastTime = performance.now();
let fpsFrames = 0;
let fpsTime = 0;

function frame(): void {
  requestAnimationFrame(frame);
  const now = performance.now();
  // Frame rate limit: skip this screen refresh if the next frame isn't due yet.
  if (settings.fpsLimit > 0 && now - lastTime < 1000 / settings.fpsLimit - 1.5) return;
  const realDt = Math.min(0.05, (now - lastTime) / 1000);
  lastTime = now;

  // Count frames for the FPS counter, updating it twice a second.
  fpsFrames++;
  fpsTime += realDt;
  if (fpsTime >= 0.5) {
    fpsEl.textContent = `${Math.round(fpsFrames / fpsTime)} FPS`;
    fpsFrames = 0;
    fpsTime = 0;
  }

  for (const action of input.takeActions()) handleAction(action);
  const controls = input.read();

  // Game time stops in the garage and photo mode, and runs slowly in slow motion.
  const driving = mode === 'drive';
  const dt = !driving ? 0 : slowMotion ? realDt * SLOW_MOTION : realDt;
  if (driving) {
    const waitingToStart = trial.active && trial.countdown > 0;
    if (!waitingToStart) drive(dt, controls);
    if (settings.weapons && !waitingToStart) useWeapons(dt, controls.fire);
    updateTimeTrial(dt);
  }

  syncCar(dt);
  effects.update(dt);
  world.update(dt, physics.x, physics.z);
  cameraRig.update(realDt, carPosition(), physics.heading, physics.speed, driving && physics.boosting);
  if (shake > 0.001 && driving) {
    camera.position.x += (Math.random() - 0.5) * shake;
    camera.position.y += (Math.random() - 0.5) * shake;
    shake *= Math.exp(-realDt * 8);
  }

  // HUD
  const slip = physics.airborne ? 0 : Math.min(1, Math.abs(physics.lateralSpeed) / 7);
  audio.update(physics.speedFraction, driving ? controls.throttle : 0, driving ? slip : 0, driving);
  speedEl.textContent = String(Math.round(physics.speed * 3.6));
  gearEl.textContent = physics.forwardSpeed < -0.5 ? 'R' : String(audio.gear);
  driftEl.textContent = driftScore >= 10 ? `DRIFT ${Math.round(driftScore).toLocaleString()}` : '';
  nitroBar.style.width = `${Math.round(physics.nitro * 100)}%`;
  nitroBar.classList.toggle('active', physics.boosting);
  boostGlow.classList.toggle('on', driving && physics.boosting);
  weaponBar.style.width = `${Math.round(weapons.readiness * 100)}%`;
  weaponBar.classList.toggle('hot', weapons.overheated);
  if (driving && settings.minimap) minimap.update(realDt, world, physics.x, physics.z, physics.heading, trial.mapMarkers());

  renderer.render(scene, camera);
}

// ---------- startup ----------
window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(window.innerWidth, window.innerHeight);
  updateViewOffset();
});
// Sound can only start after the player presses something.
for (const type of ['keydown', 'pointerdown'] as const) window.addEventListener(type, () => audio.unlock());

weaponName.textContent = `1 · ${weapons.name}`;
applySettings();
syncCar(0);
setMode('garage');
requestAnimationFrame(frame);
