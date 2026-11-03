//! Animated sanctuary tree helpers and their character wire points.

use super::*;

impl Duel {
    pub(super) fn shoot_dragon_event(
        &mut self,
        owner: i32,
        source: i32,
        dummy: i16,
        behavior: i32,
    ) {
        let definition = |fighter: &Combatant| {
            let judge = behavior - (200_000_000 + fighter.variation * 1000);
            fighter.bullets.get(&judge).cloned().map(|def| (judge, def))
        };
        let def = definition(&self.fighters[1]).or_else(|| {
            self.old_dragons
                .as_ref()?
                .actors
                .iter()
                .find_map(|a| definition(&a.duel.fighters[1]))
        });
        let (character, fighter) = if source == PLAYER_ENTITY {
            (&self.player, &self.fighters[0])
        } else if source == 2_500_800 {
            (&self.enemy, &self.fighters[1])
        } else if let Some(d) = self.old_dragons.as_ref().and_then(|e| e.actor(source)) {
            (&d.enemy, &d.fighters[1])
        } else {
            return;
        };
        if let Some((judge, def)) = def
            && let Some(&[point, forward, _]) = fighter
                .rig
                .frames(
                    dummy,
                    &character.bone_source_matrices(),
                    character.source_to_world(),
                )
                .first()
        {
            self.bullets
                .spawn_event(&def, owner, 1, judge, point, (-forward.x).atan2(-forward.z));
        }
    }
    pub fn sanctuary_dummy(&self, entity: i32, dummy: i16) -> Option<[glam::Vec3; 3]> {
        let (character, fighter) = if self.events.as_ref()?.boss_entity == entity {
            (&self.enemy, &self.fighters[1])
        } else {
            let d = self.old_dragons.as_ref()?.actor(entity)?;
            (&d.enemy, &d.fighters[1])
        };
        fighter
            .rig
            .frames(
                dummy,
                &character.bone_source_matrices(),
                character.source_to_world(),
            )
            .first()
            .copied()
    }

    pub(super) fn step_dragon_trees(&mut self, dt: f32, result: &mut DuelReport) {
        if self.spec.chr != "c5200" {
            return;
        }
        let dragon = self.ai_world(dt).me;
        let Some(mut cast) = self.old_dragons.take() else {
            return;
        };
        for actor in cast
            .actors
            .iter_mut()
            .filter(|a| (2_500_880..=2_500_884).contains(&a.entity))
        {
            let d = &mut actor.duel;
            if let Some(events) = &mut self.events {
                d.ai.as_mut()
                    .unwrap()
                    .state
                    .event_flags
                    .clone_from(&events.scripts.flags);
                d.ai.as_mut().unwrap().state.event_target = events
                    .scripts
                    .arena_c_state
                    .targets
                    .get(&actor.entity)
                    .filter(|&&target| target == events.boss_entity)
                    .map(|_| dragon.clone());
                if let Some(&(target, dummy)) =
                    events.scripts.arena_c_state.bone_warps.get(&actor.entity)
                    && target == events.boss_entity
                {
                    let model = self.enemy.bone_source_matrices();
                    if let Some(&[point, forward, _]) = self.fighters[1]
                        .rig
                        .frames(dummy, &model, self.enemy.source_to_world())
                        .first()
                    {
                        d.enemy.body.position = point.to_array();
                        d.enemy.snap_yaw((-forward.x).atan2(-forward.z));
                        d.enemy.body.velocity = [0.0; 3];
                    }
                }
                d.auxiliary_animation_enabled = events
                    .scripts
                    .cast_scene
                    .animation_enabled
                    .get(&actor.entity)
                    .copied()
                    .unwrap_or(true);
                if let Some(region) = events.scripts.cast_scene.warps.remove(&actor.entity)
                    && let Some(spot) = crate::events::warp_point(&self.cache, 25, 0, region)
                {
                    events
                        .scripts
                        .arena_c_state
                        .bone_warps
                        .remove(&actor.entity);
                    d.enemy.body.position = spot.position;
                    d.enemy.snap_yaw(spot.yaw);
                    d.enemy.body.velocity = [0.0; 3];
                }
            }
            self.swap_screen_player(d);
            d.shared_player_frame = true;
            d.god = self.god;
            d.immortal = self.immortal;
            actor.report = d.step(&InputFrame::default(), dt);
            self.swap_screen_player(d);
            result.hits.extend(actor.report.hits.iter().cloned());
            if let Some(events) = &mut self.events {
                for (flag, on) in d.ai.as_mut().unwrap().state.event_flag_changes.drain() {
                    events.scripts.set_flag(flag, on);
                }
            }
            let wire = d
                .enemy
                .tae_frame()
                .sp_effects
                .iter()
                .find_map(|id| cast.wire_effects.get(id))
                .copied();
            let source = format!("sanctuary:{}", actor.entity);
            let index = self
                .player
                .wire
                .points
                .iter()
                .position(|p| p.source_part == source)
                .unwrap_or_else(|| {
                    let index = self.player.wire.points.len();
                    self.player.wire.points.push(crate::wire::Point {
                        position: None,
                        variation: 53100,
                        model: d.spec.chr.clone(),
                        source_part: source,
                    });
                    index
                });
            self.player.wire.points[index].position =
                wire.and_then(|(variation, target, _landing)| {
                    self.player.wire.points[index].variation = variation;
                    d.fighters[1]
                        .rig
                        .frames(
                            target,
                            &d.enemy.bone_source_matrices(),
                            d.enemy.source_to_world(),
                        )
                        .first()
                        .map(|f| f[0].to_array())
                });
        }
        self.old_dragons = Some(cast);
    }
}
