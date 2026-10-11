//! Sticky mine chains and burning fuel share the same enemy-only area damage.

use super::*;

const MINE_LIMIT: usize = 48;
const MINE_RADIUS: f32 = 7.0;
const FLAME_REACH: f32 = 330.0;
const FLAME_ANGLE: f32 = 0.36;

pub struct StickyMine {
    pub pos: V2,
    pub vel: V2,
    pub life: f32,
    pub fuse: Option<f32>,
    host: Option<(u32, V2)>,
    damage: f32,
    radius: f32,
}

pub struct FuelGlob {
    pub pos: V2,
    pub vel: V2,
    pub radius: f32,
    pub life: f32,
    damage: f32,
    blast_radius: f32,
    duration: f32,
}

pub struct FirePatch {
    pub pos: V2,
    pub radius: f32,
    pub life: f32,
    pub duration: f32,
    damage: f32,
    delay: f32,
}

pub struct Burn {
    id: u32,
    life: f32,
    damage: f32,
    sparkle_cd: f32,
}

fn flame_hits(pos: V2, radius: f32, origin: V2, aim: f32) -> bool {
    let dir = V2::from_angle(aim);
    let offset = pos - origin;
    if offset.dot(dir) >= offset.len() * FLAME_ANGLE.cos() {
        return offset.len() <= FLAME_REACH + radius;
    }
    [-FLAME_ANGLE, FLAME_ANGLE].into_iter().any(|angle| {
        segments_distance(
            pos,
            pos,
            origin,
            origin + V2::from_angle(aim + angle) * FLAME_REACH,
        ) <= radius
    })
}

fn orb_crosses(o: &Orb, from: V2, to: V2, radius: f32, times: (f32, f32), dt: f32) -> bool {
    let (begin, end) = times;
    let a = o.prev + (o.pos - o.prev) * begin;
    let b = o.prev + (o.pos - o.prev) * end;
    segment_hit(V2::default(), from - a, to - b, radius + o.radius())
        .is_some_and(|t| o.life + dt >= (begin + (end - begin) * t) * dt)
}

impl Game {
    pub(super) fn fire_mines(&mut self, charge: Option<f32>, multiplier: f32, boosted: bool) {
        let charge = charge.map(|charge| charge.clamp(0.0, 1.0));
        let mut planted = 0;
        if let Some(charge) = charge {
            for mine in &mut self.mines {
                if mine.fuse.is_none() {
                    let fuse = planted as f32 * if boosted { 0.09 } else { 0.13 };
                    mine.fuse = Some(fuse);
                    mine.life = mine.life.max(fuse + 0.1);
                    mine.damage = (35.0 + 70.0 * charge) * multiplier;
                    mine.radius = 170.0 + 130.0 * charge;
                    planted += 1;
                }
            }
        }
        if planted == 0 {
            let count = charge.map_or(1, |charge| 3 + (charge * 7.0).ceil() as usize);
            for i in 0..count {
                if self.mines.len() >= MINE_LIMIT {
                    self.mines.remove(0);
                }
                let spread = (i as f32 - (count - 1) as f32 * 0.5) * 0.09;
                let dir = V2::from_angle(self.player.aim + spread);
                let amount = charge.unwrap_or(0.0);
                self.mines.push(StickyMine {
                    pos: self.player.pos + dir * 24.0,
                    vel: dir * if boosted { 780.0 } else { 650.0 },
                    life: 20.0,
                    fuse: charge.map(|_| 0.5 + i as f32 * 0.13),
                    host: None,
                    damage: (35.0 + 70.0 * amount) * multiplier,
                    radius: 170.0 + 130.0 * amount,
                });
            }
            self.player.vel += -V2::from_angle(self.player.aim) * 8.0;
        }
        self.sfx.push(if charge.is_some() {
            Sfx::Shock
        } else {
            Sfx::Mine
        });
    }

    pub(super) fn fire_flame(&mut self, charge: Option<f32>, multiplier: f32, boosted: bool) {
        let aim = self.player.aim;
        let dir = V2::from_angle(aim);
        let lo = v2(WALL, WALL);
        let hi = self.arena_size - lo;
        let origin = (self.player.pos + dir * 22.0).clamp(lo, hi);
        let in_arena = |pos: V2| pos.x > lo.x && pos.y > lo.y && pos.x < hi.x && pos.y < hi.y;
        if let Some(charge) = charge {
            let charge = charge.clamp(0.0, 1.0);
            if self.fuel_globs.len() >= 24 {
                self.fuel_globs.remove(0);
            }
            self.fuel_globs.push(FuelGlob {
                pos: origin,
                vel: dir * if boosted { 1100.0 } else { 850.0 },
                radius: 15.0 + 25.0 * charge,
                life: 1.4,
                damage: (20.0 + 30.0 * charge) * multiplier,
                blast_radius: 160.0 + 180.0 * charge,
                duration: 3.0 + 3.0 * charge,
            });
            self.player.vel += -dir * 12.0;
            self.sfx.push(Sfx::Lunge);
            return;
        }
        let mut ignited = Vec::new();
        for e in &mut self.enemies {
            if e.hp > 0.0 && in_arena(e.pos) && flame_hits(e.pos, e.kind.radius(), origin, aim) {
                e.hp -= 2.2 * multiplier;
                e.flash = 0.06;
                e.impulse += dir * 12.0;
                ignited.push(e.id);
            }
        }
        if let Some(b) = &mut self.boss
            && b.hp > 0.0
            && b.entering <= 0.0
            && in_arena(b.pos)
            && flame_hits(b.pos, BOSS_RADIUS, origin, aim)
        {
            b.hp -= 2.2 * multiplier;
            b.flash = 0.06;
            b.impulse += dir * 6.0;
            ignited.push(0);
        }
        self.orbs.retain(|o| {
            o.life <= 0.0 || !in_arena(o.pos) || !flame_hits(o.pos, o.radius(), origin, aim)
        });
        for id in ignited {
            self.ignite(id, 2.2, 14.0 * multiplier);
        }
        for _ in 0..14 {
            let dir = V2::from_angle(aim + self.rng.range(-FLAME_ANGLE, FLAME_ANGLE));
            let distance = self
                .rng
                .range(0.0, FLAME_REACH)
                .min(ray_exit(origin, dir, lo, hi));
            let hue = if distance < FLAME_REACH * 0.3 {
                Hue::Yellow
            } else {
                Hue::Orange
            };
            self.emit(origin + dir * distance, dir * 95.0, 0.18, hue, 2);
        }
        self.player.vel += -dir * 1.8;
        self.sfx.push(Sfx::Flame);
        self.incendiary_kills();
    }

    fn ignite(&mut self, id: u32, life: f32, damage: f32) {
        if let Some(burn) = self.burns.iter_mut().find(|burn| burn.id == id) {
            burn.life = burn.life.max(life);
            burn.damage = burn.damage.max(damage);
        } else {
            self.burns.push(Burn {
                id,
                life,
                damage,
                sparkle_cd: 0.0,
            });
        }
    }

    pub(super) fn update_incendiary(&mut self, dt: f32) {
        let mut blasts = Vec::new();
        let lo = v2(WALL + MINE_RADIUS, WALL + MINE_RADIUS);
        let hi = self.arena_size - lo;
        self.mines.retain_mut(|mine| {
            let active = dt
                .min(mine.life)
                .min(mine.fuse.unwrap_or(f32::INFINITY))
                .max(0.0);
            let from = mine.pos;
            let speed = mine.vel.len();
            if let Some((id, offset)) = mine.host {
                let host = if id == 0 {
                    self.boss.as_ref().filter(|b| b.hp > 0.0).map(|b| b.pos)
                } else {
                    self.enemies
                        .iter()
                        .find(|e| e.id == id && e.hp > 0.0)
                        .map(|e| e.pos)
                };
                if let Some(pos) = host {
                    mine.pos = pos + offset;
                } else {
                    mine.host = None;
                }
            } else if mine.vel.len() > 0.0 {
                let from = mine.pos.clamp(lo, hi);
                let distance = (mine.vel * active).len();
                let wall = ray_exit(from, mine.vel.norm(), lo, hi);
                let to = from + mine.vel.norm() * distance.min(wall);
                let mut contact = self
                    .enemies
                    .iter()
                    .filter(|e| e.hp > 0.0)
                    .filter_map(|e| {
                        segment_hit(e.pos, from, to, e.kind.radius() + MINE_RADIUS)
                            .map(|time| (time, e.id, e.pos))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some(b) = &self.boss
                    && b.entering <= 0.0
                    && b.hp > 0.0
                    && let Some(time) = segment_hit(b.pos, from, to, BOSS_RADIUS + MINE_RADIUS)
                    && contact.is_none_or(|contact| time < contact.0)
                {
                    contact = Some((time, 0, b.pos));
                }
                if let Some((time, id, pos)) = contact {
                    mine.pos = from + (to - from) * time;
                    mine.host = Some((id, mine.pos - pos));
                    mine.vel = V2::default();
                } else {
                    mine.pos = to;
                    if distance >= wall {
                        mine.vel = V2::default();
                    }
                }
            }
            let end = if dt > 0.0 { active / dt } else { 0.0 };
            let flight = if speed > 0.0 && dt > 0.0 {
                ((mine.pos - from).len() / (speed * dt)).min(end)
            } else {
                end
            };
            self.orbs.retain(|o| {
                !orb_crosses(o, from, mine.pos, MINE_RADIUS, (0.0, flight), dt)
                    && !orb_crosses(o, mine.pos, mine.pos, MINE_RADIUS, (flight, end), dt)
            });
            mine.life -= dt;
            if let Some(fuse) = &mut mine.fuse {
                *fuse -= dt;
            }
            if mine.life <= 0.0 || mine.fuse.is_some_and(|fuse| fuse <= 0.0) {
                blasts.push((mine.pos, mine.radius, mine.damage));
                false
            } else {
                true
            }
        });
        for (pos, radius, damage) in blasts {
            self.mine_blast(pos, radius, damage);
        }

        let mut spills = Vec::new();
        self.fuel_globs.retain_mut(|glob| {
            let active = dt.min(glob.life).max(0.0);
            let lo = v2(WALL + glob.radius, WALL + glob.radius);
            let hi = self.arena_size - lo;
            let from = glob.pos.clamp(lo, hi);
            let distance = (glob.vel * active).len();
            let wall = ray_exit(from, glob.vel.norm(), lo, hi);
            let to = from + glob.vel.norm() * distance.min(wall);
            let mut contact = self
                .enemies
                .iter()
                .filter(|e| e.hp > 0.0)
                .filter_map(|e| segment_hit(e.pos, from, to, e.kind.radius() + glob.radius))
                .min_by(f32::total_cmp);
            if let Some(b) = &self.boss
                && b.hp > 0.0
                && b.entering <= 0.0
                && let Some(time) = segment_hit(b.pos, from, to, BOSS_RADIUS + glob.radius)
            {
                contact = Some(contact.map_or(time, |existing| existing.min(time)));
            }
            glob.pos = from + (to - from) * contact.unwrap_or(1.0);
            let flight = if dt > 0.0 {
                (glob.pos - from).len() / (glob.vel.len().max(1.0) * dt)
            } else {
                0.0
            };
            self.orbs
                .retain(|o| !orb_crosses(o, from, glob.pos, glob.radius, (0.0, flight), dt));
            glob.life -= dt;
            if contact.is_some() || distance >= wall || glob.life <= 0.0 {
                let delay = (glob.pos - from).len() / glob.vel.len().max(1.0);
                spills.push((
                    glob.pos,
                    glob.blast_radius,
                    glob.damage,
                    glob.duration,
                    delay,
                ));
                false
            } else {
                true
            }
        });
        for (pos, radius, damage, duration, delay) in spills {
            if self.fire_patches.len() >= 24 {
                self.fire_patches.remove(0);
            }
            self.fire_patches.push(FirePatch {
                pos,
                radius,
                life: duration,
                duration,
                damage,
                delay,
            });
            self.mine_blast(pos, radius, damage);
        }
        let mut ignited = Vec::new();
        self.fire_patches.retain_mut(|patch| {
            let begin = patch.delay;
            let active = (dt - patch.delay).max(0.0).min(patch.life);
            patch.delay = (patch.delay - dt).max(0.0);
            patch.life -= active;
            if active <= 0.0 {
                return patch.life > 0.0;
            }
            for e in &mut self.enemies {
                if e.hp > 0.0 && (e.pos - patch.pos).len() < patch.radius + e.kind.radius() {
                    e.hp -= patch.damage * active;
                    e.flash = 0.05;
                    ignited.push((begin, e.id, active + 2.0, patch.damage * 0.6));
                }
            }
            if let Some(b) = &mut self.boss
                && b.hp > 0.0
                && b.entering <= 0.0
                && (b.pos - patch.pos).len() < patch.radius + BOSS_RADIUS
            {
                b.hp -= patch.damage * active;
                b.flash = 0.05;
                ignited.push((begin, 0, active + 2.0, patch.damage * 0.6));
            }
            self.orbs.retain(|o| {
                o.life <= 0.0 || (o.pos - patch.pos).len() >= patch.radius + o.radius()
            });
            patch.life > 0.0
        });
        ignited.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut elapsed = 0.0;
        for (begin, id, life, damage) in ignited {
            self.update_burns(begin - elapsed);
            elapsed = begin;
            self.ignite(id, life, damage);
        }
        self.update_burns(dt - elapsed);
        self.incendiary_kills();
    }

    fn update_burns(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let mut sparks = Vec::new();
        self.burns.retain_mut(|burn| {
            let active = dt.min(burn.life);
            let damage = burn.damage * active;
            let pos = if burn.id == 0 {
                self.boss
                    .as_mut()
                    .filter(|b| b.hp > 0.0 && b.entering <= 0.0)
                    .map(|b| {
                        b.hp -= damage;
                        b.pos
                    })
            } else {
                self.enemies
                    .iter_mut()
                    .find(|e| e.id == burn.id && e.hp > 0.0)
                    .map(|e| {
                        e.hp -= damage;
                        e.pos
                    })
            };
            let Some(pos) = pos else { return false };
            burn.sparkle_cd -= active;
            if burn.sparkle_cd <= 0.0 {
                sparks.push(pos);
                burn.sparkle_cd = 0.08;
            }
            burn.life -= active;
            burn.life > 0.0
        });
        for pos in sparks {
            let offset = self.rng.dir() * 14.0;
            self.emit(pos + offset, offset * 3.0, 0.3, Hue::Orange, 1);
            self.emit(pos - offset, -offset * 2.0, 0.2, Hue::Yellow, 0);
        }
    }

    fn mine_blast(&mut self, pos: V2, radius: f32, damage: f32) {
        for e in &mut self.enemies {
            let distance = ((e.pos - pos).len() - e.kind.radius()).max(0.0);
            if e.hp > 0.0 && distance < radius {
                let strength = 1.0 - distance / radius;
                e.hp -= damage * (0.4 + 0.6 * strength);
                e.stagger(damage);
                e.stun = e.stun.max(1.5);
                e.flash = 0.12;
                let outward = if e.pos == pos {
                    v2(1.0, 0.0)
                } else {
                    (e.pos - pos).norm()
                };
                e.impulse += outward * (350.0 + strength * 600.0);
                if e.impulse.len() > 950.0 {
                    e.impulse = e.impulse.norm() * 950.0;
                }
            }
        }
        if let Some(b) = &mut self.boss {
            let distance = ((b.pos - pos).len() - BOSS_RADIUS).max(0.0);
            if b.hp > 0.0 && b.entering <= 0.0 && distance < radius {
                let strength = 1.0 - distance / radius;
                b.hp -= damage * (0.4 + 0.6 * strength);
                b.stagger(damage);
                b.flash = 0.12;
                b.impulse += (b.pos - pos).norm() * (150.0 + 300.0 * strength);
                if b.impulse.len() > 450.0 {
                    b.impulse = b.impulse.norm() * 450.0;
                }
            }
        }
        self.orbs
            .retain(|o| o.life <= 0.0 || (o.pos - pos).len() >= radius + o.radius());
        for i in 0..36 {
            let dir = V2::from_angle(TAU * i as f32 / 36.0);
            self.emit(pos + dir * radius, dir * 75.0, 0.25, Hue::Orange, 1);
            self.emit(pos, dir * 450.0, 0.45, Hue::Yellow, 2);
        }
        self.sfx.push(Sfx::Explode);
        self.shake = (self.shake + 5.0).min(16.0);
        self.push_motes(pos, radius, 400.0);
    }

    fn incendiary_kills(&mut self) {
        let killed: Vec<_> = self.enemies.extract_if(.., |e| e.hp <= 0.0).collect();
        for e in killed {
            self.kill(e, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Game {
        let mut game = Game::new(11);
        game.player.pos = v2(500.0, 500.0);
        game.player.aim = 0.0;
        game
    }

    #[test]
    fn incendiary_projectiles_intercept_only_simultaneous_live_orb_contacts() {
        let mut orb = Orb {
            prev: v2(700.0, 300.0),
            pos: v2(700.0, 700.0),
            vel: V2::default(),
            life: 1.0,
            heavy: false,
        };
        let (from, to) = (v2(500.0, 500.0), v2(900.0, 500.0));
        assert!(orb_crosses(&orb, from, to, MINE_RADIUS, (0.0, 1.0), 1.0));
        orb.prev.y += 50.0;
        orb.pos.y += 50.0;
        assert!(!orb_crosses(&orb, from, to, MINE_RADIUS, (0.0, 1.0), 1.0));
        orb.prev.y -= 50.0;
        orb.pos.y -= 50.0;
        orb.life = -0.7;
        assert!(!orb_crosses(&orb, from, to, MINE_RADIUS, (0.0, 1.0), 1.0));
    }

    #[test]
    fn mines_attach_to_the_first_swept_hull_and_follow_until_it_dies() {
        let mut game = setup();
        game.add_enemy(Kind::Tank, v2(720.0, 500.0), V2::default(), false);
        game.add_enemy(Kind::Tank, v2(820.0, 500.0), V2::default(), false);
        game.fire_mines(None, 1.0, false);
        game.update_incendiary(0.7);
        assert_eq!(game.mines[0].host.unwrap().0, game.enemies[0].id);
        let pos = game.mines[0].pos;
        game.enemies[0].pos += v2(50.0, 25.0);
        game.update_incendiary(0.1);
        assert_eq!(game.mines[0].pos, pos + v2(50.0, 25.0));
        game.enemies.remove(0);
        game.update_incendiary(0.1);
        assert!(game.mines[0].host.is_none());
        assert_eq!(game.mines[0].pos, pos + v2(50.0, 25.0));
    }

    #[test]
    fn mines_stick_to_walls_and_detonate_in_a_charged_sequence() {
        let mut game = setup();
        game.arena_size = v2(1000.0, 1000.0);
        for _ in 0..3 {
            game.fire_mines(None, 1.0, false);
        }
        game.update_incendiary(1.0);
        assert_eq!(game.mines[0].pos.x, 1000.0 - WALL - MINE_RADIUS);
        assert_eq!(game.mines[0].vel, V2::default());
        game.fire_mines(Some(1.0), 1.1, true);
        game.update_incendiary(0.001);
        assert_eq!(game.mines.len(), 2);
        game.update_incendiary(0.1);
        assert_eq!(game.mines.len(), 1);
        game.update_incendiary(0.1);
        assert!(game.mines.is_empty());
        assert_eq!(game.player.hp, Difficulty::Medium.hearts());
    }

    #[test]
    fn mine_charge_without_planting_launches_a_cluster_and_storage_is_bounded() {
        let mut game = setup();
        game.fire_mines(Some(1.0), 1.0, false);
        assert_eq!(game.mines.len(), 10);
        game.update_incendiary(3.0);
        assert!(game.mines.is_empty());
        for _ in 0..100 {
            game.fire_mines(None, 1.0, false);
        }
        assert_eq!(game.mines.len(), MINE_LIMIT);
        game.update_incendiary(21.0);
        assert!(game.mines.is_empty());
        assert!(game.particles.len() <= MAX_PARTICLES);
    }

    #[test]
    fn flame_cone_has_precise_edges_and_leaves_a_nonstacking_burn() {
        let origin = v2(522.0, 500.0);
        let side = V2::from_angle(FLAME_ANGLE);
        assert!(flame_hits(
            origin + side * 200.0 + side.perp() * 15.9,
            16.0,
            origin,
            0.0
        ));
        assert!(!flame_hits(
            origin + side * 200.0 + side.perp() * 16.1,
            16.0,
            origin,
            0.0
        ));
        assert!(!flame_hits(
            origin + v2(FLAME_REACH + 16.1, 0.0),
            16.0,
            origin,
            0.0
        ));
        let mut game = setup();
        game.add_enemy(Kind::Tank, v2(700.0, 500.0), V2::default(), false);
        game.enemies[0].hp = 1000.0;
        for _ in 0..5 {
            game.fire_flame(None, 1.0, false);
        }
        assert_eq!(game.burns.len(), 1);
        let hp = game.enemies[0].hp;
        game.enemies[0].pos = v2(1500.0, 900.0);
        game.update_incendiary(1.0);
        assert!((game.enemies[0].hp - (hp - 14.0)).abs() < 0.001);
        game.update_incendiary(10.0);
        assert!(game.burns.is_empty());
        assert!((game.enemies[0].hp - (hp - 14.0 * 2.2)).abs() < 0.001);
    }

    #[test]
    fn flame_cannot_damage_or_clear_shots_beyond_the_wall() {
        let mut game = setup();
        game.arena_size = v2(1000.0, 1000.0);
        game.player.pos = v2(950.0, 500.0);
        game.add_enemy(Kind::Tank, v2(1020.0, 500.0), V2::default(), false);
        let hp = game.enemies[0].hp;
        game.orbs.push(Orb {
            prev: v2(1020.0, 500.0),
            pos: v2(1020.0, 500.0),
            vel: V2::default(),
            life: 1.0,
            heavy: false,
        });
        game.fire_flame(None, 1.0, false);
        assert_eq!(game.enemies[0].hp, hp);
        assert_eq!(game.orbs.len(), 1);
        assert!(game.burns.is_empty());
    }

    #[test]
    fn fuel_burn_starts_at_impact_and_keeps_its_full_tail_at_every_frame_rate() {
        for fps in [1, 30, 60, 144] {
            let mut game = setup();
            game.add_enemy(Kind::Tank, v2(760.0, 500.0), V2::default(), false);
            game.enemies[0].hp = 1000.0;
            game.fire_flame(Some(1.0), 1.0, false);
            for _ in 0..fps {
                game.update_incendiary(1.0 / fps as f32);
            }
            let impact_time = (760.0 - 32.0 - 40.0 - 522.0) / 850.0;
            let blast_damage = 50.0 * (0.4 + 0.6 * (1.0 - 40.0 / 340.0));
            let first_second = blast_damage + 80.0 * (1.0 - impact_time);
            assert!(
                (game.enemies[0].hp - (1000.0 - first_second)).abs() < 0.08,
                "fuel DOT cannot precede impact at {fps} FPS"
            );
            for _ in 0..fps * 9 {
                game.update_incendiary(1.0 / fps as f32);
            }
            let total_damage = blast_damage + 50.0 * 6.0 + 30.0 * 8.0;
            assert!(
                (game.enemies[0].hp - (1000.0 - total_damage)).abs() < 0.08,
                "six seconds of fire must leave exactly two seconds of burn at {fps} FPS"
            );
            assert!(game.fire_patches.is_empty() && game.burns.is_empty());
        }
    }

    #[test]
    fn fuel_sweeps_into_a_ship_spreads_fire_and_leaves_a_temporary_patch() {
        let mut game = setup();
        for pos in [v2(760.0, 500.0), v2(840.0, 650.0), v2(1250.0, 650.0)] {
            game.add_enemy(Kind::Tank, pos, V2::default(), false);
            game.enemies.last_mut().unwrap().hp = 1000.0;
        }
        game.fire_flame(Some(1.0), 1.0, false);
        game.update_incendiary(0.5);
        assert!(game.fuel_globs.is_empty());
        assert_eq!(game.fire_patches.len(), 1);
        assert!(game.fire_patches[0].pos.x < 760.0);
        assert!(game.enemies[0].hp < 1000.0 && game.enemies[1].hp < 1000.0);
        assert_eq!(game.enemies[2].hp, 1000.0);
        assert_eq!(game.burns.len(), 2);
        game.update_incendiary(10.0);
        game.update_incendiary(10.0);
        assert!(game.fire_patches.is_empty() && game.burns.is_empty());
    }

    #[test]
    fn fire_and_mines_respect_boss_entrance_and_never_hurt_the_player() {
        let mut game = setup();
        game.spawn_boss();
        game.boss.as_mut().unwrap().pos = v2(700.0, 500.0);
        let hp = game.boss.as_ref().unwrap().hp;
        game.fire_flame(None, 1.0, false);
        game.mine_blast(game.player.pos, 400.0, 200.0);
        assert_eq!(game.boss.as_ref().unwrap().hp, hp);
        assert_eq!(game.player.hp, Difficulty::Medium.hearts());
        game.boss.as_mut().unwrap().entering = 0.0;
        game.fire_flame(None, 1.0, false);
        game.update_incendiary(1.0);
        assert!(game.boss.as_ref().unwrap().hp < hp);
    }
}
