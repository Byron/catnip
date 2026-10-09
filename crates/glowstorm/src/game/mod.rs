//! Glowstorm's gameplay: pure simulation, no drawing (see `render`).
//!
//! `Game` holds the whole world; each submodule adds the logic for one part
//! of it as `impl Game` blocks.

use std::{f32::consts::TAU, path::PathBuf};

use crossterm::event::KeyCode;

use crate::{
    art::{Hue, WALL, WORLD_H, WORLD_W},
    sound::Sfx,
};

mod boss;
mod combat;
mod difficulty;
mod effects;
mod enemies;
mod input;
mod math;
mod pickups;
mod player;
#[cfg(test)]
mod tests;
mod upgrades;
mod waves;

pub use boss::*;
pub use combat::*;
pub use difficulty::*;
pub use effects::*;
pub use enemies::*;
pub use input::*;
pub use math::*;
pub use pickups::*;
pub use player::*;
pub use upgrades::*;
pub use waves::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Title,
    Playing,
    Paused,
    Dying,
    GameOver,
    /// Picking an upgrade card after a boss.
    Upgrade,
}

pub struct Game {
    pub state: State,
    pub player: Player,
    pub enemies: Vec<Enemy>,
    pub bullets: Vec<Bullet>,
    pub orbs: Vec<Orb>,
    spawns: Vec<Spawn>,
    pub particles: Vec<Particle>,
    pub rings: Vec<Ring>,
    pub pickups: Vec<Pickup>,
    pub ghosts: Vec<Ghost>,
    pub cam: V2,
    pub shake_offset: V2,
    shake: f32,
    hitstop: f32,
    pub wave: u32,
    budget: i32,
    spawn_cd: f32,
    pub banner: f32,
    pub wave_clear: f32,
    pub score: u64,
    pub best: u64,
    pub new_best: bool,
    pub chain: u32,
    pub chain_t: f32,
    pub time: f32,
    state_t: f32,
    rng: Rng,
    pub sfx: Vec<Sfx>,
    best_path: Option<PathBuf>,
    dev: DevFlags,
    pub shockwaves: Vec<Shockwave>,
    pub popups: Vec<Popup>,
    pub motes: Vec<Mote>,
    pub boss: Option<Boss>,
    /// Seconds until the boss arrives (WARNING is shown meanwhile).
    pub boss_pending: f32,
    /// White screen flash, seconds left.
    pub flash: f32,
    /// "ENRAGED" was already shown this wave.
    enrage_announced: bool,
    slowmo: f32,
    slowmo_scale: f32,
    pub difficulty: Difficulty,
    /// Cards on offer in the Upgrade state, and the highlighted one.
    pub offer: Vec<Upgrade>,
    pub offer_pick: usize,
    next_id: u32,
    /// Jump the camera to its target instead of easing (new run).
    snap_camera: bool,
}

/// Developer switches read from the environment, for testing later waves:
/// `GLOWSTORM_WAVE=6` starts at wave 6, `GLOWSTORM_GOD=1` ignores damage.
#[derive(Clone, Copy)]
struct DevFlags {
    first_wave: u32,
    god: bool,
    /// `GLOWSTORM_CARDS=1`: open the upgrade screen right away.
    cards: bool,
}

impl DevFlags {
    fn from_env() -> Self {
        Self {
            first_wave: std::env::var("GLOWSTORM_WAVE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .max(1),
            god: std::env::var_os("GLOWSTORM_GOD").is_some(),
            cards: std::env::var_os("GLOWSTORM_CARDS").is_some(),
        }
    }
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let best_path = best_score_path(Difficulty::Medium);
        let best = load_best(best_path.as_ref());
        let mut game = Self {
            state: State::Title,
            player: Player::new(Difficulty::Medium.hearts()),
            enemies: Vec::new(),
            bullets: Vec::new(),
            orbs: Vec::new(),
            spawns: Vec::new(),
            particles: Vec::new(),
            rings: Vec::new(),
            pickups: Vec::new(),
            ghosts: Vec::new(),
            cam: v2(WORLD_W / 2.0, WORLD_H / 2.0),
            shake_offset: V2::default(),
            shake: 0.0,
            hitstop: 0.0,
            wave: 0,
            budget: 0,
            spawn_cd: 0.0,
            banner: 0.0,
            wave_clear: 0.0,
            score: 0,
            best,
            new_best: false,
            chain: 0,
            chain_t: 0.0,
            time: 0.0,
            state_t: 0.0,
            rng: Rng(seed | 1),
            sfx: Vec::new(),
            best_path,
            dev: DevFlags::from_env(),
            shockwaves: Vec::new(),
            popups: Vec::new(),
            motes: Vec::new(),
            boss: None,
            boss_pending: 0.0,
            flash: 0.0,
            enrage_announced: false,
            slowmo: 0.0,
            slowmo_scale: 1.0,
            difficulty: Difficulty::Medium,
            offer: Vec::new(),
            offer_pick: 0,
            next_id: 1,
            snap_camera: false,
        };
        for i in 0..MOTES {
            let pos = v2(game.rng.range(0.0, WORLD_W), game.rng.range(0.0, WORLD_H));
            let drift = game.rng.dir() * game.rng.range(6.0, 22.0);
            game.motes.push(Mote {
                pos,
                vel: drift,
                drift,
                hue: i % 2,
            });
        }
        game
    }

    fn start(&mut self) {
        let difficulty = self.difficulty;
        let seed = self.rng.next();
        *self = Game::new(seed);
        self.set_difficulty(difficulty);
        self.state = State::Playing;
        self.snap_camera = true;
        self.start_wave(self.dev.first_wave);
        if self.dev.cards {
            self.offer = self.roll_offer();
            self.state = State::Upgrade;
        }
    }

    pub fn on_focus_lost(&mut self) {
        if self.state == State::Playing {
            self.state = State::Paused;
        }
    }

    pub fn update(&mut self, dt: f32, input: &Input, view: &View) {
        self.flash = (self.flash - dt).max(0.0);
        let dt = if self.slowmo > 0.0 {
            self.slowmo -= dt;
            dt * self.slowmo_scale
        } else {
            dt
        };
        self.time += dt;
        let confirm = input.clicked || input.hit(&[KeyCode::Enter]);
        match self.state {
            State::Title => {
                let index = Difficulty::ALL
                    .iter()
                    .position(|d| *d == self.difficulty)
                    .unwrap_or(0);
                let up = input.hit(&[KeyCode::Up, KeyCode::Char('w')]);
                let down = input.hit(&[KeyCode::Down, KeyCode::Char('s')]);
                let mut pick = None;
                if up {
                    pick = Some((index + Difficulty::ALL.len() - 1) % Difficulty::ALL.len());
                } else if down {
                    pick = Some((index + 1) % Difficulty::ALL.len());
                }
                for (i, key) in ['1', '2', '3'].into_iter().enumerate() {
                    if input.hit(&[KeyCode::Char(key)]) {
                        pick = Some(i);
                    }
                }
                if let Some(i) = pick {
                    self.set_difficulty(Difficulty::ALL[i]);
                    self.sfx.push(Sfx::Combo(i as u8 * 2));
                }
                if confirm || input.hit(&[KeyCode::Char(' ')]) {
                    self.start();
                }
                // Slow drift over the arena behind the title.
                let t = self.time * 0.15;
                let target = v2(
                    WORLD_W / 2.0 + t.cos() * 500.0,
                    WORLD_H / 2.0 + (t * 1.3).sin() * 250.0,
                );
                self.follow(target, view, dt, 2.0);
                self.update_effects(dt);
            }
            State::Paused => {
                if confirm || input.hit(&[KeyCode::Esc, KeyCode::Char('p')]) {
                    self.state = State::Playing;
                }
            }
            State::Playing => {
                if input.hit(&[KeyCode::Esc, KeyCode::Char('p')]) {
                    self.state = State::Paused;
                    return;
                }
                if self.hitstop > 0.0 {
                    self.hitstop -= dt;
                    return;
                }
                self.update_player(dt, input, view);
                self.update_world(dt);
                self.follow(self.player.pos, view, dt, 8.0);
            }
            State::Dying => {
                self.update_world(dt);
                self.state_t -= dt;
                if self.state_t <= 0.0 {
                    self.state = State::GameOver;
                    self.state_t = 0.0;
                    self.sfx.push(Sfx::GameOver);
                    if self.score > self.best {
                        self.best = self.score;
                        self.new_best = true;
                        // Dev runs (skipped waves, god mode) don't count.
                        let dev_run = self.dev.first_wave > 1 || self.dev.god || self.dev.cards;
                        if let Some(path) = self.best_path.as_ref().filter(|_| !dev_run) {
                            if let Some(dir) = path.parent() {
                                let _ = std::fs::create_dir_all(dir);
                            }
                            let _ = std::fs::write(path, self.score.to_string());
                        }
                    }
                }
            }
            State::Upgrade => {
                self.follow(self.player.pos, view, dt, 8.0);
                self.update_effects(dt);
                self.update_offer(input, view);
            }
            State::GameOver => {
                self.state_t += dt;
                self.update_effects(dt);
                // A short grace period so a held fire button doesn't skip the screen.
                if self.state_t > 0.8 && (confirm || input.hit(&[KeyCode::Char('r')])) {
                    self.start();
                } else if input.hit(&[KeyCode::Esc]) {
                    let difficulty = self.difficulty;
                    let seed = self.rng.next();
                    *self = Game::new(seed);
                    self.set_difficulty(difficulty);
                }
            }
        }
        self.shake *= (-9.0 * dt).exp();
        self.shake_offset = self.rng.dir() * self.shake * self.rng.unit();
    }

    fn follow(&mut self, target: V2, view: &View, dt: f32, speed: f32) {
        let clamp_axis = |center: f32, view: f32, world: f32| {
            if view >= world {
                (world - view) / 2.0
            } else {
                (center - view / 2.0).clamp(0.0, world - view)
            }
        };
        let goal = v2(
            clamp_axis(target.x, view.size.x, WORLD_W),
            clamp_axis(target.y, view.size.y, WORLD_H),
        );
        if self.snap_camera {
            self.cam = goal;
            self.snap_camera = false;
        } else {
            self.cam += (goal - self.cam) * (1.0 - (-speed * dt).exp());
        }
    }

    fn update_world(&mut self, dt: f32) {
        self.update_waves(dt);
        self.update_enemies(dt);
        self.update_boss(dt);
        if self.state == State::Playing {
            self.update_shields(dt);
        }
        self.update_bullets(dt);
        self.shockwaves.retain_mut(|w| {
            w.r += SHOCK_SPEED * dt;
            w.r < SHOCK_RADIUS
        });
        if self.state == State::Playing {
            self.player_collisions();
            self.update_pickups(dt);
        }
        self.chain_t -= dt;
        if self.chain_t <= 0.0 {
            self.chain = 0;
        }
        self.update_effects(dt);
    }
}
