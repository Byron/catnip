//! Hits and kills: player bullets, damage to the player, shield orbs.

use super::*;

pub struct Bullet {
    pub pos: V2,
    pub(super) prev: V2,
    pub vel: V2,
    /// Distance flown so far; damage falls off with it.
    pub traveled: f32,
    /// Distance at which it fades out (Long Range raises it).
    pub range: f32,
    /// Damage multiplier: side bullets of a spread shot hit softer.
    pub(super) damage: f32,
    /// Enemies it can still pass through.
    pub(super) pierce: u8,
    /// Last enemy hit, so a piercing bullet doesn't hit it twice.
    pub(super) last_hit: u32,
    /// Loses damage with distance (off with Long Range).
    pub(super) falloff: bool,
}

impl Bullet {
    /// Damage multiplier from distance flown and the shot's spread share.
    fn strength(&self) -> f32 {
        let range = if self.falloff {
            bullet_damage(self.traveled, self.range)
        } else {
            1.0
        };
        range * self.damage
    }
}

pub(super) const BULLET_SPEED: f32 = 1250.0;

/// Bullets fade out after this distance.
pub const BULLET_RANGE: f32 = 800.0;

/// Full damage up to this distance, then linear falloff to MIN_DAMAGE at BULLET_RANGE.
pub(super) const FULL_DAMAGE_RANGE: f32 = 220.0;

pub(super) const MIN_DAMAGE: f32 = 0.75;

pub(super) const SHIELD_DPS: f32 = 7.0;

pub struct Gatling {
    pub charge: f32,
    pub boosted: bool,
    /// Damage per pellet at the beginning of the barrage.
    pub(super) damage: f32,
    pub life: f32,
    pub duration: f32,
    pub range: f32,
    pub(super) fire_cd: f32,
}

impl Gatling {
    pub fn strength(&self) -> f32 {
        (self.life / self.duration).clamp(0.0, 1.0)
    }
}

/// Damage of a bullet that has flown `traveled` units out of its `range`.
pub fn bullet_damage(traveled: f32, range: f32) -> f32 {
    let full = FULL_DAMAGE_RANGE * range / BULLET_RANGE;
    let t = ((traveled - full) / (range - full)).clamp(0.0, 1.0);
    1.0 - (1.0 - MIN_DAMAGE) * t
}

impl Game {
    pub(super) fn update_gatling(&mut self, dt: f32) {
        let Some(gun) = &mut self.gatling else { return };
        let active = dt.min(gun.life);
        let interval = if gun.boosted { 0.025 } else { 0.035 };
        let aim = self.player.aim;
        let dir = V2::from_angle(aim);
        let muzzle = self.player.pos + dir * 22.0;
        let speed = BULLET_SPEED * if gun.boosted { 1.6 } else { 1.25 };
        let spread = self.player.upgrades.level(Upgrade::Spread);
        let pierce = self.player.upgrades.level(Upgrade::Pierce);
        let falloff = self.player.upgrades.level(Upgrade::Range) == 0;
        let mut flashes = Vec::new();
        while gun.fire_cd < active {
            let strength = (gun.life - gun.fire_cd) / gun.duration;
            let damage = gun.damage * (0.35 + 0.65 * strength);
            let aim = aim + self.rng.range(-0.025, 0.025);
            for i in -(1 + i32::from(spread))..=1 + i32::from(spread) {
                let d = V2::from_angle(aim + i as f32 * 0.12);
                let damage = damage * if i.abs() > 1 { 0.35 } else { 1.0 };
                self.bullets.push(Bullet {
                    pos: muzzle,
                    prev: self.player.pos,
                    vel: d * speed + self.player.vel * 0.25,
                    traveled: 0.0,
                    range: gun.range,
                    damage,
                    pierce,
                    last_hit: 0,
                    falloff,
                });
                self.player.vel += -d * 3.0 * damage;
            }
            self.sfx.push(Sfx::Gatling);
            flashes.push(dir * 280.0 + self.rng.dir() * 100.0);
            gun.fire_cd += interval;
        }
        gun.fire_cd -= active;
        gun.life -= active;
        if gun.life <= 0.0 {
            self.gatling = None;
        }
        for vel in flashes {
            self.emit(muzzle, vel, 0.08, Hue::Yellow, 1);
        }
    }

    pub(super) fn update_bullets(&mut self, dt: f32) {
        let mut wall_hits = Vec::new();
        let mut hits = Vec::new();
        let power = self.power();
        let size = self.arena_size;
        let enemies = &mut self.enemies;
        let boss = &mut self.boss;
        let orbs = &mut self.orbs;
        self.bullets.retain_mut(|b| {
            b.prev = b.pos;
            let step = b.vel * dt;
            let distance = step.len();
            let mut flight = if distance > 0.0 {
                ((b.range - b.traveled) / distance).clamp(0.0, 1.0)
            } else {
                1.0
            };
            for (pos, delta, edge) in [(b.pos.x, step.x, size.x), (b.pos.y, step.y, size.y)] {
                if delta != 0.0 {
                    let wall = if delta > 0.0 { edge - WALL } else { WALL };
                    flight = flight.min(((wall - pos) / delta).clamp(0.0, 1.0));
                }
            }
            b.pos += step * flight;
            b.traveled += (b.pos - b.prev).len();
            // Relative motion tests where both cores are at the same time, excluding glow.
            let interception = orbs
                .iter()
                .enumerate()
                .filter_map(|(i, o)| {
                    segment_hit(
                        V2::default(),
                        b.prev - o.prev,
                        b.pos - (o.prev + (o.pos - o.prev) * flight),
                        o.radius() + 2.0,
                    )
                    .filter(|t| {
                        let at = o.prev + (o.pos - o.prev) * (*t * flight);
                        o.life + dt >= *t * flight * dt
                            && at.x > WALL
                            && at.y > WALL
                            && at.x < size.x - WALL
                            && at.y < size.y - WALL
                    })
                    .map(|t| (i, t))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let boss_hit = boss
                .as_ref()
                .and_then(|boss| segment_hit(boss.pos, b.prev, b.pos, BOSS_RADIUS + 5.0));
            let barrier = interception
                .map_or(1.0, |(_, t)| t)
                .min(boss_hit.unwrap_or(1.0));
            let mut targets: Vec<_> = enemies
                .iter()
                .enumerate()
                .filter(|(_, e)| e.hp > 0.0 && e.id != b.last_hit)
                .filter_map(|(i, e)| {
                    segment_hit(e.pos, b.prev, b.pos, e.kind.radius() + 5.0)
                        .filter(|t| *t <= barrier)
                        .map(|t| (i, t))
                })
                .collect();
            targets.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
            for (i, _) in targets {
                let e = &mut enemies[i];
                let damage = b.strength() * power;
                e.hp -= damage;
                e.flash = 0.07;
                let knock = e.kind.spec().knockback;
                e.impulse += b.vel * (knock * damage / BULLET_SPEED);
                hits.push((
                    e.pos - b.vel.norm() * e.kind.radius(),
                    b.vel,
                    e.kind.hue(),
                    damage,
                ));
                if b.pierce == 0 {
                    return false;
                }
                b.pierce -= 1;
                b.last_hit = e.id;
            }
            if boss_hit.is_some_and(|t| t <= barrier)
                && let Some(boss) = boss.as_mut()
            {
                let at = boss.pos - b.vel.norm() * BOSS_RADIUS;
                // Invulnerable while flying in: the shot just sparks off.
                let damage = if boss.entering > 0.0 {
                    0.0
                } else {
                    b.strength() * power
                };
                boss.hp -= damage;
                boss.impulse += b.vel * (0.5 * damage / BULLET_SPEED);
                boss.flash = 0.05;
                hits.push((at, b.vel, Hue::Violet, damage.max(0.2)));
                return false;
            }
            if let Some((i, t)) = interception {
                let at = b.prev + (b.pos - b.prev) * t;
                orbs.swap_remove(i);
                hits.push((at, b.vel, Hue::Red, 0.5));
                return false;
            }
            let lo = WALL;
            if b.pos.x <= lo || b.pos.y <= lo || b.pos.x >= size.x - lo || b.pos.y >= size.y - lo {
                wall_hits.push(b.pos);
                return false;
            }
            b.traveled < b.range
        });
        // Keep the final sweep of escaping or expiring orbs available for interception.
        self.orbs.retain(|o| {
            o.life > 0.0
                && o.pos.x > WALL
                && o.pos.y > WALL
                && o.pos.x < size.x - WALL
                && o.pos.y < size.y - WALL
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

    pub(super) fn kill(&mut self, e: Enemy) {
        let before = self.multiplier();
        self.chain += 1;
        self.chain_t = 2.2;
        let points = self.award(e.kind.spec().score * if e.elite { 3 } else { 1 });
        self.popup(e.pos, format!("+{points}"), e.elite);
        if e.elite {
            self.flash = 0.06;
            self.rings.push(Ring {
                pos: e.pos,
                t: -0.1,
                dur: 0.5,
                hue: Hue::Yellow,
                inward: false,
            });
        }
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
        let big = e.kind.spec().big_death;
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
                + (e.vel + e.impulse) * 0.3;
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
        }
        if let Some((kind, count)) = e.kind.spec().splits_into {
            for i in 0..count {
                let dir = V2::from_angle(i as f32 * TAU / count as f32 + self.rng.unit());
                self.add_enemy(kind, e.pos + dir * 26.0, dir * 380.0, false);
            }
        }
        let drop_chance = if e.elite {
            1.0
        } else if big {
            0.45
        } else {
            0.06
        };
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

    pub(super) fn player_collisions(&mut self) {
        let p = &self.player;
        if p.invuln > 0.0 || p.dashing() {
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
                .position(|o| (o.pos - at).len() < o.radius() + PLAYER_RADIUS)
        {
            source = Some(self.orbs.swap_remove(i).pos);
        }
        if source.is_none()
            && let Some(w) = self.shockwaves.iter_mut().find(|w| {
                !w.hit && w.r > 0.0 && ((at - w.pos).len() - w.r).abs() < 16.0 + PLAYER_RADIUS
            })
        {
            w.hit = true;
            source = Some(w.pos);
        }
        let Some(source) = source else {
            return;
        };

        let p = &mut self.player;
        p.hp -= 1;
        if self.dev.god {
            p.hp = p.hp.max(1);
        }
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

    /// Shield orbs eat enemy bullets and burn whatever they touch.
    pub(super) fn update_shields(&mut self, dt: f32) {
        let shields = self.player.shield_orbs(self.time);
        if shields.is_empty() {
            return;
        }
        let burn = SHIELD_DPS * self.power() * dt;
        let mut blocked = Vec::new();
        for &at in &shields {
            self.orbs.retain(|o| {
                let hit = (o.pos - at).len() < o.radius() + 9.0;
                if hit {
                    blocked.push(o.pos);
                }
                !hit
            });
            for e in &mut self.enemies {
                if (e.pos - at).len() < e.kind.radius() + 12.0 {
                    e.hp -= burn;
                    e.flash = 0.05;
                }
            }
            if let Some(b) = self.boss.as_mut()
                && b.entering <= 0.0
                && (b.pos - at).len() < BOSS_RADIUS + 12.0
            {
                b.hp -= burn;
                b.flash = 0.05;
            }
        }
        for pos in blocked {
            for _ in 0..4 {
                let v = self.rng.dir() * self.rng.range(60.0, 200.0);
                self.emit(pos, v, 0.25, Hue::Cyan, 0);
            }
        }
    }
}
