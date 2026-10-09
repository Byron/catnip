//! Sound effects synthesized at start-up (no asset files) and mixed by rodio.

use std::num::NonZero;

use rodio::{MixerDeviceSink, buffer::SamplesBuffer};

const RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Shoot,
    Hit,
    Explode,
    BigExplode,
    Hurt,
    Pickup,
    Dash,
    Wave,
    EnemyShoot,
    GameOver,
}

const ALL: [Sfx; 10] = [
    Sfx::Shoot,
    Sfx::Hit,
    Sfx::Explode,
    Sfx::BigExplode,
    Sfx::Hurt,
    Sfx::Pickup,
    Sfx::Dash,
    Sfx::Wave,
    Sfx::EnemyShoot,
    Sfx::GameOver,
];

pub struct Audio {
    sink: MixerDeviceSink,
    sounds: Vec<SamplesBuffer>,
}

impl Audio {
    /// None when there is no usable output device; the game runs silent then.
    pub fn open() -> Option<Self> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
        sink.log_on_drop(false);
        let sounds = ALL
            .iter()
            .map(|&sfx| {
                SamplesBuffer::new(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap(), synth(sfx))
            })
            .collect();
        Some(Self { sink, sounds })
    }

    pub fn play(&self, sfx: Sfx) {
        let index = ALL
            .iter()
            .position(|&s| s == sfx)
            .expect("every Sfx is in ALL");
        self.sink.mixer().add(self.sounds[index].clone());
    }
}

/// Tiny deterministic noise source.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

fn samples(seconds: f32) -> usize {
    (seconds * RATE as f32) as usize
}

/// Linear attack, exponential-ish decay. Avoids clicks at both ends.
fn envelope(i: usize, n: usize, attack: f32, curve: f32) -> f32 {
    let t = i as f32 / n as f32;
    let a = (i as f32 / samples(attack).max(1) as f32).min(1.0);
    a * (1.0 - t).powf(curve)
}

/// Frequency sweep `from → to` (Hz) with a waveform shaper.
fn sweep(
    seconds: f32,
    from: f32,
    to: f32,
    volume: f32,
    curve: f32,
    wave: fn(f32) -> f32,
) -> Vec<f32> {
    let n = samples(seconds);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let freq = from * (to / from).powf(t);
            phase = (phase + freq / RATE as f32).fract();
            wave(phase) * volume * envelope(i, n, 0.003, curve)
        })
        .collect()
}

/// Noise through a one-pole low-pass whose cutoff falls over time.
fn rumble(seconds: f32, bright: f32, dark: f32, volume: f32, seed: u32) -> Vec<f32> {
    let n = samples(seconds);
    let mut noise = Noise(seed);
    let mut lp = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let k = bright + (dark - bright) * t;
            lp += (noise.next() - lp) * k;
            lp * volume * envelope(i, n, 0.002, 2.0)
        })
        .collect()
}

fn mix(a: Vec<f32>, b: Vec<f32>) -> Vec<f32> {
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| a.get(i).unwrap_or(&0.0) + b.get(i).unwrap_or(&0.0))
        .collect()
}

fn concat(parts: &[Vec<f32>]) -> Vec<f32> {
    parts.concat()
}

fn square(p: f32) -> f32 {
    if p < 0.5 { 1.0 } else { -1.0 }
}

fn saw(p: f32) -> f32 {
    p * 2.0 - 1.0
}

fn sine(p: f32) -> f32 {
    (p * std::f32::consts::TAU).sin()
}

fn triangle(p: f32) -> f32 {
    1.0 - 4.0 * (p - 0.5).abs()
}

fn synth(sfx: Sfx) -> Vec<f32> {
    match sfx {
        Sfx::Shoot => mix(
            sweep(0.08, 1400.0, 420.0, 0.07, 1.5, square),
            rumble(0.04, 0.6, 0.2, 0.05, 7),
        ),
        Sfx::Hit => rumble(0.05, 0.9, 0.4, 0.18, 11),
        Sfx::Explode => mix(
            rumble(0.45, 0.5, 0.03, 0.55, 3),
            sweep(0.35, 160.0, 40.0, 0.35, 1.5, sine),
        ),
        Sfx::BigExplode => mix(
            rumble(0.9, 0.35, 0.01, 0.7, 5),
            sweep(0.8, 110.0, 28.0, 0.5, 1.2, sine),
        ),
        Sfx::Hurt => mix(
            sweep(0.35, 300.0, 70.0, 0.3, 1.2, saw),
            rumble(0.2, 0.7, 0.1, 0.3, 13),
        ),
        Sfx::Pickup => concat(&[
            sweep(0.06, 660.0, 660.0, 0.2, 0.5, triangle),
            sweep(0.06, 880.0, 880.0, 0.2, 0.5, triangle),
            sweep(0.12, 1320.0, 1320.0, 0.2, 1.5, triangle),
        ]),
        Sfx::Dash => mix(
            rumble(0.18, 0.08, 0.5, 0.3, 17),
            sweep(0.15, 200.0, 900.0, 0.06, 1.0, sine),
        ),
        Sfx::Wave => concat(&[
            sweep(0.12, 523.0, 523.0, 0.18, 0.8, triangle),
            sweep(0.12, 659.0, 659.0, 0.18, 0.8, triangle),
            sweep(0.3, 784.0, 784.0, 0.2, 1.5, triangle),
        ]),
        Sfx::EnemyShoot => sweep(0.12, 520.0, 180.0, 0.08, 1.5, triangle),
        Sfx::GameOver => concat(&[
            sweep(0.25, 392.0, 380.0, 0.22, 0.6, saw),
            sweep(0.25, 330.0, 318.0, 0.22, 0.6, saw),
            sweep(0.7, 262.0, 200.0, 0.25, 1.4, saw),
        ]),
    }
}
