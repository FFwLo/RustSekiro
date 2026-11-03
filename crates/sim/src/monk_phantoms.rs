//! The bridge's three c5005 placements, each running its own shipped Lua and HKS.

use super::*;

pub struct Phantom {
    pub entity: i32,
    pub duel: Box<Duel>,
    pub report: DuelReport,
}

pub struct Encounter {
    pub actors: Vec<Phantom>,
}

impl Encounter {
    pub fn load(cache: &Path) -> Result<Self, LoadError> {
        let fail = |message| LoadError::Io(cache.to_owned(), std::io::Error::other(message));
        let registry = crate::registry::Registry::load(cache).map_err(fail)?;
        let mut actors = Vec::new();
        for entity in [2_500_851, 2_500_853, 2_500_854] {
            let (mut spec, spot) = registry.entity("m25_00_00_00", entity).map_err(fail)?;
            // These are summoned into an active boss battle, with resident combat effect 200004.
            spec.starts_in_battle = true;
            let mut duel = Duel::load_with(cache, 0.0, spec)?;
            if duel.ai.is_none() {
                return Err(fail(format!("Monk phantom {entity}: missing extracted AI")));
            }
            duel.events = None;
            duel.enemy.body.position = spot.position;
            duel.enemy.snap_yaw(spot.yaw);
            duel.enemy_active = false;
            duel.shared_player_frame = true;
            duel.ai.as_mut().unwrap().state.map_regions = Some(
                crate::lua_ai::regions::MapRegions::new(cache, "m25_00_00_00"),
            );
            actors.push(Phantom {
                entity,
                duel: Box::new(duel),
                report: DuelReport::default(),
            });
        }
        Ok(Self { actors })
    }

    pub fn actor(&self, entity: i32) -> Option<&Duel> {
        self.actors
            .iter()
            .find(|p| p.entity == entity)
            .map(|p| &*p.duel)
    }

    pub fn place(&mut self, world: &Arc<crate::collision::CollisionWorld>) {
        for actor in &mut self.actors {
            actor.duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
        }
    }

    pub fn command(&mut self, c: &EventCommand, dt: f32) {
        for actor in &mut self.actors {
            let d = &mut actor.duel;
            match *c {
                EventCommand::AiCommand {
                    entity,
                    command,
                    slot,
                } if entity == actor.entity => {
                    d.ai.as_mut()
                        .unwrap()
                        .state
                        .event_requests
                        .insert(i32::from(slot), command);
                }
                EventCommand::AiReplan { entity } if entity == actor.entity => {
                    d.ai.as_mut().unwrap().state.replan = true;
                }
                EventCommand::CharacterEnabled { entity, enabled } if entity == actor.entity => {
                    d.enemy_active = enabled;
                }
                EventCommand::Immortality { entity, on } if entity == actor.entity => {
                    d.rules.fighters[1].vitality.cannot_die = on;
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
                    d.enemy.tae_runtime.clear_timed(id);
                }
                _ => {}
            }
        }
    }
}

impl Duel {
    /// Primary Monk advances Wolf once; each phantom tests its attacks on that same pose.
    pub(super) fn step_monk_phantoms(&mut self, dt: f32, result: &mut DuelReport) {
        let Some(mut encounter) = self.monk_phantoms.take() else {
            return;
        };
        for actor in &mut encounter.actors {
            let d = &mut actor.duel;
            if let Some(events) = &self.events {
                d.auxiliary_animation_enabled = events
                    .scripts
                    .cast_scene
                    .animation_enabled
                    .get(&actor.entity)
                    .copied()
                    .unwrap_or(true);
                d.ai.as_mut()
                    .unwrap()
                    .state
                    .event_flags
                    .clone_from(&events.scripts.flags);
                if let Some(&region) = events.scripts.cast_scene.warps.get(&actor.entity)
                    && let Some(spot) = crate::events::warp_point(&self.cache, 25, 0, region)
                {
                    d.enemy.body.position = spot.position;
                    d.enemy.snap_yaw(spot.yaw);
                    d.enemy.body.velocity = [0.0; 3];
                }
            }
            if !d.enemy_active {
                continue;
            }
            d.god = self.god;
            d.immortal = self.immortal;
            // Transfer Wolf's runtime and contact state without cloning or advancing him.
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
            result.hits.extend(actor.report.hits.iter().cloned());
            result.perilous = result.perilous.or(actor.report.perilous);
        }
        if let Some(events) = self.events.as_mut() {
            for actor in &encounter.actors {
                events.scripts.cast_scene.warps.remove(&actor.entity);
            }
        }
        self.monk_phantoms = Some(encounter);
    }
}
