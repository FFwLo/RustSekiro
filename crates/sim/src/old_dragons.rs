//! Sanctuary cast: separate installed Lua/HKS instances sharing Wolf and map events.

use super::*;

pub const CONTROLLER: i32 = 2_500_810;
pub const MAP: &str = "m25_00_00_00";

pub fn owns(spec: &EnemySpec) -> bool {
    spec.chr == "c5300" && spec.npc_param == 53_000_000
}

pub struct Actor {
    pub entity: i32,
    pub duel: Box<Duel>,
    pub report: DuelReport,
    pub revivals: u32,
    pub(super) groups: [i32; 8],
    revival: bool,
    stat_ids: std::collections::HashSet<i32>,
}

pub struct Encounter {
    pub actors: Vec<Actor>,
    pub selected: usize,
    world: Option<Arc<crate::collision::CollisionWorld>>,
    stat_effects: std::collections::HashMap<i32, (f32, f32)>,
    pub(super) wire_effects: std::collections::HashMap<i32, (u32, i16, i16)>,
}

impl Encounter {
    pub fn load(cache: &Path) -> Result<Self, LoadError> {
        let fail = |message| LoadError::Io(cache.to_owned(), std::io::Error::other(message));
        let registry = crate::registry::Registry::load(cache).map_err(fail)?;
        let bytes = std::fs::read(cache.join(format!("raw/map/MapStudio/{MAP}.msb")))
            .map_err(|e| fail(e.to_string()))?;
        let map = sekiro_formats::msb::parse(&bytes).map_err(|e| fail(e.to_string()))?;
        let stats: serde_json::Value = serde_json::from_slice(
            &std::fs::read(cache.join("params/SpEffectParam.json"))
                .map_err(|e| fail(e.to_string()))?,
        )
        .map_err(|e| fail(e.to_string()))?;
        let stat_effects = stats["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                let hp = r["maxHpRate"].as_f64()? as f32;
                let sp = r["maxStaminaRate"].as_f64()? as f32;
                (hp != 1.0 || sp != 1.0).then_some((r["id"].as_i64()? as i32, (hp, sp)))
            })
            .collect();
        let mut actors = Vec::new();
        let wire_effects = stats["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                let variation = r["chrWireVariationNo"].as_i64()?;
                (variation >= 0).then_some((
                    r["id"].as_i64()? as i32,
                    (
                        variation as u32,
                        r["chrWireTargetDmypolyId"].as_i64()? as i16,
                        r["chrWireLandingPointDmypolyId"].as_i64()? as i16,
                    ),
                ))
            })
            .collect();
        for entity in (2_500_811..=2_500_835)
            .chain([2_500_840, 2_500_930, 2_500_933, 2_500_934])
            .chain(2_500_880..=2_500_884)
        {
            let (mut spec, spot) = registry.entity(MAP, entity).map_err(fail)?;
            spec.think = spot.think;
            spec.starts_in_battle = true;
            let mut duel = Duel::load_with(cache, 0.0, spec)?;
            if duel.ai.is_none() {
                return Err(fail(format!("Old Dragon {entity}: extracted Lua missing")));
            }
            duel.events = None;
            duel.enemy.body.position = spot.position;
            duel.enemy.snap_yaw(spot.yaw);
            duel.enemy_active = duel.spec.chr == "c5310";
            duel.ai.as_mut().unwrap().state.map_regions =
                Some(crate::lua_ai::regions::MapRegions::new(cache, MAP));
            actors.push(Actor {
                entity,
                duel: Box::new(duel),
                report: DuelReport::default(),
                revivals: 0,
                groups: map
                    .parts
                    .iter()
                    .find(|p| p.entity_id == entity)
                    .map_or([-1; 8], |p| p.entity_group_ids),
                revival: false,
                stat_ids: std::collections::HashSet::new(),
            });
        }
        Ok(Self {
            actors,
            selected: 0,
            world: None,
            stat_effects,
            wire_effects,
        })
    }

    pub fn actor(&self, entity: i32) -> Option<&Duel> {
        self.actors
            .iter()
            .find(|a| a.entity == entity)
            .map(|a| &*a.duel)
    }

    fn matches(entity: i32, actor: &Actor) -> bool {
        entity == actor.entity || actor.groups.contains(&entity)
    }

    pub fn group_effect(&self, entity: i32, id: i32) -> Option<bool> {
        (entity == 2_505_830).then(|| {
            self.actors.iter().any(|a| {
                Self::matches(entity, a)
                    && (a.duel.enemy.tae_frame().sp_effects.contains(&id)
                        || a.duel.rules.fighters[1].resident().contains(&id))
            })
        })
    }

    pub fn place(&mut self, world: &Arc<crate::collision::CollisionWorld>) {
        self.world = Some(world.clone());
        for actor in &mut self.actors {
            actor.duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
        }
    }

    pub fn reset(&mut self, cache: &Path) -> Result<(), LoadError> {
        let world = self.world.clone();
        *self = Self::load(cache)?;
        if let Some(world) = world {
            self.place(&world);
        }
        Ok(())
    }

    pub fn command(&mut self, command: &EventCommand, dt: f32) {
        for actor in &mut self.actors {
            let own_entity = actor.entity;
            let groups = actor.groups;
            let matches = |entity| entity == own_entity || groups.contains(&entity);
            let d = &mut actor.duel;
            match *command {
                EventCommand::AiCommand {
                    entity,
                    command,
                    slot,
                } if matches(entity) => {
                    d.ai.as_mut()
                        .unwrap()
                        .state
                        .event_requests
                        .insert(i32::from(slot), command);
                }
                EventCommand::AiEnabled { entity, enabled }
                | EventCommand::CharacterEnabled { entity, enabled }
                    if matches(entity) =>
                {
                    d.enemy_active = enabled
                }
                EventCommand::AiReplan { entity } if matches(entity) => {
                    d.ai.as_mut().unwrap().state.replan = true
                }
                EventCommand::Immortality { entity, on } if matches(entity) => {
                    d.rules.fighters[1].vitality.cannot_die = on
                }
                EventCommand::Gravity { entity, enabled } if matches(entity) => {
                    d.enemy.body.gravity_enabled = enabled;
                    if !enabled {
                        d.enemy.body.velocity[1] = 0.0;
                    }
                }
                EventCommand::SetSpEffect { entity, id } if matches(entity) => {
                    if self.stat_effects.contains_key(&id) {
                        actor.stat_ids.insert(id);
                        let (hp, sp) = actor
                            .stat_ids
                            .iter()
                            .filter_map(|id| self.stat_effects.get(id))
                            .fold((1.0, 1.0), |(hp, sp), &(h, s)| (hp * h, sp * s));
                        let f = &mut d.rules.fighters[1];
                        if let crate::fight::FighterKind::Npc(npc) = &f.kind {
                            f.vitality.max_hp = (npc.hp as f32 * hp) as i32;
                            f.vitality.hp = f.vitality.hp.min(f.vitality.max_hp);
                            f.posture.max = (npc.max_posture as f32 * sp) as i32;
                            f.posture.remaining = f.posture.remaining.min(f.posture.max);
                        }
                    }
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
                EventCommand::ClearSpEffect { entity, id } if matches(entity) => {
                    d.enemy.tae_runtime.clear_timed(id)
                }
                _ => {}
            }
        }
    }
}

impl Duel {
    pub(super) fn step_old_dragons(&mut self, input: &InputFrame, dt: f32) -> DuelReport {
        // Keep the cast available to commands issued when the cinematic warp completes.
        if self.cinematic.is_some() {
            self.defer_map_events = true;
            let result = self.step(input, dt);
            self.defer_map_events = false;
            return result;
        }
        if self.spawn.is_none() {
            self.spawn = Some([placement(&self.player), placement(&self.enemy)]);
        }
        if let Some(left) = self.respawn_in.as_mut() {
            *left -= dt;
            if *left <= 0.0
                && let Err(error) = self.reset()
            {
                eprintln!("sanctuary reset: {error}");
            }
        }
        let mut encounter = self.old_dragons.take().unwrap();
        let event_actors: std::collections::HashMap<_, _> = encounter
            .actors
            .iter()
            .map(|a| (a.entity, a.duel.ai_world(dt).me))
            .collect();
        let player = glam::Vec3::from_array(self.player.body.position);
        encounter.selected = encounter
            .actors
            .iter()
            .position(|a| a.duel.deathblow.is_some() || a.duel.enemy_throw.is_some())
            .unwrap_or_else(|| {
                encounter
                    .actors
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.duel.spec.chr == "c5300" && !a.duel.rules.fighters[1].dead())
                    .min_by(|(_, a), (_, b)| {
                        let dist = |a: &Actor| {
                            glam::Vec3::from_array(a.duel.enemy.body.position)
                                .distance_squared(player)
                        };
                        dist(a).total_cmp(&dist(b))
                    })
                    .map_or(encounter.selected, |(i, _)| i)
            });
        let selected = encounter.selected;
        let mut result = DuelReport::default();
        let passive = InputFrame::default();
        for index in
            std::iter::once(selected).chain((0..encounter.actors.len()).filter(|&i| i != selected))
        {
            let actor = &mut encounter.actors[index];
            if (2_500_880..=2_500_884).contains(&actor.entity) {
                continue;
            }
            let d = &mut actor.duel;
            if let Some(events) = &self.events {
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
                    .and_then(|target| event_actors.get(target))
                    .cloned();
            }
            self.swap_screen_player(d);
            d.shared_player_frame = index != selected;
            d.god = self.god;
            d.immortal = self.immortal;
            let report = d.step(if index == selected { input } else { &passive }, dt);
            if index == selected {
                result = report.clone();
            } else {
                result.hits.extend(report.hits.iter().cloned());
            }
            actor.report = report;
            self.swap_screen_player(d);
            // Only the white dragons' death clip opens resurrection action 103.
            // Native resurrection scheduling is inferred from that clip's completion.
            if d.spec.chr == "c5300" && d.rules.fighters[1].dead() {
                actor.revival |= d.enemy.tae_frame().action_unlocks.contains(&103);
                if actor.revival
                    && (d.enemy.state().starts_with("DeathIdle")
                        || d.enemy
                            .behavior
                            .runtime
                            .main_clip()
                            .is_some_and(|c| c.at_end()))
                {
                    d.rules.fighters[1].vitality.hp = d.rules.fighters[1].vitality.max_hp;
                    d.rules.fighters[1].posture.remaining = d.rules.fighters[1].posture.max;
                    d.enemy.pending_damage = None;
                    d.enemy.behavior.runtime.fire_event("W_Event20000");
                    actor.revivals += 1;
                    actor.revival = false;
                }
            }
            if let Some(events) = &mut self.events {
                for (flag, on) in d.ai.as_mut().unwrap().state.event_flag_changes.drain() {
                    events.scripts.set_flag(flag, on);
                }
            }
        }
        self.last_steps[0] = result.player.clone();
        self.old_dragons = Some(encounter);
        self.step_events(dt);
        // The hidden controller advances TAE and HP effects, never attacks Wolf.
        result.enemy = self.enemy.tick(&InputFrame::default(), dt);
        self.rules.apply_effect_hp(1, self.enemy.tae_frame());
        if self.rules.fighters[0].dead() && self.respawn_in.is_none() {
            self.respawn_in = Some(RESPAWN_DELAY);
        }
        result
    }
}
