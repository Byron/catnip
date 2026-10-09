//! Sound synthesis and playback. Clips are generated as sample buffers at
//! start-up (no asset files) and mixed by rodio.

use std::num::NonZero;

use rodio::{MixerDeviceSink, buffer::SamplesBuffer};

pub const RATE: u32 = 44_100;

/// Mono clips, indexed by the order they were given to `open`.
pub struct Speaker {
    sink: MixerDeviceSink,
    clips: Vec<SamplesBuffer>,
}

impl Speaker {
    /// None when there is no usable output device; callers run silent then.
    pub fn open(clips: Vec<Vec<f32>>) -> Option<Self> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
        sink.log_on_drop(false);
        let clips = clips
            .into_iter()
            .map(|samples| {
                SamplesBuffer::new(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap(), samples)
            })
            .collect();
        Some(Self { sink, clips })
    }

    pub fn play(&self, clip: usize) {
        if let Some(buffer) = self.clips.get(clip) {
            self.sink.mixer().add(buffer.clone());
        }
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
pub fn sweep(
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
pub fn rumble(seconds: f32, bright: f32, dark: f32, volume: f32, seed: u32) -> Vec<f32> {
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

pub fn mix(a: Vec<f32>, b: Vec<f32>) -> Vec<f32> {
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| a.get(i).unwrap_or(&0.0) + b.get(i).unwrap_or(&0.0))
        .collect()
}

pub fn concat(parts: &[Vec<f32>]) -> Vec<f32> {
    parts.concat()
}

/// A plucked tone: triangle wave with a quick decay.
pub fn note(freq: f32, seconds: f32, volume: f32) -> Vec<f32> {
    sweep(seconds, freq, freq, volume, 1.8, triangle)
}

pub fn square(p: f32) -> f32 {
    if p < 0.5 { 1.0 } else { -1.0 }
}

pub fn saw(p: f32) -> f32 {
    p * 2.0 - 1.0
}

pub fn sine(p: f32) -> f32 {
    (p * std::f32::consts::TAU).sin()
}

pub fn triangle(p: f32) -> f32 {
    1.0 - 4.0 * (p - 0.5).abs()
}
