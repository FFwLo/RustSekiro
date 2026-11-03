//! Four independent installed-data opponents behind the Hall's hidden boss-bar controller.

use super::*;

#[path = "screen_monkeys_interactions.rs"]
mod interactions;

pub const MAP: &str = "m20_00_00_00";

pub fn controller(spec: &EnemySpec) -> bool {
    spec.chr == "c1000" && spec.npc_param == 12_600_050
}

pub struct Monkey {
    pub entity: i32,
    pub duel: Box<Duel>,
    pub report: DuelReport,
    requested_animation: Option<(i32, bool)>,
}

pub struct Encounter {
    pub monkeys: Vec<Monkey>,
    pub selected: usize,
    pub bell: interactions::Bell,
    world: Option<Arc<crate::collision::CollisionWorld>>,
    cache: PathBuf,
    camouflage_vfx: std::collections::HashSet<i32>,
}

impl Encounter {
    pub fn deathblow_available(&self, player: &Character) -> bool {
        self.monkeys.iter().any(|monkey| {
            let d = &monkey.duel;
            !self.camouflaged(monkey.entity)
                && d.rules.deathblow_open(1)
                && d.deathblow.is_none()
                && !d.hit_stop[1].active()
                && d.enemy.pending_damage.is_none()
                && d.throws.as_ref().is_some_and(|throws| {
                    select(
                        &throws.table,
                        d.spec.throw_chr(),
                        0,
                        Situation::Final,
                        &placement(player),
                        &placement(&d.enemy),
                    )
                    .is_some()
                })
        })
    }
    pub fn camouflaged(&self, entity: i32) -> bool {
        self.actor(entity).is_some_and(|actor| {
            !actor.rules.fighters[1].dead()
                && actor
                    .enemy
                    .tae_frame()
                    .sp_effects
                    .iter()
                    .chain(actor.rules.fighters[1].resident())
                    .any(|id| {
                        actor
                            .enemy
                            .tae
                            .sp_effects
                            .get(id)
                            .is_some_and(|effect| self.camouflage_vfx.contains(&effect.vfx))
                    })
        })
    }
    pub fn condition(
        &self,
        instruction: &sekiro_formats::emevd::Instruction,
        player: &Character,
    ) -> Option<bool> {
        let position = |entity| {
            if entity == PLAYER_ENTITY {
                Some(player.body.position)
            } else {
                self.actor(entity).map(|d| d.enemy.body.position)
            }
        };
        match instruction.code() {
            (5, 2) if instruction.i32_at(4)? == 12_000_825 => Some(self.bell.activated),
            (4, 9) => {
                let actor = self.actor(instruction.i32_at(4)?)?;
                Some(actor.ai.as_ref()?.state.target_state == instruction.i32_at(8)?)
            }
            (3, 3) => crate::events::radius_condition(
                instruction,
                position(instruction.i32_at(4)?)?,
                position(instruction.i32_at(8)?)?,
            ),
            _ => None,
        }
    }

    pub fn load(cache: &Path) -> Result<Self, LoadError> {
        let fail =
            |message: String| LoadError::Io(cache.to_owned(), std::io::Error::other(message));
        let registry = crate::registry::Registry::load(cache).map_err(fail)?;
        let vfx: serde_json::Value = serde_json::from_slice(
            &std::fs::read(cache.join("params/SpEffectVfxParam.json"))
                .map_err(|e| fail(e.to_string()))?,
        )
        .map_err(|e| fail(e.to_string()))?;
        let camouflage_vfx = vfx["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| {
                row["useCamouflage"].as_i64() == Some(1)
                    && row["camouflageMinAlpha"].as_i64() == Some(0)
                    && row["camouflageEndDist"].as_f64() == Some(0.0)
            })
            .filter_map(|row| row["id"].as_i64().map(|id| id as i32))
            .collect();
        let mut monkeys = Vec::new();
        for entity in 2_000_801..=2_000_804 {
            let (spec, spot) = registry.entity(MAP, entity).map_err(fail)?;
            let mut duel = Duel::load_with(cache, 0.0, spec)?;
            if duel.ai.is_none() {
                return Err(fail(format!("monkey {entity}: missing extracted Lua AI")));
            }
            duel.events = None;
            duel.enemy.body.position = spot.position;
            duel.enemy.snap_yaw(spot.yaw);
            duel.spawn = Some([placement(&duel.player), placement(&duel.enemy)]);
            duel.ai.as_mut().unwrap().state.map_regions =
                Some(crate::lua_ai::regions::MapRegions::new(cache, MAP));
            monkeys.push(Monkey {
                entity,
                duel: Box::new(duel),
                report: DuelReport::default(),
                requested_animation: None,
            });
        }
        Ok(Self {
            monkeys,
            selected: 0,
            bell: interactions::Bell::load(cache).map_err(fail)?,
            world: None,
            cache: cache.to_owned(),
            camouflage_vfx,
        })
    }

    pub fn actor(&self, entity: i32) -> Option<&Duel> {
        self.monkeys
            .iter()
            .find(|m| m.entity == entity)
            .map(|m| &*m.duel)
    }

    pub fn place(&mut self, world: &Arc<crate::collision::CollisionWorld>) {
        self.world = Some(world.clone());
        for monkey in &mut self.monkeys {
            monkey.duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
            monkey.duel.fall_respawn_depth = Some(crate::arena::FALL_RESPAWN_DEPTH);
        }
    }

    pub fn command(
        &mut self,
        command: &EventCommand,
        dt: f32,
        player: &Character,
        player_fighter: &Combatant,
    ) {
        if let EventCommand::CharacterEnabled {
            entity,
            enabled: true,
        } = *command
            && matches!(entity, 2_000_805..=2_000_809 | 2_000_811..=2_000_814)
            && self.actor(entity).is_none()
        {
            let load = || -> Result<Monkey, String> {
                let roster = crate::registry::Registry::load(&self.cache)?;
                let (spec, spot) = roster.entity(MAP, entity)?;
                let mut duel =
                    Duel::load_with(&self.cache, 0.0, spec).map_err(|e| e.to_string())?;
                if duel.ai.is_none() {
                    return Err(format!("apparition {entity}: extracted Lua AI missing"));
                }
                duel.events = None;
                duel.enemy.body.position = spot.position;
                duel.enemy.snap_yaw(spot.yaw);
                duel.ai.as_mut().unwrap().state.map_regions =
                    Some(crate::lua_ai::regions::MapRegions::new(&self.cache, MAP));
                if let Some(world) = &self.world {
                    duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
                }
                Ok(Monkey {
                    entity,
                    duel: Box::new(duel),
                    report: DuelReport::default(),
                    requested_animation: None,
                })
            };
            match load() {
                Ok(actor) => self.monkeys.push(actor),
                Err(error) => eprintln!("Hall apparition could not load: {error}"),
            }
        }
        for monkey in &mut self.monkeys {
            let d = &mut monkey.duel;
            match *command {
                EventCommand::AiCommand {
                    entity,
                    command,
                    slot,
                } if entity == monkey.entity => {
                    d.ai.as_mut()
                        .unwrap()
                        .state
                        .event_requests
                        .insert(i32::from(slot), command);
                }
                EventCommand::AiReplan { entity } if entity == monkey.entity => {
                    d.ai.as_mut().unwrap().state.replan = true;
                }
                EventCommand::AiParam { entity, row } if entity == monkey.entity => {
                    d.spec.think = row;
                    if let Some(mut ai) = Duel::load_ai(&d.cache, &d.spec) {
                        ai.state.map_regions =
                            Some(crate::lua_ai::regions::MapRegions::new(&d.cache, MAP));
                        d.ai = Some(ai);
                    }
                }
                EventCommand::EzStateRequest {
                    entity,
                    command,
                    slot,
                } if entity == monkey.entity => {
                    d.auxiliary_ezstate = Some((slot, command));
                }
                EventCommand::SetSpEffect { entity, id } if entity == monkey.entity => {
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
                EventCommand::ClearSpEffect { entity, id } if entity == monkey.entity => {
                    d.enemy.tae_runtime.clear_timed(id);
                }
                EventCommand::CharacterEnabled { entity, enabled } if entity == monkey.entity => {
                    d.enemy_active = enabled;
                }
                EventCommand::Immortality { entity, on } if entity == monkey.entity => {
                    d.rules.fighters[1].vitality.cannot_die = on;
                }
                EventCommand::CharacterKilled { entity } if entity == monkey.entity => {
                    d.rules.fighters[1].vitality.hp = 0;
                    d.rules.fighters[1].vitality.deathblows_left = 0;
                }
                EventCommand::WarpToDummy {
                    entity,
                    target: PLAYER_ENTITY,
                    dummy,
                    copy_floor: PLAYER_ENTITY,
                } if entity == monkey.entity && !d.rules.fighters[1].dead() => {
                    if let Ok(dummy) = i16::try_from(dummy)
                        && let Some(frame) = player_fighter
                            .rig
                            .frames(
                                dummy,
                                &player.bone_source_matrices(),
                                player.source_to_world(),
                            )
                            .first()
                    {
                        d.enemy.body.position = frame[0].to_array();
                        d.enemy.snap_yaw((-frame[1].x).atan2(-frame[1].z));
                        d.enemy.body.velocity = [0.0; 3];
                        d.enemy.body.grounded = player.body.grounded;
                        d.enemy.body.fall_top = d.enemy.body.position[1];
                    }
                }
                _ => {}
            }
        }
    }

    fn scene(&mut self, cache: &Path, scene: &mut crate::arena::scene::SceneState) {
        for monkey in &mut self.monkeys {
            if let Some(&enabled) = scene.animation_enabled.get(&monkey.entity) {
                monkey.duel.auxiliary_animation_enabled = enabled;
            }
            if let Some(region) = scene.warps.remove(&monkey.entity)
                && let Some(spot) = crate::events::warp_point(cache, 20, 0, region)
            {
                monkey.duel.enemy.body.position = spot.position;
                monkey.duel.enemy.snap_yaw(spot.yaw);
                monkey.duel.enemy.body.velocity = [0.0; 3];
            }
            if let Some(&request) = scene.animations.get(&monkey.entity)
                && monkey.requested_animation != Some(request)
            {
                monkey
                    .duel
                    .enemy
                    .behavior
                    .runtime
                    .fire_event(&format!("W_Event{}", request.0));
                monkey.requested_animation = Some(request);
            }
        }
    }
}

impl Duel {
    pub(super) fn apply_screen_health_effect(&mut self, id: i32) {
        let mut frame = self.enemy.tae_frame().clone();
        frame.sp_effects.retain(|effect| *effect != id);
        self.rules.apply_effect_hp(1, &frame);
        frame.sp_effects.push(id);
        self.rules.apply_effect_hp(1, &frame);
    }
    /// Moves the single Wolf, including pending reactions and recovery carry, between
    /// independent opponents; only the selected opponent advances that shared character.
    pub(super) fn swap_screen_player(&mut self, other: &mut Duel) {
        std::mem::swap(&mut self.player, &mut other.player);
        std::mem::swap(&mut self.fighters[0], &mut other.fighters[0]);
        std::mem::swap(&mut self.rules.fighters[0], &mut other.rules.fighters[0]);
        std::mem::swap(&mut self.hit_stop[0], &mut other.hit_stop[0]);
        std::mem::swap(&mut self.knockback[0], &mut other.knockback[0]);
        std::mem::swap(&mut self.last_steps[0], &mut other.last_steps[0]);
        std::mem::swap(&mut self.swing_hit, &mut other.swing_hit);
        std::mem::swap(&mut self.camera_director, &mut other.camera_director);
        std::mem::swap(&mut self.last_safe, &mut other.last_safe);
        std::mem::swap(&mut self.fall_return_fade, &mut other.fall_return_fade);
        std::mem::swap(&mut self.fall_return_part, &mut other.fall_return_part);
        std::mem::swap(&mut self.touching, &mut other.touching);
        std::mem::swap(&mut self.helper_events, &mut other.helper_events);
        std::mem::swap(&mut self.attack_button, &mut other.attack_button);
        std::mem::swap(&mut self.lock_button, &mut other.lock_button);
        std::mem::swap(&mut self.kit, &mut other.kit);
    }

    /// Advance an independent opponent with the authoritative Wolf borrowed for this step.
    pub(super) fn step_kernel(
        &mut self,
        other: &mut Duel,
        input: &InputFrame,
        dt: f32,
        advance_player: bool,
    ) -> DuelReport {
        self.swap_screen_player(other);
        other.shared_player_frame = !advance_player;
        other.god = self.god;
        other.immortal = self.immortal;
        if let (Some(spawn), Some(arena_spawn)) = (&mut other.spawn, self.spawn) {
            spawn[0] = arena_spawn[0];
        }
        let report = other.step(input, dt);
        other.respawn_in = None;
        self.swap_screen_player(other);
        report
    }
    pub(super) fn step_screen_monkeys(&mut self, input: &InputFrame, dt: f32) -> DuelReport {
        if self.spawn.is_none() {
            self.spawn = Some([placement(&self.player), placement(&self.enemy)]);
        }
        if let Some(left) = self.respawn_in.as_mut() {
            *left -= dt;
            if *left <= 0.0
                && let Err(error) = self.reset()
            {
                eprintln!("Hall respawn failed: {error}");
            }
        }
        let mut encounter = self.screen_monkeys.take().unwrap();
        encounter
            .bell
            .step(&self.player, input, self.fighters[0].height);
        if encounter.bell.activated {
            self.player
                .behavior
                .runtime
                .fire_event(&format!("W_Event{}", encounter.bell.player_animation));
        }
        let paired = encounter
            .monkeys
            .iter()
            .position(|m| m.duel.deathblow.is_some() || m.duel.enemy_throw.is_some());
        encounter.selected = paired.unwrap_or_else(|| {
            encounter
                .monkeys
                .iter()
                .enumerate()
                .filter(|(_, m)| !m.duel.rules.fighters[1].dead())
                .min_by(|(_, a), (_, b)| {
                    let distance = |m: &Monkey| {
                        glam::Vec3::from_array(m.duel.enemy.body.position)
                            .distance_squared(glam::Vec3::from_array(self.player.body.position))
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .map_or(encounter.selected, |(i, _)| i)
        });
        let selected = encounter.selected;
        let mut result = DuelReport::default();
        let passive = InputFrame::default();
        for i in
            std::iter::once(selected).chain((0..encounter.monkeys.len()).filter(|&i| i != selected))
        {
            let monkey = &mut encounter.monkeys[i];
            if let Some(events) = &self.events {
                monkey
                    .duel
                    .ai
                    .as_mut()
                    .unwrap()
                    .state
                    .event_flags
                    .clone_from(&events.scripts.flags);
            }
            if monkey
                .report
                .enemy
                .behavior
                .events
                .iter()
                .any(|event| event.starts_with("W_Event"))
            {
                monkey.duel.auxiliary_ezstate = None;
            }
            let report = self.step_kernel(
                &mut monkey.duel,
                if i == selected { input } else { &passive },
                dt,
                i == selected,
            );
            if i == selected {
                result = report.clone();
            } else {
                result.hits.extend(report.hits.clone());
            }
            monkey.report = report;
            if let Some(events) = &mut self.events {
                for (flag, on) in monkey
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
        self.last_steps[0] = result.player.clone();
        self.screen_monkeys = Some(encounter);
        self.step_events(dt);
        let mut encounter = self.screen_monkeys.take().unwrap();
        if let Some(events) = self.events.as_mut() {
            encounter.scene(&self.cache, &mut events.scripts.cast_scene);
        }
        self.screen_monkeys = Some(encounter);
        // Screen deaths apply the map's controller SpEffect, rather than summing invented HP.
        result.enemy = self.enemy.tick(&InputFrame::default(), dt);
        self.rules.apply_effect_hp(1, self.enemy.tae_frame());
        if self.rules.fighters[0].dead() && self.respawn_in.is_none() {
            self.respawn_in = Some(RESPAWN_DELAY);
        }
        result
    }
}
