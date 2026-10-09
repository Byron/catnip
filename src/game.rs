//! Glowstorm: the gameplay. Pure simulation; drawing lives in render.rs.

use std::{
    collections::HashSet,
    f32::consts::TAU,
    ops::{Add, AddAssign, Mul, Neg, Sub},
    path::PathBuf,
};

use catnip::audio::Sfx;
use crossterm::event::KeyCode;

use crate::art::{Hue, WALL, WORLD_H, WORLD_W};

mod boss;
pub use boss::{BOSS_RADIUS, Boss};

// ---------------------------------------------------------------- math

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

impl V2 {
    pub fn len(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn norm(self) -> V2 {
        let len = self.len();
        if len > 1e-6 {
            self * (1.0 / len)
        } else {
            V2::default()
        }
    }

    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }

    pub fn from_angle(angle: f32) -> V2 {
        v2(angle.cos(), angle.sin())
    }

    pub fn perp(self) -> V2 {
        v2(-self.y, self.x)
    }

    pub fn dot(self, o: V2) -> f32 {
        self.x * o.x + self.y * o.y
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        *self = *self + o;
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, k: f32) -> V2 {
        v2(self.x * k, self.y * k)
    }
}

impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        v2(-self.x, -self.y)
    }
}

/// Distance from `p` to the segment a→b. Fast bullets test their whole path
/// this frame, so they can't tunnel through small enemies.
fn segment_distance(p: V2, a: V2, b: V2) -> f32 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 == 0.0 {
        0.0
    } else {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    };
    (p - (a + ab * t)).len()
}

pub struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    fn dir(&mut self) -> V2 {
        V2::from_angle(self.range(0.0, TAU))
    }
}

// ---------------------------------------------------------------- input

#[derive(Default)]
pub struct Input {
    pub held: HashSet<KeyCode>,
    pub pressed: Vec<KeyCode>,
    /// Mouse position in window pixels, once the mouse has moved.
    pub mouse: Option<V2>,
    pub fire: bool,
    pub clicked: bool,
    pub dash: bool,
}

impl Input {
    fn down(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.held.contains(k))
    }

    fn hit(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.pressed.contains(k))
    }

    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.clicked = false;
        self.dash = false;
    }
}

pub struct View {
    pub scale: f32,
    pub size: V2,
}

// ---------------------------------------------------------------- entities

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Title,
    Playing,
    Paused,
    Dying,
    GameOver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Chaser,
    Swarm,
    Shooter,
    Tank,
}

impl Kind {
    fn hp(self) -> f32 {
        match self {
            Kind::Chaser => 2.0,
            Kind::Swarm => 1.0,
            Kind::Shooter => 4.0,
            Kind::Tank => 16.0,
        }
    }

    pub fn radius(self) -> f32 {
        match self {
            Kind::Chaser => 16.0,
            Kind::Swarm => 10.0,
            Kind::Shooter => 18.0,
            Kind::Tank => 32.0,
        }
    }

    fn score(self) -> u64 {
        match self {
            Kind::Chaser => 10,
            Kind::Swarm => 5,
            Kind::Shooter => 25,
            Kind::Tank => 80,
        }
    }

    pub fn hue(self) -> Hue {
        match self {
            Kind::Chaser => Hue::Magenta,
            Kind::Swarm => Hue::Yellow,
            Kind::Shooter => Hue::Orange,
            Kind::Tank => Hue::Green,
        }
    }

    /// How much of a wave's budget one spawn of this kind uses.
    fn cost(self) -> i32 {
        match self {
            Kind::Chaser => 1,
            Kind::Swarm => 3,
            Kind::Shooter => 3,
            Kind::Tank => 6,
        }
    }
}

/// What an enemy is doing. Attacks always start with a visible wind-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Move,
    Windup,
    Strike,
    Recover,
}

pub struct Enemy {
    pub kind: Kind,
    pub pos: V2,
    pub vel: V2,
    hp: f32,
    pub flash: f32,
    pub spin: f32,
    pub facing: f32,
    cooldown: f32,
    pub phase: f32,
    pub act: Act,
    act_t: f32,
    aim: V2,
}

impl Enemy {
    pub fn winding_up(&self) -> bool {
        self.act == Act::Windup
    }
}

/// A tank's ground-pound: an expanding ring that hurts where it passes.
pub struct Shockwave {
    pub pos: V2,
    pub r: f32,
    hit: bool,
}

/// Floating "+40" / "X3" text.
pub struct Popup {
    pub pos: V2,
    pub text: String,
    pub t: f32,
    pub combo: bool,
}

/// Background specks that drift and get blown around by explosions.
pub struct Mote {
    pub pos: V2,
    vel: V2,
    drift: V2,
    pub hue: usize,
}

pub struct Bullet {
    pub pos: V2,
    prev: V2,
    pub vel: V2,
    /// Distance flown so far; damage falls off with it.
    pub traveled: f32,
}

pub struct Orb {
    pub pos: V2,
    vel: V2,
    life: f32,
}

pub struct Spawn {
    kind: Kind,
    pos: V2,
    t: f32,
}

pub struct Particle {
    pub pos: V2,
    vel: V2,
    pub life: f32,
    pub max: f32,
    pub hue: Hue,
    pub size: usize,
}

pub struct Ring {
    pub pos: V2,
    pub t: f32,
    pub dur: f32,
    pub hue: Hue,
    /// Telegraph rings shrink towards the spawn point instead of expanding.
    pub inward: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PickupKind {
    Heart,
    Rapid,
}

pub struct Pickup {
    pub kind: PickupKind,
    pub pos: V2,
    pub life: f32,
}

pub struct Ghost {
    pub pos: V2,
    pub angle: f32,
    pub life: f32,
}

pub struct Player {
    pub pos: V2,
    pub vel: V2,
    pub aim: f32,
    pub hp: i32,
    pub invuln: f32,
    fire_cd: f32,
    pub dash_cd: f32,
    dash_t: f32,
    pub rapid_t: f32,
    ghost_cd: f32,
    pub max_hp: i32,
    /// Weapon heat, 0..1. Firing adds heat; at 1 the gun locks until it cools.
    pub heat: f32,
    pub overheated: bool,
}

/// Picked on the title screen. Harder settings pay more points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Medium,
    Hard,
    Extreme,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Medium, Difficulty::Hard, Difficulty::Extreme];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Medium => "MEDIUM",
            Difficulty::Hard => "HARD",
            Difficulty::Extreme => "EXTREME",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Difficulty::Medium => "5 hearts · the intended experience",
            Difficulty::Hard => "4 hearts · tougher, faster, more enemies · score ×1.5",
            Difficulty::Extreme => "3 hearts · relentless · score ×2",
        }
    }

    /// (hearts, enemy hp, enemy speed, attack rate, enemies per wave, boss hp, score)
    fn tuning(self) -> (i32, f32, f32, f32, f32, f32, f32) {
        match self {
            Difficulty::Medium => (5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0),
            Difficulty::Hard => (4, 1.25, 1.1, 1.2, 1.25, 1.3, 1.5),
            Difficulty::Extreme => (3, 1.6, 1.2, 1.4, 1.5, 1.7, 2.0),
        }
    }

    pub fn hearts(self) -> i32 {
        self.tuning().0
    }

    fn file_suffix(self) -> &'static str {
        match self {
            Difficulty::Medium => "",
            Difficulty::Hard => "_hard",
            Difficulty::Extreme => "_extreme",
        }
    }
}

const PLAYER_RADIUS: f32 = 14.0;
const ACCEL: f32 = 3400.0;
const DRAG: f32 = 7.0;
const BULLET_SPEED: f32 = 1250.0;
/// Bullets fade out after this distance.
pub const BULLET_RANGE: f32 = 620.0;
/// Full damage up to this distance, then linear falloff to MIN_DAMAGE at BULLET_RANGE.
const FULL_DAMAGE_RANGE: f32 = 170.0;
const MIN_DAMAGE: f32 = 0.3;
const HEAT_PER_SHOT: f32 = 0.075;
const HEAT_COOLING: f32 = 0.35;
/// Cools from 1.0 to HEAT_RESUME in one second.
const OVERHEAT_COOLING: f32 = 0.7;
/// After overheating, firing resumes once heat drops to this.
const HEAT_RESUME: f32 = 0.3;
const DASH_SPEED: f32 = 1350.0;
const DASH_TIME: f32 = 0.17;
pub const DASH_COOLDOWN: f32 = 1.3;
const SPAWN_TIME: f32 = 0.9;
const MAX_PARTICLES: usize = 900;
const RAPID_TIME: f32 = 7.0;
const ORB_SPEED: f32 = 340.0;
const MAX_ORBS: usize = 450;
pub const SHOCK_RADIUS: f32 = 270.0;
const SHOCK_SPEED: f32 = 560.0;
const MOTES: usize = 90;
pub const POPUP_TIME: f32 = 0.8;

impl Player {
    fn new(hearts: i32) -> Self {
        Self {
            pos: v2(WORLD_W / 2.0, WORLD_H / 2.0),
            vel: V2::default(),
            aim: -TAU / 4.0,
            hp: hearts,
            max_hp: hearts,
            invuln: 0.0,
            fire_cd: 0.0,
            dash_cd: 0.0,
            dash_t: 0.0,
            rapid_t: 0.0,
            ghost_cd: 0.0,
            heat: 0.0,
            overheated: false,
        }
    }

    pub fn dashing(&self) -> bool {
        self.dash_t > 0.0
    }
}

// ---------------------------------------------------------------- game

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
    slowmo: f32,
    slowmo_scale: f32,
    pub difficulty: Difficulty,
}

/// Developer switches read from the environment, for testing later waves:
/// `GLOWSTORM_WAVE=6` starts at wave 6, `GLOWSTORM_GOD=1` ignores damage.
#[derive(Clone, Copy)]
struct DevFlags {
    first_wave: u32,
    god: bool,
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
            slowmo: 0.0,
            slowmo_scale: 1.0,
            difficulty: Difficulty::Medium,
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

    pub fn multiplier(&self) -> u64 {
        (1 + self.chain / 6).min(8) as u64
    }

    fn start(&mut self) {
        let difficulty = self.difficulty;
        let seed = self.rng.next();
        *self = Game::new(seed);
        self.set_difficulty(difficulty);
        self.state = State::Playing;
        self.start_wave(self.dev.first_wave);
    }

    fn set_difficulty(&mut self, difficulty: Difficulty) {
        self.difficulty = difficulty;
        self.player = Player::new(difficulty.hearts());
        self.best_path = best_score_path(difficulty);
        self.best = load_best(self.best_path.as_ref());
    }

    /// The player's small per-wave edge: about 1.2× when enemies are 2×.
    pub fn power(&self) -> f32 {
        (1.0 + 0.02 * self.wave.saturating_sub(1) as f32).min(1.3)
    }

    fn enemy_hp_scale(&self) -> f32 {
        self.difficulty.tuning().1 * (1.0 + 0.06 * self.wave.saturating_sub(1) as f32)
    }

    fn enemy_speed_scale(&self) -> f32 {
        self.difficulty.tuning().2 * (1.0 + 0.03 * self.wave.saturating_sub(1) as f32).min(1.45)
    }

    /// Multiplies attack cooldowns: below 1 means enemies attack more often.
    fn enemy_cooldown_scale(&self) -> f32 {
        let rate = self.difficulty.tuning().3 * (1.0 + 0.04 * self.wave.saturating_sub(1) as f32);
        (1.0 / rate).max(0.45)
    }

    /// Points after the combo multiplier and difficulty bonus.
    fn award(&mut self, base: u64) -> u64 {
        let points =
            (base as f32 * self.multiplier() as f32 * self.difficulty.tuning().6).round() as u64;
        self.score += points;
        points
    }

    fn start_wave(&mut self, wave: u32) {
        self.wave = wave;
        self.spawn_cd = 0.0;
        if wave.is_multiple_of(5) {
            // Boss wave: no regular spawns, the boss brings its own minions.
            self.budget = 0;
            self.banner = 0.0;
            self.boss_pending = 2.6;
            self.sfx.push(Sfx::Siren);
        } else {
            self.budget = ((6 + 4 * wave) as f32 * self.difficulty.tuning().4).round() as i32;
            self.banner = 2.0;
            self.sfx.push(Sfx::Wave);
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
                        let dev_run = self.dev.first_wave > 1 || self.dev.god;
                        if let Some(path) = self.best_path.as_ref().filter(|_| !dev_run) {
                            if let Some(dir) = path.parent() {
                                let _ = std::fs::create_dir_all(dir);
                            }
                            let _ = std::fs::write(path, self.score.to_string());
                        }
                    }
                }
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
        self.cam += (goal - self.cam) * (1.0 - (-speed * dt).exp());
    }

    fn update_player(&mut self, dt: f32, input: &Input, view: &View) {
        // Half of the damage edge also goes into gun cooling.
        let cooling_boost = 1.0 + (self.power() - 1.0) / 2.0;
        let Self {
            player: p,
            sfx,
            ghosts,
            bullets,
            rng,
            cam,
            ..
        } = self;
        let axis = |neg: &[KeyCode], pos: &[KeyCode]| {
            f32::from(u8::from(input.down(pos))) - f32::from(u8::from(input.down(neg)))
        };
        let movement = v2(
            axis(&[KeyCode::Char('a')], &[KeyCode::Char('d')]),
            axis(&[KeyCode::Char('w')], &[KeyCode::Char('s')]),
        )
        .norm();
        // Arrow keys: keyboard-only twin-stick (aim + fire).
        let arrows = v2(
            axis(&[KeyCode::Left], &[KeyCode::Right]),
            axis(&[KeyCode::Up], &[KeyCode::Down]),
        );

        if arrows != V2::default() {
            p.aim = arrows.angle();
        } else if let Some(mouse) = input.mouse {
            let world = *cam + mouse * (1.0 / view.scale);
            p.aim = (world - p.pos).angle();
        } else if movement != V2::default() {
            p.aim = movement.angle();
        }

        p.dash_cd -= dt;
        p.invuln -= dt;
        p.rapid_t -= dt;
        let wants_dash = input.dash || input.hit(&[KeyCode::Char(' ')]);
        if wants_dash && p.dash_cd <= 0.0 {
            let dir = if movement != V2::default() {
                movement
            } else {
                V2::from_angle(p.aim)
            };
            p.vel = dir * DASH_SPEED;
            p.dash_t = DASH_TIME;
            p.dash_cd = DASH_COOLDOWN;
            sfx.push(Sfx::Dash);
        }

        if p.dash_t > 0.0 {
            p.dash_t -= dt;
            p.ghost_cd -= dt;
            if p.ghost_cd <= 0.0 {
                p.ghost_cd = 0.018;
                ghosts.push(Ghost {
                    pos: p.pos,
                    angle: p.aim,
                    life: 0.22,
                });
            }
            if p.dash_t <= 0.0 {
                p.vel = p.vel * 0.35;
            }
        } else {
            p.vel += movement * ACCEL * dt;
            p.vel = p.vel * (-DRAG * dt).exp();
        }
        p.pos += p.vel * dt;
        let lo = WALL + PLAYER_RADIUS;
        if p.pos.x < lo || p.pos.x > WORLD_W - lo {
            p.pos.x = p.pos.x.clamp(lo, WORLD_W - lo);
            p.vel.x = 0.0;
        }
        if p.pos.y < lo || p.pos.y > WORLD_H - lo {
            p.pos.y = p.pos.y.clamp(lo, WORLD_H - lo);
            p.vel.y = 0.0;
        }

        p.fire_cd -= dt;
        let cooling = if p.overheated {
            OVERHEAT_COOLING
        } else {
            HEAT_COOLING * cooling_boost
        };
        p.heat = (p.heat - cooling * dt).max(0.0);
        if p.overheated && p.heat <= HEAT_RESUME {
            p.overheated = false;
        }
        let firing = input.fire || arrows != V2::default();
        if firing && p.fire_cd <= 0.0 && !p.overheated {
            let rapid = p.rapid_t > 0.0;
            let interval = if rapid { 0.055 } else { 0.11 };
            p.fire_cd = (p.fire_cd + interval).max(0.0);
            // Rapid fire shoots twice as often for the same heat per second.
            p.heat += if rapid {
                HEAT_PER_SHOT / 2.0
            } else {
                HEAT_PER_SHOT
            };
            if p.heat >= 1.0 {
                p.heat = 1.0;
                p.overheated = true;
                sfx.push(Sfx::Overheat);
            }
            let spread = if rapid { 0.09 } else { 0.035 };
            let dir = V2::from_angle(p.aim + rng.range(-spread, spread));
            let muzzle = p.pos + dir * 22.0;
            bullets.push(Bullet {
                pos: muzzle,
                prev: p.pos,
                vel: dir * BULLET_SPEED + p.vel * 0.25,
                traveled: 0.0,
            });
            sfx.push(Sfx::Shoot);
            let vel = p.vel;
            for _ in 0..2 {
                let v = vel + (dir * 260.0 + self.rng.dir() * 120.0);
                self.emit(muzzle, v, 0.12, Hue::White, 0);
            }
        }
    }

    fn update_world(&mut self, dt: f32) {
        self.update_waves(dt);
        self.update_enemies(dt);
        self.update_boss(dt);
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

    fn update_waves(&mut self, dt: f32) {
        self.banner -= dt;
        if self.state != State::Playing {
            return;
        }
        if self.boss_pending > 0.0 {
            self.boss_pending -= dt;
            if self.boss_pending <= 0.0 {
                self.spawn_boss();
            }
        }
        if self.wave_clear > 0.0 {
            self.wave_clear -= dt;
            if self.wave_clear <= 0.0 {
                self.start_wave(self.wave + 1);
            }
        } else if self.budget > 0 {
            self.spawn_cd -= dt;
            if self.spawn_cd <= 0.0 && self.banner < 1.3 {
                self.spawn_cd = (0.95 - 0.05 * self.wave as f32).max(0.25);
                self.spawn_group();
            }
        } else if self.spawns.is_empty()
            && self.enemies.is_empty()
            && self.boss.is_none()
            && self.boss_pending <= 0.0
        {
            self.wave_clear = 1.8;
            // The wave bonus scales with difficulty but not with the combo.
            let bonus = (100.0 * self.wave as f32 * self.difficulty.tuning().6).round() as u64;
            self.score += bonus;
            self.slow(0.6, 0.35);
            self.flash = 0.08;
            self.sfx.push(Sfx::WaveClear);
            self.popup(self.player.pos + v2(0.0, -46.0), format!("+{bonus}"), true);
            self.popup(
                self.player.pos + v2(0.0, 46.0),
                "POWER UP".to_string(),
                true,
            );
        }

        let mut ready = Vec::new();
        self.spawns.retain_mut(|s| {
            s.t -= dt;
            if s.t <= 0.0 {
                ready.push((s.kind, s.pos));
                false
            } else {
                true
            }
        });
        for (kind, pos) in ready {
            self.add_enemy(kind, pos, V2::default());
            for _ in 0..6 {
                let v = self.rng.dir() * self.rng.range(80.0, 220.0);
                self.emit(pos, v, 0.35, kind.hue(), 1);
            }
        }
    }

    fn spawn_group(&mut self) {
        let mut options = vec![Kind::Chaser];
        if self.wave >= 2 {
            options.push(Kind::Swarm);
        }
        if self.wave >= 3 {
            options.push(Kind::Shooter);
        }
        if self.wave >= 4 {
            options.push(Kind::Tank);
        }
        options.retain(|k| k.cost() <= self.budget);
        let kind = options[(self.rng.next() as usize) % options.len()];
        self.budget -= kind.cost();

        // Somewhere inside the arena, not on top of the player.
        let margin = 90.0;
        let mut pos = V2::default();
        for _ in 0..24 {
            pos = v2(
                self.rng.range(margin, WORLD_W - margin),
                self.rng.range(margin, WORLD_H - margin),
            );
            if (pos - self.player.pos).len() > 460.0 {
                break;
            }
        }
        let count = if kind == Kind::Swarm { 5 } else { 1 };
        for i in 0..count {
            let offset = if count > 1 {
                V2::from_angle(i as f32 * TAU / count as f32) * 34.0
            } else {
                V2::default()
            };
            let at = pos + offset;
            self.spawns.push(Spawn {
                kind,
                pos: at,
                t: SPAWN_TIME,
            });
            self.rings.push(Ring {
                pos: at,
                t: 0.0,
                dur: SPAWN_TIME,
                hue: kind.hue(),
                inward: true,
            });
        }
    }

    fn add_enemy(&mut self, kind: Kind, pos: V2, vel: V2) {
        let cooldown = self.rng.range(1.0, 2.0);
        let phase = self.rng.range(0.0, TAU);
        self.enemies.push(Enemy {
            kind,
            pos,
            vel,
            hp: kind.hp() * self.enemy_hp_scale(),
            flash: 0.0,
            spin: phase,
            facing: (self.player.pos - pos).angle(),
            cooldown,
            phase,
            act: Act::Move,
            act_t: 0.0,
            aim: V2::default(),
        });
    }

    fn update_enemies(&mut self, dt: f32) {
        let target = self.player.pos;
        let target_vel = self.player.vel;
        let chasing = self.state == State::Playing;
        let speedup = self.enemy_speed_scale();
        let cooldown = self.enemy_cooldown_scale();
        let fan = self.wave >= 6;
        let mut shots = Vec::new();
        let mut shocks = Vec::new();
        let mut lunged = false;
        for e in &mut self.enemies {
            let to_player = target - e.pos;
            let dist = to_player.len();
            let dir = to_player.norm();
            e.flash -= dt;
            e.act_t -= dt;
            e.cooldown -= dt;
            let mut desired = V2::default();
            let mut steer = 3.0;
            match (e.kind, e.act) {
                (Kind::Chaser, Act::Move) => {
                    desired = dir * 230.0 * speedup;
                    if chasing && dist < 300.0 && e.cooldown <= 0.0 {
                        e.act = Act::Windup;
                        e.act_t = 0.45;
                    }
                }
                (Kind::Chaser, Act::Windup) => {
                    // Brake and lock on; the lunge direction is fixed at release.
                    steer = 10.0;
                    e.aim = dir;
                    if e.act_t <= 0.0 {
                        e.act = Act::Strike;
                        e.act_t = 0.3;
                        e.vel = e.aim * 980.0;
                        lunged = true;
                    }
                }
                (Kind::Chaser, Act::Strike) => {
                    steer = 0.0;
                    if e.act_t <= 0.0 {
                        e.act = Act::Recover;
                        e.act_t = 0.45;
                        e.cooldown = (1.9 + (e.phase * 3.0).fract()) * cooldown;
                    }
                }
                (_, Act::Recover) => {
                    steer = 4.0;
                    if e.act_t <= 0.0 {
                        e.act = Act::Move;
                    }
                }
                (Kind::Swarm, _) => {
                    // Flank: come in at an angle from either side, converge up close.
                    let side = if e.phase > TAU / 2.0 { 1.0 } else { -1.0 };
                    let spread = ((dist - 120.0) / 450.0).clamp(0.0, 1.0);
                    let heading = V2::from_angle(dir.angle() + side * spread);
                    let wobble = dir.perp() * ((self.time * 7.0 + e.phase).sin() * 90.0);
                    desired = heading * 330.0 * speedup + wobble;
                    steer = 4.0;
                }
                (Kind::Shooter, Act::Move) => {
                    let radial = if dist > 460.0 {
                        1.0
                    } else if dist < 200.0 {
                        -1.7
                    } else if dist < 320.0 {
                        -1.0
                    } else {
                        0.0
                    };
                    let strafe = if e.phase > TAU / 2.0 { 1.0 } else { -1.0 };
                    desired = dir * radial * 210.0 + dir.perp() * strafe * 120.0;
                    steer = 2.5;
                    if chasing && e.cooldown <= 0.0 && dist < 950.0 {
                        e.act = Act::Windup;
                        e.act_t = 0.4;
                    }
                }
                (Kind::Shooter, Act::Windup) => {
                    steer = 6.0;
                    if e.act_t <= 0.0 {
                        // Lead the target: aim where the player is heading.
                        let lead = (dist / ORB_SPEED).min(0.9) * 0.8;
                        let aim = (target + target_vel * lead - e.pos).norm();
                        let spread: &[f32] = if fan { &[-0.22, 0.0, 0.22] } else { &[0.0] };
                        for a in spread {
                            shots.push((e.pos + aim * 22.0, V2::from_angle(aim.angle() + a)));
                        }
                        e.act = Act::Move;
                        e.cooldown = (1.5 + (e.phase * 7.0).fract() * 0.9) * cooldown;
                    }
                }
                (Kind::Tank, Act::Move) => {
                    desired = dir * 85.0 * speedup;
                    steer = 1.5;
                    if chasing && dist < 240.0 && e.cooldown <= 0.0 {
                        e.act = Act::Windup;
                        e.act_t = 0.6;
                    }
                }
                (Kind::Tank, Act::Windup) => {
                    steer = 6.0;
                    if e.act_t <= 0.0 {
                        shocks.push(e.pos);
                        e.act = Act::Recover;
                        e.act_t = 0.5;
                        e.cooldown = 3.2 * cooldown;
                    }
                }
                _ => {}
            }
            let desired = if chasing { desired } else { V2::default() };
            if steer > 0.0 {
                e.vel += (desired - e.vel) * (1.0 - (-steer * dt).exp());
            }
            e.pos += e.vel * dt;
            let r = e.kind.radius() + WALL;
            if e.pos.x < r || e.pos.x > WORLD_W - r {
                e.pos.x = e.pos.x.clamp(r, WORLD_W - r);
                e.vel.x = -e.vel.x;
            }
            if e.pos.y < r || e.pos.y > WORLD_H - r {
                e.pos.y = e.pos.y.clamp(r, WORLD_H - r);
                e.vel.y = -e.vel.y;
            }
            let spin = match e.kind {
                Kind::Chaser => 4.0,
                Kind::Swarm => 7.0,
                Kind::Tank => 0.8,
                Kind::Shooter => 0.0,
            };
            // Winding up spins faster, part of the tell.
            e.spin += dt * spin * if e.act == Act::Windup { 3.5 } else { 1.0 };
            let turn = (dir.angle() - e.facing + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0;
            e.facing += turn * (1.0 - (-6.0 * dt).exp());
        }

        // Push overlapping enemies apart so packs spread into readable shapes.
        let n = self.enemies.len();
        for i in 0..n {
            for j in i + 1..n {
                let d = self.enemies[j].pos - self.enemies[i].pos;
                let min = self.enemies[i].kind.radius() + self.enemies[j].kind.radius();
                let len = d.len();
                if len < min && len > 1e-3 {
                    let push = d * ((min - len) / len * 0.5);
                    self.enemies[i].pos = self.enemies[i].pos - push;
                    self.enemies[j].pos += push;
                }
            }
        }

        if lunged {
            self.sfx.push(Sfx::Lunge);
        }
        if !shots.is_empty() {
            self.sfx.push(Sfx::EnemyShoot);
        }
        for (pos, dir) in shots {
            self.add_orb(pos, dir * ORB_SPEED);
        }
        for pos in shocks {
            self.shockwaves.push(Shockwave {
                pos,
                r: 0.0,
                hit: false,
            });
            self.shake = (self.shake + 7.0).min(18.0);
            self.sfx.push(Sfx::Shock);
            self.push_motes(pos, 320.0, 260.0);
        }
        let inside =
            |p: V2| p.x > WALL && p.y > WALL && p.x < WORLD_W - WALL && p.y < WORLD_H - WALL;
        self.orbs.retain_mut(|o| {
            o.pos += o.vel * dt;
            o.life -= dt;
            o.life > 0.0 && inside(o.pos)
        });
    }

    fn update_bullets(&mut self, dt: f32) {
        let mut wall_hits = Vec::new();
        let mut hits = Vec::new();
        let power = self.power();
        let enemies = &mut self.enemies;
        let boss = &mut self.boss;
        self.bullets.retain_mut(|b| {
            b.prev = b.pos;
            b.pos += b.vel * dt;
            b.traveled += (b.pos - b.prev).len();
            if let Some(e) = enemies.iter_mut().find(|e| {
                e.hp > 0.0 && segment_distance(e.pos, b.prev, b.pos) < e.kind.radius() + 5.0
            }) {
                let damage = bullet_damage(b.traveled) * power;
                e.hp -= damage;
                e.flash = 0.07;
                let knock = if e.kind == Kind::Tank { 30.0 } else { 160.0 };
                e.vel += b.vel.norm() * knock * damage;
                hits.push((
                    e.pos - b.vel.norm() * e.kind.radius(),
                    b.vel,
                    e.kind.hue(),
                    damage,
                ));
                return false;
            }
            if let Some(boss) = boss.as_mut()
                && segment_distance(boss.pos, b.prev, b.pos) < BOSS_RADIUS + 5.0
            {
                let at = boss.pos - b.vel.norm() * BOSS_RADIUS;
                // Invulnerable while flying in: the shot just sparks off.
                let damage = if boss.entering > 0.0 {
                    0.0
                } else {
                    bullet_damage(b.traveled) * power
                };
                boss.hp -= damage;
                boss.flash = 0.05;
                hits.push((at, b.vel, Hue::Violet, damage.max(0.2)));
                return false;
            }
            let lo = WALL;
            if b.pos.x < lo || b.pos.y < lo || b.pos.x > WORLD_W - lo || b.pos.y > WORLD_H - lo {
                wall_hits.push(b.pos);
                return false;
            }
            b.traveled < BULLET_RANGE
        });

        for pos in wall_hits {
            for _ in 0..3 {
                let v = self.rng.dir() * self.rng.range(60.0, 200.0);
                self.emit(pos, v, 0.2, Hue::Cyan, 0);
            }
        }
        if !hits.is_empty() {
            self.sfx.push(Sfx::Hit);
        }
        for (pos, vel, hue, damage) in hits {
            // Close-range hits throw more sparks, so full damage feels heavier.
            for _ in 0..(1 + (damage * 4.0) as usize) {
                let v = -vel.norm() * self.rng.range(80.0, 260.0) + self.rng.dir() * 90.0;
                self.emit(pos, v, 0.25, hue, 0);
            }
        }

        let (dead, alive): (Vec<_>, Vec<_>) = self.enemies.drain(..).partition(|e| e.hp <= 0.0);
        self.enemies = alive;
        for e in dead {
            self.kill(e);
        }
    }

    fn kill(&mut self, e: Enemy) {
        let before = self.multiplier();
        self.chain += 1;
        self.chain_t = 2.2;
        let points = self.award(e.kind.score());
        self.popup(e.pos, format!("+{points}"), false);
        // Each kill in a chain plays the next note up the scale.
        if self.chain >= 2 {
            self.sfx.push(Sfx::Combo((self.chain - 2).min(255) as u8));
        }
        if self.multiplier() > before {
            let mult = self.multiplier();
            self.popup(self.player.pos + v2(0.0, -52.0), format!("X{mult}"), true);
            self.sfx.push(Sfx::LevelUp);
            self.rings.push(Ring {
                pos: self.player.pos,
                t: 0.0,
                dur: 0.45,
                hue: Hue::Yellow,
                inward: false,
            });
        }
        let big = e.kind == Kind::Tank;
        let hue = e.kind.hue();
        self.rings.push(Ring {
            pos: e.pos,
            t: 0.0,
            dur: if big { 0.6 } else { 0.4 },
            hue,
            inward: false,
        });
        let count = if big { 45 } else { 16 };
        for _ in 0..count {
            let v = self.rng.dir() * self.rng.range(90.0, if big { 650.0 } else { 480.0 })
                + e.vel * 0.3;
            let life = self.rng.range(0.3, 0.8);
            let size = if self.rng.chance(0.3) { 2 } else { 1 };
            self.emit(e.pos, v, life, hue, size);
        }
        for _ in 0..count / 3 {
            let v = self.rng.dir() * self.rng.range(40.0, 200.0);
            self.emit(e.pos, v, 0.25, Hue::White, 1);
        }
        self.push_motes(
            e.pos,
            if big { 420.0 } else { 260.0 },
            if big { 420.0 } else { 240.0 },
        );
        self.shake = (self.shake + if big { 10.0 } else { 3.5 }).min(18.0);
        self.sfx
            .push(if big { Sfx::BigExplode } else { Sfx::Explode });
        if big {
            self.hitstop = 0.04;
            self.flash = 0.05;
            for i in 0..3 {
                let dir = V2::from_angle(i as f32 * TAU / 3.0 + self.rng.unit());
                self.add_enemy(Kind::Chaser, e.pos + dir * 26.0, dir * 380.0);
            }
        }
        let drop_chance = if big { 0.45 } else { 0.06 };
        if self.rng.chance(drop_chance) {
            let kind = if self.player.hp < self.player.max_hp && self.rng.chance(0.5) {
                PickupKind::Heart
            } else {
                PickupKind::Rapid
            };
            self.pickups.push(Pickup {
                kind,
                pos: e.pos,
                life: 9.0,
            });
        }
    }

    fn player_collisions(&mut self) {
        let p = &self.player;
        if p.invuln > 0.0 || p.dashing() || self.dev.god {
            return;
        }
        let at = p.pos;
        let mut source = self
            .enemies
            .iter()
            .find(|e| (e.pos - at).len() < e.kind.radius() + PLAYER_RADIUS)
            .map(|e| e.pos);
        if source.is_none() {
            source = self
                .boss
                .as_ref()
                .filter(|b| b.entering <= 0.0 && (b.pos - at).len() < BOSS_RADIUS + PLAYER_RADIUS)
                .map(|b| b.pos);
        }
        if source.is_none()
            && let Some(i) = self
                .orbs
                .iter()
                .position(|o| (o.pos - at).len() < 9.0 + PLAYER_RADIUS)
        {
            source = Some(self.orbs.swap_remove(i).pos);
        }
        if source.is_none()
            && let Some(w) = self
                .shockwaves
                .iter_mut()
                .find(|w| !w.hit && ((at - w.pos).len() - w.r).abs() < 16.0 + PLAYER_RADIUS)
        {
            w.hit = true;
            source = Some(w.pos);
        }
        let Some(source) = source else {
            return;
        };

        let p = &mut self.player;
        p.hp -= 1;
        p.invuln = 1.3;
        p.vel = (p.pos - source).norm() * 700.0;
        let pos = p.pos;
        self.chain = 0;
        self.shake = 16.0;
        self.hitstop = 0.09;
        self.sfx.push(Sfx::Hurt);
        for _ in 0..22 {
            let v = self.rng.dir() * self.rng.range(100.0, 420.0);
            let life = self.rng.range(0.3, 0.7);
            self.emit(pos, v, life, Hue::Cyan, 1);
        }
        // Clear the space around the player so a hit doesn't chain into another.
        for e in &mut self.enemies {
            let d = e.pos - pos;
            if d.len() < 220.0 {
                e.vel = d.norm() * 600.0;
            }
        }
        self.orbs.retain(|o| (o.pos - pos).len() > 160.0);

        if self.player.hp <= 0 {
            self.state = State::Dying;
            self.state_t = 1.8;
            self.slow(1.0, 0.35);
            self.flash = 0.12;
            self.sfx.push(Sfx::BigExplode);
            self.rings.push(Ring {
                pos,
                t: 0.0,
                dur: 0.9,
                hue: Hue::Cyan,
                inward: false,
            });
            self.rings.push(Ring {
                pos,
                t: -0.15,
                dur: 0.9,
                hue: Hue::White,
                inward: false,
            });
            for _ in 0..90 {
                let v = self.rng.dir() * self.rng.range(80.0, 900.0);
                let hue = if self.rng.chance(0.3) {
                    Hue::White
                } else {
                    Hue::Cyan
                };
                let life = self.rng.range(0.5, 1.4);
                self.emit(pos, v, life, hue, 2);
            }
        }
    }

    fn update_pickups(&mut self, dt: f32) {
        let p = self.player.pos;
        let mut collected = Vec::new();
        self.pickups.retain_mut(|k| {
            k.life -= dt;
            let d = p - k.pos;
            if d.len() < 150.0 {
                k.pos += d.norm() * 420.0 * dt;
            }
            if d.len() < 30.0 {
                collected.push((k.kind, k.pos));
                return false;
            }
            k.life > 0.0
        });
        for (kind, pos) in collected {
            match kind {
                PickupKind::Heart => self.player.hp = (self.player.hp + 1).min(self.player.max_hp),
                PickupKind::Rapid => self.player.rapid_t = RAPID_TIME,
            }
            let hue = if kind == PickupKind::Heart {
                Hue::Red
            } else {
                Hue::Yellow
            };
            self.rings.push(Ring {
                pos,
                t: 0.0,
                dur: 0.35,
                hue,
                inward: false,
            });
            self.sfx.push(Sfx::Pickup);
        }
    }

    fn update_effects(&mut self, dt: f32) {
        self.particles.retain_mut(|p| {
            p.pos += p.vel * dt;
            p.vel = p.vel * (-3.2 * dt).exp();
            p.life -= dt;
            p.life > 0.0
        });
        self.rings.retain_mut(|r| {
            r.t += dt;
            r.t < r.dur
        });
        self.ghosts.retain_mut(|g| {
            g.life -= dt;
            g.life > 0.0
        });
        self.popups.retain_mut(|p| {
            p.t += dt;
            p.pos.y -= 60.0 * dt;
            p.t < POPUP_TIME
        });
        let settle = 1.0 - (-1.2 * dt).exp();
        for m in &mut self.motes {
            m.vel += (m.drift - m.vel) * settle;
            m.pos += m.vel * dt;
            m.pos.x = m.pos.x.rem_euclid(WORLD_W);
            m.pos.y = m.pos.y.rem_euclid(WORLD_H);
        }
    }

    /// Blows background motes away from an explosion.
    fn push_motes(&mut self, at: V2, radius: f32, strength: f32) {
        for m in &mut self.motes {
            let d = m.pos - at;
            let len = d.len();
            if len < radius {
                m.vel += d.norm() * strength * (1.0 - len / radius);
            }
        }
    }

    fn popup(&mut self, pos: V2, text: String, combo: bool) {
        if self.popups.len() < 40 {
            self.popups.push(Popup {
                pos,
                text,
                t: 0.0,
                combo,
            });
        }
    }

    fn add_orb(&mut self, pos: V2, vel: V2) {
        if self.orbs.len() < MAX_ORBS {
            self.orbs.push(Orb {
                pos,
                vel,
                life: 6.0,
            });
        }
    }

    /// Slows game time to `scale` for `seconds` of real time.
    fn slow(&mut self, seconds: f32, scale: f32) {
        if self.slowmo <= 0.0 || scale <= self.slowmo_scale {
            self.slowmo = seconds;
            self.slowmo_scale = scale;
        }
    }

    fn emit(&mut self, pos: V2, vel: V2, life: f32, hue: Hue, size: usize) {
        if self.particles.len() < MAX_PARTICLES {
            self.particles.push(Particle {
                pos,
                vel,
                life,
                max: life,
                hue,
                size,
            });
        }
    }
}

/// Damage of a bullet that has flown `traveled` units.
pub fn bullet_damage(traveled: f32) -> f32 {
    let t = ((traveled - FULL_DAMAGE_RANGE) / (BULLET_RANGE - FULL_DAMAGE_RANGE)).clamp(0.0, 1.0);
    1.0 - (1.0 - MIN_DAMAGE) * t
}

fn best_score_path(difficulty: Difficulty) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join(format!("glowstorm/best_score{}", difficulty.file_suffix())))
}

fn load_best(path: Option<&PathBuf>) -> u64 {
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_bullet_path_hits_enemy_between_frames() {
        // A 25-unit step jumps straight over a radius-10 enemy; the swept test must still catch it.
        assert!(segment_distance(v2(12.0, 0.0), v2(0.0, 0.0), v2(25.0, 0.0)) < 10.0);
        assert!(segment_distance(v2(12.0, 30.0), v2(0.0, 0.0), v2(25.0, 0.0)) > 10.0);
    }

    #[test]
    fn damage_is_full_up_close_and_falls_off_with_distance() {
        assert_eq!(bullet_damage(0.0), 1.0);
        assert_eq!(bullet_damage(FULL_DAMAGE_RANGE), 1.0);
        let mid = bullet_damage((FULL_DAMAGE_RANGE + BULLET_RANGE) / 2.0);
        assert!(mid < 1.0 && mid > MIN_DAMAGE, "{mid}");
        assert!((bullet_damage(BULLET_RANGE) - MIN_DAMAGE).abs() < 1e-6);
    }

    #[test]
    fn holding_fire_heats_up_gradually_then_overheats() {
        let mut game = Game::new(7);
        game.start();
        game.dev.god = true;
        let input = Input {
            fire: true,
            ..Input::default()
        };
        let view = View {
            scale: 1.0,
            size: v2(1600.0, 900.0),
        };
        let released = Input::default();
        let run = |game: &mut Game, input: &Input, seconds: f32| {
            for _ in 0..(seconds * 60.0) as usize {
                game.update(1.0 / 60.0, input, &view);
            }
            (game.player.heat, game.player.overheated)
        };
        let (heat, overheated) = run(&mut game, &input, 1.5);
        assert!(
            heat > 0.25 && heat < 0.7 && !overheated,
            "after 1.5 s: {heat}"
        );
        let (_, overheated) = run(&mut game, &input, 1.5);
        assert!(overheated, "should overheat within 3 s of continuous fire");
        let (heat, overheated) = run(&mut game, &released, 1.5);
        assert!(
            !overheated && heat <= HEAT_RESUME,
            "gun unlocks after cooling: {heat}"
        );
    }

    #[test]
    fn enemies_double_while_the_player_grows_a_fifth_by_wave_eleven() {
        let mut game = Game::new(9);
        game.wave = 11;
        assert!(
            (game.power() - 1.2).abs() < 1e-5,
            "player power {}",
            game.power()
        );
        assert!((game.enemy_hp_scale() - 1.6).abs() < 1e-5);
        assert!(
            game.enemy_cooldown_scale() < 0.75,
            "enemies attack more often"
        );
        game.wave = 60;
        assert!(
            game.power() <= 1.3 && game.enemy_cooldown_scale() >= 0.45,
            "both are capped"
        );
    }

    #[test]
    fn difficulty_sets_hearts_and_keeps_separate_best_scores() {
        let mut game = Game::new(9);
        game.set_difficulty(Difficulty::Extreme);
        assert_eq!(game.player.max_hp, 3);
        let paths: Vec<_> = Difficulty::ALL
            .iter()
            .map(|&d| best_score_path(d))
            .collect();
        assert!(paths[0] != paths[1] && paths[1] != paths[2]);
    }

    #[test]
    fn multiplier_grows_with_chain_and_caps() {
        let mut game = Game::new(1);
        assert_eq!(game.multiplier(), 1);
        game.chain = 12;
        assert_eq!(game.multiplier(), 3);
        game.chain = 500;
        assert_eq!(game.multiplier(), 8);
    }
}
#[cfg(test)]
mod boss_tests {
    use super::*;

    #[test]
    fn boss_lands_away_from_a_centered_player_and_keeps_distance() {
        let mut game = Game::new(3);
        game.start();
        game.dev.god = true;
        game.start_wave(5);
        let view = View {
            scale: 1.0,
            size: v2(1600.0, 900.0),
        };
        let idle = Input::default();
        for _ in 0..(60 * 9) {
            game.update(1.0 / 60.0, &idle, &view);
        }
        let boss = game.boss.as_ref().expect("boss spawned on wave 5");
        let dist = (boss.pos - game.player.pos).len();
        assert!(dist > 200.0, "boss sits {dist} units from the player");
    }

    #[test]
    fn beating_the_boss_pays_out_and_moves_to_the_next_wave() {
        let mut game = Game::new(5);
        game.start();
        game.dev.god = true;
        game.start_wave(5);
        let view = View {
            scale: 1.0,
            size: v2(1600.0, 900.0),
        };
        let idle = Input::default();
        let run = |game: &mut Game, seconds: f32| {
            for _ in 0..(seconds * 60.0) as usize {
                game.update(1.0 / 60.0, &idle, &view);
            }
        };
        run(&mut game, 6.0);
        let score = game.score;
        game.boss.as_mut().expect("boss is up").hp = 0.0;
        run(&mut game, 0.5);
        assert!(game.boss.is_none(), "boss should be gone");
        assert!(game.score >= score + 1500, "boss bounty paid");
        assert!(game.pickups.len() >= 3, "boss drops loot");
        run(&mut game, 6.0);
        assert_eq!(game.wave, 6, "next wave starts after the boss");
    }
}
