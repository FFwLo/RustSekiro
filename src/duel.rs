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
            app.insert_resource(Duel { file, secs, timer: -WARMUP, lines: Vec::new(), shown: Vec::new(), gaps: Default::default() }).add_systems(Update, run_duel);
        }
    }
}

fn run_duel(
    time: Res<Time>,
    combat: Res<crate::data::Combat>,
    mut duel: ResMut<Duel>,
    log: Option<Res<CombatLog>>,
    mut player: Query<(&mut Actor, &Transform, Option<&crate::anim::Hurtboxes>), (With<Player>, Without<Enemy>)>,
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
    if let Ok((mut wolf, _, _)) = player.single_mut() {
        wolf.hp = wolf.hp_max;
        wolf.posture = 0.0;
    }
    let hurt: Vec<(Vec3, Vec3, f32)> = player
        .single()
        .ok()
        .and_then(|(_, _, h)| h)
        .map(|h| h.0.iter().filter_map(|&(a, b, r)| Some((markers.get(a).ok()?.translation(), markers.get(b).ok()?.translation(), r))).collect())
        .unwrap_or_default();
    if let Ok((mut e, a, tf, dummies)) = enemy.single_mut() {
        e.aggressive = true;
        // Each active attack window: how close its capsule comes to Wolf.
        let d = combat.data_of(&a);
        for (ev, atk, judge) in d.attack_windows(&a.anim) {
            if !ev.in_time(a.t) {
                continue;
            }
            let at = |id: i64| dummies.and_then(|dm| dm.0.get(&((if id >= 10000 { id % 10000 } else { id }) as i16))).and_then(|m| markers.get(*m).ok()).map(|g| g.translation());
            let gap = match (at(atk.hit0_dmy1), at(atk.hit0_dmy2).or(at(atk.hit0_dmy1))) {
                (Some(p1), Some(p2)) => hurt.iter().map(|&(h1, h2, r)| crate::combat::segment_distance(p1, p2, h1, h2) - r - atk.hit0_radius).fold(f32::MAX, f32::min),
                _ => f32::INFINITY,
            };
            let slot = duel.gaps.entry((a.anim.clone(), judge)).or_insert(f32::MAX);
            *slot = slot.min(gap);
        }
        // A sample every second: distance, state and AI plan.
        if duel.timer >= *next_sample {
            *next_sample += 1.0;
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
    if let Ok((e, a, _, _)) = enemy.single() {
        out.push_str(&format!("enemy state {} anim {} ai [{}]\n", a.state, a.anim, e.ai_desc));
    }
    for l in &duel.lines {
        out.push_str(l);
        out.push('\n');
    }
    let _ = std::fs::write(&duel.file, out);
    exit.write(AppExit::Success);
}
