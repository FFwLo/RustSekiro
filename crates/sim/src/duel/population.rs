//! Independent combat kernels sharing one authoritative Wolf, like the installed boss adds.
use super::*;

/// One NpcThinkParam sight set: `eye_*_normal`, or `eye_*_perceive` once the foe has a target.
///
/// The exe's cone test (FUN_14061bb70, eye point FUN_14061af30): the eye is the observer's
/// feet pushed `back` metres behind along its facing, and the target's feet must lie from
/// `begin` to `dist` plus the target's radius from it, within `left`/`right` degrees of yaw
/// and `down`/`up` degrees of elevation.
/// Ported from FwLo's RustSekiro (`src/stealth.rs`, `in_cone`), without the SpEffect sight cuts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SightCone {
    pub dist: f32,
    pub begin: f32,
    pub back: f32,
    pub up: f32,
    pub down: f32,
    pub left: f32,
    pub right: f32,
}

impl SightCone {
    /// The `set` ("normal", "perceive" or "around") of a think row read through `field`.
    pub fn from_think(set: &str, field: impl Fn(&str) -> f32) -> Self {
        let get = |name: &str| field(&format!("eye_{name}_{set}"));
        Self {
            dist: get("dist"),
            begin: get("BeginDist"),
            back: get("BackOffsetDist"),
            up: get("ang_upper"),
            down: get("ang_bottom"),
            left: get("ang_left"),
            right: get("ang_right"),
        }
    }

    /// Whether a target with its feet at `target` and physics radius `radius` is inside the
    /// cone of an observer with its feet at `feet` facing `yaw`.
    pub fn contains(&self, feet: glam::Vec3, yaw: f32, target: glam::Vec3, radius: f32) -> bool {
        let fwd = crate::arena::forward(yaw);
        let d = target - (feet - fwd * self.back);
        let d2 = d.length_squared();
        let far = radius + self.dist;
        if d2 < self.begin * self.begin || d2 >= far * far {
            return false;
        }
        let flat = d.with_y(0.0);
        // Positive to the observer's left, the way yaw turns.
        let side = flat.dot(glam::Vec3::Y.cross(fwd));
        let yaw = side.atan2(flat.dot(fwd)).to_degrees();
        let elevation = d.y.atan2(flat.length()).to_degrees();
        (-self.right..=self.left).contains(&yaw) && (-self.down..=self.up).contains(&elevation)
    }
}

/// One AiSoundParam row as a listener reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiSound {
    pub radius: f32,
    /// AI_SOUND_RANK (0 normal, 1 important).
    pub rank: i32,
    /// `bSpEffectEnable`: the maker's SpEffects scale the radius (`hearingSearchEnemyRate`).
    pub scaled: bool,
    /// `opposeTarget`: heard by the maker's enemies (every map foe, for Wolf's sounds).
    pub oppose: bool,
}

/// What a listener's ear is: NpcThinkParam `ear_dist` and `ear_soundcut_dist`, and its
/// hearing rate, prod(1 - hearingSearchEnemyCut / 100) over its SpEffects (FUN_140bfe590).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hearing {
    pub ear: f32,
    pub cut: f32,
    pub rate: f32,
}

impl Hearing {
    /// Whether a sound of `radius` (already scaled by its maker) made with its maker's centre
    /// `distance` metres from the listener's is heard, the bodies' radii adding up to `radii`.
    ///
    /// The exe's listener (SprjAiSoundNormalListener FUN_140610470 -> FUN_140610b30 ->
    /// FUN_140610a30): reach = rate x radius - ear_soundcut_dist, none if <= 0; heard while
    /// the gap between the bodies is under reach + ear_dist x the listener's factor at +0x20
    /// (gap: taken as 1). Ported from FwLo's RustSekiro (`src/enemy.rs`, `wolf_sounds`); the
    /// radii (listener vfunc +0x50, maker FUN_1409efb80) are our capsule radii.
    pub fn hears(&self, radius: f32, distance: f32, radii: f32) -> bool {
        let reach = self.rate * radius - self.cut;
        reach > 0.0 && distance - radii < reach + self.ear
    }
}

/// Our scheduling policy, not a recovered engine LOD distance.
/// Activation/deactivation still comes from NpcParam (100 m for a zero border).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lod {
    Dormant,
    Reduced,
    Full,
}

impl Lod {
    pub fn for_distance(distance: f32, activation: f32, engaged: bool) -> Self {
        if engaged {
            Self::Full
        } else if distance > activation {
            Self::Dormant
        } else if distance <= 20.0 {
            Self::Full
        } else {
            Self::Reduced
        }
    }

    /// Distant unaware actors run at 10 Hz; wake immediately without a catch-up spike.
    pub fn step_dt(self, accrued: &mut f32, dt: f32) -> Option<f32> {
        match self {
            Self::Dormant => {
                *accrued = 0.0;
                None
            }
            Self::Full => {
                *accrued = 0.0;
                Some(dt)
            }
            Self::Reduced => {
                *accrued += dt;
                if *accrued + 1e-6 >= 0.1 {
                    Some(std::mem::take(accrued))
                } else {
                    None
                }
            }
        }
    }
}

pub struct Foe {
    pub part: String,
    pub entity: i32,
    /// Own Character, Combatant, brain, contact history, projectiles and paired-throw state.
    pub duel: Box<Duel>,
    pub report: DuelReport,
    pub lod: Lod,
    pub clock: f32,
    pub previous: ([f32; 3], f32),
    pub can_deathblow: bool,
    pub stepped: bool,
    initial_patrol: Option<crate::lua_ai::patrol::Patrol>,
    activation: f32,
    deactivation: f32,
    memory: f32,
    accrued: f32,
    search_left: f32,
    search_rng: crate::brain::sampler::Rng,
    deathblow_range_sq: f32,
    sees: bool,
}

impl Foe {
    pub fn active(&self) -> bool {
        self.lod != Lod::Dormant
    }
    pub fn engaged(&self) -> bool {
        self.memory > 0.0 || self.duel.enemy_damaged
    }
}

pub struct Population {
    pub foes: Vec<Foe>,
    pub selected: Option<usize>,
    locked: Option<usize>,
    world: Option<Arc<crate::collision::CollisionWorld>>,
    pub registry: Option<Arc<crate::registry::Registry>>,
    pub excluded: Vec<Excluded>,
    /// AiSoundParam by id, for the foes' hearing.
    pub sounds: std::collections::HashMap<i32, AiSound>,
}

#[derive(Debug, serde::Serialize)]
pub struct Excluded {
    pub part: String,
    pub chr: String,
    pub entity: i32,
    pub reason: String,
}

pub type PlacedFoes = Vec<(EnemySpec, crate::registry::Placement)>;
pub type StartingPlacements = (PlacedFoes, Vec<Excluded>);

impl Population {
    pub fn load_map(
        cache: &Path,
        map: &str,
        world: Arc<crate::collision::CollisionWorld>,
        wolf: [f32; 3],
        story: Option<&crate::world_events::WorldState>,
    ) -> Result<Self, String> {
        let registry = crate::registry::Registry::load(cache)?;
        let msb = Self::read_msb(cache, map)?;
        let (placements, excluded) = Self::starting_with(cache, map, story, &registry, &msb)?;
        let mut p = Self::load(cache, placements).map_err(|e| e.to_string())?;
        p.excluded = excluded;
        let navmesh =
            crate::navmesh::NavMesh::load_map(cache, map, msb.map_offsets.first())?.map(Arc::new);
        for foe in &mut p.foes {
            let part = msb.parts.iter().find(|part| part.name == foe.part).unwrap();
            if let Some(ai) = &mut foe.duel.ai {
                ai.state.patrol = crate::lua_ai::patrol::Patrol::from_msb(&msb, part);
                foe.initial_patrol.clone_from(&ai.state.patrol);
                ai.state.map_regions = Some(crate::lua_ai::regions::MapRegions::new(cache, map));
            }
            foe.duel.enemy.ground = Box::new(crate::collision::EnemyGround(world.clone()));
            foe.duel.enemy.navmesh.clone_from(&navmesh);
            if let Some(row) = registry.npc_row(foe.duel.spec.npc_param) {
                let border = row["activateBorderDist"].as_f64().unwrap_or(0.0) as f32;
                // Zero delegates to an unrecovered engine default: use a documented fallback.
                foe.activation = if border > 0.0 { border } else { 100.0 };
                foe.deactivation =
                    foe.activation + row["deactivateIntervalDist"].as_f64().unwrap_or(0.0) as f32;
            }
            foe.lod = Lod::for_distance(
                glam::Vec3::from_array(wolf)
                    .distance(glam::Vec3::from_array(foe.duel.enemy.body.position)),
                foe.activation,
                false,
            );
        }
        p.world = Some(world);
        p.registry = Some(registry);
        p.sounds = crate::registry::rows(cache, "AiSoundParam")?
            .into_iter()
            .map(|(id, row)| {
                let flag = |name: &str| row[name].as_i64() == Some(1);
                let sound = AiSound {
                    radius: row["radius"].as_f64().unwrap_or(0.0) as f32,
                    rank: row["rank"].as_i64().unwrap_or(0) as i32,
                    scaled: flag("bSpEffectEnable"),
                    oppose: flag("opposeTarget"),
                };
                (id, sound)
            })
            .collect();
        Ok(p)
    }

    /// Use the visibility interpreter's entity/group writes before allocating any actor.
    pub fn starting_placements(
        cache: &Path,
        map: &str,
        story: Option<&crate::world_events::WorldState>,
    ) -> Result<StartingPlacements, String> {
        let registry = crate::registry::Registry::load(cache)?;
        let msb = Self::read_msb(cache, map)?;
        Self::starting_with(cache, map, story, &registry, &msb)
    }

    fn starting_with(
        cache: &Path,
        map: &str,
        story: Option<&crate::world_events::WorldState>,
        registry: &crate::registry::Registry,
        msb: &sekiro_formats::msb::Msb,
    ) -> Result<StartingPlacements, String> {
        let placements = Self::map_with(map, registry, msb)?;
        let layout: serde_json::Value = serde_json::from_slice(
            &std::fs::read(cache.join(format!("maps/{map}/layout.json")))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let parts: std::collections::HashMap<_, _> = story
            .into_iter()
            .flat_map(|s| s.parts(&layout))
            .map(|p| (p.name.clone(), p))
            .collect();
        let mut kept = Vec::new();
        let mut excluded = Vec::new();
        for (spec, spot) in placements {
            let team = registry
                .npc_row(spec.npc_param)
                .and_then(|row| row["teamType"].as_i64())
                .unwrap_or(0);
            let reason = if let Some(part) = parts.get(&spot.part).filter(|p| !p.enabled) {
                Some(part.source.as_ref().map_or_else(
                    || "disabled by map startup".into(),
                    |s| {
                        format!(
                            "disabled by event {} slot {} target {}",
                            s.event, s.slot, s.target
                        )
                    },
                ))
            } else {
                crate::arena::population_exclusion(&spec.chr, map, team, |flag| {
                    story.is_some_and(|s| s.flags.contains(&flag))
                })
            };
            if let Some(reason) = reason {
                excluded.push(Excluded {
                    part: spot.part,
                    chr: spec.chr,
                    entity: spot.entity,
                    reason,
                });
            } else {
                kept.push((spec, spot));
            }
        }
        Ok((kept, excluded))
    }
    /// Exact MSB Enemy parts, excluding Wolf, object models and reviewed boss rows.
    pub fn map_placements(
        cache: &Path,
        map: &str,
    ) -> Result<Vec<(EnemySpec, crate::registry::Placement)>, String> {
        let registry = crate::registry::Registry::load(cache)?;
        let msb = Self::read_msb(cache, map)?;
        Self::map_with(map, &registry, &msb)
    }

    fn read_msb(cache: &Path, map: &str) -> Result<sekiro_formats::msb::Msb, String> {
        let path = cache.join(format!("raw/map/MapStudio/{map}.msb"));
        sekiro_formats::msb::parse(
            &std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| e.to_string())
    }

    fn map_with(
        map: &str,
        registry: &crate::registry::Registry,
        msb: &sekiro_formats::msb::Msb,
    ) -> Result<PlacedFoes, String> {
        use sekiro_formats::msb::PartKind;
        let mut placements = Vec::new();
        for part in &msb.parts {
            if part.kind != PartKind::Enemy {
                continue;
            }
            let Some(model) = usize::try_from(part.model_index)
                .ok()
                .and_then(|i| msb.models.get(i))
            else {
                continue;
            };
            if !model.name.starts_with('c') || model.name == "c0000" {
                continue;
            }
            let spot = registry
                .placed
                .get(&model.name)
                .and_then(|ps| ps.iter().find(|p| p.map == map && p.part == part.name))
                .ok_or_else(|| format!("{map}: missing registry placement {}", part.name))?;
            let mut spec = registry.placement_spec(&model.name, spot)?;
            if crate::arena::boss_events(&spec).is_some() {
                continue;
            }
            spec.starts_in_battle = false;
            placements.push((spec, spot.clone()));
        }
        Ok(placements)
    }

    pub fn load(
        cache: &Path,
        placements: Vec<(EnemySpec, crate::registry::Placement)>,
    ) -> Result<Self, LoadError> {
        let mut foes = Vec::new();
        let mut player = None;
        for (i, (spec, spot)) in placements.into_iter().enumerate() {
            let mut duel = Duel::load_with_player(cache, 0.0, spec, player.take())?;
            duel.events = None;
            duel.enemy.body.position = spot.position;
            duel.enemy.snap_yaw(spot.yaw);
            duel.spawn = Some([placement(&duel.player), placement(&duel.enemy)]);
            duel.population_target = Some(false);
            player = duel.player.take();
            duel.fighters[0] = Combatant::default();
            if let Some(ai) = &mut duel.ai {
                ai.state.rng ^= (i as u64 + 1).wrapping_mul(0x85eb_ca6b);
            }
            foes.push(Foe {
                previous: (spot.position, spot.yaw),
                entity: spot.entity,
                part: spot.part,
                deathblow_range_sq: duel.throws.as_ref().map_or(0.0, |t| {
                    t.table
                        .rows
                        .iter()
                        .map(|r| r.dist)
                        .fold(0.0f32, f32::max)
                        .powi(2)
                }),
                duel: Box::new(duel),
                report: Default::default(),
                lod: Lod::Full,
                clock: 0.0,
                can_deathblow: false,
                stepped: false,
                initial_patrol: None,
                activation: 100.0,
                deactivation: 100.0,
                memory: 0.0,
                accrued: 0.0,
                search_left: 0.0,
                search_rng: crate::brain::sampler::Rng::from_state(
                    (i as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                ),
                sees: false,
            });
        }
        Ok(Self {
            foes,
            selected: None,
            locked: None,
            world: None,
            registry: None,
            excluded: Vec::new(),
            sounds: Default::default(),
        })
    }
}

impl Duel {
    /// Every foe back at its start, alive and unaware, as after Wolf's death or a rest.
    pub(super) fn respawn_foes(&mut self, population: &mut Population) {
        for foe in &mut population.foes {
            self.swap_screen_player(&mut foe.duel);
            if let Err(error) = foe.duel.reset() {
                eprintln!("population {} respawn failed: {error}", foe.part);
            }
            self.swap_screen_player(&mut foe.duel);
            if let Some(ai) = &mut foe.duel.ai {
                ai.state.patrol.clone_from(&foe.initial_patrol);
            }
            foe.clock = 0.0;
            foe.memory = 0.0;
            foe.accrued = 0.0;
            foe.search_left = 0.0;
            foe.sees = false;
            if let Some(ai) = &mut foe.duel.ai {
                ai.state.sound = None;
                ai.state.sound_new = false;
                ai.state.ai_mark = None;
            }
        }
        population.locked = None;
    }

    pub fn set_population(&mut self, population: Population) {
        self.events = None;
        self.boss_brain = None;
        self.enemy_present = false;
        self.enemy_active = false;
        self.population = Some(population);
    }

    pub(super) fn step_population(&mut self, input: &InputFrame, dt: f32) -> DuelReport {
        let _timer = crate::perf::Scope::start("sim population");
        if self.spawn.is_none() {
            self.spawn = Some([placement(&self.player), placement(&self.enemy)]);
        }
        let mut population = self.population.take().unwrap();
        let mut respawned = false;
        if let Some(left) = self.respawn_in.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                if let Err(error) = self.reset() {
                    eprintln!("population Wolf respawn failed: {error}");
                } else {
                    // A respawn after death restocks the goods as Rest does (docs/FIDELITY.md),
                    // as the solo step's does.
                    self.restock_kit();
                    self.respawn_foes(&mut population);
                    respawned = true;
                }
            }
        }
        // A rest at an idol.
        if std::mem::take(&mut self.idols.revive) {
            self.respawn_foes(&mut population);
        }
        let wolf = glam::Vec3::from_array(self.player.body.position);
        let distance =
            |foe: &Foe| glam::Vec3::from_array(foe.duel.enemy.body.position).distance_squared(wolf);
        // The AI sounds Wolf's clips make this step (TAE 237), their radius scaled by his
        // SpEffects' hearingSearchEnemyRate for bSpEffectEnable rows (Covert B 150010: 0.5).
        let noise: Vec<(i32, f32, i32)> = {
            let tae = &self.player.tae;
            let runtime = &self.player.tae_runtime;
            let rate: f32 = runtime
                .active_sp_effects()
                .iter()
                .filter_map(|id| tae.sp_effects.get(id))
                .map(|e| e.sound_rate)
                .product();
            let mut ids = runtime.frame.ai_sounds.clone();
            ids.sort_unstable();
            ids.dedup();
            ids.into_iter()
                .filter_map(|id| {
                    let s = population.sounds.get(&id).filter(|s| s.oppose)?;
                    Some((id, s.radius * if s.scaled { rate } else { 1.0 }, s.rank))
                })
                .collect()
        };
        if let Some(world) = &population.world {
            let locked = population.locked;
            for (i, foe) in population.foes.iter_mut().enumerate() {
                let range = if foe.active() {
                    foe.deactivation
                } else {
                    foe.activation
                };
                let d = &mut foe.duel;
                let engaged = foe.memory > 0.0
                    || d.enemy_damaged
                    || d.deathblow.is_some()
                    || d.enemy_throw.is_some()
                    || locked == Some(i);
                let distance = glam::Vec3::from_array(d.enemy.body.position).distance(wolf);
                foe.lod = Lod::for_distance(distance, range, engaged);
                if foe.lod == Lod::Dormant {
                    continue;
                }
                let Some(ai) = &mut d.ai else {
                    d.enemy_active = false;
                    continue;
                };
                let feet = glam::Vec3::from_array(d.enemy.body.position);
                // Hearing, every frame (FwLo's enemy.rs: sounds, then sight and the state).
                let runtime = &d.enemy.tae_runtime;
                let think = |name: &str| ai.state.think.fields.get(name).copied().unwrap_or(0.0);
                let ear = Hearing {
                    ear: think("ear_dist") as f32,
                    cut: think("ear_soundcut_dist") as f32,
                    rate: runtime
                        .active_sp_effects()
                        .iter()
                        .filter_map(|id| d.enemy.tae.sp_effects.get(id))
                        .map(|e| 1.0 - e.hearing_cut.max(0.0) / 100.0)
                        .product::<f32>()
                        .clamp(0.0, 1.0),
                };
                let radii = d.fighters[0].radius + d.fighters[1].radius;
                let heard = noise
                    .iter()
                    .filter(|(_, radius, _)| ear.hears(*radius, wolf.distance(feet), radii))
                    .max_by_key(|(_, _, rank)| *rank);
                ai.state.age_sound(dt);
                if let Some(&(id, _, rank)) = heard {
                    ai.state.hear(wolf.to_array(), id, rank);
                }
                let field =
                    |name: &str| ai.state.think.fields.get(name).copied().unwrap_or(0.0) as f32;
                let eye = feet + glam::Vec3::Y * d.fighters[1].height * 0.8;
                let delta = wolf + glam::Vec3::Y - eye;
                let horizontal = delta.x.hypot(delta.z);
                foe.search_left -= dt;
                if foe.search_left <= 0.0 {
                    // FwLo, stealth.rs:333-335 / exe FUN_140623ca0: 0.2 + rand*0.2 s.
                    // Separate sensory RNG keeps the Lua battle RNG sequence unchanged.
                    foe.search_left = 0.2 + foe.search_rng.next_f32() * 0.2;
                    // With Wolf as its target (FIND/BATTLE) the exe senses with the perceive
                    // set instead of the normal one (FUN_14061bb70 on sight context +0x56).
                    let set = if foe.memory > 0.0 {
                        "perceive"
                    } else {
                        "normal"
                    };
                    let cone = SightCone::from_think(set, field);
                    foe.sees = cone.contains(feet, d.enemy.body.yaw, wolf, d.fighters[0].radius)
                        && world
                            .raycast(eye, delta.normalize_or_zero(), delta.length())
                            .is_none();
                }
                if foe.sees || d.enemy_damaged || horizontal < field("nose_dist") {
                    foe.memory = field("SightTargetForgetTime").max(dt);
                    if !ai.state.battle {
                        ai.state.replan = true;
                    }
                } else {
                    foe.memory = (foe.memory - dt).max(0.0);
                }
                let aware = foe.memory > 0.0;
                // Unseen, a held sound makes CAUTION, nothing NONE (FUN_1406208c0 via FwLo's
                // `Targeting::update`); a change replans unless goalAction_ToCaution is 0
                // (FUN_14061f500).
                if !aware {
                    let state = i32::from(ai.state.sound.is_some());
                    if ai.state.battle || ai.state.target_state != state {
                        ai.state.battle = false;
                        ai.state.target_state = state;
                        ai.state.replan |= state == 0 || field("goalAction_ToCaution") != 0.0;
                    }
                }
                d.population_target = Some(aware);
                if aware {
                    foe.lod = Lod::Full;
                }
            }
        }
        let paired = population
            .foes
            .iter()
            .position(|f| f.duel.deathblow.is_some() || f.duel.enemy_throw.is_some());
        let alive = |f: &Foe| f.active() && !f.duel.rules.fighters[1].dead();
        if population
            .locked
            .is_some_and(|i| !alive(&population.foes[i]))
        {
            population.locked = None;
            self.release_lock();
        }
        let nearest = population
            .foes
            .iter()
            .enumerate()
            .filter(|(_, f)| alive(f))
            .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
            .map(|(i, _)| i);
        let mut deathblow = None;
        let mut deathblow_distance = f32::INFINITY;
        if paired.is_none() {
            // The existing ThrowParam selection remains the authority for range and facing.
            for (i, foe) in population.foes.iter_mut().enumerate() {
                foe.can_deathblow = false;
                if !alive(foe) {
                    continue;
                }
                let delta = glam::Vec3::from_array(foe.duel.enemy.body.position) - wolf;
                if delta.x * delta.x + delta.z * delta.z > foe.deathblow_range_sq {
                    continue;
                }
                self.swap_screen_player(&mut foe.duel);
                let available = foe.duel.deathblow_available();
                foe.can_deathblow = available;
                self.swap_screen_player(&mut foe.duel);
                if available && distance(foe) < deathblow_distance {
                    deathblow = Some(i);
                    deathblow_distance = distance(foe);
                }
            }
        }
        let selected = paired
            .or(if input.buttons.attack && !self.attack_button {
                deathblow
            } else {
                None
            })
            .or(population.locked)
            .or(nearest);
        population.selected = selected;
        self.player.aim_candidates = population
            .foes
            .iter()
            .filter(|f| alive(f))
            .map(|f| f.duel.enemy.body.position)
            .collect();
        let mut result = DuelReport::default();
        let passive = InputFrame::default();
        for foe in &mut population.foes {
            foe.stepped = false;
            foe.can_deathblow &= paired.is_none();
        }
        if let Some(i) = selected
            .filter(|&i| input.buttons.lock_on && distance(&population.foes[i]) <= 30.0 * 30.0)
        {
            population.foes[i].lod = Lod::Full;
        }
        let player_foe = selected.filter(|&i| population.foes[i].lod == Lod::Full);
        if player_foe.is_none() {
            // Wolf still advances at the fixed rate even when every visible foe is reduced.
            result = self.step(input, dt);
        }
        let mut pair_held = paired.is_some();
        for i in player_foe
            .into_iter()
            .chain((0..population.foes.len()).filter(|&i| Some(i) != player_foe))
        {
            let foe = &mut population.foes[i];
            let foe_dt = foe.lod.step_dt(&mut foe.accrued, dt);
            if !foe.active() || foe_dt.is_none() || (Some(i) != player_foe && pair_held) {
                foe.report.enemy.advanced = 0.0;
                foe.report.enemy.clip_started = false;
                continue;
            }
            let foe_dt = foe_dt.unwrap();
            foe.previous = (foe.duel.enemy.body.position, foe.duel.enemy.visual_yaw());
            if population.world.is_none() {
                foe.duel.population_target = Some(true);
            }
            let report = self.step_kernel(
                &mut foe.duel,
                if Some(i) == player_foe {
                    input
                } else {
                    &passive
                },
                foe_dt,
                Some(i) == player_foe,
            );
            pair_held |= foe.duel.deathblow.is_some() || foe.duel.enemy_throw.is_some();
            foe.clock += report.enemy.advanced;
            foe.stepped = true;
            foe.report = report;
            if Some(i) == player_foe {
                result = foe.report.clone();
                result.hits.clear();
            } else if Some(i) == selected {
                result.enemy = foe.report.enemy.clone();
            }
            for (mut hit, res) in foe.report.hits.iter().cloned() {
                if hit.attacker == 1 {
                    hit.attacker = i + 1;
                }
                if hit.defender == 1 {
                    hit.defender = i + 1;
                }
                result.hits.push((hit, res));
            }
            if let Some(mut warning) = foe.report.perilous {
                warning.attacker = i + 1;
                result.perilous = Some(warning);
            }
        }
        population.locked = selected.filter(|_| self.player.lock_target.is_some());
        self.last_steps[0] = result.player.clone();
        result.respawned = respawned;
        if self.rules.fighters[0].dead() && self.respawn_in.is_none() {
            self.respawn_in = Some(RESPAWN_DELAY);
        }
        self.population = Some(population);
        result
    }
}
