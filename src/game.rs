//! Glowstorm: the gameplay. Pure simulation; drawing lives in render.rs.

use std::{
    collections::HashSet,
    f32::consts::TAU,
    ops::{Add, AddAssign, Mul, Neg, Sub},
    path::PathBuf,
};

use crossterm::event::KeyCode;
use catnip::audio::Sfx;

use crate::art::{Hue, WALL, WORLD_H, WORLD_W};

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

pub struct Enemy {
    pub kind: Kind,
    pub pos: V2,
    pub vel: V2,
    hp: f32,
    pub flash: f32,
    pub spin: f32,
    pub facing: f32,
    cooldown: f32,
    phase: f32,
}

pub struct Bullet {
    pub pos: V2,
    prev: V2,
    pub vel: V2,
    life: f32,
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
}

pub const MAX_HP: i32 = 5;
const PLAYER_RADIUS: f32 = 14.0;
const ACCEL: f32 = 3400.0;
const DRAG: f32 = 7.0;
const BULLET_SPEED: f32 = 1250.0;
const DASH_SPEED: f32 = 1350.0;
const DASH_TIME: f32 = 0.17;
const DASH_COOLDOWN: f32 = 0.8;
const SPAWN_TIME: f32 = 0.9;
const MAX_PARTICLES: usize = 900;
const RAPID_TIME: f32 = 7.0;

impl Player {
    fn new() -> Self {
        Self {
            pos: v2(WORLD_W / 2.0, WORLD_H / 2.0),
            vel: V2::default(),
            aim: -TAU / 4.0,
            hp: MAX_HP,
            invuln: 0.0,
            fire_cd: 0.0,
            dash_cd: 0.0,
            dash_t: 0.0,
            rapid_t: 0.0,
            ghost_cd: 0.0,
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
    wave_clear: f32,
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
        let best_path = best_score_path();
        let best = best_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        Self {
            state: State::Title,
            player: Player::new(),
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
        }
    }

    pub fn multiplier(&self) -> u64 {
        (1 + self.chain / 6).min(8) as u64
    }

    fn start(&mut self) {
        let best = self.best;
        let seed = self.rng.next();
        *self = Game::new(seed);
        self.best = best;
        self.state = State::Playing;
        self.start_wave(self.dev.first_wave);
    }

    fn start_wave(&mut self, wave: u32) {
        self.wave = wave;
        self.budget = 6 + 4 * wave as i32;
        self.banner = 2.0;
        self.spawn_cd = 0.0;
        self.sfx.push(Sfx::Wave);
    }

    pub fn on_focus_lost(&mut self) {
        if self.state == State::Playing {
            self.state = State::Paused;
        }
    }

    pub fn update(&mut self, dt: f32, input: &Input, view: &View) {
        self.time += dt;
        let confirm = input.clicked || input.hit(&[KeyCode::Enter]);
        match self.state {
            State::Title => {
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
        let firing = input.fire || arrows != V2::default();
        if firing && p.fire_cd <= 0.0 {
            let rapid = p.rapid_t > 0.0;
            let interval = if rapid { 0.055 } else { 0.11 };
            p.fire_cd = (p.fire_cd + interval).max(0.0);
            let spread = if rapid { 0.09 } else { 0.035 };
            let dir = V2::from_angle(p.aim + rng.range(-spread, spread));
            let muzzle = p.pos + dir * 22.0;
            bullets.push(Bullet {
                pos: muzzle,
                prev: p.pos,
                vel: dir * BULLET_SPEED + p.vel * 0.25,
                life: 0.9,
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
        self.update_bullets(dt);
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
        } else if self.spawns.is_empty() && self.enemies.is_empty() {
            self.wave_clear = 1.4;
            self.score += 100 * self.wave as u64;
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
            hp: kind.hp(),
            flash: 0.0,
            spin: phase,
            facing: (self.player.pos - pos).angle(),
            cooldown,
            phase,
        });
    }

    fn update_enemies(&mut self, dt: f32) {
        let target = self.player.pos;
        let chasing = self.state == State::Playing;
        let speedup = 1.0 + 0.04 * self.wave as f32;
        let mut shots = Vec::new();
        for e in &mut self.enemies {
            let to_player = target - e.pos;
            let dist = to_player.len();
            let dir = to_player.norm();
            e.flash -= dt;
            let (desired, steer) = match e.kind {
                Kind::Chaser => (dir * 230.0 * speedup, 3.0),
                Kind::Swarm => {
                    let wobble = dir.perp() * ((self.time * 7.0 + e.phase).sin() * 220.0);
                    (dir * 330.0 * speedup + wobble, 4.0)
                }
                Kind::Shooter => {
                    let radial = if dist > 460.0 {
                        1.0
                    } else if dist < 320.0 {
                        -1.0
                    } else {
                        0.0
                    };
                    let strafe = if e.phase > TAU / 2.0 { 1.0 } else { -1.0 };
                    e.cooldown -= dt;
                    if e.cooldown <= 0.0 && dist < 950.0 && chasing {
                        e.cooldown = 1.4 + (e.phase * 7.0).fract() * 0.9;
                        shots.push((e.pos + dir * 22.0, dir));
                    }
                    (dir * radial * 210.0 + dir.perp() * strafe * 120.0, 2.5)
                }
                Kind::Tank => (dir * 85.0 * speedup, 1.5),
            };
            let desired = if chasing { desired } else { V2::default() };
            e.vel += (desired - e.vel) * (1.0 - (-steer * dt).exp());
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
            e.spin += dt
                * match e.kind {
                    Kind::Chaser => 4.0,
                    Kind::Swarm => 7.0,
                    Kind::Tank => 0.8,
                    Kind::Shooter => 0.0,
                };
            // Shooters turn smoothly to face the player.
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

        for (pos, dir) in shots {
            self.orbs.push(Orb {
                pos,
                vel: dir * 340.0,
                life: 4.0,
            });
            self.sfx.push(Sfx::EnemyShoot);
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
        let enemies = &mut self.enemies;
        self.bullets.retain_mut(|b| {
            b.prev = b.pos;
            b.pos += b.vel * dt;
            b.life -= dt;
            if let Some(e) = enemies.iter_mut().find(|e| {
                e.hp > 0.0 && segment_distance(e.pos, b.prev, b.pos) < e.kind.radius() + 5.0
            }) {
                e.hp -= 1.0;
                e.flash = 0.07;
                let knock = if e.kind == Kind::Tank { 30.0 } else { 160.0 };
                e.vel += b.vel.norm() * knock;
                hits.push((e.pos - b.vel.norm() * e.kind.radius(), b.vel, e.kind.hue()));
                return false;
            }
            let lo = WALL;
            if b.pos.x < lo || b.pos.y < lo || b.pos.x > WORLD_W - lo || b.pos.y > WORLD_H - lo {
                wall_hits.push(b.pos);
                return false;
            }
            b.life > 0.0
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
        for (pos, vel, hue) in hits {
            for _ in 0..3 {
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
        self.chain += 1;
        self.chain_t = 2.2;
        self.score += e.kind.score() * self.multiplier();
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
        self.shake = (self.shake + if big { 10.0 } else { 3.5 }).min(18.0);
        self.sfx
            .push(if big { Sfx::BigExplode } else { Sfx::Explode });
        if big {
            self.hitstop = 0.04;
            for i in 0..3 {
                let dir = V2::from_angle(i as f32 * TAU / 3.0 + self.rng.unit());
                self.add_enemy(Kind::Chaser, e.pos + dir * 26.0, dir * 380.0);
            }
        }
        let drop_chance = if big { 0.45 } else { 0.06 };
        if self.rng.chance(drop_chance) {
            let kind = if self.player.hp < MAX_HP && self.rng.chance(0.5) {
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
        let touched = self
            .enemies
            .iter()
            .find(|e| (e.pos - p.pos).len() < e.kind.radius() + PLAYER_RADIUS)
            .map(|e| e.pos);
        let orb = self
            .orbs
            .iter()
            .position(|o| (o.pos - p.pos).len() < 9.0 + PLAYER_RADIUS);
        let source = match (touched, orb) {
            (Some(pos), _) => pos,
            (None, Some(i)) => self.orbs.swap_remove(i).pos,
            (None, None) => return,
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
                PickupKind::Heart => self.player.hp = (self.player.hp + 1).min(MAX_HP),
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

fn best_score_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("glowstorm/best_score"))
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
    fn multiplier_grows_with_chain_and_caps() {
        let mut game = Game::new(1);
        assert_eq!(game.multiplier(), 1);
        game.chain = 12;
        assert_eq!(game.multiplier(), 3);
        game.chain = 500;
        assert_eq!(game.multiplier(), 8);
    }
}
