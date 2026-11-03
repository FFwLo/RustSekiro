//! Drives an exported character GLB with decoded Havok clips.
//!
//! Clips and skeletons come from `cache/anim/<chr>/` and are converted to Bevy space once at load.
//! Each frame the active clip is sampled in Havok bone order, composed to model space, and each
//! glTF joint (matched by bone name) gets the transform relative to its glTF parent. Root motion
//! moves the character entity itself.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::prelude::*;
use sekiro_formats::anim::{AnimClip, Skeleton, Trs, to_bevy};

/// Loaded skeleton and clip cache for one character id (`c0000`, `c1010`).
pub struct AnimLibrary {
    dir: PathBuf,
    pub skeleton: Skeleton,
    clips: HashMap<String, Arc<AnimClip>>,
    /// Animations that play another's HKX (`aliases.json` from the extractor).
    aliases: HashMap<String, String>,
    /// Clips read from disk by [`AnimLibrary::clip`] since the last
    /// [`AnimLibrary::take_load_stats`], and the milliseconds spent (for the perf log).
    loads: (usize, f64),
    /// Clips decoded ahead by [`AnimLibrary::prefetch_all`]'s thread, by file name.
    prefetched: Arc<Mutex<HashMap<String, Arc<AnimClip>>>>,
    prefetching: bool,
}

impl AnimLibrary {
    pub fn load(cache: &Path, chr: &str) -> Result<Self> {
        let dir = cache.join("anim").join(chr);
        let bytes = std::fs::read(dir.join("skeleton.bin"))
            .with_context(|| format!("{}: run `sekiro-extract anims {chr}`", dir.display()))?;
        let skeleton = to_bevy::skeleton(&Skeleton::from_bytes(&bytes)?);
        let aliases = std::fs::read_to_string(dir.join("aliases.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Ok(Self {
            dir,
            skeleton,
            clips: HashMap::new(),
            aliases,
            loads: (0, 0.0),
            prefetched: Arc::default(),
            prefetching: false,
        })
    }

    pub fn clip(&mut self, name: &str) -> Result<Arc<AnimClip>> {
        if let Some(c) = self.clips.get(name) {
            return Ok(c.clone());
        }
        let file = self.aliases.get(name).map_or(name, String::as_str);
        let ready = self
            .prefetched
            .lock()
            .ok()
            .and_then(|m| m.get(file).cloned());
        if let Some(clip) = ready {
            self.clips.insert(name.to_string(), clip.clone());
            return Ok(clip);
        }
        let started = std::time::Instant::now();
        let path = self.dir.join(format!("{file}.bin"));
        let bytes = std::fs::read(&path).with_context(|| format!("{}", path.display()))?;
        let clip = Arc::new(to_bevy::clip(&AnimClip::from_bytes(&bytes)?));
        self.clips.insert(name.to_string(), clip.clone());
        self.loads.0 += 1;
        self.loads.1 += started.elapsed().as_secs_f64() * 1000.0;
        Ok(clip)
    }

    /// Decodes every clip of the character on a background thread, so a clip shown for the
    /// first time mid-fight does not wait for the disk and the decode on the frame it starts
    /// (25 to 55 ms each, measured). Clips already asked for are skipped; runs once.
    pub fn prefetch_all(&mut self) {
        if self.prefetching {
            return;
        }
        self.prefetching = true;
        let dir = self.dir.clone();
        let done: Vec<String> = self.clips.keys().cloned().collect();
        let shared = self.prefetched.clone();
        let spawned = std::thread::Builder::new()
            .name("clip-prefetch".into())
            .spawn(move || {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    return;
                };
                let mut files: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.extension().is_some_and(|x| x == "bin")
                            && p.file_stem().is_some_and(|s| s != "skeleton")
                    })
                    .collect();
                files.sort();
                let started = std::time::Instant::now();
                let mut count = 0;
                for path in files {
                    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    if done.iter().any(|d| d == stem) {
                        continue;
                    }
                    let Ok(bytes) = std::fs::read(&path) else {
                        continue;
                    };
                    let Ok(clip) = AnimClip::from_bytes(&bytes) else {
                        continue;
                    };
                    let clip = Arc::new(to_bevy::clip(&clip));
                    if let Ok(mut m) = shared.lock() {
                        m.insert(stem.to_string(), clip);
                    }
                    count += 1;
                }
                info!(
                    "prefetched {count} clips of {} in {:.1} s",
                    dir.display(),
                    started.elapsed().as_secs_f32()
                );
            });
        if let Err(e) = spawned {
            warn!("clip prefetch thread: {e}");
        }
    }

    /// Clips read from disk and milliseconds spent since the last call.
    pub fn take_load_stats(&mut self) -> (usize, f64) {
        std::mem::take(&mut self.loads)
    }
}

/// What a character is playing. Gameplay code sets `clip`; [`animate`] advances and applies it.
#[derive(Component)]
pub struct Animator {
    pub clip: Arc<AnimClip>,
    pub time: f32,
    pub looping: bool,
    pub speed: f32,
    pub apply_root_motion: bool,
    /// Root motion sample at the previous frame, for deltas.
    last_root: [f32; 4],
    /// The animation name that was asked for (an alias may play another clip's data).
    name: String,
    /// The last displayed local pose, including any unfinished crossfade.
    displayed_pose: Option<Vec<Trs>>,
    blend: Option<PoseBlend>,
    /// The locked-on walk twist in degrees, positive with the legs turned right
    /// (`sekiro_sim::Character::walk_twist`), applied to the pose by [`apply_walk_twist`].
    pub walk_twist: f32,
    /// The clock of the character this animator draws, when the simulation drives it: the
    /// crossfade advances with it rather than with real time, so a character held in hit stop
    /// holds its whole pose. `None` uses the frame time.
    pub clock: Option<f32>,
    /// `clock` at the previous frame.
    last_clock: Option<f32>,
}

struct PoseBlend {
    source: BlendSource,
    duration: f32,
    elapsed: f32,
}

enum BlendSource {
    Pose(Vec<Trs>),
    /// Before the first draw there is no displayed pose to freeze.
    Clip(Arc<AnimClip>, f32),
}

impl Animator {
    pub fn new(clip: Arc<AnimClip>, looping: bool) -> Self {
        Self {
            time: 0.0,
            looping,
            speed: 1.0,
            apply_root_motion: true,
            last_root: [0.0; 4],
            name: clip_name_of(&clip),
            blend: None,
            displayed_pose: None,
            walk_twist: 0.0,
            clock: None,
            last_clock: None,
            clip,
        }
    }

    pub fn play(&mut self, clip: Arc<AnimClip>, looping: bool) {
        self.name = clip_name_of(&clip);
        self.blend = None;
        self.displayed_pose = None;
        self.clip = clip;
        self.time = 0.0;
        self.looping = looping;
        self.last_root = [0.0; 4];
    }

    pub fn finished(&self) -> bool {
        !self.looping && self.time >= self.clip.duration
    }

    /// Plays `clip` under the requested animation `name`, crossfading from the current pose over
    /// `blend_secs` (0 cuts).
    pub fn play_named(&mut self, name: &str, clip: Arc<AnimClip>, looping: bool, blend_secs: f32) {
        let from = self.displayed_pose.take().map_or_else(
            || {
                let time = if self.looping && self.clip.duration > 0.0 {
                    self.time.rem_euclid(self.clip.duration)
                } else {
                    self.time.min(self.clip.duration)
                };
                BlendSource::Clip(self.clip.clone(), time)
            },
            BlendSource::Pose,
        );
        self.play(clip, looping);
        self.name = name.to_string();
        if blend_secs.is_finite() && blend_secs > 0.0 {
            self.blend = Some(PoseBlend {
                source: from,
                duration: blend_secs,
                elapsed: 0.0,
            });
        }
    }

    /// Blends local bones only; clip time, TAE and root motion remain independent.
    fn sample_pose(&mut self, skeleton: &Skeleton, time: f32, dt: f32) -> Vec<Trs> {
        let mut pose = self.clip.sample(time, skeleton);
        if let Some(blend) = &mut self.blend {
            blend.elapsed += dt.max(0.0);
            let weight = smooth_blend_weight(blend.elapsed / blend.duration);
            let old = match &blend.source {
                BlendSource::Pose(pose) => Cow::Borrowed(pose.as_slice()),
                BlendSource::Clip(clip, time) => Cow::Owned(clip.sample(*time, skeleton)),
            };
            for (bone, from) in pose.iter_mut().zip(old.iter()) {
                *bone = from.lerp(bone, weight);
            }
        }
        if self.blend.as_ref().is_some_and(|b| b.elapsed >= b.duration) {
            self.blend = None;
        }
        self.displayed_pose = Some(pose.clone());
        pose
    }

    /// The requested animation name (which may be an alias of the clip data playing).
    pub fn clip_name(&self) -> &str {
        &self.name
    }
}

use sekiro_sim::motion::smoothstep as smooth_blend_weight;

/// Havok skeleton plus the glTF joint entity for each bone (filled once the scene spawns).
#[derive(Component)]
pub struct Rig {
    pub skeleton: Arc<Skeleton>,
    /// Per Havok bone: the joint entity and the Havok index of its glTF parent, if animated.
    joints: Vec<Option<(Entity, Option<usize>)>>,
    pub bound: bool,
    /// Model-space bone matrices from the last applied pose (Bevy space, character local).
    pub model: Vec<Mat4>,
}

impl Rig {
    pub fn new(skeleton: Arc<Skeleton>) -> Self {
        Self {
            skeleton,
            joints: Vec::new(),
            bound: false,
            model: Vec::new(),
        }
    }
}

/// Binds Havok bones to glTF joint entities by name once the character's scene exists.
pub fn bind_rigs(
    mut rigs: Query<(Entity, &mut Rig)>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
) {
    for (root, mut rig) in &mut rigs {
        if rig.bound {
            continue;
        }
        let mut by_name: HashMap<String, Entity> = HashMap::new();
        for e in children.iter_descendants(root) {
            if let Ok(n) = names.get(e) {
                by_name.entry(n.as_str().to_string()).or_insert(e);
            }
        }
        if !rig
            .skeleton
            .bones
            .first()
            .is_some_and(|b| by_name.contains_key(&b.name))
        {
            continue; // scene not spawned yet
        }
        let index: HashMap<Entity, usize> = rig
            .skeleton
            .bones
            .iter()
            .enumerate()
            .filter_map(|(i, b)| by_name.get(&b.name).map(|&e| (e, i)))
            .collect();
        let joints = rig
            .skeleton
            .bones
            .iter()
            .map(|b| {
                by_name.get(&b.name).map(|&e| {
                    let parent = parents
                        .get(e)
                        .ok()
                        .and_then(|p| index.get(&p.parent()).copied());
                    (e, parent)
                })
            })
            .collect();
        let bound = rig
            .skeleton
            .bones
            .iter()
            .filter(|b| by_name.contains_key(&b.name))
            .count();
        info!("rig bound {bound}/{} bones", rig.skeleton.bones.len());
        rig.joints = joints;
        rig.bound = true;
    }
}

/// Placed actors outside their simulation activation range retain their last skinned pose.
#[derive(Component)]
pub struct PoseDormant;

/// Population presentation budget, chosen before animation; absent on the existing boss path.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct PresentationLod {
    pub draw: bool,
    pub pose: bool,
    pub cloth: bool,
}

#[allow(clippy::type_complexity)] // Typed pose budgets share the existing joint writer.
pub fn animate(
    time: Res<Time>,
    mut q: Query<(
        &mut Animator,
        &mut Rig,
        &mut Transform,
        Option<&PresentationLod>,
        Has<PoseDormant>,
    )>,
    mut joints: Query<&mut Transform, Without<Rig>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("animation pose upload");
    let dt = time.delta_secs();
    for (mut anim, mut rig, mut body, lod, dormant) in &mut q {
        if !rig.bound {
            continue;
        }
        // Initialize a standing pose once; otherwise don't touch joints or upload a hidden pose.
        if !rig.model.is_empty() && (dormant || lod.is_some_and(|l| !l.pose)) {
            continue;
        }
        let prev = anim.time;
        anim.time += dt * anim.speed;
        let clock_dt = match anim.clock {
            Some(clock) => {
                let step = anim.last_clock.map_or(0.0, |last| (clock - last).max(0.0));
                anim.last_clock = Some(clock);
                step
            }
            None => dt,
        };
        let clip = anim.clip.clone();
        let t = if anim.looping && clip.duration > 0.0 {
            anim.time.rem_euclid(clip.duration)
        } else {
            anim.time.min(clip.duration)
        };

        if anim.apply_root_motion
            && let Some(rm) = &clip.root_motion
        {
            let wrapped = anim.looping && clip.duration > 0.0 && t < prev.rem_euclid(clip.duration);
            let now = rm.sample(t);
            let delta = if wrapped {
                let end = rm.total();
                [
                    end[0] - anim.last_root[0] + now[0],
                    end[1] - anim.last_root[1] + now[1],
                    end[2] - anim.last_root[2] + now[2],
                    end[3] - anim.last_root[3] + now[3],
                ]
            } else {
                [
                    now[0] - anim.last_root[0],
                    now[1] - anim.last_root[1],
                    now[2] - anim.last_root[2],
                    now[3] - anim.last_root[3],
                ]
            };
            anim.last_root = now;
            let local = Vec3::new(delta[0], delta[1], delta[2]);
            let moved = body.rotation * local;
            body.translation += moved;
            body.rotate_y(delta[3]);
        }

        let pose = anim.sample_pose(&rig.skeleton, t, clock_dt);
        let mut model = rig.skeleton.model_space(&pose);
        apply_walk_twist(&rig.skeleton, &mut model, anim.walk_twist);
        for (i, joint) in rig.joints.iter().enumerate() {
            let Some((entity, parent)) = joint else {
                continue;
            };
            let local = match parent {
                Some(p) => model[*p].inverse() * model[i],
                None => model[i],
            };
            if let Ok(mut tf) = joints.get_mut(*entity) {
                *tf = Transform::from_matrix(local);
            }
        }
        rig.model = model;
    }
}

/// The locked-on walk twist on a model-space pose, as Wolf's behaviour graph does it (its
/// `TwistMaster` and `TwistRootRotYCancel` twist modifiers, bound to `TwistMasterAngle`; facts
/// from FwLo's RustSekiro, `docs/kb/movement.md`): the Master bone, and so the whole body, turns
/// by `degrees` about the vertical (positive turns the legs right), then Spine, Spine1 and
/// Spine2 each turn back by a third about their own joints, so the legs step along the move
/// while the chest, arms and head keep facing the lock-on target.
fn apply_walk_twist(skeleton: &Skeleton, model: &mut [Mat4], degrees: f32) {
    if degrees.abs() < 0.01 {
        return;
    }
    let find = |name: &str| skeleton.bones.iter().position(|b| b.name == name);
    let Some(master) = find("Master") else {
        return;
    };
    // Bevy model space: forward is -Z and a positive turn about +Y goes left.
    let angle = degrees.to_radians();
    turn_subtree(skeleton, model, master, Mat4::from_rotation_y(-angle));
    let spine = ["Spine", "Spine1", "Spine2"].map(find);
    let share = angle / spine.len() as f32;
    for bone in spine.into_iter().flatten() {
        let pivot = model[bone].w_axis.truncate();
        let back = Mat4::from_translation(pivot)
            * Mat4::from_rotation_y(share)
            * Mat4::from_translation(-pivot);
        turn_subtree(skeleton, model, bone, back);
    }
}

/// Applies the model-space transform `by` to bone `root` and everything under it (parents
/// always precede their children in the skeleton).
fn turn_subtree(skeleton: &Skeleton, model: &mut [Mat4], root: usize, by: Mat4) {
    let mut inside = vec![false; skeleton.bones.len()];
    for (i, bone) in skeleton.bones.iter().enumerate().skip(root) {
        inside[i] = i == root || bone.parent.is_some_and(|p| inside[p as usize]);
        if inside[i]
            && let Some(m) = model.get_mut(i)
        {
            *m = by * *m;
        }
    }
}

fn clip_name_of(clip: &AnimClip) -> String {
    clip.name.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sekiro_formats::anim::{BlendHint, Bone, BoneTrack};

    fn fixture(x: f32, angle: f32) -> (Skeleton, Arc<AnimClip>) {
        let skeleton = Skeleton {
            name: "test".into(),
            bones: vec![Bone {
                name: "arm".into(),
                parent: None,
                local: Trs::IDENTITY,
            }],
        };
        let clip = Arc::new(AnimClip {
            name: format!("clip{x}"),
            duration: 1.0,
            frame_count: 2,
            frame_duration: 1.0,
            blend: BlendHint::Normal,
            skeleton_bones: 1,
            tracks: vec![BoneTrack {
                bone: 0,
                translation: vec![[x, 0.0, 0.0]],
                rotation: vec![Quat::from_rotation_y(angle).to_array()],
                scale: vec![[1.0 + x; 3]],
            }],
            root_motion: None,
        });
        (skeleton, clip)
    }

    #[test]
    fn pose_blend_interpolates_translation_scale_and_shortest_rotation() {
        assert_eq!(smooth_blend_weight(-1.0), 0.0);
        assert_eq!(smooth_blend_weight(0.25), 0.15625);
        assert_eq!(smooth_blend_weight(0.5), 0.5);
        assert_eq!(smooth_blend_weight(0.75), 0.84375);
        assert_eq!(smooth_blend_weight(2.0), 1.0);
        let from = Trs::IDENTITY;
        let to = Trs {
            translation: [2.0, 4.0, 6.0],
            rotation: (-Quat::from_rotation_y(2.0)).to_array(),
            scale: [3.0; 3],
        };
        let mid = from.lerp(&to, 0.5);
        assert_eq!(mid.translation, [1.0, 2.0, 3.0]);
        assert_eq!(mid.scale, [2.0; 3]);
        assert!(mid.quat().dot(Quat::from_rotation_y(1.0)).abs() > 0.99999);
    }

    #[test]
    fn clip_change_has_intermediate_bones_and_preserves_interrupted_pose() {
        let (skeleton, first) = fixture(0.0, 0.0);
        let (_, second) = fixture(2.0, 2.0);
        let (_, third) = fixture(4.0, 0.0);
        let mut anim = Animator::new(first, false);
        anim.sample_pose(&skeleton, 0.9, 0.0);
        anim.play_named("second", second, false, 0.2);
        let halfway = anim.sample_pose(&skeleton, 0.1, 0.1)[0];
        assert_eq!(halfway.translation[0], 1.0);
        assert!(halfway.quat().dot(Quat::from_rotation_y(1.0)).abs() > 0.99999);
        anim.play_named("third", third.clone(), false, 0.2);
        // Interrupting a blend starts at the displayed pose, not second's raw x=2 pose.
        assert_eq!(anim.sample_pose(&skeleton, 0.0, 0.0)[0], halfway);
        assert_eq!(anim.sample_pose(&skeleton, 0.1, 0.1)[0].translation[0], 2.5);
        // Hit stop freezes the entire blend.
        assert_eq!(anim.sample_pose(&skeleton, 0.1, 0.0)[0].translation[0], 2.5);
        assert_eq!(
            anim.sample_pose(&skeleton, 0.2, 0.1),
            third.sample(0.2, &skeleton)
        );
        assert!(anim.blend.is_none());
        anim.play_named("cut", fixture(0.0, 0.0).1, false, 0.0);
        assert_eq!(anim.sample_pose(&skeleton, 0.0, 0.0)[0].translation[0], 0.0);
    }

    /// With the legs twisted 45 degrees right, the hips turn right with them while the
    /// shoulders still face where they did.
    #[test]
    fn walk_twist_turns_the_hips_and_keeps_the_shoulders() {
        let cache = sekiro_sim::cache::root();
        let Ok(lib) = AnimLibrary::load(&cache, "c0000") else {
            eprintln!("skipping: c0000 animations are not extracted");
            return;
        };
        let skeleton = &lib.skeleton;
        let at = |model: &[Mat4], name: &str| {
            let i = skeleton.bones.iter().position(|b| b.name == name).unwrap();
            model[i].w_axis.truncate()
        };
        let across = |model: &[Mat4], left: &str, right: &str| {
            let v = at(model, right) - at(model, left);
            Vec3::new(v.x, 0.0, v.z).normalize()
        };
        let still = skeleton.model_space(&[]);
        let mut twisted = still.clone();
        apply_walk_twist(skeleton, &mut twisted, 45.0);
        // Facing -Z, the right side is +X; turning right by 45 degrees is -45 about +Y.
        let side = across(&still, "L_Thigh", "R_Thigh");
        assert!(
            side.x > 0.9,
            "the model does not face -Z: right thigh is {side:?} of the left"
        );
        let right = Quat::from_rotation_y(-45f32.to_radians());
        let hips = across(&twisted, "L_Thigh", "R_Thigh");
        let want = right * across(&still, "L_Thigh", "R_Thigh");
        assert!(hips.dot(want) > 0.99, "hips {hips:?}, want {want:?}");
        let shoulders = across(&twisted, "L_UpperArm", "R_UpperArm");
        let before = across(&still, "L_UpperArm", "R_UpperArm");
        assert!(
            shoulders.dot(before) > 0.99,
            "shoulders {shoulders:?}, were {before:?}"
        );
    }
}
