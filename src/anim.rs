//! Real animation playback: skeletons and spline-decoded clips exported by
//! tools/sekiro-extract (extracted/anim_<chr>.bin), posed every frame from the
//! actor's current anim and clock, drawn as bone lines.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::actor::{Actor, Side, data_for};
use crate::data::{CharData, Combat};
use crate::player::CAPSULE_HALF_HEIGHT;

pub struct BoneDef {
    pub name: String,
    pub parent: i16,
    pub pose: Transform,
}

pub struct Clip {
    pub frame_duration: f32,
    pub frames: usize,
    pub track_to_bone: Vec<i16>,
    /// frames * tracks transforms (game space).
    pub data: Vec<Transform>,
}

#[derive(Default)]
pub struct Lib {
    pub bones: Vec<BoneDef>,
    pub clips: HashMap<String, Clip>,
}

#[derive(Resource, Default)]
pub struct AnimLib {
    pub player: Lib,
    pub enemy: Lib,
}

impl AnimLib {
    pub fn lib(&self, side: Side) -> &Lib {
        match side {
            Side::Player => &self.player,
            Side::Enemy => &self.enemy,
        }
    }
}

/// Havok -> game space: mirror X (same convention as the root motion export).
fn convert(v: &[f32]) -> Transform {
    Transform {
        translation: Vec3::new(-v[0], v[1], v[2]),
        rotation: Quat::from_xyzw(v[3], -v[4], -v[5], v[6]).normalize(),
        scale: Vec3::new(v[7], v[8], v[9]),
    }
}

fn load(path: &std::path::Path) -> Lib {
    let Ok(d) = std::fs::read(path) else {
        warn!("{} missing; characters will show as capsules", path.display());
        return Lib::default();
    };
    let u16_ = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let f32s = |o: usize, n: usize| -> Vec<f32> { (0..n).map(|i| f32::from_le_bytes(d[o + i * 4..o + i * 4 + 4].try_into().unwrap())).collect() };
    assert_eq!(&d[0..4], b"SHAN");
    let mut o = 8;
    let nb = u32_(o) as usize;
    o += 4;
    let mut bones = Vec::with_capacity(nb);
    for _ in 0..nb {
        let l = u16_(o) as usize;
        o += 2;
        let name = String::from_utf8_lossy(&d[o..o + l]).into_owned();
        o += l;
        let parent = u16_(o) as i16;
        o += 2;
        let pose = convert(&f32s(o, 10));
        o += 40;
        bones.push(BoneDef { name, parent, pose });
    }
    let nc = u32_(o) as usize;
    o += 4;
    let mut clips = HashMap::with_capacity(nc);
    for _ in 0..nc {
        let l = u16_(o) as usize;
        o += 2;
        let key = String::from_utf8_lossy(&d[o..o + l]).into_owned();
        o += l;
        let frame_duration = f32s(o, 1)[0];
        let frames = u32_(o + 4) as usize;
        let tracks = u32_(o + 8) as usize;
        o += 12;
        let track_to_bone: Vec<i16> = (0..tracks).map(|i| u16_(o + i * 2) as i16).collect();
        o += tracks * 2;
        let mut data = Vec::with_capacity(frames * tracks);
        for k in 0..frames * tracks {
            data.push(convert(&f32s(o + k * 40, 10)));
        }
        o += frames * tracks * 40;
        clips.insert(key, Clip { frame_duration, frames, track_to_bone, data });
    }
    Lib { bones, clips }
}

/// Skeleton construction (models attach after it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnimSet;

/// Marks a character whose real mesh is attached (bone lines are then hidden).
#[derive(Component)]
pub struct HasModel;

/// Hurtbox capsules: two marker entities (endpoints, parented to the bone) and the
/// radius, from the ragdoll bodies in the character's physics HKX.
#[derive(Component)]
pub struct Hurtboxes(pub Vec<(Entity, Entity, f32)>);

/// Bone entities of one character, indexed like the skeleton.
#[derive(Component)]
pub struct Skeleton {
    pub bones: Vec<Entity>,
    /// Clip shown last frame, and the pose to crossfade from after a clip change.
    pub(crate) shown: String,
    from: Vec<Transform>,
    /// Last pose written by pose_skeletons (before the twist layer), the start of crossfades:
    /// snapshotting the bone transforms instead would bake that frame's twist into the fade.
    last: Vec<Transform>,
    pub(crate) blend: f32,
    /// Crossfade length into the current clip (TAE Blend event, else CROSSFADE).
    blend_len: f32,
    /// Last frame showed an upper action (its legs were the walk/run loop already).
    was_upper: bool,
    /// The running crossfade leaves the legs alone (upper action -> its move loop: the legs keep
    /// the same loop at the same time, so only the upper body fades).
    upper_fade: bool,
}

/// Crossfade into an anim: its TAE "Blend" event (type 16, frames 0-N; most anims have
/// one: player 3-6 frames, c1020 1-15) or, without one, this default.
/// Behavior-graph transition effects all have duration 0 (sekiro-extract transitions), so
/// crossfades come from the target clip's TAE Blend; this is only for clips without TAE.
const CROSSFADE: f32 = 0.12;

pub struct AnimPlugin;

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        let dir = crate::paths::root().join("extracted");
        let chr = app.world().get_resource::<Combat>().map_or("c1020".to_string(), |c| c.foe.chr.clone());
        app.insert_resource(AnimLib { player: load(&dir.join("anim_c0000.bin")), enemy: load(&dir.join(format!("anim_{chr}.bin"))) })
            .add_systems(PostStartup, build_skeletons.in_set(AnimSet).after(crate::player::player_visuals).after(crate::enemy::enemy_visuals))
            .add_systems(Update, (pose_skeletons, apply_walk_twist, apply_twists, draw_skeletons).chain().before(TransformSystems::Propagate));
    }
}

fn build_skeletons(
    mut commands: Commands,
    lib: Res<AnimLib>,
    combat: Res<Combat>,
    actors: Query<(Entity, &Actor, Option<&Children>), Without<Skeleton>>,
    blades: Query<(), With<crate::world::Blade>>,
) {
    for (e, a, children) in &actors {
        let l = lib.lib(a.side);
        if l.bones.is_empty() {
            continue;
        }
        // Feet level: the actor's origin is the capsule middle.
        let root = commands.spawn((Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0), Visibility::default(), Name::new("SkeletonRoot"))).id();
        commands.entity(e).add_child(root);
        let bones: Vec<Entity> = l
            .bones
            .iter()
            .map(|b| commands.spawn((b.pose, Visibility::default(), Name::new(b.name.clone()))).id())
            .collect();
        for (i, b) in l.bones.iter().enumerate() {
            let parent = if b.parent >= 0 { bones[b.parent as usize] } else { root };
            commands.entity(parent).add_child(bones[i]);
        }
        // Blade onto the weapon bone. Player: R_Weapon, blade along its local -Y.
        // Enemy: R_Hand, blade toward the R_Katana_long tip bone.
        let find = |n: &str| l.bones.iter().position(|b| b.name == n);
        let mount = match a.side {
            Side::Player => find("R_Weapon").map(|i| (i, Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)))),
            Side::Enemy => find("R_Hand").zip(find("R_Katana_long")).map(|(hand, tip)| {
                let dir = l.bones[tip].pose.translation;
                let rot = Quat::from_rotation_arc(Vec3::NEG_Z, dir.normalize());
                (hand, Transform::from_rotation(rot).with_scale(Vec3::new(1.0, 1.0, dir.length())))
            }),
        };
        if let (Some((bone, tf)), Some(children)) = (mount, children) {
            for c in children.iter().filter(|c| blades.contains(*c)) {
                commands.entity(bones[bone]).add_child(c);
                commands.entity(c).insert(tf);
            }
        }
        // Hurtboxes: ragdoll capsules (model space, bind pose) re-expressed in their bone's
        // bind space, so they follow the animation.
        let mut bind: Vec<Transform> = Vec::with_capacity(l.bones.len());
        for (i, b) in l.bones.iter().enumerate() {
            let parent = if b.parent >= 0 && (b.parent as usize) < i { bind[b.parent as usize] } else { Transform::IDENTITY };
            bind.push(parent.mul_transform(b.pose));
        }
        let mut boxes = Vec::new();
        for h in &data_for(&combat, a.side).hurtboxes {
            let Some(bi) = find(&h.bone) else { continue };
            let inv = bind[bi].compute_affine().inverse();
            let mut marker = |p: [f32; 3]| {
                let local = inv.transform_point3(Vec3::new(-p[0], p[1], p[2]));
                let m = commands.spawn((Transform::from_translation(local), Name::new("Hurtbox"))).id();
                commands.entity(bones[bi]).add_child(m);
                m
            };
            let (ma, mb) = (marker(h.a), marker(h.b));
            boxes.push((ma, mb, h.r));
        }
        if !boxes.is_empty() {
            commands.entity(e).insert(Hurtboxes(boxes));
        }
        // The skeleton replaces the placeholder capsule.
        commands.entity(e).insert(Skeleton { bones, shown: String::new(), from: Vec::new(), last: Vec::new(), blend: 1.0, blend_len: CROSSFADE, was_upper: false, upper_fade: false }).remove::<Mesh3d>();
    }
}

/// Clip to show for an actor: its current anim (or the sibling it borrows a clip
/// from), or a loop for procedural states.
/// `t` is the anim time to draw (draw_time).
fn clip_for<'a>(lib: &'a Lib, d: &CharData, a: &Actor, t: f32) -> Option<(&'a Clip, f32)> {
    if let Some(c) = lib.clips.get(d.clip_key(&a.anim)) {
        return Some((c, t));
    }
    let (key, t) = proc_clip(a, t);
    lib.clips.get(&key).map(|c| (c, c.wrap(t)))
}

impl Clip {
    /// Loop time: Havok loops repeat frame 0 as their last frame (idle, walk/run loops, guard
    /// idle: last vs first < 0.4 deg), so a cycle is frames - 1 intervals long.
    fn wrap(&self, t: f32) -> f32 {
        let len = (self.frames.max(2) - 1) as f32 * self.frame_duration;
        t.rem_euclid(len.max(1e-3))
    }
}

/// Anim time drawn this frame: between the last two simulated steps, like the actor's position
/// (interp.rs), so poses move every screen frame instead of in 60 Hz steps.
fn draw_time(a: &Actor, alpha: Option<f32>) -> f32 {
    match alpha {
        Some(k) if a.prev_t <= a.t => a.prev_t + (a.t - a.prev_t) * k,
        _ => a.t,
    }
}

/// The clip a procedural state shows and the time in it (before looping): locomotion = the move
/// start, then the loop (Actor::move_clip), else idle.
fn proc_clip(a: &Actor, t: f32) -> (String, f32) {
    let key = match (a.side, a.state.as_str()) {
        (Side::Player, "Locomotion") => {
            let (key, t, _) = a.locomotion_clip_at(t);
            return (key, t);
        }
        (Side::Player, _) => "a000_000000",
        (Side::Enemy, "IdleBattle") | (Side::Enemy, "Approach") => "a000_400000",
        (Side::Enemy, "PostureBroken") => "a000_008550",
        (Side::Enemy, _) => "a000_400000",
    };
    (key.to_string(), t)
}

/// Upper-body layer mask (c0000.hkx StandMoveOverwrite LayerGenerator): the StandMoveUpper_SM
/// layer's boneWeights are 0 for the first 39 bones (Master, foot targets, RootPos, Pelvis, both
/// legs) and full above, so those bones keep the walk/run of StandMoveLower_SM.
const LOWER_BODY_BONES: usize = 39;

/// Key of the clip being shown: the TAE anim, or the procedural state's clip.
fn clip_key(lib: &Lib, d: &CharData, a: &Actor, t: f32) -> String {
    if lib.clips.contains_key(d.clip_key(&a.anim)) { a.anim.clone() } else { proc_clip(a, t).0 }
}

fn pose_skeletons(
    time: Res<Time>,
    alpha: Option<Res<crate::interp::DrawAlpha>>,
    lib: Res<AnimLib>,
    combat: Res<Combat>,
    mut actors: Query<(&Actor, &mut Skeleton)>,
    mut tfs: Query<&mut Transform>,
) {
    for (a, mut skel) in &mut actors {
        let l = lib.lib(a.side);
        let d = data_for(&combat, a.side);
        let drawn = draw_time(a, alpha.as_deref().map(|k| k.0));
        let Some((clip, t)) = clip_for(l, d, a, drawn) else { continue };
        let tracks = clip.track_to_bone.len();
        if tracks == 0 || clip.frames == 0 {
            continue;
        }
        // Clip changed: remember the current pose and fade from it.
        let key = clip_key(l, d, a, drawn);
        if key != skel.shown {
            skel.from = if skel.last.len() == skel.bones.len() { skel.last.clone() } else { skel.bones.iter().map(|&b| tfs.get(b).copied().unwrap_or_default()).collect() };
            // Behavior-graph transitions all have duration 0 (c0000.hkx / c9997.hkx: TaeBlend*,
            // DefaultTransition, StateToStateBlend): the blend time is the target anim's TAE Blend
            // event, and an anim without one (hit / deflect / stagger reactions) snaps in at once.
            // Locomotion is clip selection too (CMSGs; the only blenders in c0000.hkx are the
            // master layer and the ledge hang), so a walk/run/idle switch blends by the shown
            // clip's own TAE Blend (walk/run 6 frames, idle 9). CROSSFADE only for clips with no TAE.
            let blend_len = if d.anim(&key).is_some() { d.blend_in(&key).unwrap_or(0.0) } else { CROSSFADE };
            skel.blend = if skel.shown.is_empty() || blend_len <= 0.0 { 1.0 } else { 0.0 };
            skel.blend_len = blend_len.max(1.0 / 60.0);
            skel.upper_fade = skel.was_upper && a.state == "Locomotion";
            skel.shown = key;
        }
        skel.was_upper = a.side == Side::Player && crate::player::is_upper_action(&a.state);
        skel.blend = (skel.blend + time.delta_secs() / skel.blend_len).min(1.0);
        let w = skel.blend * skel.blend * (3.0 - 2.0 * skel.blend);
        let f = (t / clip.frame_duration).clamp(0.0, (clip.frames - 1) as f32);
        let (i0, k) = (f.floor() as usize, f.fract());
        let i1 = (i0 + 1).min(clip.frames - 1);
        // Additive layer (Actor.add_anim, HKS AddDamageBlend / AddDeflectGuardBlend): the clip holds
        // per-bone deltas from identity (frame 0 is identity), multiplied onto the base pose.
        let add = (!a.add_anim.is_empty()).then(|| l.clips.get(d.clip_key(&a.add_anim))).flatten().filter(|c| c.frames > 0).map(|c| {
            let f = (a.add_t / c.frame_duration).clamp(0.0, (c.frames - 1) as f32);
            let (j0, kk) = (f.floor() as usize, f.fract());
            (c, j0, (j0 + 1).min(c.frames - 1), kk)
        });
        let mut add_delta: HashMap<usize, (Vec3, Quat)> = HashMap::new();
        if let Some((c, j0, j1, kk)) = add {
            let n = c.track_to_bone.len();
            for (track, &bone) in c.track_to_bone.iter().enumerate() {
                if bone >= 0 {
                    let (p, q) = (&c.data[j0 * n + track], &c.data[j1 * n + track]);
                    add_delta.insert(bone as usize, (p.translation.lerp(q.translation, kk), p.rotation.slerp(q.rotation, kk)));
                }
            }
        }
        // Upper actions (player::is_upper_action): the legs take the walk/run clip, on the
        // action's clock (the loop continues from it when the action ends).
        let lower = (a.side == Side::Player && crate::player::is_upper_action(&a.state))
            .then(|| l.clips.get(&a.move_clip(false)))
            .flatten()
            .filter(|c| c.frames > 1)
            .map(|c| {
                let f = c.wrap(t) / c.frame_duration;
                let (j0, kk) = ((f.floor() as usize).min(c.frames - 2), f.fract());
                let mut track_of = vec![usize::MAX; skel.bones.len()];
                for (track, &bone) in c.track_to_bone.iter().enumerate() {
                    if let Some(slot) = usize::try_from(bone).ok().and_then(|b| track_of.get_mut(b)) {
                        *slot = track;
                    }
                }
                (c, j0, j0 + 1, kk, track_of)
            });
        // Bones the clip has no track for stay at the reference pose (as in Havok); resetting them
        // every frame also stops per-frame layers (TAE 700 twists) from accumulating on them.
        let mut tracked = vec![false; skel.bones.len()];
        for &bone in &clip.track_to_bone {
            if let Some(t) = usize::try_from(bone).ok().and_then(|b| tracked.get_mut(b)) {
                *t = true;
            }
        }
        for (bi, _) in tracked.iter().enumerate().filter(|(_, t)| !**t) {
            if let (Some(def), Ok(mut tf)) = (l.bones.get(bi), tfs.get_mut(skel.bones[bi])) {
                *tf = def.pose;
            }
            if let (Some(def), Some(slot)) = (l.bones.get(bi), skel.last.get_mut(bi)) {
                *slot = def.pose;
            }
        }
        for (track, &bone) in clip.track_to_bone.iter().enumerate() {
            if bone < 0 || bone as usize >= skel.bones.len() {
                continue;
            }
            let (p, q, k) = match &lower {
                Some((c, j0, j1, kk, track_of)) if (bone as usize) < LOWER_BODY_BONES && track_of[bone as usize] != usize::MAX => {
                    let (n, lt) = (c.track_to_bone.len(), track_of[bone as usize]);
                    (&c.data[j0 * n + lt], &c.data[j1 * n + lt], *kk)
                }
                _ => (&clip.data[i0 * tracks + track], &clip.data[i1 * tracks + track], k),
            };
            let target = Transform {
                translation: p.translation.lerp(q.translation, k),
                rotation: p.rotation.slerp(q.rotation, k),
                scale: p.scale.lerp(q.scale, k),
            };
            let bone = bone as usize;
            let mut out = match skel.from.get(bone) {
                Some(from) if w < 1.0 && !(skel.upper_fade && bone < LOWER_BODY_BONES) => Transform {
                    translation: from.translation.lerp(target.translation, w),
                    rotation: from.rotation.slerp(target.rotation, w),
                    scale: from.scale.lerp(target.scale, w),
                },
                _ => target,
            };
            if let Some(&(dt_, dq)) = add_delta.get(&bone) {
                out.rotation = (out.rotation * dq).normalize();
                out.translation += dt_;
            }
            if let Ok(mut tf) = tfs.get_mut(skel.bones[bone]) {
                *tf = out;
            }
            if skel.last.len() != skel.bones.len() {
                skel.last = vec![Transform::default(); skel.bones.len()];
            }
            skel.last[bone] = out;
        }
    }
}

/// Bone lines, computed from local transforms (GlobalTransform lags a frame here).
fn draw_skeletons(
    lib: Res<AnimLib>,
    actors: Query<(&Actor, &Skeleton, &Transform), Without<HasModel>>,
    tfs: Query<&Transform, Without<Actor>>,
    mut gizmos: Gizmos,
) {
    for (a, skel, actor_tf) in &actors {
        let l = lib.lib(a.side);
        let root = actor_tf.mul_transform(Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0));
        let mut world: Vec<Transform> = Vec::with_capacity(l.bones.len());
        for (i, b) in l.bones.iter().enumerate() {
            let local = tfs.get(skel.bones[i]).copied().unwrap_or(b.pose);
            let parent = if b.parent >= 0 && (b.parent as usize) < i { world[b.parent as usize] } else { root };
            world.push(parent.mul_transform(local));
        }
        let color = match a.side {
            Side::Player => Color::srgb(0.95, 0.9, 0.8),
            Side::Enemy => Color::srgb(1.0, 0.45, 0.35),
        };
        for (i, b) in l.bones.iter().enumerate() {
            if b.parent >= 0 && !b.name.contains("Target") && !b.name.contains("Master") {
                let p = b.parent as usize;
                if l.bones[p].name.contains("Master") || l.bones[p].name.contains("Target") {
                    continue;
                }
                gizmos.line(world[p].translation, world[i].translation, color);
            }
        }
    }
}

/// Lower-body twist (c0000.hkx hkbTwistModifiers bound to TwistMasterAngle, set by the exe's
/// WalkTwist from Actor::twist): TwistMaster turns the Master bone (the whole body) about the model
/// Y axis, TwistRootRotYCancel turns Spine..Spine2 (bones 42-44) back by the same angle split over
/// the chain, so the legs run along the move direction while the chest keeps facing the target.
fn apply_walk_twist(lib: Res<AnimLib>, actors: Query<(&Actor, &Skeleton)>, mut bones: Query<&mut Transform, Without<Actor>>) {
    for (a, skel) in &actors {
        if a.twist.abs() < 1e-4 {
            continue;
        }
        let l = lib.lib(a.side);
        let find = |n: &str| l.bones.iter().position(|b| b.name == n);
        let Some(master) = find("Master") else { continue };
        // + = legs turned right (toward model +X; forward is -Z).
        let r = Quat::from_rotation_y(-a.twist);
        if let Some(Ok(mut t)) = skel.bones.get(master).map(|&e| bones.get_mut(e)) {
            t.rotation = (r * t.rotation).normalize();
            t.translation = r * t.translation;
        }
        let chain: Vec<usize> = ["Spine", "Spine1", "Spine2"].iter().filter_map(|n| find(n)).collect();
        let cancel = Quat::from_rotation_y(a.twist / chain.len().max(1) as f32);
        for &bi in &chain {
            // Parent's model-space rotation from the current local transforms.
            let mut pw = Quat::IDENTITY;
            let mut p = l.bones[bi].parent;
            while p >= 0 {
                let Some(Ok(t)) = skel.bones.get(p as usize).map(|&e| bones.get(e)) else { break };
                pw = t.rotation * pw;
                p = l.bones[p as usize].parent;
            }
            if let Some(Ok(mut t)) = skel.bones.get(bi).map(|&e| bones.get_mut(e)) {
                t.rotation = (pw.inverse() * cancel * pw * t.rotation).normalize();
            }
        }
    }
}

/// TAE 700 CustomLookAtTwistModifier (c0000.hkx, combat_data "twists"): while the event runs,
/// Wolf's upper body turns toward the target - the lock-on target, else the attack's auto-aim
/// target - within the modifier's limits. ModifierID N uses the modifiers named "N_*"
/// (0_Twist = 0_TwistUD + 0_TwistLR: +-25 deg up/down, +-45 deg left/right; 100-130_Attack:
/// up 30-35 / down 25-35, no left/right). Each chain (TwistParam) gets `rate` of the angle,
/// split evenly over the bones below `start` down to `end` (0_Twist: RootRotY..Spine2 30 %,
/// Neck..Head 70 %), and eases toward it by onGain per 1/30 s frame (offGain back to 0 when
/// the event ends). Applied as a world-space rotation about each bone (parent's GlobalTransform,
/// one frame old).
fn apply_twists(
    time: Res<Time>,
    lib: Res<AnimLib>,
    combat: Res<Combat>,
    lock: Option<Res<crate::camera::LockOn>>,
    actors: Query<(Entity, &Actor, &Transform, &Skeleton)>,
    targets: Query<(Entity, &Transform, Has<crate::player::Player>), With<Actor>>,
    globals: Query<&GlobalTransform>,
    mut bones: Query<&mut Transform, Without<Actor>>,
    mut state: Local<HashMap<(Entity, usize), (f32, f32)>>,
    mut last_chains: Local<HashMap<Entity, Vec<(crate::data::TwistChain, [f32; 4])>>>,
) {
    let dt = time.delta_secs();
    let step = |gain: f32| 1.0 - (1.0 - gain.clamp(0.0, 1.0)).powf(dt * 30.0);
    for (e, a, tf, skel) in &actors {
        let l = lib.lib(a.side);
        // The active TAE 700 event and its modifiers.
        let data = crate::actor::data_for(&combat, a.side);
        let ev = if a.anim.is_empty() { None } else { data.events_at(&a.anim, a.t).find(|ev| ev.kind == 700) };
        // ModifierID "0: 0_Twist" -> modifier number 0; Wolf's graph names it "0_Twist", the NPC
        // graph (c9997) "00_Twist" (several modifiers may share a number: 02_Twist_Neck/_Spine).
        let id = ev
            .and_then(|ev| ev.args.get("ModifierID"))
            .and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())))
            .and_then(|s| s.split(':').next().and_then(|n| n.trim().parse::<i32>().ok()));
        let table = if a.side == crate::actor::Side::Player { &combat.twists } else { &combat.enemy.twists };
        let mut mods: Vec<(&String, &crate::data::Twist)> = match id {
            Some(n) => table.iter().filter(|(k, _)| k.split('_').next().and_then(|p| p.parse::<i32>().ok()) == Some(n)).collect(),
            None => Vec::new(),
        };
        mods.sort_by(|x, y| x.0.cmp(y.0));
        let mods: Vec<&crate::data::Twist> = mods.into_iter().map(|(_, t)| t).collect();
        // Desired yaw (positive = right) and pitch (positive = up) toward the target: Wolf's lock
        // target (else his homing target), an NPC's opponent - Wolf.
        let target = if a.side == crate::actor::Side::Player {
            lock.as_ref().and_then(|l| l.target).or(a.homing).and_then(|t| targets.get(t).ok()).map(|(_, t, _)| t)
        } else {
            targets.iter().find(|(te, _, is_player)| *is_player && *te != e).map(|(_, t, _)| t)
        };
        // Each chain is clamped by its own modifier's limits (Wolf's 0_TwistLR turns, 0_TwistUD tilts).
        let (mut raw_yaw, mut raw_pitch) = (0.0f32, 0.0f32);
        if let (Some(t), false) = (target, mods.is_empty()) {
            let d = t.translation - tf.translation;
            let flat = d.with_y(0.0);
            let fwd = a.forward();
            let right = fwd.cross(Vec3::Y);
            raw_yaw = f32::atan2(flat.dot(right), flat.dot(fwd));
            raw_pitch = f32::atan2(d.y, flat.length().max(0.1));
        }
        // While a modifier runs its chains ease in by onGain; after it ends the last chains
        // ease back to zero by their offGain.
        let active = !mods.is_empty();
        let chains: Vec<(crate::data::TwistChain, [f32; 4])> = if active {
            let all: Vec<(crate::data::TwistChain, [f32; 4])> = mods
                .iter()
                .flat_map(|m| m.chains.iter().filter(|c| c.start >= 0 && c.end >= 0).map(|c| (c.clone(), [m.up, m.down, m.right, m.left])))
                .collect();
            last_chains.insert(e, all.clone());
            all
        } else {
            last_chains.get(&e).cloned().unwrap_or_default()
        };
        for ci in 0..chains.len() {
            let chain = chains.get(ci).map(|(c, _)| c);
            let [up, down, right, left] = chains.get(ci).map_or([0.0; 4], |(_, l)| l.map(f32::to_radians));
            let (want_yaw, want_pitch) = (raw_yaw.clamp(-left, right), raw_pitch.clamp(-down, up));
            let cur = state.entry((e, ci)).or_insert((0.0, 0.0));
            let (rate, gain) = match chain {
                Some(c) if active => (c.rate, c.on_gain),
                Some(c) => (0.0, c.off_gain),
                None => (0.0, 0.05),
            };
            cur.0 += (want_yaw * rate - cur.0) * step(gain);
            cur.1 += (want_pitch * rate - cur.1) * step(gain);
            let (yaw, pitch) = *cur;
            let Some(c) = chain else { continue };
            if yaw.abs() < 1e-4 && pitch.abs() < 1e-4 {
                continue;
            }
            // Bones from `end` up to (not including) `start`.
            let mut path = Vec::new();
            let mut b = c.end as i32;
            while b >= 0 && b != c.start as i32 && path.len() < 32 {
                path.push(b as usize);
                b = l.bones.get(b as usize).map_or(-1, |bd| bd.parent as i32);
            }
            if b != c.start as i32 || path.is_empty() {
                continue;
            }
            let n = path.len() as f32;
            let fwd = a.forward();
            let r = Quat::from_axis_angle(Vec3::Y, -yaw / n) * Quat::from_axis_angle(fwd.cross(Vec3::Y).normalize_or_zero(), pitch / n);
            for &bi in path.iter().rev() {
                let parent = l.bones[bi].parent;
                let pw = if parent >= 0 {
                    skel.bones.get(parent as usize).and_then(|&p| globals.get(p).ok()).map(|g| g.compute_transform().rotation)
                } else {
                    None
                };
                let Some(pw) = pw else { continue };
                if let Some(Ok(mut t)) = skel.bones.get(bi).map(|&be| bones.get_mut(be)) {
                    t.rotation = (pw.inverse() * r * pw * t.rotation).normalize();
                }
            }
        }
    }
}

#[cfg(test)]
mod loop_tests {
    /// The move loops (c0000.hkx StandWalk/RunLoop CMSGs, a000_0002xx / 0005xx; a010 sheathed) repeat
    /// frame 0 as their last frame, and each move start (0001xx / 0004xx) ends within a frame of its
    /// loop's first frame. Looping the starts instead (as before) popped 50-105 deg every cycle.
    #[test]
    fn move_loops_are_seamless() {
        let lib = super::load(&crate::paths::root().join("extracted/anim_c0000.bin"));
        if lib.clips.is_empty() {
            return;
        }
        let max_angle = |a: &super::Clip, fa: usize, b: &super::Clip, fb: usize| {
            let n = a.track_to_bone.len();
            (0..n).map(|t| a.data[fa * n + t].rotation.angle_between(b.data[fb * n + t].rotation).to_degrees()).fold(0.0f32, f32::max)
        };
        for k in ["a000_000000", "a000_000200", "a000_000201", "a000_000202", "a000_000203", "a000_000500", "a000_000501", "a000_000502", "a000_000503", "a010_000200", "a010_000500"] {
            let c = &lib.clips[k];
            let d = max_angle(c, c.frames - 1, c, 0);
            assert!(d < 1.0, "{k}: last frame {d:.1} deg from the first");
        }
        for (start, lp) in [("a000_000100", "a000_000200"), ("a000_000400", "a000_000500"), ("a000_000402", "a000_000502"), ("a000_000403", "a000_000503")] {
            let (a, b) = (&lib.clips[start], &lib.clips[lp]);
            let d = max_angle(a, a.frames - 1, b, 0);
            assert!(d < 12.0, "{start} ends {d:.1} deg from {lp}");
        }
    }
}

#[cfg(test)]
mod sheath_probe {
    /// Sheath bones: parents and whether idle / attack clips animate them.
    #[test]
    #[ignore]
    fn sheath_bones() {
        let lib = super::load(&crate::paths::root().join("extracted/anim_c0000.bin"));
        let name = |i: i16| lib.bones.get(i as usize).map(|b| b.name.clone()).unwrap_or("-".into());
        for (i, b) in lib.bones.iter().enumerate() {
            if b.name.contains("heath") || b.name.contains("Weapon") {
                let mut chain = vec![];
                let mut p = b.parent;
                while p >= 0 && chain.len() < 6 {
                    chain.push(name(p));
                    p = lib.bones[p as usize].parent;
                }
                println!("{i} {} <- {:?} pose t{:?} r{:?}", b.name, chain, b.pose.translation, b.pose.rotation);
            }
        }
        for k in ["a000_000000", "a050_002000", "a050_300000", "a000_000400"] {
            if let Some(c) = lib.clips.get(k) {
                let tracked: Vec<String> = c.track_to_bone.iter().filter(|&&b| b >= 0).map(|&b| name(b)).filter(|n| n.contains("heath") || n.contains("Weapon")).collect();
                println!("{k}: {tracked:?}");
            }
        }
    }
}
