//! Sim tests, part 14: the atlas combat pass (docs/kb/mechanics-atlas.json, categories combat,
//! armor, deathblows). Each test names its atlas id.

use super::*;

/// An NPC attack row as the hit test sees it: only the fields `combat::dodged` reads.
fn npc_attack(atk_param_id: i64, throw_flag: i64) -> crate::data::Attack {
    crate::data::Attack { atk_param_id, throw_flag, ..Default::default() }
}

/// Atlas armor.ignore-iframes-attack and armor.iframes-950-mistraven (exe FUN_140b68a50 /
/// FUN_140b68ff0): Wolf's knock-down a000_100400 (TAE 950, frames 0-18) lets no plain hit through
/// but an AtkParam isDisableNoDamage one (the Shura samurai's poison fog 10700360); the General's
/// ThrowDef a000_012100 (ChrActionFlag 67, frames 0-45) lets only a throw's damage (throwFlag 2)
/// through.
#[test]
fn atlas_invincibility_bits_pass_only_their_exceptions() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    let mut wolf = Actor::new(crate::actor::Side::Player, 100.0, 100.0, 1.0, 0);
    wolf.anim = "a000_100400".into();
    wolf.t = 10.0 / TAE_FPS;
    assert!(crate::combat::dodged(combat, &wolf, None, &npc_attack(10100100, 0)), "950 stops a plain slash");
    assert!(!crate::combat::dodged(combat, &wolf, None, &npc_attack(10700360, 0)), "isDisableNoDamage goes through 950");
    wolf.t = 30.0 / TAE_FPS;
    assert!(!crate::combat::dodged(combat, &wolf, None, &npc_attack(10100100, 0)), "after frame 18 the slash lands");

    let mut general = Actor::new(crate::actor::Side::Enemy, 100.0, 100.0, 1.0, 0);
    general.anim = "a000_012100".into();
    general.t = 20.0 / TAE_FPS;
    assert!(crate::combat::dodged(combat, &general, None, &npc_attack(5000000, 0)), "flag 67 stops a slash");
    assert!(!crate::combat::dodged(combat, &general, None, &npc_attack(5000000, 2)), "flag 67 lets the throw's damage through");
}

/// Atlas combat.block-vs-deflect-repel (exe FUN_140b6c880, ported after sekiro-rs hit.rs): Wolf's
/// guard a050_203000 (judge 90 -> AtkParam_Pc 5000090, repel 100 % x Kusabimaru's 20) raised to 40
/// by the deflect window 105010 (frames 0-6) deflects a soldier's slash (guardAtkRate 30) and only
/// blocks it after the window; an attack of power 100 is blocked even in the window. The General's
/// grab 10200220 goes through the sword's guard (type 0: disableGuard 1) but the open umbrella's
/// (a076_405020, judge 100 -> 7600100, type 1: 0 / 0) blocks it.
#[test]
fn atlas_block_or_deflect_follows_the_repel_values() {
    use crate::guard::Guarded;
    let app = app();
    let combat = app.world().resource::<Combat>();
    let mut wolf = Actor::new(crate::actor::Side::Player, 100.0, 100.0, 1.0, 0);
    let d = combat.data_of(&wolf);
    let slash = |power: i64| crate::data::Attack { atk_param_id: 10100100, guard_atk_rate: power, ..Default::default() };
    let guard = |a: &Actor, umbrella: Option<i64>, atk: &crate::data::Attack| crate::combat::guard_contact(combat, d, a, None, (&a.anim, a.t), umbrella, false, atk, 5000).guarded;
    wolf.anim = "a050_203000".into();
    wolf.t = 0.1;
    assert_eq!(guard(&wolf, None, &slash(30)), Guarded::Deflect, "window: 40 >= 30");
    assert_eq!(guard(&wolf, None, &slash(100)), Guarded::Block, "window: 40 < 100");
    wolf.t = 0.4;
    assert_eq!(guard(&wolf, None, &slash(30)), Guarded::Block, "guard: 20 < 30");
    let grab = crate::data::Attack { atk_param_id: 10200220, throw_flag: 1, guard_atk_rate: 30, disable_guard: 1, disable_just_guard: 1, ..Default::default() };
    assert_eq!(guard(&wolf, None, &grab), Guarded::Hit, "the sword cannot guard a grab");
    wolf.anim = "a076_405020".into();
    wolf.t = 1.0;
    assert_eq!(guard(&wolf, Some(76000), &grab), Guarded::Block, "the umbrella can");
}

/// Atlas combat.enemy-guard-block-reduction and combat.posture-attribute-rates (exe 8439a0 /
/// 840550 / bfafa0, ported after sekiro-rs posture.rs): a soldier whose NpcParam staminaGuardDef
/// is 50 (10100100) takes half a blocked blow's posture through its guard row 10100900; the shield
/// servants' resident 400250 (stateInfo 158, slash x0.5) halves only a blocked slash, never a hit
/// or a deflect.
#[test]
fn atlas_an_enemy_guard_cuts_the_posture_it_takes() {
    let npc = crate::items::param("NpcParam", 10100100);
    let row = crate::guard::GuardRow::of(false, npc["behaviorVariationId"].as_i64().unwrap(), 900);
    assert!(row.is_some(), "the soldier's guard row");
    let def = crate::guard::GuardDef::npc(npc, 1.0);
    assert_eq!(def.def, 50.0);
    assert_eq!(crate::guard::guarded_posture(40.0, &def, row.as_ref(), &[], false, 1), 20.0, "50 % cut");
    assert_eq!(crate::guard::guarded_posture(40.0, &def, row.as_ref(), &[400250], false, 1), 10.0, "400250 on a block");
    assert_eq!(crate::guard::guarded_posture(40.0, &def, row.as_ref(), &[400250], true, 1), 20.0, "not on a deflect");
    assert!(!crate::guard::counts_on_a_hit(158) && !crate::guard::counts_on_a_hit(204) && crate::guard::counts_on_a_hit(0));
}

/// Atlas combat.elements-through-guard (exe 845ed0, ported after sekiro-rs hp.rs): a blocking
/// soldier (10100100: physGuardCutRate 100, slashGuardCutRate 100, fire 33, magic 25, dark 50,
/// lightning 0) stops a slash's physical part and lets 67 % of fire, 75 % of magic, half the dark
/// and all lightning through.
#[test]
fn atlas_elements_pass_an_enemy_guard() {
    let npc = crate::items::param("NpcParam", 10100100);
    let cut = crate::guard::npc_guard_cut(npc, None, 1);
    assert!(cut[0] <= 0.0, "physical stops: {}", cut[0]);
    for (got, want) in cut[1..].iter().zip([0.75, 0.67, 1.0, 0.5]) {
        assert!((got - want).abs() < 1e-4, "{cut:?}");
    }
}

/// Atlas combat.attack-power-growth and progression.attack-power-posture: at Attack Power 10 Wolf
/// wears 160010 (x3.25 on every attack and posture rate): his HP and posture damage rates, his
/// deflect return and his Mikiri posture all carry it; Attack Power 1 (160001) is x1.
#[test]
fn atlas_attack_power_scales_wolfs_damage_and_posture() {
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.attack_power = 10;
    app.update();
    let config = app.world().resource::<crate::config::GameConfig>().clone();
    assert_eq!(crate::combat::growth_sp(&config), 160010);
    let combat = app.world().resource::<Combat>();
    for f in ["defStaminaAttackRate", "attackHitParryStaminaAttackRate"] {
        assert!((crate::combat::skill_rate(combat, &config, f) - 3.25).abs() < 1e-5, "{f}");
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!((a.atk_rate - 3.25).abs() < 1e-5 && (a.stam_atk_rate - 3.25).abs() < 1e-5, "{} {}", a.atk_rate, a.stam_atk_rate);
}

/// Atlas combat.posture-break-recovery (StaminaControlParam ranges by TAE 960 type, exe bd4e20,
/// ported after sekiro-rs posture.rs): the soldiers' row 1000100 holds a broken meter empty in the
/// collapse (type 5), leaves it 30 % full-left as the enemy rises (type 6) and refills it in a
/// deathblow anim (type 7); type 5 does not empty a meter that is not broken. Wolf's row 0 refills
/// his in a break anim (type 5: 100 / 100).
#[test]
fn atlas_the_collapse_leaves_thirty_percent_posture() {
    let mut a = Actor::new(crate::actor::Side::Enemy, 100.0, 200.0, 1.0, 1000100);
    a.posture = 200.0;
    crate::actor::hold_posture_range(&mut a, 5);
    assert_eq!(a.posture, 200.0, "held empty");
    crate::actor::hold_posture_range(&mut a, 6);
    assert_eq!(a.posture, 140.0, "60 of 200 left");
    crate::actor::hold_posture_range(&mut a, 7);
    assert_eq!(a.posture, 0.0, "refilled");
    a.posture = 50.0;
    crate::actor::hold_posture_range(&mut a, 5);
    assert_eq!(a.posture, 50.0, "no break from the range alone");
    let mut wolf = Actor::new(crate::actor::Side::Player, 100.0, 90.0, 1.0, 0);
    wolf.posture = 90.0;
    crate::actor::hold_posture_range(&mut wolf, 5);
    assert_eq!(wolf.posture, 0.0);
}

/// Atlas combat.mikiri-dummy-hitbox: the soldier's thrust fires counter dummies 10100940 / 941
/// beside its real hit 10100230; only the dummies are counter-only (they land nothing outside a
/// MikiriCounter window, combat.rs resolve).
#[test]
fn atlas_the_counter_dummy_is_for_mikiri_only() {
    let app = app_with("c1010");
    let combat = app.world().resource::<Combat>();
    let atk = |id: i64| combat.kinds[0].data.attacks.values().find(|a| a.atk_param_id == id).cloned().unwrap_or_else(|| panic!("no {id}"));
    assert!(crate::combat::counter_dummy(&atk(10100940)));
    assert!(crate::combat::counter_dummy(&atk(10100941)));
    assert!(!crate::combat::counter_dummy(&atk(10100230)), "the real thrust");
    assert!(crate::combat::mikiri_atk(&atk(10100230)));
}

/// Atlas deathblows.mark-drops: a marked enemy's NpcParam ninsatsuResourceItemLotId_2 (10005800 on
/// the General 10200000, two marks) gives 10 Resurrective Power for each deathblow that lands; _1 (10000800)
/// gives nothing.
#[test]
fn atlas_a_deathblow_draws_the_mark_lot() {
    let npc = crate::items::param("NpcParam", 10200000);
    assert_eq!(npc["ninsatsuResourceItemLotId_2"].as_i64(), Some(10005800));
    for r in [0.0, 0.5, 0.99] {
        assert_eq!(crate::drops::roll_resource_lot(10005800, 0.0, r), Some((5, 10)));
        assert_eq!(crate::drops::roll_resource_lot(10000800, 0.0, r), None);
    }
}
