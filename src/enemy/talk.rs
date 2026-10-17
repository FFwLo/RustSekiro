//! A talk before a fight: the Owl (Father) speaks before he draws (m10 talk ESD t100121).
//!
//! t100121's state group 2147483619 sets flag 71000137, plays TalkParam 12030000 and sets 71000130
//! as it ends (TalkToPlayer's "done" flag), clearing 71000137. The map script m10 11005910 holds
//! the Owl in his 401 loop until 71000130 is on with Wolf in region 1002810, then turns his AI on
//! (2004[1]). Wolf's HKS plays the talk's start and end (c0000_transition.lua BEH_R_NPC_TALK_START
//! -> W_Event700400, BEH_R_NPC_TALK_END -> W_Event700420).
//!
//! The lines are TalkParam 12030000-12030005: the Owl's soundId0 (12030002 included) and Wolf's
//! soundId_pc on 12030003, FMOD events "v0<id>" in the map's voice bank vm10. Their timing and the
//! Owl's gesture are as rec_20261016_074559_boss_owl_father shows them (below).
//! gap: the Owl's lip sync (LipSyncBlend00 a000_041500, a blend layer), Wolf's speaking additive
//! (W_AddBlendSpeakFadeIn / Out), the subtitles and the voice lengths are not played or read: the
//! lines start at the recorded times.

use bevy::prelude::*;

use super::script::BossScript;
use super::{Enemy, Mode};
use crate::actor::Actor;
use crate::data::Combat;
use crate::player::{PadInput, Player};

pub(crate) struct TalkDef {
    /// The boss script row it opens.
    row: i64,
    /// Flag on while it runs, and the flag it sets as it ends.
    busy: i64,
    done: i64,
    /// The voice bank (FMOD project) of its lines.
    pub bank: &'static str,
    /// (start s, TalkParam row, Wolf speaks it).
    lines: &'static [(f32, i64, bool)],
    /// The speaker's own anims (start s, state).
    gestures: &'static [(f32, &'static str)],
    /// Its end.
    end: f32,
}

/// rec_20261016_074559_boss_owl_father, from 71000137 on at 167.9 ms: W_LipSync at 184.9 /
/// 2177.9 / 4177.5 / 6183.1 / 7559.6 / 9547.5 ms, the Owl's W_IdleUniqueDefault402 (a000_000402)
/// at 3352.6 ms and W_IdleBattle at 7325.8 ms, 71000130 on and 71000137 off at 10276.6 ms.
const TALKS: &[TalkDef] = &[TalkDef {
    row: 50601010,
    busy: 71000137,
    done: 71000130,
    bank: "vm10",
    lines: &[(0.017, 12030000, false), (2.010, 12030001, false), (4.010, 12030002, false), (6.015, 12030003, true), (7.392, 12030004, false), (9.380, 12030005, false)],
    gestures: &[(3.185, "IdleUniqueDefault402"), (7.158, "IdleBattle")],
    end: 10.109,
}];

/// The talks are skipped: the fight opens as if the talk had just ended (the user, 2026-10-16:
/// "skip owl animation and talk"). `true` plays them as recorded.
pub(crate) const PLAY_TALKS: bool = false;

/// The running talk (None once over, or for a fight without one).
#[derive(Resource, Default)]
pub struct Talk {
    run: Option<Running>,
}

struct Running {
    def: &'static TalkDef,
    t: f32,
    line: usize,
    gesture: usize,
}

impl Talk {
    pub fn active(&self) -> bool {
        self.run.is_some()
    }

    /// The voice bank the running talk needs (fmod.rs loads it).
    pub fn bank(&self) -> Option<&'static str> {
        self.run.as_ref().map(|r| r.def.bank)
    }
}

/// Wolf takes no input while it runs (the game's talk holds him in its state).
pub(crate) fn hold_input(talk: Res<Talk>, pad: Option<ResMut<PadInput>>) {
    if let (true, Some(mut pad)) = (talk.active(), pad) {
        pad.hold();
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn run_talk(
    time: Res<Time>,
    combat: Res<Combat>,
    mut bs: ResMut<BossScript>,
    mut talk: ResMut<Talk>,
    mut sounds: Option<ResMut<crate::sound::SoundQueue>>,
    mut wolf: Single<(&mut Actor, &Transform), (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Enemy, &mut Actor, &Transform), Without<Player>>,
) {
    let Some(run) = bs.run.as_mut() else {
        *talk = Talk::default();
        return;
    };
    if !run.talk_seen {
        run.talk_seen = true;
        talk.run = None;
        if let Some(def) = TALKS.iter().find(|d| d.row == run.row) {
            // The extractor's world-state guess (boss_scripts.py world_flags) takes the talk's
            // flags as on: it is played here instead.
            if !PLAY_TALKS {
                run.set_flag(def.busy, false);
                run.set_flag(def.done, true);
                return;
            }
            run.set_flag(def.done, false);
            run.set_flag(def.busy, true);
            let (a, _) = &mut *wolf;
            a.play_state(&combat.player, "Event700400");
            talk.run = Some(Running { def, t: 0.0, line: 0, gesture: 0 });
        }
    }
    let Some(r) = talk.run.as_mut() else { return };
    let dt = time.delta_secs();
    let boss = run.ents.get(&run.boss()).copied();
    let wolf_pos = wolf.1.translation;
    {
        // Wolf holds the talk pose (a000_700400, 20 frames) to its end.
        let (a, _) = &mut *wolf;
        if a.state == "Event700400" {
            let len = combat.player.length(&a.anim);
            a.t = a.t.min((len - 2.0 * dt).max(0.0));
        }
    }
    let boss_pos = boss.and_then(|e| q.get(e).ok()).map(|x| x.2.translation);
    while let Some(&(at, id, pc)) = r.def.lines.get(r.line) {
        if r.t < at {
            break;
        }
        let pos = if pc { wolf_pos } else { boss_pos.unwrap_or(wolf_pos) };
        if let Some(s) = sounds.as_mut() {
            s.0.push((format!("v{id:09}"), pos));
        }
        r.line += 1;
    }
    while let Some(&(at, state)) = r.def.gestures.get(r.gesture) {
        if r.t < at {
            break;
        }
        if let Some((mut e, mut a, _)) = boss.and_then(|b| q.get_mut(b).ok()) {
            let d = combat.data_of(&a);
            if state == "IdleBattle" {
                // Back to the AI-off idle (enemy.rs), its mode left as it is.
                a.procedural("IdleBattle");
            } else if a.play_state(d, state) {
                // It plays out, then the AI-off rule idles it (enemy.rs).
                e.mode = Mode::Rising(d.length(&a.anim).max(0.1));
            }
        }
        r.gesture += 1;
    }
    r.t += dt;
    if r.t >= r.def.end {
        run.set_flag(r.def.busy, false);
        run.set_flag(r.def.done, true);
        wolf.0.play_state(&combat.player, "Event700420");
        talk.run = None;
    }
}
