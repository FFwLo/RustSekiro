//! Combat rules against the player's own params in `cache/`. Skips when `cache/` is missing.

use std::sync::OnceLock;

use sekiro_formats::param::ParamSet;

use super::damage_type::{DefenderFacts, attacker_reaction, defender_reaction};
use super::effects::ActiveEffect;
use super::hit::{
    GuardInput, HitOutcome, npc_guard_repel, player_attack_repel, player_guard_repel, resolve_hit,
    resolve_part_hit,
};
use super::hp::{
    Vitality, attribute_guard_factor, element_damage, final_hp_damage, keep_hp, npc_guard_cut,
    player_guard_cut,
};
use super::params::{AttackProfile, CalcCorrectGraph, NpcCombat, StaminaControl, WeaponGuard};
use super::posture::{
    ControlRange, DefenderPosture, PostureMeter, apply_defender_posture, attacker_breaks,
    attacker_posture_damage, defender_posture_damage, player_posture_base,
};
use super::recovery::{
    RecoveryAccumulator, RecoveryInputs, displayed_recovery, player_base_speed,
    posture_recovery_per_second,
};

fn set() -> Option<&'static ParamSet> {
    static SET: OnceLock<Option<ParamSet>> = OnceLock::new();
    SET.get_or_init(|| {
        let cache = crate::cache::root();
        let dir = cache.join("raw/param/gameparam/gameparam.parambnd.d");
        let defs = cache.join("refs/paramdex/Defs");
        if !dir.is_dir() || !defs.is_dir() {
            eprintln!("skipping: cache/ not extracted");
            return None;
        }
        Some(ParamSet::load(&dir, &defs).expect("load params"))
    })
    .as_ref()
}

fn effect(set: &ParamSet, id: i32) -> ActiveEffect {
    ActiveEffect::from_row(&set.table("SpEffectParam").unwrap().row(id).unwrap()).unwrap()
}

fn soldier_attack(set: &ParamSet) -> AttackProfile {
    AttackProfile::from_row(&set.table("AtkParam_Npc").unwrap().row(10100100).unwrap()).unwrap()
}

fn soldier(set: &ParamSet) -> NpcCombat {
    NpcCombat::from_row(&set.table("NpcParam").unwrap().row(10100000).unwrap()).unwrap()
}

fn kusabimaru(set: &ParamSet) -> WeaponGuard {
    WeaponGuard::from_row(&set.table("EquipParamWeapon").unwrap().row(5000).unwrap()).unwrap()
}

/// Wolf guarding with the Kusabimaru, facing +X. The guard row's `guardBreakCorrection` is
/// assumed to be 100 (377 of 443 AtkParam_Pc rows use 100; the guard row itself is not traced).
fn wolf_guard(set: &ParamSet, effects: &[ActiveEffect]) -> GuardInput {
    let weapon = kusabimaru(set);
    GuardInput {
        guarding: true,
        forward: [1.0, 0.0],
        guard_angle: weapon.guard_angle,
        base_repel: player_guard_repel(100, weapon.guard_base_repel, effects),
        guard_attribute: 0,
    }
}

/// The soldier swings from +X toward Wolf, so the attack travels along -X.
const FROM_FRONT: [f32; 2] = [-1.0, 0.0];

#[test]
fn soldier_and_wolf_params_match_expectations() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    assert_eq!(atk.guard_atk_rate, 30);
    assert_eq!(atk.attack_power[0], 80.0);
    assert_eq!(
        (atk.direct_stam, atk.blocked_stam, atk.deflected_stam),
        (18, 18, 9)
    );
    assert_eq!(atk.attacker_deflected_stam, 45);
    let npc = soldier(set);
    assert_eq!((npc.hp, npc.max_posture), (195, 75));
    assert_eq!(npc.stamina_recover_base_vel, 20.0);
    assert_eq!(npc.max_debt_stamina, -30);
    assert_eq!(npc.guard_angle, 60);
    let window = effect(set, 105010);
    assert_eq!((window.state_info, window.def_flick_power), (158, 40));
    let weapon = kusabimaru(set);
    assert_eq!((weapon.guard_angle, weapon.guard_base_repel), (0, 20));
}

#[test]
fn guard_outcomes_against_the_soldier_slash() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);

    // Inside the just-guard window (105010, defFlickPower 40 >= guardAtkRate 30): deflect.
    let window = [effect(set, 105010)];
    let r = resolve_hit(&atk, FROM_FRONT, &wolf_guard(set, &window), &window);
    assert_eq!(r.outcome, HitOutcome::Deflect);
    assert_eq!(r.repel, 40);

    // Holding guard without the window: Kusabimaru repel 20 < 30, block.
    let r = resolve_hit(&atk, FROM_FRONT, &wolf_guard(set, &[]), &[]);
    assert_eq!(r.outcome, HitOutcome::Block);
    assert_eq!(r.repel, 20);

    // From behind the guard arc (default 90 degrees for guardAngle 0): hit.
    let r = resolve_hit(&atk, [1.0, 0.0], &wolf_guard(set, &window), &window);
    assert_eq!(r.outcome, HitOutcome::Hit);

    // Not guarding: hit.
    let mut idle = wolf_guard(set, &[]);
    idle.guarding = false;
    assert_eq!(
        resolve_hit(&atk, FROM_FRONT, &idle, &[]).outcome,
        HitOutcome::Hit
    );
}

#[test]
fn a_deflect_facing_away_is_still_a_hit() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    let window = [effect(set, 105010)];
    // Wolf inside the deflect window, the attack arriving `deg` off his facing (+X). The
    // Kusabimaru's guardAngle 0 is a 90 degree half-angle: the front half.
    let outcome = |deg: f32| {
        let a = deg.to_radians();
        let incoming = [-a.cos(), -a.sin()];
        resolve_hit(&atk, incoming, &wolf_guard(set, &window), &window).outcome
    };
    assert_eq!(outcome(0.0), HitOutcome::Deflect);
    assert_eq!(outcome(89.0), HitOutcome::Deflect);
    assert_eq!(outcome(-89.0), HitOutcome::Deflect);
    assert_eq!(outcome(91.0), HitOutcome::Hit);
    assert_eq!(outcome(180.0), HitOutcome::Hit);
    // The soldier deflecting with its 60 degree guardAngle: a wider attack lands.
    let npc = soldier(set);
    let guard = |deg: f32| {
        let a = deg.to_radians();
        let g = GuardInput {
            guarding: true,
            forward: [1.0, 0.0],
            guard_angle: npc.guard_angle,
            base_repel: 40,
            guard_attribute: 0,
        };
        resolve_hit(&atk, [-a.cos(), -a.sin()], &g, &[]).outcome
    };
    assert_eq!(guard(50.0), HitOutcome::Deflect);
    assert_eq!(guard(70.0), HitOutcome::Hit);
}

#[test]
fn soldier_guard_arc_is_sixty_degrees() {
    let Some(set) = set() else { return };
    let npc = soldier(set);
    let guard = |dir_deg: f32| {
        // Soldier faces +X; the attack arrives from `dir_deg` off its facing.
        let a = dir_deg.to_radians();
        let incoming = [-a.cos(), -a.sin()];
        super::hit::in_guard_arc([1.0, 0.0], incoming, npc.guard_angle)
    };
    assert!(guard(0.0));
    assert!(guard(59.0));
    assert!(!guard(61.0));
}

#[test]
fn perilous_flags_bypass_the_katana_guard() {
    let Some(set) = set() else { return };
    let mut atk = soldier_attack(set);
    // Perilous thrust or sweep style: unguardable and undeflectable for guard type 0 only
    // (60 AtkParam_Npc rows have this pattern).
    atk.disable_guard = [true, false];
    atk.disable_just_guard = [true, false];
    let window = [effect(set, 105010)];
    let r = resolve_hit(&atk, FROM_FRONT, &wolf_guard(set, &window), &window);
    assert_eq!(r.outcome, HitOutcome::Hit);
    assert!(r.guard_bypassed);
    let r = resolve_hit(&atk, FROM_FRONT, &wolf_guard(set, &[]), &[]);
    assert_eq!(r.outcome, HitOutcome::Hit);
    // The umbrella (guard type 1) still blocks it.
    let mut umbrella = wolf_guard(set, &[]);
    umbrella.guard_attribute = 1;
    assert_eq!(
        resolve_hit(&atk, FROM_FRONT, &umbrella, &[]).outcome,
        HitOutcome::Block
    );
    // Deflect-only attacks (111 rows): block is impossible but the window still deflects.
    atk.disable_just_guard = [false, false];
    let r = resolve_hit(&atk, FROM_FRONT, &wolf_guard(set, &window), &window);
    assert_eq!(r.outcome, HitOutcome::Deflect);
}

#[test]
fn npc_guard_repel_uses_guard_break_rate() {
    let Some(set) = set() else { return };
    // A guarding NPC whose guard row has guardBreakRate 0 still deflects player attacks, because
    // every AtkParam_Pc row has guardAtkRate 0.
    assert_eq!(npc_guard_repel(0, &[]), 0);
    let window = [effect(set, 105010)];
    assert_eq!(npc_guard_repel(50, &window), 50);
}

#[test]
fn posture_damage_to_a_neutral_npc_defender() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    let npc = DefenderPosture::default();
    let window = [effect(set, 105010), effect(set, 105020)];
    assert_eq!(
        defender_posture_damage(HitOutcome::Hit, &atk, 1.0, &npc, &[]),
        18
    );
    assert_eq!(
        defender_posture_damage(HitOutcome::Block, &atk, 1.0, &npc, &[]),
        18
    );
    assert_eq!(
        defender_posture_damage(HitOutcome::Deflect, &atk, 1.0, &npc, &window),
        9
    );
}

#[test]
fn soldier_guard_posture_includes_cut_and_behavior_cost() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    // A hypothetical NPC with staminaGuardDef 50 and a guard behaviour costing 3: (1 - 0.5) * 18 + 3.
    let npc = DefenderPosture {
        attribute_rate: 1.0,
        guard_def: 50.0,
        cut_bias: 0.0,
        guard_row_cut_rate: 0,
        guard_behavior_stamina: 3,
    };
    assert_eq!(
        defender_posture_damage(HitOutcome::Block, &atk, 1.0, &npc, &[]),
        12
    );
}

#[test]
fn repeated_deflects_break_the_soldier() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    let npc = soldier(set);
    let mut meter = PostureMeter::full(npc.max_posture, npc.max_debt_stamina);
    let window = effect(set, 105010);
    let mut expected = [(45, 30, false), (22, 8, false), (11, -3, true)].into_iter();
    for deflect_effect in [105020, 105021, 105022] {
        let effects = [window.clone(), effect(set, deflect_effect)];
        let dmg = attacker_posture_damage(HitOutcome::Deflect, &atk, false, 1.0, &effects);
        let broke = attacker_breaks(&meter, dmg);
        meter.take(dmg, None);
        let (want_dmg, want_left, want_broke) = expected.next().unwrap();
        assert_eq!(
            (dmg, meter.remaining, broke),
            (want_dmg, want_left, want_broke)
        );
        let report = attacker_reaction(
            HitOutcome::Deflect,
            false,
            0,
            false,
            broke,
            atk.does_break_repel_stam_damage,
        )
        .unwrap();
        assert_eq!(report.0, if broke { 1033 } else { 1000 });
        let defender = defender_reaction(DefenderFacts {
            outcome: HitOutcome::Deflect,
            dead: false,
            guard_broken: false,
            guard_break_reaction: false,
            damage_level: 2,
            broke: false,
            attacker_broke: broke,
            does_break_repel_stam_damage: atk.does_break_repel_stam_damage,
        });
        assert_eq!(defender.code(), if broke { 1028 } else { 3 });
    }
    assert!(meter.broken());
    // The fourth deflect effect would send back 45 * 0.125 = 5.625, truncated to 5.
    let effects = [window, effect(set, 105023)];
    assert_eq!(
        attacker_posture_damage(HitOutcome::Deflect, &atk, false, 1.0, &effects),
        5
    );
}

#[test]
fn posture_floor_and_debt() {
    let Some(set) = set() else { return };
    let npc = soldier(set);
    let mut meter = PostureMeter::full(npc.max_posture, npc.max_debt_stamina);
    meter.take(200, None);
    assert_eq!(meter.remaining, -30);
    // The player's floor is TentativePlayerParam DebtSp, 0.
    let player = set.table("TentativePlayerParam").unwrap().row(0).unwrap();
    let mut wolf = PostureMeter::full(100, player.int("DebtSp").unwrap() as i32);
    wolf.take(200, None);
    assert_eq!(wolf.remaining, 0);
}

#[test]
fn deflect_never_breaks_the_defender() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    let mut wolf = PostureMeter::full(100, 0);
    wolf.remaining = 5;
    let c = apply_defender_posture(&mut wolf, HitOutcome::Deflect, 9, &atk, None);
    assert_eq!((c.projected, wolf.remaining, c.broke), (0, 0, false));
    // The next block breaks the guard.
    let c = apply_defender_posture(&mut wolf, HitOutcome::Block, 18, &atk, None);
    assert!(c.broke);
    let report = defender_reaction(DefenderFacts {
        outcome: HitOutcome::Block,
        dead: false,
        guard_broken: c.broke,
        guard_break_reaction: false,
        damage_level: 2,
        broke: false,
        attacker_broke: false,
        does_break_repel_stam_damage: true,
    });
    assert_eq!(report.code(), 1001);
    // A direct hit that empties posture is a posture break (1027).
    let mut soldier_meter = PostureMeter::full(75, -30);
    soldier_meter.remaining = 10;
    let c = apply_defender_posture(&mut soldier_meter, HitOutcome::Hit, 18, &atk, None);
    assert!(c.broke);
    let report = defender_reaction(DefenderFacts {
        outcome: HitOutcome::Hit,
        dead: false,
        guard_broken: false,
        guard_break_reaction: false,
        damage_level: 2,
        broke: c.broke,
        attacker_broke: false,
        does_break_repel_stam_damage: true,
    });
    assert_eq!(report.code(), 1027);
}

#[test]
fn disable_stamina_attack_still_runs_the_break_check() {
    let Some(set) = set() else { return };
    let mut atk = soldier_attack(set);
    atk.disable_stamina_attack = true;
    let mut meter = PostureMeter::full(75, -30);
    meter.remaining = 5;
    let c = apply_defender_posture(&mut meter, HitOutcome::Hit, 18, &atk, None);
    assert_eq!((c.applied, meter.remaining, c.broke), (0, 5, true));
}

#[test]
fn stamina_control_ranges() {
    let Some(set) = set() else { return };
    let row = set
        .table("StaminaControlParam")
        .unwrap()
        .row(1000100)
        .unwrap();
    let control = StaminaControl::from_row(&row).unwrap();
    // Type 6 pins posture to 30% of max: 30 * 75 / 100 = 22.
    let range = ControlRange::new(&control, 6, 75).unwrap();
    assert_eq!((range.low, range.high), (22, 22));
    let mut meter = PostureMeter::full(75, -30);
    meter.set(75, false, Some(range));
    assert_eq!(meter.remaining, 22);
    // Type 7 keeps it full (min 100%): damage cannot lower it.
    let full = ControlRange::new(&control, 7, 75).unwrap();
    let mut meter = PostureMeter::full(75, -30);
    meter.take(40, Some(full));
    assert_eq!(meter.remaining, 75);
}

#[test]
fn history_keeps_most_of_a_combat_loss() {
    let mut meter = PostureMeter::full(75, -30);
    meter.take(18, None);
    // ceil(0.2 * -18) = -3.
    assert_eq!((meter.remaining, meter.history), (57, 72));
    meter.set(40, false, None);
    assert_eq!(meter.history, 55);
}

#[test]
fn soldier_recovery_by_hp() {
    let Some(set) = set() else { return };
    let npc = soldier(set);
    let resident: Vec<ActiveEffect> = npc
        .sp_effect_ids
        .iter()
        .filter(|&&id| id > 0)
        .filter_map(|&id| set.table("SpEffectParam").unwrap().find(id))
        .map(|row| ActiveEffect::from_row(&row).unwrap())
        .collect();
    let rate_at = |hp: i32| {
        let active: Vec<ActiveEffect> = resident
            .iter()
            .filter(|e| e.hp_gate_open(hp, npc.hp))
            .cloned()
            .collect();
        posture_recovery_per_second(RecoveryInputs::base(npc.stamina_recover_base_vel), &active)
    };
    assert_eq!(rate_at(195), 20.0);
    assert!((rate_at(150) - 12.0).abs() < 1e-4, "{}", rate_at(150));
    assert!((rate_at(110) - 6.0).abs() < 1e-4, "{}", rate_at(110));
    assert!((rate_at(60) - 2.0).abs() < 1e-4, "{}", rate_at(60));
    assert_eq!(displayed_recovery(rate_at(195)), 20);

    // Control type from the animation: type 10 doubles it, type 0 stops it.
    let control = StaminaControl::from_row(
        &set.table("StaminaControlParam")
            .unwrap()
            .row(npc.stamina_control_param_id as i32)
            .unwrap(),
    )
    .unwrap();
    let mut inputs = RecoveryInputs::base(npc.stamina_recover_base_vel);
    inputs.control_ratio = Some(control.recover_ratio(10));
    assert_eq!(posture_recovery_per_second(inputs, &[]), 40.0);
    inputs.control_ratio = Some(control.recover_ratio(0));
    assert_eq!(posture_recovery_per_second(inputs, &[]), 0.0);
}

#[test]
fn recovery_accumulates_whole_points() {
    let mut acc = RecoveryAccumulator::default();
    let mut meter = PostureMeter::full(75, -30);
    meter.remaining = 0;
    for _ in 0..30 {
        let gain = acc.tick(20.0, 1.0 / 30.0);
        meter.set(meter.remaining + gain, false, None);
    }
    // 20 points per second for one second, minus whatever the f32 carry still holds.
    assert!((19..=20).contains(&meter.remaining), "{}", meter.remaining);
    assert!(acc.carry < 1.0);
}

#[test]
fn hp_damage_and_deathblows() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    // Unguarded soldier slash on a defender with neutral multipliers: 80.
    let total = element_damage(atk.attack_power, [0.0; 5], [1.0; 5], 1.0);
    assert_eq!(final_hp_damage(total, 1.0, 1.0), 80);
    // Blocked by a full physical guard cut (physGuardCutRate 100): nothing gets through.
    let cut = npc_guard_cut(100.0, 0, 1.0);
    let total = element_damage(atk.attack_power, [0.0; 5], [cut, 1.0, 1.0, 1.0, 1.0], 1.0);
    assert_eq!(final_hp_damage(total, 1.0, 1.0), 0);
    // A tiny positive total rounds up to 1.
    assert_eq!(
        element_damage([0.2, 0.0, 0.0, 0.0, 0.0], [0.0; 5], [1.0; 5], 1.0),
        1.0
    );
    // excessDmgKeepHp.
    assert_eq!(keep_hp(100, 150, 1), 99);
    assert_eq!(keep_hp(100, 50, 1), 50);
    assert_eq!(keep_hp(1, 50, 1), 0);

    // Ashina general (NpcParam 10219000): two deathblows, HP 1918.
    let general =
        NpcCombat::from_row(&set.table("NpcParam").unwrap().row(10219000).unwrap()).unwrap();
    let mut v = Vitality::new(general.hp, general.deathblows);
    v.take(5000, false);
    assert_eq!(v.hp, 1);
    assert!(v.deathblow_ready());
    v.take(5000, true);
    assert_eq!((v.deathblows_left, v.hp), (1, 1));
    v.take(5000, true);
    assert_eq!((v.deathblows_left, v.hp), (0, 0));
    // A soldier has no deathblow protection.
    let mut s = Vitality::new(195, 0);
    s.take(80, false);
    assert_eq!(s.hp, 115);
    s.take(500, false);
    assert_eq!(s.hp, 0);
}

#[test]
fn discord_claims_for_the_general() {
    let Some(set) = set() else { return };
    let general =
        NpcCombat::from_row(&set.table("NpcParam").unwrap().row(10219000).unwrap()).unwrap();
    assert_eq!(
        (
            general.hp,
            general.max_posture,
            general.stamina_recover_base_vel
        ),
        (1918, 600, 60.0)
    );
    let rates: Vec<f32> = [105020, 105021, 105022, 105023]
        .iter()
        .map(|&id| effect(set, id).def_stamina_attack_rate)
        .collect();
    assert_eq!(rates, [1.0, 0.5, 0.25, 0.125]);
}

#[test]
fn wolf_guard_posture_includes_the_player_cut_bias() {
    let Some(set) = set() else { return };
    let atk = soldier_attack(set);
    let weapon = kusabimaru(set);
    // Kusabimaru staminaGuardDef 0: the player's cut is still 1% from the constant bias, so
    // trunc(0.99 * 18) = 17 on a block and trunc(0.99 * 9) = 8 on a deflect.
    let block = DefenderPosture::player(&weapon, false, 1.0);
    assert_eq!(
        defender_posture_damage(HitOutcome::Block, &atk, 1.0, &block, &[]),
        17
    );
    let deflect = DefenderPosture::player(&weapon, true, 1.0);
    let window = [effect(set, 105010), effect(set, 105020)];
    assert_eq!(
        defender_posture_damage(HitOutcome::Deflect, &atk, 1.0, &deflect, &window),
        8
    );
}

#[test]
fn player_attack_repel_and_posture_base() {
    let Some(set) = set() else { return };
    let weapon = kusabimaru(set);
    assert_eq!(weapon.attack_base_repel, 10);
    let table = set.table("AtkParam_Pc").unwrap();
    let row = table
        .rows()
        .find(|r| r.u16("guardAtkRateCorrection").unwrap() == 300)
        .expect("a PC attack with correction 300");
    let atk = AttackProfile::from_row(&row).unwrap();
    assert_eq!(atk.guard_atk_rate, 0);
    assert_eq!(player_attack_repel(&atk, weapon.attack_base_repel), 30);
    // With attackBaseStamina 0 the weapon part vanishes and the fixed field remains.
    let base = player_posture_base(HitOutcome::Hit, &atk, &weapon, 1.0, 1.0);
    assert_eq!(
        base,
        atk.direct_stam as f32 * weapon.stamina_attack_power_rate
    );
}

#[test]
fn armoured_parts_guard_player_attacks() {
    let Some(set) = set() else { return };
    let mut part = soldier_attack(set);
    part.guard_break_rate = 50;
    let mut player_atk = soldier_attack(set);
    player_atk.guard_atk_rate = 0;
    // Part repel 50 >= player repel 30: deflected by the part.
    let r = resolve_part_hit(&player_atk, 30, &part, &[]).unwrap();
    assert_eq!(r.outcome, HitOutcome::Deflect);
    // Part repel 50 < 60: blocked by the part.
    let r = resolve_part_hit(&player_atk, 60, &part, &[]).unwrap();
    assert_eq!(r.outcome, HitOutcome::Block);
    // An attack whose own guardAtkRate reaches the part's guardBreakRate ignores the part.
    player_atk.guard_atk_rate = 50;
    assert!(resolve_part_hit(&player_atk, 30, &part, &[]).is_none());
}

#[test]
fn guard_cuts_for_hp() {
    let Some(set) = set() else { return };
    let npc = soldier(set);
    let atk = soldier_attack(set);
    // Slash (atkAttribute 1) against the soldier's slashGuardCutRate 100 doubles its cut.
    let rate = npc.guard_cut_attribute_rates[atk.atk_attribute as usize - 1];
    assert_eq!(rate, 100);
    let factor = attribute_guard_factor(rate as f32);
    assert_eq!(factor, 2.0);
    assert!(npc_guard_cut(npc.phys_guard_cut_rate, 0, factor) < 0.0);
    // The Kusabimaru blocks all physical damage.
    let weapon = kusabimaru(set);
    let wf = attribute_guard_factor(weapon.guard_cut_attribute_rates[0] as f32);
    assert_eq!(
        player_guard_cut(weapon.phys_guard_cut_rate, 1.0, wf, 0),
        0.0
    );
    assert_eq!(
        player_guard_cut(weapon.phys_just_guard_cut_rate, 1.0, wf, 0),
        0.0
    );
}

#[test]
fn player_base_recovery_graph() {
    let Some(set) = set() else { return };
    let graph =
        CalcCorrectGraph::from_row(&set.table("CalcCorrectGraph").unwrap().row(504).unwrap())
            .unwrap();
    assert_eq!(player_base_speed(&graph, 1.0), 30.0);
    assert_eq!(player_base_speed(&graph, 6.0), 67.0);
    assert_eq!(player_base_speed(&graph, 11.0), 105.0);
    assert_eq!(player_base_speed(&graph, 50.0), 105.0);
}
