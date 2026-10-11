//! Sound synthesis and playback. Clips are generated as sample buffers at
//! start-up (no asset files) and mixed by rodio.

use std::{num::NonZero, time::Duration};

use rodio::{
    MixerDeviceSink, Source,
    buffer::SamplesBuffer,
    mixer::Mixer,
    source::{LimitSettings, Zero},
};

pub const RATE: u32 = 44_100;

/// Mono clips, indexed by the order they were given to `open`.
pub struct Speaker {
    _sink: MixerDeviceSink,
    mixer: Mixer,
    clips: Vec<SamplesBuffer>,
}

impl Speaker {
    /// None when there is no usable output device; callers run silent then.
    pub fn open(clips: Vec<Vec<f32>>) -> Option<Self> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
        sink.log_on_drop(false);
        let (mixer, source) = rodio::mixer::mixer(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap());
        // Keep the limited master source alive between clips.
        mixer.add(Zero::new(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap()));
        sink.mixer().add(limit_mix(source));
        let clips = clips
            .into_iter()
            .map(|samples| {
                SamplesBuffer::new(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap(), samples)
            })
            .collect();
        Some(Self {
            _sink: sink,
            mixer,
            clips,
        })
    }

    pub fn play(&self, clip: usize) {
        if let Some(buffer) = self.clips.get(clip) {
            self.mixer.add(buffer.clone());
        }
    }
}

fn limit_mix(source: impl Source + Send) -> impl Source + Send {
    source.limit(
        LimitSettings::default()
            .with_threshold(-6.0)
            .with_attack(Duration::ZERO),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_limiter_controls_overlapping_clips_and_recovers_after_silence() {
        let (mixer, source) = rodio::mixer::mixer(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap());
        let mut output = limit_mix(source);
        let quiet = vec![0.1, -0.1, 0.0];
        mixer.add(SamplesBuffer::new(
            NonZero::<u16>::MIN,
            NonZero::new(RATE).unwrap(),
            quiet.clone(),
        ));
        assert_eq!(output.by_ref().take(3).collect::<Vec<_>>(), quiet);
        for _ in 0..40 {
            mixer.add(SamplesBuffer::new(
                NonZero::<u16>::MIN,
                NonZero::new(RATE).unwrap(),
                vec![0.4, -0.4, 0.0],
            ));
        }
        let peak: Vec<_> = output.by_ref().take(3).collect();
        assert!(peak[0] > 0.4 && peak[1] < -0.4);
        assert!(peak.iter().all(|sample| sample.abs() <= 0.502));
        mixer.add(Zero::new_samples(
            NonZero::<u16>::MIN,
            NonZero::new(RATE).unwrap(),
            RATE as usize,
        ));
        assert!(
            output
                .by_ref()
                .take(RATE as usize)
                .all(|sample| sample == 0.0)
        );
        mixer.add(SamplesBuffer::new(
            NonZero::<u16>::MIN,
            NonZero::new(RATE).unwrap(),
            quiet.clone(),
        ));
        let recovered: Vec<_> = output.take(3).collect();
        assert!(
            recovered
                .iter()
                .zip(quiet)
                .all(|(actual, expected)| (actual - expected).abs() < 0.001)
        );
    }
}
