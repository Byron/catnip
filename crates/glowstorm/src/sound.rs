//! Glowstorm's sound effects: what each one is, and how it's synthesized.

use catnip::synth::{Speaker, concat, mix, note, rumble, saw, sine, square, sweep, triangle};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Shoot,
    Sniper,
    Plasma,
    Bfg,
    Gravity,
    Ricochet,
    Lightning,
    Blade,
    Mine,
    Flame,
    Gatling,
    Hit,
    Explode,
    BigExplode,
    Hurt,
    Pickup,
    Dash,
    Wave,
    EnemyShoot,
    GameOver,
    Overheat,
    Lunge,
    Shock,
    Siren,
    BossBlast,
    Victory,
    LevelUp,
    WaveClear,
    /// One note of the rising combo scale; higher `n` is a higher note.
    Combo(u8),
}

pub const COMBO_NOTES: u8 = 12;

const BASIC: [Sfx; 28] = [
    Sfx::Shoot,
    Sfx::Sniper,
    Sfx::Plasma,
    Sfx::Bfg,
    Sfx::Gravity,
    Sfx::Ricochet,
    Sfx::Lightning,
    Sfx::Blade,
    Sfx::Mine,
    Sfx::Flame,
    Sfx::Gatling,
    Sfx::Hit,
    Sfx::Explode,
    Sfx::BigExplode,
    Sfx::Hurt,
    Sfx::Pickup,
    Sfx::Dash,
    Sfx::Wave,
    Sfx::EnemyShoot,
    Sfx::GameOver,
    Sfx::Overheat,
    Sfx::Lunge,
    Sfx::Shock,
    Sfx::Siren,
    Sfx::BossBlast,
    Sfx::Victory,
    Sfx::LevelUp,
    Sfx::WaveClear,
];

impl Sfx {
    fn index(self) -> usize {
        match self {
            Sfx::Combo(n) => BASIC.len() + usize::from(n.min(COMBO_NOTES - 1)),
            other => BASIC
                .iter()
                .position(|&s| s == other)
                .expect("every other Sfx is in BASIC"),
        }
    }
}

pub struct Audio {
    speaker: Speaker,
}

impl Audio {
    /// None when there is no usable output device; the game runs silent then.
    pub fn open() -> Option<Self> {
        let clips = BASIC
            .into_iter()
            .chain((0..COMBO_NOTES).map(Sfx::Combo))
            .map(synth)
            .collect();
        Speaker::open(clips).map(|speaker| Self { speaker })
    }

    pub fn play(&self, sfx: Sfx) {
        self.speaker.play(sfx.index());
    }
}

fn synth(sfx: Sfx) -> Vec<f32> {
    match sfx {
        Sfx::Shoot => mix(
            sweep(0.08, 1400.0, 420.0, 0.07, 1.5, square),
            rumble(0.04, 0.6, 0.2, 0.05, 7),
        ),
        Sfx::Sniper => mix(
            sweep(0.12, 2200.0, 180.0, 0.18, 1.5, square),
            rumble(0.06, 0.7, 0.1, 0.2, 43),
        ),
        Sfx::Plasma => mix(
            sweep(0.09, 1700.0, 320.0, 0.12, 1.0, saw),
            sweep(0.06, 520.0, 100.0, 0.14, 1.0, sine),
        ),
        Sfx::Bfg => mix(
            mix(
                sweep(0.9, 230.0, 40.0, 0.35, 0.7, saw),
                sweep(0.65, 70.0, 30.0, 0.45, 1.2, sine),
            ),
            rumble(0.8, 0.4, 0.04, 0.4, 47),
        ),
        Sfx::Gravity => mix(
            sweep(0.22, 90.0, 260.0, 0.17, 1.2, sine),
            sweep(0.18, 180.0, 70.0, 0.12, 1.0, triangle),
        ),
        Sfx::Ricochet => sweep(0.10, 2100.0, 750.0, 0.13, 1.6, triangle),
        Sfx::Lightning => mix(
            rumble(0.09, 0.95, 0.6, 0.18, 51),
            sweep(0.08, 1500.0, 300.0, 0.06, 1.1, saw),
        ),
        Sfx::Blade => mix(
            sweep(0.22, 500.0, 150.0, 0.12, 1.2, saw),
            sweep(0.16, 1600.0, 420.0, 0.06, 1.4, triangle),
        ),
        Sfx::Mine => sweep(0.12, 240.0, 90.0, 0.15, 1.8, square),
        Sfx::Flame => rumble(0.12, 0.25, 0.05, 0.18, 53),
        Sfx::Gatling => mix(
            sweep(0.035, 900.0, 250.0, 0.12, 1.2, square),
            rumble(0.025, 0.8, 0.15, 0.14, 41),
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
        Sfx::Lunge => mix(
            rumble(0.16, 0.05, 0.6, 0.28, 29),
            sweep(0.14, 300.0, 1100.0, 0.07, 1.2, saw),
        ),
        Sfx::Shock => mix(
            sweep(0.6, 95.0, 32.0, 0.55, 1.3, sine),
            rumble(0.5, 0.25, 0.02, 0.45, 31),
        ),
        Sfx::Siren => concat(&[
            sweep(0.4, 330.0, 660.0, 0.16, 0.2, saw),
            sweep(0.4, 660.0, 330.0, 0.16, 0.2, saw),
            sweep(0.4, 330.0, 660.0, 0.16, 0.2, saw),
            sweep(0.5, 660.0, 300.0, 0.16, 1.0, saw),
        ]),
        Sfx::BossBlast => mix(
            sweep(0.3, 180.0, 60.0, 0.35, 1.5, sine),
            rumble(0.25, 0.6, 0.1, 0.25, 37),
        ),
        Sfx::Victory => concat(&[
            note(523.0, 0.12, 0.2),
            note(659.0, 0.12, 0.2),
            note(784.0, 0.12, 0.2),
            mix(note(1047.0, 0.9, 0.22), note(784.0, 0.9, 0.12)),
        ]),
        Sfx::LevelUp => concat(&[
            note(784.0, 0.06, 0.16),
            note(988.0, 0.06, 0.16),
            note(1319.0, 0.06, 0.16),
            note(1568.0, 0.25, 0.18),
        ]),
        Sfx::WaveClear => concat(&[
            note(659.0, 0.1, 0.18),
            mix(note(988.0, 0.45, 0.2), note(1319.0, 0.45, 0.08)),
        ]),
        Sfx::Combo(n) => {
            // Major pentatonic, two octaves up from C5.
            const STEPS: [f32; COMBO_NOTES as usize] =
                [0., 2., 4., 7., 9., 12., 14., 16., 19., 21., 24., 26.];
            let freq = 523.25 * 2f32.powf(STEPS[usize::from(n.min(COMBO_NOTES - 1))] / 12.0);
            mix(note(freq, 0.16, 0.14), note(freq * 2.0, 0.1, 0.04))
        }
        // A hiss with a sinking buzz: the gun venting heat.
        Sfx::Overheat => mix(
            rumble(0.6, 0.9, 0.5, 0.25, 23),
            sweep(0.4, 240.0, 90.0, 0.12, 1.0, square),
        ),
        Sfx::GameOver => concat(&[
            sweep(0.25, 392.0, 380.0, 0.22, 0.6, saw),
            sweep(0.25, 330.0, 318.0, 0.22, 0.6, saw),
            sweep(0.7, 262.0, 200.0, 0.25, 1.4, saw),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_has_its_own_clip() {
        let all: Vec<Sfx> = BASIC
            .into_iter()
            .chain((0..COMBO_NOTES).map(Sfx::Combo))
            .collect();
        let mut slots: Vec<usize> = all.iter().map(|s| s.index()).collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), all.len(), "two sounds share a clip");
        assert!(slots.iter().all(|&i| i < all.len()));
    }
}
