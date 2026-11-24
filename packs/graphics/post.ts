// The camera: everything that happens between the 3D scene and the screen.
// Ambient occlusion (soft shadows in corners and under things), bloom (bright
// lights glow), depth of field in photo mode, tone mapping, and a "lens" pass:
// colour grading, vignette, film grain, chromatic aberration and speed blur.
import * as THREE from 'three';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { GTAOPass } from 'three/addons/postprocessing/GTAOPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { BokehPass } from 'three/addons/postprocessing/BokehPass.js';
import { OutputPass } from 'three/addons/postprocessing/OutputPass.js';
import { ShaderPass } from 'three/addons/postprocessing/ShaderPass.js';
import { FXAAShader } from 'three/addons/shaders/FXAAShader.js';

export type Quality = 'medium' | 'high' | 'ultra';
export type Look = 'natural' | 'film' | 'vivid' | 'noir';
const LOOKS: Look[] = ['natural', 'film', 'vivid', 'noir'];

export interface LensSettings {
  look: Look;
  vignette: number; // 0..1
  grain: number; // 0..1
  aberration: number; // 0..1
  speedBlur: number; // 0..1 at full speed
}

/** The lens pass: works on the final picture (after tone mapping). */
const LensShader = {
  uniforms: {
    tDiffuse: { value: null as THREE.Texture | null },
    time: { value: 0 },
    look: { value: 0 },
    vignette: { value: 0.35 },
    grain: { value: 0.3 },
    aberration: { value: 0.3 },
    blur: { value: 0 },
    resolution: { value: new THREE.Vector2(1, 1) },
  },
  vertexShader: /* glsl */ `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }`,
  fragmentShader: /* glsl */ `
    uniform sampler2D tDiffuse;
    uniform float time, vignette, grain, aberration, blur;
    uniform int look;
    uniform vec2 resolution;
    varying vec2 vUv;

    // A picture sample with the colours split a little towards the edges, like a real lens.
    vec3 sampleLens(vec2 uv) {
      vec2 c = uv - 0.5;
      vec2 split = c * dot(c, c) * aberration * 0.025;
      return vec3(texture2D(tDiffuse, uv + split).r, texture2D(tDiffuse, uv).g, texture2D(tDiffuse, uv - split).b);
    }

    float hash(vec2 p) {
      return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
    }

    void main() {
      vec2 c = vUv - 0.5;
      vec3 color;
      if (blur > 0.001) {
        // Speed blur: the edges of the picture streak away from the middle, as at real speed.
        float amount = blur * 0.06 * smoothstep(0.05, 0.5, length(c));
        color = vec3(0.0);
        for (int i = 0; i < 8; i++) color += sampleLens(0.5 + c * (1.0 - amount * float(i) / 7.0));
        color /= 8.0;
      } else {
        color = sampleLens(vUv);
      }

      float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
      if (look == 0) {
        // Natural: a gentle lift in contrast and colour (the filmic tone curve is a little flat).
        color = mix(vec3(luma), color, 1.08);
        color = mix(color, color * color * (3.0 - 2.0 * color), 0.18);
      } else if (look == 1) {
        // Film: teal shadows, warm highlights, slightly faded blacks.
        color = mix(color, color * vec3(0.9, 1.02, 1.1), (1.0 - luma) * 0.45);
        color = mix(color, color * vec3(1.08, 1.0, 0.88), luma * 0.4);
        color = mix(color, color * color * (3.0 - 2.0 * color), 0.3);
        color = color * 0.96 + 0.025;
      } else if (look == 2) {
        // Vivid: rich colour and contrast.
        color = mix(vec3(luma), color, 1.3);
        color = mix(color, color * color * (3.0 - 2.0 * color), 0.35);
      } else {
        // Noir: black and white, hard contrast.
        color = vec3(luma);
        color = mix(color, color * color * (3.0 - 2.0 * color), 0.8);
      }

      // Vignette: the corners fall off, as with a real lens.
      color *= mix(1.0, smoothstep(0.95, 0.25, length(c * vec2(1.0, 0.85)) * 1.15), vignette);
      // Film grain, stronger in the shadows.
      float noise = hash(vUv * resolution + fract(time * 7.13) * 100.0) - 0.5;
      color += noise * grain * 0.07 * (1.0 - luma * 0.7);
      gl_FragColor = vec4(clamp(color, 0.0, 1.0), 1.0);
    }`,
};

export class PostFX {
  private composer: EffectComposer;
  private bloom: UnrealBloomPass;
  private bokeh: BokehPass;
  private lens: ShaderPass;
  private fxaa: ShaderPass | null = null;
  private size = new THREE.Vector2();
  private ratio = 0;

  constructor(
    private renderer: THREE.WebGLRenderer,
    scene: THREE.Scene,
    camera: THREE.PerspectiveCamera,
    readonly quality: Quality,
  ) {
    renderer.getSize(this.size);
    const ratio = renderer.getPixelRatio();
    // HDR (half float) so bright lights stay bright until the tone mapping, and
    // multisampled (MSAA) for smooth edges.
    const target = new THREE.WebGLRenderTarget(this.size.x * ratio, this.size.y * ratio, {
      type: THREE.HalfFloatType,
      samples: quality === 'medium' ? 0 : 4,
    });
    this.composer = new EffectComposer(renderer, target);
    this.composer.addPass(new RenderPass(scene, camera));

    if (quality !== 'medium') {
      // Ambient occlusion: at half resolution on High, full on Ultra.
      const gtao = new GTAOPass(scene, camera, this.size.x, this.size.y);
      const scale = quality === 'ultra' ? 1 : 0.5;
      const setSize = gtao.setSize.bind(gtao);
      gtao.setSize = (width: number, height: number) => setSize(Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale)));
      gtao.updateGtaoMaterial({ radius: 1.6, distanceExponent: 1.5, thickness: 1.5, scale: 1, samples: quality === 'ultra' ? 16 : 12 });
      gtao.updatePdMaterial({ lumaPhi: 10, depthPhi: 2, normalPhi: 3, radius: 6, rings: 2, samples: 12 });
      gtao.blendIntensity = 0.9;
      this.composer.addPass(gtao);
    }

    this.bloom = new UnrealBloomPass(this.size.clone(), 0.3, 0.5, 1);
    this.composer.addPass(this.bloom);
    // Depth of field: only in photo mode, focused on the car.
    this.bokeh = new BokehPass(scene, camera, { focus: 10, aperture: 0.004, maxblur: 0.01 });
    this.bokeh.enabled = false;
    this.composer.addPass(this.bokeh);
    this.composer.addPass(new OutputPass());
    this.lens = new ShaderPass(LensShader);
    this.composer.addPass(this.lens);
    if (quality === 'medium') {
      // No multisampling here, so smooth the edges afterwards instead.
      this.fxaa = new ShaderPass(FXAAShader);
      this.composer.addPass(this.fxaa);
    }
    this.fitToScreen();
  }

  /** Keeps every buffer the size of the screen (the window or the resolution setting changed). */
  fitToScreen(): void {
    const size = this.renderer.getSize(new THREE.Vector2());
    const ratio = this.renderer.getPixelRatio();
    if (size.equals(this.size) && ratio === this.ratio) return;
    this.size.copy(size);
    this.ratio = ratio;
    this.composer.setPixelRatio(ratio);
    this.composer.setSize(size.x, size.y);
    (this.lens.uniforms.resolution.value as THREE.Vector2).set(size.x * ratio, size.y * ratio);
    if (this.fxaa) (this.fxaa.uniforms.resolution.value as THREE.Vector2).set(1 / (size.x * ratio), 1 / (size.y * ratio));
  }

  /**
   * Settings for this frame. `speed` is 0..1 (for speed blur), `focus` the
   * distance to the car in photo mode (null for no depth of field).
   */
  set(lens: LensSettings, night: boolean, speed: number, focus: number | null, time: number): void {
    const u = this.lens.uniforms;
    u.look.value = LOOKS.indexOf(lens.look);
    u.vignette.value = lens.vignette;
    u.grain.value = lens.grain;
    u.aberration.value = lens.aberration;
    u.blur.value = lens.speedBlur * Math.max(0, speed - 0.35) / 0.65;
    u.time.value = time;
    // At night, lights glow more (and the threshold is lower, so neon and windows glow too).
    // Only really bright things glow, and only a little: lamps, lit windows and the sun.
    this.bloom.strength = night ? 0.28 : 0.1;
    this.bloom.threshold = night ? 1.0 : 1.6;
    this.bloom.radius = 0.35;
    this.bokeh.enabled = focus !== null;
    if (focus !== null) (this.bokeh.uniforms as Record<string, THREE.IUniform>).focus.value = focus;
  }

  render(): void {
    this.fitToScreen();
    this.composer.render();
  }

  dispose(): void {
    this.composer.renderTarget1.dispose();
    this.composer.renderTarget2.dispose();
    for (const pass of this.composer.passes) pass.dispose();
    this.composer.dispose();
  }
}
