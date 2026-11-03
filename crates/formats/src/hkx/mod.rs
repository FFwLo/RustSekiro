//! Havok HKX (2016 tagfile) reading: skeletons, animations, cloth and navigation meshes.
//!
//! [`tagfile`] is a generic reflected-object reader; this module pulls the animation types out of
//! it and decodes them into [`crate::anim`] types.

pub mod cloth;
pub mod navmesh;
pub mod spline;
pub mod tagfile;

pub use spline::DecodeStats;
pub use tagfile::{Compendium, TagFile, Value};

use crate::anim::{AnimClip, BlendHint, Bone, BoneTrack, RootMotion, Skeleton, Trs};
use crate::reader::bail;
use crate::{Error, Result};

fn qs_transform(v: Value<'_>) -> Result<Trs> {
    let f = v.floats()?;
    if f.len() < 12 {
        return bail(format!(
            "{} has {} floats, expected 12",
            v.type_name(),
            f.len()
        ));
    }
    Ok(Trs {
        translation: [f[0], f[1], f[2]],
        rotation: [f[4], f[5], f[6], f[7]],
        scale: [f[8], f[9], f[10]],
    })
}

/// Reads an `hkaSkeleton` object.
pub fn read_skeleton(v: Value<'_>) -> Result<Skeleton> {
    let parents = v.get("parentIndices")?.ints()?;
    let bones = v.get("bones")?.elements()?;
    let pose = v.get("referencePose")?.elements()?;
    if parents.len() != bones.len() || pose.len() != bones.len() {
        return bail(format!(
            "skeleton has {} bones, {} parents, {} reference transforms",
            bones.len(),
            parents.len(),
            pose.len()
        ));
    }
    let bones = bones
        .iter()
        .zip(&parents)
        .zip(&pose)
        .map(|((b, &p), t)| {
            Ok(Bone {
                name: b.get("name")?.string()?,
                parent: u16::try_from(p).ok(),
                local: qs_transform(*t)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let skeleton = Skeleton {
        name: v.get("name")?.string()?,
        bones,
    };
    skeleton.validate()?;
    Ok(skeleton)
}

/// The first skeleton in a tagfile.
pub fn skeleton_from_tagfile(file: &TagFile) -> Result<Skeleton> {
    read_skeleton(file.first("hkaSkeleton")?)
}

fn read_root_motion(v: Value<'_>) -> Result<Option<RootMotion>> {
    if !v.is_a("hkaDefaultAnimatedReferenceFrame") {
        return bail(format!(
            "unsupported extracted motion type {}",
            v.type_name()
        ));
    }
    let up = v.get("up")?.floats()?;
    let forward = v.get("forward")?.floats()?;
    let samples = v
        .get("referenceFrameSamples")?
        .elements()?
        .iter()
        .map(|s| {
            let f = s.floats()?;
            Ok([f[0], f[1], f[2], f[3]])
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(RootMotion {
        up: [up[0], up[1], up[2]],
        forward: [forward[0], forward[1], forward[2]],
        duration: v.get("duration")?.f32()?,
        samples,
    }))
}

/// Decoded transform tracks plus timing, before bone binding.
struct RawAnim {
    duration: f32,
    frame_count: u32,
    frame_duration: f32,
    tracks: Vec<spline::SampledTrack>,
}

fn decode_animation(a: Value<'_>, stats: &mut DecodeStats) -> Result<RawAnim> {
    let duration = a.get("duration")?.f32()?;
    let transform_tracks = a.get("numberOfTransformTracks")?.int()? as usize;
    let float_tracks = a.get("numberOfFloatTracks")?.int()? as usize;
    if transform_tracks > 1024 {
        return bail(format!("implausible track count {transform_tracks}"));
    }
    if a.is_a("hkaSplineCompressedAnimation") {
        let num_frames = a.get("numFrames")?.int()? as usize;
        let anim = spline::SplineAnimation {
            num_frames,
            num_blocks: a.get("numBlocks")?.int()? as usize,
            max_frames_per_block: a.get("maxFramesPerBlock")?.int()? as usize,
            transform_tracks,
            float_tracks,
            block_offsets: a
                .get("blockOffsets")?
                .ints()?
                .into_iter()
                .map(|o| o as usize)
                .collect(),
            data: a.get("data")?.array_bytes()?,
        };
        let frame_duration = a.get("frameDuration")?.f32()?;
        let tracks = spline::decode(&anim, stats)?;
        Ok(RawAnim {
            duration,
            frame_count: num_frames as u32,
            frame_duration,
            tracks,
        })
    } else if a.is_a("hkaInterleavedUncompressedAnimation") {
        let transforms = a.get("transforms")?.elements()?;
        let n = transform_tracks.max(1);
        let frames = transforms.len() / n;
        let mut tracks = vec![spline::SampledTrack::default(); transform_tracks];
        for f in 0..frames {
            for (t, track) in tracks.iter_mut().enumerate() {
                let trs = qs_transform(transforms[f * n + t])?;
                track.translation.push(trs.translation);
                track.rotation.push(trs.rotation);
                track.scale.push(trs.scale);
            }
        }
        Ok(RawAnim {
            duration,
            frame_count: frames as u32,
            frame_duration: if frames > 1 {
                duration / (frames - 1) as f32
            } else {
                0.0
            },
            tracks,
        })
    } else {
        bail(format!("unsupported animation type {}", a.type_name()))
    }
}

/// Decodes the animation bound by the first `hkaAnimationBinding` in a tagfile.
/// `skeleton_bones` bounds the track-to-bone mapping.
pub fn clip_from_tagfile(
    file: &TagFile,
    name: &str,
    skeleton_bones: usize,
    stats: &mut DecodeStats,
) -> Result<AnimClip> {
    let binding = file.first("hkaAnimationBinding")?;
    let anim = binding
        .get("animation")?
        .deref()?
        .ok_or_else(|| Error::Format("binding has no animation".into()))?;
    let raw = decode_animation(anim, stats)?;
    let mut map: Vec<i64> = binding.get("transformTrackToBoneIndices")?.ints()?;
    if map.is_empty() {
        map = (0..raw.tracks.len() as i64).collect();
    }
    if map.len() != raw.tracks.len() {
        return bail(format!(
            "binding maps {} tracks but animation has {}",
            map.len(),
            raw.tracks.len()
        ));
    }
    let blend = match binding.get("blendHint")?.int()? {
        0 => BlendHint::Normal,
        _ => BlendHint::Additive,
    };
    let mut tracks = Vec::with_capacity(raw.tracks.len());
    for (track, &bone) in raw.tracks.into_iter().zip(&map) {
        if bone < 0 {
            continue;
        }
        if bone as usize >= skeleton_bones {
            return bail(format!(
                "track maps to bone {bone} but skeleton has {skeleton_bones}"
            ));
        }
        tracks.push(BoneTrack {
            bone: bone as u16,
            translation: track.translation,
            rotation: track.rotation,
            scale: track.scale,
        });
    }
    let root_motion = match anim.get("extractedMotion")?.deref()? {
        Some(m) => read_root_motion(m)?,
        None => None,
    };
    Ok(AnimClip {
        name: name.to_string(),
        duration: raw.duration,
        frame_count: raw.frame_count,
        frame_duration: raw.frame_duration,
        blend,
        skeleton_bones: skeleton_bones as u16,
        tracks,
        root_motion,
    })
}
