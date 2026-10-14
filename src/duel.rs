//! Duel check for the enemy roster: with SHINOBI_DUEL=<file> the game lets the enemy fight an
//! idle, unkillable Wolf for SHINOBI_DUEL_SECS (default 20) seconds with its real model, dummy
//! polys and hurtboxes, then writes the combat log to <file> and exits. The headless smoke test
//! (sim_tests all_enemies_smoke) has no model, so its reach guess misses big enemies.

use bevy::prelude::*;

use crate::actor::Actor;
use crate::enemy::Enemy;
use crate::hud::CombatLog;
use crate::player::Player;

/// Seconds for models and textures to load before the fight starts.
const WARMUP: f32 = 4.0;

#[derive(Resource)]
struct Duel {
    file: String,
    secs: f32,
    timer: f32,
    /// Every log line so far (the on-screen CombatLog keeps the last 6).
    lines: Vec<String>,
    shown: Vec<String>,
    /// (anim, judge) -> closest gap [m] between its hit capsule and Wolf's hurtboxes (negative =
    /// overlapping, i.e. it should have hit).
    gaps: std::collections::BTreeMap<(String, i64), f32>,
}

pub struct DuelPlugin;

impl Plugin for DuelPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(file) = std::env::var("SHINOBI_DUEL") {
            let secs = std::env::var("SHINOBI_DUEL_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(20.0);
            app.insert_resource(Duel { file, secs, timer: -WARMUP, lines: Vec::new(), shown: Vec::new(), gaps: Default::default() })
                .add_systems(Update, run_duel)
                .add_systems(FixedUpdate, keep_wolf_up.after(crate::actor::ActorSet::Resolve).after(crate::status::tick));
        }
    }
}

/// Wolf can't die or break in a duel run. Each fixed step, after the hits and the status ticks
/// (as debug_menu player_cheats): in Update, a frame of two fixed steps let the second play the
/// first one's kill (round 65).
fn keep_wolf_up(mut player: Query<&mut Actor, (With<Player>, Without<Enemy>)>) {
    for mut wolf in &mut player {
        wolf.hp = wolf.hp_max;
        wolf.posture = 0.0;
        wolf.no_posture = true;
        wolf.death_ref = 0;
        wolf.death_hit = None;
    }
}

fn run_duel(
    time: Res<Time>,
    combat: Res<crate::data::Combat>,
    mut duel: ResMut<Duel>,
    log: Option<Res<CombatLog>>,
    player: Query<(&Actor, &Transform, Option<&crate::anim::Hurtboxes>), (With<Player>, Without<Enemy>)>,
    mut enemy: Query<(&mut Enemy, &Actor, &Transform, Option<&crate::model::Dummies>), Without<Player>>,
    markers: Query<&GlobalTransform>,
    mut next_sample: Local<f32>,
    mut exit: MessageWriter<AppExit>,
) {
    duel.timer += time.delta_secs();
    if duel.timer < 0.0 {
        return;
    }
    // New lines: the tail of the log past its overlap with what was on screen last frame.
    if let Some(log) = log.as_ref() {
        let now: Vec<String> = log.lines.iter().map(|l| l.0.clone()).collect();
        let keep = (0..=duel.shown.len()).find(|&i| now.starts_with(&duel.shown[i..])).unwrap_or(duel.shown.len());
        let fresh = now[(duel.shown.len() - keep).min(now.len())..].to_vec();
        duel.lines.extend(fresh);
        duel.shown = now;
    }
    let wolf_at = player.single().map(|(_, t, _)| t.translation).unwrap_or_default();
    let wolf_hurt = player.single().ok().and_then(|(_, _, h)| h).map_or(0, |h| h.0.len());
    let hurt: Vec<(Vec3, Vec3, f32)> = player
        .single()
        .ok()
        .and_then(|(_, _, h)| h)
        .map(|h| h.0.iter().filter_map(|&(a, b, r)| Some((markers.get(a).ok()?.translation(), markers.get(b).ok()?.translation(), r))).collect())
        .unwrap_or_default();
    // Every enemy (a group, the Tomoe swap, the Monk's phantoms); before, single_mut() gave up
    // once a second one existed and the samples stopped (round 48).
    let sample = duel.timer >= *next_sample;
    if sample {
        *next_sample += 1.0;
    }
    for (mut e, a, tf, dummies) in &mut enemy {
        e.aggressive = true;
        // Each active attack window: how close its capsules come to Wolf - every shape the row
        // uses (hit0..hit15) and combat.rs's dummy lookup (round 48: before hit0 only, and ids
        // >= 10000 always as id % 10000).
        let d = combat.data_of(&a);
        for (ev, atk, judge) in d.attack_windows(&a.anim) {
            if !ev.in_time(a.t) {
                continue;
            }
            let at = |id: i64| dummies.and_then(|dm| crate::combat::dmy_entity(dm, id)).and_then(|m| markers.get(m).ok()).map(|g| g.translation());
            let gap = atk
                .shapes()
                .into_iter()
                .filter_map(|(d1, d2, rad)| Some((at(d1)?, if d2 < 0 { at(d1)? } else { at(d2)? }, rad)))
                .flat_map(|(p1, p2, rad)| hurt.iter().map(move |&(h1, h2, r)| crate::combat::segment_distance(p1, p2, h1, h2) - r - rad))
                .fold(f32::INFINITY, f32::min);
            let slot = duel.gaps.entry((a.anim.clone(), judge)).or_insert(f32::INFINITY);
            *slot = slot.min(gap);
        }
        // A sample every second: distance, state and AI plan.
        if sample {
            // The usual hit dummies (10 body, 30/31 weapon or horns): their distance to Wolf.
            let dm: Vec<String> = [10i16, 30, 31]
                .iter()
                .filter_map(|k| dummies?.0.get(k).and_then(|m| markers.get(*m).ok()).map(|g| format!("d{k} {:.1}", g.translation().distance(wolf_at))))
                .collect();
            let line = format!("[{:4.1}s] dist {:.1} {} {} t{:.2} | {} | {} | wolf hurt {wolf_hurt}", duel.timer, tf.translation.distance(wolf_at), a.state, a.anim, a.t, dm.join(" "), e.ai_desc);
            duel.lines.push(line);
        }
    }
    if duel.timer < duel.secs {
        return;
    }
    let mut out = String::new();
    for (e, a, _, _) in &enemy {
        out.push_str(&format!("enemy state {} anim {} ai [{}]\n", a.state, a.anim, e.ai_desc));
    }
    for l in &duel.lines {
        out.push_str(l);
        out.push('\n');
    }
    // The reach per attack window (the module's purpose; before, measured but never written).
    out.push_str("gaps (anim judge: closest hit shape to Wolf's hurtboxes, m; negative = overlapping)\n");
    for ((anim, judge), gap) in &duel.gaps {
        out.push_str(&format!("{anim} {judge}: {gap:.2}\n"));
    }
    let _ = std::fs::write(&duel.file, out);
    exit.write(AppExit::Success);
}
