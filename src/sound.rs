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
            // TAE sounds per simulation step (two steps in one frame would skip the first's events).
            .add_systems(FixedUpdate, play_tae_sounds.after(crate::actor::ActorSet::Advance))
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
fn play_tae_sounds(
    mut commands: Commands,
    combat: Res<Combat>,
    config: Option<Res<crate::config::GameConfig>>,
    assets: Res<AssetServer>,
    volume: Res<SoundVolume>,
    mut index: ResMut<SoundIndex>,
    actors: Query<(Entity, &Actor, &Transform, Option<&crate::player::Player>)>,
    mut seen: Local<HashMap<Entity, (String, f32)>>,
    mut fmod: Option<NonSendMut<crate::fmod::Fmod>>,
    mut slotted: Local<Vec<(Entity, String, f32, crate::fmod::EventHandle)>>,
) {
    // Sounds of slotted FFX (SpawnOneShotFFX with a SlotID >= 0, e.g. the Mortal Blade's aura
    // 440000 on slot 0, frames 5-100 of a106_316000): the effect lives while its TAE event runs and
    // ends with it or when the anim changes - its (looping) sound with it. Before, the loop kept
    // playing after Mortal Draw.
    if let Some(f) = fmod.as_mut() {
        slotted.retain(|(ent, anim, end, ev)| {
            let alive = actors.get(*ent).is_ok_and(|(_, a, _, _)| a.anim == *anim && a.t < *end);
            if !alive {
                f.stop(*ev);
            }
            alive
        });
    }
    for (entity, a, tf, player) in &actors {
        if a.anim.is_empty() {
            continue;
        }
        // Several render frames can share one fixed step: handle each step once.
        let step = (a.anim.clone(), a.t);
        if seen.get(&entity) == Some(&step) {
            continue;
        }
        seen.insert(entity, step);
        let d = data_for(&combat, &a);
        let Some(anim) = d.anim(&a.anim) else { continue };
        let mut states: Option<Vec<i64>> = None;
        for e in &anim.events {
            if !matches!(e.kind, 128..=132 | 96 | 112 | 118) {
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
            let started = (e.start > a.prev_t && e.start <= a.t) || (a.prev_t == 0.0 && e.start == 0.0 && a.t > 0.0 && a.t < 0.05);
            if !started {
                continue;
            }
            // Effects with their own sound: SpawnOneShotFFX (96) / SpawnFFX_ByFloor (112) /
            // SpawnFFX_Blade (118) play 's' + the FFX id when the banks have it (e.g. Wolf's third
            // slash FFX 404000 -> s000404000, the General's 3020 FFX 4065 / 4066).
            if matches!(e.kind, 96 | 112 | 118) {
                if let (Some(ffx), Some(f)) = (e.arg_i64("FFXID").filter(|v| *v > 0), fmod.as_mut()) {
                    let key = format!("s{ffx:09}");
                    if f.has(&key) {
                        if e.arg_i64("SlotID").is_some_and(|s| s >= 0) {
                            if let Some(ev) = f.play_tracked(&key, tf.translation) {
                                slotted.push((entity, a.anim.clone(), e.end, ev));
                            }
                        } else {
                            f.play(&key, tf.translation);
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
                'x' => format!("c{:09}", id / 1000 * 1000 + floor_material(tf.translation)),
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
                f.play(&event, tf.translation);
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
                let key = if letter == 'x' { format!("c{:09}", id / 1000 * 1000 + floor_material(tf.translation)) } else { format!("{letter}{id:09}") };
                let Some(file) = pick(&mut index, &key) else { continue };
                file
            };
            commands.spawn((
                AudioPlayer::<PcmSound>(assets.load(format!("sound_pcm/{file}"))),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume.0)).with_spatial(true),
                Transform::from_translation(tf.translation),
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
