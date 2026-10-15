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

mod bfg;
mod boss;
mod combat;
mod difficulty;
mod effects;
mod enemies;
mod fields;
mod incendiary;
mod input;
mod math;
mod pickups;
mod player;
#[cfg(test)]
mod tests;
mod trickshots;
mod trophies;
mod upgrades;
mod waves;

pub use bfg::*;
pub use boss::*;
pub use combat::*;
pub use difficulty::*;
pub use effects::*;
pub use enemies::*;
pub use fields::*;
pub use incendiary::*;
pub use input::*;
pub use math::*;
pub use pickups::*;
pub use player::*;
pub use trickshots::*;
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
    pub arena_size: V2,
    pub player: Player,
    pub enemies: Vec<Enemy>,
    pub bullets: Vec<Bullet>,
    pub bfgs: Vec<Bfg>,
    pub gravity_fields: Vec<GravityField>,
    pub thrown_ships: Vec<ThrownShip>,
    pub lightning_arcs: Vec<LightningArc>,
    pub storm: Option<LightningStorm>,
    pub ricochets: Vec<Ricochet>,
    pub blades: Vec<Blade>,
    pub mines: Vec<StickyMine>,
    pub fuel_globs: Vec<FuelGlob>,
    pub fire_patches: Vec<FirePatch>,
    pub burns: Vec<Burn>,
    pub gatling: Option<Gatling>,
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
    wave_spawn_index: usize,
    late_rosters: [&'static [Kind]; waves::LATE_ROSTERS.len()],
    pub banner: f32,
    pub wave_clear: f32,
    pub score: u64,
    pub best: u64,
    pub new_best: bool,
    pub chain: u32,
    pub chain_t: f32,
    pub time: f32,
    state_t: f32,
    seed: u64,
    rng: Rng,
    pub sfx: Vec<Sfx>,
    best_path: Option<PathBuf>,
    pub(crate) dev: DevFlags,
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
    pub trophy_offer: Option<BossKind>,
    trophy_cd: [f32; Upgrade::TROPHIES.len()],
    trophy_time: f32,
    next_id: u32,
    /// Jump the camera to its target instead of easing (new run).
    snap_camera: bool,
}

/// Developer switches for testing waves and power-ups.
#[derive(Clone, Copy, Debug, clap::Args)]
pub(crate) struct DevFlags {
    /// Start at this wave
    #[arg(long = "wave", env = "GLOWSTORM_WAVE", default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub first_wave: u32,
    /// Prevent death while keeping hit reactions
    #[arg(long, env = "GLOWSTORM_GOD", value_parser = clap::builder::BoolishValueParser::new())]
    pub god: bool,
    /// Open the upgrade screen at the start of each run
    #[arg(long, env = "GLOWSTORM_CARDS", value_parser = clap::builder::BoolishValueParser::new())]
    pub cards: bool,
    /// Remove all gun, charge, and thrust heat
    #[arg(long, env = "GLOWSTORM_PERFECT_COOLING", value_parser = clap::builder::BoolishValueParser::new())]
    pub perfect_cooling: bool,
    /// Start each run with the spread-shot upgrade
    #[arg(long, env = "GLOWSTORM_SPREAD_SHOT", value_parser = clap::builder::BoolishValueParser::new())]
    pub spread_shot: bool,
    /// Keep the lightning-bolt pickup's double firing rate active
    #[arg(long, visible_alias = "lightning", env = "GLOWSTORM_RAPID_FIRE", value_parser = clap::builder::BoolishValueParser::new())]
    pub rapid_fire: bool,
}

impl Default for DevFlags {
    fn default() -> Self {
        Self {
            first_wave: 1,
            god: false,
            cards: false,
            perfect_cooling: false,
            spread_shot: false,
            rapid_fire: false,
        }
    }
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let best_path = best_score_path(Difficulty::Medium);
        let best = load_best(best_path.as_ref());
        let mut game = Self {
            state: State::Title,
            arena_size: v2(WORLD_W, WORLD_H),
            player: Player::new(Difficulty::Medium.hearts()),
            enemies: Vec::new(),
            bullets: Vec::new(),
            bfgs: Vec::new(),
            gravity_fields: Vec::new(),
            thrown_ships: Vec::new(),
            lightning_arcs: Vec::new(),
            storm: None,
            ricochets: Vec::new(),
            blades: Vec::new(),
            mines: Vec::new(),
            fuel_globs: Vec::new(),
            fire_patches: Vec::new(),
            burns: Vec::new(),
            gatling: None,
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
            wave_spawn_index: 0,
            late_rosters: waves::LATE_ROSTERS,
            banner: 0.0,
            wave_clear: 0.0,
            score: 0,
            best,
            new_best: false,
            chain: 0,
            chain_t: 0.0,
            time: 0.0,
            state_t: 0.0,
            seed,
            rng: Rng(seed.max(1)),
            sfx: Vec::new(),
            best_path,
            dev: DevFlags::default(),
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
            trophy_offer: None,
            trophy_cd: [0.0; Upgrade::TROPHIES.len()],
            trophy_time: 0.0,
            next_id: 1,
            snap_camera: false,
        };
        for i in (1..game.late_rosters.len()).rev() {
            let j = (game.rng.next() % (i + 1) as u64) as usize;
            game.late_rosters.swap(i, j);
        }
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

    fn resize_arena(&mut self, size: V2) {
        if size == self.arena_size {
            return;
        }
        for m in &mut self.motes {
            m.pos.x *= size.x / self.arena_size.x;
            m.pos.y *= size.y / self.arena_size.y;
        }
        self.arena_size = size;
        self.snap_camera = true;
        let r = WALL + PLAYER_RADIUS;
        self.player.pos = if self.state == State::Title {
            size * 0.5
        } else {
            self.player.pos.clamp(v2(r, r), size - v2(r, r))
        };
        for e in &mut self.enemies {
            let r = WALL + e.kind.radius();
            e.pos = e.pos.clamp(v2(r, r), size - v2(r, r));
        }
        for spawn in &mut self.spawns {
            let r = WALL + spawn.kind.radius();
            spawn.pos = spawn.pos.clamp(v2(r, r), size - v2(r, r));
        }
        for pickup in &mut self.pickups {
            pickup.pos = pickup.pos.clamp(v2(r, r), size - v2(r, r));
        }
        if let Some(boss) = &mut self.boss {
            boss.resize_arena(size);
        }
    }

    fn start(&mut self) {
        let difficulty = self.difficulty;
        let arena_size = self.arena_size;
        let dev = self.dev;
        *self = Game::new(self.seed);
        self.dev = dev;
        self.resize_arena(arena_size);
        self.set_difficulty(difficulty);
        self.state = State::Playing;
        self.snap_camera = true;
        self.start_wave(self.dev.first_wave);
        if self.dev.cards {
            self.offer = self.roll_offer();
            self.state = State::Upgrade;
            self.state_t = 0.0;
        }
    }

    pub fn on_focus_lost(&mut self) {
        self.pause();
    }

    pub fn pause(&mut self) {
        if self.state == State::Playing {
            self.player.charge = 0.0;
            self.player.charge_blocked = true;
            self.player.roll_hold = None;
            self.player.charge_spin = false;
            self.gravity_fields.retain(|field| !field.charging);
            self.state = State::Paused;
        }
    }

    /// Q quits from menus; mid-run it only pauses, so a stray press next to
    /// W/A (or Z/Q/S/D on AZERTY) doesn't throw the run away.
    pub fn quit_allowed(&self) -> bool {
        !matches!(self.state, State::Playing | State::Dying)
    }

    pub fn update(&mut self, dt: f32, input: &Input, view: &View) {
        if self.arena_size != view.arena_size() {
            self.resize_arena(view.arena_size());
            self.follow(self.player.pos, view, 0.0, 0.0);
        }
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
                    self.arena_size.x / 2.0 + t.cos() * 500.0,
                    self.arena_size.y / 2.0 + (t * 1.3).sin() * 250.0,
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
                let count = Weapon::ALL.len() as i32;
                let index = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0']
                    .iter()
                    .position(|&key| input.hit(&[KeyCode::Char(key)]))
                    .unwrap_or_else(|| {
                        (self.player.weapon as i32 + input.weapon_scroll.rem_euclid(count))
                            .rem_euclid(count) as usize
                    });
                let weapon = Weapon::ALL[index];
                if weapon != self.player.weapon {
                    self.player.weapon = weapon;
                    if self.player.charge > 0.0 && !self.player.charge_spin {
                        self.player.charge = 0.0;
                        self.player.charge_blocked = true;
                    }
                }
                if input.hit(&[KeyCode::Esc, KeyCode::Char('p')]) {
                    self.pause();
                    return;
                }
                if input.middle_clicked && !self.bfgs.is_empty() {
                    self.detonate_bfgs();
                    self.player.charge_blocked = true;
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
                        let dev_run = self.dev.first_wave > 1
                            || self.dev.god
                            || self.dev.cards
                            || self.dev.perfect_cooling
                            || self.dev.spread_shot
                            || self.dev.rapid_fire;
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
                self.state_t += dt;
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
                    let arena_size = self.arena_size;
                    let dev = self.dev;
                    *self = Game::new(self.seed);
                    self.dev = dev;
                    self.resize_arena(arena_size);
                    self.set_difficulty(difficulty);
                }
            }
        }
        self.shake *= (-9.0 * dt).exp();
        self.shake_offset = self.rng.dir() * self.shake * self.rng.unit();
    }

    fn follow(&mut self, target: V2, view: &View, dt: f32, speed: f32) {
        // The camera may look up to 35% of the view past a wall, so the ship
        // never sits at the window's edge and aiming at a corner doesn't push
        // the mouse off the window.
        let clamp_axis = |center: f32, view: f32, world: f32| {
            if view >= world {
                (world - view) / 2.0
            } else {
                (center - view / 2.0).clamp(-0.35 * view, world - 0.65 * view)
            }
        };
        let goal = v2(
            clamp_axis(target.x, view.size.x, self.arena_size.x),
            clamp_axis(target.y, view.size.y, self.arena_size.y),
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
            self.update_spin(dt);
        }
        if self.state == State::Playing {
            self.update_shields(dt);
            self.update_gatling(dt);
            self.update_trophies(dt);
        }
        self.update_fields(dt);
        self.update_trickshots(dt);
        self.update_incendiary(dt);
        self.update_bfgs(dt);
        self.update_bullets(dt);
        self.shockwaves.retain_mut(|w| {
            w.r += SHOCK_SPEED * dt;
            w.r < SHOCK_RADIUS
        });
        if self.state == State::Playing {
            self.player_collisions();
            self.update_pickups(dt);
        }
        if self.state != State::Playing {
            self.gatling = None;
            self.player.charge = 0.0;
            self.player.charge_blocked = true;
            self.player.spin = None;
            self.player.roll_hold = None;
            self.player.charge_spin = false;
        }
        self.chain_t -= dt;
        if self.chain_t <= 0.0 {
            self.chain = 0;
        }
        self.update_effects(dt);
    }
}
