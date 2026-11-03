//! The burrow's brown ape is c5100_0002, not its c5001 centipede.
//! Each ape runs an independent installed Lua/HKS runtime against one shared Wolf.
use super::*;

pub const BROWN: i32 = 1_700_852;

pub struct Encounter {
    pub duel: Box<Duel>,
    pub report: DuelReport,
    animation: Option<(i32, bool)>,
}

impl Encounter {
    pub fn load(cache: &Path) -> Result<Self, LoadError> {
        let fail = |message| LoadError::Io(cache.to_owned(), std::io::Error::other(message));
        let registry = crate::registry::Registry::load(cache).map_err(fail)?;
        let (mut spec, spot) = registry.entity("m17_00_00_00", BROWN).map_err(fail)?;
        spec.starts_in_battle = true;
        let mut duel = Duel::load_with(cache, 0.0, spec)?;
        if duel.ai.is_none() {
            return Err(fail("brown ape: missing extracted Lua".to_owned()));
        }
        duel.events = None;
        duel.enemy.body.position = spot.position;
        duel.enemy.snap_yaw(spot.yaw);
        duel.enemy_active = false;
        duel.shared_player_frame = true;
        duel.ai.as_mut().unwrap().state.map_regions = Some(
            crate::lua_ai::regions::MapRegions::new(cache, "m17_00_00_00"),
        );
        Ok(Self {
            duel: Box::new(duel),
            report: Default::default(),
            animation: None,
        })
    }

    pub fn place(&mut self, world: &Arc<crate::collision::CollisionWorld>) {
        self.duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
    }

    pub fn command(&mut self, command: &EventCommand, dt: f32) {
        let d = &mut self.duel;
        match *command {
            EventCommand::CharacterEnabled {
                entity: BROWN,
                enabled,
            } => d.enemy_active = enabled,
            EventCommand::AiCommand {
                entity: BROWN,
                slot,
                command,
            } => {
                d.ai.as_mut()
                    .unwrap()
                    .state
                    .event_requests
                    .insert(i32::from(slot), command);
            }
            EventCommand::AiReplan { entity: BROWN } => d.ai.as_mut().unwrap().state.replan = true,
            EventCommand::CharacterKilled { entity: BROWN } => {
                d.rules.fighters[1].vitality.deathblows_left = 0;
                d.rules.fighters[1].vitality.cannot_die = false;
                d.rules.fighters[1].vitality.hp = 0;
            }
            EventCommand::Immortality { entity: BROWN, on } => {
                d.rules.fighters[1].vitality.cannot_die = on
            }
            EventCommand::SetSpEffect { entity: BROWN, id } => {
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
            EventCommand::ClearSpEffect { entity: BROWN, id } => {
                d.enemy.tae_runtime.clear_timed(id)
            }
            _ => {}
        }
    }
}

impl Duel {
    /// The companion's execution marker uses its own vitality and installed throw range.
    pub fn brown_deathblow_available(&self) -> bool {
        let Some(encounter) = &self.headless_ape else {
            return false;
        };
        let d = &encounter.duel;
        if !d.enemy_active
            || d.rules.fighters[1].dead()
            || !d.rules.deathblow_open(1)
            || d.deathblow.is_some()
            || self.deathblow.is_some()
            || self.rules.fighters[0].dead()
            || d.hit_stop[1].active()
            || d.enemy.pending_damage.is_some()
        {
            return false;
        }
        let Some(throws) = &d.throws else {
            return false;
        };
        crate::deathblow::select(
            &throws.table,
            5100,
            0,
            Situation::Final,
            &placement(&self.player),
            &placement(&d.enemy),
        )
        .is_some()
    }
    /// The selected ape advances Wolf once; the other resolves its attacks on that frame.
    /// Paired throws retain their owner until they release, regardless of target distance.
    pub(super) fn step_ape_encounter(&mut self, input: &InputFrame, dt: f32) -> DuelReport {
        let mut encounter = self.headless_ape.take().unwrap();
        let d = &mut encounter.duel;
        if let Some(events) = &self.events {
            d.ai.as_mut()
                .unwrap()
                .state
                .event_flags
                .clone_from(&events.scripts.flags);
            if let Some(&animation) = events.scripts.cast_scene.animations.get(&BROWN)
                && encounter.animation != Some(animation)
            {
                encounter.animation = Some(animation);
                d.enemy
                    .behavior
                    .runtime
                    .fire_event(&format!("W_Event{}", animation.0));
            }
        }
        let targets = self
            .events
            .as_ref()
            .map(|e| &e.scripts.arena_c_state.targets);
        d.ai.as_mut().unwrap().state.event_target = targets
            .filter(|t| t.get(&BROWN) == Some(&1_700_850))
            .map(|_| self.ai_world(dt).me);
        let target = targets
            .filter(|t| t.get(&1_700_850) == Some(&BROWN))
            .map(|_| d.ai_world(dt).me);
        if let Some(ai) = self.ai.as_mut() {
            ai.state.event_target = target;
        }
        let distance = |p: [f32; 3]| {
            glam::Vec3::from_array(p)
                .distance_squared(glam::Vec3::from_array(self.player.body.position))
        };
        let brown_selected = d.enemy_active
            && (d.deathblow.is_some()
                || d.enemy_throw.is_some()
                || (!d.rules.fighters[1].dead()
                    && self.deathblow.is_none()
                    && self.enemy_throw.is_none()
                    && (!self.enemy_active
                        || distance(d.enemy.body.position) < distance(self.enemy.body.position))));
        self.defer_map_events = true;
        d.god = self.god;
        d.immortal = self.immortal;
        let passive = InputFrame::default();
        let mut main_report;
        if brown_selected {
            self.swap_screen_player(d);
            d.shared_player_frame = false;
            encounter.report = d.step(input, dt);
            d.respawn_in = None;
            self.swap_screen_player(d);
            self.shared_player_frame = true;
            main_report = self.step(&passive, dt);
            self.shared_player_frame = false;
            main_report.player = encounter.report.player.clone();
            main_report.deathblow = encounter.report.deathblow.clone();
            main_report.grab = encounter.report.grab.clone();
            main_report.locked_on = encounter.report.locked_on;
        } else {
            main_report = self.step(input, dt);
            if d.enemy_active {
                self.swap_screen_player(d);
                d.shared_player_frame = true;
                encounter.report = d.step(&passive, dt);
                self.swap_screen_player(d);
            } else {
                encounter.report = DuelReport::default();
            }
        }
        main_report
            .hits
            .extend(encounter.report.hits.iter().cloned());
        main_report.perilous = main_report.perilous.or(encounter.report.perilous);
        if let Some(events) = self.events.as_mut() {
            for (flag, on) in d.ai.as_mut().unwrap().state.event_flag_changes.drain() {
                if on {
                    events.scripts.flags.insert(flag);
                } else {
                    events.scripts.flags.remove(&flag);
                }
            }
        }
        self.defer_map_events = false;
        self.headless_ape = Some(encounter);
        self.step_events(dt);
        main_report
    }
}
