// Game entry point: sets up the 3D scene and runs the game loop.
import * as THREE from 'three';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { loadConfig, saveConfig, type CarConfig } from '../game/carConfig';
import { buildCar, type CarRig } from '../game/carModel';
import { CarPhysics } from '../game/carPhysics';
import { CameraRig } from '../game/cameraRig';
import { EngineAudio } from '../game/engineAudio';
import { GarageUI } from '../game/garageUI';
import { Input, type Action } from '../game/input';
import { World } from '../game/world';

const STEP = 1 / 120; // physics runs at a fixed 120 steps per second

// ---------- renderer, scene, camera ----------
const container = document.getElementById('game')!;
const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: 'high-performance' });
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
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
const world = new World(scene);
const physics = new CarPhysics();
const input = new Input();
const audio = new EngineAudio();

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
const hud = document.getElementById('hud')!;
const speedEl = document.getElementById('speed')!;
const gearEl = document.getElementById('gear')!;
const driftEl = document.getElementById('drift')!;
const bestEl = document.getElementById('best')!;
const helpEl = document.getElementById('help')!;
const hintEl = document.getElementById('hint')!;
const toastEl = document.getElementById('toast')!;
let toastTimer = 0;

function toast(text: string): void {
  toastEl.textContent = text;
  toastEl.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toastEl.classList.remove('show'), 1600);
}

let bestDrift = 0;
try {
  bestDrift = Number(localStorage.getItem('carGame.bestDrift')) || 0;
} catch {
  // ignore
}
function showBest(): void {
  bestEl.textContent = bestDrift > 0 ? `Best drift ${bestDrift.toLocaleString()}` : '';
}
showBest();

// ---------- garage / drive modes ----------
let mode: 'garage' | 'drive' = 'garage';

const garage = new GarageUI(
  config,
  (_c, change) => {
    if (change === 'look') rebuildCar();
    else physics.configure(config, car);
    // Save right away: a delayed save is lost if the window closes first.
    saveConfig(config);
  },
  () => setMode('drive'),
);
document.body.append(garage.root);

function carPosition(): THREE.Vector3 {
  return new THREE.Vector3(physics.x, physics.y, physics.z);
}

function setMode(next: 'garage' | 'drive'): void {
  mode = next;
  const inGarage = next === 'garage';
  if (inGarage) {
    // Park the car where it is.
    physics.vx = physics.vz = physics.vy = 0;
    physics.y = world.heightAt(physics.x, physics.z);
    physics.airborne = false;
  }
  garage.setVisible(inGarage);
  hud.classList.toggle('hidden', inGarage);
  hintEl.classList.toggle('hidden', inGarage);
  if (inGarage) helpEl.classList.add('hidden');
  cameraRig.setGarage(inGarage, carPosition(), physics.heading);
  updateViewOffset();
  (document.activeElement as HTMLElement | null)?.blur();
}

/** In the garage, shift the view so the car sits left of the panel. */
function updateViewOffset(): void {
  const w = window.innerWidth;
  const h = window.innerHeight;
  if (mode === 'garage' && w > 820) camera.setViewOffset(w, h, Math.min(380, w * 0.3) / 2, 0, w, h);
  else camera.clearViewOffset();
}

function handleAction(action: Action): void {
  switch (action) {
    case 'garage':
      setMode(mode === 'garage' ? 'drive' : 'garage');
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
        physics.reset(world.spawn.x, world.spawn.z, world.spawn.heading);
        cameraRig.snap(carPosition(), physics.heading);
        driftScore = 0;
      }
      break;
    case 'help':
      if (mode === 'drive') helpEl.classList.toggle('hidden');
      break;
    case 'fullscreen':
      if (document.fullscreenElement) void document.exitFullscreen();
      else void document.documentElement.requestFullscreen().catch(() => toast('Fullscreen not available'));
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

let accumulator = 0;
let lastTime = performance.now();

function frame(): void {
  const now = performance.now();
  const dt = Math.min(0.05, (now - lastTime) / 1000);
  lastTime = now;
  for (const action of input.takeActions()) handleAction(action);

  const controls = input.read();
  let slip = 0;
  if (mode === 'drive') {
    accumulator += dt;
    let steps = 0;
    while (accumulator >= STEP && steps < 10) {
      const wasAirborne = physics.airborne;
      const ev = physics.step(STEP, controls, world);
      accumulator -= STEP;
      steps++;
      if (physics.airborne) airTime += STEP;
      else if (wasAirborne) {
        if (airTime > 0.7) toast(`Air time ${airTime.toFixed(1)} s`);
        airTime = 0;
      }
      if (ev.impact > 3) {
        audio.crash(ev.impact);
        shake = Math.min(0.6, ev.impact * 0.03);
        if (driftScore > 0) {
          driftScore = 0;
          toast('Crashed! Drift lost');
        }
      }
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
    } else if (driftTimer > 0) {
      driftTimer -= dt;
      if (driftTimer <= 0) bankDrift();
    }
  }

  syncCar(dt);
  world.update(dt, physics.x, physics.z);
  cameraRig.update(dt, carPosition(), physics.heading, physics.speed);
  if (shake > 0.001) {
    camera.position.x += (Math.random() - 0.5) * shake;
    camera.position.y += (Math.random() - 0.5) * shake;
    shake *= Math.exp(-dt * 8);
  }

  audio.update(physics.speedFraction, mode === 'drive' ? controls.throttle : 0, slip, mode === 'drive');
  speedEl.textContent = String(Math.round(physics.speed * 3.6));
  gearEl.textContent = physics.forwardSpeed < -0.5 ? 'R' : String(audio.gear);
  driftEl.textContent = driftScore >= 10 ? `DRIFT ${Math.round(driftScore).toLocaleString()}` : '';

  renderer.render(scene, camera);
  requestAnimationFrame(frame);
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

syncCar(0);
setMode('garage');
requestAnimationFrame(frame);
