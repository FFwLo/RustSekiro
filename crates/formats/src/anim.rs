//! Engine-neutral skeleton and animation clip types, decoded from Havok HKX by [`crate::hkx`].
//!
//! Everything here is in FromSoftware source space (left-handed, Y up, character facing -Z) unless
//! a function says otherwise. [`to_bevy`] converts once, at load time, for the renderer. Quaternions
//! are `[x, y, z, w]`. Clips are stored sampled at their authored frames; [`AnimClip::sample`]
//! interpolates between frames (lerp for translation and scale, slerp for rotation).
//!
//! Serialized with `postcard` (see [`AnimClip::to_bytes`]).

use glam::{Mat4, Quat, Vec3, Vec4};
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::reader::bail;

/// A local transform: translation, rotation, scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trs {
    pub translation: [f32; 3],
    /// `[x, y, z, w]`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for Trs {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Trs {
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    };

    pub fn quat(&self) -> Quat {
        Quat::from_array(self.rotation)
    }

    /// Column-vector matrix `T * R * S`.
    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::from_array(self.scale),
            self.quat(),
            Vec3::from_array(self.translation),
        )
    }

    /// Interpolates: lerp for translation and scale, shortest-path slerp for rotation.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let a = self.quat();
        let mut b = other.quat();
        if a.dot(b) < 0.0 {
            b = -b;
        }
        Self {
            translation: Vec3::from_array(self.translation)
                .lerp(Vec3::from_array(other.translation), t)
                .to_array(),
            rotation: a.slerp(b, t).normalize().to_array(),
            scale: Vec3::from_array(self.scale)
                .lerp(Vec3::from_array(other.scale), t)
                .to_array(),
        }
    }

    /// Applies an additive (delta) transform on top of this one, scaled by `weight`, using Havok's
    /// additive convention: translations add, rotations compose as `base * delta`, scales multiply.
    pub fn add(&self, delta: &Self, weight: f32) -> Self {
        let d = Trs::IDENTITY.lerp(delta, weight);
        Self {
            translation: (Vec3::from_array(self.translation) + Vec3::from_array(d.translation))
                .to_array(),
            rotation: (self.quat() * d.quat()).normalize().to_array(),
            scale: (Vec3::from_array(self.scale) * Vec3::from_array(d.scale)).to_array(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    /// Parent bone index, or `None` for a root. Parents always precede children in Sekiro data.
    pub parent: Option<u16>,
    /// Reference (bind) pose, relative to the parent.
    pub local: Trs,
}

/// A Havok animation skeleton (`hkaSkeleton`). This is the skeleton clips are authored against;
/// its bone order is not the FLVER node order, so match FLVER nodes by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skeleton {
    pub name: String,
    pub bones: Vec<Bone>,
}

impl Skeleton {
    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    pub fn reference_pose(&self) -> Vec<Trs> {
        self.bones.iter().map(|b| b.local).collect()
    }

    /// Composes a local pose into model-space matrices (`parent * local`).
    pub fn model_space(&self, pose: &[Trs]) -> Vec<Mat4> {
        let mut out: Vec<Mat4> = Vec::with_capacity(self.bones.len());
        for (i, bone) in self.bones.iter().enumerate() {
            let local = pose.get(i).unwrap_or(&bone.local).to_mat4();
            let m = match bone.parent {
                Some(p) if (p as usize) < i => out[p as usize] * local,
                // Out-of-order parents never occur in Sekiro skeletons; treat as a root.
                _ => local,
            };
            out.push(m);
        }
        out
    }

    /// Checks parent indices point backwards and transforms are finite.
    pub fn validate(&self) -> Result<()> {
        for (i, b) in self.bones.iter().enumerate() {
            if let Some(p) = b.parent
                && p as usize >= i
            {
                return bail(format!("bone {i} {} has parent {p} after it", b.name));
            }
            let l = &b.local;
            if !l
                .translation
                .iter()
                .chain(&l.rotation)
                .chain(&l.scale)
                .all(|v| v.is_finite())
            {
                return bail(format!("bone {} has a non-finite transform", b.name));
            }
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_allocvec(self).map_err(|e| crate::Error::Format(e.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        postcard::from_bytes(bytes).map_err(|e| crate::Error::Format(e.to_string()))
    }
}

/// `hkaAnimationBinding::BlendHint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendHint {
    /// Tracks are full local transforms.
    Normal,
    /// Tracks are deltas to add on top of another pose (`ADDITIVE` / `ADDITIVE_DEPRECATED`).
    Additive,
}

/// One bone's animated channels. A channel with one entry is constant for the whole clip;
/// otherwise it has one entry per authored frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoneTrack {
    pub bone: u16,
    pub translation: Vec<[f32; 3]>,
    pub rotation: Vec<[f32; 4]>,
    pub scale: Vec<[f32; 3]>,
}

impl BoneTrack {
    pub fn at_frame(&self, frame: usize) -> Trs {
        fn pick<T: Copy>(v: &[T], f: usize) -> T {
            v[f.min(v.len() - 1)]
        }
        Trs {
            translation: pick(&self.translation, frame),
            rotation: pick(&self.rotation, frame),
            scale: pick(&self.scale, frame),
        }
    }

    pub fn is_constant(&self) -> bool {
        self.translation.len() == 1 && self.rotation.len() == 1 && self.scale.len() == 1
    }
}

/// Extracted root motion (`hkaDefaultAnimatedReferenceFrame`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RootMotion {
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub duration: f32,
    /// Evenly spaced over `duration`: xyz displacement from the clip start, w yaw in radians
    /// about `up`. Source space.
    pub samples: Vec<[f32; 4]>,
}

impl RootMotion {
    /// Interpolated root displacement and yaw at `time` (clamped).
    pub fn sample(&self, time: f32) -> [f32; 4] {
        let n = self.samples.len();
        if n == 0 {
            return [0.0; 4];
        }
        if n == 1 || self.duration <= 0.0 {
            return self.samples[0];
        }
        let f = (time / self.duration).clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (f.floor() as usize).min(n - 2);
        let a = f - i as f32;
        Vec4::from_array(self.samples[i])
            .lerp(Vec4::from_array(self.samples[i + 1]), a)
            .to_array()
    }

    /// Total displacement and yaw over the clip.
    pub fn total(&self) -> [f32; 4] {
        self.samples.last().copied().unwrap_or([0.0; 4])
    }
}

/// A decoded animation clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimClip {
    /// File stem, e.g. `a000_003000`.
    pub name: String,
    /// Seconds. Equals `(frame_count - 1) * frame_duration`.
    pub duration: f32,
    pub frame_count: u32,
    /// Seconds per authored frame (1/30 in Sekiro).
    pub frame_duration: f32,
    pub blend: BlendHint,
    /// Number of bones in the skeleton the tracks were bound to.
    pub skeleton_bones: u16,
    /// Only bones the clip animates; others keep the reference pose (or identity if additive).
    pub tracks: Vec<BoneTrack>,
    pub root_motion: Option<RootMotion>,
}

impl AnimClip {
    pub fn fps(&self) -> f32 {
        if self.frame_duration > 0.0 {
            1.0 / self.frame_duration
        } else {
            0.0
        }
    }

    /// Fractional frame position for `time`, clamped to the clip.
    fn frame_at(&self, time: f32) -> (usize, usize, f32) {
        let last = self.frame_count.saturating_sub(1) as usize;
        if last == 0 || self.frame_duration <= 0.0 {
            return (0, 0, 0.0);
        }
        let f = (time / self.frame_duration).clamp(0.0, last as f32);
        let i = (f.floor() as usize).min(last);
        let j = (i + 1).min(last);
        (i, j, f - i as f32)
    }

    /// Writes the pose at `time` (seconds, clamped) into `pose`. Bones without a track are left as
    /// they are, so pass the reference pose for normal clips (or a base pose to override).
    pub fn sample_into(&self, time: f32, pose: &mut [Trs]) {
        let (i, j, a) = self.frame_at(time);
        for track in &self.tracks {
            if let Some(slot) = pose.get_mut(track.bone as usize) {
                *slot = if track.is_constant() {
                    track.at_frame(0)
                } else {
                    track.at_frame(i).lerp(&track.at_frame(j), a)
                };
            }
        }
    }

    /// The local pose at `time` (seconds, clamped). Untracked bones take the skeleton's reference
    /// pose, or identity for additive clips.
    pub fn sample(&self, time: f32, skeleton: &Skeleton) -> Vec<Trs> {
        let mut pose = match self.blend {
            BlendHint::Normal => skeleton.reference_pose(),
            BlendHint::Additive => vec![Trs::IDENTITY; skeleton.bones.len()],
        };
        self.sample_into(time, &mut pose);
        pose
    }

    /// Like [`sample`](Self::sample) with `time` wrapped into the clip for looping playback.
    pub fn sample_looped(&self, time: f32, skeleton: &Skeleton) -> Vec<Trs> {
        let t = if self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            0.0
        };
        self.sample(t, skeleton)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_allocvec(self).map_err(|e| crate::Error::Format(e.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        postcard::from_bytes(bytes).map_err(|e| crate::Error::Format(e.to_string()))
    }
}

/// Converts source-space data to Bevy space by mirroring the X axis.
///
/// FromSoftware data is left-handed with Y up and the character facing -Z; Bevy is right-handed
/// with Y up. Negating X (the convention in `docs/FORMATS.md`) turns one into the other. For a
/// transform that means `M' = S * M * S` with `S = diag(-1, 1, 1)`: translations negate x,
/// quaternions negate y and z, scales are unchanged. Applying this to every local transform and to
/// root motion gives the same result as mirroring the composed model-space matrices.
pub mod to_bevy {
    use super::*;

    pub fn translation(t: [f32; 3]) -> [f32; 3] {
        [-t[0], t[1], t[2]]
    }

    pub fn rotation(q: [f32; 4]) -> [f32; 4] {
        [q[0], -q[1], -q[2], q[3]]
    }

    pub fn trs(t: &Trs) -> Trs {
        Trs {
            translation: translation(t.translation),
            rotation: rotation(t.rotation),
            scale: t.scale,
        }
    }

    pub fn skeleton(s: &Skeleton) -> Skeleton {
        Skeleton {
            name: s.name.clone(),
            bones: s
                .bones
                .iter()
                .map(|b| Bone {
                    local: trs(&b.local),
                    ..b.clone()
                })
                .collect(),
        }
    }

    /// Mirrors every track and the root motion. Root motion yaw (about +Y) flips sign.
    pub fn clip(c: &AnimClip) -> AnimClip {
        AnimClip {
            tracks: c
                .tracks
                .iter()
                .map(|t| BoneTrack {
                    bone: t.bone,
                    translation: t.translation.iter().map(|&v| translation(v)).collect(),
                    rotation: t.rotation.iter().map(|&v| rotation(v)).collect(),
                    scale: t.scale.clone(),
                })
                .collect(),
            root_motion: c.root_motion.as_ref().map(|r| RootMotion {
                up: translation(r.up),
                forward: translation(r.forward),
                duration: r.duration,
                samples: r
                    .samples
                    .iter()
                    .map(|s| [-s[0], s[1], s[2], -s[3]])
                    .collect(),
            }),
            ..c.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skel() -> Skeleton {
        let rot = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.3);
        Skeleton {
            name: "t".into(),
            bones: vec![
                Bone {
                    name: "root".into(),
                    parent: None,
                    local: Trs {
                        translation: [1.0, 2.0, 3.0],
                        rotation: rot.to_array(),
                        scale: [1.0; 3],
                    },
                },
                Bone {
                    name: "child".into(),
                    parent: Some(0),
                    local: Trs {
                        translation: [0.5, -0.25, 2.0],
                        rotation: Quat::from_rotation_z(-1.1).to_array(),
                        scale: [1.0; 3],
                    },
                },
            ],
        }
    }

    #[test]
    fn model_space_composes_parents() {
        let s = skel();
        let m = s.model_space(&s.reference_pose());
        let want = s.bones[0].local.to_mat4() * s.bones[1].local.to_mat4();
        assert!(m[1].abs_diff_eq(want, 1e-6));
        let tip = m[1].transform_point3(Vec3::ZERO);
        let manual =
            Vec3::new(1.0, 2.0, 3.0) + s.bones[0].local.quat() * Vec3::new(0.5, -0.25, 2.0);
        assert!(tip.abs_diff_eq(manual, 1e-5));
    }

    #[test]
    fn bevy_mirror_matches_model_space_mirror() {
        let s = skel();
        let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        let src = s.model_space(&s.reference_pose());
        let b = to_bevy::skeleton(&s);
        let dst = b.model_space(&b.reference_pose());
        for (a, d) in src.iter().zip(&dst) {
            assert!((mirror * *a * mirror).abs_diff_eq(*d, 1e-5));
            // A mirrored rigid transform stays a proper rotation (no reflection in the result).
            assert!(d.determinant() > 0.0);
        }
    }

    #[test]
    fn clip_sampling_interpolates_and_clamps() {
        let s = skel();
        let clip = AnimClip {
            name: "c".into(),
            duration: 1.0,
            frame_count: 3,
            frame_duration: 0.5,
            blend: BlendHint::Normal,
            skeleton_bones: 2,
            tracks: vec![BoneTrack {
                bone: 1,
                translation: vec![[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
                rotation: vec![
                    [0.0, 0.0, 0.0, 1.0],
                    Quat::from_rotation_y(1.0).to_array(),
                    Quat::from_rotation_y(2.0).to_array(),
                ],
                scale: vec![[1.0; 3]],
            }],
            root_motion: None,
        };
        let p = clip.sample(0.25, &s);
        assert_eq!(p[0], s.bones[0].local);
        assert!((p[1].translation[0] - 0.5).abs() < 1e-6);
        let q = Quat::from_array(p[1].rotation);
        assert!(q.abs_diff_eq(Quat::from_rotation_y(0.5), 1e-5));
        assert!((clip.sample(5.0, &s)[1].translation[0] - 2.0).abs() < 1e-6);
        assert!((clip.sample_looped(1.25, &s)[1].translation[0] - 0.5).abs() < 1e-5);
        let bytes = clip.to_bytes().unwrap();
        assert_eq!(AnimClip::from_bytes(&bytes).unwrap(), clip);
    }

    #[test]
    fn root_motion_sampling() {
        let r = RootMotion {
            up: [0.0, 1.0, 0.0],
            forward: [0.0, 0.0, 1.0],
            duration: 2.0,
            samples: vec![[0.0; 4], [0.0, 0.0, -1.0, 0.0], [0.0, 0.0, -4.0, 0.5]],
        };
        assert_eq!(r.sample(0.5)[2], -0.5);
        assert_eq!(r.sample(9.0), [0.0, 0.0, -4.0, 0.5]);
        assert_eq!(r.total()[2], -4.0);
    }

    #[test]
    fn additive_identity_is_noop() {
        let base = skel().bones[0].local;
        let out = base.add(&Trs::IDENTITY, 1.0);
        assert!(out.quat().abs_diff_eq(base.quat(), 1e-6));
        assert_eq!(out.translation, base.translation);
    }
}
