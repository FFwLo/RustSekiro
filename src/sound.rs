//! Sound effects from the player's own Sekiro install: the FMOD banks are decoded by
//! the game's own fmodex64.dll (tools/sekiro-extract sounds-fmod) into
//! extracted/sound_pcm/<name>.pcm, and each actor's TAE PlaySound events play them.
//!
//! TAE sound key = SoundType letter + SoundID as 9 digits ("1: (c) Character", 4011 ->
//! c000004011); FMOD picks one of the numbered variants (c000004010, ...b, ...c) at
//! random, so do we. Floor (x) sounds map to c-events by floor material (see
//! FLOOR_MATERIAL); armour (b) sounds use the cloth / plate sets (see play_tae_sounds).

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::audio::{AddAudioSource, ChannelCount, Decodable, SampleRate, Source, Volume};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;

use crate::actor::{Actor, data_for};
use crate::data::Combat;

/// Interleaved PCM decoded from an FSB sample.
#[derive(Asset, TypePath, Clone)]
pub struct PcmSound {
    channels: u16,
    rate: u32,
    samples: Arc<[f32]>,
}

pub struct PcmDecoder {
    samples: Arc<[f32]>,
    pos: usize,
    channels: u16,
    rate: u32,
}

impl Iterator for PcmDecoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let v = self.samples.get(self.pos).copied();
        self.pos += 1;
        v
    }
}

impl Source for PcmDecoder {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len().saturating_sub(self.pos))
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.channels.max(1)).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.rate.max(1)).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.samples.len() as f64 / self.channels.max(1) as f64 / self.rate.max(1) as f64))
    }
}

impl Decodable for PcmSound {
    type Decoder = PcmDecoder;
    fn decoder(&self) -> PcmDecoder {
        PcmDecoder { samples: self.samples.clone(), pos: 0, channels: self.channels, rate: self.rate }
    }
}

#[derive(Default, TypePath)]
struct PcmLoader;

impl AssetLoader for PcmLoader {
    type Asset = PcmSound;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(&self, reader: &mut dyn Reader, _: &(), _: &mut LoadContext<'_>) -> Result<PcmSound, Self::Error> {
        let mut d = Vec::new();
        reader.read_to_end(&mut d).await?;
        if d.len() < 12 || &d[0..4] != b"SPCM" {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not an SPCM file"));
        }
        let rate = u32::from_le_bytes(d[4..8].try_into().unwrap());
        let channels = u16::from_le_bytes([d[8], d[9]]);
        let samples: Arc<[f32]> = d[12..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
        Ok(PcmSound { channels, rate, samples })
    }

    fn extensions(&self) -> &[&str] {
        &["pcm"]
    }
}

/// Base key ("c000004010") -> file names of its variants.
#[derive(Resource, Default)]
struct SoundIndex {
    variants: HashMap<String, Vec<String>>,
    rng: u32,
}

#[derive(Resource)]
pub struct SoundVolume(pub f32);

/// Sounds requested by gameplay code (base keys, e.g. "s000003010").
#[derive(Resource, Default)]
pub struct SoundQueue(pub Vec<(String, Vec3)>);

/// HitEffectSeParam's 9-value column groups in paramdef order; the exe picks a group by the
/// attack's atkMaterial_forSe (FUN_1410c0ea0: group k at row offset k * 0x24).
const HIT_SE_GROUPS: [&str; 15] =
    ["Iron", "Fire", "Wood", "Body", "Eclipse", "Energy", "None", "Dmy1", "Dmy2", "Dmy3", "Maggot", "Wax", "FireFlame", "EclipseGas", "EnergyStrong"];

/// Hit sounds as the exe builds them (FUN_14092ecb0 -> FUN_1410c0cb0): for each of the defender's
/// two materials (NpcParam materialSe1/2, protector defenseMaterial1/2) the HitEffectSeParam row
/// of that id gives, at the attack's group / type (index pow + type * 3 over Slash, Blow, Thrust)
/// / power (S, L, LL), a sound of type 12 = 'z'. E.g. Wolf's sword (group Iron, slash, S) on the
/// General (114, 108): z100000103 + z000000108. Zero / missing rows: silent.
pub fn hit_sounds(combat: &Combat, atk: &crate::data::Attack, def_materials: [i64; 2]) -> Vec<String> {
    lookup(combat, "HitEffectSeParam", atk, def_materials)
}

fn lookup(combat: &Combat, table: &str, atk: &crate::data::Attack, def_materials: [i64; 2]) -> Vec<String> {
    let Some(table) = combat.params.get(table) else { return Vec::new() };
    let Some(group) = HIT_SE_GROUPS.get(atk.atk_material_se.max(0) as usize) else { return Vec::new() };
    let kind = match atk.atk_type {
        1 => "Blow",
        2 => "Thrust",
        _ => "Slash",
    };
    let pow = ["S", "L", "LL"][atk.atk_pow_se.clamp(0, 2) as usize];
    def_materials
        .iter()
        .filter(|m| **m >= 0)
        .filter_map(|m| table.get(m.to_string())?.get(format!("{group}_{kind}_{pow}"))?.as_i64())
        .filter(|id| *id > 0)
        .map(|id| format!("z{id:09}"))
        .collect()
}

/// Guard / deflect sounds (the same exe path: FUN_14092f040 for a deflect, HitEffectSeJustGuardParam
/// first and HitEffectSeParam when that gives nothing; a block uses HitEffectSeParam): rows = the
/// attack's defSeMaterial1/2. The General's sword attacks (100 / 139, Iron group): deflect
/// z199999980, block z200000101.
pub fn guard_sounds(combat: &Combat, atk: &crate::data::Attack, deflect: bool) -> Vec<String> {
    let mats = crate::vfx::guard_materials(combat, [atk.def_se_material1, atk.def_se_material2], true);
    if deflect {
        let jg = lookup(combat, "HitEffectSeJustGuardParam", atk, mats);
        if !jg.is_empty() {
            return jg;
        }
    }
    lookup(combat, "HitEffectSeParam", atk, mats)
}

/// The defender's two hit-sound materials: the enemy's NpcParam materialSe1/2, Wolf's body
/// protector defenseMaterial1/2.
pub fn defender_materials(combat: &Combat, def: &crate::actor::Actor) -> [i64; 2] {
    let (r, a, b) = match def.side {
        crate::actor::Side::Enemy => (combat.npc(def), "materialSe1", "materialSe2"),
        crate::actor::Side::Player => (combat.param("EquipParamProtector", 100000), "defenseMaterial1", "defenseMaterial2"),
    };
    [r[a].as_i64().unwrap_or(-1), r[b].as_i64().unwrap_or(-1)]
}

/// Floor sounds (TAE type 'x'): FMOD event c + (id / 1000 * 1000 + floor material). HitMtrlParam
/// row names (soulsmods Paramdex SDT/Names): 1 cobblestone, 3 soil, 4 wood, 5 grass, 6 gravel,
/// 16 tile, 28 tatami, ... - all have footstep events. The arena is stone: 1 (Ashina's courtyards).
/// gap: real floors come from map collision.
const FLOOR_MATERIAL: i64 = 1;

/// The floor material under an actor: the map's hit collision triangle (HitMtrlParam id) when a
/// map is loaded, else the flat arena's stone.
fn floor_material(centre: Vec3) -> i64 {
    match crate::map::floor_material(centre) {
        Some(m) if m != u32::MAX => m as i64,
        _ => FLOOR_MATERIAL,
    }
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<PcmSound>()
            .init_asset_loader::<PcmLoader>()
            .insert_resource(SoundVolume(0.6))
            .insert_resource(index_sounds())
            .init_resource::<SoundQueue>()
            .init_resource::<SlottedSounds>()
            .add_systems(Update, stop_slotted_off_screen)
            // TAE sounds per simulation step (two steps in one frame would skip the first's events).
            .add_systems(FixedUpdate, (play_tae_sounds, deathblow_mix).after(crate::actor::ActorSet::Advance))
            .add_systems(Update, play_queued);
    }
}

fn index_sounds() -> SoundIndex {
    let dir = crate::paths::root().join("extracted/sound_pcm");
    let mut variants: HashMap<String, Vec<String>> = HashMap::new();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".pcm") else { continue };
        let base = stem.trim_end_matches(|c: char| c.is_ascii_lowercase() && stem.len() > 10);
        variants.entry(base.to_string()).or_default().push(name);
    }
    if variants.is_empty() {
        warn!("{} has no sounds: run tools/extract.ps1 (sounds-fmod) for audio", dir.display());
    }
    SoundIndex { variants, rng: 0x1234_5678 }
}

/// "1: (c) Character" -> 'c'
fn type_letter(t: &str) -> Option<char> {
    t.split_once('(').and_then(|(_, r)| r.chars().next())
}

/// stateInfos of the SpEffects on an actor now: its anim's TAE SpEffects, plus (Wolf) the resident
/// SpEffects of his equipment - the equipped prosthetic tool level (e.g. 70000: 127000 "Shuriken
/// LV1", stateInfo 905; 71000 Firecracker: 911) and the combat art - and 914 (SpEffect 106000
/// "No enchantment": Wolf's sword carries no buff; gap: buff items are not modelled). Gated TAE
/// events (PlaySound / SpawnFFX StateInfo) run only when one matches.
pub fn active_state_infos(combat: &Combat, a: &Actor, config: Option<&crate::config::GameConfig>, tool: Option<i64>) -> Vec<i64> {
    const NO_ENCHANTMENT: i64 = 914;
    let d = data_for(combat, &a);
    let mut v: Vec<i64> = if a.anim.is_empty() { Vec::new() } else { d.sp_effects_at(&a.anim, a.t).iter().map(|(_, s)| s.state_info).collect() };
    if a.side == crate::actor::Side::Player {
        v.push(NO_ENCHANTMENT);
        let mut weapons: Vec<i64> = tool.into_iter().collect();
        if let Some(c) = config {
            weapons.push(c.player.combat_art);
        }
        for w in weapons {
            let row = combat.param("EquipParamWeapon", w);
            let mut ids: Vec<i64> = ["residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2"].iter().filter_map(|k| row[*k].as_i64()).collect();
            // The tools' rows are not in params.EquipParamWeapon: their Prosthetic.resident (the
            // Flame Vent's 127200 "Ignition LV1" -> 915 turns on its flame FFX 300161 / 300164).
            if let Some(t) = combat.player.prosthetics.iter().find(|t| t.id == w) {
                ids.push(t.resident);
            }
            for id in ids.into_iter().filter(|id| *id > 0) {
                let si = d.sp_effects.get(&id.to_string()).map(|s| s.state_info).or_else(|| combat.param("SpEffectParam", id)["stateInfo"].as_i64());
                if let Some(si) = si.filter(|s| *s > 0) {
                    if !v.contains(&si) {
                        v.push(si);
                    }
                }
            }
        }
    }
    v
}


#[allow(clippy::too_many_arguments)]
/// The clip whose TAE sounds play this step and the time windows (lo, hi] in it: the TAE anim
/// from prev_t to t, or - Wolf's procedural walk / run / crouch-walk (Locomotion, no anim; their
/// footsteps c00000100x / 200x and armour rustle are the clips' PlaySound events) - the shown
/// locomotion clip (`locomotion_clip_at`), wrapping at the loop's end. A step that changes
/// locomotion clip (the move start into the loop, a turn to another direction) starts the new
/// clip at 0. gap: the rest of the clip left behind is not played. Also the clip time (the
/// once-per-step key).
pub(crate) fn sound_windows(a: &Actor, d: &crate::data::CharData) -> Option<(String, Vec<(f32, f32)>, f32)> {
    const AT_ZERO: f32 = -1e-6;
    if !a.anim.is_empty() {
        // An event at 0 fires in the anim's first step.
        let lo = if a.prev_t == 0.0 && a.t > 0.0 && a.t < 0.05 { AT_ZERO } else { a.prev_t };
        return Some((a.anim.clone(), vec![(lo, a.t)], a.t));
    }
    if a.state != "Locomotion" {
        return None;
    }
    let dt = a.t - a.prev_t;
    let (key, t, looping) = a.locomotion_clip_at(a.t);
    let len = d.length(&key);
    let now = if looping && len > 0.0 { t.rem_euclid(len) } else { t };
    if dt <= 0.0 {
        return Some((key, Vec::new(), now));
    }
    if a.locomotion_clip_at(a.prev_t).0 != key {
        return Some((key, vec![(AT_ZERO, now)], now));
    }
    let before = now - dt;
    let windows = if before >= 0.0 {
        vec![(before, now)]
    } else if looping && len > 0.0 {
        vec![(before + len, len), (AT_ZERO, now)]
    } else {
        vec![(AT_ZERO, now)]
    };
    Some((key, windows, now))
}

/// A slotted FFX's sound (play_tae_sounds): its owner and SlotID, the anim and event end it
/// lives to, the anim time last seen (a drop = the anim started over).
pub(crate) struct Slotted {
    pub(crate) owner: Entity,
    pub(crate) slot: i64,
    pub(crate) anim: String,
    pub(crate) end: f32,
    pub(crate) last_t: f32,
    pub(crate) ev: crate::fmod::EventHandle,
}

/// The slotted sounds playing (play_tae_sounds).
#[derive(Resource, Default)]
pub(crate) struct SlottedSounds(pub(crate) Vec<Slotted>);

/// Stops every slotted sound on the title, the picker and death: play_tae_sounds ends them
/// with their anims, but FixedUpdate is frozen off the Playing screen.
pub(crate) fn stop_slotted_off_screen(screen: Option<Res<State<crate::menu::Screen>>>, fmod: Option<NonSendMut<crate::fmod::Fmod>>, mut slotted: ResMut<SlottedSounds>) {
    use crate::menu::Screen;
    if slotted.0.is_empty() || !screen.is_some_and(|s| matches!(s.get(), Screen::Title | Screen::Picker | Screen::Dead)) {
        return;
    }
    if let Some(mut f) = fmod {
        for s in &slotted.0 {
            f.stop(s.ev);
        }
    }
    slotted.0.clear();
}

fn play_tae_sounds(
    mut commands: Commands,
    combat: Res<Combat>,
    config: Option<Res<crate::config::GameConfig>>,
    assets: Res<AssetServer>,
    volume: Res<SoundVolume>,
    mut index: ResMut<SoundIndex>,
    actors: Query<(Entity, &Actor, &Transform, Option<&crate::player::Player>, Option<&crate::model::Dummies>)>,
    globals: Query<&GlobalTransform>,
    mut seen: Local<HashMap<Entity, (String, f32)>>,
    mut fmod: Option<NonSendMut<crate::fmod::Fmod>>,
    mut slotted: ResMut<SlottedSounds>,
) {
    let slotted = &mut slotted.0;
    // Sounds of slotted FFX (SpawnOneShotFFX with a SlotID >= 0, e.g. the Mortal Blade's aura
    // 440000 on slot 0, frames 5-100 of a106_316000): the effect lives while its TAE event runs and
    // ends with it or when the anim changes - its (looping) sound with it. Before, the loop kept
    // playing after Mortal Draw. A looping anim (an idle: enemy.rs starts it over at t 0 under
    // the same key) ends it too, and a new sound on a slot replaces the slot's old one, so a
    // slot event that runs past the clip's end does not stack an instance per loop (c1140
    // IdleBattle s000611410).
    if let Some(f) = fmod.as_mut() {
        slotted.retain_mut(|s| {
            let now = actors.get(s.owner).ok().filter(|(_, a, ..)| a.anim == s.anim).map(|(_, a, ..)| a.t);
            match now {
                Some(t) if t < s.end && t >= s.last_t => {
                    s.last_t = t;
                    true
                }
                _ => {
                    f.stop(s.ev);
                    false
                }
            }
        });
    }
    for (entity, a, tf, player, dm) in &actors {
        // Where a sound plays: its event's DummyPolyID (PlaySound_BySlot 129 / 131,
        // _ByDummyPoly_PlayerVoice 130; an FFX's sound on the FFX's dummy, SpawnFFX_Blade's on its
        // DummyPolyBladeBaseID; DSAnimStudio TAE.Template.SDT), else the character (CenterBody 128,
        // Weapon 132). A weapon / tool dummy that is not modelled falls back as the effects do
        // (ffx::dummy_candidates); any other missing dummy -> the character.
        let place = |e: &crate::data::Event| -> Vec3 {
            let id = if e.kind == 118 { e.arg_i64("DummyPolyBladeBaseID") } else { e.arg_i64("DummyPolyID") };
            id.filter(|d| *d >= 0)
                .and_then(|d| {
                    let cands = if d >= 10000 { crate::ffx::dummy_candidates(d) } else { vec![d as i16] };
                    cands.iter().find_map(|k| dm?.0.get(k)).and_then(|m| globals.get(*m).ok()).map(|g| g.translation())
                })
                .unwrap_or(tf.translation)
        };
        let d = data_for(&combat, &a);
        let Some((clip, windows, now)) = sound_windows(a, d) else { continue };
        // Several render frames can share one fixed step: handle each step once.
        let step = (clip.clone(), now);
        if seen.get(&entity) == Some(&step) {
            continue;
        }
        seen.insert(entity, step);
        let Some(anim) = d.anim(&clip) else { continue };
        let mut states: Option<Vec<i64>> = None;
        for e in &anim.events {
            if !matches!(e.kind, 128..=132 | 96 | 118) {
                continue;
            }
            if !e.ungated() {
                let gate = e.arg_i64("StateInfo").unwrap_or(0);
                let tool = player.zip(config.as_deref()).and_then(|(p, c)| crate::player::equipped_tool(&combat, c, p.tool_slot)).map(|t| t.id);
                let active = states.get_or_insert_with(|| active_state_infos(&combat, a, config.as_deref(), tool));
                if !active.contains(&gate) {
                    continue;
                }
            }
            let started = windows.iter().any(|&(lo, hi)| e.start > lo && e.start <= hi);
            if !started {
                continue;
            }
            let at = place(e);
            // Effects with their own sound: SpawnOneShotFFX (96) / SpawnFFX_Blade (118) play 's' +
            // the FFX id when the banks have it (e.g. Wolf's third slash FFX 404000 -> s000404000,
            // the General's 3020 FFX 4065 / 4066). SpawnFFX_ByFloor (112) is left out: its FFXID
            // is a FootSfxParam column (ffx.rs FootSfx), not an effect id.
            if matches!(e.kind, 96 | 118) {
                if let (Some(ffx), Some(f)) = (e.arg_i64("FFXID").filter(|v| *v > 0), fmod.as_mut()) {
                    let key = format!("s{ffx:09}");
                    if f.has(&key) {
                        if let Some(slot) = e.arg_i64("SlotID").filter(|s| *s >= 0 && !a.anim.is_empty()) {
                            slotted.retain(|s| {
                                let same = s.owner == entity && s.slot == slot;
                                if same {
                                    f.stop(s.ev);
                                }
                                !same
                            });
                            if let Some(ev) = f.play_tracked(&key, at) {
                                slotted.push(Slotted { owner: entity, slot, anim: a.anim.clone(), end: e.end, last_t: a.t, ev });
                            }
                        } else {
                            f.play(&key, at);
                        }
                    }
                }
                continue;
            }
            let (Some(id), Some(letter)) = (e.arg_i64("SoundID"), e.args.get("SoundType").and_then(|v| v.as_str()).and_then(type_letter)) else {
                continue;
            };
            // The game's own FMOD event when available: floor 'x' = id rounded to 1000 + floor
            // material, armour 'b' = id + defense material (Wolf's protector 113, c1020/c1010 108).
            let event = match letter {
                'x' => format!("c{:09}", id / 1000 * 1000 + floor_material(at.with_y(tf.translation.y))),
                // Armour: id + the wearer's material - the first of its two (protector
                // defenseMaterial1/2, NpcParam materialSe1/2) the banks have an event for
                // (Wolf 113; the General 114 has none, 108 has).
                'b' => {
                    let mats = defender_materials(&combat, &a);
                    let keys: Vec<String> = mats.iter().filter(|m| **m > 0).map(|m| format!("c{:09}", id + m)).collect();
                    let found = fmod.as_ref().and_then(|f| keys.iter().find(|k| f.has(k)).cloned());
                    found.or_else(|| keys.first().cloned()).unwrap_or_default()
                }
                _ => format!("{letter}{id:09}"),
            };
            // With the game's sound system running, an event its projects lack is silent in the game
            // too: no raw-sample stand-in (those were sounds the game never plays here).
            if let Some(f) = fmod.as_mut() {
                f.play(&event, at);
                continue;
            }
            let file = if letter == 'b' {
                // Armour rustle: FEV events c0000xx113 (Wolf, EquipParamProtector defenseMaterial 113)
                // / ...108 (c1020 NpcParam materialSe). gap: the event -> sounddef link is not parsed;
                // inferred sets: cloth robe for Wolf, plate armour for the NPCs (c1020 / c1010 both 108).
                let set = if a.side == crate::actor::Side::Player { "body-lobe-" } else { "body-armor-" };
                match pick_prefix(&mut index, set) {
                    Some(f) => f,
                    None => continue,
                }
            } else {
                let key = if letter == 'x' { format!("c{:09}", id / 1000 * 1000 + floor_material(at.with_y(tf.translation.y))) } else { format!("{letter}{id:09}") };
                let Some(file) = pick(&mut index, &key) else { continue };
                file
            };
            commands.spawn((
                AudioPlayer::<PcmSound>(assets.load(format!("sound_pcm/{file}"))),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume.0)).with_spatial(true),
                Transform::from_translation(at),
            ));
        }
    }
}

fn pick(index: &mut SoundIndex, key: &str) -> Option<String> {
    let n = index.variants.get(key)?.len();
    index.rng ^= index.rng << 13;
    index.rng ^= index.rng >> 17;
    index.rng ^= index.rng << 5;
    Some(index.variants[key][(index.rng as usize) % n].clone())
}

/// A random sample whose name starts with `prefix` and ends in a digit (e.g. body-lobe-3).
fn pick_prefix(index: &mut SoundIndex, prefix: &str) -> Option<String> {
    let files: Vec<String> = index
        .variants
        .values()
        .flatten()
        .filter(|f| f.starts_with(prefix) && f.trim_end_matches(".pcm").ends_with(|c: char| c.is_ascii_digit()))
        .cloned()
        .collect();
    if files.is_empty() {
        return None;
    }
    index.rng ^= index.rng << 13;
    index.rng ^= index.rng >> 17;
    index.rng ^= index.rng << 5;
    Some(files[(index.rng as usize) % files.len()].clone())
}

fn play_queued(
    mut commands: Commands,
    assets: Res<AssetServer>,
    volume: Res<SoundVolume>,
    mut index: ResMut<SoundIndex>,
    mut queue: ResMut<SoundQueue>,
    mut fmod: Option<NonSendMut<crate::fmod::Fmod>>,
) {
    for (key, pos) in std::mem::take(&mut queue.0) {
        if let Some(f) = fmod.as_mut() {
            f.play(&key, pos);
            continue;
        }
        if let Some(file) = pick(&mut index, &key) {
            commands.spawn((
                AudioPlayer::<PcmSound>(assets.load(format!("sound_pcm/{file}"))),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume.0)).with_spatial(true),
                Transform::from_translation(pos),
            ));
        }
    }
}

/// One deathblow mix slot (exe, 5 floats each at the mix object's start; FUN_140ed7980):
/// fade-in time and what is left of it, fade-out time and what is left, the value (0-1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct MixSlot {
    in_total: f32,
    in_left: f32,
    out_total: f32,
    out_left: f32,
    value: f32,
}

/// The deathblow mix (exe: the mix object at the sound manager's +600). Wolf's TAE event 970
/// (DSAnimStudio "OldSoundDebug"; handler FUN_140b55260 -> FUN_140ed78f0, the player only) asks
/// for it each frame it runs with its 28 bytes: a type byte, then three (fade-in, fade-out)
/// pairs. A new request (another type; FUN_140ed7d20) picks the slot by type (1 -> slot 2,
/// 2 -> 0, 3 -> 1, 0 -> 1 when the byte at GameMan +0x11fc is 3, else 0) and loads that slot's
/// pair (tables 0x142b42c50 / 0x142b42c40: fade-in at +4 / +12 / +20, fade-out at +8 / +16 /
/// +24). Each frame the requested slot eases its value to 1 over what is left of its fade-in,
/// the others to 0 over theirs (FUN_140ed7980); a slot without a time jumps. The values go to
/// the mixer events ntc_ninsatsu2 / 1 / 3 (Fmod::set_deathblow_mix).
/// gap: the GameMan +0x11fc byte (copied from +0x11fd; looks like an online mode) is taken as
/// not 3 offline, so type 0 = slot 0 (ntc_ninsatsu2, the 445 Hz muffle).
#[derive(Default)]
pub(crate) struct DeathblowMix {
    slots: [MixSlot; 3],
    active: Option<usize>,
    /// The type of the request being served (the exe compares the requester and the type).
    last: Option<u8>,
}

impl DeathblowMix {
    /// One frame: `req` = the running 970 event's (type, its six times), if any. The values.
    pub(crate) fn step(&mut self, req: Option<(u8, [f32; 6])>, dt: f32) -> [f32; 3] {
        match req {
            None => {
                self.active = None;
                self.last = None;
            }
            Some((ty, t)) if self.last != Some(ty) => {
                let slot = match ty {
                    1 => Some(2),
                    0 | 2 => Some(0),
                    3 => Some(1),
                    _ => None,
                };
                self.active = slot;
                if let Some(i) = slot {
                    let (fin, fout) = (t[i * 2], t[i * 2 + 1]);
                    let s = &mut self.slots[i];
                    (s.in_total, s.in_left, s.out_total, s.out_left) = (fin, fin, fout, fout);
                }
                self.last = Some(ty);
            }
            _ => {}
        }
        for (i, s) in self.slots.iter_mut().enumerate() {
            if self.active == Some(i) {
                if s.in_total == 0.0 {
                    s.out_left = s.out_total;
                    s.in_left = 0.0;
                    s.value = 1.0;
                } else if dt < s.in_left {
                    s.value += (1.0 - s.value) * dt / s.in_left;
                    s.in_left -= dt;
                } else {
                    s.value = 1.0;
                    s.in_left = 0.0;
                }
            } else if s.out_total == 0.0 {
                s.out_left = 0.0;
                s.value = 0.0;
            } else if dt < s.out_left {
                s.value += (0.0 - s.value) * dt / s.out_left;
                s.out_left -= dt;
            } else {
                s.value = 0.0;
                s.out_total = 0.0;
            }
        }
        self.slots.map(|s| s.value)
    }
}

/// Wolf's running TAE 970 (the deathblow mix request): the type byte (the first field's low
/// byte; the export reads every field as f32, "unk0", missing when 0) and "unk1".."unk6".
fn mix_request(combat: &Combat, a: &Actor) -> Option<(u8, [f32; 6])> {
    let anim = data_for(combat, a).anim(&a.anim)?;
    let e = anim.events.iter().find(|e| e.kind == 970 && e.in_time(a.t))?;
    let f = |k: &str| e.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let ty = (f("unk0").to_bits() & 0xff) as u8;
    Some((ty, [f("unk1"), f("unk2"), f("unk3"), f("unk4"), f("unk5"), f("unk6")]))
}

fn deathblow_mix(
    time: Res<Time>,
    combat: Res<Combat>,
    wolf: Query<&Actor, With<crate::player::Player>>,
    fmod: Option<NonSendMut<crate::fmod::Fmod>>,
    mut mix: Local<DeathblowMix>,
) {
    let Some(mut fmod) = fmod else { return };
    let req = wolf.single().ok().filter(|a| !a.anim.is_empty()).and_then(|a| mix_request(&combat, a));
    let v = mix.step(req, time.delta_secs());
    fmod.set_deathblow_mix(v);
}

#[cfg(test)]
mod mix_tests {
    use super::DeathblowMix;

    /// A typical deathblow (type 0: in 0.3 s, out 2 s on slot 0): slot 0 rises to 1 in 0.3 s,
    /// stays while the event runs and falls to 0 over 2 s after it; type 1 drives slot 2 only.
    #[test]
    fn deathblow_mix_fades() {
        let t = [0.3, 2.0, 0.3, 1.0, 0.3, 3.0];
        let dt = 1.0 / 60.0;
        let mut m = DeathblowMix::default();
        let mut v = [0.0; 3];
        for _ in 0..9 {
            v = m.step(Some((0, t)), dt);
        }
        assert!(v[0] > 0.2 && v[0] < 1.0, "{v:?}");
        for _ in 0..30 {
            v = m.step(Some((0, t)), dt);
        }
        assert_eq!(v, [1.0, 0.0, 0.0]);
        for _ in 0..60 {
            v = m.step(None, dt);
        }
        assert!(v[0] > 0.0 && v[0] < 1.0, "{v:?}");
        for _ in 0..70 {
            v = m.step(None, dt);
        }
        assert_eq!(v, [0.0; 3]);
        for _ in 0..30 {
            v = m.step(Some((1, t)), dt);
        }
        assert_eq!(v, [0.0, 0.0, 1.0]);
    }
}
