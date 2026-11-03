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


use glam::{Vec2, Vec3};
use sekiro_formats::hknp::CollisionMesh;

use crate::enemy::EnemySpec;
use crate::events::EventEntry;

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
