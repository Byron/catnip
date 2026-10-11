//! Gravity gathers ammunition from the arena; lightning jumps between nearby hulls.

use super::*;

pub struct GravityField {
    pub pos: V2,
    pub dir: V2,
    pub radius: f32,
    pub charge: f32,
    pub life: f32,
    pub gathering: bool,
    pub captured: usize,
    pub(super) charging: bool,
    multiplier: f32,
    boosted: bool,
}

pub struct ThrownShip {
    id: u32,
    prev: V2,
    life: f32,
    damage: f32,
    hit_ids: Vec<u32>,
}

pub struct LightningArc {
    pub from: V2,
    pub to: V2,
    pub life: f32,
    pub power: f32,
}

pub struct LightningStorm {
    pub life: f32,
    pub duration: f32,
    pub charge: f32,
    multiplier: f32,
    boosted: bool,
    fire_cd: f32,
}

impl Game {
    fn gravity_field(&self, charge: f32, multiplier: f32, boosted: bool) -> GravityField {
        let dir = V2::from_angle(self.player.aim);
        let margin = v2(WALL + 36.0, WALL + 36.0);
        GravityField {
            pos: (self.player.pos + dir * 220.0).clamp(margin, self.arena_size - margin),
            dir,
            radius: 145.0 + 170.0 * charge,
            charge,
            life: 0.2,
            gathering: true,
            captured: 0,
            charging: false,
            multiplier,
            boosted,
        }
    }

    pub(super) fn fire_gravity(&mut self, charge: Option<f32>, multiplier: f32, boosted: bool) {
        if let Some(charge) = charge {
            let old = self.gravity_fields.iter().position(|f| f.charging);
            let mut field = if let Some(i) = old {
                self.gravity_fields.remove(i)
            } else {
                self.gravity_field(charge, multiplier, boosted)
            };
            let current = self.gravity_field(charge, multiplier, boosted);
            field.pos = current.pos;
            field.dir = current.dir;
            field.radius = current.radius;
            field.charge = charge;
            field.multiplier = multiplier;
            field.boosted = boosted;
            field.charging = false;
            self.launch_gravity(&mut field);
            self.gravity_fields.push(field);
        } else {
            self.gravity_fields
                .push(self.gravity_field(0.0, multiplier, boosted));
            self.sfx.push(Sfx::Gravity);
        }
    }

    fn launch_gravity(&mut self, field: &mut GravityField) {
        let speed = (1100.0 + 600.0 * field.charge) * if field.boosted { 1.25 } else { 1.0 };
        let damage = (12.0 + 48.0 * field.charge) * field.multiplier;
        let ids: Vec<_> = self
            .enemies
            .iter()
            .filter(|e| {
                e.hp > 0.0
                    && !self.thrown_ships.iter().any(|s| s.id == e.id)
                    && (e.pos - field.pos).len() < field.radius + e.kind.radius()
            })
            .map(|e| e.id)
            .collect();
        for e in &mut self.enemies {
            if ids.contains(&e.id) {
                e.stagger(damage);
                e.stun = e.stun.max(1.4);
                e.vel = V2::default();
                e.impulse = field.dir * speed;
                e.flash = 0.1;
                self.thrown_ships.retain(|s| s.id != e.id);
                self.thrown_ships.push(ThrownShip {
                    id: e.id,
                    prev: e.pos,
                    life: 1.4,
                    damage,
                    // The gathered pack flies together instead of colliding at launch.
                    hit_ids: ids.clone(),
                });
            }
        }
        if let Some(b) = &mut self.boss
            && b.hp > 0.0
            && b.entering <= 0.0
            && (b.pos - field.pos).len() < field.radius + BOSS_RADIUS
        {
            b.hp -= damage;
            b.stagger(damage);
            b.impulse = field.dir * speed.min(650.0);
            b.flash = 0.12;
        }
        let count = 1 + field.captured.min(24) + (field.charge * 8.0) as usize;
        for i in 0..count {
            let angle = if count == 1 {
                0.0
            } else {
                (i as f32 / (count - 1) as f32 - 0.5) * 0.24
            };
            let dir = V2::from_angle(field.dir.angle() + angle);
            self.bullets.push(Bullet {
                pos: field.pos,
                prev: field.pos,
                delay: 0.0,
                vel: dir * speed,
                traveled: 0.0,
                range: 1000.0 + 600.0 * field.charge,
                // Shared bullet collisions apply the current wave power themselves.
                damage: (4.0 + 12.0 * field.charge) * field.multiplier / self.power(),
                pierce: u8::from(field.charge > 0.5),
                hit_ids: ids.clone(),
                exits: Vec::new(),
                falloff: false,
                weapon: Weapon::Gravity,
                homing: false,
            });
        }
        field.life = 0.18;
        field.gathering = false;
        self.sfx.push(Sfx::Gravity);
        self.shake = self.shake.max(3.0 + field.charge * 7.0);
        for _ in 0..24 {
            let vel = field.dir * self.rng.range(100.0, 500.0) + self.rng.dir() * 90.0;
            self.emit(field.pos, vel, 0.35, Hue::Magenta, 1);
        }
    }

    pub(super) fn fire_lightning(&mut self, charge: Option<f32>, multiplier: f32, boosted: bool) {
        self.lightning_chain(charge, multiplier);
        if let Some(charge) = charge {
            let duration = 0.6 + 1.8 * charge;
            self.storm = Some(LightningStorm {
                life: duration,
                duration,
                charge,
                multiplier,
                boosted,
                fire_cd: if boosted { 0.1 } else { 0.15 },
            });
        }
    }

    fn lightning_chain(&mut self, charge: Option<f32>, multiplier: f32) {
        let dir = V2::from_angle(self.player.aim);
        let lo = v2(WALL, WALL);
        let hi = self.arena_size - lo;
        let muzzle = (self.player.pos + dir * 22.0).clamp(lo, hi);
        let reach = 650.0 + 150.0 * f32::from(self.player.upgrades.level(Upgrade::Range));
        let targets: Vec<_> = self
            .enemies
            .iter()
            .filter(|e| e.hp > 0.0 && e.pos == e.pos.clamp(lo, hi))
            .map(|e| (e.id, e.pos, e.kind.radius()))
            .chain(
                self.boss
                    .iter()
                    .filter(|b| b.entering <= 0.0 && b.hp > 0.0 && b.pos == b.pos.clamp(lo, hi))
                    .map(|b| (0, b.pos, BOSS_RADIUS)),
            )
            .collect();
        let first = targets
            .iter()
            .enumerate()
            .filter(|(_, (_, pos, radius))| {
                let delta = *pos - muzzle;
                delta.len() < reach + radius
                    && delta.dot(dir) > 0.0
                    && delta.dot(dir.perp()).abs() < 35.0 + radius + delta.len() * 0.22
            })
            .min_by(|(_, a), (_, b)| (a.1 - muzzle).len().total_cmp(&(b.1 - muzzle).len()))
            .map(|(i, _)| i);
        let Some(first) = first else {
            let wall = ray_exit(muzzle, dir, lo, hi);
            self.lightning_arc(muzzle, muzzle + dir * reach.min(wall), 0.5);
            self.sfx.push(Sfx::Lightning);
            return;
        };
        let max_hits = charge.map_or(
            5 + usize::from(self.player.upgrades.level(Upgrade::Spread)),
            |_| targets.len(),
        );
        let jump = 240.0 + 180.0 * charge.unwrap_or(0.0);
        let base_damage = charge.map_or(4.5, |c| 9.0 + 18.0 * c) * multiplier;
        let mut visited = vec![first];
        let mut links = vec![(muzzle, first)];
        while visited.len() < max_hits {
            let sources = if charge.is_some() {
                visited.as_slice()
            } else {
                &visited[visited.len() - 1..]
            };
            let next = sources
                .iter()
                .flat_map(|&from| {
                    let (_, origin, radius) = targets[from];
                    let visited = &visited;
                    targets.iter().enumerate().filter_map(move |(to, target)| {
                        let distance = (target.1 - origin).len();
                        (!visited.contains(&to) && distance < jump + target.2 + radius)
                            .then_some((distance, from, to))
                    })
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let Some((_, from, to)) = next else { break };
            visited.push(to);
            links.push((targets[from].1, to));
        }
        for (hop, (from, index)) in links.into_iter().enumerate() {
            let (id, pos, _) = targets[index];
            let damage = base_damage
                * if charge.is_some() {
                    0.92_f32.powi(hop as i32).max(0.25)
                } else {
                    0.76_f32.powi(hop as i32)
                };
            if id == 0 {
                let b = self.boss.as_mut().unwrap();
                b.hp -= damage;
                b.stagger(damage);
                if charge.is_some() {
                    b.stun = b.stun.max(0.4);
                }
                b.flash = 0.08;
            } else if let Some(e) = self.enemies.iter_mut().find(|e| e.id == id) {
                e.hp -= damage;
                e.stagger(damage.max(3.0));
                e.stun = e.stun.max(charge.map_or(0.25, |c| 0.8 + c));
                e.flash = 0.1;
                e.impulse += (pos - from).norm() * 35.0;
            }
            self.lightning_arc(from, pos, (damage / 4.5).sqrt().min(3.0));
            for _ in 0..6 {
                let vel = self.rng.dir() * self.rng.range(50.0, 180.0);
                self.emit(pos, vel, 0.2, Hue::Cyan, 1);
            }
        }
        self.sfx.push(Sfx::Lightning);
        self.finish_field_kills();
    }

    fn lightning_arc(&mut self, from: V2, to: V2, power: f32) {
        self.orbs
            .retain(|o| o.life <= 0.0 || segment_hit(o.pos, from, to, o.radius() + 5.0).is_none());
        if self.lightning_arcs.len() < 256 {
            self.lightning_arcs.push(LightningArc {
                from,
                to,
                life: 0.12,
                power,
            });
        }
    }

    pub(super) fn update_fields(&mut self, dt: f32) {
        self.lightning_arcs.retain_mut(|arc| {
            arc.life -= dt;
            arc.life > 0.0
        });
        if self.state == State::Playing {
            if let Some(mut storm) = self.storm.take() {
                let active = dt.min(storm.life);
                while storm.fire_cd < active {
                    self.lightning_chain(Some(storm.charge), storm.multiplier);
                    storm.fire_cd += if storm.boosted { 0.1 } else { 0.15 };
                }
                storm.fire_cd -= active;
                storm.life -= active;
                if storm.life > 0.0 {
                    self.storm = Some(storm);
                }
            }
        } else {
            self.storm = None;
        }
        let charging = self.state == State::Playing
            && self.player.weapon == Weapon::Gravity
            && self.player.charge > 0.0
            && !self.player.charge_spin;
        if charging && !self.gravity_fields.iter().any(|f| f.charging) {
            let mut field = self.gravity_field(self.player.charge, self.power(), false);
            field.charging = true;
            self.gravity_fields.push(field);
        }
        let mut fields = std::mem::take(&mut self.gravity_fields);
        fields.retain_mut(|field| {
            if field.charging {
                if !charging {
                    return false;
                }
                let current = self.gravity_field(self.player.charge, self.power(), false);
                field.pos = current.pos;
                field.dir = current.dir;
                field.radius = current.radius;
                field.charge = current.charge;
            } else {
                field.life -= dt;
            }
            if field.gathering {
                let active = if field.charging {
                    dt
                } else {
                    dt.min((field.life + dt).max(0.0))
                };
                for e in &mut self.enemies {
                    let offset = field.pos - e.pos;
                    if e.hp > 0.0
                        && !self.thrown_ships.iter().any(|s| s.id == e.id)
                        && offset.len() < field.radius + e.kind.radius()
                    {
                        e.stagger(3.0);
                        let desired = offset * 8.0;
                        e.impulse += (desired - e.impulse) * (1.0 - (-14.0 * active).exp());
                    }
                }
                if let Some(b) = &mut self.boss
                    && b.entering <= 0.0
                    && b.hp > 0.0
                    && (b.pos - field.pos).len() < field.radius + BOSS_RADIUS
                {
                    let desired = (field.pos - b.pos) * 2.0;
                    b.impulse += (desired - b.impulse) * (1.0 - (-8.0 * active).exp());
                    b.stagger(5.0);
                }
                self.orbs.retain_mut(|o| {
                    if o.life > 0.0 && (o.pos - field.pos).len() < field.radius + o.radius() {
                        field.captured = (field.captured + 1).min(64);
                        false
                    } else {
                        true
                    }
                });
                if field.life <= 0.0 && !field.charging {
                    self.launch_gravity(field);
                }
            }
            field.life > 0.0 || field.charging
        });
        self.gravity_fields = fields;
        self.update_thrown_ships(dt);
        self.finish_field_kills();
    }

    fn update_thrown_ships(&mut self, dt: f32) {
        let mut ships = std::mem::take(&mut self.thrown_ships);
        for ship in &mut ships {
            ship.life -= dt;
            let Some(e) = self.enemies.iter().find(|e| e.id == ship.id && e.hp > 0.0) else {
                ship.life = 0.0;
                continue;
            };
            let pos = e.pos;
            let radius = e.kind.radius();
            let dir = (pos - ship.prev).norm();
            if (pos - ship.prev).len() > 1.0 {
                let target = self
                    .enemies
                    .iter()
                    .filter(|e| e.hp > 0.0 && !ship.hit_ids.contains(&e.id))
                    .filter_map(|e| {
                        segment_hit(e.pos, ship.prev, pos, radius + e.kind.radius())
                            .map(|t| (t, e.id))
                    })
                    .chain(
                        self.boss
                            .iter()
                            .filter(|b| {
                                b.entering <= 0.0 && b.hp > 0.0 && !ship.hit_ids.contains(&0)
                            })
                            .filter_map(|b| {
                                segment_hit(b.pos, ship.prev, pos, radius + BOSS_RADIUS)
                                    .map(|t| (t, 0))
                            }),
                    )
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, id)) = target {
                    ship.hit_ids.push(id);
                    if id == 0 {
                        let b = self.boss.as_mut().unwrap();
                        b.hp -= ship.damage;
                        b.stagger(ship.damage);
                        b.impulse += dir * 300.0;
                        if b.impulse.len() > 650.0 {
                            b.impulse = b.impulse.norm() * 650.0;
                        }
                        b.flash = 0.12;
                    } else if let Some(e) = self.enemies.iter_mut().find(|e| e.id == id) {
                        e.hp -= ship.damage;
                        e.stagger(ship.damage);
                        e.impulse = dir * 850.0;
                        e.flash = 0.12;
                    }
                    if let Some(e) = self.enemies.iter_mut().find(|e| e.id == ship.id) {
                        e.hp -= ship.damage * 0.35;
                        e.flash = 0.12;
                    }
                    self.sfx.push(Sfx::Hit);
                    for _ in 0..12 {
                        let vel = self.rng.dir() * self.rng.range(80.0, 350.0);
                        self.emit(pos, vel, 0.3, Hue::Magenta, 1);
                    }
                }
            }
            ship.prev = pos;
        }
        ships.retain(|s| s.life > 0.0 && self.enemies.iter().any(|e| e.id == s.id && e.hp > 0.0));
        self.thrown_ships = ships;
    }

    fn finish_field_kills(&mut self) {
        let killed: Vec<_> = self.enemies.extract_if(.., |e| e.hp <= 0.0).collect();
        for e in killed {
            self.kill(e, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena() -> Game {
        let mut game = Game::new(11);
        game.state = State::Playing;
        game.player.pos = v2(400.0, 600.0);
        game.player.aim = 0.0;
        game
    }

    fn target(game: &mut Game, pos: V2) {
        game.add_enemy(Kind::Tank, pos, V2::default(), false);
        game.enemies.last_mut().unwrap().hp = 1000.0;
    }

    #[test]
    fn gravity_gathers_orbs_then_hurls_hulls_into_other_ships() {
        let mut game = arena();
        target(&mut game, v2(600.0, 600.0));
        target(&mut game, v2(1000.0, 600.0));
        game.orbs.push(Orb {
            pos: v2(650.0, 620.0),
            prev: v2(650.0, 620.0),
            vel: v2(-200.0, 0.0),
            life: 2.0,
            heavy: false,
        });
        let player_hp = game.player.hp;
        game.fire_gravity(None, 1.0, false);
        game.update_fields(0.1);
        assert!(game.orbs.is_empty());
        assert_eq!(game.gravity_fields[0].captured, 1);
        assert!(game.enemies[0].impulse.x > 0.0);
        assert!(game.bullets.is_empty());
        game.update_fields(0.1);
        assert_eq!(game.thrown_ships.len(), 1);
        assert_eq!(game.bullets.len(), 2);
        assert!(game.enemies[0].impulse.x >= 1100.0);
        assert!(
            game.bullets
                .iter()
                .all(|b| b.hit_ids.contains(&game.enemies[0].id))
        );
        // A fast thrown hull passes completely through the target this frame.
        game.enemies[0].pos.x = 1120.0;
        game.update_fields(0.2);
        assert!(game.enemies[1].hp < 1000.0);
        assert!(game.enemies[1].impulse.x > 800.0);
        let damaged_hp = game.enemies[1].hp;
        game.enemies[0].pos.x = 900.0;
        game.update_fields(0.1);
        assert_eq!(
            game.enemies[1].hp, damaged_hp,
            "a hull cannot hit the same target twice"
        );
        assert_eq!(game.player.hp, player_hp);
        assert_eq!(game.player.vel, V2::default());
    }

    #[test]
    fn charged_gravity_grows_and_cancels_on_weapon_switch_or_death() {
        for cancel_by_death in [false, true] {
            let mut game = arena();
            game.player.weapon = Weapon::Gravity;
            game.player.charge = 0.2;
            game.update_fields(0.02);
            let small = game.gravity_fields[0].radius;
            game.player.charge = 0.9;
            game.update_fields(0.02);
            assert!(game.gravity_fields[0].radius > small);
            assert!(game.gravity_fields[0].gathering);
            if cancel_by_death {
                game.state = State::Dying;
            } else {
                game.player.weapon = Weapon::Default;
            }
            game.update_fields(0.02);
            assert!(game.gravity_fields.is_empty());
            assert!(game.bullets.is_empty());
        }
        let mut game = arena();
        game.fire_gravity(Some(1.0), 1.1, true);
        assert_eq!(game.bullets.len(), 9);
        assert!(game.bullets[0].damage > 16.0);
        assert!(game.bullets[0].vel.len() > 2000.0);
        assert!(!game.gravity_fields[0].gathering);
        game.update_fields(0.2);
        assert!(game.gravity_fields.is_empty());
    }

    #[test]
    fn converted_gravity_bullets_apply_wave_power_once() {
        let mut game = arena();
        game.wave = 20;
        target(&mut game, v2(1000.0, 600.0));
        let power = game.power();
        game.fire_gravity(Some(1.0), power, false);
        game.bullets.swap(0, 4);
        game.bullets.truncate(1);
        game.update_bullets(0.5);
        assert!((game.enemies[0].hp - (1000.0 - 16.0 * power)).abs() < 0.001);
    }

    #[test]
    fn perfect_cooling_holds_one_gravity_field_and_release_uses_current_aim() {
        let mut game = arena();
        game.dev.perfect_cooling = true;
        game.player.weapon = Weapon::Gravity;
        let view = View {
            scale: 1.0,
            size: game.arena_size,
        };
        let held = Input {
            charge: true,
            ..Input::default()
        };
        for _ in 0..1200 {
            game.update_player(1.0 / 60.0, &held, &view);
            game.update_fields(1.0 / 60.0);
        }
        assert_eq!(game.player.charge, 1.0);
        assert_eq!(game.gravity_fields.len(), 1);
        assert!(game.bullets.is_empty());
        game.player.aim = TAU / 4.0;
        game.update_player(0.0, &Input::default(), &view);
        assert_eq!(game.bullets.len(), 9);
        assert!(game.bullets.iter().all(|b| b.vel.y > 1600.0));
        assert!(!game.gravity_fields[0].gathering);
        game.state = State::Dying;
        game.update_fields(2.0);
        game.update_bullets(2.0);
        assert!(
            game.gravity_fields.is_empty()
                && game.thrown_ships.is_empty()
                && game.bullets.is_empty()
        );
    }

    #[test]
    fn launched_fields_freeze_on_pause_and_lightning_stops_at_walls() {
        let mut game = arena();
        game.fire_gravity(None, 1.0, false);
        game.fire_lightning(Some(1.0), 1.0, false);
        let field_life = game.gravity_fields[0].life;
        let storm_life = game.storm.as_ref().unwrap().life;
        let view = View {
            scale: 1.0,
            size: game.arena_size,
        };
        game.pause();
        game.update(1.0, &Input::default(), &view);
        assert_eq!(game.gravity_fields[0].life, field_life);
        assert_eq!(game.storm.as_ref().unwrap().life, storm_life);

        game.state = State::Playing;
        game.player.pos = v2(game.arena_size.x - WALL - PLAYER_RADIUS, 600.0);
        game.lightning_arcs.clear();
        let outside = v2(game.arena_size.x + 40.0, 600.0);
        target(&mut game, outside);
        game.fire_lightning(None, 1.0, false);
        assert_eq!(game.enemies[0].hp, 1000.0);
        let arc = &game.lightning_arcs[0];
        assert_eq!(arc.from.x, game.arena_size.x - WALL);
        assert_eq!(arc.to.x, game.arena_size.x - WALL);
    }

    #[test]
    fn lightning_chains_with_falloff_and_stops_at_a_gap() {
        let mut game = arena();
        for pos in [
            v2(650.0, 600.0),
            v2(850.0, 600.0),
            v2(1000.0, 700.0),
            v2(1450.0, 1000.0),
        ] {
            target(&mut game, pos);
        }
        game.fire_lightning(None, 1.0, false);
        assert_eq!(game.lightning_arcs.len(), 3);
        assert!(game.enemies[0].hp < game.enemies[1].hp);
        assert!(game.enemies[1].hp < game.enemies[2].hp);
        assert_eq!(game.enemies[3].hp, 1000.0);
        assert!(game.enemies[..3].iter().all(|e| e.stun > 0.0));
        assert_eq!(game.lightning_arcs[0].from, v2(422.0, 600.0));
    }

    #[test]
    fn lightning_respects_aim_and_boss_entrance_and_intercepts_orbs() {
        let mut game = arena();
        target(&mut game, v2(300.0, 600.0));
        game.spawn_boss();
        let boss = game.boss.as_mut().unwrap();
        boss.pos = v2(700.0, 600.0);
        let hp = boss.hp;
        game.orbs.push(Orb {
            pos: v2(600.0, 600.0),
            prev: v2(600.0, 600.0),
            vel: V2::default(),
            life: 2.0,
            heavy: true,
        });
        game.fire_lightning(None, 1.0, false);
        assert_eq!(game.enemies[0].hp, 1000.0);
        assert_eq!(game.boss.as_ref().unwrap().hp, hp);
        assert!(game.orbs.is_empty());
        game.boss.as_mut().unwrap().entering = 0.0;
        game.fire_lightning(None, 1.0, false);
        assert!(game.boss.as_ref().unwrap().hp < hp);
    }

    #[test]
    fn charged_lightning_reaches_branches_and_expires_at_every_frame_rate() {
        let mut damages = Vec::new();
        for fps in [30, 60, 144] {
            let mut game = arena();
            for pos in [v2(650.0, 600.0), v2(850.0, 350.0), v2(850.0, 850.0)] {
                target(&mut game, pos);
            }
            game.fire_lightning(Some(1.0), 1.1, true);
            assert_eq!(game.lightning_arcs.len(), 3);
            assert!(game.enemies.iter().all(|e| e.stun > 1.5));
            for _ in 0..fps * 3 {
                game.update_fields(1.0 / fps as f32);
            }
            assert!(game.storm.is_none());
            assert!(game.lightning_arcs.is_empty());
            assert!(game.particles.len() <= MAX_PARTICLES);
            damages.push(game.enemies[0].hp);
        }
        assert!(
            damages
                .iter()
                .all(|damage| (damage - damages[0]).abs() < 0.01)
        );
    }
}
