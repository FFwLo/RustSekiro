//! The Shinobi ninjutsu: Bloodsmoke (EquipParamGoods 2100), Puppeteer (2110) and Bestowal (2120),
//! key items (goodsType 8) worn in the Equipment's ninjutsu slot (config `player.ninjutsu`).
//!
//! - The deathblow: a behind deathblow (unaware, ThrowKind 100, or posture-broken, 41100) has
//!   ninjutsu twins in the enemy's ThrowParam rows (main +1 / +2 / +3: Ochimusha 11010022-24,
//!   11010112-114), whose ThrowKindParam rows (201-203, 41201-41203) carry `throwSkillId` = the
//!   ninjutsu and `derivationSourceThrowKind` = the plain kind. With the ninjutsu worn and its
//!   Spirit Emblems held (consumeResourceItemA 6 / 5 / 7), the kill takes the twin: Wolf's
//!   a200_510210 / 220 / 230 (broken 511xxx), the enemy's ThrowDef12210 / 20 / 30 (13xxx).
//!   Only the blow that kills (no marks left). gap: the exe's pick of the twin and its emblem
//!   payment are not traced (the rows' fields and names stand in).
//! - Bloodsmoke: the clip's TAE 2 bullets 300 / 301 (v5000, prosthetic.rs spawn_bullets as any
//!   sword bullet): a 4-6 m cloud for 4 s, 220010 (the blood reaction, ref 1000500) and 220014
//!   (blind: sightSearchRate 0, hearingSearchEnemyCut 100, 0.5 s) on the enemies in it.
//! - Puppeteer: the clip's TAE 304 judge 400 -> AtkParam_Pc 400 puts 220020 (no resurrection),
//!   220021 (360 deg sight), 220030 (NOT_TO_DEATH_ANIME 1000111 and immortal stateInfo 143 for
//!   45 s, then 220022: changeHpRate 100, dead) and 220040 (changeTeamType 15, charmed) on the
//!   enemy (`throw_effects`). Its ThrowDef12220 has no death in it (no corpse ref 1000124, its
//!   own cancel windows); with 1000111 worn it gets up and fights for Wolf (enemy.rs).
//! - Bestowal: the clip's TAE 940 judge 500 -> BehaviorParam_PC 105000500 -> SpEffect 106040 on
//!   Wolf for 30 s ("red blood enchantment": spAttribute 3, stateInfo 153, behaviorRefId 232 =
//!   no guard / deflect recoil, vfx 6040); prosthetic.rs's TAE 940 pass.

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::Combat;
use crate::enemy::Enemy;
use crate::player::Player;

/// The three ninjutsu goods, in the menu's order.
pub const NINJUTSU: [i64; 3] = [2100, 2110, 2120];

/// SP_EFFECT_REF_NOT_TO_DEATH_ANIME (c9997.lua 568): the Puppeteer's 220030 / 220031.
pub const REF_NOT_TO_DEATH: i64 = 1_000_111;

/// The Spirit Emblems a ninjutsu costs (EquipParamGoods consumeResourceItemA: 6 / 5 / 7).
pub fn cost(id: i64) -> u32 {
    crate::items::param("EquipParamGoods", id)["consumeResourceItemA"].as_i64().unwrap_or(0).max(0) as u32
}

/// The ninjutsu twin of a deathblow's main ThrowParam row `row`: the row 1-3 after it whose
/// ThrowKind has `throwSkillId` == `ninjutsu` and derives from the main row's kind. Its suffix.
pub fn twin(row: i64, ninjutsu: i64) -> Option<i64> {
    if ninjutsu <= 0 {
        return None;
    }
    let kind = crate::items::param("ThrowParam", row)["throwKind"].as_i64()?;
    (1..=3).map(|k| row + k).find(|&r| {
        let Some(k2) = crate::items::param("ThrowParam", r)["throwKind"].as_i64() else { return false };
        let tk = crate::items::param("ThrowKindParam", k2);
        tk["throwSkillId"].as_i64() == Some(ninjutsu) && tk["derivationSourceThrowKind"].as_i64() == Some(kind)
    })
}

/// The ninjutsu deathblow in place of the plain one: the twin's suffix and its row, when the
/// ninjutsu is worn, paid for (`emblems`, unless `free`), the blow kills (`kills`) and Wolf has
/// the twin's clip.
pub fn pick(combat: &Combat, ninjutsu: i64, emblems: u32, free: bool, foe_row: impl Fn(i64) -> i64, suffix: i64, kills: bool) -> Option<(i64, crate::data::Throw)> {
    if !kills || (!free && emblems < cost(ninjutsu)) {
        return None;
    }
    let base = foe_row(0);
    let row = twin(foe_row(suffix), ninjutsu)?;
    let th = combat.throw(row).filter(|t| combat.player.anim(&t.atk_anim).is_some())?;
    Some((row - base, th))
}

/// TAE 304 ThrowAttackBehavior in Wolf's deathblow clip: its BehaviorParam_PC row (variation
/// 5000, refType 0) names an AtkParam_Pc row whose spEffectId0-4 go on the thrown enemy (the
/// Puppeteer's 400; the plain kinds' 790-792 -> 110080-110082). Lasting rows for their
/// effectEndurance (-1: for good).
pub(crate) fn throw_effects(combat: Res<Combat>, player: Query<(&Actor, &Player)>, mut enemies: Query<&mut Actor, (With<Enemy>, Without<Player>)>) {
    for (a, p) in &player {
        let Some(target) = p.throw_target else { continue };
        if a.anim.is_empty() || a.t == a.prev_t || !a.anim.starts_with("a2") {
            continue;
        }
        let d = &combat.player;
        let Some(anim) = d.anim(&a.anim) else { continue };
        let Ok(mut ea) = enemies.get_mut(target) else { continue };
        for e in anim.events.iter().filter(|e| e.kind == 304 && e.start > a.prev_t && e.start <= a.t) {
            let Some(b) = e.arg_i64("BehaviorJudgeID").and_then(|j| d.behavior(5000, j)).filter(|b| b.ref_type == 0) else { continue };
            let atk = crate::items::param("AtkParam_Pc", b.ref_id);
            for id in (0..5).filter_map(|k| atk[format!("spEffectId{k}")].as_i64()).filter(|id| *id > 0) {
                let Some(row) = crate::data::sp_cycle(id) else { continue };
                if row.endurance == 0.0 {
                    continue;
                }
                let secs = if row.endurance < 0.0 { f32::INFINITY } else { row.endurance };
                ea.sp_chained.retain(|c| c.0 != id);
                ea.sp_chained.push((id, secs));
            }
        }
    }
}

/// Whether an actor wears a lasting SpEffect with this behaviorRefId (`sp_chained`).
pub fn wears_ref(a: &Actor, re: i64) -> bool {
    a.sp_chained.iter().any(|(id, _)| crate::data::sp_effect_behavior_ref(*id) == re)
}
