//! Eight placed c1510 actors sharing Wolf's frame and the map's event flags.
use super::*;

pub struct Illusion {
    pub entity: i32,
    pub duel: Box<Duel>,
    pub report: DuelReport,
    animation: Option<(i32, bool)>,
    pulse_contacts: std::collections::HashMap<u64, f32>,
}

pub struct Encounter {
    pub actors: Vec<Illusion>,
    shooter_effects: std::collections::HashMap<i32, i32>,
    shooter_contacts: std::collections::HashSet<u64>,
}

impl Encounter {
    pub fn load(cache: &Path) -> Result<Self, LoadError> {
        let fail = |message| LoadError::Io(cache.to_owned(), std::io::Error::other(message));
        let registry = crate::registry::Registry::load(cache).map_err(fail)?;
        let mut actors = Vec::new();
        for entity in 1_000_820..=1_000_827 {
            let (mut spec, spot) = registry.entity("m10_00_00_00", entity).map_err(fail)?;
            spec.think = spot.think;
            spec.starts_in_battle = true;
            let mut duel = Duel::load_with(cache, 0.0, spec)?;
            if duel.ai.is_none() {
                return Err(fail(format!(
                    "Butterfly illusion {entity}: extracted Lua is missing"
                )));
            }
            duel.events = None;
            duel.enemy.body.position = spot.position;
            duel.enemy.snap_yaw(spot.yaw);
            duel.enemy_active = false;
            duel.shared_player_frame = true;
            duel.ai.as_mut().unwrap().state.map_regions = Some(
                crate::lua_ai::regions::MapRegions::new(cache, "m10_00_00_00"),
            );
            actors.push(Illusion {
                entity,
                duel: Box::new(duel),
                report: Default::default(),
                animation: None,
                pulse_contacts: Default::default(),
            });
        }
        let table = crate::hits::load_table(
            &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            &cache.join("refs/paramdex/Defs"),
            "Bullet",
            "BulletParam",
        )
        .ok_or_else(|| fail("Butterfly: extracted Bullet table is missing".to_owned()))?;
        let shooter_effects = table
            .rows()
            .filter_map(|row| {
                let id = row.i32("spEffectIDForShooter").ok()?;
                (id > 0).then_some((row.id(), id))
            })
            .collect();
        Ok(Self {
            actors,
            shooter_effects,
            shooter_contacts: Default::default(),
        })
    }

    pub fn actor(&self, entity: i32) -> Option<&Duel> {
        self.actors
            .iter()
            .find(|a| a.entity == entity)
            .map(|a| &*a.duel)
    }

    pub fn place(&mut self, world: &Arc<crate::collision::CollisionWorld>) {
        for actor in &mut self.actors {
            actor.duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
        }
    }

    pub fn command(&mut self, command: &EventCommand, dt: f32) {
        for actor in &mut self.actors {
            let d = &mut actor.duel;
            match *command {
                EventCommand::CharacterEnabled { entity, enabled } if entity == actor.entity => {
                    d.enemy_active = enabled
                }
                EventCommand::AiCommand {
                    entity,
                    slot,
                    command,
                } if entity == actor.entity => {
                    d.ai.as_mut()
                        .unwrap()
                        .state
                        .event_requests
                        .insert(i32::from(slot), command);
                }
                EventCommand::AiReplan { entity } if entity == actor.entity => {
                    d.ai.as_mut().unwrap().state.replan = true
                }
                EventCommand::CharacterKilled { entity } if entity == actor.entity => {
                    d.rules.fighters[1].vitality.hp = 0
                }
                EventCommand::SetSpEffect { entity, id } if entity == actor.entity => {
                    let duration = d.enemy.tae.sp_effects.get(&id).map_or(dt, |e| e.duration);
                    d.enemy.tae_runtime.add_timed(
                        id,
                        if duration < 0.0 {
                            f32::INFINITY
                        } else {
                            duration.max(dt)
                        },
                    );
                }
                EventCommand::ClearSpEffect { entity, id } if entity == actor.entity => {
                    d.enemy.tae_runtime.clear_timed(id)
                }
                _ => {}
            }
        }
    }
}

impl Duel {
    pub(super) fn step_butterfly_illusions(&mut self, dt: f32, report: &mut DuelReport) {
        let Some(mut encounter) = self.butterfly_illusions.take() else {
            return;
        };
        encounter
            .shooter_contacts
            .retain(|serial| self.bullets.live.iter().any(|b| b.serial == *serial));
        for bullet in self.bullets.live.iter().filter(|b| b.shooter == 1) {
            if encounter.shooter_contacts.insert(bullet.serial)
                && let Some(&id) = encounter.shooter_effects.get(&bullet.def.id)
            {
                let duration = self
                    .enemy
                    .tae
                    .sp_effects
                    .get(&id)
                    .map_or(dt, |e| e.duration);
                self.enemy.tae_runtime.add_timed(
                    id,
                    if duration < 0.0 {
                        f32::INFINITY
                    } else {
                        duration.max(dt)
                    },
                );
            }
        }
        for actor in &mut encounter.actors {
            let d = &mut actor.duel;
            if let Some(events) = &self.events {
                d.ai.as_mut()
                    .unwrap()
                    .state
                    .event_flags
                    .clone_from(&events.scripts.flags);
                if let Some(&animation) = events.scripts.cast_scene.animations.get(&actor.entity)
                    && actor.animation != Some(animation)
                {
                    actor.animation = Some(animation);
                    if animation.0 >= 0 {
                        d.enemy
                            .behavior
                            .runtime
                            .fire_event(&format!("W_Event{}", animation.0));
                    }
                }
            }
            if !d.enemy_active {
                continue;
            }
            actor
                .pulse_contacts
                .retain(|serial, _| self.bullets.live.iter().any(|b| b.serial == *serial));
            // These installed system bullets carry effects to the illusion cast.
            // Other boss projectiles remain on the ordinary hostile contact path.
            for bullet in self.bullets.live.iter().filter(|b| {
                b.shooter == 1 && matches!(b.def.id, 50_900_920 | 50_900_921 | 50_900_930)
            }) {
                let position = glam::Vec3::from_array(d.enemy.body.position);
                let radius = d.fighters[1].radius;
                let bottom = position + glam::Vec3::Y * radius;
                let top = position + glam::Vec3::Y * (d.fighters[1].height - radius).max(radius);
                if crate::hits::segment_distance(bullet.position, bullet.position, bottom, top)
                    > bullet.radius + radius
                {
                    continue;
                }
                if let Some(&age) = actor.pulse_contacts.get(&bullet.serial)
                    && (bullet.def.rehit <= 0.0 || bullet.age - age < bullet.def.rehit - 0.0001)
                {
                    continue;
                }
                actor.pulse_contacts.insert(bullet.serial, bullet.age);
                for &id in &bullet.def.sp_effects {
                    let duration = d.enemy.tae.sp_effects.get(&id).map_or(dt, |e| e.duration);
                    d.enemy.tae_runtime.add_timed(
                        id,
                        if duration < 0.0 {
                            f32::INFINITY
                        } else {
                            duration.max(dt)
                        },
                    );
                }
            }
            d.god = self.god;
            d.immortal = self.immortal;
            std::mem::swap(&mut self.player, &mut d.player);
            std::mem::swap(&mut self.fighters[0], &mut d.fighters[0]);
            std::mem::swap(&mut self.rules.fighters[0], &mut d.rules.fighters[0]);
            std::mem::swap(&mut self.hit_stop[0], &mut d.hit_stop[0]);
            std::mem::swap(&mut self.knockback[0], &mut d.knockback[0]);
            std::mem::swap(&mut self.last_steps[0], &mut d.last_steps[0]);
            std::mem::swap(&mut self.swing_hit, &mut d.swing_hit);
            actor.report = d.step(&InputFrame::default(), dt);
            std::mem::swap(&mut self.player, &mut d.player);
            std::mem::swap(&mut self.fighters[0], &mut d.fighters[0]);
            std::mem::swap(&mut self.rules.fighters[0], &mut d.rules.fighters[0]);
            std::mem::swap(&mut self.hit_stop[0], &mut d.hit_stop[0]);
            std::mem::swap(&mut self.knockback[0], &mut d.knockback[0]);
            std::mem::swap(&mut self.last_steps[0], &mut d.last_steps[0]);
            std::mem::swap(&mut self.swing_hit, &mut d.swing_hit);
            report.hits.extend(actor.report.hits.iter().cloned());
            report.perilous = report.perilous.or(actor.report.perilous);
            if let Some(events) = self.events.as_mut() {
                for (flag, on) in d.ai.as_mut().unwrap().state.event_flag_changes.drain() {
                    if on {
                        events.scripts.flags.insert(flag);
                    } else {
                        events.scripts.flags.remove(&flag);
                    }
                }
            }
        }
        self.butterfly_illusions = Some(encounter);
    }
}
