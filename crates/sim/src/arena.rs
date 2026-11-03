//! Boss arenas: where the game fights a boss, as data.
//!
//! An [`ArenaSpec`] names a map, the MSB enemy part the game places the boss as, and a radius.
//! [`ArenaLayout::resolve`] reads the map's `layout.json` (written by `sekiro-extract map`) and
//! returns the boss's MSB transform and where Wolf starts; [`crop`] keeps the collision within
//! the radius. Everything is in Bevy space (FromSoftware x mirrored, see `docs/FORMATS.md`).
//!
//! Adding a boss is one new row in [`ARENAS`].

/// Render-only map scenery radius; collision remains ArenaSpec::radius.
pub const ARENA_VISUAL_RADIUS: f32 = 1500.0;

use std::path::Path;
use std::sync::Arc;

use glam::{Vec2, Vec3};
use sekiro_formats::hknp::CollisionMesh;
use serde_json::Value;

use crate::body::Ground;
use crate::collision::CollisionWorld;
use crate::duel::Duel;
use crate::enemy::EnemySpec;
use crate::events::EventEntry;
use crate::fogwall::FogWall;

#[path = "arenas/depths_palace.rs"]
pub mod depths_palace;

#[path = "arenas/scene.rs"]
pub mod scene;

/// One boss arena.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArenaSpec {
    /// Command-line name (`--arena owl`).
    pub name: &'static str,
    /// Map id, exported to `cache/maps/<map>`.
    pub map: &'static str,
    /// The boss's MSB enemy part: his position and facing.
    pub boss_part: &'static str,
    /// The enemy (see [`EnemySpec::by_name`]).
    pub enemy: &'static str,
    /// Horizontal collision and object radius in metres around the boss.
    /// Render-only map pieces use ARENA_VISUAL_RADIUS in the game crate.
    pub radius: f32,
    /// Where Wolf starts: an MSB player start part, or `None` for [`FALLBACK_DISTANCE`]
    /// metres in front of the boss.
    /// `region:<id>` selects an authored cinematic warp in the original local MSB.
    pub player_start: Option<&'static str>,
    /// Map piece models not shown in this fight: event-state alternatives that the map's
    /// event script keeps hidden during it, by model name prefix.
    pub hidden_models: &'static [&'static str],
    /// The time-advancing bosses ([`crate::story::TIME_BOSS_FLAGS`]) already beaten when the
    /// game stages this fight: they set the story time of day ([`ArenaSpec::story_time`]).
    pub beaten: &'static [i32],
}

/// The map events that run a boss fight (see [`crate::events`]): which events of the map's
/// script to run, from where, and the boss's entity id in them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BossEvents {
    /// The map whose `event/<map>.emevd` holds the events.
    pub map: &'static str,
    /// The boss's MSB entity id, as the events name him.
    pub boss_entity: i32,
    /// Event flags that are on when the fight starts (the story state the fight needs).
    pub flags: &'static [i32],
    pub events: &'static [EventEntry],
    /// Events of the map's script that start the fight's fog wall (common event 20005820, see
    /// [`crate::fogwall`]); we do not run them, only read what they place.
    pub fog_events: &'static [i64],
    /// The `SetLighting` time of day (1 morning to 4 night) the fight's events switch to when
    /// the fight reaches this boss, when they do ([`crate::story`]).
    /// Variant 0 also represents common event 100's fixed prologue/memory environment.
    pub lighting: Option<i32>,
}

/// The Great Shinobi Owl on the castle top (`docs/OWL.md`, "Phase 2"). From
/// `event/m11_01_00_00.emevd`:
///
/// - 11115860 sets the fight up after the intro cutscene; we join it after its `Label6`:
///   immortality on, the overhead HP bar off, the boss bar (NpcName 905060) after 2 s;
/// - 11115870 is phase 2: when his first deathblow's get-up (`a000_012000`) raises event
///   message 20, it hides the boss bar, sends HKS command 506000 (his heat-up `W_Event20005`)
///   and gives him SpEffect 200051 ("behaviour pattern change 1"); after 3 s it waits for
///   message 60 (his smoke escape) or for the heat-up SpEffect 3506080 to end, sets flag
///   11110860 and shows the boss bar again. With 11110860 already set (a retry) it only gives
///   200051;
/// - 11115872 lifts the immortality when his finishing deathblow clip adds 3506050;
/// - 11115850 handles his defeat on message 50 (in the finishing clip) or his death;
/// - 11115873 shows the smoke map effect on message 40.
///
/// Flags 8301 and 9303 are the story state in which the event script starts this fight
/// (11115860 needs both); they open no other branch of the events we run. Flag 11115722 is
/// raised by event 11115721 when Wolf walks into the roof (area 1112712) and cleared by the
/// defeat: it stands the fog wall, whose events (11115861) we only read.
pub const OWL_EVENTS: BossEvents = BossEvents {
    map: "m11_01_00_00",
    boss_entity: 1_110_850,
    flags: &[8301, 9303, 11_115_722],
    events: &[
        EventEntry {
            id: 11_115_860,
            from_label: Some(6),
        },
        EventEntry {
            id: 11_115_870,
            from_label: None,
        },
        EventEntry {
            id: 11_115_872,
            from_label: None,
        },
        EventEntry {
            id: 11_115_850,
            from_label: None,
        },
        EventEntry {
            id: 11_115_873,
            from_label: None,
        },
    ],
    fog_events: &[11_115_861],
    lighting: None,
};

/// The fight events of an enemy, by character id (also used off the arena, on the flat floor).
pub fn boss_events(spec: &EnemySpec) -> Option<&'static BossEvents> {
    if spec.chr == "c5400" && matches!(spec.think, 54030000 | 54030100) {
        return Some(&crate::sword_saint::REMATCH_EVENTS);
    }
    if let Some(events) = crate::arenas_a::identity_events(spec) {
        return Some(events);
    }
    BOSS_EVENT_ROWS
        .iter()
        .find(|(character, row, _)| *character == spec.chr && *row == spec.npc_param)
        .map(|(_, _, events)| events)
}

/// Reviewed first-visit exclusions alongside the boss identity rows.
/// The team allowlist is a start-role policy, not a recovered hostility table.
/// Unsupported event predicates stay blocked; Ministry invasion needs the night flag.
pub enum PopulationExclusion {
    Models(&'static [&'static str], &'static str),
    EnemyTeams(&'static [i64], &'static str),
    RequiresFlag {
        map: &'static str,
        team: i64,
        flag: i32,
        reason: &'static str,
    },
}
pub const POPULATION_EXCLUSIONS: &[PopulationExclusion] = &[
    PopulationExclusion::Models(
        &["c1000", "c1001"],
        "event/controller model without a combat body",
    ),
    PopulationExclusion::EnemyTeams(&[6, 24, 29], "no initial enemy combat role"),
    PopulationExclusion::Models(
        &["c5010"],
        "Great Serpent is a region-scripted setpiece, not a roaming combatant",
    ),
    PopulationExclusion::RequiresFlag {
        map: "m11_00_00_00",
        team: 24,
        flag: 9802,
        reason: "late Ministry invasion role, absent at the Outskirts first visit",
    },
];

pub fn population_exclusion(
    chr: &str,
    map: &str,
    team: i64,
    flag: impl Fn(i32) -> bool,
) -> Option<String> {
    POPULATION_EXCLUSIONS.iter().find_map(|rule| match rule {
        PopulationExclusion::Models(models, reason) if models.contains(&chr) => {
            Some((*reason).into())
        }
        PopulationExclusion::EnemyTeams(teams, reason) if !teams.contains(&team) => {
            Some(format!("{reason} (NpcParam teamType {team})"))
        }
        PopulationExclusion::RequiresFlag {
            map: wanted,
            team: wanted_team,
            flag: required,
            reason,
        } if map == *wanted && team == *wanted_team && !flag(*required) => Some((*reason).into()),
        _ => None,
    })
}
/// Reviewed map-event rows, keyed by boss identity instead of dispatch code.
pub const BOSS_EVENT_ROWS: &[(&str, i32, BossEvents)] = &[
    ("c5060", 50_600_000, OWL_EVENTS),
    (
        "c7100",
        71_001_000,
        BossEvents {
            map: "m11_01_00_00",
            boss_entity: 1_110_800,
            flags: &[],
            events: &[
                EventEntry {
                    id: 11_115_810,
                    from_label: Some(3),
                },
                EventEntry {
                    id: 11_115_820,
                    from_label: None,
                },
            ],
            fog_events: &[11_115_811],
            lighting: None,
        },
    ),
    (
        "c7110",
        71_100_000,
        BossEvents {
            map: "m11_01_00_00",
            boss_entity: 1_110_801,
            // 11115801 is the first phase's fog wall, still up in the second (event 11115810
            // sets it, 11115800 ends it with flag 9403).
            flags: &[11_115_802, 11_115_801],
            events: &[
                EventEntry {
                    id: 11_115_800,
                    from_label: None,
                },
                EventEntry {
                    id: 11_115_831,
                    from_label: None,
                },
            ],
            fog_events: &[11_115_811],
            // Event 11115820 plays the hand-over cutscene under `SetLighting(3)`: the storm.
            lighting: Some(3),
        },
    ),
    ("c5090", 50_900_000, crate::arenas_a::BUTTERFLY),
    ("c5090", 50_900_001, crate::arenas_a::BUTTERFLY_SECOND),
    ("c5060", 50_601_010, crate::arenas_a::FATHER),
    ("c5060", 50_601_010, crate::arenas_a::INNER_FATHER),
    ("c1000", 12_600_050, crate::arenas_a::MONKEYS),
    ("c7100", 71_000_000, crate::arena_c::RESERVOIR_GENICHIRO),
    ("c7110", 71_101_000, crate::arena_c::RESERVOIR_TOMOE),
    ("c5400", 54_000_000, crate::arena_c::SWORD_SAINT),
    ("c5100", 51_000_000, crate::arena_c::GUARDIAN_APE),
    ("c5100", 51_000_100, crate::arena_c::HEADLESS_APE),
    ("c5000", 50_001_000, depths_palace::MIBU_EVENTS),
    ("c5000", 50_000_000, depths_palace::TRUE_MONK_EVENTS),
    ("c5300", 53_000_000, depths_palace::OLD_DRAGONS_EVENTS),
    ("c5200", 52_000_000, depths_palace::DIVINE_DRAGON_EVENTS),
    ("c5080", 50_800_000, crate::arena_b::GYOUBU),
    ("c7020", 70_200_000, crate::arena_b::DEMON),
    ("c7400", 74_000_010, crate::arena_b::EMMA),
    ("c5430", 54_300_000, crate::arena_b::ISSHIN),
];

/// Every arena. The player start choices are documented in `docs/OWL.md` ("The castle roof").
pub const ARENAS: &[ArenaSpec] = &[
    ArenaSpec {
        name: "owl",
        map: "m11_01_00_00",
        boss_part: "c5060_0000",
        enemy: "owl",
        radius: 60.0,
        // On the same roof, 18.9 m from the Owl; the other start on the roof (`c0000_0010`) is 1 m
        // from him.
        player_start: Some("c0000_0003"),
        // The burnt floor of the castle top (entity 1114551, group 1117551) overlaps the tatami
        // floor m015070 (1114550, group 1117550). Event 11115943 activates the burnt one only once
        // flag 11115921 is set (after cutscene 11010080, which starts the later fight with c5430 on
        // this roof) and the tatami again with 11110100; the Owl fight shows the tatami.
        hidden_models: &["m015072"],
        // The castle-roof progression (`--progression castle-roof`): Genichiro and the three
        // other time-advancing bosses (Memories for attack power 6) are down. Event 820 then
        // raises flags 9800 and 9801 and event 100 shows the evening sky and light.
        beaten: &[9303, 9304, 9305, 9306],
    },
    ArenaSpec {
        name: "genichiro",
        map: "m11_01_00_00",
        boss_part: "c7100_0000",
        enemy: "c7100:71001000",
        radius: 60.0,
        player_start: Some("c0000_0003"),
        hidden_models: &["m015072"],
        // His first visit: no time-advancing boss is down, so no time flag is on and event 100
        // shows the morning (environment variant 1).
        beaten: &[],
    },
    crate::arenas_a::HIRATA_HALL,
    crate::arenas_a::HIRATA_FATHER,
    crate::arenas_a::HIRATA_INNER_FATHER,
    crate::arenas_a::SENPOU_SCREENS,
    crate::arena_c::SILVERGRASS,
    crate::arena_c::SILVERGRASS_GENICHIRO,
    crate::arena_c::SILVERGRASS_ISSHIN,
    crate::sword_saint::REMATCH_ARENA,
    crate::arena_c::WATERING_HOLE,
    crate::arena_c::APES_BURROW,
    depths_palace::MIBU,
    depths_palace::TRUE_MONK,
    depths_palace::SANCTUARY,
    crate::arena_b::GYOUBU_ARENA,
    crate::arena_b::DEMON_ARENA,
    crate::arena_b::SHURA_ARENA,
];

/// Distance in front of the boss where Wolf starts when the arena names no player start.
pub const FALLBACK_DISTANCE: f32 = 6.0;

impl ArenaSpec {
    pub fn by_name(name: &str) -> Result<&'static Self, String> {
        let name = name.to_ascii_lowercase();
        ARENAS.iter().find(|a| a.name == name).ok_or_else(|| {
            let known: Vec<&str> = ARENAS.iter().map(|a| a.name).collect();
            format!("unknown arena {name}: known are {}", known.join(", "))
        })
    }

    pub fn enemy_spec(&self) -> EnemySpec {
        if let Some(enemy) = crate::arenas_a::arena_enemy(self) {
            return enemy;
        }
        EnemySpec::by_name(self.enemy).expect("arena enemies are known enemy names")
    }

    /// The story time of day this fight is staged in: the one the story flags give
    /// ([`crate::story::StoryTime::from_flags`]), unless the fight's events switch the lighting
    /// when `phase` (the enemy actually fought) begins.
    pub fn story_time(&self, phase: &EnemySpec) -> crate::story::StoryTime {
        boss_events(phase)
            .and_then(|e| e.lighting)
            .and_then(crate::story::StoryTime::from_variant)
            .unwrap_or_else(|| crate::story::StoryTime::from_flags(|f| self.beaten.contains(&f)))
    }
}

/// A position on the map and a Bevy-space yaw in radians (yaw 0 faces -Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    pub position: Vec3,
    pub yaw: f32,
}

/// Where an arena puts its two characters.
#[derive(Debug, Clone, PartialEq)]
pub struct ArenaLayout {
    /// The boss at his MSB part's transform.
    pub boss: Spot,
    /// Wolf, facing the boss.
    pub wolf: Spot,
    /// Where Wolf's spot came from, for logs.
    pub wolf_source: String,
}

impl ArenaLayout {
    /// Reads the boss's part and Wolf's start from `layout.json`.
    pub fn resolve(spec: &ArenaSpec, layout: &Value) -> Result<Self, String> {
        let row = |key: &str, name: &str| -> Result<Spot, String> {
            let r = layout[key]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["name"].as_str() == Some(name))
                .ok_or_else(|| format!("{}: no part {name} in {key}", spec.map))?;
            let t = &r["translation"];
            let f = |i: usize| t[i].as_f64().map(|v| v as f32);
            let position = match (f(0), f(1), f(2)) {
                (Some(x), Some(y), Some(z)) => Vec3::new(x, y, z),
                _ => return Err(format!("{name}: no translation")),
            };
            let yaw = r["yaw_degrees"].as_f64().unwrap_or(0.0) as f32;
            Ok(Spot {
                position,
                yaw: yaw.to_radians(),
            })
        };
        let boss = row("enemies", spec.boss_part)?;
        let (position, wolf_source) = match spec.player_start {
            Some(start) => (
                row("players", start)?.position,
                format!("player start {start}"),
            ),
            None => {
                let ahead = forward(boss.yaw) * FALLBACK_DISTANCE;
                (
                    boss.position + ahead,
                    format!("{FALLBACK_DISTANCE} m in front of {}", spec.boss_part),
                )
            }
        };
        // Facing the boss (the MSB yaw of a player start is the way a warp leaves you facing,
        // not necessarily toward the fight).
        let wolf = Spot {
            position,
            yaw: yaw_toward(position, boss.position),
        };
        Ok(Self {
            boss,
            wolf,
            wolf_source,
        })
    }

    /// [`ArenaLayout::resolve`] from `<dir>/layout.json`.
    pub fn load(spec: &ArenaSpec, dir: &Path) -> Result<Self, String> {
        let path = dir.join("layout.json");
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let json: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let mut layout = depths_palace::layout(&dir.join("../.."), spec, &json)?;
        crate::arena_c::story_start(&dir.join("../.."), spec, &mut layout)?;
        Ok(layout)
    }

    /// The arena's centre: the boss's start.
    pub fn center(&self) -> Vec3 {
        self.boss.position
    }
}

/// How far below its start a character may fall before it is put back (fall death is not
/// modelled yet; the castle roof's eaves are about 6 m above the next roof down).
pub const FALL_RESPAWN_DEPTH: f32 = 30.0;

/// A fog wall placed on the map: the event's wall and its asset's MSB transform and local box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogWallPlace {
    pub wall: FogWall,
    /// Bevy space, like every part of the layout.
    pub position: Vec3,
    pub rotation: glam::Quat,
    /// The asset's box in its own frame (from its model bounds).
    pub local_min: Vec3,
    pub local_max: Vec3,
}

/// An arena ready to fight on: where both stand and the collision they stand on.
pub struct Arena {
    pub spec: &'static ArenaSpec,
    pub layout: ArenaLayout,
    pub world: Arc<CollisionWorld>,
    /// The fight's fog walls; their collision is the world's gate, open while one stands.
    pub fog_walls: Vec<FogWallPlace>,
}

/// The thinnest a fog wall's blocker is made: the asset is a flat plane, and a body must not
/// step through it between two ticks.
const FOG_WALL_THICKNESS: f32 = 0.6;

impl Arena {
    /// Reads `cache/maps/<map>`: the layout, and the collision within the arena's radius (or
    /// all of it with `full_map`). The fight's fog walls (events read with
    /// [`crate::fogwall`]) are held back as the world's gate: the arena's boundary collision
    /// and a box on each wall's asset, which block only while the walls stand.
    pub fn load(cache: &Path, spec: &'static ArenaSpec, full_map: bool) -> Result<Self, String> {
        let dir = cache.join("maps").join(spec.map);
        let path = dir.join("layout.json");
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let json: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let mut layout = depths_palace::layout(cache, spec, &json)?;
        crate::arena_c::story_start(cache, spec, &mut layout)?;
        crate::arena_b::story_start(cache, spec, &mut layout)?;
        let fog_walls = match boss_events(&spec.enemy_spec()) {
            Some(events) if !events.fog_events.is_empty() => {
                let walls = crate::fogwall::load(cache, events.map, events.fog_events)?;
                fog_wall_places(&walls, &json)?
            }
            _ => Vec::new(),
        };
        // The collision characters and the camera meet, as in explore mode
        // ([`crate::lo_collision`]): `rows` are the layout's collision rows with their triangle
        // ranges in that mesh, so every step below takes a part's own triangles out of it.
        let crate::lo_collision::BlockMesh { mut mesh, rows } =
            crate::lo_collision::read(&dir, &json)?;
        // These steps collapse triangles in place, so every triangle range in `rows` stays
        // valid until the fog walls' split below. The helpers go last: disabled, event and
        // hidden parts are already gone and do not come back as helpers.
        crate::arena_c::filter_collision(spec, &rows, &mut mesh);
        let event_collision = crate::arena_c::take_event_collision(spec, &rows, &mut mesh);
        let hidden = crate::arenas_a::hidden_parts(cache, spec)?;
        crate::arenas_a::remove_hidden_collision(&mut mesh, &rows, |n| hidden.contains(n));
        let mut helpers = crate::hit_filter::strip_helpers(&mut mesh, &rows);
        if !full_map {
            for helper in helpers.meshes_mut() {
                *helper = crop(helper, layout.center(), spec.radius);
            }
            // An event wall stays whole if any of it reaches the arena: `l`'s are a few large
            // triangles, and cropped to those that reach, the Owl roof's lost the side a fall
            // from the roof's edge passes through (#189).
            helpers.triggers.retain(|t| {
                !crop(&t.mesh, layout.center(), spec.radius)
                    .indices
                    .is_empty()
            });
        }
        let mut gate = CollisionMesh::default();
        for place in &fog_walls {
            if let Some(range) = place
                .wall
                .collision_entity
                .and_then(|entity| collision_range(&rows, entity))
            {
                let (rest, part) = split_triangles(&mesh, range);
                mesh = rest;
                gate.extend(&part);
            }
            gate.extend(&place.blocker());
        }
        let mut world = if fog_walls.is_empty() {
            if full_map {
                CollisionWorld::from_mesh(&mesh)
            } else {
                CollisionWorld::from_mesh(&crop(&mesh, layout.center(), spec.radius))
            }
        } else if full_map {
            CollisionWorld::from_mesh_with_gate(&mesh, &gate)
        } else {
            CollisionWorld::from_mesh_with_gate(
                &crop(&mesh, layout.center(), spec.radius),
                &crop(&gate, layout.center(), spec.radius),
            )
        };
        world.add_helpers(&helpers);
        if let Some((part, who)) = event_collision {
            let part = if full_map {
                part
            } else {
                crop(&part, layout.center(), spec.radius)
            };
            world.append_event_part(1_124_860, &part, who, true);
        }
        Ok(Self {
            spec,
            layout,
            world: Arc::new(world),
            fog_walls,
        })
    }

    /// The floor at or just below a spot (its own height when there is none).
    pub fn floor_at(&self, p: Vec3) -> Vec3 {
        let y = Ground::floor(&*self.world, p.x, p.z, p.y + 1.0).unwrap_or(p.y);
        p.with_y(y)
    }

    /// Puts Wolf and the boss on their spots, on the arena's collision, with the fall respawn.
    /// The boss stands on what enemies collide with ([`crate::collision::EnemyGround`]).
    pub fn place(&self, duel: &mut Duel) {
        let enemy = crate::collision::EnemyGround(self.world.clone());
        for (c, spot, ground) in [
            (
                &mut duel.player,
                self.layout.wolf,
                Box::new(self.world.clone()) as Box<dyn Ground>,
            ),
            (&mut duel.enemy, self.layout.boss, Box::new(enemy)),
        ] {
            let p = spot.position;
            let y = ground.floor(p.x, p.z, p.y + 1.0).unwrap_or(p.y);
            c.body.position = p.with_y(y).to_array();
            c.snap_yaw(spot.yaw);
            c.body.fall_top = c.body.position[1];
            c.ground = ground;
        }
        duel.fall_respawn_depth = Some(FALL_RESPAWN_DEPTH);
        crate::demon::on_place(duel);
        if let Some(encounter) = duel.screen_monkeys.as_mut() {
            encounter.place(&self.world);
        }
        if let Some(encounter) = duel.monk_phantoms.as_mut() {
            encounter.place(&self.world);
        }
        if let Some(bird) = duel.father_bird.as_mut() {
            bird.place(&self.world);
        }
        if let Some(encounter) = duel.old_dragons.as_mut() {
            encounter.place(&self.world);
        }
        if let Some(encounter) = duel.butterfly_illusions.as_mut() {
            encounter.place(&self.world);
        }
        if let Some(encounter) = duel.headless_ape.as_mut() {
            encounter.place(&self.world);
        }
        if !self.fog_walls.is_empty() {
            duel.fog_gate = Some(self.world.clone());
        }
    }
}

impl FogWallPlace {
    /// A box over the asset's plane, [`FOG_WALL_THICKNESS`] thick at least, as collision.
    pub fn blocker(&self) -> CollisionMesh {
        let mut min = self.local_min;
        let mut max = self.local_max;
        let thickness = (max.z - min.z).max(FOG_WALL_THICKNESS);
        let mid = (min.z + max.z) * 0.5;
        min.z = mid - thickness * 0.5;
        max.z = mid + thickness * 0.5;
        let corners = [0, 1, 2, 3, 4, 5, 6, 7].map(|i| {
            let local = Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            );
            (self.position + self.rotation * local).to_array()
        });
        const QUADS: [[u32; 4]; 6] = [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
        ];
        let mut mesh = CollisionMesh {
            vertices: corners.to_vec(),
            ..Default::default()
        };
        for [a, b, c, d] in QUADS {
            mesh.indices.extend([a, b, c, a, c, d]);
            mesh.materials.extend([u32::MAX; 2]);
        }
        mesh
    }
}

/// The wall assets in the layout: their transform and model bounds.
fn fog_wall_places(walls: &[FogWall], layout: &Value) -> Result<Vec<FogWallPlace>, String> {
    let vec3 = |v: &Value| -> Option<Vec3> {
        Some(Vec3::new(
            v[0].as_f64()? as f32,
            v[1].as_f64()? as f32,
            v[2].as_f64()? as f32,
        ))
    };
    walls
        .iter()
        .map(|wall| {
            let row = layout["objects"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["entity_id"].as_i64() == Some(i64::from(wall.object_entity)))
                .ok_or_else(|| format!("no object with entity {}", wall.object_entity))?;
            let bounds = &layout["models"][row["model"].as_str().unwrap_or_default()];
            let q = &row["rotation"];
            let rotation = glam::Quat::from_xyzw(
                q[0].as_f64().unwrap_or(0.0) as f32,
                q[1].as_f64().unwrap_or(0.0) as f32,
                q[2].as_f64().unwrap_or(0.0) as f32,
                q[3].as_f64().unwrap_or(1.0) as f32,
            );
            Ok(FogWallPlace {
                wall: *wall,
                position: vec3(&row["translation"]).ok_or("object has no translation")?,
                rotation,
                local_min: vec3(&bounds["min"]).ok_or("fog wall model has no bounds")?,
                local_max: vec3(&bounds["max"]).ok_or("fog wall model has no bounds")?,
            })
        })
        .collect()
}

/// The triangle range `[first, first + count)` of the collision part with `entity`.
fn collision_range(layout: &Value, entity: i32) -> Option<std::ops::Range<usize>> {
    let row = layout["collisions"]
        .as_array()?
        .iter()
        .find(|c| c["entity_id"].as_i64() == Some(i64::from(entity)))?;
    let first = row["triangles"][0].as_u64()? as usize;
    Some(first..first + row["triangles"][1].as_u64()? as usize)
}

/// `mesh` without the triangles in `range`, and those triangles on their own.
fn split_triangles(
    mesh: &CollisionMesh,
    range: std::ops::Range<usize>,
) -> (CollisionMesh, CollisionMesh) {
    let mut rest = CollisionMesh {
        vertices: mesh.vertices.clone(),
        ..Default::default()
    };
    let mut part = rest.clone();
    for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let side = if range.contains(&t) {
            &mut part
        } else {
            &mut rest
        };
        side.indices.extend(tri);
        side.materials
            .push(mesh.materials.get(t).copied().unwrap_or(u32::MAX));
    }
    (rest, part)
}

/// The horizontal direction a yaw faces (yaw 0 faces -Z, positive yaw turns left).
pub fn forward(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

/// The yaw at `from` that faces `to`.
pub fn yaw_toward(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.z)
}

/// Whether a horizontal box reaches within `radius` of `center` (on the horizontal plane).
pub fn box_within(min: Vec2, max: Vec2, center: Vec2, radius: f32) -> bool {
    let nearest = center.clamp(min, max);
    nearest.distance_squared(center) <= radius * radius
}

/// The triangles of `mesh` that reach within `radius` of `center` on the horizontal plane
/// (bounding-box test), with only the vertices they use.
pub fn crop(mesh: &CollisionMesh, center: Vec3, radius: f32) -> CollisionMesh {
    let c = Vec2::new(center.x, center.z);
    let mut remap = vec![u32::MAX; mesh.vertices.len()];
    let mut out = CollisionMesh::default();
    for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let p = tri.map(|i| {
            let v = mesh.vertices[i as usize];
            Vec2::new(v[0], v[2])
        });
        let min = p[0].min(p[1]).min(p[2]);
        let max = p[0].max(p[1]).max(p[2]);
        if !box_within(min, max, c, radius) {
            continue;
        }
        for &i in tri {
            let r = &mut remap[i as usize];
            if *r == u32::MAX {
                *r = out.vertices.len() as u32;
                out.vertices.push(mesh.vertices[i as usize]);
            }
            out.indices.push(*r);
        }
        out.materials
            .push(mesh.materials.get(t).copied().unwrap_or(u32::MAX));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn yaw_toward_matches_forward() {
        let from = Vec3::new(1.0, 0.0, 2.0);
        for to in [
            Vec3::new(5.0, 3.0, -1.0),
            Vec3::new(-4.0, 0.0, 2.0),
            Vec3::new(1.0, 0.0, 9.0),
        ] {
            let f = forward(yaw_toward(from, to));
            let want = (to - from).with_y(0.0).normalize();
            assert!(f.distance(want) < 1e-5, "{f} vs {want}");
        }
        // Yaw 0 faces -Z; a quarter turn left faces -X.
        assert!(forward(0.0).distance(Vec3::NEG_Z) < 1e-6);
        assert!(forward(std::f32::consts::FRAC_PI_2).distance(Vec3::NEG_X) < 1e-6);
    }

    #[test]
    fn resolves_boss_and_start() {
        let layout = json!({
            "enemies": [{ "name": "c5060_0000", "translation": [104.5, 54.0, 244.0], "yaw_degrees": -46.0 }],
            "players": [{ "name": "c0000_0003", "translation": [120.0, 54.0, 234.0], "yaw_degrees": -46.8 }],
        });
        let spec = ArenaSpec::by_name("OWL").unwrap();
        let a = ArenaLayout::resolve(spec, &layout).unwrap();
        assert_eq!(a.boss.position, Vec3::new(104.5, 54.0, 244.0));
        assert!((a.boss.yaw - (-46f32).to_radians()).abs() < 1e-6);
        let to_boss = (a.boss.position - a.wolf.position).normalize();
        assert!(forward(a.wolf.yaw).dot(to_boss) > 0.999);

        let no_start = ArenaSpec {
            player_start: None,
            ..*spec
        };
        let a = ArenaLayout::resolve(&no_start, &layout).unwrap();
        let d = a.wolf.position.distance(a.boss.position);
        assert!((d - FALLBACK_DISTANCE).abs() < 1e-4, "{d}");
        assert!(ArenaSpec::by_name("nobody").is_err());
        assert_eq!(spec.enemy_spec(), EnemySpec::owl());
    }

    #[test]
    fn crop_keeps_nearby_triangles() {
        let mesh = CollisionMesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [100.0, 0.0, 0.0],
                [101.0, 0.0, 0.0],
                [100.0, 0.0, 1.0],
                // A long triangle that crosses the circle without a vertex inside it.
                [-50.0, 0.0, 5.0],
                [50.0, 0.0, 5.0],
                [50.0, 0.0, 6.0],
            ],
            indices: vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            materials: vec![1, 2, 3],
        };
        let out = crop(&mesh, Vec3::ZERO, 10.0);
        assert_eq!(out.materials, vec![1, 3]);
        assert_eq!(out.vertices.len(), 6);
        assert_eq!(out.indices, vec![0, 1, 2, 3, 4, 5]);
    }
}
