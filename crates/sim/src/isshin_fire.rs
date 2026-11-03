//! Shura's static fire emitter, placed and triggered by the installed map event.

use std::{collections::HashMap, path::Path};

use crate::{bullets::Bullets, fight::CombatRules, hits::Combatant, registry::Registry};

pub const OWNER: i32 = 1110921;

pub struct FireEmitter {
    pub fighter: crate::fight::Fighter,
    sources: HashMap<i32, crate::deathblow::Placement>,
}

impl FireEmitter {
    pub fn load(cache: &Path, wolf: &Combatant) -> Result<Self, String> {
        let registry = Registry::load(cache)?;
        let placement = registry
            .placed
            .get("c1000")
            .and_then(|rows| {
                rows.iter()
                    .find(|p| p.map == "m11_01_00_00" && p.entity == OWNER)
            })
            .ok_or("Shura fire emitter placement is missing")?;
        let path = cache.join("anim/c1000/skeleton.bin");
        let skeleton = sekiro_formats::anim::Skeleton::from_bytes(
            &std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| e.to_string())?;
        let emitter = Combatant::npc(cache, "c1000", placement.npc, &skeleton);
        let mut rules = CombatRules::load(cache, &[wolf, &emitter]);
        let path = cache.join("raw/event/m11_01_00_00.emevd");
        let script = sekiro_formats::emevd::Emevd::parse(
            &std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| e.to_string())?;
        let shots = script
            .event(11115944)
            .ok_or("Shura fire event is missing")?;
        let msb = cache.join("raw/map/MapStudio/m11_01_00_00.msb");
        sekiro_formats::msb::parse(
            &std::fs::read(&msb).map_err(|e| format!("{}: {e}", msb.display()))?,
        )
        .map_err(|e| e.to_string())?;
        let sources = shots
            .instructions
            .iter()
            .filter(|ins| ins.code() == (2003, 5) && ins.i32_at(0) == Some(OWNER))
            .filter_map(|ins| {
                let source = ins.i32_at(4)?;
                Some((source, crate::events::warp_point(cache, 11, 1, source)?))
            })
            .collect();
        Ok(Self {
            fighter: rules
                .fighters
                .pop()
                .ok_or("fire emitter has no combat row")?,
            sources,
        })
    }
}

/// The installed Shura instructions use point sources, no dummy, and zero angle overrides.
pub fn shoot(
    emitter: &FireEmitter,
    bullets: &mut Bullets,
    boss: &Combatant,
    source: i32,
    behavior: i32,
) -> Result<(), String> {
    // The installed EMEVD also names 1112960/61, absent from its complete MSB.
    // An unresolved source has no world transform and cannot emit a shot.
    let Some(point) = emitter.sources.get(&source) else {
        return Ok(());
    };
    let judge = behavior - (200_000_000 + boss.variation * 1000);
    let def = boss
        .bullets
        .get(&judge)
        .ok_or_else(|| format!("Shura event bullet behavior {behavior} is not extracted"))?;
    bullets.spawn_event(def, OWNER, 1, judge, point.position.into(), point.yaw);
    Ok(())
}
