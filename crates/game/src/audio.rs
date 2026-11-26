//! The game's sound, all synthesized as it plays (no sound files): the
//! engine, tire squeal, crashes, explosions, guns, horns, chimes and the
//! synthwave music. The game sets what it wants through `Sound`; a small
//! synthesizer on the sound thread makes the samples.
use std::f32::consts::TAU;
use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use bevy::audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::prelude::*;

const RATE: u32 = 44_100;
const DT: f32 = 1.0 / RATE as f32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wave {
    Sine,
    Square,
    Sawtooth,
    Triangle,
}

fn wave(w: Wave, phase: f32) -> f32 {
    match w {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Sawtooth => 2.0 * phase - 1.0,
        Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
    }
}

/// What each engine sounds like: the two waveforms, pitch compared to the V8, and loudness.
fn engine_sound(name: &str) -> (Wave, Wave, f32, f32) {
    use Wave::*;
    match name {
        "v12" => (Sawtooth, Sawtooth, 1.6, 0.9),
        "diesel" => (Square, Sawtooth, 0.55, 1.1),
        "electric" => (Sine, Triangle, 3.2, 0.8),
        "jet" => (Sawtooth, Triangle, 2.6, 1.0),
        "moped" => (Square, Square, 2.3, 0.7),
        _ => (Sawtooth, Square, 1.0, 1.0),
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FilterKind {
    Low,
    High,
    Band,
}

/// A biquad filter (the classic "audio EQ cookbook" one).
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn new(kind: FilterKind, freq: f32, q: f32) -> Self {
        let mut b = Biquad::default();
        b.set(kind, freq, q);
        b
    }

    fn set(&mut self, kind: FilterKind, freq: f32, q: f32) {
        let w = TAU * (freq.clamp(10.0, RATE as f32 * 0.45)) / RATE as f32;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q.max(0.05));
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match kind {
            FilterKind::Low => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0),
            FilterKind::High => ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0),
            FilterKind::Band => (alpha, 0.0, -alpha),
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2.0 * c / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A cheap noise source (xorshift).
#[derive(Clone, Copy)]
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[derive(Clone, Copy, Debug)]
enum Envelope {
    /// Starts at `volume`, falls exponentially to almost nothing over `length`.
    Decay { volume: f32, length: f32 },
    /// Fades in quickly to `volume`, holds, then fades out at `length`.
    Note { volume: f32, length: f32, attack: f32 },
}

/// One sound being played: a tone or noise, maybe filtered, with an envelope.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    delay: f32,
    age: f32,
    wave: Option<Wave>,
    freq: f32,
    freq_end: f32,
    sweep: f32,
    phase: f32,
    noise: bool,
    filter: Option<(FilterKind, f32, f32)>,
    biquad: Biquad,
    env: Envelope,
    music: bool,
}

impl Voice {
    fn tone(wave: Wave, freq: f32, env: Envelope) -> Self {
        Voice {
            delay: 0.0,
            age: 0.0,
            wave: Some(wave),
            freq,
            freq_end: freq,
            sweep: 0.0,
            phase: 0.0,
            noise: false,
            filter: None,
            biquad: Biquad::default(),
            env,
            music: false,
        }
    }

    fn noise(kind: FilterKind, freq: f32, q: f32, env: Envelope) -> Self {
        Voice {
            noise: true,
            wave: None,
            filter: Some((kind, freq, q)),
            biquad: Biquad::new(kind, freq, q),
            ..Voice::tone(Wave::Sine, 0.0, env)
        }
    }

    fn length(&self) -> f32 {
        match self.env {
            Envelope::Decay { length, .. } | Envelope::Note { length, .. } => length,
        }
    }

    fn gain(&self) -> f32 {
        let t = self.age;
        match self.env {
            Envelope::Decay { volume, length } => volume * (0.001f32 / volume.max(0.001)).powf((t / length).min(1.0)),
            Envelope::Note { volume, length, attack } => {
                if t < attack {
                    0.0001 * (volume / 0.0001f32).powf(t / attack)
                } else if t < length - 0.05 {
                    volume
                } else {
                    volume * (0.0001f32 / volume).powf(((t - (length - 0.05)) / 0.05).min(1.0))
                }
            }
        }
    }

    /// The next sample, or None once finished.
    fn sample(&mut self, noise: &mut Noise) -> Option<f32> {
        if self.delay > 0.0 {
            self.delay -= DT;
            return Some(0.0);
        }
        if self.age > self.length() + 0.02 {
            return None;
        }
        let mut x = 0.0;
        if let Some(w) = self.wave {
            let f = if self.sweep > 0.0 {
                self.freq * (self.freq_end / self.freq).powf((self.age / self.sweep).min(1.0))
            } else {
                self.freq
            };
            self.phase = (self.phase + f * DT).fract();
            x += wave(w, self.phase);
        }
        if self.noise {
            x += noise.next();
        }
        if self.filter.is_some() {
            x = self.biquad.run(x);
        }
        let out = x * self.gain();
        self.age += DT;
        Some(out)
    }
}

/// Values shared with the sound thread.
struct Shared {
    master: AtomicU32,
    engine_freq: AtomicU32,
    engine_cutoff: AtomicU32,
    engine_gain: AtomicU32,
    engine_jitter: AtomicBool,
    squeal_gain: AtomicU32,
    engine_kind: AtomicU32,
    music_on: AtomicBool,
    music_intensity: AtomicU32,
    new_voices: Mutex<Vec<Voice>>,
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

/// The synthesizer, as a Bevy sound.
#[derive(Asset, TypePath, Clone)]
struct Synth {
    shared: Arc<Shared>,
}

const BPM: f32 = 108.0;
const STEP: f32 = 60.0 / BPM / 4.0;
const CHORDS: [[i32; 4]; 4] = [[0, 3, 7, 12], [-4, 0, 3, 8], [3, 7, 10, 15], [-2, 2, 5, 10]];
const BASS: [i32; 16] = [0, 0, 12, 0, 0, 12, 0, 7, 0, 0, 12, 0, 0, 12, 7, 10];
const ARP: [usize; 16] = [0, 1, 2, 3, 2, 1, 2, 3, 0, 1, 2, 3, 3, 2, 1, 2];

fn hz(semitones: i32) -> f32 {
    55.0 * 2f32.powf(semitones as f32 / 12.0)
}

struct SynthPlayer {
    shared: Arc<Shared>,
    noise: Noise,
    voices: Vec<Voice>,
    // Engine
    freq: f32,
    cutoff: f32,
    gain: f32,
    squeal: f32,
    master: f32,
    phase_a: f32,
    phase_b: f32,
    engine_filter: Biquad,
    squeal_filter: Biquad,
    jitter: f32,
    // Music
    music_gain: f32,
    level: f32,
    step: usize,
    to_next_step: f32,
    delay_line: Vec<f32>,
    delay_at: usize,
    counter: u32,
}

impl SynthPlayer {
    fn music_step(&mut self) {
        let step = self.step;
        let in_bar = step % 16;
        let chord = CHORDS[(step / 32) % CHORDS.len()];
        let driving = self.level > 0.15;
        let mut add = |v: Voice, voices: &mut Vec<Voice>| voices.push(Voice { music: true, ..v });
        let tone = |wave: Wave, freq: f32, length: f32, volume: f32, cutoff: f32, attack: f32| {
            let detune = 2f32.powf((crate::rng::random() as f32 - 0.5) * 12.0 / 1200.0);
            let mut v = Voice::tone(
                wave,
                freq * detune,
                Envelope::Note {
                    volume,
                    length,
                    attack: attack.max(0.005),
                },
            );
            v.filter = Some((FilterKind::Low, cutoff, 0.7));
            v.biquad = Biquad::new(FilterKind::Low, cutoff, 0.7);
            v
        };
        if in_bar == 0 {
            for n in &chord[..3] {
                add(tone(Wave::Sawtooth, hz(n + 24), STEP * 16.0, 0.035, 900.0, 0.6), &mut self.voices);
            }
        }
        if if driving { in_bar % 2 == 0 } else { in_bar % 4 == 0 } {
            add(
                tone(
                    Wave::Sawtooth,
                    hz(chord[0] + BASS[in_bar] - 12),
                    STEP * 1.8,
                    0.12,
                    400.0 + self.level * 900.0,
                    0.05,
                ),
                &mut self.voices,
            );
        }
        if driving {
            if in_bar % 4 == 0 {
                let mut kick = Voice::tone(Wave::Sine, 140.0, Envelope::Decay { volume: 0.5, length: 0.3 });
                kick.freq_end = 40.0;
                kick.sweep = 0.12;
                add(kick, &mut self.voices);
            }
            if in_bar == 4 || in_bar == 12 {
                add(
                    Voice::noise(
                        FilterKind::Band,
                        1800.0,
                        1.0,
                        Envelope::Decay {
                            volume: 0.18,
                            length: 0.25,
                        },
                    ),
                    &mut self.voices,
                );
            }
            if in_bar % 2 == 1 || self.level > 0.7 {
                add(
                    Voice::noise(
                        FilterKind::High,
                        8000.0,
                        0.7,
                        Envelope::Decay {
                            volume: 0.04,
                            length: 0.07,
                        },
                    ),
                    &mut self.voices,
                );
            }
            if self.level > 0.45 {
                add(
                    tone(
                        Wave::Square,
                        hz(chord[ARP[in_bar]] + 36),
                        STEP * 0.9,
                        0.035 * self.level,
                        2500.0,
                        0.01,
                    ),
                    &mut self.voices,
                );
            }
        }
        self.step = (self.step + 1) % (16 * 8);
    }
}

impl Iterator for SynthPlayer {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        // Every 256 samples (about 6 ms), pick up what the game wants.
        if self.counter.is_multiple_of(256) {
            let s = &self.shared;
            if let Ok(mut new) = s.new_voices.try_lock() {
                self.voices.append(&mut new);
            }
            let block = 256.0 * DT;
            let follow = |v: &mut f32, target: f32, tau: f32| *v += (target - *v) * (1.0 - (-block / tau).exp());
            let mut freq = load(&s.engine_freq);
            if s.engine_jitter.load(Ordering::Relaxed) {
                self.jitter = 0.94 + crate::rng::random() as f32 * 0.08;
                freq *= self.jitter;
            }
            follow(&mut self.freq, freq, 0.05);
            follow(&mut self.cutoff, load(&s.engine_cutoff), 0.08);
            follow(&mut self.gain, load(&s.engine_gain), 0.1);
            follow(&mut self.squeal, load(&s.squeal_gain), 0.05);
            follow(&mut self.master, load(&s.master), 0.05);
            let on = s.music_on.load(Ordering::Relaxed);
            follow(&mut self.music_gain, if on { 0.32 } else { 0.0 }, 0.3);
            let intensity = load(&s.music_intensity);
            self.level += (intensity - self.level) * (block * 0.8).min(1.0);
            self.engine_filter.set(FilterKind::Low, self.cutoff, 1.0);
            if !on {
                self.voices.retain(|v| !v.music);
            }
        }
        self.counter = self.counter.wrapping_add(1);
        // The music's beat.
        if self.shared.music_on.load(Ordering::Relaxed) {
            self.to_next_step -= DT;
            if self.to_next_step <= 0.0 {
                self.to_next_step += STEP;
                self.music_step();
            }
        }
        // Engine: two oscillators through a low-pass filter.
        let kind = engine_sound(match self.shared.engine_kind.load(Ordering::Relaxed) {
            1 => "v12",
            2 => "diesel",
            3 => "electric",
            4 => "jet",
            5 => "moped",
            _ => "v8",
        });
        self.phase_a = (self.phase_a + self.freq * DT).fract();
        self.phase_b = (self.phase_b + self.freq / 2.0 * DT).fract();
        let engine = self
            .engine_filter
            .run(wave(kind.0, self.phase_a) + 0.4 * wave(kind.1, self.phase_b))
            * self.gain;
        let squeal = self.squeal_filter.run(self.noise.next()) * self.squeal;
        let mut fx = 0.0;
        let mut music = 0.0;
        let mut noise = self.noise;
        self.voices.retain_mut(|v| match v.sample(&mut noise) {
            Some(s) => {
                if v.music {
                    music += s;
                } else {
                    fx += s;
                }
                true
            }
            None => false,
        });
        self.noise = noise;
        // A little echo on the music makes it sound bigger.
        let len = self.delay_line.len();
        let echo = self.delay_line[self.delay_at];
        let music_out = music * self.music_gain;
        self.delay_line[self.delay_at] = music_out + echo * 0.3;
        self.delay_at = (self.delay_at + 1) % len;
        let mix = (engine + squeal + fx + music_out + echo * 0.25) * self.master;
        Some(mix.tanh())
    }
}

impl Source for SynthPlayer {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        NonZero::new(1).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl Decodable for Synth {
    type Decoder = SynthPlayer;

    fn decoder(&self) -> SynthPlayer {
        SynthPlayer {
            shared: self.shared.clone(),
            noise: Noise(0x1234_5678),
            voices: Vec::new(),
            freq: 30.0,
            cutoff: 900.0,
            gain: 0.0,
            squeal: 0.0,
            master: 0.0,
            phase_a: 0.0,
            phase_b: 0.0,
            engine_filter: Biquad::new(FilterKind::Low, 900.0, 1.0),
            squeal_filter: Biquad::new(FilterKind::Band, 1800.0, 6.0),
            jitter: 1.0,
            music_gain: 0.0,
            level: 0.0,
            step: 0,
            to_next_step: 0.1,
            delay_line: vec![0.0; (STEP * 3.0 * RATE as f32) as usize],
            delay_at: 0,
            counter: 0,
        }
    }
}

/// The game's sound: call these to make noises.
#[derive(Resource, Clone)]
pub struct Sound {
    shared: Arc<Shared>,
    muted: bool,
    engine: (Wave, Wave, f32, f32),
}

impl Sound {
    fn new(muted: bool) -> Self {
        let shared = Arc::new(Shared {
            master: AtomicU32::new(if muted { 0f32 } else { 0.5 }.to_bits()),
            engine_freq: AtomicU32::new(30f32.to_bits()),
            engine_cutoff: AtomicU32::new(900f32.to_bits()),
            engine_gain: AtomicU32::new(0),
            engine_jitter: AtomicBool::new(false),
            squeal_gain: AtomicU32::new(0),
            engine_kind: AtomicU32::new(0),
            music_on: AtomicBool::new(false),
            music_intensity: AtomicU32::new(0),
            new_voices: Mutex::new(Vec::new()),
        });
        Sound {
            shared,
            muted,
            engine: engine_sound("v8"),
        }
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        store(&self.shared.master, if self.muted { 0.0 } else { 0.5 });
        self.muted
    }

    pub fn set_muted(&mut self, muted: bool) {
        if self.muted != muted {
            self.toggle_mute();
        }
    }

    /// Changes the engine sound.
    pub fn set_engine(&mut self, name: &str) {
        self.engine = engine_sound(name);
        let kind = ["v8", "v12", "diesel", "electric", "jet", "moped"]
            .iter()
            .position(|n| *n == name)
            .unwrap_or(0);
        self.shared.engine_kind.store(kind as u32, Ordering::Relaxed);
    }

    /// Call every frame. revs 0 (idle) to 1 (redline); speed fraction, throttle and slip 0..1.
    pub fn update(&self, revs: f64, speed_fraction: f64, throttle: f64, slip: f64, active: bool, limiter: bool) {
        let rpm = 900.0 + revs.clamp(0.0, 1.05) * 6300.0;
        let (_, _, pitch, volume) = self.engine;
        let s = &self.shared;
        store(&s.engine_freq, (rpm as f32 / 30.0) * pitch);
        s.engine_jitter.store(limiter, Ordering::Relaxed);
        store(
            &s.engine_cutoff,
            (500.0 + throttle as f32 * 1400.0 + speed_fraction as f32 * 800.0) * (pitch * 0.7).max(1.0),
        );
        store(&s.engine_gain, if active { 0.12 + throttle as f32 * 0.12 } else { 0.05 } * volume);
        store(&s.squeal_gain, if active { (slip as f32 * 0.3).min(0.25) } else { 0.0 });
    }

    /// The music: on or off, and how intense (0 in the menus, up to 1 flat out in a race).
    pub fn music(&self, on: bool, intensity: f64) {
        self.shared.music_on.store(on, Ordering::Relaxed);
        store(&self.shared.music_intensity, intensity as f32);
    }

    fn play(&self, voices: impl IntoIterator<Item = Voice>) {
        if let Ok(mut list) = self.shared.new_voices.lock() {
            list.extend(voices);
            // Never pile up too many at once.
            let len = list.len();
            if len > 200 {
                list.drain(..len - 200);
            }
        }
    }

    fn burst(kind: FilterKind, freq: f32, volume: f32, length: f32, q: f32) -> Voice {
        Voice::noise(kind, freq, q, Envelope::Decay { volume, length })
    }

    fn sweep(w: Wave, from: f32, to: f32, volume: f32, length: f32) -> Voice {
        let mut v = Voice::tone(w, from, Envelope::Decay { volume, length });
        v.freq_end = to;
        v.sweep = length;
        v
    }

    fn notes(&self, w: Wave, notes: &[(f32, f32, f32)], volume: f32) {
        self.play(notes.iter().map(|&(freq, start, length)| Voice {
            delay: start,
            ..Voice::tone(
                w,
                freq,
                Envelope::Note {
                    volume,
                    length,
                    attack: 0.02,
                },
            )
        }));
    }

    /// A sharp crack: high hiss plus a low thump.
    pub fn gunshot(&self) {
        self.play([
            Self::burst(FilterKind::High, 900.0, 0.22, 0.07, 1.0),
            Self::burst(FilterKind::Low, 400.0, 0.18, 0.09, 1.0),
        ]);
    }

    /// A whoosh with a falling buzz.
    pub fn rocket_launch(&self) {
        self.play([
            Self::burst(FilterKind::Band, 1200.0, 0.4, 0.5, 0.8),
            Self::sweep(Wave::Sawtooth, 300.0, 90.0, 0.06, 0.4),
        ]);
    }

    /// An explosion; far-away booms are quieter.
    pub fn explosion(&self, size: f64, distance: f64) {
        let near = 1.0 / (1.0 + distance / 40.0);
        self.play([
            Self::burst(FilterKind::Low, 500.0, (0.8 * size * near).min(1.0) as f32, 1.4, 1.0),
            Self::sweep(Wave::Sine, 120.0, 30.0, (0.5 * size * near).min(0.8) as f32, 0.9),
        ]);
    }

    /// A deep falling boom.
    pub fn shockwave(&self) {
        self.play([
            Self::sweep(Wave::Sine, 180.0, 25.0, 0.7, 1.1),
            Self::burst(FilterKind::Low, 800.0, 0.6, 0.8, 1.0),
        ]);
    }

    /// A two-note chime for a checkpoint, or three notes for the finish line.
    pub fn chime(&self, finish: bool) {
        let notes: &[f32] = if finish { &[660.0, 880.0, 1320.0] } else { &[880.0, 1320.0] };
        self.play(notes.iter().enumerate().map(|(i, f)| Voice {
            delay: i as f32 * 0.09,
            ..Voice::tone(
                Wave::Triangle,
                *f,
                Envelope::Decay {
                    volume: 0.25,
                    length: 0.35,
                },
            )
        }));
    }

    /// Another driver's horn: two short, quieter beeps.
    pub fn traffic_horn(&self) {
        self.notes(Wave::Square, &[(440.0, 0.0, 0.18), (440.0, 0.26, 0.3)], 0.035);
    }

    pub fn horn(&self, kind: &str) {
        match kind {
            "truck" => self.notes(Wave::Sawtooth, &[(220.0, 0.0, 0.9), (277.0, 0.0, 0.9), (330.0, 0.0, 0.9)], 0.06),
            "train" => self.notes(
                Wave::Sawtooth,
                &[(155.0, 0.0, 1.4), (196.0, 0.0, 1.4), (233.0, 0.0, 1.4), (311.0, 0.0, 1.4)],
                0.05,
            ),
            "clown" => self.notes(Wave::Triangle, &[(740.0, 0.0, 0.16), (560.0, 0.2, 0.22)], 0.25),
            "tune" => self.notes(
                Wave::Square,
                &[
                    (523.0, 0.0, 0.12),
                    (659.0, 0.13, 0.12),
                    (784.0, 0.26, 0.12),
                    (659.0, 0.39, 0.12),
                    (1047.0, 0.52, 0.3),
                ],
                0.06,
            ),
            _ => self.notes(Wave::Square, &[(415.0, 0.0, 0.45), (523.0, 0.0, 0.45)], 0.07),
        }
    }

    pub fn crash(&self, strength: f64) {
        if strength < 2.0 {
            return;
        }
        self.play([Self::burst(FilterKind::Low, 700.0, (strength / 20.0).min(0.9) as f32, 0.4, 0.7)]);
    }
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Synth>().add_systems(Startup, start);
    }
}

fn start(mut commands: Commands, mut synths: ResMut<Assets<Synth>>, storage: Res<crate::storage::Storage>) {
    let muted = storage.get("carGame.muted") == Some("1");
    let sound = Sound::new(muted);
    let handle = synths.add(Synth {
        shared: sound.shared.clone(),
    });
    commands.spawn(AudioPlayer(handle));
    commands.insert_resource(sound);
}
