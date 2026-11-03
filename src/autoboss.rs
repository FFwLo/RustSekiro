//! Boss check in the real game. With SHINOBI_AUTOBOSS=<shots dir> the game plays the configured
//! fight on its own, as the boss audit does headless (sim_tests boss_audit): an unkillable Wolf
//! walks up and swings for SHINOBI_AUTOBOSS_SECS (default 20) s, then the boss in play is broken
//! and Wolf deathblows it (pressing again while it kneels in 20200 for the Todome), phase after
//! phase until the map script's defeat or no boss stands for 90 s (Isshin Ashina comes in about a
//! minute after Emma falls). It logs each step
//! ("autoboss: ...") and saves screenshots: every 10 frames for 10 s after each deathblow, and
//! one every 5 s while fighting, every 10 frames for 5 s after the win. Then it exits. The window
//! is the game's own (config video) and never takes the mouse.

use bevy::prelude::*;

use crate::actor::Actor;
use crate::camera::LockOn;
use crate::data::Combat;
use crate::enemy::{BossScript, Cast, Enemy};
use crate::player::{Action, PadInput, Player};

pub struct AutoBossPlugin;

impl Plugin for AutoBossPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(list) = std::env::var("SHINOBI_ARENA_CYCLE") {
            let fights = list.split(',').filter_map(|f| f.split_once(':')).map(|(c, r)| (c.to_string(), r.parse().ok())).collect();
            app.insert_resource(Cycle { fights, frame: 0 }).add_systems(Update, cycle);
        }
        let Ok(dir) = std::env::var("SHINOBI_AUTOBOSS") else { return };
        let secs = std::env::var("SHINOBI_AUTOBOSS_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(20.0);
        let _ = std::fs::create_dir_all(&dir);
        app.insert_resource(AutoBossOn).insert_resource(Auto { dir, secs, frame: 0, phase: 0, since: 0, blowing: false, idle: 0, over: None, target: None, last: String::new() })
            .add_systems(Update, drive.after(crate::player::read_input))
            .add_systems(PostUpdate, free_cursor);
    }
}

/// SHINOBI_ARENA_CYCLE=<chr>:<row>,...: the fights are started one after another, one every
/// 6 s, as the Bosses tab does (map switches back to back); the last one stays.
#[derive(Resource)]
struct Cycle {
    fights: Vec<(String, Option<i64>)>,
    frame: u32,
}

fn cycle(mut c: ResMut<Cycle>, mut spawn: MessageWriter<crate::enemy::SpawnEncounter>) {
    c.frame += 1;
    let i = (c.frame / 360) as usize;
    if c.frame % 360 != 0 || i > c.fights.len() {
        return;
    }
    let (chr, npc_row) = c.fights[i - 1].clone();
    info!("arena cycle: {chr} {npc_row:?}");
    spawn.write(crate::enemy::SpawnEncounter { chr, npc_row, group: Vec::new() });
}

/// Only the test drives Wolf: player.rs read_input does not run (real keys and clicks are not
/// read).
pub use crate::player::ScriptedInput as AutoBossOn;

#[derive(Resource)]
struct Auto {
    dir: String,
    secs: f32,
    frame: u32,
    phase: u32,
    /// Frames since this step (fight or deathblow) began.
    since: u32,
    blowing: bool,
    /// Frames with no boss standing.
    idle: u32,
    /// Frame the fight was won.
    over: Option<u32>,
    /// The boss being deathblown (followed while it kneels in 20200, when it counts as dead).
    target: Option<Entity>,
    /// The boss's last state, for the log of its moves.
    last: String,
}

/// The mouse stays free (the game confines it on a click or the fight screen).
fn free_cursor(mut cursor: Single<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>) {
    if cursor.grab_mode != bevy::window::CursorGrabMode::None || !cursor.visible {
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// The boss in play: the main one while it stands, else the strongest enabled cast member
/// (Isshin Ashina after Emma), as sim_tests boss_in_play. SHINOBI_AUTOBOSS_CHR keeps to one chr
/// (the Folding Screen Monkeys' c1260, not the c1250 monkeys among them).
fn boss_in_play(combat: &Combat, q: &Query<(Entity, &mut Enemy, &mut Actor, &mut Transform, Has<Cast>), Without<Player>>) -> Option<Entity> {
    let only = std::env::var("SHINOBI_AUTOBOSS_CHR").ok();
    let all: Vec<(Entity, bool, f32, bool)> = q
        .iter()
        .filter(|(_, _, a, _, _)| only.as_ref().is_none_or(|c| combat.kind(a.kind).foe.chr == *c))
        .map(|(ent, e, a, _, cast)| (ent, e.is_disabled() || e.stand_in || e.is_dead(), a.hp_max, cast))
        .collect();
    if let Some(main) = all.iter().find(|c| !c.3 && !c.1) {
        return Some(main.0);
    }
    all.iter().filter(|c| !c.1).max_by(|x, y| x.2.total_cmp(&y.2)).map(|c| c.0)
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    mut auto: ResMut<Auto>,
    mut pad: ResMut<PadInput>,
    mut lock: ResMut<LockOn>,
    combat: Res<Combat>,
    script: Res<BossScript>,
    mut wolf: Query<(&mut Actor, &mut Transform), With<Player>>,
    mut foes: Query<(Entity, &mut Enemy, &mut Actor, &mut Transform, Has<Cast>), Without<Player>>,
    mut exit: MessageWriter<AppExit>,
) {
    auto.frame += 1;
    auto.since += 1;
    let f = auto.frame;
    let dir = auto.dir.clone();
    let shot = |commands: &mut Commands, name: String| {
        let path = format!("{dir}/{name}.png");
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
    };
    // Let the fight load and its script start.
    if f < 120 {
        return;
    }
    if f == 120 {
        for (ent, e, a, t, cast) in foes.iter() {
            let foe = &combat.kind(a.kind).foe;
            info!("autoboss: cast {ent:?} {} row {} cast {cast} disabled {} hp {:.0} at {:.1}", foe.chr, foe.npc_row, e.is_disabled(), a.hp_max, t.translation);
        }
    }
    if let Some(at) = auto.over {
        // The end plays on (the Todome's last frames, the boss falling): one every 10 frames.
        if (f - at) % 10 == 0 && f - at <= 300 {
            shot(&mut commands, format!("won_{:03}", f - at));
        }
        if f > at + 360 {
            info!("autoboss: done");
            exit.write(AppExit::Success);
        }
        return;
    }
    let Ok((mut wa, mut wt)) = wolf.single_mut() else { return };
    // Wolf can't die or break.
    wa.hp = wa.hp_max;
    wa.posture = 0.0;
    let boss = if auto.blowing { auto.target.filter(|t| foes.get(*t).is_ok_and(|x| !x.1.is_disabled())).or(boss_in_play(&combat, &foes)) } else { boss_in_play(&combat, &foes) };
    let won = script.run.as_ref().is_some_and(|r| r.defeated.is_some());
    auto.idle = if boss.is_some() { 0 } else { auto.idle + 1 };
    if won || auto.idle > 60 * 90 || auto.phase > 12 {
        info!("autoboss: over after {} deathblows (script defeat {won})", auto.phase);
        auto.over = Some(f);
        return;
    }
    let Some(boss) = boss else { return };
    let Ok((_, mut e, mut ba, mut bt, _)) = foes.get_mut(boss) else { return };
    lock.target = Some(boss);
    let chr = combat.kind(ba.kind).foe.chr.clone();
    if auto.blowing {
        // The Todome: pressed while the boss kneels. Pressed again while Wolf is still busy
        // with a swing and the boss stays broken.
        let waiting = ba.state.starts_with("TrunkCollapse") && !wa.state.starts_with("Deathblow");
        // The "enemy special state A" finisher (window 104, Isshin's 21000 -> 15400590) is taken
        // from behind him, facing his way (ThrowParam DiffAng 0-45; player.rs deathblow_check).
        let special = combat.data_of(&ba).finisher_window(&ba.anim, ba.t) == Some(104);
        let free = !wa.state.starts_with("Deathblow") && !wa.state.starts_with("Throw");
        let spot = if special { bt.translation - ba.forward() * 1.2 } else { bt.translation };
        // Knocked out of reach (a hit that was already on its way): walk back in.
        let to = (spot - wt.translation).with_y(0.0);
        if (waiting || special && free) && to.length() > if special { 0.3 } else { 1.8 } {
            wt.translation += to.normalize() * 3.0 / 60.0;
            wa.yaw = f32::atan2(-to.x, -to.z);
        } else if special && free {
            let f = ba.forward();
            wa.yaw = f32::atan2(-f.x, -f.z);
        }
        let kneel = ba.state == "Event20200" || special;
        if kneel && f % 20 == 0 || waiting && auto.since < 240 && auto.since % 20 == 0 {
            pad.press(Action::Attack);
        }
        // The end plays on while the boss kneels or Wolf is in a deathblow (Isshin's Todome, then
        // his 21000 and the finisher, take more than 15 s).
        if kneel || !free {
            auto.since = auto.since.min(60 * 14);
        }
        if auto.since % 10 == 0 && auto.since <= 600 {
            shot(&mut commands, format!("p{:02}_blow_{:03}", auto.phase, auto.since));
        }
        if auto.since == 30 || auto.since == 120 {
            info!("autoboss: phase {} deathblow +{}: wolf {} {} / {chr} {} {} bars {:?}", auto.phase, auto.since, wa.state, wa.anim, ba.state, ba.anim, e.ninsatsu);
        }
        if auto.since > 60 * 15 {
            auto.blowing = false;
            auto.since = 0;
            auto.phase += 1;
        }
        return;
    }
    // The fight: the boss is worn down but never killed or broken by it (only the forced
    // deathblows end phases): health from 100 % to 30 % over the phase, posture held under 90 %.
    // Moves gated on low health come up (Lady Butterfly's phantoms at 60 %, 509000_battle.lua).
    let worn = 1.0 - 0.7 * (auto.since as f32 / (auto.secs * 60.0)).min(1.0);
    ba.hp = ba.hp.max(1.0).min(ba.hp_max * worn);
    ba.posture = ba.posture.min(ba.posture_max * 0.9);
    e.aggressive = true;
    if ba.state != auto.last {
        info!("autoboss: {chr} -> {} {} at {:.1}", ba.state, ba.anim, bt.translation);
        auto.last = ba.state.clone();
    }
    let to = (bt.translation - wt.translation).with_y(0.0);
    if to.length() > 2.5 {
        wt.translation += to.normalize() * if to.length() > 10.0 { 6.0 } else { 3.0 } / 60.0;
    }
    wa.yaw = f32::atan2(-to.x, -to.z);
    if auto.since % 90 == 45 && to.length() < 3.5 {
        pad.press(Action::Attack);
    }
    if auto.since % 300 == 0 {
        shot(&mut commands, format!("p{:02}_fight_{:04}", auto.phase, auto.since / 60));
        info!("autoboss: phase {} fighting {chr}: {} {} t {:.1} hp {:.0} bars {:?} at {:.1} wolf {:.1}", auto.phase, ba.state, ba.anim, ba.t, ba.hp, e.ninsatsu, bt.translation, wt.translation);
    }
    if auto.since as f32 >= auto.secs * 60.0 {
        // Break it right in front of Wolf, facing him, and deathblow.
        ba.posture = ba.posture_max;
        e.on_posture_break(&mut ba, &combat, false, 4.0);
        let fwd = Vec3::new(-wa.yaw.sin(), 0.0, -wa.yaw.cos());
        bt.translation = wt.translation + fwd * 1.8;
        ba.yaw = wa.yaw + std::f32::consts::PI;
        pad.press(Action::Attack);
        info!("autoboss: phase {} deathblow on {chr} bars {:?}", auto.phase, e.ninsatsu);
        auto.blowing = true;
        auto.target = Some(boss);
        auto.since = 0;
    }
}
