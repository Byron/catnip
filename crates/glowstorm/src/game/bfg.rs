//! The charged plasma ball pierces ships before bursting into green lightning.

use super::*;

pub struct Bfg {
    pub pos: V2,
    pub vel: V2,
    pub charge: f32,
    pub(super) damage: f32,
    pub(super) range: f32,
    traveled: f32,
    boss_hit: bool,
}

impl Bfg {
    pub(super) fn new(pos: V2, vel: V2, charge: f32, damage: f32, range: f32) -> Self {
        Self {
            pos,
            vel,
            charge,
            damage,
            range,
            traveled: 0.0,
            boss_hit: false,
        }
    }

    pub fn radius(&self) -> f32 {
        24.0 + 56.0 * self.charge
    }

    pub fn blast_radius(&self) -> f32 {
        300.0 + 380.0 * self.charge
    }
}

impl Game {
    pub(super) fn detonate_bfgs(&mut self) {
        for ball in std::mem::take(&mut self.bfgs) {
            self.bfg_blast(ball.pos, ball.blast_radius(), ball.damage, 1.0, 0.0);
        }
    }

    pub(super) fn update_bfgs(&mut self, dt: f32) {
        let mut killed = Vec::new();
        let mut impacts = Vec::new();
        let mut trails = Vec::new();
        let mut blasts = Vec::new();
        let enemies = &mut self.enemies;
        let boss = &mut self.boss;
        let orbs = &mut self.orbs;
        let size = self.arena_size;
        self.bfgs.retain_mut(|ball| {
            let radius = ball.radius();
            let from = ball.pos;
            let dir = ball.vel.norm();
            let step = ball.vel * dt;
            let distance = step.len();
            let lo = v2(WALL + radius, WALL + radius);
            let hi = size - lo;
            let outside = from.x <= lo.x || from.y <= lo.y || from.x >= hi.x || from.y >= hi.y;
            let wall = if outside {
                0.0
            } else {
                ray_exit(from, dir, lo, hi)
            };
            let remaining = (ball.range - ball.traveled).max(0.0).min(wall);
            let flight = if distance > 0.0 {
                (remaining / distance).clamp(0.0, 1.0)
            } else if remaining <= 0.0 {
                0.0
            } else {
                1.0
            };
            ball.pos += step * flight;

            killed.extend(enemies.extract_if(.., |e| {
                if e.hp > 0.0
                    && let Some(t) = segment_hit(e.pos, from, ball.pos, radius + e.kind.radius())
                {
                    impacts.push((from + (ball.pos - from) * t, e.pos));
                    e.impulse += dir * 900.0;
                    true
                } else {
                    false
                }
            }));
            if let Some(b) = boss.as_mut()
                && b.entering <= 0.0
                && b.hp > 0.0
                && !ball.boss_hit
                && let Some(t) = segment_hit(b.pos, from, ball.pos, radius + BOSS_RADIUS)
            {
                b.hp -= ball.damage;
                b.stagger(ball.damage);
                b.stun = b.stun.max(0.8);
                b.flash = 0.15;
                b.impulse += dir * 600.0;
                if b.impulse.len() > 600.0 {
                    b.impulse = b.impulse.norm() * 600.0;
                }
                ball.boss_hit = true;
                impacts.push((from + (ball.pos - from) * t, b.pos));
            }
            orbs.retain(|o| {
                let end = o.prev + (o.pos - o.prev) * flight;
                let hit = segment_hit(
                    V2::default(),
                    from - o.prev,
                    ball.pos - end,
                    radius + o.radius(),
                )
                .is_some_and(|t| {
                    let time = t * flight;
                    let at = o.prev + (o.pos - o.prev) * time;
                    o.life + dt >= time * dt
                        && at.x > WALL
                        && at.y > WALL
                        && at.x < size.x - WALL
                        && at.y < size.y - WALL
                });
                !hit
            });

            let traveled = ball.traveled + (ball.pos - from).len();
            // Distance-spaced sparks keep the trail equally dense at every frame rate.
            for i in (ball.traveled / 24.0).floor() as u32 + 1..=(traveled / 24.0).floor() as u32 {
                trails.push((from + dir * (i as f32 * 24.0 - ball.traveled), dir, radius));
            }
            ball.traveled = traveled;
            if distance >= remaining || outside {
                blasts.push((ball.pos, ball.blast_radius(), ball.damage, flight));
                false
            } else {
                true
            }
        });
        for (pos, dir, radius) in trails {
            for side in [-1.0, 1.0] {
                let at = pos + dir.perp() * (side * radius * 0.6);
                self.emit(at, -dir * 90.0, 0.3, Hue::Green, 2);
            }
        }
        for (from, at) in impacts {
            self.bfg_impact(from, at);
        }
        for e in killed {
            self.kill(e, false);
        }
        for (pos, radius, damage, flight) in blasts {
            self.bfg_blast(pos, radius, damage, flight, dt);
        }
    }

    fn bfg_impact(&mut self, from: V2, at: V2) {
        for i in 0..10 {
            let pos = from + (at - from) * (i as f32 / 10.0);
            let vel = self.rng.dir() * 45.0;
            self.emit(pos, vel, 0.3, Hue::Green, 1);
        }
        for i in 0..24 {
            let vel = self.rng.dir() * self.rng.range(100.0, 650.0);
            self.emit(
                at,
                vel,
                0.55,
                if i % 5 == 0 { Hue::White } else { Hue::Green },
                2,
            );
        }
    }

    fn bfg_blast(&mut self, pos: V2, radius: f32, damage: f32, flight: f32, dt: f32) {
        let outward = |at: V2| {
            let offset = at - pos;
            if offset.len() > 1e-6 {
                offset.norm()
            } else {
                v2(1.0, 0.0)
            }
        };
        let mut targets = Vec::new();
        let killed: Vec<_> = self
            .enemies
            .extract_if(.., |e| {
                let distance = ((e.pos - pos).len() - e.kind.radius()).max(0.0);
                if e.hp > 0.0 && distance < radius {
                    let strength = 1.0 - distance / radius;
                    e.hp -= damage * (0.35 + 0.65 * strength);
                    e.stagger(damage);
                    e.stun = e.stun.max(1.5 + strength);
                    e.flash = 0.15;
                    e.impulse += outward(e.pos) * (400.0 + 700.0 * strength);
                    if e.impulse.len() > 1100.0 {
                        e.impulse = e.impulse.norm() * 1100.0;
                    }
                    targets.push(e.pos);
                    e.hp <= 0.0
                } else {
                    false
                }
            })
            .collect();
        if let Some(b) = &mut self.boss {
            let distance = ((b.pos - pos).len() - BOSS_RADIUS).max(0.0);
            if b.hp > 0.0 && b.entering <= 0.0 && distance < radius {
                let strength = 1.0 - distance / radius;
                b.hp -= damage * (0.35 + 0.65 * strength);
                b.stagger(damage);
                b.stun = b.stun.max(0.8);
                b.flash = 0.15;
                b.impulse += outward(b.pos) * (250.0 + 350.0 * strength);
                if b.impulse.len() > 600.0 {
                    b.impulse = b.impulse.norm() * 600.0;
                }
                targets.push(b.pos);
            }
        }
        self.orbs.retain(|o| {
            let at = o.prev + (o.pos - o.prev) * flight;
            o.life + dt < flight * dt || (at - pos).len() >= radius + o.radius()
        });
        for target in targets {
            self.bfg_impact(pos, target);
        }
        for e in killed {
            self.kill(e, false);
        }
        self.sfx.push(Sfx::BigExplode);
        self.shake = 18.0;
        self.flash = 0.12;
        self.rings.push(Ring {
            pos,
            t: 0.0,
            dur: 0.8,
            hue: Hue::Green,
            inward: false,
        });
        self.rings.push(Ring {
            pos,
            t: -0.12,
            dur: 0.6,
            hue: Hue::White,
            inward: false,
        });
        for i in 0..48 {
            let dir = V2::from_angle(i as f32 * TAU / 48.0);
            self.emit(pos + dir * radius, dir * 110.0, 0.45, Hue::Green, 2);
            for step in 1..=3 {
                let at = pos + dir * (radius * step as f32 / 4.0);
                self.emit(
                    at,
                    dir * 180.0,
                    0.5,
                    if i % 4 == 0 { Hue::White } else { Hue::Green },
                    2,
                );
            }
        }
        self.push_motes(pos, radius * 1.3, 700.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bfg_annihilates_swept_hulls_without_splits_and_hits_each_boss_once() {
        let mut game = Game::new(7);
        game.add_enemy(Kind::Tank, v2(600.0, 670.0), V2::default(), true);
        game.add_enemy(Kind::Chaser, v2(1200.0, 600.0), V2::default(), false);
        game.spawn_boss();
        let boss = game.boss.as_mut().unwrap();
        boss.pos = v2(850.0, 600.0);
        boss.entering = 0.0;
        boss.hp = 2000.0;
        game.bfgs.push(Bfg::new(
            v2(300.0, 600.0),
            v2(1000.0, 0.0),
            0.5,
            300.0,
            2000.0,
        ));
        game.update_bfgs(0.8);
        assert_eq!(game.enemies.len(), 1, "tanks leave no surviving children");
        assert_eq!(game.enemies[0].pos.x, 1200.0);
        assert_eq!(game.boss.as_ref().unwrap().hp, 1700.0);
        assert_eq!(game.bfgs.len(), 1, "ships cannot stop the ball");
        game.bfgs[0].vel = v2(-1000.0, 0.0);
        game.update_bfgs(0.3);
        assert_eq!(game.boss.as_ref().unwrap().hp, 1700.0);
        assert!(game.particles.iter().any(|p| p.hue == Hue::Green));
    }

    #[test]
    fn bfg_disk_sweep_distinguishes_grazing_hits_from_near_misses() {
        let mut game = Game::new(7);
        for y in [639.9, 640.1] {
            game.add_enemy(Kind::Chaser, v2(650.0, y), V2::default(), false);
        }
        game.bfgs.push(Bfg::new(
            v2(300.0, 600.0),
            v2(1000.0, 0.0),
            0.0,
            120.0,
            2000.0,
        ));
        game.update_bfgs(0.6);
        assert_eq!(game.enemies.len(), 1);
        assert_eq!(game.enemies[0].pos.y, 640.1);
    }

    #[test]
    fn bfg_clips_its_sweep_at_range_and_at_the_edge_of_its_ball() {
        for wall in [false, true] {
            let mut game = Game::new(7);
            game.arena_size = v2(1000.0, 1500.0);
            let end = if wall { 1000.0 - WALL - 24.0 } else { 500.0 };
            game.add_enemy(Kind::Chaser, v2(end + 80.0, 600.0), V2::default(), false);
            game.enemies[0].hp = 10000.0;
            game.bfgs.push(Bfg::new(
                v2(300.0, 600.0),
                v2(1000.0, 0.0),
                0.0,
                120.0,
                if wall { 2000.0 } else { 200.0 },
            ));
            game.update_bfgs(1.0);
            assert!(game.bfgs.is_empty());
            assert_eq!(game.rings[0].pos, v2(end, 600.0));
            assert_eq!(
                game.enemies.len(),
                1,
                "beyond the sweep, only blast damage applies"
            );
            assert!(game.enemies[0].hp < 10000.0 && game.enemies[0].hp >= 9880.0);
        }
    }

    #[test]
    fn bfg_intercepts_orbs_only_where_their_live_cores_meet_in_time() {
        let mut game = Game::new(7);
        for (from, to, life) in [
            (v2(600.0, 300.0), v2(600.0, 700.0), 2.0),
            (v2(600.0, 400.0), v2(600.0, 800.0), 2.0),
            (v2(600.0, 500.0), v2(600.0, 500.0), -0.5),
        ] {
            game.orbs.push(Orb {
                pos: to,
                prev: from,
                vel: V2::default(),
                life,
                heavy: false,
            });
        }
        game.bfgs.push(Bfg::new(
            v2(300.0, 500.0),
            v2(1000.0, 0.0),
            0.0,
            120.0,
            2000.0,
        ));
        game.update_bfgs(0.6);
        assert_eq!(game.orbs.len(), 2);
        assert_eq!(game.orbs[0].pos, v2(600.0, 800.0));
        assert!(
            game.orbs[1].life < 0.0,
            "expired orbs await the shared cleanup"
        );
    }

    #[test]
    fn bfg_blast_has_falloff_and_push_without_self_damage_or_entrance_damage() {
        let mut game = Game::new(7);
        let center = v2(700.0, 700.0);
        game.player.pos = center;
        let hp = game.player.hp;
        for x in [0.0, 200.0, 332.0] {
            game.add_enemy(Kind::Tank, center + v2(x, 0.0), V2::default(), true);
            game.enemies.last_mut().unwrap().hp = 10000.0;
        }
        game.spawn_boss();
        game.boss.as_mut().unwrap().pos = center;
        let boss_hp = game.boss.as_ref().unwrap().hp;
        game.bfg_blast(center, 300.0, 120.0, 1.0, 0.0);
        assert_eq!(game.player.hp, hp);
        assert_eq!(game.player.vel, V2::default());
        assert_eq!(game.enemies[0].hp, 9880.0);
        assert!(game.enemies[0].hp < game.enemies[1].hp && game.enemies[1].hp < 10000.0);
        assert_eq!(
            game.enemies[2].hp, 10000.0,
            "hull outside the blast is safe"
        );
        assert!(game.enemies[0].impulse.x > 1000.0 && game.enemies[1].stun > 1.5);
        assert_eq!(game.boss.as_ref().unwrap().hp, boss_hp);
        game.boss.as_mut().unwrap().entering = 0.0;
        game.bfg_blast(center, 300.0, 120.0, 1.0, 0.0);
        assert_eq!(game.boss.as_ref().unwrap().hp, boss_hp - 120.0);
        assert!(game.boss.as_ref().unwrap().impulse.len() <= 600.0);
        assert!(game.particles.len() <= MAX_PARTICLES);
    }

    #[test]
    fn bfg_trails_are_frame_rate_independent_and_every_ball_expires() {
        for fps in [1, 30, 60, 144] {
            let mut game = Game::new(7);
            game.bfgs.push(Bfg::new(
                v2(300.0, 600.0),
                v2(560.0, 0.0),
                0.0,
                120.0,
                1000.0,
            ));
            let mut emitted = 0;
            for _ in 0..fps * 3 {
                game.update_bfgs(1.0 / fps as f32);
                emitted += game.particles.len();
                game.particles.clear();
            }
            assert!(game.bfgs.is_empty());
            assert_eq!(emitted, 274, "trail density must be stable at {fps} FPS");
        }
    }
}
