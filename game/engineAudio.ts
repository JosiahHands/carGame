// Synthesised engine, tyre squeal and crash sounds (Web Audio, no sound files).

const GEAR_TOPS = [0.18, 0.34, 0.52, 0.72, 1.0]; // fraction of top speed where each gear ends

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
  gear = 1;

  constructor() {
    try {
      this.muted = localStorage.getItem('carGame.muted') === '1';
    } catch {
      // ignore
    }
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

  /** speedFraction 0..1, throttle 0..1, slip 0..1 (tyre squeal). */
  update(speedFraction: number, throttle: number, slip: number, active: boolean): void {
    // Pick a gear and work out the revs within it.
    let gear = 0;
    while (gear < GEAR_TOPS.length - 1 && speedFraction > GEAR_TOPS[gear]) gear++;
    this.gear = gear + 1;
    if (!this.ctx) return;
    const lo = gear === 0 ? 0 : GEAR_TOPS[gear - 1];
    const inGear = (speedFraction - lo) / (GEAR_TOPS[gear] - lo);
    const rpm = 900 + Math.max(0, Math.min(1, inGear)) * 6000 * (gear === 0 ? 0.9 : 0.75) + (gear > 0 ? 1500 : 0);
    const freq = rpm / 30;
    const t = this.ctx.currentTime;
    this.oscA.frequency.setTargetAtTime(freq, t, 0.05);
    this.oscB.frequency.setTargetAtTime(freq / 2, t, 0.05);
    this.filter.frequency.setTargetAtTime(500 + throttle * 1400 + speedFraction * 800, t, 0.08);
    this.engineGain.gain.setTargetAtTime(active ? 0.12 + throttle * 0.12 : 0.05, t, 0.1);
    this.squealGain.gain.setTargetAtTime(active ? Math.min(0.25, slip * 0.3) : 0, t, 0.05);
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
