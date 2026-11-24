// Synthesised engine, tyre squeal, horn and crash sounds (Web Audio, no sound files).
import type { EngineSound, Horn } from './carConfig';

/**
 * What each engine sounds like: the two oscillator waveforms, how high the
 * note is compared to the V8, and how loud it is.
 */
const ENGINES: Record<EngineSound, { a: OscillatorType; b: OscillatorType; pitch: number; volume: number }> = {
  v8: { a: 'sawtooth', b: 'square', pitch: 1, volume: 1 },
  v12: { a: 'sawtooth', b: 'sawtooth', pitch: 1.6, volume: 0.9 },
  diesel: { a: 'square', b: 'sawtooth', pitch: 0.55, volume: 1.1 },
  electric: { a: 'sine', b: 'triangle', pitch: 3.2, volume: 0.8 },
  jet: { a: 'sawtooth', b: 'triangle', pitch: 2.6, volume: 1 },
  moped: { a: 'square', b: 'square', pitch: 2.3, volume: 0.7 },
};

export class EngineAudio {
  private ctx: AudioContext | null = null;
  private master!: GainNode;
  private engineGain!: GainNode;
  private oscA!: OscillatorNode;
  private oscB!: OscillatorNode;
  private filter!: BiquadFilterNode;
  private squealGain!: GainNode;
  private noise!: AudioBuffer;
  private muted = false;
  private engine = ENGINES.v8;

  constructor() {
    try {
      this.muted = localStorage.getItem('carGame.muted') === '1';
    } catch {
      // ignore
    }
  }

  get isMuted(): boolean {
    return this.muted;
  }

  /** The sound system and its volume control, once sound has started (for the music). */
  get context(): AudioContext | null {
    return this.ctx;
  }
  get output(): GainNode | null {
    return this.ctx ? this.master : null;
  }

  /** Browsers only allow sound after a key press or click. */
  unlock(): void {
    if (this.ctx) {
      if (this.ctx.state === 'suspended') void this.ctx.resume();
      return;
    }
    const ctx = new AudioContext();
    this.ctx = ctx;
    this.master = ctx.createGain();
    this.master.gain.value = this.muted ? 0 : 0.5;
    this.master.connect(ctx.destination);

    this.filter = ctx.createBiquadFilter();
    this.filter.type = 'lowpass';
    this.filter.frequency.value = 900;
    this.engineGain = ctx.createGain();
    this.engineGain.gain.value = 0;
    this.filter.connect(this.engineGain).connect(this.master);
    this.oscA = ctx.createOscillator();
    this.oscA.type = 'sawtooth';
    this.oscB = ctx.createOscillator();
    this.oscB.type = 'square';
    const bGain = ctx.createGain();
    bGain.gain.value = 0.4;
    this.oscA.connect(this.filter);
    this.oscB.connect(bGain).connect(this.filter);
    this.oscA.start();
    this.oscB.start();
    this.setEngine(this.engineName);

    this.noise = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
    const data = this.noise.getChannelData(0);
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    const squeal = ctx.createBufferSource();
    squeal.buffer = this.noise;
    squeal.loop = true;
    const band = ctx.createBiquadFilter();
    band.type = 'bandpass';
    band.frequency.value = 1800;
    band.Q.value = 6;
    this.squealGain = ctx.createGain();
    this.squealGain.gain.value = 0;
    squeal.connect(band).connect(this.squealGain).connect(this.master);
    squeal.start();
  }

  private engineName: EngineSound = 'v8';

  /** Changes the engine sound (works before sound has started, too). */
  setEngine(name: EngineSound): void {
    this.engineName = name;
    this.engine = ENGINES[name];
    if (!this.ctx) return;
    this.oscA.type = this.engine.a;
    this.oscB.type = this.engine.b;
  }

  toggleMute(): boolean {
    this.muted = !this.muted;
    try {
      localStorage.setItem('carGame.muted', this.muted ? '1' : '0');
    } catch {
      // ignore
    }
    if (this.ctx) this.master.gain.setTargetAtTime(this.muted ? 0 : 0.5, this.ctx.currentTime, 0.05);
    return this.muted;
  }

  /**
   * Call every frame. revs: 0 (idle) to 1 (redline), from the gearbox;
   * speedFraction 0..1 of top speed; throttle 0..1; slip 0..1 (tyre squeal);
   * limiter: bouncing off the rev limiter, which makes the note stutter.
   */
  update(revs: number, speedFraction: number, throttle: number, slip: number, active: boolean, limiter = false): void {
    if (!this.ctx) return;
    let rpm = 900 + Math.max(0, Math.min(1.05, revs)) * 6300;
    if (limiter) rpm *= 0.94 + Math.random() * 0.08;
    const freq = (rpm / 30) * this.engine.pitch;
    const t = this.ctx.currentTime;
    this.oscA.frequency.setTargetAtTime(freq, t, 0.05);
    this.oscB.frequency.setTargetAtTime(freq / 2, t, 0.05);
    this.filter.frequency.setTargetAtTime((500 + throttle * 1400 + speedFraction * 800) * Math.max(1, this.engine.pitch * 0.7), t, 0.08);
    this.engineGain.gain.setTargetAtTime((active ? 0.12 + throttle * 0.12 : 0.05) * this.engine.volume, t, 0.1);
    this.squealGain.gain.setTargetAtTime(active ? Math.min(0.25, slip * 0.3) : 0, t, 0.05);
  }

  /** A short burst of filtered noise, the building block of most effects. */
  private noiseBurst(type: BiquadFilterType, freq: number, volume: number, length: number, q = 1): void {
    if (!this.ctx) return;
    const ctx = this.ctx;
    const src = ctx.createBufferSource();
    src.buffer = this.noise;
    // Start somewhere random in the noise so repeated shots don't sound identical.
    const offset = Math.random() * 0.5;
    const filter = ctx.createBiquadFilter();
    filter.type = type;
    filter.frequency.value = freq;
    filter.Q.value = q;
    const g = ctx.createGain();
    g.gain.setValueAtTime(volume, ctx.currentTime);
    g.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + length);
    src.connect(filter).connect(g).connect(this.master);
    src.start(ctx.currentTime, offset);
    src.stop(ctx.currentTime + length + 0.05);
  }

  /** A falling tone, for booms and whooshes. */
  private sweep(type: OscillatorType, from: number, to: number, volume: number, length: number): void {
    if (!this.ctx) return;
    const ctx = this.ctx;
    const osc = ctx.createOscillator();
    osc.type = type;
    osc.frequency.setValueAtTime(from, ctx.currentTime);
    osc.frequency.exponentialRampToValueAtTime(to, ctx.currentTime + length);
    const g = ctx.createGain();
    g.gain.setValueAtTime(volume, ctx.currentTime);
    g.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + length);
    osc.connect(g).connect(this.master);
    osc.start();
    osc.stop(ctx.currentTime + length + 0.05);
  }

  /** A sharp crack: high hiss plus a low thump. */
  gunshot(): void {
    this.noiseBurst('highpass', 900, 0.22, 0.07);
    this.noiseBurst('lowpass', 400, 0.18, 0.09);
  }

  /** A whoosh with a falling buzz. */
  rocketLaunch(): void {
    this.noiseBurst('bandpass', 1200, 0.4, 0.5, 0.8);
    this.sweep('sawtooth', 300, 90, 0.06, 0.4);
  }

  /** An explosion. `distance` in metres makes far-away booms quieter. */
  explosion(size: number, distance: number): void {
    const near = 1 / (1 + distance / 40);
    this.noiseBurst('lowpass', 500, Math.min(1, 0.8 * size * near), 1.4);
    this.sweep('sine', 120, 30, Math.min(0.8, 0.5 * size * near), 0.9);
  }

  /** A deep falling boom. */
  shockwave(): void {
    this.sweep('sine', 180, 25, 0.7, 1.1);
    this.noiseBurst('lowpass', 800, 0.6, 0.8);
  }

  /** A two-note chime for a checkpoint, or three notes for the finish line. */
  chime(finish = false): void {
    if (!this.ctx) return;
    const notes = finish ? [660, 880, 1320] : [880, 1320];
    for (let i = 0; i < notes.length; i++) {
      // Each note starts 0.09 s after the one before and fades out over 0.35 s.
      const start = this.ctx.currentTime + i * 0.09;
      const osc = this.ctx.createOscillator();
      osc.type = 'triangle';
      osc.frequency.value = notes[i];
      const gain = this.ctx.createGain();
      gain.gain.setValueAtTime(0.0001, this.ctx.currentTime);
      gain.gain.setValueAtTime(0.25, start);
      gain.gain.exponentialRampToValueAtTime(0.001, start + 0.35);
      osc.connect(gain).connect(this.master);
      osc.start(start);
      osc.stop(start + 0.4);
    }
  }

  /** Plays notes: each is [frequency, start time, length], in seconds from now. */
  private notes(type: OscillatorType, notes: [number, number, number][], volume: number): void {
    if (!this.ctx) return;
    const now = this.ctx.currentTime;
    for (const [freq, start, length] of notes) {
      const osc = this.ctx.createOscillator();
      osc.type = type;
      osc.frequency.value = freq;
      const gain = this.ctx.createGain();
      // A quick fade in and out, so the notes don't click.
      gain.gain.setValueAtTime(0.0001, now + start);
      gain.gain.exponentialRampToValueAtTime(volume, now + start + 0.02);
      gain.gain.setValueAtTime(volume, now + start + length - 0.05);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + start + length);
      osc.connect(gain).connect(this.master);
      osc.start(now + start);
      osc.stop(now + start + length + 0.05);
    }
  }

  /** Another driver's horn: two short, quieter beeps. */
  trafficHorn(): void {
    this.notes('square', [[440, 0, 0.18], [440, 0.26, 0.3]], 0.035);
  }

  horn(kind: Horn): void {
    switch (kind) {
      case 'classic': // two notes together, like most car horns
        this.notes('square', [[415, 0, 0.45], [523, 0, 0.45]], 0.07);
        break;
      case 'truck': // a loud three-note air horn chord
        this.notes('sawtooth', [[220, 0, 0.9], [277, 0, 0.9], [330, 0, 0.9]], 0.06);
        break;
      case 'train': // a low, long chord
        this.notes('sawtooth', [[155, 0, 1.4], [196, 0, 1.4], [233, 0, 1.4], [311, 0, 1.4]], 0.05);
        break;
      case 'clown': // honk honk
        this.notes('triangle', [[740, 0, 0.16], [560, 0.2, 0.22]], 0.25);
        break;
      case 'tune': // a cheerful run of notes
        this.notes('square', [[523, 0, 0.12], [659, 0.13, 0.12], [784, 0.26, 0.12], [659, 0.39, 0.12], [1047, 0.52, 0.3]], 0.06);
        break;
    }
  }

  crash(strength: number): void {
    if (!this.ctx || strength < 2) return;
    const ctx = this.ctx;
    const src = ctx.createBufferSource();
    src.buffer = this.noise;
    const lp = ctx.createBiquadFilter();
    lp.type = 'lowpass';
    lp.frequency.value = 700;
    const g = ctx.createGain();
    const vol = Math.min(0.9, strength / 20);
    g.gain.setValueAtTime(vol, ctx.currentTime);
    g.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.4);
    src.connect(lp).connect(g).connect(this.master);
    src.start();
    src.stop(ctx.currentTime + 0.45);
  }
}
