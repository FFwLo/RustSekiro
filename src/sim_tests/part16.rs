//! Sim tests, part 16: the atlas bosses pass (docs/kb/mechanics-atlas.json: bosses, the boss steps
//! of flow, the boss status records). Each test names its atlas id.

use super::*;

/// The sim with `chr` / `row`, the enemy switched off (no AI, no attacks), Wolf at the origin.
fn quiet_app(chr: &str, row: i64) -> (App, Entity) {
    let mut app = app_cfg(chr, Some(row), Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    world.get_mut::<Enemy>(shooter).unwrap().disabled = true;
    (app, shooter)
}

fn wolf(world: &mut World) -> Entity {
    world.query_filtered::<Entity, With<Player>>().single(world).unwrap()
}

/// Atlas status.poison-owl-floor: Owl (Father)'s poison ball (Bullet 50601730, 11 m/s, its
/// HitBulletID 50600731) drops five drops (numShoot 5, 100 m/s2 down) that each leave poison
/// floor 50600732 (2 m, 18 s, SpEffect 9019 poizonAttackPower 30, re-hit 0.15 s, one shared hit
/// list). A Wolf standing in it takes no damage and no reaction, only poison: 30 per contact
/// every 0.15 s, so his 200 resist fills on the 7th contact (about 0.9 s) and 9019 hands on to
/// 9050 (replaceSpEffectId).
#[test]
fn atlas_status_poison_owl_floor_builds_poison_every_0_15_s() {
    let (mut app, shooter) = quiet_app("c5060", 50601010);
    let world = app.world_mut();
    let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("50601730").expect("Owl (Father)'s poison ball").clone();
    assert_eq!((spec.hit_bullet_id, spec.init_vellocity), (50600731, 11.0));
    let we = wolf(world);
    let hp = world.get::<Actor>(we).unwrap().hp;
    // Thrown at Wolf's chest from 1.5 m ahead: it ends on him, the drops fall at his feet.
    let at = Vec3::new(0.0, CAPSULE_HALF_HEIGHT + 0.6, 1.5);
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at, Vec3::NEG_Z, 0.0, None);
    world.flush();
    let (mut floors, mut gauge, mut rises, mut poisoned_at) = (0usize, 0.0f32, Vec::new(), None);
    for f in 1..=(60 * 2) {
        app.update();
        let world = app.world_mut();
        floors = floors.max(world.query::<&crate::enemy_bullet::EnemyBullet>().iter(world).filter(|b| b.row() == 50600732).count());
        let s = world.resource::<crate::status::Statuses>().0.get(&we);
        let (g, has_9050) = s.map_or((0.0, false), |s| (s.poison, s.effects.iter().any(|e| e.id == 9050)));
        if g > gauge + 1.0 {
            rises.push(f);
        }
        gauge = g;
        if poisoned_at.is_none() && has_9050 {
            poisoned_at = Some(f);
        }
    }
    assert_eq!(floors, 5, "five drops, five floors");
    // Contacts at least 0.15 s (9 frames) apart, not one per floor per frame.
    assert!(rises.windows(2).all(|w| w[1] - w[0] >= 9), "contacts {rises:?}");
    let p = poisoned_at.unwrap_or_else(|| panic!("never poisoned; contacts {rises:?}\n{}", log_text(&app)));
    // Seven contacts 0.15 s apart: about 0.9 s after the first.
    let first = rises.first().copied().unwrap_or(p);
    assert!((50..=70).contains(&(p - first)), "poisoned {} frames after the first contact; contacts {rises:?}", p - first);
    let world = app.world_mut();
    let a = world.get::<Actor>(we).unwrap();
    assert!(!a.state.contains("Damage"), "a reaction to the floor: {}", a.state);
    // Only the poison's own ticks take HP (none before 9050's first interval).
    assert!(a.hp >= hp - 0.5 * hp, "{} -> {}", hp, a.hp);
}

/// The Headless Ape's scream (c5100 row 51000100) with Wolf `dist` metres ahead: Wolf's terror
/// gauge at the end and whether he died.
fn scream_at(dist: f32) -> (f32, bool, usize) {
    let mut app = app_cfg("c5100", Some(51000100), Vec::new());
    app.update();
    let ape = app.world_mut().query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(app.world()).unwrap();
    // Past its entrance (the map script's Event20005 rise, 6.3 s).
    let mut risen = false;
    for _ in 0..900 {
        app.update();
        let event = app.world().get::<Actor>(ape).unwrap().state.starts_with("Event");
        if risen && !event {
            break;
        }
        risen |= event;
    }
    let world = app.world_mut();
    {
        let mut q = world.query::<(&mut Actor, &mut Transform)>();
        let (mut a, mut t) = q.get_mut(world, ape).unwrap();
        // Facing Wolf at the origin (+Z ahead of him), `dist` away.
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, dist);
        a.yaw = 0.0;
        a.play("Ez3007", "a100_003007");
    }
    let we = wolf(world);
    let (mut terror, mut died, mut shots) = (0.0f32, false, 0usize);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..(60 * 7) {
        app.update();
        let world = app.world_mut();
        for (e, b) in world.query::<(Entity, &crate::enemy_bullet::EnemyBullet)>().iter(world) {
            if b.row() % 1000 == 580 && seen.insert(e) {
                shots += 1;
            }
        }
        terror = terror.max(world.resource::<crate::status::Statuses>().0.get(&we).map_or(0.0, |s| s.terror));
        died |= world.get::<Actor>(we).unwrap().hp <= 0.0;
    }
    (terror, died, shots)
}

/// Atlas status.terror-scream: the scream a100_003007 fires Bullet 51000580 (hitRadius 10, life
/// 0.2, SpEffect 9220 registIllness 18) every 0.167 s from its TAE. Within 10 m each one adds
/// 18 to Wolf's terror; against his 200 resist the 12th fills it, and 9220's changeHpRate 100
/// kills him. 20 m away he stays clean.
#[test]
fn atlas_status_terror_scream_kills_near_and_spares_far() {
    let (terror, died, shots) = scream_at(4.0);
    assert!(shots >= 12, "the scream fired {shots} bullets");
    assert!(died, "Wolf 4 m away lived (terror peak {terror})");
    let (terror, died, _) = scream_at(20.0);
    assert!(!died && terror == 0.0, "Wolf 20 m away: terror {terror}, died {died}");
}

/// Atlas bosses.team-type-flips: the map script's Set Character Team Type (2004[2]) changes the
/// boss's team: the Demon of Hatred's entrance (m11_00 11105910) makes him 28 "Coop NPC" and
/// 11105923 / 11105924 put him back to 6 "Enemy" once the fight is on.
#[test]
fn atlas_bosses_team_type_flips_follow_the_demons_script() {
    let mut app = app_cfg("c7020", Some(70200000), Vec::new());
    let mut teams: Vec<i64> = Vec::new();
    for _ in 0..(60 * 40) {
        app.update();
        let world = app.world_mut();
        let boss = boss_in_play(world);
        let combat = world.resource::<Combat>();
        let t = crate::combat::team_of(combat, world.get::<Actor>(boss).unwrap());
        if teams.last() != Some(&t) {
            teams.push(t);
        }
        // Wolf walks up to him (the fight starts on his approach).
        let et = world.get::<Transform>(boss).unwrap().translation;
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        let mut wt = q.single_mut(world).unwrap();
        let to = (et - wt.translation).with_y(0.0);
        if to.length() > 3.0 {
            wt.translation += to.normalize() * 4.0 / 60.0;
        }
    }
    assert!(teams.windows(2).any(|w| w == [28, 6]), "teams {teams:?}");
}

/// Atlas bosses.dragons: the Divine Dragon (NpcParam 52000000) fights with one deathblow mark
/// and ninsatsuDamageRate 5.0. The Old Dragons' markless bar body is
/// the_old_dragons_bar_is_the_scripted_bar_body, their fall and the bar hand-over
/// the_old_dragons_fall_hands_over_to_the_divine_dragon, the finisher 15200090
/// the_divine_dragons_lightning_comes_back_at_it.
#[test]
fn atlas_bosses_dragons_divine_dragon_has_one_mark() {
    let mut app = app_cfg("c5200", Some(52000000), Vec::new());
    app.update();
    let world = app.world_mut();
    let boss = boss_in_play(world);
    let (marks, rate) = {
        let combat = world.resource::<Combat>();
        let a = world.get::<Actor>(boss).unwrap();
        (world.get::<Enemy>(boss).unwrap().ninsatsu.1, combat.npc(a)["ninsatsuDamageRate"].as_f64())
    };
    assert_eq!(marks, 1);
    assert_eq!(rate, Some(5.0));
}

/// Atlas flow.15b-father-bell-charm-owl-memory / bosses.owl-father (the fight step; sekiro-rs
/// father_bird.rs): Owl (Father)'s bird c5070 (1000901) runs on our enemy AI from his map
/// script alone. Its cast comes on its first enable; after his first deathblow his event message 20 makes m10 11005912 give him
/// 200051 (the poison-ball phase); at his next backstep (a000_005201's SpEffect 3506022) it
/// enables the bird, warps it onto him (2004[42]), plays its 20000, then AI command 10.
#[test]
fn atlas_owl_father_calls_his_bird_after_the_first_deathblow() {
    let mut app = app_cfg("c5060", Some(50601010), Vec::new());
    for _ in 0..300 {
        app.update();
    }
    let find_bird = |app: &App| app.world().resource::<crate::enemy::BossScript>().run.as_ref().and_then(|r| r.ents.get(&1000901).copied());
    // The cast comes on its first enable: no bird before his phase 2.
    assert!(find_bird(&app).is_none(), "the bird is out before his phase 2");
    break_and_deathblow(&mut app, 60);
    let (mut states, mut called, mut stepped) = (Vec::<String>::new(), false, false);
    for _ in 0..(60 * 20) {
        app.update();
        // In phase 2 his backstep (a000_005201, TAE 67 SpEffect 3506022) is the call.
        let owl = boss_in_play(app.world_mut());
        let mut oa = app.world_mut().get_mut::<Actor>(owl).unwrap();
        if !stepped && oa.resident.contains(&200051) && !oa.state.starts_with("Throw") {
            oa.play("Ez5201", "a000_005201");
            stepped = true;
        }
        let Some(b) = find_bird(&app) else { continue };
        let world = app.world();
        let (Some(e), Some(a)) = (world.get::<Enemy>(b), world.get::<Actor>(b)) else { continue };
        if !e.disabled && states.last() != Some(&a.state) {
            states.push(a.state.clone());
        }
        called |= e.event_req().iter().any(|&(slot, cmd)| slot == 0 && cmd == 10);
        if called {
            break;
        }
    }
    let owl = boss_in_play(app.world_mut());
    assert!(app.world().get::<Actor>(owl).unwrap().resident.contains(&200051), "no 200051 on the Owl");
    assert!(states.iter().any(|s| s == "Event20000"), "bird states {states:?}");
    assert!(called, "no AI command 10 to the bird; states {states:?}");
}

/// Atlas flow.30b-headless-ape (the fight step; sekiro-rs headless_ape.rs): after the Headless
/// Ape's first deathblow m17 11705880 sends it to region 1702871 (AI command 10; the sim has no
/// map regions, so the test puts it there), summons its
/// partner 1700852 (c5100 row 51000010, 20020) and names each the other's TARGET_EVENT
/// (2004[11]), so both AIs read their partner live (ai.rs EventView).
#[test]
fn atlas_headless_ape_and_its_partner_target_each_other() {
    let mut app = app_cfg("c5100", Some(51000100), Vec::new());
    for _ in 0..600 {
        app.update();
    }
    break_and_deathblow(&mut app, 60);
    let script_ent = |app: &App, id: i64| app.world().resource::<crate::enemy::BossScript>().run.as_ref().and_then(|r| r.ents.get(&id).copied());
    let main = script_ent(&app, 1700850).expect("the Headless Ape");
    let mut paired = None;
    // Its walk to 1702871 needs the map's regions (the AI's SetEventMoveTarget): set it there
    // once it has the command.
    let spot = app.world().resource::<crate::enemy::BossScript>().run.as_ref().and_then(|r| r.place_of(1702871)).expect("region 1702871").0;
    for _ in 0..(60 * 60) {
        app.update();
        if app.world().get::<Enemy>(main).unwrap().event_req().contains(&(0, 10)) {
            app.world_mut().get_mut::<Transform>(main).unwrap().translation = spot;
        }
        let Some(mate) = script_ent(&app, 1700852) else { continue };
        let world = app.world();
        let (Some(a), Some(b)) = (world.get::<Enemy>(main), world.get::<Enemy>(mate)) else { continue };
        if a.event_target == Some(mate) && b.event_target == Some(main) {
            paired = Some(mate);
            break;
        }
    }
    let mate = paired.unwrap_or_else(|| panic!("never paired; flags {:?}", app.world().resource::<crate::enemy::BossScript>().run.as_ref().map(|r| r.flags_on())));
    let world = app.world();
    assert_eq!(world.resource::<Combat>().npc(world.get::<Actor>(mate).unwrap())["hp"].as_i64(), Some(1600), "the partner is row 51000010");
}
