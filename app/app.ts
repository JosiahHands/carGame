// Game entry point: sets up the 3D scene and runs the game loop.
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
import { EFFECTS_SCALE, SHADOW_SIZES, loadSettings, saveSettings, type Settings } from '../game/settings';
import { SettingsUI } from '../game/settingsUI';
import { TimeTrial, formatTime } from '../game/timeTrial';
import { WEAPONS, Weapons } from '../game/weapons';
import { World, type Target } from '../game/world';

const STEP = 1 / 120; // physics runs at a fixed 120 steps per second
const SLOW_MOTION = 0.3;

const settings: Settings = loadSettings();

// ---------- renderer, scene, camera ----------
const container = document.getElementById('game')!;
// Anti-aliasing can only be chosen when the renderer is made, so that setting
// takes effect the next time the game starts.
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
const nitroFill = $('nitro-fill');
const helpEl = $('help');
const hintEl = $('hint');
const toastEl = $('toast');
const fpsEl = $('fps');
const weaponEl = $('weapon');
const weaponName = $('weapon-name');
const weaponFill = $('weapon-fill');
const scoreEl = $('score');
const trialEl = $('trial');
const countdownEl = $('countdown');
const slowmoEl = $('slowmo');
const boostFx = $('boostfx');
const photoBar = $('photo-bar');
let toastTimer = 0;

function toast(text: string, ms = 1600): void {
  toastEl.textContent = text;
  toastEl.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toastEl.classList.remove('show'), ms);
}

function readNumber(key: string): number {
  try {
    return Number(localStorage.getItem(key)) || 0;
  } catch {
    return 0;
  }
}

function writeNumber(key: string, value: number): void {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // ignore
  }
}

let bestDrift = readNumber('carGame.bestDrift');
function showBest(): void {
  const parts = [];
  if (bestDrift > 0) parts.push(`Best drift ${bestDrift.toLocaleString()}`);
  if (trial.best > 0) parts.push(`Best lap ${formatTime(trial.best)}`);
  bestEl.textContent = parts.join(' · ');
}
showBest();

// Destruction score: what the damage would cost, in dollars.
let damageTotal = 0;
let bestDamage = readNumber('carGame.bestDamage');
function showScore(): void {
  scoreEl.textContent = damageTotal > 0 ? `$${damageTotal.toLocaleString()} damage` : '';
  scoreEl.title = bestDamage > 0 ? `Record: $${bestDamage.toLocaleString()}` : '';
}

const DESTROYED_NAMES: Record<Target['kind'], string> = { building: 'Building', car: 'Car', lamp: 'Lamp post', tree: 'Tree' };
let combo = 0;
let comboTimer = 0;

world.onDestroyed = (t) => {
  damageTotal += t.value;
  combo++;
  comboTimer = 2.5;
  if (damageTotal > bestDamage) {
    bestDamage = damageTotal;
    writeNumber('carGame.bestDamage', bestDamage);
  }
  showScore();
  physics.nitro = Math.min(1, physics.nitro + (t.kind === 'building' ? 0.25 : 0.05));
  const distance = Math.hypot(t.x - physics.x, t.z - physics.z);
  if (t.kind === 'building') {
    audio.explosion(2.5, distance);
    shake = Math.max(shake, Math.min(0.5, 12 / (distance + 10)));
  } else if (t.kind === 'car') {
    audio.explosion(1.4, distance);
  } else {
    audio.crash(6);
  }
  if (t.kind === 'building' || t.kind === 'car' || combo > 1) {
    const name = DESTROYED_NAMES[t.kind];
    toast(combo > 1 ? `${combo}× combo! ${name} destroyed +$${t.value.toLocaleString()}` : `${name} destroyed +$${t.value.toLocaleString()}`);
  }
};

// ---------- weapons ----------
const weapons = new Weapons(scene, world, effects, {
  shot: () => audio.gunshot(),
  launch: () => audio.rocketLaunch(),
  explosion: (at, radius) => {
    const d = Math.hypot(at.x - physics.x, at.y - physics.y, at.z - physics.z);
    audio.explosion(1.5, d);
    shake = Math.max(shake, Math.min(0.7, 6 / (d + 4)));
    // Close explosions throw the car about.
    const reach = radius * 1.3;
    if (d < reach && mode === 'drive') {
      const k = 1 - d / reach;
      const dx = (physics.x - at.x) / (d || 1);
      const dz = (physics.z - at.z) / (d || 1);
      physics.push(dx * 12 * k, 7 * k, dz * 12 * k);
    }
  },
  shockwave: () => {
    audio.shockwave();
    shake = Math.max(shake, 0.5);
    physics.push(0, 3, 0);
  },
});

function showWeapon(): void {
  weaponName.textContent = `${weapons.index + 1} · ${weapons.name}`;
}
showWeapon();

// ---------- settings ----------
let drawingHeight = window.innerHeight;

function applySettings(): void {
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2) * settings.renderScale);
  renderer.setSize(window.innerWidth, window.innerHeight);
  if (settings.shadows === 'off') world.setShadows(false, 1024, 35);
  else world.setShadows(true, SHADOW_SIZES[settings.shadows].map, SHADOW_SIZES[settings.shadows].extent);
  world.setViewDistance(settings.viewDistance);
  camera.far = settings.viewDistance + 200;
  camera.updateProjectionMatrix();
  cameraRig.baseFov = settings.fov;
  effects.amount = EFFECTS_SCALE[settings.effects];
  drawingHeight = renderer.getDrawingBufferSize(new THREE.Vector2()).y;
  updateHudVisibility();
}

const settingsUI = new SettingsUI(
  settings,
  (restartNeeded) => {
    saveSettings(settings);
    applySettings();
    if (restartNeeded) toast('Anti-aliasing changes next time you start the game', 2600);
  },
  () => closeSettings(),
  () => {
    world.rebuildAll();
    damageTotal = 0;
    showScore();
    toast('The city has been rebuilt');
  },
);
document.body.append(settingsUI.root);

function openSettings(): void {
  settingsUI.setVisible(true);
  garage.setVisible(false);
  helpEl.classList.add('hidden');
  updateHudVisibility();
}

function closeSettings(): void {
  settingsUI.setVisible(false);
  garage.setVisible(mode === 'garage');
  lastTime = performance.now(); // don't count the time spent in the menu
  updateHudVisibility();
  (document.activeElement as HTMLElement | null)?.blur();
}

// ---------- garage / drive / photo modes ----------
let mode: 'garage' | 'drive' | 'photo' = 'garage';

const garage = new GarageUI(
  config,
  (_c, change) => {
    if (change === 'look') rebuildCar();
    else physics.configure(config, car);
    // Save right away: a delayed save is lost if the window closes first.
    saveConfig(config);
  },
  () => setMode('drive'),
  () => openSettings(),
);
document.body.append(garage.root);

function carPosition(): THREE.Vector3 {
  return new THREE.Vector3(physics.x, physics.y, physics.z);
}

function updateHudVisibility(): void {
  const driving = mode === 'drive';
  const menu = settingsUI.visible;
  hud.classList.toggle('hidden', !driving || menu);
  hintEl.classList.toggle('hidden', !driving || menu);
  weaponEl.classList.toggle('hidden', !driving || menu || !settings.weapons);
  scoreEl.classList.toggle('hidden', !driving || menu);
  minimap.setVisible(driving && !menu && settings.minimap);
  fpsEl.classList.toggle('hidden', !settings.showFps);
  trialEl.classList.toggle('hidden', !driving || !trial.active);
  photoBar.classList.toggle('hidden', mode !== 'photo' || menu);
  slowmoEl.classList.toggle('hidden', !slowMotion || mode !== 'drive');
}

function setMode(next: 'garage' | 'drive' | 'photo'): void {
  const prev = mode;
  mode = next;
  if (next === 'garage') {
    // Park the car where it is.
    physics.vx = physics.vz = physics.vy = 0;
    physics.y = world.heightAt(physics.x, physics.z);
    physics.airborne = false;
    if (trial.active) trial.stop();
    helpEl.classList.add('hidden');
  }
  if (next === 'photo') helpEl.classList.add('hidden');
  garage.setVisible(next === 'garage');
  if (next === 'garage') cameraRig.setGarage(true, carPosition(), physics.heading);
  else if (next === 'photo') cameraRig.setGarage(true, carPosition(), physics.heading, true);
  else if (prev !== 'drive') cameraRig.setGarage(false, carPosition(), physics.heading);
  lastTime = performance.now();
  updateViewOffset();
  updateHudVisibility();
  (document.activeElement as HTMLElement | null)?.blur();
}

/** In the garage, shift the view so the car sits left of the panel. */
function updateViewOffset(): void {
  const w = window.innerWidth;
  const h = window.innerHeight;
  if (mode === 'garage' && w > 820) camera.setViewOffset(w, h, Math.min(380, w * 0.3) / 2, 0, w, h);
  else camera.clearViewOffset();
}

/** Saves what is on screen as a PNG, without the HUD. */
function savePhoto(): void {
  renderer.render(scene, camera);
  const url = renderer.domElement.toDataURL('image/png');
  const a = document.createElement('a');
  const stamp = new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '-');
  a.href = url;
  a.download = `car-game-${stamp}.png`;
  a.click();
  toast('Photo saved');
}
$('photo-save').addEventListener('click', savePhoto);

function resetCar(): void {
  physics.reset(world.spawn.x, world.spawn.z, world.spawn.heading);
  weapons.clear();
  cameraRig.snap(carPosition(), physics.heading);
  driftScore = 0;
}

function startTrial(): void {
  physics.reset(trial.start.x, trial.start.z, trial.start.heading);
  weapons.clear();
  cameraRig.snap(carPosition(), physics.heading);
  driftScore = 0;
  trial.begin();
  updateHudVisibility();
}

let slowMotion = false;

function handleAction(action: Action): void {
  // Settings open: only Esc (close) does anything.
  if (settingsUI.visible) {
    if (action === 'settings') closeSettings();
    return;
  }
  switch (action) {
    case 'garage':
      if (mode !== 'photo') setMode(mode === 'garage' ? 'drive' : 'garage');
      break;
    case 'settings':
      if (mode === 'photo') setMode('drive');
      else openSettings();
      break;
    case 'camera':
      if (mode === 'drive') {
        const m = cameraRig.nextMode();
        toast({ chase: 'Chase camera', far: 'Far camera', hood: 'Bonnet camera' }[m]);
      }
      break;
    case 'night':
      night = !night;
      world.setNight(night);
      scene.environmentIntensity = night ? 0.2 : 1;
      toast(night ? 'Night' : 'Day');
      break;
    case 'mute':
      toast(audio.toggleMute() ? 'Sound off' : 'Sound on');
      break;
    case 'reset':
      if (mode === 'drive') {
        if (trial.active) startTrial();
        else resetCar();
      }
      break;
    case 'help':
      if (mode === 'drive') helpEl.classList.toggle('hidden');
      break;
    case 'fullscreen':
      if (document.fullscreenElement) void document.exitFullscreen();
      else void document.documentElement.requestFullscreen().catch(() => toast('Fullscreen not available'));
      break;
    case 'photo':
      if (mode === 'drive') {
        setMode('photo');
        toast('Photo mode: drag to look around, Enter to save');
      } else if (mode === 'photo') {
        setMode('drive');
      }
      break;
    case 'snapshot':
      if (mode === 'photo') savePhoto();
      break;
    case 'slowmo':
      if (mode === 'drive') {
        slowMotion = !slowMotion;
        audio.setTimeScale(slowMotion ? SLOW_MOTION : 1);
        updateHudVisibility();
      }
      break;
    case 'trial':
      if (mode !== 'drive') break;
      if (trial.active) {
        trial.stop();
        toast('Time trial cancelled');
        updateHudVisibility();
      } else {
        startTrial();
      }
      break;
    case 'nextWeapon':
    case 'weapon1':
    case 'weapon2':
    case 'weapon3':
      if (mode === 'drive' && settings.weapons) {
        weapons.select(action === 'nextWeapon' ? weapons.index + 1 : Number(action.slice(-1)) - 1);
        showWeapon();
        toast(WEAPONS[weapons.index].name);
      }
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
  car.setNitro(physics.boosting && mode === 'drive' ? 1 : 0);
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
    writeNumber('carGame.bestDrift', points);
    toast(`New best drift: ${points.toLocaleString()}!`);
    showBest();
  } else {
    toast(`Drift +${points.toLocaleString()}`);
  }
}

let accumulator = 0;
let lastTime = performance.now();
let fpsFrames = 0;
let fpsTime = 0;
const carVelocity = new THREE.Vector3();

function frame(): void {
  requestAnimationFrame(frame);
  const now = performance.now();
  // Frame rate limit: skip this screen refresh if it is too soon.
  if (settings.fpsLimit > 0 && now - lastTime < 1000 / settings.fpsLimit - 1.5) return;
  const realDt = Math.min(0.05, (now - lastTime) / 1000);
  lastTime = now;

  fpsFrames++;
  fpsTime += realDt;
  if (fpsTime >= 0.5) {
    const fps = fpsFrames / fpsTime;
    fpsEl.textContent = `${Math.round(fps)} FPS`;
    settingsUI.setFps(fps);
    fpsFrames = 0;
    fpsTime = 0;
  }

  for (const action of input.takeActions()) handleAction(action);

  const paused = settingsUI.visible || mode === 'photo';
  const dt = paused ? 0 : realDt * (slowMotion && mode === 'drive' ? SLOW_MOTION : 1);
  const controls = input.read();
  let slip = 0;
  if (mode === 'drive' && !paused) {
    const frozen = trial.active && trial.countdown > 0;
    accumulator += frozen ? 0 : dt;
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
          physics.nitro = Math.min(1, physics.nitro + airTime * 0.08);
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
    slip = physics.airborne ? 0 : Math.min(1, Math.abs(physics.lateralSpeed) / 7);
    if (drifting) {
      driftScore += speed * slipAngle * dt * 10;
      driftTimer = 0.8;
      physics.nitro = Math.min(1, physics.nitro + dt * 0.06); // drifting fills the nitro
    } else if (driftTimer > 0) {
      driftTimer -= dt;
      if (driftTimer <= 0) bankDrift();
    }
    // The bottle slowly refills on its own.
    if (!physics.boosting) physics.nitro = Math.min(1, physics.nitro + dt * 0.03);

    if (settings.weapons && !frozen) {
      carVelocity.set(physics.vx, physics.vy, physics.vz);
      weapons.update(dt, controls.fire, car, carVelocity);
    }

    comboTimer -= dt;
    if (comboTimer <= 0) combo = 0;

    const ev = trial.update(dt, physics.x, physics.y, physics.z);
    if (ev?.kind === 'go') {
      toast('GO!', 900);
      audio.chime();
    } else if (ev?.kind === 'checkpoint') {
      audio.chime();
    } else if (ev?.kind === 'finish') {
      audio.chime(true);
      toast(ev.best ? `New best lap: ${formatTime(ev.time!)}!` : `Lap ${formatTime(ev.time!)}  (best ${formatTime(trial.best)})`, 4000);
      showBest();
      updateHudVisibility();
    }
  }

  syncCar(dt);
  effects.update(dt);
  world.update(dt, physics.x, physics.z);
  cameraRig.update(realDt, carPosition(), physics.heading, physics.speed, physics.boosting && !paused);
  effects.setView(camera, drawingHeight);
  if (shake > 0.001 && mode === 'drive') {
    camera.position.x += (Math.random() - 0.5) * shake;
    camera.position.y += (Math.random() - 0.5) * shake;
    shake *= Math.exp(-realDt * 8);
  }

  // HUD
  const driving = mode === 'drive';
  audio.update(physics.speedFraction, driving && !paused ? controls.throttle : 0, paused ? 0 : slip, driving && !paused);
  speedEl.textContent = String(Math.round(physics.speed * 3.6));
  gearEl.textContent = physics.forwardSpeed < -0.5 ? 'R' : String(audio.gear);
  driftEl.textContent = driftScore >= 10 ? `DRIFT ${Math.round(driftScore).toLocaleString()}` : '';
  nitroFill.style.width = `${Math.round(physics.nitro * 100)}%`;
  nitroFill.classList.toggle('active', physics.boosting);
  boostFx.classList.toggle('on', driving && !paused && physics.boosting);
  weaponFill.style.width = `${Math.round(weapons.readiness * 100)}%`;
  weaponFill.classList.toggle('hot', weapons.kind === 'gun' && weapons.overheated);
  if (trial.active) {
    trialEl.textContent = `Checkpoint ${Math.min(trial.next, trial.checkpoints)}/${trial.checkpoints}  ·  ${formatTime(trial.time)}${trial.best ? `  ·  Best ${formatTime(trial.best)}` : ''}`;
    countdownEl.textContent = trial.countdown > 0 ? String(Math.ceil(trial.countdown)) : '';
  } else {
    countdownEl.textContent = '';
  }
  if (driving && settings.minimap && !paused) minimap.update(realDt, world, physics.x, physics.z, physics.heading, trial.markers());

  renderer.render(scene, camera);
}

// ---------- startup ----------
window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(window.innerWidth, window.innerHeight);
  drawingHeight = renderer.getDrawingBufferSize(new THREE.Vector2()).y;
  updateViewOffset();
});
// Sound can only start after the player presses something.
for (const type of ['keydown', 'pointerdown'] as const) window.addEventListener(type, () => audio.unlock());
input.mouseFireEnabled = () => mode === 'drive' && !settingsUI.visible && settings.weapons;

applySettings();
showScore();
syncCar(0);
setMode('garage');
requestAnimationFrame(frame);
