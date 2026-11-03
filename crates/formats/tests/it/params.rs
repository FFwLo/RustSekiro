//! Checks the PARAM reader against the player's own unpacked game data in `cache/`.
//! Values are the ones decoded independently by the earlier sekiro-deflection project
//! (docs/BASELINE-COMBAT-FIXTURE.md, COMBAT-RULES-RESEARCH.md, LOCAL-DATA-EVIDENCE.md).
//! Skips when `cache/` has not been extracted.

use std::path::PathBuf;
use std::sync::OnceLock;

use sekiro_formats::param::ParamSet;
use sekiro_formats::param::typed::CombatParams;

fn root() -> PathBuf {
    sekiro_formats::cache::root()
}

fn set() -> Option<&'static ParamSet> {
    static SET: OnceLock<Option<ParamSet>> = OnceLock::new();
    SET.get_or_init(|| {
        let dir = root().join("raw/param/gameparam/gameparam.parambnd.d");
        let defs = root().join("refs/paramdex/Defs");
        if !dir.is_dir() || !defs.is_dir() {
            eprintln!("skipping: cache/ not extracted (run sekiro-extract unpack and fetch-refs)");
            return None;
        }
        Some(ParamSet::load(&dir, &defs).expect("load params"))
    })
    .as_ref()
}

#[test]
fn every_gameparam_table_loads() {
    let Some(set) = set() else { return };
    let fatal: Vec<_> = set.issues().iter().filter(|i| i.is_fatal()).collect();
    assert!(fatal.is_empty(), "tables failed: {fatal:#?}");
    assert!(set.len() > 100, "only {} tables", set.len());
}

#[test]
fn soldier_attack_posture_fields() {
    let Some(set) = set() else { return };
    let row = set.table("AtkParam_Npc").unwrap().row(10100100).unwrap();
    assert_eq!(row.int("atkStam").unwrap(), 9);
    assert_eq!(row.int("repelLostStamDamage").unwrap(), 18);
    assert_eq!(row.int("directAtkStamDamage").unwrap(), 18);
    assert_eq!(row.int("directAtkStamDamage_Attacker").unwrap(), 0);
    assert_eq!(row.int("repelVictoryStamDamage_Attacker").unwrap(), 0);
    assert_eq!(row.int("repelLostStamDamage_Attacker").unwrap(), 45);
    assert_eq!(row.int("staminaDamageAttackHitParry").unwrap(), 90);
    assert!(row.bool("doesBreakRepelStamDamage").unwrap());
}

#[test]
fn soldier_behavior_and_npc() {
    let Some(set) = set() else { return };
    let behavior = set.table("BehaviorParam").unwrap().row(210100100).unwrap();
    assert_eq!(behavior.i32("variationId").unwrap(), 10100);
    assert_eq!(behavior.i32("behaviorJudgeId").unwrap(), 100);
    assert_eq!(behavior.u8("refType").unwrap(), 0);
    assert_eq!(behavior.i32("refId").unwrap(), 10100100);

    let npc = set.table("NpcParam").unwrap().row(10100000).unwrap();
    assert_eq!(npc.u32("hp").unwrap(), 195);
    assert_eq!(npc.u16("stamina").unwrap(), 75);
    assert_eq!(npc.u16("staminaRecoverBaseVel").unwrap(), 20);
    assert_eq!(npc.u32("staminaControlParamId").unwrap(), 1000100);
    assert_eq!(npc.i32("behaviorVariationId").unwrap(), 10100);
    assert_eq!(npc.i32("spEffectID28").unwrap(), 300600);
    assert_eq!(npc.i32("spEffectID29").unwrap(), 300601);
    assert_eq!(npc.i32("spEffectID30").unwrap(), 300602);
}

#[test]
fn deflect_effects() {
    let Some(set) = set() else { return };
    let sp = set.table("SpEffectParam").unwrap();
    for (id, rate) in [
        (105020, 1.0),
        (105021, 0.5),
        (105022, 0.25),
        (105023, 0.125),
    ] {
        let row = sp.row(id).unwrap();
        assert_eq!(row.f32("defStaminaAttackRate").unwrap(), rate, "{id}");
        assert_eq!(row.int("stateInfo").unwrap(), 204, "{id}");
        assert_eq!(row.f32("effectEndurance").unwrap(), 0.0, "{id}");
    }
    assert_eq!(sp.row(105010).unwrap().int("stateInfo").unwrap(), 158);
    for (id, hp, rate) in [
        (300600, 80.0, 0.6),
        (300601, 60.0, 0.5),
        (300602, 40.0, 1.0 / 3.0),
    ] {
        let row = sp.row(id).unwrap();
        assert_eq!(row.f32("conditionHp").unwrap(), hp, "{id}");
        assert!((row.f32("staminaRecoverSpeedRate").unwrap() - rate).abs() < 1e-6);
        assert_eq!(row.f32("effectEndurance").unwrap(), -1.0, "{id}");
    }
}

#[test]
fn typed_tables_match_generic_rows() {
    let Some(set) = set() else { return };
    let combat = CombatParams::from_set(set).expect("typed combat params");
    let atk = combat.atk_npc.get(10100100).unwrap();
    assert_eq!(atk.atk_stam, 9);
    assert_eq!(atk.repel_lost_stam_damage, 18);
    assert_eq!(atk.repel_lost_stam_damage_attacker, 45);
    assert!(atk.does_break_repel_stam_damage);
    let behavior = combat.behavior_npc.get(210100100).unwrap();
    assert_eq!(behavior.ref_id, 10100100);
    let npc = combat.npc.get(10100000).unwrap();
    assert_eq!((npc.hp, npc.stamina), (195, 75));
    assert_eq!(npc.sp_effect_ids[28..31], [300600, 300601, 300602]);
    let control = combat
        .stamina_control
        .get(npc.stamina_control_param_id as i32)
        .unwrap();
    assert_eq!(control.recover_ratio[0], 0);
    assert_eq!(control.recover_ratio[10], 200);
    assert_eq!(
        combat
            .sp_effect
            .get(105021)
            .unwrap()
            .def_stamina_attack_rate,
        0.5
    );
}

#[test]
fn row_names_come_from_paramdex() {
    let Some(set) = set() else { return };
    let named = set
        .table("SpEffectParam")
        .unwrap()
        .rows()
        .filter(|r| r.name().is_some())
        .count();
    assert!(named > 0);
}
