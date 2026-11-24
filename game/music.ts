// The game's music: a synthwave loop played live with Web Audio, no sound
// files. In the menus it's just pads and bass; driving adds drums and an
// arpeggio, and it builds up with speed. A race's final lap gets the full mix.
//
// Notes are scheduled a fraction of a second ahead, from update() each frame,
// so the beat stays steady even when frames are late.

const BPM = 108;
const STEP = 60 / BPM / 4; // seconds per sixteenth note
const LOOKAHEAD = 0.25; // seconds of notes scheduled ahead

/** Four chords, two bars each: A minor, F, C, G. Notes as semitones above A1 (55 Hz). */
const CHORDS = [
  [0, 3, 7, 12],
  [-4, 0, 3, 8],
  [3, 7, 10, 15],
  [-2, 2, 5, 10],
];
const BASS = [0, 0, 12, 0, 0, 12, 0, 7, 0, 0, 12, 0, 0, 12, 7, 10]; // the bass line's pattern over a bar
const ARP = [0, 1, 2, 3, 2, 1, 2, 3, 0, 1, 2, 3, 3, 2, 1, 2]; // which note of the chord, each step

const hz = (semitones: number) => 55 * Math.pow(2, semitones / 12);

export class Music {
  enabled = true;
  /** 0 in the menus, up to 1 flat out in a race. */
  intensity = 0;
  private ctx: AudioContext | null = null;
  private out!: GainNode;
  private noise!: AudioBuffer;
  private step = 0;
  private nextAt = 0;
  private level = 0; // intensity, smoothed

  /**
   * Call every frame with the sound system (once it has started) and the
   * volume control to play through.
   */
  update(dt: number, ctx: AudioContext | null, destination: GainNode | null): void {
    if (!ctx || !destination) return;
    if (!this.ctx) this.start(ctx, destination);
    this.level += (this.intensity - this.level) * Math.min(1, dt * 0.8);
    this.out.gain.setTargetAtTime(this.enabled ? 0.32 : 0, ctx.currentTime, 0.3);
    if (!this.enabled) {
      this.nextAt = ctx.currentTime + 0.1;
      return;
    }
    if (this.nextAt < ctx.currentTime) this.nextAt = ctx.currentTime + 0.05; // fell behind (tab hidden): restart the beat
    while (this.nextAt < ctx.currentTime + LOOKAHEAD) {
      this.play(this.step, this.nextAt);
      this.nextAt += STEP;
      this.step = (this.step + 1) % (16 * 8);
    }
  }

  private start(ctx: AudioContext, destination: GainNode): void {
    this.ctx = ctx;
    this.out = ctx.createGain();
    this.out.gain.value = 0;
    // A little echo makes it sound bigger.
    const delay = ctx.createDelay();
    delay.delayTime.value = STEP * 3;
    const feedback = ctx.createGain();
    feedback.gain.value = 0.3;
    const wet = ctx.createGain();
    wet.gain.value = 0.25;
    this.out.connect(destination);
    this.out.connect(delay).connect(feedback).connect(delay);
    delay.connect(wet).connect(destination);
    this.noise = ctx.createBuffer(1, ctx.sampleRate / 2, ctx.sampleRate);
    const data = this.noise.getChannelData(0);
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    this.nextAt = ctx.currentTime + 0.1;
  }

  /** Plays everything on one sixteenth-note step of the 8-bar loop. */
  private play(step: number, t: number): void {
    const inBar = step % 16;
    const chord = CHORDS[Math.floor(step / 32) % CHORDS.length];
    const driving = this.level > 0.15;
    // Pads: a long chord at the start of every bar.
    if (inBar === 0) for (const n of chord.slice(0, 3)) this.tone('sawtooth', hz(n + 24), t, STEP * 16, 0.035, 900, 0.6);
    // Bass, eighth notes; quieter and on the beat only in the menus.
    if (driving ? inBar % 2 === 0 : inBar % 4 === 0) this.tone('sawtooth', hz(chord[0] + BASS[inBar] - 12), t, STEP * 1.8, 0.12, 400 + this.level * 900, 0.05);
    if (!driving) return;
    // Drums: kick on the beat, snare on 2 and 4, hats on the off-beats.
    if (inBar % 4 === 0) this.kick(t);
    if (inBar === 4 || inBar === 12) this.hit(t, 'bandpass', 1800, 0.25, 0.18);
    if (inBar % 2 === 1 || this.level > 0.7) this.hit(t, 'highpass', 8000, 0.07, 0.04);
    // The arpeggio comes in at speed.
    if (this.level > 0.45) this.tone('square', hz(chord[ARP[inBar]] + 36), t, STEP * 0.9, 0.035 * this.level, 2500, 0.01);
  }

  private tone(type: OscillatorType, freq: number, t: number, length: number, volume: number, cutoff: number, attack: number): void {
    const ctx = this.ctx!;
    const osc = ctx.createOscillator();
    osc.type = type;
    osc.frequency.value = freq;
    osc.detune.value = (Math.random() - 0.5) * 12;
    const filter = ctx.createBiquadFilter();
    filter.type = 'lowpass';
    filter.frequency.value = cutoff;
    const g = ctx.createGain();
    g.gain.setValueAtTime(0.0001, t);
    g.gain.exponentialRampToValueAtTime(volume, t + attack);
    g.gain.exponentialRampToValueAtTime(0.0001, t + length);
    osc.connect(filter).connect(g).connect(this.out);
    osc.start(t);
    osc.stop(t + length + 0.05);
  }

  private kick(t: number): void {
    const ctx = this.ctx!;
    const osc = ctx.createOscillator();
    osc.frequency.setValueAtTime(140, t);
    osc.frequency.exponentialRampToValueAtTime(40, t + 0.12);
    const g = ctx.createGain();
    g.gain.setValueAtTime(0.5, t);
    g.gain.exponentialRampToValueAtTime(0.001, t + 0.3);
    osc.connect(g).connect(this.out);
    osc.start(t);
    osc.stop(t + 0.35);
  }

  private hit(t: number, type: BiquadFilterType, freq: number, length: number, volume: number): void {
    const ctx = this.ctx!;
    const src = ctx.createBufferSource();
    src.buffer = this.noise;
    const filter = ctx.createBiquadFilter();
    filter.type = type;
    filter.frequency.value = freq;
    const g = ctx.createGain();
    g.gain.setValueAtTime(volume, t);
    g.gain.exponentialRampToValueAtTime(0.001, t + length);
    src.connect(filter).connect(g).connect(this.out);
    src.start(t, Math.random() * 0.3);
    src.stop(t + length + 0.02);
  }
}
