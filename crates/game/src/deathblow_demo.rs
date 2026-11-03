//! `--arena --deathblow`: the soldier's posture breaks, Wolf performs the front deathblow.
//!
//! The rules live in `sekiro_sim::deathblow` (ThrowParam rows, eligibility, paired timeline and
//! the defender's alignment to Wolf's sorb dummy); this module only drives the two animators
//! and places the soldier where the simulation says.

use std::path::Path;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::prelude::*;
use sekiro_sim::deathblow::{
    Deathblow, DummyFrame, Phase, Placement, Situation, ThrowTable, ThrowTimes, deathblow_marks,
    select,
};

use crate::arena::{Libraries, Soldier, Wolf};
use crate::character::{Animator, Rig};

/// Seconds after the rigs bind when the soldier's posture breaks.
const BREAK_AT: f32 = 1.0;
/// Seconds into the stagger when Wolf presses attack.
const ATTACK_AFTER: f32 = 0.6;
/// The front posture-break stagger (behaviour graph `TrunkCollapseFront`).
const POSTURE_BREAK_CLIP: &str = "a000_008300";
const SOLDIER_CHR: i32 = 1010;

#[derive(Resource)]
pub struct DeathblowDemo {
    table: ThrowTable,
    sorb_dummies: sekiro_formats::flver::Flver,
    cache: std::path::PathBuf,
    clock: f32,
    stage: Stage,
    db: Option<Deathblow>,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Waiting,
    Staggered,
    Deathblow,
    Finished,
}

impl DeathblowDemo {
    pub fn load(cache: &Path) -> Result<Self> {
        let table = ThrowTable::load_from_cache(cache).context("ThrowParam")?;
        let flver = std::fs::read(cache.join("raw/chr/c0000.chrbnd.d/c0000.flver"))?;
        Ok(Self {
            table,
            sorb_dummies: sekiro_formats::flver::parse(&flver)?,
            cache: cache.to_path_buf(),
            clock: 0.0,
            stage: Stage::Waiting,
            db: None,
            status: "waiting".into(),
        })
    }
}

fn placement(t: &Transform) -> Placement {
    let (yaw, _, _) = t.rotation.to_euler(EulerRot::YXZ);
    Placement {
        position: t.translation.to_array(),
        yaw,
    }
}

/// Runs after `animate`, so Wolf has moved by this frame's root motion.
#[allow(clippy::type_complexity)]
pub fn drive(
    time: Res<Time>,
    mut demo: ResMut<DeathblowDemo>,
    mut libs: ResMut<Libraries>,
    mut wolf: Query<(&mut Animator, &mut Transform, &Rig), (With<Wolf>, Without<Soldier>)>,
    mut soldier: Query<(&mut Animator, &mut Transform, &Rig), (With<Soldier>, Without<Wolf>)>,
) {
    let (Ok((mut w_anim, mut w_tf, w_rig)), Ok((mut s_anim, mut s_tf, s_rig))) =
        (wolf.single_mut(), soldier.single_mut())
    else {
        return;
    };
    if !w_rig.bound || !s_rig.bound {
        return;
    }
    let dt = time.delta_secs();
    demo.clock += dt;
    match demo.stage {
        Stage::Waiting if demo.clock >= BREAK_AT => {
            s_anim.play(libs.soldier.clip(POSTURE_BREAK_CLIP).unwrap(), false);
            s_anim.apply_root_motion = true;
            demo.stage = Stage::Staggered;
            demo.status = format!("posture broken ({POSTURE_BREAK_CLIP})");
        }
        Stage::Staggered if demo.clock >= BREAK_AT + ATTACK_AFTER => {
            let attacker = placement(&w_tf);
            let defender = placement(&s_tf);
            let Some((start, body)) = select(
                &demo.table,
                SOLDIER_CHR,
                0,
                Situation::PostureBroken,
                &attacker,
                &defender,
            ) else {
                demo.status = "out of deathblow range".into();
                demo.stage = Stage::Finished;
                return;
            };
            let (start, body) = (start.clone(), body.clone());
            let times = ThrowTimes::load(
                &demo.cache,
                "c0000",
                &start.attacker_anim(),
                &body.attacker_anim(),
                body.defender_anim().as_deref().map(|d| ("c1010", d)),
            )
            .expect("throw TAE events");
            let sorb = DummyFrame::from_flver(&demo.sorb_dummies, body.atk_sorb_dummy)
                .expect("sorb dummy");
            let body_duration = libs.wolf.clip(&body.attacker_anim()).unwrap().duration;
            // NpcParam 10100000 ninsatuNum is 0: one deathblow kills.
            let (db, first) = Deathblow::begin((&start, &body), times, sorb, body_duration, {
                deathblow_marks(0)
            });
            if let Some(yaw) = db.start_facing(&attacker, &defender) {
                w_tf.rotation = Quat::from_rotation_y(yaw);
            }
            if let Some(c) = first.attacker_clip {
                w_anim.play(libs.wolf.clip(&c).unwrap(), false);
                w_anim.apply_root_motion = true;
            }
            demo.status = format!("deathblow rows {} / {}", start.id, body.id);
            demo.db = Some(db);
            demo.stage = Stage::Deathblow;
        }
        Stage::Deathblow => {
            let attacker = placement(&w_tf);
            let defender = placement(&s_tf);
            let Some(db) = demo.db.as_mut() else { return };
            let step = db.step(dt, &attacker, &defender);
            let phase = db.phase;
            if let Some(c) = &step.attacker_clip {
                w_anim.play(libs.wolf.clip(c).unwrap(), false);
            }
            if let Some(c) = &step.defender_clip {
                s_anim.play(libs.soldier.clip(c).unwrap(), false);
                s_anim.apply_root_motion = true;
            }
            // The soldier's rows absorb the defender onto Wolf's dummy; both then play their
            // own root motion (flat floor, no collision here).
            if let Some(a) = &step.absorb {
                s_tf.translation += Vec3::from_array(a.delta);
                s_tf.rotate_y(a.yaw);
            }
            if step.killed {
                demo.status = "deathblow landed: soldier killed".into();
            }
            match phase {
                Phase::Done => {
                    demo.stage = Stage::Finished;
                }
                Phase::Missed => {
                    demo.status = "grab missed".into();
                    demo.stage = Stage::Finished;
                }
                _ => {}
            }
        }
        Stage::Finished if w_anim.finished() && w_anim.clip_name() != "a000_000000" => {
            w_anim.play(libs.wolf.clip("a000_000000").unwrap(), true);
            w_anim.apply_root_motion = false;
        }
        _ => {}
    }
}

/// Keeps the camera on the pair, from the side, so the alignment is visible.
#[allow(clippy::type_complexity)]
pub fn follow_camera(
    actors: Query<&Transform, Or<(With<Wolf>, With<Soldier>)>>,
    mut camera: Query<&mut Transform, (With<Camera3d>, Without<Wolf>, Without<Soldier>)>,
) {
    let points: Vec<Vec3> = actors.iter().map(|t| t.translation).collect();
    if points.len() < 2 {
        return;
    }
    let mid = points.iter().copied().sum::<Vec3>() / points.len() as f32;
    let target = mid + Vec3::Y * 1.0;
    for mut cam in &mut camera {
        *cam = Transform::from_translation(target + Vec3::new(3.2, 0.6, 1.4))
            .looking_at(target, Vec3::Y);
    }
}
