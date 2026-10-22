//! An enemy as the game runs it: the shared NPC script and its own (c9997.hks + c<chr>.hks) on
//! its behbnd's c9997.hkx, with the engine's answers from the sim (`answers`, used by the
//! lockstep tests too). It runs one per enemy in the game (SHINOBI_HKS=0: off) beside enemy.rs
//! (`HksEnemyPlugin`); docs/kb/hks-graph.md.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::Combat;
use crate::enemy::{DamageEnv, Enemy};
use crate::hks::{Host, HksVm, ScriptChr};
use crate::player::Player;

/// c9997.lua 258-259: the guard and general-stance requests.
const ACTION_TYPE_GUARD: f64 = 9910.0;
const ACTION_TYPE_GENERAL_STANCE: f64 = 9920.0;
/// TAE AI cancel windows (enemy.rs AI_CANCEL_FLAGS): 23 combo attack, 86 attack, 79 step.
const FLAG_COMBO: i64 = 23;
const FLAG_ATTACK: i64 = 86;
const FLAG_STEP: i64 = 79;
/// "End If AI Move Queued".
const FLAG_AI_MOVE: i64 = 78;

/// Where Wolf is for an enemy's answers, and whether he just got out of its grab.
#[derive(Clone, Copy)]
pub struct WolfSeen {
    pub pos: Vec3,
    pub escaped: bool,
}

/// The engine's answers for the enemy as the sim has it now. TAE reads (windows, SpEffects) are
/// of the script side's clip (`clip`), as the game's engine reads the clip its graph plays.
#[allow(clippy::too_many_arguments)]
pub fn answers(h: &mut Host, a: &Actor, e: &Enemy, cmd: &crate::ai::Command, pos: Vec3, wolf: WolfSeen, combat: &Combat, clip: Option<(String, f32)>, anims: &[i64], request: Option<i64>, damage: Option<DamageEnv>, throw_start: bool) {
    let d = combat.data_of(a);
    let (shown, t) = clip.unwrap_or_else(|| a.shown_clip(d));
    let mut env: HashMap<(i64, Option<i64>), f64> = HashMap::new();
    let mut set = |id: i64, arg: Option<i64>, v: f64| {
        env.insert((id, arg), v);
    };
    // HP: at 0 with a deathblow left the exe holds it at 1 (IsNoDead FUN_140bd5df0 -> SetHP);
    // enemy.rs keeps 0 and opens the deathblow.
    set(1000, None, if a.hp <= 0.0 && !e.is_dead() { 1.0 } else { a.hp as f64 });
    // On the ground: not airborne, nor lifted off it by a clip's root motion (a jump attack).
    let off_ground = a.airborne || a.lift_ground.is_some_and(|g| pos.y > g + 0.05) || damage.is_some_and(|d| d.aerial);
    set(201, None, (!off_ground) as i64 as f64);
    // IsTrulyLanding (env 248, the physics module's ground probe; Wolf's in hks_wolf.rs): on the
    // ground again. c9997 LandingDecision reads it for the jump attacks alone (W_JumpAttack32n2 out
    // of 32n0 / the 32n1 fall loop); unanswered, Lady Butterfly's 3201 / 3291 looped for good.
    set(248, None, (!off_ground) as i64 as f64);
    set(200, None, (a.airborne && a.vel_y < 0.0) as i64 as f64);
    set(333, None, 1000.0 / 60.0);
    // env 2000 (can cancel into movement): the TAE's "End If AI Move Queued" (flag 78), the
    // window enemy.rs leaves an anim by for a move. gap: the exe's NPC side of env 2000.
    set(2000, None, d.flag(&shown, t, FLAG_AI_MOVE) as i64 as f64);
    // Its SpEffects: worn, the clip's TAE ones, the AI state's; env 3036 by behaviour ref.
    let mut ids: Vec<i64> = crate::combat::worn_sp(a, &e.clash);
    ids.extend(d.sp_effects_at(&shown, t).iter().map(|(id, _)| *id));
    ids.push(e.ai_state_sp());
    ids.retain(|id| !crate::enemy::AI_STATE_SP.contains(id) || *id == e.ai_state_sp());
    for id in ids {
        let r = d.sp_effects.get(&id.to_string()).map_or_else(|| crate::data::sp_effect_behavior_ref(id), |s| s.behavior_ref_id);
        set(3036, Some(r), 1.0);
        set(1116, Some(id), 1.0);
    }
    set(3036, None, 0.0);
    set(1116, None, 0.0);
    // This frame's hit (ExecDamage): 202 type (DAMAGE_TYPE_NONE 99999 without one), 236 level,
    // 256 damaged at all, 222 direction, 3056 / 3057 the guard / deflect side, 258 NpcParam
    // guardLevel.
    let dm = damage.unwrap_or(DamageEnv::NONE);
    set(202, None, dm.kind as f64);
    set(236, None, dm.level as f64);
    set(256, None, damage.is_some() as i64 as f64);
    set(222, None, dm.dir as f64);
    // 3032: DIRECTION_FB_BACK 1 from behind, else FORWARD 0.
    set(3032, None, (dm.dir == 3) as i64 as f64);
    set(3056, None, dm.guard_dir as f64);
    set(3057, None, dm.just_dir as f64);
    set(3018, None, dm.bound as f64);
    set(3019, None, dm.just_bound as f64);
    set(258, None, combat.npc(a)["guardLevel"].as_i64().unwrap_or(4) as f64);
    // No talk (the talk ESDs are not run): env 3053 TALK_BEH_IDENTIFIER__NOT_TALKING -1 (an
    // unanswered 0 read as TALK_PLAY_00: c1010's 21000 left for its talk 20010), 3054 no talk
    // ref.
    set(3053, None, -1.0);
    set(3054, None, 0.0);
    // A throw the engine runs (ThrowParam): env 273 its anim id, 206 in a throw (no death or
    // damage check then: ThrowCommonFunction), 1112 a throw requested (its first frame:
    // ExecPassiveTransition then leaves the clip to the throw, before ExecDeath). The engine
    // fires the throw's events itself, the death's too: the graph has W_ThrowDefDeath12001 but
    // not the W_ThrowDefDeath12000 that 276's "W_ThrowDefDeath" .. ThrowID would fire (inferred
    // engine-side, as sekiro-rs duel.rs does), so 276 stays off. 273 follows the throw's state
    // byte (case 0x111 -> FUN_140bdfb20): atkAnimId, defAnimId, and once escaped atkAnimId + 2
    // (enemy.rs's ThrowRelease<grab + 2>: the zombie's 4122) / defAnimId + 3.
    let throw = ["ThrowDefDeath", "ThrowAtk", "ThrowDef", "ThrowRelease"].iter().find_map(|p| a.state.strip_prefix(p).and_then(|n| n.parse::<i64>().ok()).map(|n| (*p, n)));
    set(273, None, throw.filter(|(p, _)| *p != "ThrowDefDeath").map_or(0, |(_, n)| n) as f64);
    set(206, None, throw.is_some() as i64 as f64);
    set(276, None, 0.0);
    // env 275: Wolf got out of its grab (combat.rs resolve_throws: his ThrowEscape) ->
    // ThrowAtkCommonFunction's W_ThrowRelease<ThrowID>.
    set(275, None, wolf.escaped as i64 as f64);
    set(1112, None, throw_start as i64 as f64);
    // env 1114 (IsExistAnime): the chr has the clip (anim id = category * 1e6 + number).
    // The exe (case 0x45a, 140b3.c:10600) tries the id plus each anim offset first
    // (FUN_140988510: animmgr +0x60..0x6c), then the raw id: an a100 enemy's IsExistAnime(4120)
    // is its a100_004120. gap: what fills the other offset slots.
    let set_off = a.anim_group as i64 * 100 * 1_000_000;
    for &id in anims {
        set(1114, Some(id), 1.0);
        if set_off > 0 && id / 1_000_000 * 1_000_000 == set_off {
            set(1114, Some(id - set_off), 1.0);
        }
    }
    set(1114, None, 0.0);
    // The AI's request: env 106 in idle / move states, the cancel slots while the TAE's AI
    // cancel window for its kind is open (107 combo, 120 attack, 118 step, 123 endure).
    let action = if cmd.guard { ACTION_TYPE_GUARD } else if cmd.general { ACTION_TYPE_GENERAL_STANCE } else { request.unwrap_or(0) as f64 };
    set(106, None, action);
    // env 330: RequestEmergencyQuickTurn (a turn clip under 45 degrees too).
    set(330, None, cmd.emergency as i64 as f64);
    if let Some(n) = request {
        let step = (5000..6000).contains(&n);
        let open = |f: i64| d.flag(&shown, t, f);
        if open(FLAG_COMBO) && !step {
            set(107, None, n as f64);
        }
        if open(FLAG_ATTACK) && !step {
            set(120, None, n as f64);
        }
        if open(FLAG_STEP) && step {
            set(118, None, n as f64);
        }
        // 123: an EndureAttack goal's request (Goal.Parry's guard / deflect), taken first.
        if open(crate::ai::FLAG_ENDURE) && cmd.endure {
            set(123, None, n as f64);
        }
    }
    h.env = env;
    // The move variables (sekiro-rs NpcControl: walk up to 0.75, run above; MoveAngle F 0,
    // B 180, L -90, R 90). gap: the exe's NPC writes of these are not traced.
    let (level, angle) = match cmd.mv {
        Some(m) => (if cmd.walk { 0.5 } else { 1.0 }, match m { 'B' => 180.0, 'L' => -90.0, 'R' => 90.0, _ => 0.0 }),
        None => (0.0, 0.0),
    };
    // TurnAngle: from its facing to the point it is told to face (the request's, else Wolf),
    // degrees, right positive; 0 while the AI asks no turn. Facing it already reads 0: float
    // noise (7e-8 here, exactly 0 in enemy.rs's yaw math) would make an emergency ExecTurn
    // turn 90 (any |angle| > 0 does).
    let turn = if cmd.face || cmd.turn {
        let to = (cmd.at.unwrap_or(wolf.pos) - pos).with_y(0.0).normalize_or_zero();
        let fwd = a.forward().with_y(0.0).normalize_or_zero();
        let deg = (fwd.x * to.z - fwd.z * to.x).atan2(fwd.dot(to)).to_degrees() as f64;
        if deg.abs() < 1e-3 { 0.0 } else { deg }
    } else {
        0.0
    };
    h.vars.insert("MoveSpeedLevel".into(), level);
    h.vars.insert("MoveAngle".into(), angle);
    h.vars.insert("TurnAngle".into(), turn);
}

/// A chr's files: its c9997.hkx, its clip lengths and the clips IsExistAnime knows (the clip
/// files' and the TAE's, aliases included: c1060's DamageAerialFront a000_008140).
pub struct ChrFiles {
    pub graph: Arc<sekiro_formats::hkb::Graph>,
    pub lengths: HashMap<String, f32>,
    pub anims: Vec<i64>,
    /// The compiled chunks: c9997.hks, then c<chr>.hks.
    pub own: String,
}

fn anim_id(k: &str) -> Option<i64> {
    let (cat, num) = k.strip_prefix('a')?.split_once('_')?;
    Some(cat.parse::<i64>().ok()? * 1_000_000 + num.parse::<i64>().ok()?)
}

impl ChrFiles {
    /// `id`'s files (c1020), with `d`'s TAE anims. None without the extracted data.
    pub fn load(id: &str, d: &crate::data::CharData) -> Option<Self> {
        let x = crate::paths::extracted();
        let graph = Arc::new(sekiro_formats::hkb::parse(&std::fs::read(x.join(format!("chr/{id}.behbnd.d/c9997.hkx"))).ok()?).ok()?);
        let lengths: HashMap<String, f32> = serde_json::from_str(&std::fs::read_to_string(x.join(format!("clip_lengths/{id}.json"))).ok()?).ok()?;
        let mut anims: Vec<i64> = lengths.keys().chain(d.anims.keys()).filter_map(|k| anim_id(k)).collect();
        anims.sort_unstable();
        anims.dedup();
        Some(Self { graph, lengths, anims, own: format!("{id}.hks") })
    }

    pub fn script(&self) -> Option<ScriptChr> {
        let vm = HksVm::load_chunks(&crate::paths::extracted().join("action/script"), &["c9997.hks", self.own.as_str()]).ok()?;
        let rt = crate::hkb_runtime::BehaviorRuntime::new(self.graph.clone(), crate::hkb_runtime::AnimOffsets::default());
        ScriptChr::new(vm, rt).ok()
    }
}

/// Each enemy's own script (SHINOBI_HKS=0: off) on its graph, beside enemy.rs, after its decide (the
/// AI thinks before the script reads its request). Where the game's clip and enemy.rs's differ
/// the graph's state and clip win (SHINOBI_HKS_ENEMY_LEAD=0: enemy.rs keeps its own and the
/// runner only counts them, `EnemyScripts::off`); SHINOBI_HKS_TRACE=1 logs them.
pub struct HksEnemyPlugin;

impl Plugin for HksEnemyPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("SHINOBI_HKS").is_ok_and(|v| v == "0") {
            return;
        }
        let trace = std::env::var("SHINOBI_HKS_TRACE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let lead = std::env::var("SHINOBI_HKS_ENEMY_LEAD").map_or(true, |v| v != "0");
        app.insert_non_send(EnemyScripts { trace, lead, ..Default::default() })
            .add_systems(FixedUpdate, run_scripts.after(crate::actor::ActorSet::Decide).before(crate::actor::ActorSet::Advance));
    }
}

/// One enemy's script and what the runner keeps between frames.
pub struct EnemyScript {
    chr: ScriptChr,
    files: Arc<ChrFiles>,
    /// The AI's attack request, posted until the script plays it.
    request: Option<i64>,
    prev_clip: Option<(String, f32)>,
    /// enemy.rs's state after the last run.
    state_after: String,
    anim_after: String,
    /// enemy.rs is in a state the graph has no event for: not counted till they meet again.
    stood_aside: bool,
}

#[derive(Default)]
pub struct EnemyScripts {
    /// By the chr whose behaviour it uses (Foe::anim_chr); None: no files.
    files: HashMap<String, Option<Arc<ChrFiles>>>,
    chrs: HashMap<Entity, EnemyScript>,
    /// Frames the game's clip and enemy.rs's differed, frames enemy.rs was in a state the graph
    /// has no event for, frames the graph's dead body lay (ragdoll) where enemy.rs holds its
    /// death clip, and frames run.
    pub off: u32,
    pub aside: u32,
    pub ragdoll: u32,
    /// enemy.rs's picks the graph overrode (`lead`).
    pub overrides: u32,
    lead: bool,
    pub frames: u32,
    trace: u8,
}

/// The AI's command, or the sandbox partner's walk (enemy.rs `passive_or_fallback` / `drill`, our
/// practice modes) read as one: a forward walk (it turns to Wolf itself, no turn clips). Their
/// attacks, like any anim forced without an AI request (`Enemy::force_anim`), go in as the
/// engine's W_Event<n> below.
fn sandbox_command(a: &Actor, e: &Enemy) -> crate::ai::Command {
    let mut cmd = e.ai_cmd.clone();
    if cmd.anim.is_none() && cmd.mv.is_none() && a.state == "WalkFrontBattle" {
        (cmd.mv, cmd.walk) = (Some('F'), true);
    }
    cmd
}

fn run_scripts(scripts: Option<NonSendMut<EnemyScripts>>, time: Res<Time>, combat: Res<Combat>, wolf: Single<(&Transform, &Actor), With<Player>>, mut enemies: Query<(Entity, &mut Actor, &Enemy, &Transform), Without<Player>>) {
    // (The lockstep tests run the script themselves and take it out.)
    let Some(mut scripts) = scripts else { return };
    let s = &mut *scripts;
    let (wtf, wa) = *wolf;
    let seen = WolfSeen { pos: wtf.translation, escaped: wa.state == "ThrowEscape" };
    let tick = time.delta_secs();
    s.chrs.retain(|e, _| enemies.contains(*e));
    for (ent, mut a, e, tf) in &mut enemies {
        // The exe's character update level (enemy/world.rs update_lod): a far one runs every
        // 5th / 30th tick, that many ticks' time.
        if e.is_disabled() || !a.lod_due {
            continue;
        }
        let dt = tick * a.lod.max(1) as f32;
        if s.trace > 0 && s.frames > 0 && s.frames % 600 == 0 {
            let (busy, ticks) = s.chrs.values().fold((0.0, 0u32), |(b, n), es| (b + es.chr.busy.as_secs_f64(), n + es.chr.ticks));
            eprintln!("hks enemies: {} frames, {} differ, {} aside, {} ragdoll, {} overridden; {:.0} us a tick", s.frames, s.off, s.aside, s.ragdoll, s.overrides, busy * 1e6 / ticks.max(1) as f64);
        }
        let d = combat.data_of(&a);
        let chr_id = combat.foe_of(&a).anim_chr.clone();
        let Some(files) = s.files.entry(chr_id.clone()).or_insert_with(|| ChrFiles::load(&chr_id, d).map(Arc::new)).clone() else { continue };
        let lengths = files.lengths.clone();
        let mut dur = |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)));
        if !s.chrs.contains_key(&ent) {
            let Some(mut chr) = files.script() else { continue };
            // The script's own start (InitState -> W_IdleBattle) before it reads a request.
            let clip = chr.rt.main_clip().map(|c| (c.animation.clone(), c.time));
            answers(&mut chr.vm.host.borrow_mut(), &a, e, &e.ai_cmd, tf.translation, seen, &combat, clip, &files.anims, None, None, false);
            if let Err(err) = chr.tick(1.0 / 60.0, &mut dur) {
                warn!("SHINOBI_HKS: {chr_id}'s script: {err}");
                continue;
            }
            s.chrs.insert(ent, EnemyScript { chr, files: files.clone(), request: None, prev_clip: None, state_after: String::new(), anim_after: String::new(), stood_aside: false });
        }
        let es = s.chrs.get_mut(&ent).unwrap();
        // offsetType 15: its anim group (Actor::grouped).
        es.chr.rt.offsets.anim = a.anim_group as i64 * 100;
        let clip = es.chr.rt.main_clip().map(|c| (c.animation.clone(), c.time));
        let cmd = sandbox_command(&a, e);
        if let Some(n) = cmd.anim {
            es.request = Some(n);
        }
        // Played: the script (re)started that clip since the last frame.
        let started = clip.as_ref().is_some_and(|(c, t)| es.prev_clip.as_ref().is_none_or(|(pc, pt)| pc != c || t < pt));
        if started && es.request.is_some_and(|n| clip.as_ref().is_some_and(|(c, _)| c.ends_with(&format!("_{:06}", n % 1_000_000)))) {
            es.request = None;
        }
        es.prev_clip = clip.clone();
        let damage = a.damage_env.take();
        // The engine starts a throw on the graph itself (W_ThrowAtk<id> / W_ThrowDef<id>).
        let throw_start = a.state != es.state_after && (a.state.starts_with("ThrowAtk") || a.state.starts_with("ThrowDef"));
        if throw_start {
            es.chr.rt.fire_event(&format!("W_{}", a.state));
        }
        answers(&mut es.chr.vm.host.borrow_mut(), &a, e, &cmd, tf.translation, seen, &combat, clip, &es.files.anims, es.request, damage, throw_start);
        match es.chr.tick(dt, &mut dur) {
            Ok(events) => {
                if s.trace >= 3 && (damage.is_some() || !events.is_empty()) {
                    let h = es.chr.vm.host.borrow();
                    eprintln!("hks enemy {chr_id} {ent}: {} {} t {:.2} hit {damage:?} fired {events:?} -> {} (hp {:?} {})", a.state, a.anim, a.t, es.chr.rt.main_state_name(), h.env.get(&(1000, None)), a.hp);
                }
            }
            Err(err) => warn!("SHINOBI_HKS: {chr_id}'s script: {err}"),
        }
        // A state enemy.rs started outside the AI's requests (a map event, a throw's end, a hit
        // the graph took otherwise) goes in as the exe's events: W_Event<anim>, the throw pair,
        // W_<state>. None of them: the graph stands aside until the two meet again.
        let changed = a.state != es.state_after || a.anim != es.anim_after;
        es.state_after = a.state.clone();
        es.anim_after = a.anim.clone();
        s.frames += 1;
        let ours = a.shown_clip(d).0;
        let theirs = |es: &EnemyScript| es.chr.rt.main_clip().map(|c| c.animation.clone()).unwrap_or_default();
        let n = a.anim.rsplit('_').next().and_then(|n| n.parse::<u32>().ok());
        // The AI's own request is the script's to play (it may take a frame).
        let requested = n.is_some_and(|n| es.request.is_some_and(|r| r % 1_000_000 == n as i64));
        if changed && !requested && theirs(es) != ours {
            let mut names: Vec<String> = n.map(|n| ["Event", "ThrowAtk", "ThrowDef"].iter().map(|k| format!("W_{k}{n}")).collect()).unwrap_or_default();
            names.push(format!("W_{}", a.state));
            let synced = names.into_iter().find(|ev| es.chr.engine_event(ev, &mut dur).unwrap_or(false));
            es.stood_aside = synced.is_none();
            if s.trace > 0 {
                eprintln!("hks enemy {chr_id} {ent}: engine {} {} ({})", a.state, a.anim, synced.as_deref().unwrap_or("not in the graph"));
            }
        }
        let theirs = theirs(es);
        // The same clip from another anim set (a100_003011 / a200_003011): enemy.rs's pick of
        // the set stands. gap: where the exe's offsetType 15 takes another set than Actor::anim_group.
        let same_number = |x: &str, y: &str| x.split_once('_').map(|p| p.1) == y.split_once('_').map(|p| p.1);
        if theirs == ours || same_number(&theirs, &ours) {
            es.stood_aside = false;
        } else if es.stood_aside || e.is_down() || (e.is_dead() && a.state.starts_with("Event")) {
            // Held down by its map script (immortal at 0 bars in its 20200, enemy.rs Mode::Held),
            // or its map script playing events on its body (Isshin's 21000 before the finisher):
            // the exe's side, not the script's. gap: how the exe keeps c9997's ExecDeath off then.
            s.aside += 1;
        } else if es.chr.rt.main_state_name() == "DeathIdleDefault" && a.state.contains("Death") {
            s.ragdoll += 1;
        } else {
            s.off += 1;
            if s.trace > 0 {
                eprintln!("hks enemy {chr_id} {ent}: ours {} {ours} {:.2} | theirs {} {theirs} | mv {:?} walk {} anim {:?} 2000 {:?}", a.state, a.t, es.chr.rt.main_state_name(), cmd.mv, cmd.walk, cmd.anim, es.chr.vm.host.borrow().env.get(&(2000, None)));
            }
            // The graph's state and clip win, as Wolf's do (`lead`). Both just
            // standing, moving or turning: enemy.rs keeps its own; its clip ending this step:
            // it leaves it itself.
            let state = es.chr.rt.main_state_name().to_string();
            let ending = !a.anim.is_empty() && a.t + dt >= d.length(&a.anim);
            if s.lead && !(free(&a.state) && free(&state)) && !ending && d.anim(&theirs).is_some() {
                let t = es.chr.rt.main_clip().map_or(0.0, |c| c.time);
                a.play(&state, &theirs);
                a.t = (t - dt).max(0.0);
                a.prev_t = a.t;
                s.overrides += 1;
            }
        }
    }
}

/// A state with the enemy standing, moving or turning (enemy.rs's and the graph's names).
fn free(state: &str) -> bool {
    ["Idle", "StandIdle", "Approach", "Walk", "Run", "Turn", "Move"].iter().any(|p| state.starts_with(p))
}
