//! Bank shots gain power at the walls; returning blades follow the moving ship.

use super::*;

pub struct Ricochet {
    pub pos: V2,
    pub vel: V2,
    pub radius: f32,
    pub bounces: u8,
    pub charged: bool,
    damage: f32,
    range: f32,
    hit_ids: Vec<u32>,
}

pub struct Blade {
    pub pos: V2,
    pub vel: V2,
    pub radius: f32,
    pub returning: bool,
    pub charged: bool,
    pub angle: f32,
    damage: f32,
    range: f32,
    life: f32,
    hit_ids: Vec<u32>,
}

impl Game {
    pub(super) fn fire_ricochet(&mut self, charge: Option<f32>, multiplier: f32, boosted: bool) {
        if self.ricochets.len() >= 64 {
            return;
        }
        let c = charge.unwrap_or(0.0);
        let radius = if charge.is_some() {
            12.0 + 20.0 * c
        } else {
            7.0
        };
        let dir = V2::from_angle(self.player.aim);
        self.ricochets.push(Ricochet {
            pos: self.player.pos + dir * 26.0,
            vel: dir * (if boosted { 1450.0 } else { 1100.0 }),
            radius,
            bounces: if charge.is_some() {
                4 + (8.0 * c).ceil() as u8
            } else {
                3
            },
            charged: charge.is_some(),
            damage: multiplier
                * if charge.is_some() {
                    12.0 + 24.0 * c
                } else {
                    5.0
                },
            range: if charge.is_some() {
                6000.0 + 6000.0 * c
            } else {
                4000.0
            },
            hit_ids: Vec::new(),
        });
        self.player.vel += -dir * 8.0;
        self.sfx.push(Sfx::Ricochet);
    }

    pub(super) fn fire_blade(&mut self, charge: Option<f32>, multiplier: f32, boosted: bool) {
        if self.blades.len() >= 8 {
            return;
        }
        let c = charge.unwrap_or(0.0);
        let radius = if charge.is_some() {
            30.0 + 30.0 * c
        } else {
            22.0
        };
        let dir = V2::from_angle(self.player.aim);
        let speed = if charge.is_some() {
            560.0 - 160.0 * c
        } else {
            850.0
        };
        self.blades.push(Blade {
            pos: self.player.pos + dir * 30.0,
            vel: dir * speed * if boosted { 1.25 } else { 1.0 },
            radius,
            returning: false,
            charged: charge.is_some(),
            angle: self.player.aim,
            damage: multiplier
                * if charge.is_some() {
                    18.0 + 40.0 * c
                } else {
                    10.0
                },
            range: if charge.is_some() {
                650.0 + 400.0 * c
            } else {
                480.0
            },
            life: 8.0,
            hit_ids: Vec::new(),
        });
        self.player.vel += -dir * 10.0;
        self.sfx.push(Sfx::Blade);
    }

    pub(super) fn update_trickshots(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let mut hit = false;
        let mut bounced = false;
        let mut ricochets = std::mem::take(&mut self.ricochets);
        ricochets.retain_mut(|shot| {
            let lo = v2(WALL + shot.radius, WALL + shot.radius);
            let hi = self.arena_size - lo;
            if hi.x <= lo.x || hi.y <= lo.y {
                return false;
            }
            shot.pos = shot.pos.clamp(lo, hi);
            let speed = shot.vel.len();
            if speed <= 0.0 {
                return false;
            }
            let mut time = 0.0;
            while time < dt {
                let dir = shot.vel.norm();
                let wall = ray_exit(shot.pos, dir, lo, hi);
                let distance = (speed * (dt - time)).min(shot.range).min(wall);
                let from = shot.pos;
                shot.pos = (from + dir * distance).clamp(lo, hi);
                let end = (time + distance / speed).min(dt);
                hit |= self.trickshot_hits(
                    [from, shot.pos],
                    shot.radius,
                    shot.damage,
                    &mut shot.hit_ids,
                    time / dt..end / dt,
                    dt,
                );
                shot.range -= distance;
                time = end;
                if shot.range <= 0.001 {
                    return false;
                }
                if distance < wall {
                    break;
                }
                bounced = true;
                for _ in 0..6 {
                    let vel = -dir * 120.0 + self.rng.dir() * 100.0;
                    self.emit(shot.pos, vel, 0.22, Hue::Yellow, 1);
                }
                if shot.bounces == 0 {
                    return false;
                }
                shot.bounces -= 1;
                shot.damage *= 1.3;
                shot.hit_ids.clear();
                if (shot.pos.x <= lo.x + 0.001 && shot.vel.x < 0.0)
                    || (shot.pos.x >= hi.x - 0.001 && shot.vel.x > 0.0)
                {
                    shot.vel.x = -shot.vel.x;
                }
                if (shot.pos.y <= lo.y + 0.001 && shot.vel.y < 0.0)
                    || (shot.pos.y >= hi.y - 0.001 && shot.vel.y > 0.0)
                {
                    shot.vel.y = -shot.vel.y;
                }
            }
            true
        });
        self.ricochets = ricochets;

        let mut blades = std::mem::take(&mut self.blades);
        blades.retain_mut(|blade| {
            let lo = v2(WALL + blade.radius, WALL + blade.radius);
            let hi = self.arena_size - lo;
            if hi.x <= lo.x || hi.y <= lo.y {
                return false;
            }
            blade.pos = blade.pos.clamp(lo, hi);
            let active = dt.min(blade.life);
            blade.life -= dt;
            blade.angle += active * if blade.charged { 10.0 } else { 18.0 };
            let speed = blade.vel.len();
            if speed <= 0.0 {
                return false;
            }
            let mut time = 0.0;
            while time < active {
                let from = blade.pos;
                let (end, finish_leg) = if blade.returning {
                    let to_player = self.player.pos - from;
                    blade.vel = to_player.norm() * speed;
                    let flight = (active - time).min(to_player.len() / speed);
                    let to = (from + blade.vel * flight).clamp(lo, hi);
                    let catch =
                        segment_hit(self.player.pos, from, to, blade.radius + PLAYER_RADIUS);
                    let fraction = catch.unwrap_or(1.0);
                    blade.pos = from + (to - from) * fraction;
                    (time + flight * fraction, catch.is_some())
                } else {
                    let limit = blade.range.min(ray_exit(from, blade.vel.norm(), lo, hi));
                    let distance = (speed * (active - time)).min(limit);
                    blade.range -= distance;
                    blade.pos = (from + blade.vel.norm() * distance).clamp(lo, hi);
                    ((time + distance / speed).min(active), distance >= limit)
                };
                hit |= self.trickshot_hits(
                    [from, blade.pos],
                    blade.radius,
                    blade.damage,
                    &mut blade.hit_ids,
                    time / dt..end / dt,
                    dt,
                );
                time = end;
                if !finish_leg {
                    break;
                }
                if blade.returning {
                    return false;
                }
                blade.returning = true;
                blade.hit_ids.clear();
            }
            blade.life > 0.0
        });
        self.blades = blades;
        if hit {
            self.sfx.push(Sfx::Hit);
        }
        if bounced {
            self.sfx.push(Sfx::Ricochet);
        }
        let killed: Vec<_> = self.enemies.extract_if(.., |e| e.hp <= 0.0).collect();
        for e in killed {
            self.kill(e, true);
        }
    }

    fn trickshot_hits(
        &mut self,
        [from, to]: [V2; 2],
        radius: f32,
        damage: f32,
        hit_ids: &mut Vec<u32>,
        flight: std::ops::Range<f32>,
        dt: f32,
    ) -> bool {
        let mut impacts = Vec::new();
        let dir = (to - from).norm();
        for e in &mut self.enemies {
            if e.hp > 0.0
                && !hit_ids.contains(&e.id)
                && let Some(t) = segment_hit(e.pos, from, to, e.kind.radius() + radius)
            {
                hit_ids.push(e.id);
                e.hp -= damage;
                e.stagger(damage);
                e.flash = 0.1;
                e.impulse += dir * (damage * 30.0).min(650.0);
                if e.impulse.len() > 650.0 {
                    e.impulse = e.impulse.norm() * 650.0;
                }
                impacts.push(from + (to - from) * t);
            }
        }
        if let Some(b) = &mut self.boss
            && b.entering <= 0.0
            && b.hp > 0.0
            && !hit_ids.contains(&0)
            && let Some(t) = segment_hit(b.pos, from, to, BOSS_RADIUS + radius)
        {
            hit_ids.push(0);
            b.hp -= damage;
            b.stagger(damage);
            b.flash = 0.1;
            b.impulse += dir * (damage * 10.0).min(350.0);
            if b.impulse.len() > 350.0 {
                b.impulse = b.impulse.norm() * 350.0;
            }
            impacts.push(from + (to - from) * t);
        }
        let size = self.arena_size;
        self.orbs.retain(|o| {
            let a = o.prev + (o.pos - o.prev) * flight.start;
            let b = o.prev + (o.pos - o.prev) * flight.end;
            let collision = segment_hit(V2::default(), from - a, to - b, radius + o.radius())
                .filter(|&t| {
                    let time = flight.start + (flight.end - flight.start) * t;
                    let at = o.prev + (o.pos - o.prev) * time;
                    o.life + dt >= time * dt
                        && at.x > WALL
                        && at.y > WALL
                        && at.x < size.x - WALL
                        && at.y < size.y - WALL
                });
            if let Some(t) = collision {
                impacts.push(from + (to - from) * t);
            }
            collision.is_none()
        });
        let hit = !impacts.is_empty();
        for pos in impacts {
            for _ in 0..8 {
                let vel = dir * 160.0 + self.rng.dir() * 180.0;
                self.emit(pos, vel, 0.22, Hue::White, 1);
            }
        }
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        let mut game = Game::new(7);
        game.best_path = None;
        game.arena_size = v2(1000.0, 800.0);
        game.player.pos = v2(200.0, 400.0);
        game.player.aim = 0.0;
        game
    }

    #[test]
    fn ricochet_banks_gain_power_and_sweep_multiple_walls_in_one_frame() {
        let mut game = game();
        game.fire_ricochet(None, 1.0, false);
        let shot = &mut game.ricochets[0];
        shot.pos = v2(900.0, 400.0);
        shot.vel = v2(2000.0, 0.0);
        game.add_enemy(Kind::Tank, v2(600.0, 400.0), V2::default(), false);
        game.enemies[0].hp = 1000.0;
        game.update_trickshots(1.0);
        let shot = &game.ricochets[0];
        assert_eq!(shot.bounces, 0);
        assert!(shot.vel.x < 0.0);
        assert!((shot.damage - 5.0 * 1.3_f32.powi(3)).abs() < 0.001);
        assert!(game.enemies[0].hp < 985.1 && game.enemies[0].hp > 985.0);
        assert!(shot.pos.x > WALL + shot.radius && shot.pos.x < 1000.0 - WALL - shot.radius);
        assert!(game.enemies[0].impulse.len() > 0.0);
    }

    #[test]
    fn ricochet_corners_and_range_expiry_never_hit_outside_arena() {
        let mut game = game();
        game.arena_size = v2(500.0, 500.0);
        game.fire_ricochet(Some(1.0), 1.0, false);
        game.ricochets[0].pos = v2(250.0, 250.0);
        game.ricochets[0].vel = v2(1000.0, 1000.0);
        game.ricochets[0].range = 300.0;
        game.add_enemy(Kind::Tank, v2(560.0, 560.0), V2::default(), false);
        let hp = game.enemies[0].hp;
        game.update_trickshots(10.0);
        assert!(game.ricochets.is_empty());
        assert_eq!(game.enemies[0].hp, hp);
        assert!(!game.particles.is_empty());
    }

    #[test]
    fn ricochet_hits_a_moving_orb_only_at_the_correct_time() {
        for intersects in [false, true] {
            let mut game = game();
            game.fire_ricochet(None, 1.0, false);
            game.ricochets[0].pos = v2(300.0, 400.0);
            game.ricochets[0].vel = v2(400.0, 0.0);
            game.orbs.push(Orb {
                prev: v2(500.0, 200.0),
                pos: v2(500.0, if intersects { 600.0 } else { 1200.0 }),
                vel: V2::default(),
                life: 3.0,
                heavy: false,
            });
            game.update_trickshots(1.0);
            assert_eq!(game.orbs.is_empty(), intersects);
            assert_eq!(game.ricochets.len(), 1);
        }
    }

    #[test]
    fn blade_hits_once_each_way_and_follows_the_current_ship() {
        let mut game = game();
        game.fire_blade(None, 1.0, false);
        game.add_enemy(Kind::Tank, v2(450.0, 400.0), V2::default(), false);
        game.enemies[0].hp = 1000.0;
        game.update_trickshots(0.35);
        assert_eq!(game.enemies[0].hp, 990.0);
        game.update_trickshots(0.05);
        assert_eq!(game.enemies[0].hp, 990.0);
        game.update_trickshots(0.5);
        assert_eq!(game.enemies[0].hp, 980.0);
        assert!(game.blades[0].returning);
        game.player.pos = v2(250.0, 700.0);
        game.update_trickshots(0.1);
        assert!(game.blades[0].vel.y > 0.0);
        game.update_trickshots(2.0);
        assert!(game.blades.is_empty());
    }

    #[test]
    fn charged_blade_is_caught_only_at_the_actual_ship_near_walls_and_corners() {
        let mut game = game();
        game.player.pos = v2(WALL + PLAYER_RADIUS, 400.0);
        game.fire_blade(Some(1.0), 1.0, false);
        game.blades[0].returning = true;
        game.blades[0].pos = v2(200.0, 400.0);
        game.update_trickshots(0.16);
        assert_eq!(game.blades.len(), 1);
        assert!((game.blades[0].pos - game.player.pos).len() > 60.0 + PLAYER_RADIUS);
        game.update_trickshots(0.1);
        assert!(game.blades.is_empty());

        game.player.pos = v2(WALL + PLAYER_RADIUS, WALL + PLAYER_RADIUS);
        game.fire_blade(Some(1.0), 1.0, false);
        game.blades[0].returning = true;
        game.blades[0].pos = v2(WALL + 60.0, 300.0);
        for _ in 0..120 {
            game.update_trickshots(1.0 / 60.0);
        }
        assert!(
            game.blades.is_empty(),
            "a returning blade can slide along the wall into the ship"
        );
    }

    #[test]
    fn charged_trickshots_are_larger_stronger_and_finite() {
        let mut game = game();
        game.fire_ricochet(None, 1.0, false);
        game.fire_ricochet(Some(1.0), 1.1, true);
        let (normal, charged) = (&game.ricochets[0], &game.ricochets[1]);
        assert!(charged.radius > normal.radius && charged.damage > normal.damage);
        assert!(charged.bounces > normal.bounces && charged.vel.len() > normal.vel.len());
        game.fire_blade(None, 1.0, false);
        game.fire_blade(Some(1.0), 0.75, false);
        let (normal, charged) = (&game.blades[0], &game.blades[1]);
        assert!(charged.radius > normal.radius && charged.damage > normal.damage);
        assert!(charged.range > normal.range && charged.vel.len() < normal.vel.len());
        for _ in 0..100 {
            game.fire_ricochet(None, 1.0, false);
            game.fire_blade(None, 1.0, false);
        }
        assert_eq!(game.ricochets.len(), 64);
        assert_eq!(game.blades.len(), 8);
        game.update_trickshots(30.0);
        assert!(game.ricochets.is_empty() && game.blades.is_empty());
        assert!(game.particles.len() <= MAX_PARTICLES);
    }

    #[test]
    fn trickshots_respect_boss_entrance_and_do_not_repeat_overlap_damage() {
        let mut game = game();
        game.spawn_boss();
        game.boss.as_mut().unwrap().pos = v2(450.0, 400.0);
        game.fire_blade(Some(1.0), 1.0, false);
        game.update_trickshots(0.5);
        let boss = game.boss.as_mut().unwrap();
        assert_eq!(boss.hp, boss.max_hp);
        boss.entering = 0.0;
        game.update_trickshots(0.01);
        let hp = game.boss.as_ref().unwrap().hp;
        assert!(hp < game.boss.as_ref().unwrap().max_hp);
        game.update_trickshots(0.01);
        assert_eq!(game.boss.as_ref().unwrap().hp, hp);
    }
}
