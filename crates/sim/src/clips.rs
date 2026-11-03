//! Decoded animation clips for the simulation: lengths and root motion.
//!
//! Clips come from the extracted cache (`cache/anim/<chr>/<name>.bin`). Some animations have no
//! HKX of their own and play another's, as recorded in the TAE headers; [`ClipLibrary::alias`]
//! resolves those. The library is a cheap shared handle so the behaviour runtime (lengths) and
//! the locomotion (root motion) read the same cache.

use crate::behavior::ClipDurations;
use sekiro_formats::anim::AnimClip;
use sekiro_formats::tae;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// FwLo actor.rs: sample the last simulation frame for ended-source velocity.
const END_VELOCITY_SAMPLE_INTERVAL: f32 = 1.0 / 60.0;

#[derive(Default)]
struct Inner {
    dir: PathBuf,
    clips: HashMap<String, Option<Arc<AnimClip>>>,
    /// Full animation id to the id whose HKX it plays.
    hkx_alias: HashMap<i64, i64>,
}

/// A shared, lazily filled clip cache.
#[derive(Clone, Default)]
pub struct ClipLibrary(Rc<RefCell<Inner>>);

impl ClipLibrary {
    /// One lazy clip library per animation donor on the simulation thread.
    pub fn shared(dir: PathBuf) -> Self {
        thread_local! { static LIBRARIES: RefCell<HashMap<PathBuf, std::rc::Weak<RefCell<Inner>>>> = RefCell::new(HashMap::new()); }
        let dir = dir.canonicalize().unwrap_or(dir);
        LIBRARIES.with_borrow_mut(|libraries| {
            if let Some(inner) = libraries.get(&dir).and_then(std::rc::Weak::upgrade) {
                return Self(inner);
            }
            let library = Self::new(dir.clone());
            libraries.insert(dir, Rc::downgrade(&library.0));
            library
        })
    }
    /// Opens `dir` (`cache/anim/<chr>`), reading the extractor's `aliases.json` (animation name
    /// to the decoded clip it plays, chains already resolved) when present.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let aliases: std::collections::BTreeMap<String, String> =
            std::fs::read_to_string(dir.join("aliases.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
        let lib = Self(Rc::new(RefCell::new(Inner {
            dir,
            ..Inner::default()
        })));
        for (name, src) in aliases {
            if let (Some(a), Some(b)) = (tae::anim_id(&name), tae::anim_id(&src)) {
                lib.add_alias(a, b);
            }
        }
        lib
    }

    /// Records that animation `id` plays the HKX of `source` (full ids). The first alias recorded
    /// for an id wins, so the resolved `aliases.json` takes precedence over single TAE hops.
    pub fn add_alias(&self, id: i64, source: i64) {
        if id != source {
            self.0.borrow_mut().hkx_alias.entry(id).or_insert(source);
        }
    }

    /// Reads HKX aliases from every `*.tae` in `dir` (an extracted `anibnd`).
    pub fn with_tae_dir(self, dir: &Path) -> Self {
        for (id, anim) in crate::tae::read_tae_dir(dir) {
            self.add_alias(id, anim.hkx_source);
        }
        self
    }

    /// The animation name whose HKX `name` plays.
    pub fn alias(&self, name: &str) -> String {
        tae::anim_id(name)
            .and_then(|id| self.0.borrow().hkx_alias.get(&id).copied())
            .map(tae::anim_name)
            .unwrap_or_else(|| name.to_owned())
    }

    /// The decoded clip (source space), following HKX aliases. `None` if not extracted.
    pub fn clip(&self, name: &str) -> Option<Arc<AnimClip>> {
        if let Some(c) = self.0.borrow().clips.get(name) {
            return c.clone();
        }
        let load = |n: &str| -> Option<Arc<AnimClip>> {
            let bytes = std::fs::read(self.0.borrow().dir.join(format!("{n}.bin"))).ok()?;
            AnimClip::from_bytes(&bytes).ok().map(Arc::new)
        };
        let clip = load(name).or_else(|| {
            let src = self.alias(name);
            (src != name).then(|| load(&src)).flatten()
        });
        self.0
            .borrow_mut()
            .clips
            .insert(name.to_owned(), clip.clone());
        clip
    }

    /// Clip-local root delta, including a looping generator's tail and head or an ended
    /// blend source's final velocity
    /// (FwLo's MIT RustSekiro, actor.rs::advance).
    pub fn root_delta(&self, name: &str, start: f32, elapsed: f32, looping: bool) -> [f32; 4] {
        let Some(clip) = self.clip(name) else {
            return [0.0; 4];
        };
        let Some(root) = &clip.root_motion else {
            return [0.0; 4];
        };
        let len = clip.duration;
        if len <= 0.0 || elapsed <= 0.0 {
            return [0.0; 4];
        }
        let cycle_delta = if looping {
            let a = root.sample(0.0);
            let b = root.sample(len);
            std::array::from_fn(|i| b[i] - a[i])
        } else {
            [0.0; 4]
        };
        let sample = |t: f32| {
            if looping {
                let cycles = (t / len).floor();
                let p = root.sample(t.rem_euclid(len));
                std::array::from_fn::<_, 4, _>(|i| p[i] + cycles * cycle_delta[i])
            } else if t > len {
                // FwLo actor.rs: an ended source carries its last 1/60 s velocity
                // through the blend (SprintStart -> SprintLoop would otherwise stop).
                let end = root.sample(len);
                let before = root.sample((len - END_VELOCITY_SAMPLE_INTERVAL).max(0.0));
                let interval = len.min(END_VELOCITY_SAMPLE_INTERVAL);
                std::array::from_fn(|i| {
                    if i == 3 {
                        end[i]
                    } else {
                        end[i] + (end[i] - before[i]) / interval * (t - len)
                    }
                })
            } else {
                root.sample(t)
            }
        };
        let a = sample(start);
        let b = sample(start + elapsed);
        let [x, z] = crate::motion::rotate_xz([b[0] - a[0], b[2] - a[2]], -a[3]);
        [x, b[1] - a[1], z, b[3] - a[3]]
    }
}

impl ClipDurations for ClipLibrary {
    fn duration(&mut self, animation: &str) -> Option<f32> {
        // An animation the game data does not contain (the graph references a few, such as
        // a000_206100) counts as zero length, so its state ends at once instead of hanging.
        Some(self.clip(animation).map_or(0.0, |c| c.duration))
    }
}
