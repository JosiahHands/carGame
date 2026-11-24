// 🌅 Photo Real: makes the game look real. A physically based sky with
// drifting clouds (and stars at night) that also lights and reflects in
// everything, soft shadows in every corner (ambient occlusion), glowing
// lights, filmic colour, detailed ground that gets wet in the rain, street
// lamps and headlights that really light the road, sculpted car bodies (see
// carBody.ts) with metal-flake paint, and a lens with grain, vignette and
// speed blur.
import * as THREE from 'three';
import type { GameApi, Pack, PackInstance } from '../../game/packs';
import { SkySystem, SUN_HEIGHT, weatherFor, type TimeOfDay } from './sky';
import { PostFX, type Look, type Quality } from './post';
import { NightLights } from './lights';
import { sculptCar, unsculptCars } from './carBody';
import { detailGround, dressCar, groundUniforms, sharpenTextures, undoGround, undressCars } from './surfaces';

type Tone = 'agx' | 'aces' | 'neutral';

interface Options {
  quality: Quality;
  time: TimeOfDay;
  tone: Tone;
  look: Look;
  exposure: number;
  vignette: number;
  grain: number;
  aberration: number;
  speedBlur: number;
}

const DEFAULTS: Options = {
  quality: 'high',
  time: 'morning',
  tone: 'agx',
  look: 'natural',
  exposure: 1,
  vignette: 0.4,
  grain: 0.25,
  aberration: 0.35,
  speedBlur: 0.5,
};

const TONES: Record<Tone, THREE.ToneMapping> = {
  agx: THREE.AgXToneMapping,
  aces: THREE.ACESFilmicToneMapping,
  neutral: THREE.NeutralToneMapping,
};

/** The world's lights and fog as the game set them, before this pack changes them. */
interface Base {
  sunIntensity: number;
  sunColor: THREE.Color;
  hemiIntensity: number;
  fogColor: THREE.Color | null;
  environment: THREE.Texture | null;
  environmentIntensity: number;
}

const pack: Pack = {
  id: 'graphics',
  name: 'Photo Real',
  icon: '🌅',
  about:
    'Makes the game look real: a true sky with drifting clouds and stars that lights and reflects in everything, soft shadows in every corner, ' +
    'glowing lights, filmic colour, detailed roads that shine in the rain, street lamps and headlights that light the road, and a real camera lens. ' +
    'Every car gets a sculpted body with curved glass, pillars, mirrors and wheel wells, real tyres, brake discs and metal-flake paint, shaped by your garage settings.',
  activate(game: GameApi): PackInstance {
    const { renderer, scene, camera, world } = game;
    const options: Options = { ...DEFAULTS, ...game.load<Partial<Options>>('options', {}) };
    const saved = { toneMapping: renderer.toneMapping, exposure: renderer.toneMappingExposure };

    // The world's sun and sky light (found in the scene: the world keeps them to itself).
    const sun = scene.children.find((c): c is THREE.DirectionalLight => (c as THREE.DirectionalLight).isDirectionalLight);
    const hemi = scene.children.find((c): c is THREE.HemisphereLight => (c as THREE.HemisphereLight).isHemisphereLight);
    // The game's own environment map (a studio), put back when the pack is switched off.
    const base: Base = {
      sunIntensity: 0, sunColor: new THREE.Color(), hemiIntensity: 0, fogColor: null,
      environment: scene.environment, environmentIntensity: scene.environmentIntensity,
    };
    /** Notes the game's lights and fog, just after the game has set them (a new map, or day/night). */
    const captureBase = () => {
      base.sunIntensity = sun?.intensity ?? 0;
      base.sunColor.copy(sun?.color ?? new THREE.Color());
      base.hemiIntensity = hemi?.intensity ?? 0;
      base.fogColor = (scene.fog as THREE.Fog | null)?.color.clone() ?? null;
    };

    const sky = new SkySystem(renderer);
    sky.attach(scene);
    const lights = new NightLights(scene);
    let post: PostFX | null = null;
    let skyEnv: THREE.Texture | null = null;
    let skyKey = '';
    let time = 0;
    const anisotropy = renderer.capabilities.getMaxAnisotropy();
    const sunDirection = new THREE.Vector3();
    captureBase();

    /** Where the sun is: the time of day sets its height; it keeps the game's compass direction, so shadows fall the same way. */
    const placeSun = () => {
      const height = THREE.MathUtils.degToRad(SUN_HEIGHT[options.time]);
      const around = Math.atan2(40, 25); // the game's sun is at (+40, +80, +25) from the car
      sunDirection.set(Math.sin(around) * Math.cos(height), Math.sin(height), Math.cos(around) * Math.cos(height));
    };

    /** A new sky and environment map, when the venue, weather, time or day/night changes. */
    const refreshSky = () => {
      placeSun();
      const theme = world.theme;
      const night = world.isNight;
      const key = `${world.map}/${night}/${options.time}/${theme.weather}`;
      if (key === skyKey) return;
      skyKey = key;
      const ground = new THREE.Color(theme.ground === 'sand' ? 0xc9a476 : theme.ground === 'snow' ? 0xe8eef4 : theme.ground === 'dirt' ? 0x6b5a45 : 0x4f6b3a);
      skyEnv = sky.update(theme, night, sunDirection, ground);
      // Rain soaks the ground; snow leaves it damp.
      groundUniforms.wetness.value = theme.weather === 'rain' ? 1 : theme.weather === 'snow' ? 0.35 : 0;
    };

    /** Lights, fog and exposure for this frame (worked out from the game's own values, so nothing builds up). */
    const applyLighting = () => {
      const night = world.isNight;
      const weather = weatherFor(world.theme);
      scene.environment = skyEnv;
      // The sky now lights everything, so the flat sky light is turned down to match.
      scene.environmentIntensity = night ? 0.4 : 0.5;
      if (hemi) hemi.intensity = base.hemiIntensity * (night ? 0.8 : 0.55);
      if (sun) {
        sun.intensity = base.sunIntensity * (night ? 1 : weather.sun * (options.time === 'golden' ? 0.85 : 1));
        sun.color.copy(base.sunColor);
        if (!night && options.time === 'golden') sun.color.lerp(new THREE.Color(0xffb070), 0.6);
        if (!night && options.time === 'morning') sun.color.lerp(new THREE.Color(0xfff0dc), 0.4);
        // Overcast light is soft: the shadows blur out.
        sun.shadow.radius = weather.clouds > 0.7 ? 6 : 2.5;
        sun.shadow.blurSamples = 12;
        // The sun goes where the sky shows it (the game moves it with the car first).
        if (!night) sun.position.copy(sun.target.position).addScaledVector(sunDirection, 100);
      }
      const fog = scene.fog as THREE.Fog | null;
      if (fog && base.fogColor) {
        fog.color.copy(base.fogColor);
        // The haze glows warm at golden hour.
        if (!night && options.time === 'golden') fog.color.lerp(new THREE.Color(0xf2c39a), 0.45);
      }
      renderer.toneMapping = TONES[options.tone];
      renderer.toneMappingExposure = options.exposure * (night ? 1.1 : 0.95);
    };

    const buildPost = () => {
      post?.dispose();
      post = new PostFX(renderer, scene, camera, options.quality);
    };

    const save = (rebuild = false) => {
      game.save('options', options);
      if (rebuild) buildPost();
      skyKey = '';
      refreshSky();
    };

    // The map that's already loaded, and the player's car as it is now.
    detailGround(world.group);
    sharpenTextures(world.group, anisotropy);
    sculptCar(game.playerCar());
    dressCar(game.playerCar(), (x, z) => world.heightAt(x, z));
    refreshSky();
    buildPost();

    return {
      mapLoaded() {
        captureBase();
        detailGround(world.group);
        sharpenTextures(world.group, anisotropy);
        refreshSky();
      },
      nightChanged() {
        captureBase();
        base.environmentIntensity = scene.environmentIntensity; // the game sets it for day or night
        refreshSky();
      },
      carBuilt(rig) {
        sculptCar(rig);
        dressCar(rig, (x, z) => world.heightAt(x, z));
        sharpenTextures(rig.root, anisotropy);
      },
      frame(_dt, realDt) {
        time += realDt;
        const night = world.isNight;
        refreshSky();
        applyLighting();
        sky.frame(realDt, camera, night);
        const mode = game.mode();
        lights.frame(realDt, world, game.player, camera, night, mode === 'drive');
        const speed = mode === 'drive' ? Math.min(1, game.player.speed / 55) : 0;
        const focus = mode === 'photo' ? camera.position.distanceTo(new THREE.Vector3(game.player.x, game.player.y + 0.8, game.player.z)) : null;
        post?.set(options, night, speed, focus, time);
      },
      render() {
        if (!post) return false;
        post.render();
        return true;
      },
      settings(panel) {
        panel.choice('Quality', [
          { value: 'medium', label: 'Medium' },
          { value: 'high', label: 'High' },
          { value: 'ultra', label: 'Ultra' },
        ], () => options.quality, (v) => ((options.quality = v), save(true)));
        panel.hint('High adds soft shadows in every corner (ambient occlusion). Ultra draws them at full resolution: it needs a strong graphics card.');
        panel.choice('Time of day', [
          { value: 'morning', label: 'Morning' },
          { value: 'noon', label: 'Noon' },
          { value: 'golden', label: 'Golden hour' },
        ], () => options.time, (v) => ((options.time = v), save()));
        panel.choice('Colour', [
          { value: 'natural', label: 'Natural' },
          { value: 'film', label: 'Film' },
          { value: 'vivid', label: 'Vivid' },
          { value: 'noir', label: 'Noir' },
        ], () => options.look, (v) => ((options.look = v), save()));
        panel.choice('Tone', [
          { value: 'agx', label: 'Filmic' },
          { value: 'aces', label: 'Punchy' },
          { value: 'neutral', label: 'Neutral' },
        ], () => options.tone, (v) => ((options.tone = v), save()));
        const percent = (v: number) => `${Math.round(v * 100)}%`;
        panel.slider('Exposure', 0.6, 1.8, 0.05, () => options.exposure, (v) => ((options.exposure = v), save()), (v) => v.toFixed(2));
        panel.slider('Speed blur', 0, 1, 0.05, () => options.speedBlur, (v) => ((options.speedBlur = v), save()), percent);
        panel.slider('Vignette', 0, 1, 0.05, () => options.vignette, (v) => ((options.vignette = v), save()), percent);
        panel.slider('Film grain', 0, 1, 0.05, () => options.grain, (v) => ((options.grain = v), save()), percent);
        panel.slider('Lens fringing', 0, 1, 0.05, () => options.aberration, (v) => ((options.aberration = v), save()), percent);
        panel.hint('Photo mode focuses on your car and blurs the background, like a real camera.');
      },
      dispose() {
        post?.dispose();
        post = null;
        sky.dispose();
        lights.dispose();
        undoGround();
        undressCars(scene);
        unsculptCars(scene);
        renderer.toneMapping = saved.toneMapping;
        renderer.toneMappingExposure = saved.exposure;
        scene.environment = base.environment;
        scene.environmentIntensity = base.environmentIntensity;
        if (hemi) hemi.intensity = base.hemiIntensity;
        if (sun) {
          sun.intensity = base.sunIntensity;
          sun.color.copy(base.sunColor);
          sun.shadow.radius = 1;
        }
        if (scene.fog && base.fogColor) (scene.fog as THREE.Fog).color.copy(base.fogColor);
        skyEnv?.dispose();
      },
    };
  },
};

export default pack;
