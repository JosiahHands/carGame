// Game entry point: sets up the 3D scene, the HUD and the game loop.
import * as THREE from 'three';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import installedPacks, { buildId } from 'virtual:installed-packs';
import { loadConfig, saveConfig, type CarConfig } from '../game/carConfig';
import { buildCar, carBuiltHooks, type CarRig } from '../game/carModel';
import { CarPhysics, GEAR_TOPS } from '../game/carPhysics';
import { CameraRig } from '../game/cameraRig';
import { Effects } from '../game/effects';
import { EngineAudio } from '../game/engineAudio';
import { GarageUI } from '../game/garageUI';
import { Input, type Action } from '../game/input';
import { Minimap } from '../game/minimap';
import { EFFECTS_AMOUNT, SHADOW_SIZES, TRAFFIC_COUNTS, loadSettings, saveSettings } from '../game/settings';
import { TimeTrial, formatTime } from '../game/timeTrial';
import { Weapons } from '../game/weapons';
import { venue } from '../game/venues';
import { World } from '../game/world';
import { CHASSIS, awaitingChoice, careerCar, dailyLevel, loadCareer, makeLevel, payDaily, payLevel, resetCareer, saveCareer, armourFor, lockedReason, LEVELS, type Level } from '../game/career';
import { EventRun, type EventSpec } from '../game/event';
import { MenuUI } from '../game/menuUI';
import { FreeRoam, type FreeRoamEvent } from '../game/freeRoam';
import { Music } from '../game/music';
import { Pedestrians } from '../game/pedestrians';
import { Traffic } from '../game/traffic';
import { addXp, bump, checkTrophies, driverLevel, record } from '../game/rewards';
import { SKILLS, SkillChain, type SkillKind } from '../game/skills';
import { PackHost } from '../game/packs';
import { showPacksTab } from '../game/packsUI';
import { el } from '../libs/dom';

const STEP = 1 / 120; // physics runs at a fixed 120 steps per second
const MAX_FRAME = 0.25; // seconds: the longest frame the game catches up on
const MAX_STEPS = Math.ceil(MAX_FRAME / STEP); // physics steps for that frame
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
const world = new World(scene, effects, settings.map);
const physics = new CarPhysics();
const input = new Input(renderer.domElement);
const audio = new EngineAudio();
const weapons = new Weapons(scene, world, effects, audio);
const trial = new TimeTrial(scene);
trial.setCourse(world.course, world.map);
const minimap = new Minimap();
document.body.append(minimap.canvas);
const freeRoam = new FreeRoam(scene);
const music = new Music();
// The city's NPCs: traffic and people on the pavements (free roam only).
const traffic = new Traffic();
traffic.onHonk = () => audio.trafficHorn();
const people = new Pedestrians(scene);

// ---------- game packs (see packs/README.md) ----------
// Started further down, once the rest of the game is set up.
const packOverlay = el('div', 'pack-overlay');
document.body.append(packOverlay);
let packs: PackHost | null = null;
/** Tells the packs about something that happened (once they've started). */
const tellPacks: PackHost['emit'] = (type, e) => packs?.emit(type, e);
carBuiltHooks.push((rig) => packs?.carBuilt(rig));
world.onLoaded = () => packs?.mapLoaded();

// ---------- the player's car ----------
/** The garage build: what you drive in free roam, and the paint and parts on career cars. */
const config: CarConfig = loadConfig();
/** The car being driven now: the garage build, or the car for the event. */
let driveConfig: CarConfig = config;
let night = false;
let car: CarRig = buildCar(driveConfig, night);
scene.add(car.root);
physics.reset(world.spawn.x, world.spawn.z, world.spawn.heading);
physics.y = world.heightAt(physics.x, physics.z);
fitToCar();

/** Rebuilds the car after a change. Returns true if its size changed a lot. */
function rebuildCar(): boolean {
  car.dispose();
  car = buildCar(driveConfig, night);
  scene.add(car.root);
  const resized = fitToCar();
  syncCar(0);
  return resized;
}

/**
 * Sets up the physics, cameras, sound and shadows for the car as it is now
 * built. Returns true if the car's size changed by more than 10%.
 */
function fitToCar(): boolean {
  physics.configure(driveConfig, car);
  cameraRig.eyeHeight = car.eyeHeight;
  // How much bigger than a normal car (about 4.5 m long and 1.4 m tall) it is.
  const size = Math.max(car.length / 4.5, car.height / 1.4);
  const resized = Math.abs(Math.log(size / cameraRig.size)) > 0.1;
  cameraRig.size = size;
  audio.setEngine(driveConfig.engineSound);
  applyShadows();
  applyViewDistance();
  return resized;
}

/**
 * Sets how far you can see: the View distance setting, but always far enough
 * to see the whole car from as far away as the camera can orbit (40 × its size,
 * in photo mode). Otherwise a giant car is lost in the fog or past the far plane.
 */
function applyViewDistance(): void {
  const s = cameraRig.size;
  const reach = 40 * s + Math.max(car.length, car.height);
  const distance = Math.max(settings.viewDistance, reach * 1.5);
  world.setViewDistance(distance);
  // A huge far plane needs the near plane further out too, or distant
  // surfaces flicker. The hood camera of a big car is far from its nose anyway.
  camera.near = 0.1 * Math.max(1, s / 5);
  camera.far = distance + 200; // a bit past the fog, for the hills on the horizon
  camera.updateProjectionMatrix();
}

/** Shadows cover the area around the car; a giant car needs a bigger area. */
function applyShadows(): void {
  const spread = Math.min(8, Math.max(1, cameraRig.size));
  if (settings.shadows === 'off') world.setShadows(false, SHADOW_SIZES.low.mapSize, SHADOW_SIZES.low.area);
  else world.setShadows(true, SHADOW_SIZES[settings.shadows].mapSize, SHADOW_SIZES[settings.shadows].area * spread);
}

/** Metres as feet or metres, and m/s as mph or km/h, depending on the Units setting. */
function showLength(m: number): string {
  if (settings.units === 'mph') return `${Math.round(m * 3.281).toLocaleString()} ft`;
  return `${m < 10 ? m.toFixed(1) : Math.round(m).toLocaleString()} m`;
}
function speedInUnits(metresPerSecond: number): number {
  return metresPerSecond * (settings.units === 'mph' ? 2.237 : 3.6);
}

// ---------- HUD ----------
const $ = (id: string) => document.getElementById(id)!;
const hud = $('hud');
const speedEl = $('speed');
const speedUnitEl = $('speed-unit');
const gearEl = $('gear');
const driftEl = $('drift');
const bestEl = $('best');
const nitroBar = $('nitro-fill');
const revsBar = $('revs-fill');
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
const raceEl = $('race');
const raceStatsEl = $('race-stats');
const raceStarsEl = $('race-stars');
const standingsEl = $('standings');
const healthEl = $('health');
const healthFill = $('health-fill');
const warningEl = $('warning');
const chainEl = $('chain');
const chainTotal = $('chain-total');
const chainMult = $('chain-mult');
const chainFill = $('chain-fill');
const chainFeed = $('chain-feed');
const boostsEl = $('boosts');
const zoneEl = $('zone');
const popupsEl = $('popups');
const speedLinesEl = $('speedlines');
let toastTimer = 0;

function toast(text: string, ms = 1600): void {
  toastEl.textContent = text;
  toastEl.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toastEl.classList.remove('show'), ms);
}

/** A bigger message that slides in at the side: trophies, level ups, finds. */
function popup(icon: string, title: string, text: string): void {
  const box = el('div', 'popup');
  const words = el('div', 'popup-text');
  words.append(el('strong', undefined, title), el('span', undefined, text));
  box.append(el('span', 'popup-icon', icon), words);
  popupsEl.append(box);
  window.setTimeout(() => box.remove(), 4600);
  audio.chime(true);
}

/** The announcer: says a line out loud, if it's switched on and the sound isn't muted. */
function speak(text: string): void {
  if (!settings.announcer || audio.isMuted || !('speechSynthesis' in window)) return;
  try {
    // Countdown numbers cut off whatever is being said, so they stay on the beat.
    if (/^(\d|Go!)$/.test(text)) speechSynthesis.cancel();
    const line = new SpeechSynthesisUtterance(text);
    line.rate = 1.15;
    line.pitch = 0.9;
    line.volume = 0.9;
    speechSynthesis.speak(line);
  } catch {
    // No voices: carry on without the announcer.
  }
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
  tellPacks('destroyed', { x: target.x, z: target.z, kind: target.kind });
  damage += target.value;
  event?.addDamage(target.value);
  scoreEl.textContent = `$${damage.toLocaleString()} damage`;
  const distance = Math.hypot(target.x - physics.x, target.z - physics.z);
  if (target.kind !== 'lamp' && target.kind !== 'tree') people.panic(target.x, target.z, 40);
  if (target.kind === 'building') {
    physics.nitro = Math.min(1, physics.nitro + 0.25);
    audio.explosion(2.5, distance);
    shake = Math.max(shake, Math.min(0.5, 12 / (distance + 10)));
    toast(`Building destroyed +$${target.value.toLocaleString()}`);
    skill('smash', 300, 'Building smash');
  } else if (target.kind === 'car') {
    physics.nitro = Math.min(1, physics.nitro + 0.1);
    audio.explosion(1.4, distance);
    toast(`Car destroyed +$${target.value.toLocaleString()}`);
    skill('smash', 150, 'Car smash');
  } else {
    physics.nitro = Math.min(1, physics.nitro + 0.05);
    audio.crash(6);
    skill('smash', 25);
  }
};

// ---------- settings ----------
function applySettings(): void {
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2) * settings.resolution);
  renderer.setSize(window.innerWidth, window.innerHeight);
  applyShadows();
  speedUnitEl.textContent = settings.units === 'mph' ? 'mph' : 'km/h';
  applyViewDistance();
  cameraRig.baseFov = settings.fov;
  effects.amount = EFFECTS_AMOUNT[settings.effects];
  physics.manual = settings.transmission === 'manual';
  gearEl.classList.toggle('manual', physics.manual);
  updateHud();
}

// ---------- menu, garage, driving and photo modes ----------
let mode: 'menu' | 'garage' | 'drive' | 'photo' = 'menu';
let slowMotion = false;

const garage = new GarageUI(config, settings, {
  carChanged(change) {
    driveConfig = config;
    roamingInCareerCar = false;
    // A much bigger or smaller car needs the garage camera moved to fit it.
    const resized = change === 'look' ? rebuildCar() : fitToCar();
    if (resized && mode === 'garage') cameraRig.setGarage(true, carPosition(), physics.heading);
    // Save right away: a delayed save is lost if the window closes first.
    saveConfig(config);
  },
  settingsChanged() {
    saveSettings(settings);
    applySettings();
    // Traffic or pedestrians switched on or off: put the right ones in the city.
    const npcs = `${settings.traffic}/${settings.pedestrians}`;
    if (npcs !== npcSettings && !event) loadNpcs();
  },
  rebuildCity() {
    world.rebuildAll(physics.x, physics.z, carRadius());
    damage = 0;
    scoreEl.textContent = '';
    toast('Everything has been rebuilt');
  },
  mapChanged() {
    saveSettings(settings);
    loadMap();
  },
  drive: () => setMode('drive'),
  menu() {
    setMode('menu');
    menu.showMain();
  },
  honk() {
    audio.unlock();
    audio.horn(config.horn);
  },
  describeCar() {
    const top = speedInUnits(config.tuning.topSpeed * 0.44704);
    // Matches the physics: a car 3 m or taller flattens things up to twice its height.
    const giant = car.height >= 3 ? ` · Flattens anything under ${showLength(car.height * 2)} tall` : '';
    return `${showLength(car.height)} tall · ${showLength(car.length)} long · top speed ${Math.round(top)} ${settings.units === 'mph' ? 'mph' : 'km/h'}${giant}`;
  },
  showPacks(panel) {
    if (packs) showPacksTab(packs, panel);
  },
});
document.body.append(garage.root);

/** How far the car's footprint reaches from its centre: half its diagonal. */
function carRadius(): number {
  return Math.hypot(car.length, car.width) / 2;
}

function carPosition(): THREE.Vector3 {
  return new THREE.Vector3(physics.x, physics.y, physics.z);
}

/** Shows the parts of the HUD that belong to the current mode and settings. */
function updateHud(): void {
  const driving = mode === 'drive';
  hud.classList.toggle('hidden', !driving);
  hintEl.classList.toggle('hidden', !driving || event !== null);
  scoreEl.classList.toggle('hidden', !driving || (event !== null && event.spec.kind !== 'destruction'));
  weaponPanel.classList.toggle('hidden', !driving || !weaponsOn());
  minimap.setVisible(driving && settings.minimap);
  zoneEl.classList.toggle('hidden', !driving || event !== null || zoneEl.textContent === '');
  freeRoam.setVisible(event === null);
  trialEl.classList.toggle('hidden', !driving || !trial.active);
  countdownEl.classList.toggle('hidden', !driving || !trial.active);
  raceEl.classList.toggle('hidden', !driving || !event);
  standingsEl.classList.toggle('hidden', !driving || !event?.isRace);
  healthEl.classList.toggle('hidden', !driving || !event?.spec.weapons || !event.track);
  warningEl.classList.toggle('hidden', !driving || !event);
  slowMotionEl.classList.toggle('hidden', !driving || !slowMotion);
  photoBar.classList.toggle('hidden', mode !== 'photo');
  fpsEl.classList.toggle('hidden', !settings.showFps);
  if (!driving) helpEl.classList.add('hidden');
}

function setMode(next: 'menu' | 'garage' | 'drive' | 'photo'): void {
  const previous = mode;
  mode = next;
  if (next !== 'menu') menu.hide();
  if (next === 'garage' || next === 'menu') {
    // Park the car where it is, and give up any time trial.
    physics.vx = physics.vz = physics.vy = 0;
    physics.y = world.heightAt(physics.x, physics.z);
    physics.airborne = false;
    trial.stop();
  }
  garage.setVisible(next === 'garage');
  // The menu, garage and photo mode all use the orbit camera; driving follows the car.
  if (next === 'garage' || next === 'menu') cameraRig.setGarage(true, carPosition(), physics.heading);
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
  if ((mode === 'garage' || mode === 'menu') && w > 820) camera.setViewOffset(w, h, Math.min(380, w * 0.3) / 2, 0, w, h);
  else camera.clearViewOffset();
}

/** Saves the picture on screen (without the HUD, which is HTML) as a PNG download. */
function savePhoto(): void {
  // Draw a fresh frame first: the canvas is cleared after each frame is shown.
  draw();
  const link = document.createElement('a');
  const stamp = new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '-');
  link.href = renderer.domElement.toDataURL('image/png');
  link.download = `car-game-${stamp}.png`;
  link.click();
  toast('Photo saved');
}
$('photo-save').addEventListener('click', savePhoto);

/** Switches to the map chosen in the settings, and puts the car at its start. */
function loadMap(): void {
  trial.stop();
  weapons.clear();
  effects.clear();
  traffic.clear(); // before the world is cleared, while its cars are still there to remove
  world.load(settings.map);
  freeRoam.load(world, career.found);
  trial.setCourse(world.course, world.map);
  ghost?.dispose();
  ghost = null;
  damage = 0;
  scoreEl.textContent = '';
  placeCar(world.spawn.x, world.spawn.z, world.spawn.heading);
  // After the car is at the new map's start: traffic is placed away from it.
  loadNpcs();
  syncCar(0);
  if (mode === 'garage') cameraRig.setGarage(true, carPosition(), physics.heading);
  showBest();
  updateHud();
  toast(venue(world.map).label);
}

/** Puts the car at a spot, standing still, with the camera behind it. */
function placeCar(x: number, z: number, heading: number): void {
  physics.reset(x, z, heading);
  physics.y = world.heightAt(x, z); // the start can be on a banked straight
  weapons.clear();
  cameraRig.snap(carPosition(), physics.heading);
  driftScore = 0;
  driftHeld = 0;
  chain.clear();
}

function startTimeTrial(): void {
  placeCar(trial.start.x, trial.start.z, trial.start.heading);
  trial.begin();
  // Race the ghost of the best lap, if there is one.
  ghost?.dispose();
  ghost = trial.hasGhost ? makeGhost() : null;
  updateHud();
}

// ---------- the ghost of the best lap ----------
let ghost: CarRig | null = null;
const ghostMaterial = new THREE.MeshStandardMaterial({ color: 0x9fd3ff, emissive: 0x3a7bd5, emissiveIntensity: 0.7, transparent: true, opacity: 0.35, depthWrite: false });

/** A see-through blue copy of the player's car. */
function makeGhost(): CarRig {
  const rig = buildCar(config, false, false);
  rig.root.traverse((o) => {
    if (o instanceof THREE.Mesh) {
      o.material = ghostMaterial;
      o.castShadow = false;
    } else if (o instanceof THREE.Sprite || o instanceof THREE.Points || o instanceof THREE.Light) {
      o.visible = false;
    }
  });
  rig.root.visible = false;
  scene.add(rig.root);
  return rig;
}

/** Moves the ghost to where the best lap was at this point in the lap. */
function updateGhost(): void {
  if (!ghost) return;
  const pose = trial.ghostPose();
  ghost.root.visible = pose !== null && mode !== 'garage';
  if (!pose) return;
  ghost.root.position.set(pose.x, pose.y, pose.z);
  ghost.root.rotation.set(pose.pitch, pose.heading, pose.groundRoll, 'YXZ');
}

function handleAction(action: Action): void {
  // In the menus, only the keys for sound and fullscreen do anything.
  if (mode === 'menu' && action !== 'mute' && action !== 'fullscreen') return;
  // Paused or on the results screen: only Esc (resume) works.
  if (event && menu.visible && action !== 'mute' && action !== 'fullscreen') {
    if (action === 'settings' && paused) resumeEvent();
    return;
  }
  // Keys that work in every mode.
  if (action === 'night' && !event) {
    night = !night;
    applyNight();
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

  // Driving in an event: some keys do other things, and some are off.
  if (event) {
    if (action === 'settings') {
      pauseEvent();
      return;
    }
    if (action === 'reset') {
      if (event.state === 'running' && event.playerWrecked <= 0) event.recover();
      return;
    }
    if (action === 'garage' || action === 'trial' || action === 'slowmo') {
      toast('Not during an event · Esc to pause');
      return;
    }
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
    case 'horn':
      audio.horn(config.horn);
      tellPacks('horn', { x: physics.x, z: physics.z });
      break;
    case 'shiftUp':
    case 'shiftDown':
      if (!physics.manual) {
        toast('Automatic gearbox: pick Manual in Settings (Esc)', 2200);
        break;
      }
      // A refused change (top gear, first gear, or too fast to change down) flashes the gear red.
      if (!(action === 'shiftUp' ? physics.shiftUp() : physics.shiftDown())) gearRefused = 0.25;
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
      if (!weaponsOn()) break;
      if (action === 'nextWeapon') weapons.select(weapons.selected + 1);
      else weapons.select(Number(action.slice(-1)) - 1); // 'weapon2' -> 1
      weaponName.textContent = `${weapons.selected + 1} · ${weapons.name}`;
      toast(weapons.name);
      break;
  }
}

// ---------- per-frame updates ----------
/** Seconds left of the red flash on the gear, after a gear change that wasn't allowed. */
let gearRefused = 0;

function syncCar(dt: number): void {
  car.root.position.set(physics.x, physics.y, physics.z);
  car.root.rotation.set(physics.pitch, physics.heading, physics.groundRoll, 'YXZ');
  car.body.rotation.set(physics.bodyPitch, 0, physics.roll);
  for (const w of car.wheels) {
    w.spin.rotation.x += (physics.forwardSpeed / w.radius) * dt;
    if (w.front) w.steer.rotation.y = physics.steerAngle;
  }
  car.setLights({ night: world.isNight, braking: physics.braking && mode === 'drive' });
  car.setWrecked(event !== null && event.playerWrecked > 0);
  car.setNitro(physics.boosting && mode === 'drive');
  car.animate(performance.now() / 1000);
}

/**
 * Tyre smoke while drifting or spinning the wheels, and the boost trail while
 * on nitro. Particles are spawned at a steady rate, whatever the frame rate.
 */
let smokeDue = 0;
let sparksDue = 0;
let grassDue = 0;
let trailDue = 0;
let lastTips: THREE.Vector3[] = []; // where the exhausts were last frame
let odometer = 0; // metres driven, for the rainbow trail's colour bands
const spot = new THREE.Vector3();
function carEffects(dt: number, throttle: number): void {
  const size = cameraRig.size;
  const sliding = !physics.airborne && Math.abs(physics.lateralSpeed) > 4 && physics.speed > 4;
  const burnout = !physics.airborne && throttle > 0.9 && physics.speed < 6 && physics.forwardSpeed >= 0;
  smokeDue += config.tireSmoke && (sliding || burnout) ? dt * 50 * effects.amount : 0;
  while (smokeDue >= 1) {
    smokeDue--;
    const wheel = car.wheels[2 + Math.floor(Math.random() * 2)]; // a back wheel
    wheel.steer.getWorldPosition(spot);
    spot.y = physics.y + 0.15 * size;
    const drift = new THREE.Vector3((Math.random() - 0.5) * 2, 0.6 + Math.random(), (Math.random() - 0.5) * 2).multiplyScalar(size);
    effects.addSmoke({ at: spot.clone(), velocity: drift, life: 1.6, size: [0.5 * size, 2.6 * size], color: config.tireSmokeColor, opacity: 0.55, drag: 1.5 });
  }

  // Mini-turbo sparks from the back wheels while a drift is held: blue, then orange, then purple.
  if (sparkTier > 0 && driftHeld > 0) {
    sparksDue += dt * 70 * effects.amount;
    while (sparksDue >= 1) {
      sparksDue--;
      car.wheels[2 + Math.floor(Math.random() * 2)].steer.getWorldPosition(spot);
      spot.y = physics.y + 0.1 * size;
      const fly = new THREE.Vector3(-physics.vx * 0.2 + (Math.random() - 0.5) * 4, 1 + Math.random() * 3, -physics.vz * 0.2 + (Math.random() - 0.5) * 4).multiplyScalar(size);
      effects.addGlow({ at: spot.clone(), velocity: fly, life: 0.35, size: [0.22 * size, 0.04 * size], color: SPARK_COLORS[sparkTier - 1], rise: -12 });
    }
  }

  // Dirt and grass thrown up by the wheels off the edge of a race track.
  grassDue += physics.onGrass && physics.speed > 5 ? dt * Math.min(60, physics.speed) * effects.amount : 0;
  while (grassDue >= 1) {
    grassDue--;
    car.wheels[Math.floor(Math.random() * car.wheels.length)].steer.getWorldPosition(spot);
    spot.y = physics.y + 0.2 * size;
    const kick = new THREE.Vector3(-physics.vx * 0.15 + (Math.random() - 0.5) * 2, 1 + Math.random() * 2, -physics.vz * 0.15 + (Math.random() - 0.5) * 2).multiplyScalar(size);
    effects.addSmoke({ at: spot.clone(), velocity: kick, life: 0.9, size: [0.4 * size, 1.6 * size], color: Math.random() < 0.5 ? 0x7a6a48 : 0x5f7d3a, opacity: 0.6, drag: 2 });
  }

  // More particles the faster the car goes, so the trail has no gaps (up to 40 a frame).
  // The particle pools are shared with explosions, so this stays modest.
  trailDue += physics.boosting ? dt * 40 * Math.max(1, physics.speed / 25) * effects.amount : 0;
  trailDue = Math.min(trailDue, 40);
  const startDistance = odometer;
  odometer += physics.speed * dt;
  // Spread this frame's particles evenly along the path each exhaust moved
  // since the last frame, so a fast car leaves a smooth trail, not clumps.
  const tips = car.exhausts.map((exhaust) => exhaust.getWorldPosition(new THREE.Vector3()));
  if (lastTips.length !== tips.length) lastTips = tips.map((t) => t.clone());
  const count = Math.floor(trailDue);
  trailDue -= count;
  for (let k = 1; k <= count; k++) {
    for (let i = 0; i < tips.length; i++) {
      const at = lastTips[i].clone().lerp(tips[i], k / count);
      switch (config.boostTrail) {
        case 'flames': // embers in the flame colour
          effects.addGlow({ at, velocity: new THREE.Vector3((Math.random() - 0.5) * 2, Math.random(), (Math.random() - 0.5) * 2).multiplyScalar(size), life: 0.3, size: [0.25 * size, 0.05 * size], color: config.nitroColor });
          break;
        case 'rainbow': // a ribbon of coloured puffs that hangs in the air, changing colour along its length
          at.y += 0.15 * size;
          effects.addSmoke({ at, life: 0.8, size: [0.4 * size, 0.25 * size], color: new THREE.Color().setHSL(((startDistance + (odometer - startDistance) * (k / count)) / 4) % 1, 1, 0.5), opacity: 0.9 }); // a full rainbow every 4 m
          break;
        case 'sparkles':
          effects.addGlow({ at, velocity: new THREE.Vector3((Math.random() - 0.5) * 6, Math.random() * 4, (Math.random() - 0.5) * 6).multiplyScalar(size), life: 0.6, size: [0.18 * size, 0.02 * size], color: Math.random() < 0.5 ? 0xffffff : 0xffd23f, rise: -6 * size });
          break;
        case 'smoke':
          effects.addSmoke({ at, velocity: new THREE.Vector3(0, 0.5 * size, 0), life: 1.4, size: [0.4 * size, 2 * size], color: 0x3a3a3a, opacity: 0.6, drag: 1 });
          break;
      }
    }
  }
  lastTips = tips;
}

let driftScore = 0;
let driftTimer = 0;
let airTime = 0;
let shake = 0;

// ---------- skill chains, mini-turbos, XP and trophies ----------
const chain = new SkillChain();
/** Seconds the chain panel stays up after a chain is banked or lost. */
let chainShown = 0;
/** Seconds the current drift has been held, and since it stopped (for the mini-turbo). */
let driftHeld = 0;
let driftGap = 0;
/** 0, or 1-3 for the blue, orange and purple mini-turbo. */
let sparkTier = 0;
const SPARK_COLORS = [0x4fb3ff, 0xff9f40, 0xc77dff];
const SPARK_NAMES = ['Blue', 'Orange', 'Purple'];
/** Seconds counted towards the next slipstream and speed-demon points. */
let draftTick = 0;
let speedTick = 0;
/** Free roam in the career car: only then do challenge stars count. */
let roamingInCareerCar = false;

/** Skills that are counted for trophies and the stats. */
const COUNTED: SkillKind[] = ['nearMiss', 'takedown', 'overtake', 'perfectStart', 'driftBoost'];

/** Adds a skill to the chain. */
function skill(kind: SkillKind, points: number, label?: string): void {
  // After the finish the chain has been banked. Skills from the finishing
  // frame itself still count: they're added before the chain is banked.
  if (event?.state === 'done' && celebrated) return;
  chain.add(kind, points, label);
  if (COUNTED.includes(kind)) bump(career, kind);
}

chain.onBank = (points) => {
  physics.nitro = Math.min(1, physics.nitro + Math.min(0.4, points / 10000)); // a good chain refills the nitro
  record(career, 'bestChain', points);
  showChainEnd(`+${points.toLocaleString()} XP`, 'banked');
  gainXp(points);
};
chain.onBreak = (lost) => {
  showChainEnd(`Chain lost −${lost.toLocaleString()}`, 'broken');
  if (lost > 1000) speak('Chain lost!');
};

function showChainEnd(text: string, cls: 'banked' | 'broken'): void {
  chainShown = 1.4;
  chainEl.classList.remove('banked', 'broken');
  chainEl.classList.add(cls);
  chainTotal.textContent = text;
  chainMult.textContent = '';
  chainFill.style.width = '0%';
  chainFeed.replaceChildren();
}

/** Adds XP: a new driver level earns a spin of the prize wheel. Also awards any trophies just won. */
function gainXp(xp: number): string[] {
  const lines: string[] = [];
  const levels = addXp(career, xp);
  if (levels > 0) {
    const level = driverLevel(career.xp).level;
    popup('🎖️', `Driver level ${level}!`, `+${levels} prize spin${levels > 1 ? 's' : ''}: spin from the main menu`);
    speak('Level up!');
    lines.push(`🎖️ Driver level ${level}! Prize wheel spin earned`);
  }
  for (const t of checkTrophies(career)) {
    popup(t.icon, `Trophy: ${t.name}`, `${t.about} · +🪙 ${t.reward.toLocaleString()}`);
    lines.push(`${t.icon} Trophy: ${t.name} +🪙 ${t.reward.toLocaleString()}`);
  }
  saveCareer(career);
  return lines;
}

/** Draws the chain panel: total, multiplier, time left and the latest skills. */
let chainDrawn = '';
function drawChain(dt: number): void {
  if (chain.count === 0) {
    chainShown -= dt;
    chainEl.classList.toggle('hidden', chainShown <= 0 || mode !== 'drive');
    chainDrawn = '';
    return;
  }
  chainEl.classList.remove('hidden', 'banked', 'broken');
  chainFill.style.width = `${Math.round((chain.timer / 4) * 100)}%`;
  const key = `${chain.total}|${chain.feed.map((f) => f.points).join(',')}`;
  if (key === chainDrawn) return;
  chainDrawn = key;
  chainTotal.textContent = chain.total.toLocaleString();
  chainMult.textContent = `×${chain.multiplier}`;
  chainFeed.replaceChildren(...chain.feed.map((f) => {
    const row = el('li');
    row.style.color = SKILLS[f.kind].color;
    row.append(el('span', undefined, f.label), el('span', undefined, `+${f.points.toLocaleString()}`));
    return row;
  }));
}

/**
 * The drift mini-turbo: a drift held for 1, 2 or 3 seconds charges a blue,
 * orange or purple boost, fired when the drift ends.
 */
function miniTurbo(dt: number, drifting: boolean): void {
  if (drifting) {
    driftHeld += dt;
    driftGap = 0;
  } else if (driftHeld > 0) {
    driftGap += dt;
    // A moment's grace, so a wobble mid-drift doesn't end it.
    if (driftGap > 0.15) {
      if (sparkTier > 0 && physics.forwardSpeed > 5) {
        physics.giveTurbo([0.6, 1, 1.5][sparkTier - 1], [1.15, 1.25, 1.35][sparkTier - 1]);
        skill('driftBoost', 150 * sparkTier, `${SPARK_NAMES[sparkTier - 1]} boost`);
        audio.chime();
      }
      driftHeld = 0;
    }
  }
  sparkTier = driftHeld >= 3 ? 3 : driftHeld >= 2 ? 2 : driftHeld >= 1 ? 1 : 0;
}

/** Slipstream and flat-out driving earn points every second. */
function steadySkills(dt: number): void {
  if (physics.draft > 0.5 && physics.speed > 15) {
    draftTick += dt;
    bump(career, 'draft', dt);
    if (draftTick >= 1) {
      draftTick -= 1;
      skill('draft', 60);
    }
  }
  if (physics.speedFraction > 0.9 && !physics.airborne) {
    speedTick += dt;
    if (speedTick >= 1) {
      speedTick -= 1;
      skill('speed', 80);
    }
  } else {
    speedTick = 0;
  }
  if (event) record(career, 'topSpeed', Math.round(physics.speed * 2.237));
}

/** The labels over the speedo: slipstream, turbo and the drift sparks. */
let boostsDrawn = '';
function drawBoosts(): void {
  const parts: string[] = [];
  if (sparkTier > 0) parts.push(`spark${sparkTier}|${'●'.repeat(sparkTier)}${'○'.repeat(3 - sparkTier)} ${SPARK_NAMES[sparkTier - 1].toUpperCase()}`);
  if (physics.draft > 0.3) parts.push(`draft|SLIPSTREAM ${Math.round(physics.draft * 100)}%`);
  if (physics.turbo > 0) parts.push('turbo|TURBO');
  const key = mode === 'drive' ? parts.join('/') : '';
  if (key === boostsDrawn) return;
  boostsDrawn = key;
  boostsEl.replaceChildren(...(key ? parts : []).map((p) => {
    const [cls, text] = p.split('|');
    return el('span', cls, text);
  }));
}

function bankDrift(): void {
  const points = Math.round(driftScore);
  driftScore = 0;
  if (points < 50) return;
  tellPacks('drift', { x: physics.x, z: physics.z, points });
  event?.addDrift(points);
  if (!event) freeRoam.addDrift(points);
  skill('drift', points / 2, `Drift ${points.toLocaleString()}`);
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
  while (accumulator >= STEP && steps < MAX_STEPS) {
    const wasAirborne = physics.airborne;
    const ev = physics.step(STEP, controls, world);
    accumulator -= STEP;
    steps++;
    if (physics.airborne) airTime += STEP;
    else if (wasAirborne) {
      if (airTime > 0.5) event?.addAir(airTime);
      if (airTime > 0.7) {
        tellPacks('jump', { x: physics.x, z: physics.z, airTime });
        toast(`Air time ${airTime.toFixed(1)} s`);
        physics.nitro = Math.min(1, physics.nitro + airTime * 0.08); // jumps fill the nitro
        skill('air', airTime * 300, `Air ${airTime.toFixed(1)} s`);
        if (airTime > 1.2) people.cheer(physics.x, physics.z, 45);
        bump(career, 'air', airTime);
      }
      airTime = 0;
    }
    if (ev.impact > 3) {
      tellPacks('crash', { x: physics.x, z: physics.z, impact: ev.impact });
      audio.crash(ev.impact);
      shake = Math.min(0.6, ev.impact * 0.03);
      if (driftScore > 0) {
        driftScore = 0;
        driftHeld = 0;
        toast('Crashed! Drift lost');
      }
      if (ev.impact > 7) chain.break(); // a hard crash loses the skill chain
      // Ramming a parked car hard enough wrecks it.
      if (ev.hitTarget && traffic.owns(ev.hitTarget)) traffic.hit(ev.hitTarget);
      if (ev.hitTarget && ev.impact > 6) {
        world.damage(ev.hitTarget, ev.impact * 5, new THREE.Vector3(physics.vx, 0, physics.vz).normalize());
      }
    }
    if (ev.smashed > 0) shake = Math.max(shake, 0.15);
    if (physics.onGrass && physics.speed > 10) shake = Math.max(shake, 0.04); // bumpy
    if (ev.cones > 0) audio.crash(5);
    if (ev.landed > 6) audio.crash(ev.landed * 0.6);
    // The rivals, and bumping into them.
    if (event) {
      const bump = event.step(STEP);
      if (bump > 3) {
        audio.crash(bump);
        shake = Math.max(shake, Math.min(0.5, bump * 0.03));
      }
      if (bump > 7) chain.break(); // as hard a crash as hitting a wall
    }
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
  miniTurbo(dt, drifting);
  steadySkills(dt);
  if (event) {
    // Still sliding or flying when the time runs out? It counts.
    event.unbankedDrift = driftScore >= 50 ? Math.round(driftScore) : 0;
    event.unbankedAir = physics.airborne && airTime > 0.5 ? airTime : 0;
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
    people.panic(blast.at.x, blast.at.z, blast.radius * 3 + 12);
    tellPacks('explosion', { x: blast.at.x, z: blast.at.z, size: blast.radius });
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
  const event = trial.update(dt, physics);
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

// ---------- events: career levels, quick races and the daily challenge ----------
let career = loadCareer();
let event: EventRun | null = null;
let paused = false;
/** Starts the current event again, for Restart and Retry. */
let again: (() => void) | null = null;
/** The career level (or daily challenge) being played; null in a quick race. */
let playing: Level | null = null;
/** Seconds since the event ended: the results come up after a moment. */
let endedFor = 0;
let resultsShown = false;
let eventHudTimer = 0;
/** No controls pressed: for holding the car still. */
const STOPPED = { throttle: 0, brake: 0, steer: 0, handbrake: false, boost: false, jump: false, fire: false };

/** Weapons are on in free roam if the setting says so, and in events that have them. */
function weaponsOn(): boolean {
  return event ? event.spec.weapons : settings.weapons;
}

/** Lights and sky for day or night (some venues are always at night). */
function applyNight(): void {
  world.setNight(night);
  scene.environmentIntensity = world.isNight ? 0.2 : 1;
  packs?.nightChanged(world.isNight);
}

const menu = new MenuUI(career, config, {
  playLevel,
  playQuick(spec, drivenCar) {
    again = () => startEvent(spec, drivenCar, null, false);
    again();
  },
  freeRoam() {
    leaveEvents();
    setMode('drive');
  },
  freeRoamCareer() {
    leaveEvents();
    driveConfig = careerCar(config, career.car, career.upgrades);
    rebuildCar();
    physics.allTerrain = CHASSIS[career.car].allTerrain ?? false;
    roamingInCareerCar = true;
    setMode('drive');
    toast('Your career car: challenge stars count', 2500);
  },
  garage() {
    leaveEvents();
    setMode('garage');
  },
  carChanged() {
    career = menu.save;
    saveCareer(career);
    // Show off the career car behind the menu (rebuilt only when it looks different).
    if (mode === 'menu' && !event) {
      const next = careerCar(config, career.car, career.upgrades);
      if (JSON.stringify(next) !== JSON.stringify(driveConfig)) {
        driveConfig = next;
        rebuildCar();
        cameraRig.setGarage(true, carPosition(), physics.heading);
      }
    }
  },
  resume: resumeEvent,
  restart: () => again?.(),
  quit: quitEvent,
  chooseSandbox() {
    career.mode = 'sandbox';
    career.finished = true;
    saveCareer(career);
    if (event) quitEvent();
    menu.showCareer('Levels', 11);
  },
  chooseReset() {
    career = resetCareer(career, true);
    menu.save = career;
    freeRoam.load(world, career.found);
    if (event) quitEvent();
    menu.showCareer('Levels', 1);
  },
  units: () => settings.units,
});
document.body.append(menu.root);

/** Plays a career level or the daily challenge (level number 0), if it's unlocked. */
function playLevel(level: Level): void {
  const reason = level.number > 0 && level.number <= LEVELS ? lockedReason(career, level.number) : '';
  if (reason) {
    toast(reason);
    return;
  }
  level.spec.armour = armourFor(career.upgrades.armour);
  const drivenCar = careerCar(config, career.car, career.upgrades);
  // A fresh copy of the level each time: the targets are worked out as it starts.
  again = () => playLevel(level.number === 0 ? dailyLevel(career) : makeLevel(level.number));
  startEvent(level.spec, drivenCar, level, CHASSIS[career.car].allTerrain ?? false);
}

/** Loads the venue, puts everyone on the grid and starts the countdown. */
function startEvent(spec: EventSpec, drivenCar: CarConfig, level: Level | null, allTerrain: boolean): void {
  event?.dispose();
  // No traffic or pedestrians in events.
  traffic.clear();
  people.load(world, false);
  playing = level;
  trial.stop();
  ghost?.dispose();
  ghost = null;
  weapons.clear();
  effects.clear();
  night = spec.night;
  if (world.map !== spec.map || world.layout !== spec.layout) world.load(spec.map, spec.layout);
  else world.rebuildAll(1e6, 1e6, 0); // put back anything destroyed last time
  applyNight();
  driveConfig = drivenCar;
  rebuildCar();
  physics.allTerrain = allTerrain;
  damage = 0;
  scoreEl.textContent = '';
  driftScore = 0;
  airTime = 0;
  slowMotion = false;
  paused = false;
  endedFor = 0;
  resultsShown = false;
  event = new EventRun(spec, scene, world, effects, audio, physics, world.isNight);
  chain.clear();
  driftHeld = 0;
  throttleAt = null;
  wasCountdown = false;
  wheelspin = 0;
  celebrated = false;
  fireworks = 0; // a Retry during the celebration stops the fireworks
  fireworkDue = 0;
  // A rival who keeps beating you is your nemesis.
  const nemesis = event.racers.filter((r) => r.rival && (career.nemesis[r.name] ?? 0) >= 2).map((r) => r.name)[0];
  if (nemesis) {
    event.notes.push(`😈 Your nemesis ${nemesis} is here. Beat them for revenge!`);
    event.callouts.push(`Your nemesis, ${nemesis}, is here.`);
  }
  syncCar(0);
  setMode('drive');
  cameraRig.setGarage(false, carPosition(), physics.heading);
  cameraRig.snap(carPosition(), physics.heading);
  updateHud();
}

function pauseEvent(): void {
  if (!event || event.state === 'done') return;
  paused = true;
  menu.showPause(event.spec.title);
}

function resumeEvent(): void {
  paused = false;
  menu.hide();
}

/** Ends any event and goes back to the garage build, on the map chosen in the settings. */
function leaveEvents(): void {
  event?.dispose();
  event = null;
  playing = null;
  paused = false;
  physics.allTerrain = false;
  roamingInCareerCar = false;
  if (driveConfig !== config) {
    driveConfig = config;
    rebuildCar();
  }
  if (night) {
    night = false;
    applyNight();
  }
  if (world.map !== settings.map || world.layout !== 0) {
    loadMap();
  } else {
    placeCar(world.spawn.x, world.spawn.z, world.spawn.heading);
    freeRoam.load(world, career.found);
    loadNpcs();
  }
}

/** The Traffic and Pedestrians settings when the NPCs were last put in. */
let npcSettings = '';

/** Puts traffic and people in the city for free roam, as many as the settings say. */
function loadNpcs(): void {
  npcSettings = `${settings.traffic}/${settings.pedestrians}`;
  traffic.load(world, TRAFFIC_COUNTS[settings.traffic], physics.x, physics.z);
  people.load(world, settings.pedestrians);
  people.setVisible(true);
}

/** Moves the traffic and people on, and scores near misses with traffic. */
function updateNpcs(dt: number): void {
  const player = { x: physics.x, y: physics.y, z: physics.z, vx: physics.vx, vz: physics.vz, heading: physics.heading, width: car.width, length: car.length };
  for (const points of traffic.update(dt, player, world.isNight)) {
    skill('nearMiss', points, 'Near miss: traffic');
    tellPacks('nearMiss', { x: physics.x, z: physics.z });
    toast('Near miss!', 700);
  }
  people.update(dt, { x: physics.x, y: physics.y, z: physics.z, vx: physics.vx, vz: physics.vz, reach: carRadius() });
}

/** Quits an event (or finishes with one) and goes to the main menu. */
function quitEvent(): void {
  event?.dispose();
  event = null;
  paused = false;
  physics.allTerrain = false;
  placeCar(world.spawn.x, world.spawn.z, world.spawn.heading);
  setMode('menu');
  menu.showMain();
}

/** Each frame of an event: messages, the HUD, and the results at the end. */
function updateEvent(dt: number): void {
  const e = event!;
  for (const note of e.notes.splice(0)) toast(note, note === 'GO!' ? 900 : note.startsWith('😈') ? 3000 : 1800);
  for (const s of e.skills.splice(0)) skill(s.kind, s.points, s.label);
  for (const line of e.callouts.splice(0)) speak(line);
  if (e.state === 'done' && !celebrated) celebrate(e);
  if (fireworks > 0) {
    fireworks -= dt;
    fireworkDue -= dt;
    if (fireworkDue <= 0) {
      fireworkDue = 0.3 + Math.random() * 0.3;
      const colors = [0xff3b3b, 0xffd23f, 0x3bb273, 0x1b98e0, 0xc62f9e, 0x7af0ff];
      const a = Math.random() * Math.PI * 2;
      const s = cameraRig.size;
      effects.firework(new THREE.Vector3(physics.x + Math.sin(a) * 12 * s, physics.y + (10 + Math.random() * 6) * s, physics.z + Math.cos(a) * 12 * s), colors[Math.floor(Math.random() * colors.length)], s);
      audio.explosion(0.4, 30);
    }
  }
  eventHudTimer -= dt;
  if (eventHudTimer <= 0) {
    eventHudTimer = 0.1;
    drawEventHud(e);
  }
  if (e.state === 'done' && !resultsShown) {
    endedFor += dt;
    if (endedFor > 1.5) showResults(e);
  }
}

// ---------- the start and the finish ----------
/** The countdown time when the throttle went down (null while it's up), for the launch. */
let throttleAt: number | null = null;
let wasCountdown = false;
/** Seconds of wheelspin left after a start that was too early. */
let wheelspin = 0;
let celebrated = false;
let fireworks = 0;
let fireworkDue = 0;

/**
 * Launch control: press the throttle in the last 0.6 seconds of the countdown
 * for a perfect start with a turbo boost. Press it too early and the wheels
 * spin for a moment at the start.
 */
function launch(e: EventRun, throttle: number): void {
  if (e.state === 'countdown') {
    wasCountdown = true;
    if (throttle > 0.5) throttleAt ??= e.countdown;
    else throttleAt = null;
    return;
  }
  if (!wasCountdown) return;
  wasCountdown = false;
  if (throttleAt !== null && throttleAt <= 0.6) {
    physics.giveTurbo(1.2, 1.3);
    skill('perfectStart', 500);
    toast('PERFECT START!', 1200);
    speak('Perfect start!');
  } else if (throttleAt !== null && throttleAt > 1.2) {
    wheelspin = 0.7;
    toast('Wheelspin! Too early', 1200);
  }
  throttleAt = null;
}

/** At the finish: bank the chain, and fireworks and an orbiting camera for a good result. */
function celebrate(e: EventRun): void {
  celebrated = true;
  chain.bank();
  if (!e.result || e.result.stars === 0) return;
  fireworks = e.result.stars === 3 || e.result.position === 1 ? 4 : 2;
  fireworkDue = 0;
  effects.confetti(carPosition(), 60);
  cameraRig.setGarage(true, carPosition(), physics.heading);
}

function drawEventHud(e: EventRun): void {
  const h = e.hud();
  raceStatsEl.replaceChildren(...h.stats.map((s) => {
    const box = el('div', 'race-stat');
    box.append(el('span', 'label', s.label), el('span', 'value', s.value));
    return box;
  }));
  raceStarsEl.textContent = '★'.repeat(h.stars) + '☆'.repeat(3 - h.stars);
  standingsEl.replaceChildren(...h.standings.map((r, i) => {
    const row = el('li', r.me ? 'me' : r.out ? 'out' : undefined);
    row.append(el('span', 'pos', String(i + 1)), el('span', 'name', r.name), el('span', 'gap', r.gap));
    return row;
  }));
  warningEl.textContent = h.warning;
  warningEl.classList.toggle('count', e.state === 'countdown');
  if (h.health !== null) {
    healthFill.style.width = `${Math.round(h.health * 100)}%`;
    healthFill.classList.toggle('low', h.health < 0.3);
  }
}

/** Pays out and shows the results (or, after level 100, the end of the career). */
function showResults(e: EventRun): void {
  resultsShown = true;
  const r = e.result!;
  if (r.stars > 0) audio.chime(true);
  const extras = rewardEvent(e);
  if (!playing) {
    menu.showResults(e.spec, r, null, null, extras);
    return;
  }
  const level = playing;
  const payout = level.number === 0 ? payDaily(career, level, r.stars) : payLevel(career, level, { ...r, lowerIsBetter: e.lowerIsBetter });
  let next: Level | null = null;
  if (level.number > 0 && r.passed) {
    // Level 100 beaten: offer Sandbox or Reset. Nothing is saved as chosen
    // until the player picks one, so closing the game here keeps the choice
    // waiting on the main menu.
    if (level.number === LEVELS && awaitingChoice(career)) {
      menu.showCareerComplete();
      return;
    }
    const n = level.number >= LEVELS ? (career.mode === 'sandbox' ? career.sandboxLevel : 0) : level.number + 1;
    if (n > LEVELS || (n > 0 && lockedReason(career, n) === '')) next = makeLevel(n);
  }
  menu.showResults(e.spec, r, payout, next, extras);
}

/** XP for an event, and the nemesis: lines for the results screen. */
function rewardEvent(e: EventRun): string[] {
  const r = e.result!;
  const lines: string[] = [];
  let xp = 1000 + 1000 * r.stars;
  if (e.isRace && r.standings.length > 1) {
    const mine = r.standings.findIndex((s) => s.me);
    // Revenge: finishing ahead of your nemesis.
    for (const [i, s] of r.standings.entries()) {
      if (s.me || (career.nemesis[s.name] ?? 0) < 2 || i < mine) continue;
      career.nemesis[s.name] = 0;
      bump(career, 'revenge');
      xp += 1500;
      lines.push(`😤 Revenge on ${s.name}! +1,500 XP`);
      speak(`Revenge on ${s.name}!`);
    }
    // Every rival who finishes ahead of you might become your nemesis.
    for (const s of r.standings.slice(0, Math.max(0, mine))) {
      career.nemesis[s.name] = (career.nemesis[s.name] ?? 0) + 1;
      if (career.nemesis[s.name] === 2) lines.push(`😈 ${s.name} has beaten you twice: they're your nemesis now`);
    }
  }
  lines.unshift(`+${xp.toLocaleString()} XP`);
  lines.push(...gainXp(xp));
  return lines;
}

// ---------- free roam: gold wrenches and challenges ----------
function updateFreeRoam(dt: number): void {
  const found = freeRoam.update(dt, { x: physics.x, y: physics.y, z: physics.z, speed: physics.speed, airborne: physics.airborne }, driftScore, airTime, carRadius());
  for (const f of found) freeRoamEvent(f);
  const text = freeRoam.hud(physics, driftScore);
  if (zoneEl.textContent !== text) {
    zoneEl.textContent = text;
    zoneEl.classList.toggle('hidden', text === '');
  }
}

function freeRoamEvent(f: FreeRoamEvent): void {
  if (f.type === 'wrench') {
    if (career.found.includes(f.id)) return;
    career.found.push(f.id);
    career.coins += 500;
    career.earned += 500;
    skill('collect', 500, 'Gold wrench');
    popup('🔧', `Gold wrench ${career.found.length} found!`, '+🪙 500 · +300 XP');
    gainXp(300);
    return;
  }
  const z = f.zone;
  const stars = FreeRoam.stars(z, f.score);
  const starText = '★'.repeat(stars) + '☆'.repeat(3 - stars);
  if (!roamingInCareerCar) {
    toast(`${z.name}: ${f.text} ${starText} · stars only count in your career car`, 3000);
    return;
  }
  const had = career.zones[z.id] ?? 0;
  if (stars > had) {
    const coins = 400 * (stars - had);
    career.zones[z.id] = stars;
    career.coins += coins;
    career.earned += coins;
    popup('🗺️', `${z.name}: ${starText}`, `${f.text} · +🪙 ${coins.toLocaleString()}`);
    gainXp(250 * (stars - had));
  } else {
    toast(`${z.name}: ${f.text} ${starText}${had ? ` (best ${'★'.repeat(had)})` : ''}`, 2500);
  }
}

let accumulator = 0;
let lastTime = performance.now();
let nextFrameDue = 0; // for the frame rate limit: when the next frame should be drawn
let fpsFrames = 0;
let fpsTime = 0;

function frame(now: number): void {
  requestAnimationFrame(frame);
  // Frame rate limit: draw on the first screen refresh at or after each due
  // time, then move the due time on by exactly one frame. Keeping a schedule
  // (rather than timing from the last frame) makes the average come out right
  // even when the screen's refresh rate isn't a multiple of the limit, e.g.
  // 60 FPS on a 144 Hz screen. The 1 ms allows for timing jitter.
  if (settings.fpsLimit > 0) {
    const interval = 1000 / settings.fpsLimit;
    if (now < nextFrameDue - 1) return;
    if (now - nextFrameDue > interval) nextFrameDue = now; // fell behind (or just turned on): start again from now
    nextFrameDue += interval;
  }
  const elapsed = Math.max(0, (now - lastTime) / 1000);
  lastTime = now;
  // Game updates are capped at a quarter of a second, so a long pause doesn't
  // make the car jump. Below that the game keeps up with real time, even at
  // 4 FPS, so race clocks and time limits run at the right speed.
  const realDt = Math.min(MAX_FRAME, elapsed);

  // Count frames for the FPS counter, updating it twice a second. This uses
  // the real time between frames, not the capped time, so slow frames show
  // their true rate.
  fpsFrames++;
  fpsTime += elapsed;
  if (fpsTime >= 0.5) {
    fpsEl.textContent = `${Math.round(fpsFrames / fpsTime)} FPS`;
    fpsFrames = 0;
    fpsTime = 0;
  }

  for (const action of input.takeActions()) handleAction(action);
  const controls = input.read();

  // Game time stops in the menus, the garage, photo mode and when paused,
  // and runs slowly in slow motion.
  const driving = mode === 'drive' && !paused;
  const dt = !driving ? 0 : slowMotion ? realDt * SLOW_MOTION : realDt;
  if (driving && event) {
    // In an event the car is held still for the countdown, while wrecked and after the finish.
    const held = event.state !== 'running' || event.playerWrecked > 0 || wheelspin > 0;
    if (event.spec.noNitro) controls.boost = false;
    launch(event, controls.throttle);
    wheelspin = Math.max(0, wheelspin - dt);
    // After the finish, the handbrake stops the car for the cameras.
    drive(dt, event.state === 'done' ? { ...STOPPED, handbrake: true } : held ? STOPPED : controls);
    if (weaponsOn() && !held) useWeapons(dt, controls.fire);
    event.update(realDt);
    updateEvent(realDt);
  } else if (driving) {
    const waitingToStart = trial.active && trial.countdown > 0;
    if (!waitingToStart) drive(dt, controls);
    if (settings.weapons && !waitingToStart) useWeapons(dt, controls.fire);
    updateFreeRoam(dt);
    updateNpcs(dt);
    // The lap clock and countdown use real time, so slow motion can't
    // make laps look faster or the countdown drag on.
    updateTimeTrial(realDt);
  }

  if (driving) chain.update(dt);
  drawChain(realDt);
  drawBoosts();
  // The music builds up with speed, and goes all out on a race's last lap.
  music.enabled = settings.music;
  music.intensity = !driving ? 0 : event?.state === 'countdown' ? 0.2 : event ? 0.55 + 0.45 * physics.speedFraction : 0.3 + 0.5 * physics.speedFraction;
  music.update(realDt, audio.context, audio.output);
  // Speed lines at the edges of the screen when flat out, slipstreaming or boosting.
  const rush = driving && settings.speedLines ? Math.max(0, (physics.speedFraction - 0.65) / 0.35) * 0.5 + physics.draft * 0.4 + (physics.boosting ? 0.35 : 0) : 0;
  speedLinesEl.style.opacity = rush > 0.02 ? String(Math.min(0.8, rush)) : '0';
  if (rush > 0.02) speedLinesEl.style.transform = `rotate(${Math.random() * 6}deg)`;

  syncCar(dt);
  event?.sync(dt, world.isNight);
  // The orbiting camera at the finish follows the car as it stops.
  if (event?.state === 'done' && celebrated) cameraRig.orbit.target.lerp(new THREE.Vector3(physics.x, physics.y + 0.7 * cameraRig.size, physics.z), Math.min(1, realDt * 4));
  updateGhost();
  world.setStartLights(event ? event.startLights : trial.startLights);
  if (driving) carEffects(dt, controls.throttle);
  effects.update(dt);
  world.update(dt, physics.x, physics.z, carRadius());
  cameraRig.update(realDt, carPosition(), physics.heading, physics.speed, driving && physics.boosting);
  if (shake > 0.001 && driving) {
    camera.position.x += (Math.random() - 0.5) * shake;
    camera.position.y += (Math.random() - 0.5) * shake;
    shake *= Math.exp(-realDt * 8);
  }

  // HUD
  const slip = physics.airborne ? 0 : Math.min(1, Math.abs(physics.lateralSpeed) / 7);
  audio.update(physics.revs, physics.speedFraction, driving ? controls.throttle : 0, driving ? slip : 0, driving, driving && physics.limiter);
  speedEl.textContent = String(Math.round(speedInUnits(physics.speed)));
  gearEl.textContent = physics.forwardSpeed < -0.5 ? 'R' : String(physics.gear);
  gearRefused = Math.max(0, gearRefused - realDt);
  gearEl.classList.toggle('refused', gearRefused > 0);
  // The rev counter. With a manual gearbox, the red shift light says change up now.
  const revs = Math.min(1, Math.max(0, physics.revs));
  revsBar.style.width = `${Math.round(revs * 100)}%`;
  revsBar.classList.toggle('high', revs > 0.75);
  revsBar.classList.toggle('shift', physics.manual && revs > 0.92 && physics.gear < GEAR_TOPS.length);
  driftEl.textContent = driftScore >= 10 ? `DRIFT ${Math.round(driftScore).toLocaleString()}` : '';
  nitroBar.style.width = `${Math.round(physics.nitro * 100)}%`;
  nitroBar.classList.toggle('active', physics.boosting);
  boostGlow.classList.toggle('on', driving && physics.boosting);
  weaponBar.style.width = `${Math.round(weapons.readiness * 100)}%`;
  weaponBar.classList.toggle('hot', weapons.overheated);
  if (driving && settings.minimap) minimap.update(realDt, world, physics.x, physics.z, physics.heading, event ? event.mapMarkers() : [...trial.mapMarkers(), ...freeRoam.mapMarkers(physics)]);

  packs?.frame(dt, realDt);
  draw();
}

/** Draws the frame: a pack can draw it instead (the graphics pack adds its effects). */
function draw(): void {
  if (!packs?.render()) renderer.render(scene, camera);
}

/** Starts the installed packs, giving them the parts of the game they can use. */
function startPacks(): void {
  packs = new PackHost(installedPacks, buildId, {
    renderer,
    scene,
    camera,
    world,
    effects,
    settings,
    pedestrians: people,
    traffic,
    player: physics,
    playerCar: () => car,
    mode: () => mode,
    inEvent: () => event !== null,
    paused: () => mode !== 'drive' || paused,
    toast,
  }, packOverlay);
  packs.startAll();
}

// ---------- startup ----------
window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(window.innerWidth, window.innerHeight);
  updateViewOffset();
  packs?.resize(window.innerWidth, window.innerHeight);
});
// Keys the packs use (while driving, and not while typing in a box).
window.addEventListener('keydown', (e) => {
  const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
  if (!e.repeat && !typing && mode === 'drive' && !paused) packs?.key(e.code);
});
// Sound can only start after the player presses something.
for (const type of ['keydown', 'pointerdown'] as const) window.addEventListener(type, () => audio.unlock());

weaponName.textContent = `1 · ${weapons.name}`;
freeRoam.load(world, career.found);
loadNpcs();
applySettings();
applyNight();
syncCar(0);
startPacks();
setMode('menu');
menu.showMain();
requestAnimationFrame(frame);
