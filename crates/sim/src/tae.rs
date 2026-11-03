//! TAE (TimeAct) events at run time: what the playing animations say right now.
//!
//! Each animation has a timeline of events. The ones the simulation uses:
//!
//! - `ChrActionFlag` (type 0): cancel windows and state flags, by flag type. The flag types that
//!   gate actions are listed in [`cancel`].
//! - `AddSpEffect` (types 66, 67, 401): applies a SpEffect while the event is active. The effect's
//!   `SpEffectParam.behaviorRefId` is what the scripts query with `env(3036, ref)`.
//! - `SetTurnSpeed` (type 224): turn speed in degrees per second while active.
//! - `ChrPhysicsVelocityChange` (type 920): sets the body velocity from
//!   `ChrPhysicsVelocityChangeParam` when the event starts (jumps).
//! - `AttackBehavior` (1), `BulletBehavior` (2), `PCBehavior` (307) and `IFrames` (954): kept for
//!   combat.
//!
//! Event names and parameter layouts come from DSAnimStudio's Sekiro template
//! (`cache/refs/TAE.Template.SDT.xml`); action-flag meanings are DS3-era community labels checked
//! against which animations carry them, not against the exe.

use sekiro_formats::param::{RawParam, Table};
use sekiro_formats::paramdef::ParamDef;
use sekiro_formats::tae::{self, AnimHeader, Template};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// ChrActionFlag types that open action windows, by the action they allow.
pub mod cancel {
    /// Suppresses damage motion: native handler `b51b70` case 24 sets action +0x88 bit 16;
    /// `bdbb40` reads it into damage record +0x1CA (`b6e6a0`). See docs/OWL.md.
    pub const DISABLE_DAMAGE_MOTION: i32 = 24;
    /// Interrupt into movement (stick) allowed.
    pub const MOVE: i32 = 11;
    /// Step / dodge allowed.
    pub const DODGE: i32 = 26;
    /// Turning disabled while active.
    pub const DISABLE_TURN: i32 = 7;
    /// Main weapon attack (R1) allowed: combo windows.
    pub const ATTACK: i32 = 115;
    /// Interrupt into a deathblow allowed (including the Mikiri counter's recovery).
    pub const DEATHBLOW: i32 = 154;
    /// Guard (L1) allowed.
    pub const GUARD: i32 = 117;
    /// Combat art (L2 in the DS3 naming) allowed.
    pub const COMBAT_ART: i32 = 118;
    /// Jump allowed.
    pub const JUMP: i32 = 119;
    /// Prosthetic (R2 in the DS3 naming) allowed.
    pub const PROSTHETIC: i32 = 137;
    /// Full invulnerability while active, like TAE 951: the exe's ChrActionFlag handler
    /// (`FUN_140b51b70`, case 0x5e) sets bit 0 of the action module's flag word +0x70, the bit
    /// FwLo's RustSekiro traced for 951 (`docs/kb/damage.md`). In the data only the Great
    /// Shinobi Owl's warp clips carry it, over each window in which he is faded out
    /// (`docs/OWL.md`, "Warp and vanish"). The DSAnimStudio label is `SetStartbit0`.
    pub const WARP_INVULNERABLE: i32 = 94;
    /// Item use allowed.
    pub const ITEM: i32 = 31;
    /// Weapon or prosthetic switch allowed.
    pub const SWITCH: i32 = 32;
}

/// One decoded event of interest.
#[derive(Debug, Clone, PartialEq)]
pub enum TaeKind {
    /// A visual FXR request. The renderer decides whether its format is supported.
    Effect(TaeEffect),
    /// `DecalParamID_DummyPoly` (138) and `_CenterBody` (137, `dummy` -1): a DecalParam stain
    /// where the dummy is (a deathblow's blood on the floor).
    Decal {
        id: i32,
        dummy: i32,
    },
    /// Wire timing and raw arguments; semantics beyond the travel window remain untraced.
    Wire {
        kind: i32,
        args: Vec<u8>,
    },
    ActionFlag(i32),
    SpEffect(i32),
    /// TAE 224; IsLockOnCheck gates the rate (FwLo, FUN_140b574b0).
    TurnSpeed {
        speed: f32,
        lock_only: bool,
    },
    /// TAE 605: indexed value sampled from its authored start/end gradient.
    HavokVariable {
        id: i32,
        values: [f32; 2],
    },
    VelocityChange(i32),
    /// TAE 922 scales existing velocity while its window is active.
    VelocityScale(i32),
    Attack {
        judge: i32,
        attack_type: i32,
    },
    Bullet {
        judge: i32,
        dummy_poly: i32,
    },
    PcBehavior {
        judge: i32,
    },
    IFrames(i32),
    /// `IFrames_MistRaven` (950) and `IFrames_DuringAction` (951): full invulnerability. 950 sits
    /// on Wolf's knock-down, launch, death and revival animations, 951 on the deathblow throws.
    /// `IFrames_ThrowAtkStillHurts` (952) sits only on Wolf's being-thrown clips (`a2xx_600xxx`):
    /// no other attack lands while the throw holds him, and the throw's own damage still does
    /// (it is applied by the throw, not by hit detection; see [`crate::grab`]).
    Invulnerable,
    /// `BoostRootMotionToReachTarget` (760): scales root motion so an attack reaches its target.
    ReachTarget(ReachTarget),
    /// `SetKnockbackPercent` (226): scales the knockback this animation's character receives.
    KnockbackPercent(f32),
    /// `FixedRotationDirection` (703): the character stops turning toward its target.
    FixedRotation,
    /// `SetSPRegenRatePercent` (225): scales posture recovery.
    PostureRegenPercent(f32),
    /// `Blend` (16): crossfade into this animation over the event's length.
    Blend,
    /// `ChrActionFlag` 3 (`ShieldBlock`): guarding. ArgB is the behaviour judge of the guard
    /// action, whose AtkParam row is the guard row (the soldier's BehaviorParam for judge 900 is
    /// its "deflect" row, Wolf's judge 90 is "sword_guard").
    Guard {
        judge: i32,
    },
    /// `StaminaControlParam` (960): which `staminaRecoverRatio_forTypeNNN` posture recovery uses.
    StaminaControl(i32),
    /// `CommonBehavior` (5): a behaviour without a hitbox; in a deathblow start clip, the grab
    /// check.
    CommonBehavior {
        judge: i32,
    },
    /// `ThrowAttackBehavior` (304): where a throw's damage lands.
    ThrowAttack {
        judge: i32,
    },
    /// `PlaySound_*` (128 to 132): a sound of type `kind` (0 a, 1 c, ...; see
    /// [`sound_name`]) and number `id`.
    Sound {
        kind: i32,
        id: i32,
    },
    /// `SetOpacityKeyframe` (193): the model's opacity goes linearly from `from` at the event's
    /// start to `to` at its end (the Owl's warps fade him out and back in with three of these).
    Opacity {
        from: f32,
        to: f32,
    },
    /// `CreateAISound2` (237): the character makes AiSoundParam sound `0` while the event is
    /// active (Wolf's walk, run and sprint loops carry one over their whole length).
    AiSound(i32),
    /// `SetEzStateRequestID` (231): a message to the map's event scripts while the event is
    /// active. The scripts wait for it with `IfCharacterHasEventMessage` (EMEVD 4[8]); the Owl's
    /// first deathblow get-up raises 20, which starts his phase 2 (`docs/OWL.md`).
    EzStateRequest(i32),
    /// TAE 932 exposes a special action/throw opening, separate from ChrActionFlag.
    ActionUnlock(i32),
    /// TAE 233 persistently switches model variant groups, including the ape's severed head.
    DrawMask([u8; 32]),
    /// `CameraLookAtTarget` (151): while active the camera focus chases dummy poly `dummy` of
    /// the character, at `rate` per 1/30 s (the normal chase rates when 0), and the camera's
    /// pitch and yaw stay within `limits` degrees (up, down, left, right) around the direction
    /// to the dummy. Field meanings traced by FwLo's RustSekiro (`docs/kb/camera.md`, exe
    /// `FUN_140b517a0`, `FUN_140742230`); the rate is the template's `Unk04`.
    CameraLookAt {
        dummy: i32,
        rate: f32,
        limits: [f32; 4],
    },
    /// `CameraModule3` (153): the camera distance chases `distance` (metres) while active,
    /// at `start_speed` per 1/30 s (the param rate when 0); `end_speed`, when above 0, is the rate
    /// back to the normal distance afterwards; `slow_start` eases from the distance at the
    /// event's first frame instead (FwLo's trace, `docs/kb/camera.md`).
    CameraDistance {
        distance: f32,
        start_speed: f32,
        end_speed: f32,
        slow_start: bool,
    },
    /// `SetLockParamID` (155): the camera uses CameraSetParam slot `n` (1 is the default) while
    /// active: its LockCamParam and CameraParam rows, blended in over the slot's `beginTime`.
    CameraSlot(i32),
}

/// Template-defined visual effect parameters, independent of rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct TaeEffect {
    pub event_type: i32,
    pub id: i32,
    pub dummy: i32,
    pub blade_tip: Option<i32>,
    /// TAE 118: 0 body, 1 left weapon, 2 right weapon.
    pub dummy_source: i32,
    pub slot: i32,
    pub follow_dummy: bool,
    pub ignore_dummy_angle: bool,
    pub restrict_to_dummy: bool,
    pub extra_condition: i32,
    pub repeat: bool,
    pub repeat_type: i32,
}

/// The sound event name for a TAE sound type and id: the type letter followed by the id padded
/// to nine digits (type 1, id 101001001 gives `c101001001`). Letters from the DSAnimStudio
/// Sekiro template.
pub fn sound_name(kind: i32, id: i32) -> String {
    let letter = match kind {
        0 => 'a',
        1 => 'c',
        2 => 'f',
        3 => 'o',
        4 => 'p',
        5 => 's',
        6 => 'm',
        7 => 'v',
        8 => 'x',
        9 => 'b',
        10 => 'g',
        // Hit, guard and deflect sounds from HitEffectSeParam (exe sound type 12, traced by
        // FwLo's RustSekiro, `docs/kb/sound.md`).
        12 => 'z',
        _ => '?',
    };
    format!("{letter}{id:09}")
}

/// Fields of a `BoostRootMotionToReachTarget` (760) event, metres and degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReachTarget {
    pub reference_dist: f32,
    pub range_min: f32,
    pub range_max: f32,
    pub arrive_angle: f32,
    pub arrive_dist: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimedEvent {
    pub start: f32,
    pub end: f32,
    pub kind: TaeKind,
    /// The event's `StateInfo` (action flags, attacks, bullets, sounds, effects). When non-zero
    /// the engine applies the event only while an SpEffect with that `stateInfo` is active, so
    /// weapon-enchantment hits and upgraded-prosthetic sounds stay off by default.
    pub state_info: i32,
}

impl TimedEvent {
    pub fn active_at(&self, t: f32) -> bool {
        t >= self.start && t < self.end
    }

    /// Clamped linear progress. A point event reaches its end value at its start.
    pub fn progress(&self, t: f32) -> f32 {
        if self.end > self.start {
            ((t - self.start) / (self.end - self.start)).clamp(0.0, 1.0)
        } else if t >= self.start {
            1.0
        } else {
            0.0
        }
    }
}

/// One animation's TAE entry.
#[derive(Debug, Clone, Default)]
pub struct TaeAnim {
    pub hkx_source: i64,
    pub events: Arc<Vec<TimedEvent>>,
}

/// Raw animations from every `*.tae` in an extracted `anibnd`, by full id.
pub fn read_tae_dir(dir: &Path) -> HashMap<i64, ParsedAnim> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("tae") {
            continue;
        }
        let Ok(t) = std::fs::read(&path)
            .map_err(|_| ())
            .and_then(|d| tae::parse(&d).map_err(|_| ()))
        else {
            continue;
        };
        let category =
            tae::category_from_file_name(&path.file_name().unwrap_or_default().to_string_lossy());
        for a in t.animations {
            let id = tae::full_id(category, a.id);
            let events_from = match &a.header {
                Some(AnimHeader::ImportOther { from, .. }) => Some(tae::full_id(category, *from)),
                _ => None,
            };
            out.insert(
                id,
                ParsedAnim {
                    hkx_source: tae::full_id(category, a.hkx_source()),
                    events_from,
                    events: a.events,
                },
            );
        }
    }
    out
}

/// An animation as read from a TAE file, before decoding.
pub struct ParsedAnim {
    pub hkx_source: i64,
    /// Set when the animation takes its events from another one.
    pub events_from: Option<i64>,
    pub events: Vec<tae::TaeEvent>,
}

/// What a SpEffect means to the simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpEffectInfo {
    /// `behaviorRefId`, answered by `env(3036)`; 0 for none.
    pub behavior_ref: i32,
    /// Seconds the effect lasts after it is applied; 0 means only while the TAE event is active.
    pub duration: f32,
    pub state_info: i32,
    /// `spCategory` and `categoryPriority`: an effect applied later replaces the active effects
    /// of its category whose priority does not exceed its own ([`crate::sp_category`]).
    pub category: i32,
    pub priority: i32,
    /// `vfxId`: the SpEffectVfxParam row it shows while active (48100 is the perilous-attack
    /// kanji 危, [`crate::perilous::KANJI_VFX`]); 0 or -1 for none.
    pub vfx: i32,
    /// `hearingSearchEnemyRate`: scales the radius of the AI sounds its bearer makes (rows with
    /// `bSpEffectEnable`), as Covert B 150010's 0.5 ([`crate::duel::population::Hearing`]).
    pub sound_rate: f32,
    /// `hearingSearchEnemyCut`: percent taken off its bearer's hearing (200140 "hearing 0").
    pub hearing_cut: f32,
}

/// A velocity change for jumps (`ChrPhysicsVelocityChangeParam`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityChange {
    pub fall_type: u8,
    pub horizontal_scale: f32,
    pub vertical_scale: f32,
    pub horizontal: f32,
    pub vertical: f32,
    pub horizontal_angle: f32,
}

fn load_table(param_dir: &Path, defs_dir: &Path, file: &str, def: &str) -> Option<Table> {
    let raw = RawParam::parse(std::fs::read(param_dir.join(format!("{file}.param"))).ok()?).ok()?;
    let def = ParamDef::load(&defs_dir.join(format!("{def}.xml"))).ok()?;
    Table::new(file, &raw, Arc::new(def), None).ok()
}

/// All TAE timelines of one character plus the params the events refer to.
#[derive(Default)]
pub struct TaeDb {
    anims: HashMap<i64, TaeAnim>,
    pub sp_effects: HashMap<i32, SpEffectInfo>,
    pub velocity_changes: HashMap<i32, VelocityChange>,
}

impl TaeDb {
    /// Builds a timeline database from decoded animations (also useful for offline fixtures).
    pub fn from_animations(anims: impl IntoIterator<Item = (i64, TaeAnim)>) -> Self {
        Self {
            anims: anims.into_iter().collect(),
            ..Self::default()
        }
    }
    /// Loads timelines from an extracted `anibnd` directory, decoding them with the template, and
    /// the SpEffect and velocity-change params from the extracted `gameparam` directory.
    pub fn load(anibnd_dir: &Path, template: &Template, param_dir: &Path, defs_dir: &Path) -> Self {
        let parsed = read_tae_dir(anibnd_dir);
        let mut anims = HashMap::with_capacity(parsed.len());
        for (&id, a) in &parsed {
            let events = match a.events_from.and_then(|f| parsed.get(&f)) {
                Some(src) => &src.events,
                None => &a.events,
            };
            let decoded: Vec<TimedEvent> =
                events.iter().filter_map(|e| decode(template, e)).collect();
            anims.insert(
                id,
                TaeAnim {
                    hkx_source: a.hkx_source,
                    events: Arc::new(decoded),
                },
            );
        }
        let mut db = TaeDb {
            anims,
            ..TaeDb::default()
        };
        if let Some(t) = load_table(param_dir, defs_dir, "SpEffectParam", "SpEffect") {
            for row in t.rows() {
                db.sp_effects.insert(
                    row.id(),
                    SpEffectInfo {
                        behavior_ref: row.i32("behaviorRefId").unwrap_or(0),
                        duration: row.f32("effectEndurance").unwrap_or(0.0),
                        state_info: row.int("stateInfo").unwrap_or(0) as i32,
                        category: row.int("spCategory").unwrap_or(0) as i32,
                        priority: row.int("categoryPriority").unwrap_or(0) as i32,
                        vfx: row.i32("vfxId").unwrap_or(0),
                        sound_rate: row.f32("hearingSearchEnemyRate").unwrap_or(1.0),
                        hearing_cut: row.f32("hearingSearchEnemyCut").unwrap_or(0.0),
                    },
                );
            }
        }
        if let Some(t) = load_table(
            param_dir,
            defs_dir,
            "ChrPhysicsVelocityChangeParam",
            "ChrPhysicsVelocityChangeParam",
        ) {
            for row in t.rows() {
                db.velocity_changes.insert(
                    row.id(),
                    VelocityChange {
                        fall_type: row.int("fallType").unwrap_or(0) as u8,
                        horizontal_scale: row.f32("horizontalVelocityScale").unwrap_or(1.0),
                        vertical_scale: row.f32("verticalVelocityScale").unwrap_or(1.0),
                        horizontal: row.f32("horizontalVelocityChange").unwrap_or(0.0),
                        vertical: row.f32("verticalVelocityChange").unwrap_or(0.0),
                        horizontal_angle: row.f32("horizontalVelocityAngle").unwrap_or(0.0),
                    },
                );
            }
        }
        db
    }

    /// Loads from the standard cache layout.
    pub fn load_from_cache(cache: &Path, chr: &str) -> Self {
        let template = std::fs::read_to_string(cache.join("refs/TAE.Template.SDT.xml"))
            .ok()
            .and_then(|s| Template::parse_xml(&s).ok());
        match template {
            Some(t) => Self::load(
                &cache.join(format!("raw/chr/{chr}.anibnd.d")),
                &t,
                &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
                &cache.join("refs/paramdex/Defs"),
            ),
            None => Self::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.anims.len()
    }

    pub fn is_empty(&self) -> bool {
        self.anims.is_empty()
    }

    pub fn anim(&self, name: &str) -> Option<&TaeAnim> {
        self.anims.get(&tae::anim_id(name)?)
    }

    /// Full ids of every animation with a TAE entry (what env 1114 "does the animation exist"
    /// is answered from).
    pub fn anim_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.anims.keys().copied()
    }

    /// Crossfade length into `name`: the end of its `Blend` event that starts at 0, if any.
    pub fn blend_in(&self, name: &str) -> Option<f32> {
        self.anim(name)?
            .events
            .iter()
            .find(|e| e.kind == TaeKind::Blend && e.start <= 0.001)
            .map(|e| e.end)
    }

    /// Full id to HKX source id, for every animation that borrows another's HKX.
    pub fn hkx_aliases(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        self.anims
            .iter()
            .filter(|(id, a)| **id != a.hkx_source)
            .map(|(id, a)| (*id, a.hkx_source))
    }
}

fn decode(template: &Template, e: &tae::TaeEvent) -> Option<TimedEvent> {
    if matches!(e.event_type, 715 | 720 | 900..=905 | 921) {
        return Some(TimedEvent {
            start: e.start,
            end: e.end,
            state_info: 0,
            kind: TaeKind::Wire {
                kind: e.event_type,
                args: e.params.clone(),
            },
        });
    }
    let d = template.decode(e)?;
    let int = |name: &str| d.field(name).and_then(|v| v.as_int()).map(|v| v as i32);
    let kind = match e.event_type {
        96 | 99 | 100 | 118..=121 => TaeKind::Effect(TaeEffect {
            event_type: e.event_type,
            id: int(if e.event_type == 120 {
                "FFXID_MAIN"
            } else {
                "FFXID"
            })?,
            dummy: int(if e.event_type == 118 {
                "DummyPolyBladeBaseID"
            } else {
                "DummyPolyID"
            })?,
            blade_tip: if e.event_type == 118 {
                int("DummyPolyBladeTipID")
            } else {
                None
            },
            dummy_source: int("DummyPolySource").unwrap_or(0),
            slot: int("SlotID").unwrap_or(-1),
            follow_dummy: int("IsFollowDummyPoly").unwrap_or(0) != 0,
            ignore_dummy_angle: int("IsIgnoreDummyPolyAngle").unwrap_or(0) != 0,
            restrict_to_dummy: int("IsRestrictToDummyPoly").unwrap_or(0) != 0,
            extra_condition: int("ExtraSpawnCondition").unwrap_or(0),
            repeat: e.event_type == 99 || int("IsRepeat").unwrap_or(0) != 0,
            repeat_type: int("RepeatType").unwrap_or(0),
        }),
        137 | 138 => TaeKind::Decal {
            id: int("DecalParamID")?,
            dummy: int("DummyPolyID").unwrap_or(-1),
        },
        0 if int("FlagType")? == 3 => TaeKind::Guard {
            judge: int("ArgB").unwrap_or(-1),
        },
        0 => TaeKind::ActionFlag(int("FlagType")?),
        960 => TaeKind::StaminaControl(int("StaminaRatioType")?),
        66 | 67 | 401 => TaeKind::SpEffect(int("SpEffectID")?),
        224 => {
            let speed = d.field("TurnSpeed")?.as_f32()?;
            TaeKind::TurnSpeed {
                speed,
                lock_only: int("IsLockOnCheck").unwrap_or(0) != 0,
            }
        }
        233 => TaeKind::DrawMask(e.params.get(..32)?.try_into().ok()?),
        605 => {
            let tae::FieldValue::Grad(values) = d.field("ValueGradient")? else {
                return None;
            };
            TaeKind::HavokVariable {
                id: int("VariableID")?,
                values: *values,
            }
        }
        920 => TaeKind::VelocityChange(int("ChrPhysicsVelocityParam ID")?),
        922 => TaeKind::VelocityScale(int("ChrPhysicsVelocityParam ID")?),
        1 => TaeKind::Attack {
            judge: int("BehaviorJudgeID")?,
            attack_type: int("AttackType").unwrap_or(0),
        },
        2 => TaeKind::Bullet {
            judge: int("BehaviorJudgeID")?,
            dummy_poly: int("DummyPolyID").unwrap_or(-1),
        },
        307 => TaeKind::PcBehavior {
            judge: int("BehaviorJudgeID")?,
        },
        954 => TaeKind::IFrames(int("IFrameType").unwrap_or(0)),
        5 => TaeKind::CommonBehavior {
            judge: int("BehaviorJudgeID")?,
        },
        304 => TaeKind::ThrowAttack {
            judge: int("BehaviorJudgeID")?,
        },
        950..=952 => TaeKind::Invulnerable,
        760 if int("IsEnable").unwrap_or(1) != 0 => {
            let f = |name: &str| d.field(name).and_then(|v| v.as_f32()).unwrap_or(0.0);
            TaeKind::ReachTarget(ReachTarget {
                reference_dist: f("ReferenceDist"),
                range_min: f("EnableRangeMin"),
                range_max: f("EnableRangeMax"),
                arrive_angle: f("ArriveAngleFromTarget"),
                arrive_dist: f("ArriveDistFromTarget"),
            })
        }
        226 => TaeKind::KnockbackPercent(int("KnockbackPercent")? as f32),
        703 if int("IsEnable").unwrap_or(1) != 0 => TaeKind::FixedRotation,
        225 => TaeKind::PostureRegenPercent(int("RegenRatePercent")? as f32),
        16 => TaeKind::Blend,
        128..=132 => TaeKind::Sound {
            kind: int("SoundType")?,
            id: int("SoundID")?,
        },
        193 => {
            let f = |name: &str| d.field(name).and_then(|v| v.as_f32()).unwrap_or(1.0);
            TaeKind::Opacity {
                from: f("OpacityAtEventStart"),
                to: f("OpacityAtEventEnd"),
            }
        }
        231 => TaeKind::EzStateRequest(int("EzStateRequestID")?),
        237 => TaeKind::AiSound(int("AISoundID")?),
        932 => TaeKind::ActionUnlock(int("Unk00")?),
        // Finishing throws raise the map's player message 10 through TAE 936.
        // Cross-checked with FwLo's MIT boss-ending notes; other 936 operations remain opaque.
        936 if e.params.get(..4) == Some(&10_i32.to_le_bytes()) => TaeKind::EzStateRequest(10),
        151 => {
            let f = |name: &str| d.field(name).and_then(|v| v.as_f32()).unwrap_or(0.0);
            TaeKind::CameraLookAt {
                dummy: int("DummyPolyID")?,
                rate: f("Unk04"),
                limits: [
                    f("LookUpLimit"),
                    f("LookDownLimit"),
                    f("LookLeftLimit"),
                    f("LookRightLimit"),
                ],
            }
        }
        153 => {
            let f = |name: &str| d.field(name).and_then(|v| v.as_f32()).unwrap_or(0.0);
            TaeKind::CameraDistance {
                distance: d.field("CamDistTargetOverride")?.as_f32()?,
                start_speed: f("StartInterpolationSpeed"),
                end_speed: f("EndInterpolationSpeed"),
                slow_start: int("SlowStart").unwrap_or(0) != 0,
            }
        }
        155 => TaeKind::CameraSlot(int("LockParamID")?),
        _ => return None,
    };
    Some(TimedEvent {
        start: e.start,
        end: e.end,
        kind,
        state_info: int("StateInfo").unwrap_or(0),
    })
}

/// A clip being played, as the TAE runtime sees it.
#[derive(Debug, Clone)]
pub struct PlayingClip {
    pub animation: String,
    /// Time before and after this tick's advance.
    pub prev_time: f32,
    pub time: f32,
    pub looping: bool,
}

/// The TAE state for this tick.
#[derive(Debug, Clone, Default)]
pub struct TaeFrame {
    /// Visual events active or started this tick. They do not change gameplay rules.
    pub effects: Vec<EffectCue>,
    /// Floor stains (DecalParam id, dummy) whose event started this tick.
    pub decals: Vec<(i32, i32)>,
    pub flags: HashSet<i32>,
    /// SpEffects applied by active events plus timed effects still running.
    pub sp_effects: Vec<i32>,
    pub behavior_refs: HashSet<i32>,
    /// Smallest active `SetTurnSpeed`, degrees per second.
    pub turn_speed: Option<f32>,
    pub locked_turn_speed: Option<f32>,
    pub havok_variables: Vec<(i32, f32)>,
    /// Guard judge of an active `ShieldBlock` flag (see [`TaeKind::Guard`]).
    pub guard_judge: Option<i32>,
    /// Posture recovery control type (TAE 960) of the first playing clip that sets one.
    pub stamina_type: Option<i32>,
    /// Velocity changes whose event started this tick.
    pub velocity_changes: Vec<i32>,
    pub velocity_scales: Vec<i32>,
    pub attacks: Vec<TimedEvent>,
    pub iframes: Vec<i32>,
    /// True if a playing clip has action flags other than physics marker 28.
    /// Clips without them (idle, locomotion loops) put no limits on actions.
    pub restricted: bool,
    /// Sound events that started this tick: (sound type, sound id).
    pub sounds: Vec<(i32, i32)>,
    /// TAE 950 / 951 / 952, or ChrActionFlag 94 ([`cancel::WARP_INVULNERABLE`]): no hit lands
    /// at all.
    pub invulnerable: bool,
    /// Model opacity from TAE 193 (the lowest of the active events); `None` is fully opaque.
    /// See [`TaeFrame::opacity`].
    pub opacity: Option<f32>,
    /// TAE 231 messages to the event scripts, active or started this tick.
    pub ezstate_requests: Vec<i32>,
    /// `CreateAISound2` (237) AiSoundParam ids, active or started this tick: the noise the
    /// character makes for the AI's hearing ([`crate::duel::population::Hearing`]).
    pub ai_sounds: Vec<i32>,
    pub action_unlocks: HashSet<i32>,
    /// TAE 760 of the first playing clip that has one.
    pub reach_target: Option<ReachTarget>,
    /// TAE 226 as a fraction (0.3 for 30 %); `None` is full knockback.
    pub knockback_scale: Option<f32>,
    /// TAE 703: stop tracking the target.
    pub fixed_rotation: bool,
    /// TAE 225 as a fraction; `None` is normal posture recovery.
    pub posture_regen_scale: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectCue {
    pub effect: TaeEffect,
    pub started: bool,
    pub start: f32,
    pub end: f32,
}

impl TaeFrame {
    pub fn turn_speed_for(&self, locked: bool) -> Option<f32> {
        let mut speed = self.turn_speed;
        if let Some(s) = self.locked_turn_speed.filter(|_| locked) {
            min_assign(&mut speed, s);
        }
        speed
    }

    pub fn flag(&self, f: i32) -> bool {
        self.flags.contains(&f)
    }

    /// Model opacity, 0 (not drawn) to 1 (fully drawn).
    pub fn opacity(&self) -> f32 {
        self.opacity.unwrap_or(1.0)
    }
}

fn min_assign(current: &mut Option<f32>, value: f32) {
    *current = Some(current.map_or(value, |c| c.min(value)));
}

/// Tracks timed SpEffects across ticks and builds each [`TaeFrame`].
#[derive(Default)]
pub struct TaeRuntime {
    /// Last authored group changes; 255 preserves the row's initial group value.
    pub draw_mask: Option<[u8; 32]>,
    /// Effects with a duration, and their remaining seconds.
    timed: HashMap<i32, f32>,
    /// SpEffects the character always has (equipment, skills); they open `StateInfo` gates.
    pub resident: Vec<i32>,
    pub frame: TaeFrame,
    /// Native character message register, retained until the next TAE 231 write.
    pub event_message: Option<i32>,
    /// An animation event that adds an SpEffect whose `effectEndurance` is -1 leaves it on the
    /// character until a map event clears it (the Demon of Hatred's 5022 "opener done" marker,
    /// read by its Lua). Only the Demon opts in: other enemies' clips add -1 state effects
    /// (200000, 200004, 275060 and so on) whose persistence is not verified.
    pub keep_endless: bool,
    /// Effects applied since the last [`Self::take_applied`] that last beyond their event window,
    /// for the category replacement in [`crate::sp_category`].
    applied: Vec<i32>,
}

impl TaeRuntime {
    /// A runtime for an enemy (`keep_endless`) or for Wolf.
    pub fn for_kind(keep_endless: bool) -> Self {
        Self {
            keep_endless,
            ..Self::default()
        }
    }

    /// Starts (or refreshes) a timed SpEffect from outside the animation, such as one a bullet
    /// left on the character ([`crate::status`]). It shows in the frame from the next update on.
    pub fn add_timed(&mut self, id: i32, seconds: f32) {
        let r = self.timed.entry(id).or_insert(0.0);
        *r = r.max(seconds);
        self.applied.push(id);
    }

    /// The effects applied (and still running) since the last call.
    pub fn take_applied(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.applied)
    }

    /// Ids of the timed effects still running.
    pub fn timed_ids(&self) -> Vec<i32> {
        self.timed.keys().copied().collect()
    }

    /// Ids of the SpEffects active this frame (the clips' events and timed effects) plus the
    /// resident ones, each once, so a rate product never counts an effect twice.
    pub fn active_sp_effects(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self
            .frame
            .sp_effects
            .iter()
            .chain(&self.resident)
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Removes an externally applied effect when a map event clears it.
    pub fn clear_timed(&mut self, id: i32) {
        self.timed.remove(&id);
        self.frame.sp_effects.retain(|effect| *effect != id);
    }

    /// Computes the frame for `clips` (the main clip first) after they advanced by `dt`.
    pub fn update(&mut self, db: &TaeDb, clips: &[PlayingClip], dt: f32) {
        for remaining in self.timed.values_mut() {
            *remaining -= dt;
        }
        self.timed.retain(|_, r| *r > 0.0);
        let states = self.active_states(db, clips);
        let mut f = TaeFrame::default();
        for clip in clips {
            let Some(anim) = db.anim(&clip.animation) else {
                continue;
            };
            let t = clip.time;
            for e in anim.events.iter() {
                if e.state_info != 0 && !states.contains(&e.state_info) {
                    continue;
                }
                // Flag 28 is the water/ladder physics marker, not an input window.
                if matches!(e.kind, TaeKind::ActionFlag(flag) if flag != 28)
                    || matches!(e.kind, TaeKind::Guard { .. })
                {
                    f.restricted = true;
                }
                // Started this tick: the event's start lies in [prev_time, time). A clip that just
                // began reports prev_time 0, so events at 0 fire on its first tick.
                let started = e.start >= clip.prev_time && e.start < t.max(clip.prev_time + 1e-6);
                if !e.active_at(t) && !started {
                    continue;
                }
                match &e.kind {
                    TaeKind::DrawMask(values) => {
                        let mask = self.draw_mask.get_or_insert([255; 32]);
                        for (current, &value) in mask.iter_mut().zip(values) {
                            if value <= 1 {
                                *current = value;
                            }
                        }
                    }
                    TaeKind::Effect(effect) => f.effects.push(EffectCue {
                        effect: effect.clone(),
                        started,
                        start: e.start,
                        end: e.end,
                    }),
                    TaeKind::Decal { id, dummy } => {
                        if started {
                            f.decals.push((*id, *dummy));
                        }
                    }
                    TaeKind::Wire { .. } => {}
                    TaeKind::ActionFlag(flag) => {
                        f.flags.insert(*flag);
                        if *flag == cancel::WARP_INVULNERABLE && e.active_at(t) {
                            f.invulnerable = true;
                        }
                    }
                    TaeKind::SpEffect(id) => {
                        f.sp_effects.push(*id);
                        if let Some(info) = db.sp_effects.get(id)
                            && info.duration > 0.0
                            && started
                        {
                            self.timed.insert(*id, info.duration);
                            self.applied.push(*id);
                        } else if self.keep_endless
                            && started
                            && db
                                .sp_effects
                                .get(id)
                                .is_some_and(|info| info.duration < 0.0)
                        {
                            self.timed.insert(*id, f32::INFINITY);
                            self.applied.push(*id);
                        }
                    }
                    TaeKind::TurnSpeed { speed, lock_only } => {
                        min_assign(
                            if *lock_only {
                                &mut f.locked_turn_speed
                            } else {
                                &mut f.turn_speed
                            },
                            *speed,
                        );
                    }
                    TaeKind::HavokVariable { id, values } => {
                        if e.active_at(t) && !f.havok_variables.iter().any(|(key, _)| key == id) {
                            f.havok_variables
                                .push((*id, values[0] + (values[1] - values[0]) * e.progress(t)));
                        }
                    }
                    TaeKind::VelocityScale(id) => {
                        if e.active_at(t) {
                            f.velocity_scales.push(*id);
                        }
                    }
                    TaeKind::VelocityChange(id) => {
                        if started {
                            f.velocity_changes.push(*id);
                        }
                    }
                    TaeKind::Attack { .. }
                    | TaeKind::Bullet { .. }
                    | TaeKind::PcBehavior { .. } => {
                        f.attacks.push(e.clone());
                    }
                    TaeKind::IFrames(kind) => f.iframes.push(*kind),
                    TaeKind::Invulnerable => f.invulnerable = true,
                    TaeKind::ReachTarget(r) => {
                        f.reach_target.get_or_insert(*r);
                    }
                    TaeKind::KnockbackPercent(p) => {
                        f.knockback_scale.get_or_insert(p / 100.0);
                    }
                    TaeKind::FixedRotation => f.fixed_rotation = true,
                    TaeKind::PostureRegenPercent(p) => {
                        f.posture_regen_scale.get_or_insert(p / 100.0);
                    }
                    // Read by the deathblow timeline (`deathblow::ThrowTimes`), not per frame.
                    TaeKind::Blend
                    | TaeKind::CommonBehavior { .. }
                    | TaeKind::ThrowAttack { .. } => {}
                    // Read by the camera director (`camera::CameraDirector`) from the clip.
                    TaeKind::CameraLookAt { .. }
                    | TaeKind::CameraDistance { .. }
                    | TaeKind::CameraSlot(_) => {}
                    TaeKind::Guard { judge } => {
                        f.flags.insert(3);
                        f.guard_judge.get_or_insert(*judge);
                    }
                    TaeKind::Sound { kind, id } => {
                        if started {
                            f.sounds.push((*kind, *id));
                        }
                    }
                    TaeKind::StaminaControl(kind) => {
                        if e.active_at(t) {
                            f.stamina_type.get_or_insert(*kind);
                        }
                    }
                    TaeKind::Opacity { from, to } => {
                        if e.active_at(t) {
                            let k = e.progress(t);
                            let o = from + (to - from) * k;
                            f.opacity = Some(f.opacity.map_or(o, |c: f32| c.min(o)));
                        }
                    }
                    TaeKind::ActionUnlock(id) => {
                        f.action_unlocks.insert(*id);
                    }
                    TaeKind::AiSound(id) => f.ai_sounds.push(*id),
                    TaeKind::EzStateRequest(id) => {
                        f.ezstate_requests.push(*id);
                        if started {
                            self.event_message = Some(*id);
                        }
                    }
                }
            }
        }
        f.sp_effects.extend(self.timed.keys().copied());
        f.sp_effects.sort_unstable();
        f.sp_effects.dedup();
        for id in &f.sp_effects {
            if let Some(info) = db.sp_effects.get(id)
                && info.behavior_ref > 0
            {
                f.behavior_refs.insert(info.behavior_ref);
            }
        }
        self.frame = f;
    }

    /// `stateInfo` of every SpEffect active this tick: from the clips' SpEffect events, timed
    /// effects still running and resident effects.
    fn active_states(&self, db: &TaeDb, clips: &[PlayingClip]) -> HashSet<i32> {
        let mut ids: Vec<i32> = self.timed.keys().chain(&self.resident).copied().collect();
        for clip in clips {
            let Some(anim) = db.anim(&clip.animation) else {
                continue;
            };
            for e in anim.events.iter() {
                if let TaeKind::SpEffect(id) = e.kind
                    && e.active_at(clip.time)
                {
                    ids.push(id);
                }
            }
        }
        ids.iter()
            .filter_map(|id| db.sp_effects.get(id))
            .map(|info| info.state_info)
            .filter(|&s| s != 0)
            .collect()
    }
}

#[cfg(test)]
mod fxr_event_tests {
    use super::*;

    #[test]
    fn turn_speed_lock_check_survives_decoding() {
        let template = Template::parse_xml(r#"<action_template game="SDT"><action id="224" name="SetTurnSpeed"><f32 name="TurnSpeed"/><b name="IsLockOnCheck"/></action></action_template>"#).unwrap();
        for locked_only in [false, true] {
            let mut params = 180.0f32.to_le_bytes().to_vec();
            params.push(u8::from(locked_only));
            let decoded = decode(&template, &event(224, params)).unwrap();
            let db = TaeDb::from_animations([(
                0,
                TaeAnim {
                    hkx_source: 0,
                    events: Arc::new(vec![decoded]),
                },
            )]);
            let mut runtime = TaeRuntime::default();
            runtime.update(
                &db,
                &[PlayingClip {
                    animation: "a000_000000".into(),
                    prev_time: 0.0,
                    time: 0.5,
                    looping: false,
                }],
                0.5,
            );
            let frame = runtime.frame;
            assert_eq!(frame.turn_speed_for(true), Some(180.0));
            assert_eq!(frame.turn_speed_for(false), (!locked_only).then_some(180.0));
        }
    }

    #[test]
    fn gradients_are_windowed_even_when_a_tick_skips_the_whole_event() {
        for span in [0.1, 0.0] {
            let event = TimedEvent {
                start: 0.1,
                end: 0.1 + span,
                state_info: 0,
                kind: TaeKind::HavokVariable {
                    id: 5,
                    values: [2.0, 4.0],
                },
            };
            let opacity = TimedEvent {
                kind: TaeKind::Opacity { from: 1.0, to: 0.0 },
                ..event.clone()
            };
            let db = TaeDb::from_animations([(
                0,
                TaeAnim {
                    hkx_source: 0,
                    events: Arc::new(vec![event.clone(), opacity]),
                },
            )]);
            for (time, expected) in [(0.15, (span > 0.0).then_some(3.0)), (0.25, None)] {
                let mut runtime = TaeRuntime::default();
                runtime.update(
                    &db,
                    &[PlayingClip {
                        animation: "a000_000000".into(),
                        prev_time: 0.0,
                        time,
                        looping: false,
                    }],
                    time,
                );
                let value = runtime
                    .frame
                    .havok_variables
                    .first()
                    .map(|(_, value)| *value);
                match expected {
                    Some(expected) => {
                        assert!((value.unwrap() - expected).abs() < 1e-6);
                        assert!((runtime.frame.opacity.unwrap() - 0.5).abs() < 1e-6);
                    }
                    None => {
                        assert!(value.is_none());
                        assert!(runtime.frame.opacity.is_none());
                    }
                }
            }
            if span == 0.0 {
                assert_eq!(event.progress(0.1), 1.0);
            }
        }
    }

    const TEMPLATE: &str = r#"<action_template game="SDT">
    <action id="96" name="SpawnOneShotFFX">
    <s32 name="FFXID"/><s32 name="DummyPolyID"/><s32 name="SlotID"/>
    <b name="IsIgnoreDummyPolyAngle"/><b name="IsFollowDummyPoly"/>
    <b name="IsRestrictToDummyPoly"/><u8 name="ExtraSpawnCondition"/>
    <s16 name="StateInfo"/><s16 assert="0"/>
    </action>
    <action id="118" name="SpawnFFX_Blade">
    <s32 name="FFXID"/><s16 name="DummyPolySource"/>
    <s16 name="DummyPolyBladeBaseID"/><s16 name="DummyPolyBladeTipID"/>
    <s16 name="SlotID"/><s32 assert="0"/>
    </action></action_template>"#;

    fn event(kind: i32, params: Vec<u8>) -> tae::TaeEvent {
        tae::TaeEvent {
            event_type: kind,
            start: 0.0,
            end: 1.0,
            unk04: 0,
            group: None,
            params,
        }
    }

    #[test]
    fn fxr_decoding_retains_dummy_flags_slots_and_blade_pair() {
        let template = Template::parse_xml(TEMPLATE).unwrap();
        let mut bytes = Vec::new();
        for value in [220505i32, 800, -1] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[1, 1, 1, 2, 7, 0, 0, 0]);
        let decoded = decode(&template, &event(96, bytes)).unwrap();
        assert_eq!(decoded.state_info, 7);
        let TaeKind::Effect(effect) = decoded.kind else {
            panic!("expected FXR request")
        };
        assert_eq!((effect.id, effect.dummy, effect.slot), (220505, 800, -1));
        assert!(effect.follow_dummy && effect.ignore_dummy_angle && effect.restrict_to_dummy);
        assert_eq!(effect.extra_condition, 2);
        let mut bytes = 401000i32.to_le_bytes().to_vec();
        for value in [2i16, 300, 301, 4] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0i32.to_le_bytes());
        let decoded = decode(&template, &event(118, bytes)).unwrap();
        let TaeKind::Effect(effect) = decoded.kind else {
            panic!("expected blade FXR request")
        };
        assert_eq!(
            (
                effect.dummy_source,
                effect.dummy,
                effect.blade_tip,
                effect.slot
            ),
            (2, 300, Some(301), 4)
        );
        assert!(decode(&template, &event(118, vec![0; 3])).is_none());
    }

    #[test]
    fn fxr_cues_obey_start_edges_state_info_and_event_end() {
        let template = Template::parse_xml(TEMPLATE).unwrap();
        let mut bytes = Vec::new();
        for value in [220505i32, 800, -1] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 8]);
        let mut decoded = decode(&template, &event(96, bytes)).unwrap();
        decoded.state_info = 7;
        let mut db = TaeDb::from_animations([(
            3000,
            TaeAnim {
                hkx_source: 3000,
                events: Arc::new(vec![decoded]),
            },
        )]);
        db.sp_effects.insert(
            123,
            SpEffectInfo {
                behavior_ref: 0,
                duration: 0.0,
                state_info: 7,
                category: 0,
                priority: 0,
                vfx: 0,
                sound_rate: 1.0,
                hearing_cut: 0.0,
            },
        );
        let mut runtime = TaeRuntime::default();
        let clip = |prev_time, time| {
            [PlayingClip {
                animation: "a000_003000".into(),
                prev_time,
                time,
                looping: false,
            }]
        };
        runtime.update(&db, &clip(0.0, 0.1), 0.1);
        assert!(runtime.frame.effects.is_empty());
        runtime.resident.push(123);
        runtime.update(&db, &clip(0.0, 0.1), 0.1);
        assert_eq!(runtime.frame.effects.len(), 1);
        assert!(runtime.frame.effects[0].started);
        runtime.update(&db, &clip(0.1, 0.2), 0.1);
        assert!(!runtime.frame.effects[0].started);
        runtime.update(&db, &clip(0.9, 1.0), 0.1);
        assert!(runtime.frame.effects.is_empty());
    }

    #[test]
    fn resident_effect_also_in_frame_counts_once() {
        let covert = SpEffectInfo {
            behavior_ref: 0,
            duration: 0.0,
            state_info: 0,
            category: 0,
            priority: 0,
            vfx: 0,
            sound_rate: 0.5,
            hearing_cut: 0.0,
        };
        let mut runtime = TaeRuntime::default();
        runtime.resident.push(150010);
        runtime.frame.sp_effects.push(150010);
        let ids = runtime.active_sp_effects();
        assert_eq!(ids, [150010]);
        let effects = std::collections::HashMap::from([(150010, covert)]);
        let rate: f32 = ids
            .iter()
            .filter_map(|id| effects.get(id))
            .map(|e| e.sound_rate)
            .product();
        assert_eq!(rate, 0.5);
    }
}
