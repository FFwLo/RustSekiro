//! Father's independently scripted c5070, sharing the already advanced Wolf frame.
use super::*;

pub struct Bird {
    pub entity: i32,
    pub duel: Box<Duel>,
    pub report: DuelReport,
    pub enabled: bool,
    warps: Vec<(i32, i32, i16)>,
    wake_animation: bool,
}

impl Bird {
    pub fn load(cache: &Path, father: &EnemySpec) -> Result<Self, LoadError> {
        let entity = crate::owl_father::bird_entity(father);
        let fail = |message| LoadError::Io(cache.to_owned(), std::io::Error::other(message));
        let roster = crate::registry::Registry::load(cache).map_err(fail)?;
        let (mut spec, spot) = roster.entity("m10_00_00_00", entity).map_err(fail)?;
        // Each placed bird shares NpcParam but selects its own battle goal.
        spec.think = spot.think;
        let mut duel = Duel::load_with(cache, 0.0, spec)?;
        if duel.ai.is_none() {
            return Err(fail(format!("missing extracted bird AI for {entity}")));
        }
        duel.events = None;
        duel.shared_player_frame = true;
        duel.enemy.body.position = spot.position;
        duel.enemy.snap_yaw(spot.yaw);
        duel.enemy.body.gravity_enabled = false;
        duel.enemy_active = false;
        Ok(Self {
            entity,
            duel: Box::new(duel),
            report: DuelReport::default(),
            enabled: false,
            warps: Vec::new(),
            wake_animation: false,
        })
    }

    pub fn place(&mut self, world: &Arc<crate::collision::CollisionWorld>) {
        self.duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
    }

    pub fn command(&mut self, command: &EventCommand, dt: f32) {
        let d = &mut self.duel;
        match *command {
            EventCommand::WarpToDummy {
                entity,
                target,
                dummy,
                ..
            } => {
                self.warps.push((entity, target, dummy as i16));
            }
            EventCommand::CharacterEnabled { entity, enabled } if entity == self.entity => {
                self.wake_animation |= enabled && !self.enabled;
                self.enabled = enabled;
            }
            EventCommand::AiEnabled { entity, enabled } if entity == self.entity => {
                d.enemy_active = enabled;
            }
            EventCommand::AiCommand {
                entity,
                command,
                slot,
            } if entity == self.entity => {
                d.ai.as_mut()
                    .unwrap()
                    .state
                    .event_requests
                    .insert(i32::from(slot), command);
            }
            EventCommand::AiReplan { entity } if entity == self.entity => {
                d.ai.as_mut().unwrap().state.replan = true;
            }
            EventCommand::EzStateRequest {
                entity,
                command,
                slot,
            } if entity == self.entity => {
                d.auxiliary_ezstate = Some((slot, command));
            }
            EventCommand::SetSpEffect { entity, id } if entity == self.entity => {
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
            EventCommand::ClearSpEffect { entity, id } if entity == self.entity => {
                d.enemy.tae_runtime.clear_timed(id);
            }
            _ => {}
        }
    }
}

impl Duel {
    pub(super) fn step_father_bird(&mut self, dt: f32, report: &mut DuelReport) {
        let Some(mut bird) = self.father_bird.take() else {
            return;
        };
        // Installed SpEffect 3506030 is the map's transparent-during-warp marker.
        if self.enemy.tae_frame().sp_effects.contains(&3506030) {
            report.enemy.fade = 1.0;
        }
        if let Some(events) = self.events.as_mut() {
            // 11005900 disables the dormant model; 11005912 enables it and forces 20000.
            // Re-enabling that model resumes animation, while later explicit freezes remain.
            if bird.wake_animation {
                events
                    .scripts
                    .cast_scene
                    .animation_enabled
                    .insert(bird.entity, true);
                bird.wake_animation = false;
            }
            // Consume one-shot map attachment requests on the current posed dummy.
            for (entity, destination, dummy) in bird.warps.drain(..) {
                if (entity == bird.entity && destination == events.boss_entity)
                    || (entity == events.boss_entity && destination == bird.entity)
                {
                    let (source, fighter) = if destination == bird.entity {
                        (&bird.duel.enemy, &bird.duel.fighters[1])
                    } else {
                        (&self.enemy, &self.fighters[1])
                    };
                    let point = if dummy < 0 {
                        Some(source.body.position)
                    } else {
                        let bones = source.bone_source_matrices();
                        fighter
                            .rig
                            .frames(dummy, &bones, source.source_to_world())
                            .first()
                            .map(|f| f[0].to_array())
                    };
                    if let Some(point) = point {
                        let yaw = source.body.yaw;
                        let character = if entity == bird.entity {
                            &mut bird.duel.enemy
                        } else {
                            &mut self.enemy
                        };
                        character.snap_yaw(yaw);
                        let body = &mut character.body;
                        body.position = point;
                        body.velocity = [0.0; 3];
                    }
                }
            }
            bird.duel.auxiliary_animation_enabled = events
                .scripts
                .cast_scene
                .animation_enabled
                .get(&bird.entity)
                .copied()
                .unwrap_or(true);
            bird.duel
                .ai
                .as_mut()
                .unwrap()
                .state
                .event_flags
                .clone_from(&events.scripts.flags);
            if let Some((animation, _)) = events.scripts.cast_scene.animations.remove(&bird.entity)
            {
                bird.duel
                    .enemy
                    .behavior
                    .runtime
                    .fire_event(&format!("W_Event{animation}"));
            }
        }
        if bird.enabled {
            self.swap_screen_player(&mut bird.duel);
            bird.duel.god = self.god;
            bird.duel.immortal = self.immortal;
            bird.report = bird.duel.step(&InputFrame::default(), dt);
            if bird.duel.enemy.tae_frame().sp_effects.contains(&3506030) {
                bird.report.enemy.fade = 1.0;
            }
            self.swap_screen_player(&mut bird.duel);
            report.hits.extend(bird.report.hits.iter().cloned());
            if let Some(events) = self.events.as_mut() {
                for (flag, on) in bird
                    .duel
                    .ai
                    .as_mut()
                    .unwrap()
                    .state
                    .event_flag_changes
                    .drain()
                {
                    events.scripts.set_flag(flag, on);
                }
            }
        }
        self.father_bird = Some(bird);
    }
}
