//! Guardian Ape's false death, verified against m17 event 11705821 and c5100 TAE.
use crate::deathblow::{Placement, ThrowTable};
use crate::{combat::hp::Vitality, enemy::EnemySpec};
use std::path::Path;

/// The lethal headless absorption exposes another paired throw before map message 30.
/// The native throw search is not recovered; eligibility uses the authored 220420 window.
pub fn finishing_rows(
    spec: &EnemySpec,
    character: &crate::character::Character,
    table: &ThrowTable,
    attacker: &Placement,
    defender: &Placement,
) -> Option<(i32, i32)> {
    if !headless(spec) || !character.tae_frame().sp_effects.contains(&220420) {
        return None;
    }
    let clip = &character.behavior.runtime.main_clip()?.animation;
    let id = match clip.as_str() {
        "a100_012001" => 15100509,
        "a100_013201" => 15100619,
        _ => return None,
    };
    let row = table.rows.iter().find(|r| r.id == id)?;
    row.in_range(attacker, defender).then_some((id, id))
}

/// Continuations have no lunge: the body row starts directly, at its own TAE strike.
pub fn finishing_times(
    player: &crate::tae::TaeDb,
    body: &crate::deathblow::ThrowRow,
) -> Option<crate::deathblow::ThrowTimes> {
    let damage = player
        .anim(&body.attacker_anim())?
        .events
        .iter()
        .filter_map(|e| {
            matches!(e.kind, crate::tae::TaeKind::ThrowAttack { .. }).then_some(e.start)
        })
        .reduce(f32::min)
        .unwrap_or(f32::INFINITY);
    Some(crate::deathblow::ThrowTimes {
        grab: (0.0, 0.0),
        damage,
        death_branch: None,
    })
}

/// The waterfall ape spends its only visible mark before the scripted headless revival.
pub fn false_death(spec: &EnemySpec) -> bool {
    spec.chr == "c5100" && spec.npc_param == 51000000 && spec.think == 51000000
}

/// Preserve the map's immortality across the decapitation until the revival TAE restores HP.
pub fn decapitated(vitality: &mut Vitality) {
    vitality.hp = 1;
    vitality.cannot_die = true;
}

/// The headless set uses Wolf's a235 pair and the ape's neck dummy 235, not a230.
/// Pair selection is inferred from ThrowParam and the a100 defender timelines.
pub fn headless(spec: &EnemySpec) -> bool {
    spec.chr == "c5100" && spec.think == 51000100
}

pub fn headless_rows(
    table: &ThrowTable,
    attacker: &Placement,
    defender: &Placement,
) -> Option<(i32, i32)> {
    [(500, 501), (610, 611)]
        .into_iter()
        .find_map(|(start, body)| {
            let start = table.player_row(5100, 0, start)?;
            let body = table.player_row(5100, 0, body)?;
            start
                .in_range(attacker, defender)
                .then_some((start.id, body.id))
        })
}

/// Revival TAE adds 275100 alongside HP recovery; its maximum-stat rates persist.
/// Native application order is inferred: apply the new maxima before the full refill.
pub fn revival_stats(cache: &Path, fighter: &mut crate::fight::Fighter) -> Option<()> {
    let crate::fight::FighterKind::Npc(npc) = &fighter.kind else {
        return None;
    };
    if fighter.vitality.max_hp != npc.hp {
        return Some(());
    }
    let table = crate::hits::load_table(
        &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
        &cache.join("refs/paramdex/Defs"),
        "SpEffectParam",
        "SpEffect",
    )?;
    let row = table.find(275100)?;
    let hp_rate = row.f32("maxHpRate").ok()?;
    let posture_rate = row.f32("maxStaminaRate").ok()?;
    fighter.vitality.max_hp = (npc.hp as f32 * hp_rate) as i32;
    fighter.posture.max = (npc.max_posture as f32 * posture_rate) as i32;
    Some(())
}
