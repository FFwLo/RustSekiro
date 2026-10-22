//! Sim tests, part 6: 35 tests, from `lightning_on_the_ground_shocks_wolf` to `the_bandits_torch_sets_wolf_on_fire`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// Round 43: Genichiro's lightning (c7110 a000_003042, AtkParam 71100361 with the charge
/// SpEffect 9492) striking Wolf on the ground shocks him at once: 9492 -> 9412 -> 9432 (160 +
/// 30 % of max HP, posture) -> 9450 (ref 593) W_GroundDamageElectricShockWeak. Before, a plain
/// hit.
#[test]
pub(crate) fn lightning_on_the_ground_shocks_wolf() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    let mut app = app_with("c7110");
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003042");
    }
    let (hp0, max) = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        (a.hp, a.hp_max)
    };
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..240 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
        if seen.iter().any(|s| s == "GroundDamageElectricShockWeak") {
            break;
        }
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!(seen.iter().any(|s| s == "GroundDamageElectricShockWeak"), "{seen:?}");
    assert!(hp0 - a.hp >= 160.0 + max * 0.30 - 0.5, "hp {hp0} -> {} (max {max})", a.hp);
}

/// The Lightning Reversal's hit (AtkParam_Pc 5000281: atkPhys 0, spEffectId0 9400 -> its cycle
/// 9420, registFreeze 200 vs the Way of Tomoe's resist_freeze 100) fills her shock gauge at once:
/// 9420's 67 % of 2556 + 320 = 2032, x her resident 9461's freezeDamageRate 25 % = 508 HP, and
/// 180 + 5 % posture (status.rs STATE_INFO_FREEZE). Before, 9400 was a 0 s row doing nothing and
/// the reversal took no HP at all.
#[test]
pub(crate) fn the_reversal_takes_a_fifth_of_tomoes_bar() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    let mut app = app_with("c7110");
    app.update();
    let (ent, hp0, posture0, max, pmax) = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &Actor), (With<Enemy>, Without<crate::enemy::Cast>)>();
        let (e, a) = q.single(world).unwrap();
        (e, a.hp, a.posture, a.hp_max, a.posture_max)
    };
    assert_eq!(max, 2556.0, "the Way of Tomoe's NpcParam hp");
    let lines = app.world_mut().resource_scope::<crate::status::Statuses, _>(|world, mut statuses| {
        world.resource_scope::<Combat, _>(|world, combat| {
            let a = world.get::<Actor>(ent).unwrap();
            statuses.hit(&combat, ent, a, &[9400], false)
        })
    });
    assert!(lines.iter().any(|l| l == "SHOCK"), "{lines:?}");
    app.update();
    let a = app.world().get::<Actor>(ent).unwrap();
    let took = hp0 - a.hp;
    assert!((took - 508.0).abs() < 2.0, "took {took} of {max}");
    let posture = a.posture - posture0;
    assert!((posture - (180.0 + pmax * 0.05)).abs() < 1.0, "posture +{posture} (max {pmax})");
}

/// The Way of Tomoe's lightning arrow (a000_003039 BulletBehavior 521 at 1.87 s: Bullet 71100521
/// spAttribute 10, spEffectId1 9492; its AtkParam 71100521 has no SpEffect and no atkPhys) shocks
/// Wolf on the ground through the Bullet row's charge: GroundDamageElectricShockWeak and 9432's
/// 160 + 30 % HP. Before, the bullet's 9492 went to a timed row nothing read and the arrow landed
/// as a plain hit for 0.
#[test]
pub(crate) fn a_lightning_arrow_shocks_wolf_on_the_ground() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    let mut app = app_with("c7110");
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003039");
    }
    let (hp0, max) = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        (a.hp, a.hp_max)
    };
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..300 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
        if seen.iter().any(|s| s == "GroundDamageElectricShockWeak") {
            break;
        }
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!(seen.iter().any(|s| s == "GroundDamageElectricShockWeak"), "{seen:?}");
    assert!(hp0 - a.hp >= 160.0 + max * 0.30 - 0.5, "hp {hp0} -> {} (max {max})", a.hp);
}

/// Genichiro's lightning (c7110 a000_003042 f50 AttackBehavior 361: AtkParam 71100361
/// spAttribute 10) catching Wolf in a jump: he is charged (AirDamageElectroChargeWeakStart,
/// no HP lost), and an attack before landing throws it back (AirWeakElectroReceiveAttack).
#[test]
pub(crate) fn lightning_caught_in_the_air_charges_wolf_and_an_attack_throws_it_back() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    // The jump's timing against the strike (f50 = 1.67 s): try a few take-offs. The weak charge
    // (a000_102930) lifts by row 1612 (+3.2 m/s, the fall kept at half) and takes the release
    // only from 0.7 s (flag 115), so a catch near the top of the jump lands first.
    let mut tries = vec![];
    for jump_at in [0.95f32, 1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.35, 1.45] {
        let mut app = app_with("c7110");
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.force_anim(&mut a, "a000_003042");
        }
        let hp0 = {
            let world = app.world_mut();
            world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp
        };
        let mut jumped = false;
        let mut caught = false;
        for _ in 0..240 {
            let t = enemy_state(&mut app).2;
            if !jumped && t >= jump_at {
                app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
                jumped = true;
            }
            app.update();
            if player_state(&mut app).0.starts_with("AirDamageElectroCharge") {
                caught = true;
                break;
            }
        }
        if !caught {
            tries.push((jump_at, vec![]));
            continue;
        }
        let s = player_state(&mut app).0;
        assert_eq!(s, "AirDamageElectroChargeWeakStart");
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        assert!(a.hp >= hp0 && a.electro.is_some_and(|c| (9490..=9494).contains(&c.0)), "hp {} -> {}, charge {:?}", hp0, a.hp, a.electro);
        let mut seen = vec![s];
        for i in 0..240 {
            if i % 5 == 0 && !seen.iter().any(|s| s == "AirWeakElectroReceiveAttack") {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        if seen.iter().any(|s| s == "AirWeakElectroReceiveAttack") {
            // The release lands into its own Land anim, not the shock (HKS 1920-1925).
            assert!(seen.iter().any(|s| s == "LandAirWeakElectroReceiveAttack") && !log_text(&app).contains("SHOCKED"), "{seen:?}
{}", log_text(&app));
            let world = app.world_mut();
            assert!(world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().electro.is_none(), "still charged: {seen:?}");
            return;
        }
        tries.push((jump_at, seen));
    }
    panic!("no release: {tries:?}");
}

#[test]
pub(crate) fn a_jump_in_an_updraft_is_the_storm_jump() {
    // The Divine Dragon's updraft bullets give Wolf 106100 (ref 110003, 0.5 s): a jump there is
    // W_GroundStormJumpReady -> GroundStormJumpStart, launched by TAE 922 row 980 (+25 m/s); in
    // the air, from 1.0 s (ref 101), a second press is W_AirStormJumpStart; the fall lands in
    // W_LandStormJumpFall.
    let mut app = app();
    let updraft = |app: &mut App| {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.timed.retain(|(id, _)| *id != 106100);
        p.timed.push((106100, 0.5));
    };
    move_enemy_far(&mut app);
    updraft(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut seen, mut top, mut air_jump) = (vec![], 0.0f32, false);
    let y0 = player_actor(&mut app).3.y;
    for i in 0..400 {
        let (s, t) = { let ps = player_state(&mut app); (ps.0, ps.2) };
        if s == "GroundStormJumpStart" && t > 1.1 && !air_jump {
            updraft(&mut app);
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
            air_jump = true;
        }
        if i < 20 {
            updraft(&mut app);
        }
        app.update();
        top = top.max(player_actor(&mut app).3.y - y0);
        if std::env::var("SHINOBI_DBG").is_ok() && i % 4 == 0 {
            let (st, vy, air, pos) = player_actor(&mut app);
            eprintln!("{i} {st} t {:.2} vy {vy:.2} air {air} y {:.2}", player_state(&mut app).2, pos.y - y0);
        }
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    for want in ["GroundStormJumpReady", "GroundStormJumpStart", "AirStormJumpStart", "LandStormJumpFall"] {
        assert!(seen.iter().any(|s| s == want), "no {want}: {seen:?}");
    }
    assert!(top > 8.0, "rose only {top:.1} m: {seen:?}");
}

/// The Divine Dragon fight (m25 events, NpcParam 52000000) up to Wolf being airborne in an
/// updraft once the fight runs (12505888 set).
#[cfg(test)]
pub(crate) fn dragon_app() -> Option<App> {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5310.json")).exists() {
        return None;
    }
    let mut app = app_cfg("c5200", Some(52000000), Vec::new());
    for i in 0..(60 * 7) {
        if i == 2 {
            let world = app.world_mut();
            for mut e in world.query::<&mut Enemy>().iter_mut(world) {
                e.aggressive = true;
            }
        }
        app.update();
    }
    Some(app)
}

/// m25 12505901 (one per tree, 1 s after the phase flag 12505888): the Divine Dragon's five
/// trees, c5310 2500880-84, are warped onto its dummy polys 50 / 53 / 54 / 55 / 56 (2004[42] Warp
/// Character and Copy Floor), facing each dummy's +Z. They were left at their MSB spot (883 / 884
/// were not even in the cast), so Wolf had nothing to grapple.
#[test]
pub(crate) fn the_divine_dragons_trees_stand_on_its_dummies() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5310.json")).exists() {
        return;
    }
    let mut app = app_geom("c5200", Some(52000000));
    let find = |app: &mut App, id: i64| {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &crate::enemy::script::Scripted)>();
        q.iter(world).find(|(_, s)| s.0 == id).map(|(e, _)| e)
    };
    let pairs = [(2500880, 50), (2500881, 53), (2500882, 54), (2500883, 55), (2500884, 56)];
    let mut last = Vec::new();
    for i in 0..(60 * 12) {
        if i == 2 {
            let world = app.world_mut();
            for mut e in world.query::<&mut Enemy>().iter_mut(world) {
                e.aggressive = true;
            }
        }
        app.update();
        let Some(dragon) = find(&mut app, 2500800) else { continue };
        last.clear();
        for (tree, dmy) in pairs {
            let Some(t) = find(&mut app, tree) else { break };
            let world = app.world();
            let Some(m) = world.get::<crate::model::Dummies>(dragon).and_then(|d| crate::combat::dmy_entity(d, dmy)) else { break };
            let at = world.get::<GlobalTransform>(m).unwrap().translation();
            last.push((tree, world.get::<Transform>(t).unwrap().translation.distance(at)));
        }
        if last.len() == pairs.len() && last.iter().all(|(_, d)| *d < 0.05) {
            return;
        }
    }
    panic!("trees off the dragon's dummies: {last:?}");
}

/// The trees' grapple points: m25 12505901 attaches o259920-922 to each tree's dummies 600-602
/// (2003[58]) and switches them off (2005[3]); 12505911 asks the trees to bloom (AI request 10 ->
/// AttackImmediateAction 20000 / 20002 / 20004, HKS -> loop Event21000-21002 with 5025-5027),
/// and 12505906 switches on the point of the bloom. Each object carries four grapple records
/// (o259920 / 921 variation 45 -> ChrPhysicsHoming 300053105), ridden like a map point.
#[test]
pub(crate) fn the_divine_dragons_trees_open_grapple_points_when_they_bloom() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5310.json")).exists() {
        return;
    }
    let mut app = app_geom("c5200", Some(52000000));
    let trees = [2500880, 2500881, 2500882, 2500883, 2500884];
    let mut bloomed = false;
    for i in 0..(60 * 40) {
        if i == 2 {
            let world = app.world_mut();
            for mut e in world.query::<&mut Enemy>().iter_mut(world) {
                e.aggressive = true;
            }
        }
        app.update();
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &crate::enemy::script::Scripted, &Actor, &Transform)>();
        let tree_of: Vec<(Entity, String, Vec3)> = q.iter(world).filter(|(_, s, _, _)| trees.contains(&s.0)).map(|(e, _, a, t)| (e, a.state.clone(), t.translation)).collect();
        bloomed |= tree_of.iter().any(|(_, st, _)| st.starts_with("Event2100") || matches!(st.as_str(), "Ez20000" | "Ez20002" | "Ez20004"));
        let g: Vec<_> = world.resource::<crate::enemy::grapple::EnemyGrapples>().0.iter().filter(|g| g.object).cloned().collect();
        if !bloomed {
            assert!(g.is_empty(), "frame {i}: points before any bloom {g:?}");
            continue;
        }
        if g.is_empty() {
            continue;
        }
        // Only the bloom's point (5025 -> o259920 on 600, 5026 -> 921 on 601, 5027 -> 922 on
        // 602): four records on that tree, the other two objects stay off.
        for (t, st, _) in &tree_of {
            let n = g.iter().filter(|p| p.entity == *t).count();
            assert!(n <= 4, "frame {i}: {n} points on one tree ({st})");
            assert!(n == 0 || st.starts_with("Event2100") || st.starts_with("Ez2000"), "frame {i}: points on a tree in {st}");
        }
        for p in &g {
            let (_, st, at) = tree_of.iter().find(|t| t.0 == p.entity).unwrap_or_else(|| panic!("point on a non-tree {p:?}"));
            assert!(p.hook.distance(*at) < 12.0 && p.land.distance(*at) < 12.0, "frame {i}: {p:?} far from its tree at {at:?} ({st})");
            assert!(p.homing_row == 300_053_105 || (p.variation == 0 && p.homing_row == 300_000_000), "{p:?}");
        }
        // Wolf takes it as a ridden map point: the search finds it from a camera looking at it.
        let p = &g[0];
        let wolf = p.hook + Vec3::new(0.0, -3.0, 8.0);
        let pt = crate::traversal::GrapplePoint { hook: p.hook, land: p.land, fwd: (wolf - p.hook).with_y(0.0).normalize(), variation: p.variation };
        let cam = wolf + Vec3::new(0.0, 1.5, 3.0);
        let t = crate::traversal::grapple_target(cam, (p.hook - cam).normalize(), wolf, None, &[pt]).expect("the point is not found");
        assert!(t.ride && t.in_action_range, "{t:?}");
        return;
    }
    panic!("no tree grapple point (bloomed: {bloomed})");
}

#[test]
pub(crate) fn the_divine_dragons_lightning_comes_back_at_it() {
    // m25 12505916: Wolf touching a lightning sign (53100681: 3531061, 2 s) brings the dragon's
    // strong lightning 252000555 (radius 150, AtkParam 52000510 spAttribute 6); caught in the
    // air it charges him (9495), and his release's bolt 500184 puts 3520085 on the dragon, which
    // with a hit on its head (part 2 -> PartBlend_Add02 a000_009511: 3520081) staggers it
    // (12505927 slot 1 -> 20004). At 30 % HP (12505889 -> 12505890) the same brings its collapse
    // (12505887: 20002, then Event21000 with 932 / 104), where Wolf's finisher 15200090 ends the
    // fight (ThrowDef12900; 12505880 Handle Boss Defeat).
    let Some(mut app) = dragon_app() else { return };
    let dragon = {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &crate::enemy::script::Scripted)>();
        q.iter(world).find(|(_, s)| s.0 == 2500800).map(|(e, _)| e).unwrap()
    };
    // A "head" capsule (part 2) at its root (no model here).
    let dpos = app.world().get::<Transform>(dragon).unwrap().translation;
    {
        let world = app.world_mut();
        let ma = world.spawn((Transform::from_translation(dpos), GlobalTransform::from_translation(dpos))).id();
        let mb = world.spawn((Transform::from_translation(dpos + Vec3::Y * 4.0), GlobalTransform::from_translation(dpos + Vec3::Y * 4.0))).id();
        world.entity_mut(dragon).insert(crate::anim::Hurtboxes(vec![(ma, mb, 3.0)], vec![2]));
    }
    let set_timed = |app: &mut App, id: i64, secs: f32| {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.timed.retain(|(x, _)| *x != id);
        p.timed.push((id, secs));
    };
    let dragon_anim = |app: &App| app.world().get::<Actor>(dragon).unwrap().anim.clone();
    // Wolf 12 m from it, up in an updraft (both storm jumps), the sign on the way down, the
    // release locked on and facing it. Until the dragon plays one of `want`.
    let reversal = |app: &mut App, want: &[&str]| -> Vec<String> {
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
            let (mut a, mut t) = q.single_mut(world).unwrap();
            t.translation = Vec3::new(dpos.x, t.translation.y, dpos.z + 12.0);
            a.yaw = 0.0;
        }
        set_timed(app, 106100, 0.5);
        app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        let mut seen: Vec<String> = Vec::new();
        let (mut signed, mut released, mut air_jump) = (false, false, false);
        for i in 0..(60 * 8) {
            if i < 20 {
                set_timed(app, 106100, 0.5);
            }
            let (s, t) = { let ps = player_state(app); (ps.0, ps.2) };
            if s == "GroundStormJumpStart" && t > 1.1 && !air_jump {
                set_timed(app, 106100, 0.5);
                app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
                air_jump = true;
            }
            if s == "StormJumpFall" && !signed {
                set_timed(app, 3531061, 2.0);
                signed = true;
            }
            let charged = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().electro.is_some();
            if charged && !released && i % 5 == 0 {
                app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(dragon);
                let world = app.world_mut();
                let mut q = world.query_filtered::<(&mut Actor, &Transform), With<Player>>();
                let (mut a, t) = q.single_mut(world).unwrap();
                let to = (dpos - t.translation).with_y(0.0);
                a.yaw = f32::atan2(-to.x, -to.z);
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let s = player_state(app).0;
            if s.contains("ElectroReceiveAttack") {
                released = true;
            }
            if seen.last() != Some(&s) {
                seen.push(s);
            }
            let da = dragon_anim(app);
            if want.iter().any(|w| da.ends_with(w)) {
                return seen;
            }
        }
        let world = app.world_mut();
        let d = world.query::<(&Actor, &Enemy)>().get(world, dragon).map(|(a, e)| format!("{} {} add {} clash {:?}", a.anim, a.hp, a.add_anim, e.clash)).unwrap();
        panic!("no {want:?}: wolf {seen:?} dragon {d}
{}", log_text(app));
    };
    let seen = reversal(&mut app, &["_020004"]);
    assert!(seen.iter().any(|s| s == "AirDamageElectroChargeStart"), "{seen:?}");
    // Let the stagger and Wolf's landing play out.
    for _ in 0..(60 * 6) {
        app.update();
    }
    // At 30 %: the collapse.
    {
        let mut a = app.world_mut().get_mut::<Actor>(dragon).unwrap();
        a.hp = a.hp_max * 0.25;
    }
    // (Its 20001 first: 12505887 on 12505889.)
    let mut t = 0;
    while t < 60 * 2 || dragon_anim(&app).ends_with("_020001") {
        app.update();
        t += 1;
        assert!(t < 60 * 25, "stuck in {}", dragon_anim(&app));
    }
    reversal(&mut app, &["_020002", "_021000"]);
    let mut t = 0;
    while !dragon_anim(&app).ends_with("_021000") {
        app.update();
        t += 1;
        assert!(t < 60 * 15, "no Event21000: {}", dragon_anim(&app));
    }
    // The finisher: next to it (its judge dummy 230 without a model: the root), any side.
    {
        let world = app.world_mut();
        let dpos = world.get::<Transform>(dragon).unwrap().translation;
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        t.translation = Vec3::new(dpos.x + 1.0, t.translation.y, dpos.z);
        a.yaw = f32::atan2(1.0, 0.0);
    }
    for _ in 0..2 {
        app.update();
    }
    {
        let world = app.world_mut();
        let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let mut q = world.query::<(&Actor, &Enemy, &Transform)>();
        let (ea, e, et) = q.get(world, dragon).unwrap();
        let c = crate::player::deathblow_check(world.resource::<Combat>(), wolf, (et.translation - wolf).with_y(0.0).normalize_or_zero(), false, ea, e, et.translation, &|_| None);
        assert!(c.as_ref().is_some_and(|c| c.in_reach), "finisher not open: {} {:?} wolf {wolf} dragon {}", ea.anim, c.map(|c| c.suffix), et.translation);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut defeated = false;
    for _ in 0..(60 * 20) {
        app.update();
        let bs = app.world().resource::<crate::enemy::BossScript>();
        if bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some()) {
            defeated = true;
            break;
        }
    }
    assert!(defeated, "not defeated: wolf {:?} dragon {}
{}", player_state(&mut app), dragon_anim(&app), log_text(&app));
}

#[test]
pub(crate) fn dragon_probe() {
    if std::env::var("SHINOBI_DBG").is_err() {
        return;
    }
    let mut app = app_cfg("c5200", None, Vec::new());
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut last = String::new();
    for f in 0..1800 {
        app.update();
        let world = app.world_mut();
        let n = world.query::<&crate::enemy_bullet::EnemyBullet>().iter(world).count();
        let (ea, et) = world.query_filtered::<(&Actor, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).map(|(a, t)| (a.anim.clone(), t.translation)).unwrap();
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        let hp = a.hp;
        a.hp = a.hp_max;
        let to = (et - t.translation).with_y(0.0);
        if to.length() > 2.5 {
            t.translation += to.normalize() * 3.0 / 60.0;
        }
        a.yaw = f32::atan2(-to.x, -to.z);
        let line = format!("{ea} bullets {n} dist {:.1} wolf {} max {} y {:.1}/{:.1}", to.length(), a.state, a.hp_max, t.translation.y, et.y);
        if line != last || hp < a.hp_max {
            eprintln!("{f} {line} hp {hp}");
            last = line;
        }
    }
    eprintln!("{}", log_text(&app));
}

/// Wolf's first slash with the real geometry against the General at `dist` m: the TAE frame of
/// the first contact (his AtkParam capsule between the sword's dummy polys against the
/// General's ragdoll capsules), None for a miss.
pub(crate) fn geom_first_hit_frame(dist: f32) -> Option<(String, f32)> {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.z = dist;
        }
    }
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    // (The hit can swap the state the same frame: a deflect.)
    let mut last = (String::new(), 0.0);
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if a.attack_hit {
            return Some(last);
        }
        last = (a.anim.clone(), a.t * TAE_FPS);
    }
    None
}

/// Wolf's push-back after his first slash connects on `chr` at 2 m: the knockback the enemy's
/// NpcParam knockbackRate_vsPlayer_* cut refused (TAE flag 72 KnockbackValue, combat::recoil).
pub(crate) fn wolf_recoil_on(chr: &str) -> Option<Vec3> {
    let mut app = app_with(chr);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.z = 2.0;
        }
    }
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if a.attack_hit {
            return Some(a.kb_vel);
        }
    }
    None
}

#[test]
pub(crate) fn wolf_bounces_off_an_enemy_that_cuts_his_knockback() {
    // c1180 (club samurai, cut 75 %): pushed back (-Z, away from the enemy at +Z) over 0.09 s.
    let v = wolf_recoil_on("c1180").expect("the slash connects");
    assert!(v.z < -1.0 && v.x.abs() < v.z.abs() * 0.3, "recoil {v}");
    // c1020 (cut 0): the General takes it all, Wolf stays.
    let v = wolf_recoil_on("c1020").expect("the slash connects");
    assert!(v.length() < 1e-4, "recoil {v}");
}

/// The straw doll (c5040 50400000, resident 5300: every dmgLv_* 1, motion level 0) stays put
/// under Wolf's slash and he gets the whole scaled knockback back as recoil (exe FUN_140b6bfe0:
/// cut 1.0, the push start skipped; HANDOFF 4.47 round 25).
#[test]
pub(crate) fn a_level_0_hit_leaves_the_doll_still_and_wolf_recoils() {
    let mut app = app_cfg("c5040", Some(50400000), Vec::new());
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.x = 0.0;
            t.translation.z = 2.0;
        }
    }
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    // (Wolf's recoil, where the doll stands) on the frame the slash connects.
    let mut hit = None;
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if a.attack_hit {
            let v = a.kb_vel;
            hit = Some((v, world.query_filtered::<&Transform, With<Enemy>>().iter(world).next().unwrap().translation));
            break;
        }
    }
    let (v, doll0) = hit.expect("the slash connects");
    // Pushed back (-Z, away from the doll at +Z).
    assert!(v.z < -1.0, "Wolf's recoil {v}");
    let mut path = Vec::new();
    for _ in 0..30 {
        app.update();
        let world = app.world_mut();
        let (ea, et) = world.query_filtered::<(&Actor, &Transform), With<Enemy>>().iter(world).next().unwrap();
        path.push(format!("{} {} {:.2} {:.3}", ea.state, ea.anim, ea.t, (et.translation - doll0).with_y(0.0).length()));
    }
    let world = app.world_mut();
    let (ea, et) = world.query_filtered::<(&Actor, &Transform), With<Enemy>>().iter(world).next().unwrap();
    let moved = (et.translation - doll0).with_y(0.0).length();
    assert!(moved < 0.02 && ea.kb_vel.length() < 1e-4, "the doll slid {moved:.3} m (kb {}): {path:?}", ea.kb_vel);
}

/// Wolf's first slash on `chr` standing at `at`: (his recoil velocity, his forward, the enemy's
/// root minus his) on the frame it connects.
pub(crate) fn wolf_recoil_at(chr: &str, at: Vec3) -> Option<(Vec3, Vec3, Vec3)> {
    let mut app = app_with(chr);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.x = at.x;
            t.translation.z = at.z;
        }
    }
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let (a, wt) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        if a.attack_hit {
            let (v, f, w) = (a.kb_vel, a.forward(), wt.translation);
            let et = world.query_filtered::<&Transform, With<Enemy>>().iter(world).next().unwrap().translation;
            return Some((v, f, et - w));
        }
    }
    None
}

/// FUN_140b6bfe0's facing factor: a push square to the attacker's facing is whole, one at an angle
/// is times cos^2, never below 0.4 (and 0.4 behind him). Wolf's recoil off c1180 (cut 75 %) is
/// taken from the scaled distance, so a slash at an angle bounces him back less.
#[test]
pub(crate) fn knockback_scales_with_the_attackers_facing() {
    use crate::combat::knockback_facing;
    let fwd = Vec3::NEG_Z;
    assert!((knockback_facing(Vec3::NEG_Z * 3.0, fwd) - 1.0).abs() < 1e-6);
    let a = 40f32.to_radians();
    let k = knockback_facing(Vec3::new(a.sin(), 0.0, -a.cos()), fwd);
    assert!((k - a.cos().powi(2)).abs() < 1e-5, "{k}");
    assert_eq!(knockback_facing(Vec3::X, fwd), 0.4, "side on: cos^2 0 floored");
    assert_eq!(knockback_facing(Vec3::Z, fwd), 0.4, "behind");

    let (square, f0, d0) = wolf_recoil_at("c1180", Vec3::new(0.0, 0.0, 2.0)).expect("the square slash connects");
    let (angled, f1, d1) = wolf_recoil_at("c1180", Vec3::new(-0.8, 0.0, 1.8)).expect("the angled slash connects");
    let (k0, k1) = (knockback_facing(d0, f0), knockback_facing(d1, f1));
    assert!(k0 > 0.95 && k1 < 0.9, "facing factors {k0} {k1}");
    let ratio = angled.length() / square.length();
    assert!((ratio - k1 / k0).abs() < 0.03, "recoil {square} vs {angled}: ratio {ratio}, expected {}", k1 / k0);
}

/// AtkParam hitSourceType (Paramdex enum ATK_PARAM_HIT_SOURCE: 0 weapon, 1 body - which model's
/// dummy ids the shapes name) and hti*_Priority ("the higher one wins when two touch at once")
/// are not read by combat.rs: the merged dummy map (weapon ids over body ids) and "any shape
/// connects" give the same hits on this data. Every NPC row is 0, every priority is 0, and no
/// body-source PC row names an id Wolf's sword (WP_A_0300) has; only the wire rows' 30133 is
/// also on the grapple model (WP_A_9500 133).
#[test]
pub(crate) fn hit_source_and_priority_change_no_hit_on_this_data() {
    let rows = |name: &str| -> Vec<serde_json::Value> {
        let t = std::fs::read_to_string(crate::paths::extracted().join(format!("json/params/{name}.json"))).unwrap();
        serde_json::from_str::<serde_json::Value>(&t).unwrap()["rows"].as_array().unwrap().clone()
    };
    // The dummy ids of an exported model (model.rs `load`: the last section, u32 count then
    // 18-byte records led by the i16 id).
    let model_ids = |file: &str| -> std::collections::HashSet<i64> {
        let d = std::fs::read(crate::paths::extracted().join(file)).unwrap();
        (1..1000usize)
            .find_map(|n| {
                let o = d.len().checked_sub(4 + 18 * n)?;
                (u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize == n)
                    .then(|| (0..n).map(|i| i16::from_le_bytes([d[o + 4 + 18 * i], d[o + 5 + 18 * i]]) as i64).collect())
            })
            .unwrap()
    };
    let sword = model_ids("model_c0000_wp_a_0300.bin");
    assert!(sword.contains(&100) && sword.contains(&120), "Kusabimaru's blade dummies");
    let prio = |r: &serde_json::Value| (0..16).all(|i| r[format!("hti{i}_Priority")].as_i64().unwrap_or(0) == 0);
    let npc = rows("AtkParam_Npc");
    assert!(npc.len() > 2000 && npc.iter().all(|r| r["hitSourceType"].as_i64() == Some(0) && prio(r)));
    let pc = rows("AtkParam_Pc");
    assert!(pc.iter().all(prio));
    let body: Vec<_> = pc.iter().filter(|r| r["hitSourceType"].as_i64() == Some(1)).collect();
    assert_eq!(body.len(), 54);
    for r in body {
        for i in 0..16 {
            for k in ["DmyPoly1", "DmyPoly2"] {
                let id = r[format!("hit{i}_{k}")].as_i64().unwrap_or(-1);
                if id > 0 {
                    assert!(!sword.contains(&(id % 10000)), "row {} hit{i} {id} is on the sword", r["id"]);
                }
            }
        }
    }
}

/// A knockback covers the distance it asks for (v = dist / (cont + dec / 2), held then falling
/// linearly), whatever the frame step: 1 m with KnockBackParam row 0 damage_S-like times
/// (2026-10-12 bug hunt: whole steps at the start speed went 1.18 m).
#[test]
pub(crate) fn a_knockback_covers_exactly_its_distance() {
    for (cont, dec) in [(0.1f32, 0.2f32), (0.033, 0.05), (0.0, 0.3), (0.25, 0.0)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        let start = {
            let world = app.world_mut();
            let (mut a, t) = world.query_filtered::<(&mut Actor, &Transform), With<Player>>().single_mut(world).unwrap();
            a.knockback(Vec3::X, 1.0, cont, dec);
            t.translation
        };
        for _ in 0..40 {
            app.update();
        }
        let world = app.world_mut();
        let t = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let moved = (t - start).with_y(0.0).length();
        assert!((moved - 1.0).abs() < 0.02, "cont {cont} dec {dec}: moved {moved}");
    }
}

/// A negative knockback pulls back along its direction (the Spear's "Spear_pull" rows: -8 / -6 /
/// -4 m), and `limit_knockback` stops it at the cap: 0.5 m of a 2 m pull.
#[test]
pub(crate) fn a_negative_knockback_pulls_and_stops_at_its_cap() {
    for (cap, want) in [(f32::INFINITY, 2.0f32), (0.5, 0.5)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        let start = {
            let world = app.world_mut();
            let (mut a, t) = world.query_filtered::<(&mut Actor, &Transform), With<Player>>().single_mut(world).unwrap();
            a.knockback(Vec3::X, -2.0, 0.1, 0.2);
            a.limit_knockback(cap);
            t.translation
        };
        for _ in 0..40 {
            app.update();
        }
        let world = app.world_mut();
        let t = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let d = t - start;
        assert!(d.x < 0.0, "pulled the wrong way: {d}");
        assert!((d.with_y(0.0).length() - want).abs() < 0.02, "cap {cap}: moved {}", d.length());
    }
}

/// The Spear's pull (v7800 judge 115: knockbackDist_DirectHit -8) draws the enemy to Wolf and
/// stops it at the touch of their capsules, not 8 m on through him (round 30).
#[test]
pub(crate) fn a_spear_pull_draws_the_enemy_to_wolfs_capsule() {
    let mut app = app();
    app.update();
    let (wolf, start) = {
        let world = app.world_mut();
        let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let start = world.query_filtered::<&Transform, With<Enemy>>().single(world).unwrap().translation;
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
            crate::combat::apply_knockback(&combat, &mut a, wolf, start, None, -8.0, crate::combat::damage_kb(2), "none");
        });
        (wolf, start)
    };
    for _ in 0..60 {
        app.update();
    }
    let world = app.world_mut();
    let end = world.query_filtered::<&Transform, With<Enemy>>().single(world).unwrap().translation;
    let (was, now) = ((start - wolf).with_y(0.0), (end - wolf).with_y(0.0));
    assert!(now.length() < was.length() - 0.5, "not pulled: {was} -> {now}");
    assert!(now.dot(was) > 0.0 && now.length() > 0.6, "pulled through Wolf: {was} -> {now}");
}

/// Wolf's first attack-window frames (TAE, 30 fps) for the clip comparison in kb/damage.md
/// "Reach and first-hit vs the clips": Combo1, Ichimonji (art 5300) and Mortal Draw (5500),
/// with the art's variation judges as in play. `cargo test wolf_first_hit_frames -- --ignored
/// --nocapture` prints them.
#[test]
#[ignore]
pub(crate) fn wolf_first_hit_frames() {
    let mut app = app();
    app.update();
    let mut combat = app.world_mut().resource_mut::<Combat>();
    for (art, anim) in [(5000, "a050_300100"), (5300, "a102_316000"), (5500, "a104_316000"), (5500, "a104_316010")] {
        let var = combat.param("EquipParamWeapon", art)["behaviorVariationId"].as_i64();
        combat.player.art_variation = var;
        // Every StateInfo gate the anim's events name opened (Mortal Draw's emblem slash).
        let gates: Vec<i64> = combat.player.anim(anim).into_iter().flat_map(|a| a.events.iter()).filter_map(|e| e.arg_i64("StateInfo")).filter(|g| *g > 0).collect();
        combat.player.resident_gates = gates;
        let w: Vec<_> = combat
            .player
            .attack_windows(anim)
            .into_iter()
            .map(|(e, atk, j)| format!("j{j} f{:.1}-{:.1} r{:.2}", e.start * TAE_FPS, e.end * TAE_FPS, atk.hit0_radius))
            .collect();
        println!("{art} {anim}: {w:?}");
    }
}

/// MSB patrol routes (tools/patrols.py -> patrols.json, enemy.rs msb_patrol): the gate map's
/// Ochimusha c1010_0051 (entity 1110248) stands 23 m off the export origin, turned into the arena
/// like the map pieces; c1010_0046 (1110236, PatrolType 5) walks its three points in order and
/// stays at the last, the route stepping on each MoveToSomewhere(POINT_MOVE_POINT) success
/// (common_non_battle_act.lua NonBattleAct_Common). A point is reached on entering its region
/// (a 1.2 m sphere: IsInsideMovePoint, FUN_1405caa00 -> FUN_1409825b0), not on top of it.
#[test]
pub(crate) fn an_msb_enemy_walks_its_patrol_route() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110248) else { return };
    assert!((spot.at - Vec3::new(-11.9, 0.03, -19.78)).length() < 0.1, "{}", spot.at);
    assert_eq!(spot.route.0.points.len(), 1);
    assert!(crate::enemy::msb_patrol_on("m11_01_00_00", 1110414).is_none(), "the General has no route");
    let spot = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236).expect("c1010_0046");
    let mut spot = spot;
    assert_eq!((spot.route.0.kind, spot.route.0.points.len()), (5, 3));
    let radii: Vec<f32> = spot.route.0.areas.iter().map(|a| a.as_ref().map_or(0.0, |a| a.size[0])).collect();
    assert_eq!(radii, [1.2, 1.2, 1.2], "the points' sphere regions");
    // The test arena is flat: the regions stand on its floor.
    for a in spot.route.0.areas.iter_mut().flatten() {
        a.at.y = 0.0;
    }
    let g = crate::config::GroupEnemy { chr: "c1010".into(), npc_row: Some(10102010), at: None, msb: None };
    let mut app = app_cfg("c1010", Some(10102010), vec![g]);
    let points = spot.route.0.points.clone();
    let patroller = {
        let world = app.world_mut();
        // Wolf far behind the line-up, out of sight; the first enemy far away too.
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
        let ents: Vec<Entity> = world.query_filtered::<Entity, With<Enemy>>().iter(world).collect();
        let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
        let (mut t, mut a, mut e) = q.get_mut(world, ents[0]).unwrap();
        t.translation = Vec3::new(40.0, CAPSULE_HALF_HEIGHT, 40.0);
        a.yaw = 0.0;
        e.make_unaware(&mut a);
        let (mut t, mut a, mut e) = q.get_mut(world, ents[1]).unwrap();
        t.translation = spot.at.with_y(CAPSULE_HALF_HEIGHT);
        a.yaw = spot.yaw;
        e.make_unaware(&mut a);
        world.entity_mut(ents[1]).insert(spot.route);
        ents[1]
    };
    let mut reached = Vec::new();
    for _ in 0..60 * 40 {
        app.update();
        let world = app.world_mut();
        let pos = world.get::<Transform>(patroller).unwrap().translation;
        if let Some((cur, prev)) = world.non_send::<crate::enemy::Brains>().map.get(&patroller).and_then(|b| b.patrol_points()) {
            if prev >= 0 && reached.last() != Some(&prev) {
                let d = (pos - points[prev as usize]).with_y(0.0).length();
                reached.push(prev);
                assert!(d < 1.2 && d > 0.9, "stepped at point {prev} {d:.2} m away (its region's edge: 1.2 m)");
            }
            if reached == [0, 1, 2] {
                // A type 5 route ends at its last point: cur -1 (ai.rs Patrol, FUN_1405c2710).
                assert_eq!(cur, -1, "the route is over");
                break;
            }
        }
    }
    assert_eq!(reached, [0, 1, 2], "points reached in order");
}

/// The gate's c1010 1110236 waits with its AI off (its map event, common_func 20005110) until
/// Wolf walks into "Event_巡回起動領域_地下牢前の橋" (a box 37 x 81 x 13 m turned -48°), or
/// until it is hurt; then it walks its route.
#[test]
pub(crate) fn an_msb_enemy_waits_for_wolf_in_its_wake_region() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236) else { return };
    let hold = spot.hold.clone().expect("1110236 has its map event's AI hold");
    let region = hold.region.clone().expect("its wake region");
    assert_eq!((region.shape, region.size.len()), (5, 3));
    // The box's far corner is inside, a point just past its end is not.
    let inside = region.at + region.x * 18.0 + region.z * 40.0 + Vec3::Y;
    assert!(region.contains(inside) && !region.contains(region.at + region.z * 41.0 + Vec3::Y));
    assert!(!region.contains(region.at + region.x * 2.0 - Vec3::Y), "below its base");
    for wake_by_hurt in [false, true] {
        let g = crate::config::GroupEnemy { chr: "c1010".into(), npc_row: Some(10102010), at: None, msb: None };
        let mut app = app_cfg("c1010", Some(10102010), vec![g]);
        let held = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Transform, With<Player>>();
            q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
            let ents: Vec<Entity> = world.query_filtered::<Entity, With<Enemy>>().iter(world).collect();
            let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
            let (mut t, mut a, mut e) = q.get_mut(world, ents[0]).unwrap();
            t.translation = Vec3::new(40.0, CAPSULE_HALF_HEIGHT, 40.0);
            e.make_unaware(&mut a);
            let (mut t, mut a, mut e) = q.get_mut(world, ents[1]).unwrap();
            t.translation = spot.at.with_y(CAPSULE_HALF_HEIGHT);
            a.yaw = spot.yaw;
            e.make_unaware(&mut a);
            world.entity_mut(ents[1]).insert((spot.route.clone(), hold.clone()));
            ents[1]
        };
        let start = spot.at.with_y(0.0);
        assert!(!region.contains(Vec3::new(0.0, 0.0, 80.0)));
        for _ in 0..60 * 5 {
            app.update();
        }
        let pos = app.world().get::<Transform>(held).unwrap().translation;
        assert!((pos.with_y(0.0) - start).length() < 0.1, "held: it stays put ({pos})");
        assert!(app.world().get::<crate::enemy::AiHold>(held).is_some());
        {
            let world = app.world_mut();
            if wake_by_hurt {
                world.get_mut::<Actor>(held).unwrap().hp -= 1.0;
                world.get_mut::<Enemy>(held).unwrap().hurt_by_wolf = true;
            } else {
                let mut q = world.query_filtered::<&mut Transform, With<Player>>();
                // Inside the region on the flat sim floor (its base is 5.3 m below Wolf's start).
                let p = region.at + region.x * 12.0 + region.z * 30.0;
                q.single_mut(world).unwrap().translation = p.with_y(CAPSULE_HALF_HEIGHT);
            }
        }
        let mut moved = 0.0f32;
        for _ in 0..60 * 20 {
            app.update();
            let pos = app.world().get::<Transform>(held).unwrap().translation;
            moved = moved.max((pos.with_y(0.0) - start).length());
        }
        assert!(app.world().get::<crate::enemy::AiHold>(held).is_none(), "woken (hurt: {wake_by_hurt})");
        assert!(moved > 1.0, "it moves once woken (hurt: {wake_by_hurt}): {moved:.2} m");
    }
}

/// AI pacing for the comparison with the live recordings (kb/enemy.md "AI pacing"): the enemy
/// (`SHINOBI_PACE=<chr>[:<NpcParam row>]`, default c1010 and c1020) fights a Wolf who only holds
/// his guard (HP and posture refilled each tick) for 90 s; prints every attack start (time,
/// anim, distance) and the gaps. `cargo test ai_pacing -- --ignored --nocapture`.
#[test]
#[ignore]
pub(crate) fn ai_pacing() {
    let runs: Vec<(String, Option<i64>)> = match std::env::var("SHINOBI_PACE") {
        Ok(v) => v.split(',').map(|c| {
            let (chr, row) = c.split_once(':').map_or((c, None), |(c, r)| (c, r.parse().ok()));
            (chr.to_string(), row)
        }).collect(),
        Err(_) => vec![("c1010".into(), None), ("c1020".into(), None)],
    };
    for (chr, row) in runs {
        let mut app = app_cfg(&chr, row, Vec::new());
        {
            let world = app.world_mut();
            world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
        }
        app.world_mut().resource_mut::<PadInput>().guard_held = true;
        let mut attacks: Vec<(f32, String, f32, f32)> = Vec::new(); // (start, anim, distance, end)
        let mut cur: Option<String> = None;
        let fps = 60.0;
        for frame in 0..(90 * 60) {
            app.update();
            let t = frame as f32 / fps;
            let world = app.world_mut();
            {
                let mut q = world.query_filtered::<&mut Actor, With<Player>>();
                let mut w = q.single_mut(world).unwrap();
                w.hp = w.hp_max;
                w.posture = 0.0;
            }
            let wp = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
            let (anim, ep) = {
                let mut q = world.query_filtered::<(&Actor, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>();
                let (a, t) = q.single(world).unwrap();
                (a.anim.clone(), t.translation)
            };
            let kind = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().kind;
            let combat = world.resource::<Combat>();
            let is_attack = !combat.kind(kind).data.attack_windows(&anim).is_empty();
            if cur.as_deref() != Some(anim.as_str()) {
                if let Some(last) = attacks.last_mut().filter(|l| l.3 < 0.0) {
                    last.3 = t;
                }
                if is_attack {
                    attacks.push((t, anim.clone(), (ep - wp).with_y(0.0).length(), -1.0));
                }
                cur = Some(anim);
            }
        }
        let n = attacks.len();
        let gaps: Vec<f32> = attacks.windows(2).map(|w| w[1].0 - w[0].0).collect();
        let idle: Vec<f32> = attacks.windows(2).map(|w| (w[1].0 - w[0].3).max(0.0)).collect();
        let combos = idle.iter().filter(|g| **g < 0.2).count();
        let mean = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        let median = |v: &[f32]| {
            let mut v = v.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.get(v.len() / 2).copied().unwrap_or(0.0)
        };
        let mut by: std::collections::BTreeMap<String, usize> = Default::default();
        for a in &attacks {
            *by.entry(a.1.clone()).or_default() += 1;
        }
        let mut d: Vec<f32> = attacks.iter().map(|a| a.2).collect();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "{chr} {row:?}: {n} attacks in 90 s ({:.1}/min), start-to-start mean {:.2} / median {:.2} s, end-to-start mean {:.2} / median {:.2} s, chained {combos}/{}, distance min {:.1} median {:.1} max {:.1}",
            n as f32 / 1.5,
            mean(&gaps),
            median(&gaps),
            mean(&idle),
            median(&idle),
            idle.len(),
            d.first().copied().unwrap_or(0.0),
            d.get(d.len() / 2).copied().unwrap_or(0.0),
            d.last().copied().unwrap_or(0.0)
        );
        println!("  {by:?}");
    }
}

/// A fight reset (Wolf's death, Retry) respawns the line-up: the new enemies get their skeleton
/// and model geometry (dummy polys) like the first ones.
#[test]
pub(crate) fn enemies_respawned_by_a_fight_reset_get_their_bodies() {
    let mut app = app_geom("c1020", None);
    let bodies = |app: &mut App| {
        let world = app.world_mut();
        let n = world.query_filtered::<Entity, With<Enemy>>().iter(world).count();
        let skel = world.query_filtered::<Entity, (With<Enemy>, With<crate::anim::Skeleton>)>().iter(world).count();
        let dmy = world.query_filtered::<Entity, (With<Enemy>, With<crate::model::Dummies>)>().iter(world).count();
        (n, skel, dmy)
    };
    let first = bodies(&mut app);
    assert!(first.0 > 0 && first.1 == first.0 && first.2 == first.0, "{first:?}");
    app.world_mut().resource_mut::<crate::enemy::FightReset>().0 = true;
    for _ in 0..5 {
        app.update();
    }
    let again = bodies(&mut app);
    assert_eq!(again, first, "(enemies, with skeleton, with dummies) after the reset");
}

/// The encounter picker (C2): SpawnEncounter swaps the fight without a relaunch - the General
/// becomes the Ochimusha (own anims, model, AI), then Genichiro brings his boss events' Tomoe
/// kind and his script; the list knows him as a boss.
#[test]
pub(crate) fn spawn_encounter_swaps_the_fight_in_place() {
    let list = crate::data::encounter_list();
    let genichiro = list.iter().find(|e| e.npc_row == 71001000 && e.boss).expect("Genichiro is a boss encounter");
    let mut app = app_geom("c1020", None);
    let enemy = |app: &mut App| {
        let world = app.world_mut();
        let combat = world.resource::<Combat>().kinds.iter().map(|k| k.foe.chr.clone()).collect::<Vec<_>>();
        let mut q = world.query_filtered::<(&Actor, Has<crate::anim::Skeleton>, Has<crate::model::Dummies>), (With<Enemy>, Without<crate::enemy::Cast>)>();
        let (a, skel, dmy) = q.iter(world).next().unwrap();
        (combat[a.kind].clone(), skel && dmy, combat)
    };
    assert_eq!(enemy(&mut app).0, "c1020");
    let wolf_clips = app.world().resource::<crate::anim::AnimLib>().player.clips.len();
    app.world_mut().write_message(crate::enemy::SpawnEncounter { chr: "c1010".into(), ..Default::default() });
    for _ in 0..5 {
        app.update();
    }
    let (chr, body, kinds) = enemy(&mut app);
    assert_eq!((chr.as_str(), body), ("c1010", true), "kinds {kinds:?}");
    app.world_mut().write_message(crate::enemy::SpawnEncounter { chr: "c7100".into(), npc_row: Some(genichiro.npc_row), ..Default::default() });
    for _ in 0..5 {
        app.update();
    }
    let (chr, body, kinds) = enemy(&mut app);
    assert_eq!((chr.as_str(), body), ("c7100", true));
    assert!(kinds.iter().any(|k| k == "c7110"), "Tomoe's kind from his events: {kinds:?}");
    // Wolf's clips against him (anim_c0000_c7100.bin: his deathblows) came in.
    let lib = app.world().resource::<crate::anim::AnimLib>();
    let wolf_vs = crate::paths::root().join("extracted/anim_c0000_c7100.bin");
    assert!(!wolf_vs.exists() || lib.player.clips.len() > wolf_clips, "Wolf's clips against him");
    assert!(app.world().resource::<crate::enemy::BossScript>().run.is_some(), "his boss script runs");
}

#[test]
#[ignore]
pub(crate) fn geometry_probe() {
    let mut app = app_geom("c1020", None);
    let world = app.world_mut();
    let n_dmy: Vec<usize> = world.query::<&crate::model::Dummies>().iter(world).map(|d| d.0.len()).collect();
    let n_hurt: Vec<usize> = world.query::<&crate::anim::Hurtboxes>().iter(world).map(|h| h.0.len()).collect();
    println!("dummies {n_dmy:?} hurtboxes {n_hurt:?}");
    for d in [1.0, 2.4, 3.6, 4.0, 4.4, 4.8, 5.2, 6.0] {
        println!("dist {d}: first hit frame {:?}", geom_first_hit_frame(d));
    }
}

/// An enemy's attack `key` at Wolf (idle) `dist` m away with the real geometry: the TAE frame
/// of the first contact.
pub(crate) fn geom_enemy_hit_frame(chr: &str, key: &str, dist: f32) -> Option<f32> {
    let mut app = app_geom(chr, None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Transform, &mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
        for (mut t, mut e, mut a) in q.iter_mut(world) {
            t.translation.z = dist;
            e.force_anim(&mut a, key);
        }
    }
    let mut last = 0.0;
    for _ in 0..240 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
        if a.attack_hit {
            return Some(last);
        }
        if a.anim != key {
            return None;
        }
        last = a.t * TAE_FPS;
    }
    None
}

/// B1 table (docs/kb/damage.md): the first attack of six common enemies with the real
/// geometry: its TAE hit window, the frame it first touches Wolf at 1.5 m and its reach.
#[test]
#[ignore]
pub(crate) fn enemy_reach_table() {
    for chr in std::env::var("SHINOBI_ENEMIES").unwrap_or("c1020,c1040,c1010,c1150,c7100,c5000".into()).split(',') {
        let app = app_geom(chr, None);
        let combat = app.world().resource::<Combat>();
        let d = combat.enemy0();
        let mut keys: Vec<&String> = d.anims.keys().filter(|k| k.contains("_0030")).collect();
        keys.sort();
        let Some((key, start)) = keys.iter().find_map(|k| {
            d.attack_windows(k).iter().filter(|(_, a, _)| a.atk_phys > 0.0).map(|(e, _, _)| e.start).reduce(f32::min).map(|s| ((*k).clone(), s))
        }) else {
            println!("{chr}: no attack");
            continue;
        };
        drop(app);
        let close = geom_enemy_hit_frame(chr, &key, 1.5);
        let mut reach = None;
        let mut dist = 1.0;
        while dist <= 9.0 {
            if geom_enemy_hit_frame(chr, &key, dist).is_some() {
                reach = Some(dist);
            } else if reach.is_some() {
                break;
            }
            dist += 0.25;
        }
        println!("{chr} {key}: window from f{:.1}, contact at 1.5 m f{close:?}, reach {reach:?} m", start * TAE_FPS);
    }
}


/// Wolf's first slash (a050_300100, AttackBehavior from TAE frame 7) with the game's geometry:
/// it touches the General on its first hit frame up close, still reaches him at 4.2 m with its
/// step-in (4.8 m before the blend-in weighted the step out of a standing start; the live
/// switches have no idle -> 300100 sample) and misses at 6 m.
#[test]
pub(crate) fn wolfs_first_slash_hits_on_its_tae_frame_with_the_game_geometry() {
    let close = geom_first_hit_frame(2.4).expect("hits at 2.4 m");
    assert_eq!(close.0, "a050_300100");
    assert!((6.5..=7.5).contains(&close.1), "first contact at TAE frame {}", close.1);
    assert!(geom_first_hit_frame(4.2).is_some(), "reaches 4.2 m");
    assert!(geom_first_hit_frame(6.0).is_none(), "misses at 6 m");
}

/// The General's first attack (a000_003000, window from frame 18) touches Wolf 1-2 frames into
/// it at 1.5 m (the sword still has to come down) and misses at 5 m.
#[test]
pub(crate) fn the_generals_slash_touches_wolf_inside_its_window() {
    let f = geom_enemy_hit_frame("c1020", "a000_003000", 1.5).expect("hits at 1.5 m");
    assert!((18.0..=20.5).contains(&f), "first contact at TAE frame {f}");
    assert!(geom_enemy_hit_frame("c1020", "a000_003000", 5.0).is_none());
}

/// A one-dummy attack is a sphere (AtkParam DmyPoly2 -1, Paramdex SDT "-1だと球になる"): the
/// dog's leaping bite (c1150 AtkParam 100: dummy 10, radius 0.4) bites at 1.5 m and misses at
/// 5 m (it used to fall back to the reach approximation and reach 8 m).
#[test]
pub(crate) fn a_one_dummy_attack_is_a_sphere() {
    assert!(geom_enemy_hit_frame("c1150", "a000_003000", 1.5).is_some());
    assert!(geom_enemy_hit_frame("c1150", "a000_003000", 5.0).is_none());
}

/// A boss with deathblows left gets its vitality back from its ThrowDef reaction's own TAE
/// (AddSpEffect 300610 "HP reset", changeHpRate -100): not at the deathblow, at that frame.
#[test]
pub(crate) fn a_boss_refills_at_its_reactions_hp_reset_frame() {
    let mut app = app_with("c7100");
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
        a.hp = a.hp_max * 0.3;
    }
    break_and_deathblow(&mut app, 1);
    let mut low_in_throw_def = false;
    for _ in 0..600 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
        if a.state.starts_with("ThrowDef") && a.hp < a.hp_max {
            low_in_throw_def = true;
        }
        if a.hp >= a.hp_max {
            let combat = world.resource::<Combat>();
            let ev = combat.data_of(a).anim(&a.anim).and_then(|an| an.events.iter().find(|e| e.kind == 67 && e.arg_i64("SpEffectID") == Some(300610)).map(|e| e.start));
            let ev = ev.unwrap_or_else(|| panic!("refilled in {} without 300610", a.anim));
            assert!(low_in_throw_def, "refilled before its reaction");
            assert!((a.t - ev).abs() <= 1.0 / 60.0 + 1e-3, "refilled at {:.3} s, 300610 at {ev:.3} s ({})", a.t, a.anim);
            return;
        }
    }
    panic!("never refilled");
}


/// The picker's names are the game's own (the health bars' NPC名 text, tools/npc_names.py); a
/// name two rows share gets the place.
#[test]
pub(crate) fn encounters_carry_the_health_bar_names() {
    if !crate::paths::root().join("extracted/enemies/npc_names.json").exists() {
        return;
    }
    let list = crate::data::encounter_list();
    let name = |row: i64| list.iter().find(|e| e.npc_row == row).map(|e| e.name.clone()).unwrap_or_default();
    assert_eq!(name(71001000), "Genichiro Ashina (Ashina Castle)");
    assert!(name(51000000).starts_with("Guardian Ape"), "{}", name(51000000));
    assert_ne!(name(51000000), name(51000100));
    assert!(name(10201000).starts_with("Leader Shigenori Yamauchi"), "{}", name(10201000));
    for e in list.iter().filter(|e| e.bar) {
        println!("{} {} {}{}", e.chr, e.npc_row, e.name, if e.boss { " [boss]" } else { "" });
    }
}

/// An enemy under a "hearing 0" SpEffect (220601 General purpose: sight 0 hearing 0, on c1020's
/// TAE) hears nothing: the listener's hearing rate (exe FUN_140bfe590) is 0.
#[test]
pub(crate) fn a_deafened_enemy_hears_nothing() {
    let mut app = app_with("c1020");
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        let mut a = q.iter_mut(world).next().unwrap();
        a.resident.clear();
        assert_eq!(crate::enemy::hearing_rate(&combat, &a), 1.0);
        a.resident.push(220601);
        assert_eq!(crate::enemy::hearing_rate(&combat, &a), 0.0);
    });
}


/// The bandit's oil pot (Bullet 15500700, AtkParam decalId1 / 2 50010 / 50011) leaves both its
/// stains where it lands.
#[test]
pub(crate) fn the_oil_pot_leaves_its_stains_where_it_lands() {
    let mut app = app_cfg("c1550", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("15500700").expect("oil pot row").clone();
    let at = Vec3::new(6.0, 1.5, 6.0);
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at, Vec3::NEG_Y, 0.0, None);
    world.flush();
    let mut cursor = world.resource::<Messages<crate::gore::PlaceStain>>().get_cursor();
    let mut placed = Vec::new();
    for _ in 0..(60 * 3) {
        app.update();
        let m = app.world().resource::<Messages<crate::gore::PlaceStain>>();
        placed.extend(cursor.read(m).map(|p| (p.id, p.at)));
    }
    let ids: Vec<i64> = placed.iter().map(|p| p.0).collect();
    assert_eq!(ids, vec![50010, 50011], "{placed:?}");
    assert!(placed.iter().all(|p| p.1.xz().distance(at.xz()) < 0.5 && p.1.y < 0.1), "{placed:?}");
}

/// The Divine Dragon's warning (Bullet 52000980, FollowType 3 "follow target", hitRadius 10,
/// AtkParam 8 dummy): fired 40 m away it rides on Wolf and puts SpEffect 211000 ("dangerous enemy
/// attack VFX") on him, with no damage.
#[test]
pub(crate) fn the_divine_dragons_warning_follows_wolf() {
    let mut app = app_cfg("c5200", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("52000980").expect("warning row").clone();
    assert_eq!(spec.follow_type, 3);
    let wolf_at = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    let hp = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, wolf_at + Vec3::new(40.0, 10.0, 0.0), Vec3::NEG_Z, 0.0, None);
    world.flush();
    let mut seen = false;
    for _ in 0..3 {
        app.update();
        let world = app.world_mut();
        seen |= world.query::<&Player>().single(world).unwrap().timed.iter().any(|t| t.0 == 211000);
    }
    assert!(seen, "no 211000 on Wolf");
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp, hp);
}

/// The Chained Ogre's grab a100_003005 opens the grapple (TAE 66 SpEffect 3502020
/// chrWireVariationNo 50200, hook dummy 650, landing 260) only in its window 0.63-1.07 s
/// (kb/grapple.md section 4); the hook sits on his choker, high above his root.
#[test]
pub(crate) fn the_ogres_grab_opens_the_grapple_in_its_window() {
    let mut app = app_geom("c5020", None);
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>();
        let mut a = q.single_mut(world).unwrap();
        a.play("Ez3005", "a100_003005");
        for (t, open) in [(0.3, false), (0.8, true), (1.2, false)] {
            a.t = t;
            a.prev_t = t;
            let got = crate::enemy::grapple::wire_effect(&combat, &a);
            assert_eq!(got.is_some(), open, "t {t}: {got:?}");
        }
        a.t = 0.8;
        a.prev_t = 0.79;
    });
    app.update();
    let world = app.world_mut();
    let root = world.query_filtered::<&Transform, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().translation;
    let g = world.resource::<crate::enemy::grapple::EnemyGrapples>().0.clone();
    assert_eq!(g.len(), 1, "{g:?}");
    assert_eq!((g[0].variation, g[0].homing_row, g[0].sp_effect), (50200, 300050200, 3502020));
    // The body dummy, high on him (bent into the grab at 0.8 s: 2.3 m off the floor), not the
    // no-model fallback two metres over the root.
    assert!(g[0].hook.y - root.y > 1.0 && g[0].hook.distance(root + Vec3::Y * 2.0) > 1e-3, "hook {:?} root {root:?}", g[0].hook);
    // The Grappling Hook Attack on him: ThrowParam 15020170 (Wolf a213_511600, his 13600) and
    // 15020171 (511610 / 13610).
    let combat = app.world().resource::<Combat>();
    let foe = &combat.kind(0).foe;
    let rows: Vec<(i64, String, i64)> = combat.wire_throws(foe).into_iter().map(|(id, t)| (id, t.atk_anim, t.def_anim)).collect();
    assert_eq!(rows, vec![(15020170, "a213_511600".to_string(), 13600), (15020171, "a213_511610".to_string(), 13610)]);
}

/// Wolf's hook in the Chained Ogre mid-grab (a100_003005 holds 510010, ENABLE_WIRE_DAMAGE0):
/// c9997 HKS SP_DAMAGE_WIRE -> DamageWire0 (a000_008910). Out of the window it does nothing.
#[test]
pub(crate) fn the_hook_in_the_ogres_grab_staggers_him() {
    let mut app = app_with("c5020");
    let set = |app: &mut App, t: f32| {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &mut Actor), (With<Enemy>, Without<crate::enemy::Cast>)>();
        let (ent, mut a) = q.single_mut(world).unwrap();
        a.play("Ez3005", "a100_003005");
        a.t = t;
        a.prev_t = t - 0.01;
        ent
    };
    let ent = set(&mut app, 0.3);
    app.world_mut().write_message(crate::enemy::grapple::WireHit(ent));
    app.update();
    assert_ne!(enemy_state(&mut app).0, "DamageWire0", "outside the window");
    let ent = set(&mut app, 0.8);
    app.world_mut().write_message(crate::enemy::grapple::WireHit(ent));
    app.update();
    assert_eq!(enemy_state(&mut app).0, "DamageWire0");
}

/// The Ochimusha's ledge-hang and wall-hug deathblows (ThrowKind 110000 / 110010 hanging,
/// 90000 / 90010 left wall, 100000 / 100010 right): start rows without a defender anim, body rows
/// with ThrowDef12800 / 13000 / 12600 / 12700, all in his anim set.
#[test]
pub(crate) fn the_ochimusha_has_his_hang_and_hug_deathblows() {
    let app = app_with("c1010");
    let combat = app.world().resource::<Combat>();
    let foe = &combat.kind(0).foe;
    let body = |kinds: &[i64]| -> Vec<(i64, i64)> { combat.throws_of_kinds(foe, kinds).into_iter().map(|(id, t)| (id, t.def_anim)).collect() };
    assert_eq!(body(&[110_010]), vec![(11010081, 12800), (11010131, 13000)]);
    assert_eq!(body(&[90_010, 100_010]), vec![(11010061, 12600), (11010071, 12700)]);
    let d = &combat.kind(0).data;
    for def in [12600, 12700, 12800, 13000] {
        assert!(d.anim_key(&format!("ThrowDef{def}")).is_some(), "ThrowDef{def}");
    }
}

/// Round 41: the exe's throw range check (FUN_140be0270) - diffAngMyToDef 35 on the General's
/// start rows: a broken General 1.5 m ahead takes the deathblow only while Wolf faces him; the
/// back start (0110, DiffAng 0-90) only with both facing the same way.
#[test]
pub(crate) fn a_deathblow_needs_wolf_to_face_the_enemy() {
    let mut app = app_with("c1010");
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
        let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
        a.posture = a.posture_max;
        e.on_posture_break(&mut a, &combat, false, 4.0);
        let wolf = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
        t.translation = wolf + Vec3::Z * 1.5;
        let at = t.translation;
        // yaw 0 faces -Z: the General faces Wolf.
        a.yaw = 0.0;
        let e = &*e;
        let to = |fwd: Vec3, ea: &Actor| crate::player::deathblow_check(&combat, wolf, fwd, false, ea, e, at, &|_| None).map(|c| (c.behind, c.in_reach));
        let facing_back = -a.forward();
        assert_eq!(to(facing_back, &a), Some((false, true)), "facing him");
        assert_eq!(to(Vec3::X, &a).map(|c| c.1), Some(false), "facing aside");
        assert_eq!(to(-facing_back, &a).map(|c| c.1), Some(false), "back to him");
        // He faces away: Wolf behind him, facing his back, takes the back start.
        a.yaw += std::f32::consts::PI;
        assert_eq!(to(a.forward(), &a), Some((true, true)), "behind him");
    });
}

/// The Owl's phase-2 heat-up (a000_020005, TAE marker 3506080) offers no deathblow even with
/// his posture broken (sekiro-rs b997933).
#[test]
pub(crate) fn the_owl_has_no_deathblow_during_his_heat_up() {
    let mut app = app_with("c5060");
    let world = app.world_mut();
    let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&Enemy, &mut Actor, &Transform), Without<crate::enemy::Cast>>();
        let (e, mut a, tf) = q.single_mut(world).unwrap();
        let at = tf.translation;
        a.play("Ez20005", "a000_020005");
        a.t = 2.0;
        assert!(crate::player::deathblow_check(&combat, wolf, (at - wolf).with_y(0.0).normalize_or_zero(), false, &a, e, at, &|_| None).is_none());
    });
}

/// The zombie's dumpling (c1500 Bullet 15000031: SpEffect 9222, registIllness 70, stateInfo 5)
/// builds up terror on Wolf against his resist 200: the 3rd clean hit fills it, and the row's
/// changeHpRate 100 (effectEndurance 0) takes all his HP once (round 36: no terror at all).
/// Deflected (diseaseJustGuardResist 100) none lands.
#[test]
pub(crate) fn the_zombies_dumplings_build_up_terror() {
    use crate::status::{Guard, Statuses};
    let mut app = app_cfg("c1500", None, Vec::new());
    move_enemy_far(&mut app);
    app.update();
    let world = app.world_mut();
    let wolf = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    let weapon = world.resource::<crate::config::GameConfig>().player.combat_art;
    world.resource_scope(|world, combat: Mut<Combat>| {
        assert!(Statuses::is_build_up(&combat, 9222), "9222 is not a build-up");
        let mut s = Statuses::default();
        let n = (1..=5).find(|_| s.hit_wolf(&combat, wolf, weapon, &[], &[9222], Guard::None).iter().any(|l| l == "TERROR"));
        assert_eq!(n, Some(3));
        let mut s = Statuses::default();
        assert!((1..=10).all(|_| !s.hit_wolf(&combat, wolf, weapon, &[], &[9222], Guard::Deflect).iter().any(|l| l == "TERROR")));
        let mut st = world.resource_mut::<Statuses>();
        for _ in 0..3 {
            st.hit_wolf(&combat, wolf, weapon, &[], &[9222], Guard::None);
        }
    });
    app.update();
    app.update();
    // Round 57: the fill shows 9222's vfx 48020 once - init 5013, the terror kanji.
    let kanji = app.world_mut().query::<&crate::fxr::FxEffect>().iter(app.world()).any(|f| f.id == 5013);
    assert!(kanji, "no terror kanji 5013");
    let hp = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().hp;
    assert_eq!(hp, 0.0, "terror did not kill:
{}", log_text(&app));
    // Round 38: BEH_R_DEATH 966 - the terror death W_GroundDeathStartInsane (a000_191600), then
    // the face-down (IndexDeath B) revival-wait loop.
    let mut seen = Vec::<String>::new();
    for _ in 0..60 * 6 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert_eq!(seen.first().map(String::as_str), Some("GroundDeathStartInsane"), "{seen:?}");
    assert!(seen.iter().any(|s| s == "GroundRevivalStayLoop_B"), "{seen:?}");
}

/// The bandit's torch (SpEffect 9115, registBlood 62) builds up on Wolf against his resist 200
/// (status::WOLF_RESIST): burning on the 4th clean hit, the 5th through his guard (sword
/// bloodGuardResist 33 %), the 7th through deflects (bloodJustGuardResist 50 %); then he burns.
#[test]
pub(crate) fn the_bandits_torch_sets_wolf_on_fire() {
    use crate::status::{Guard, Statuses};
    let mut app = app_cfg("c1550", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let wolf = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    let weapon = world.resource::<crate::config::GameConfig>().player.combat_art;
    let hp = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
    world.resource_scope(|world, combat: Mut<Combat>| {
        assert!(Statuses::is_build_up(&combat, 9115), "9115 not found as a build-up");
        for (guard, hits) in [(Guard::None, 4), (Guard::Block, 5), (Guard::Deflect, 7)] {
            let mut s = Statuses::default();
            let n = (1..=10).find(|_| s.hit_wolf(&combat, wolf, weapon, &[], &[9115], guard).iter().any(|l| l == "BURNING"));
            assert_eq!(n, Some(hits), "{guard:?}");
        }
        let mut st = world.resource_mut::<Statuses>();
        for _ in 0..4 {
            st.hit_wolf(&combat, wolf, weapon, &[], &[9115], Guard::None);
        }
    });
    for _ in 0..(60 * 3) {
        app.update();
    }
    let after = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().hp;
    assert!(after < hp, "no burn damage: {hp} -> {after}");
}

/// The whole Lightning Reversal on the Way of Tomoe: caught in the air, thrown back with an
/// attack, the bolt (Bullet 500184's weak twin, AtkParam 105000281) hits him: c9997 GetSpDamage
/// -> W_DamageLightningStart (the stun), and the shock SpEffect takes its HP (the test above:
/// 508 of 2556).
#[test]
pub(crate) fn the_reversal_bolt_stuns_tomoe() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    let mut tries = vec![];
    for jump_at in [0.95f32, 1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.35, 1.45] {
        let mut app = app_with("c7110");
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.force_anim(&mut a, "a000_003042");
        }
        let mut jumped = false;
        let mut caught = false;
        for _ in 0..240 {
            let t = enemy_state(&mut app).2;
            if !jumped && t >= jump_at {
                app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
                jumped = true;
            }
            app.update();
            if player_state(&mut app).0.starts_with("AirDamageElectroCharge") {
                caught = true;
                break;
            }
        }
        if !caught {
            continue;
        }
        // The bolt is a 7.76 m capsule along the blade (dummies 10120 -> 10141, the latter on
        // WP_A_0300's Blade02 at -7.755); headless there are no dummies and the reach is short,
        // so he stands 2 m in front of Wolf.
        let hp0 = {
            let world = app.world_mut();
            let e = boss_in_play(world);
            world.get::<Actor>(e).unwrap().hp
        };
        let place = |app: &mut App| {
            let world = app.world_mut();
            let (wpos, fwd) = {
                let mut q = world.query_filtered::<(&Transform, &Actor), With<Player>>();
                let (t, a) = q.single(world).unwrap();
                (t.translation, a.forward())
            };
            let e = boss_in_play(world);
            let y = world.get::<Transform>(e).unwrap().translation.y;
            world.get_mut::<Transform>(e).unwrap().translation = (wpos + fwd * 2.0).with_y(y);
        };
        let mut released = false;
        let mut states = vec![enemy_state(&mut app).0];
        for i in 0..480 {
            if !released && i % 5 == 0 {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            if !released && player_state(&mut app).0 == "AirWeakElectroReceiveAttack" {
                released = true;
                place(&mut app);
            }
            let s = enemy_state(&mut app).0;
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        if !released {
            continue;
        }
        let hp = {
            let world = app.world_mut();
            let e = boss_in_play(world);
            world.get::<Actor>(e).unwrap().hp
        };
        tries.push((jump_at, hp0 - hp, states.clone()));
        assert!(states.iter().any(|s| s.starts_with("DamageLightning")), "no stun: took {} HP, {states:?}\n{}", hp0 - hp, log_text(&app));
        assert!(hp0 - hp >= 500.0, "took {} HP: {states:?}\n{}", hp0 - hp, log_text(&app));
        return;
    }
    panic!("no release: {tries:?}");
}
