//! Authored map effect inventory. No image, GPU or live-game dependencies.
use crate::world_events::{EffectWrite, WorldState};
use sekiro_formats::{
    flver,
    msb::{self, MapRegion, PartKind},
    param::ParamSet,
};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, Serialize)]
pub struct Placement {
    pub name: String,
    pub entity: i32,
    pub effect: i32,
    /// Already mirrored into renderer coordinates, with dummy/bone transforms applied.
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub enabled: bool,
    pub activation: Option<String>,
    pub dummy: Option<i32>,
    pub param: Option<i32>,
    pub source: Option<EffectWrite>,
    /// A Sculptor's Idol's idle effect: the object's idle clip (unlit or lit) plays it, so it
    /// shows only while the idol's lit state matches.
    pub idol: Option<IdolIdle>,
}

/// Which idle clip of a Sculptor's Idol (a registered bonfire object) plays an effect: the
/// unlit idle ([`UNLIT_IDLE`], the embers circling the idol) or the lit one ([`LIT_IDLE`], the
/// flame above its hands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IdolIdle {
    /// The lit idle's effect (else the unlit one's).
    pub lit: bool,
    /// The story lights the idol already (its bonfire flag is on or it holds kindling).
    pub lit_at_start: bool,
}

impl IdolIdle {
    /// Whether the effect shows, `found` saying whether Wolf has lit the idol in play.
    pub fn shows(&self, found: bool) -> bool {
        self.lit == (self.lit_at_start || found)
    }
}

/// The idol objects' idle clips (o005000, o005500, o115040 TAE): 1 unlit, 11 lit.
pub const UNLIT_IDLE: i64 = 1;
pub const LIT_IDLE: i64 = 11;

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub placements: Vec<Placement>,
    pub regions: Vec<MapRegion>,
}

pub fn load(cache: &Path, map: &str, state: &WorldState) -> Result<Catalog, String> {
    let load = || -> Result<Catalog, Box<dyn std::error::Error>> {
        let msb = msb::parse(&std::fs::read(
            cache.join(format!("raw/map/mapstudio/{map}.msb")),
        )?)?;
        let params = ParamSet::load(
            &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            &cache.join("refs/paramdex/Defs"),
        )?;
        let table = params.table("ModelSfxParam")?;
        let mut placements = Vec::new();
        for r in &msb.regions {
            let Some(effect) = r.effect else {
                continue;
            };
            let source = state
                .effects
                .iter()
                .rev()
                .find(|w| w.entity == r.entity_id && matches!(w.command, 1 | 2))
                .cloned();
            placements.push(Placement {
                name: r.name.clone(),
                entity: r.entity_id,
                effect,
                position: [-r.position[0], r.position[1], r.position[2]],
                rotation: mirror_rotation(r.rotation),
                enabled: source
                    .as_ref()
                    .map_or(!r.start_disabled, |w| w.command == 2),
                activation: msb.part_by_index(r.activation_part).map(|p| p.name.clone()),
                dummy: None,
                param: None,
                source,
                idol: None,
            });
        }
        let mut models = BTreeMap::new();
        let template = sekiro_formats::tae::Template::parse_xml(&std::fs::read_to_string(
            cache.join("refs/TAE.Template.SDT.xml"),
        )?)?;
        for part in msb.parts.iter().filter(|p| p.kind == PartKind::Object) {
            let Some(model) = msb.models.get(part.model_index as usize) else {
                continue;
            };
            let mut requests = Vec::new();
            if let Some(object) = &part.object {
                let relative = i32::from(object.model_sfx_param_relative_id);
                if relative >= 0 {
                    let model_id: i32 = model.name.trim_start_matches('o').parse()?;
                    let row_id = model_id * 1000 + relative;
                    if let Ok(row) = table.row(row_id) {
                        for slot in 0..8 {
                            let effect = row.i32(&format!("sfxId_{slot}"))?;
                            if effect > 0 {
                                requests.push((
                                    effect,
                                    row.i32(&format!("dmypolyId_{slot}"))?,
                                    Some(row_id),
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                }
            }
            let mut scripted = BTreeMap::new();
            for write in state
                .effects
                .iter()
                .filter(|w| w.entity == part.entity_id && part.entity_id > 0)
            {
                match write.command {
                    4 => {
                        scripted.insert(write.dummy, write.clone());
                    }
                    5 => scripted.clear(),
                    _ => {}
                }
            }
            for (dummy, write) in scripted {
                requests.push((write.effect, dummy, None, Some(write), None));
            }
            if let Some(bonfire) = state.bonfires.get(&part.entity_id) {
                let lit_at_start = bonfire.kindling > 0 || state.flags.contains(&bonfire.flag);
                let path = cache.join(format!(
                    "raw/obj/{0}.objbnd.d/{0}.anibnd.d/{0}.tae",
                    model.name
                ));
                let tae = sekiro_formats::tae::parse(&std::fs::read(path)?)?;
                // Both idles: lighting the idol in play swaps the unlit one for the lit one.
                for (animation, lit) in [(UNLIT_IDLE, false), (LIT_IDLE, true)] {
                    let Some(idle) = tae.animations.iter().find(|a| a.id == animation) else {
                        continue;
                    };
                    for event in idle.events.iter().filter(|e| {
                        e.start <= 0. && matches!(e.event_type, 96 | 99 | 100 | 118..=121)
                    }) {
                        let Some(decoded) = template.decode(event) else {
                            continue;
                        };
                        let int = |field| {
                            decoded.field(field).and_then(|v| v.as_int()).unwrap_or(-1) as i32
                        };
                        let effect = int("FFXID");
                        let dummy = int("DummyPolyID");
                        if effect > 0 {
                            requests.push((
                                effect,
                                dummy,
                                None,
                                Some(EffectWrite {
                                    event: bonfire.event,
                                    slot: bonfire.slot,
                                    instruction: bonfire.instruction,
                                    command: 200903,
                                    entity: part.entity_id,
                                    dummy,
                                    effect,
                                }),
                                Some(IdolIdle { lit, lit_at_start }),
                            ));
                        }
                    }
                }
            }
            if requests.is_empty() {
                continue;
            }
            if !models.contains_key(&model.name) {
                let data = std::fs::read(
                    cache.join(format!("raw/obj/{0}.objbnd.d/{0}.flver", model.name)),
                )?;
                models.insert(model.name.clone(), flver::parse(&data)?);
            }
            let flver = &models[&model.name];
            for (effect, dummy, param, source, idol) in requests {
                let mut local = glam::Vec3::ZERO;
                if dummy >= 0 {
                    let Some(d) = flver
                        .dummies
                        .iter()
                        .find(|d| i32::from(d.reference_id) == dummy)
                    else {
                        continue;
                    };
                    local = d.position.into();
                    let mut parent = d.parent_bone;
                    for _ in 0..flver.nodes.len() {
                        let Some(node) = usize::try_from(parent)
                            .ok()
                            .and_then(|i| flver.nodes.get(i))
                        else {
                            break;
                        };
                        local = node.local_matrix().transform_point3(local);
                        parent = node.parent;
                    }
                }
                let p = part.matrix().transform_point3(local);
                let enabled = state
                    .visibility
                    .iter()
                    .rev()
                    .find(|w| {
                        w.kind == crate::world_events::PartKind::Object
                            && w.target > 0
                            && (w.target == part.entity_id
                                || part.entity_group_ids.contains(&w.target))
                    })
                    .is_none_or(|w| w.enabled);
                placements.push(Placement {
                    name: part.name.clone(),
                    entity: part.entity_id,
                    effect,
                    position: [-p.x, p.y, p.z],
                    rotation: mirror_rotation(part.rotation),
                    enabled,
                    activation: Some(part.name.clone()),
                    dummy: Some(dummy),
                    param,
                    source,
                    idol,
                });
            }
        }
        Ok(Catalog {
            placements,
            regions: msb.regions,
        })
    };
    load().map_err(|e| format!("map effects {map}: {e}"))
}

fn mirror_rotation(r: [f32; 3]) -> [f32; 4] {
    (glam::Quat::from_rotation_y(-r[1].to_radians())
        * glam::Quat::from_rotation_z(-r[2].to_radians())
        * glam::Quat::from_rotation_x(r[0].to_radians()))
    .to_array()
}
