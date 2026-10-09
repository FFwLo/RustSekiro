# sound: Sound: FSB/FMOD decoding, sound events

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: NEXT 9 done: sound. sound/*.fsb are FSB5, codec 15 (Vorbis) with stripped setup headers; FMOD Ex keeps
  them in fmodex64.dll (16-byte {ptr,size,crc} entries; some headers are split codebook/rest blobs). Rather than
  reimplementing that, `sekiro-extract sounds-fmod` drives the game's own fmodex64.dll (FMOD_System_Create /
  CreateSound OPENONLY / GetSubSound / ReadData, NOSOUND output) -> extracted/sound_pcm/<name>.pcm (SPCM header + i16).
  FMOD segfaults on ~140 of 1604 subsounds; progress file + rerun loop skips them (1467 decoded). smain.fsb is
  encrypted (skipped). (`sounds` = experimental pure-Rust Ogg remux, works only for full-header CRCs.)
  Game: src/sound.rs = PcmSound asset + rodio Source, plays TAE PlaySound (types 128-132) keyed
  "<letter><SoundID:09>" with random b/c/d variants (deflect c000004011, guard c000006510, swings c000004010...).
  Enemy events resolve 85%, player 'c' 50% (rest in encrypted smain). Gaps: floor 'x' / armour 'b' sounds are
  material-resolved in the .fev event project; hit/clash SE (HitEffectSe*Param) not wired; no 3D panning.
- 2026-10-06: polish: crossfades use each anim's TAE "Blend" event (type 16 at frame 0; player 67/102 anims, mostly
  3-6 frames; c1020 291/364, 1-15 frames) instead of a uniform 0.12 s. Positional sound (SpatialListener on the
  camera, sounds at the emitting actor). Flesh-hit SE s000003010 (HitEffectSeParam Body_* 3010001) on hits.
  Footstep/armour (x/b) sounds still need the engine's floor-material mapping.
- 2026-10-06: floor sounds: main.fev defines c000001000-c0000010xx (and 2000/3000 families) = TAE 'x' id rounded to
  1000 + floor material; arena uses material 4 (hard per HitMtrlParam, shortest/brightest set). Armour 'b' still off.
- 2026-10-06: armour sounds: TAE 'b' = armour-material events (FEV has c0000xx113 = Wolf's protector defenseMaterial
  113, c0000xx108 = c1020 materialSe). FEV legacy (LGCY) STRR string table parsed (event names, sounddefs at string
  index 1879+ = ordinal), event->sounddef link not decoded; inferred sets: cloth robe (body-lobe-N) for Wolf, plate
  (body-armor-N) for the General.
- 2026-10-07 REAL FMOD EVENTS (S4 mostly solved without parsing .fev): the game's fmod_event64.dll (FMOD Ex 4.4 Event
  API; load fmodex64.dll first) loads extracted/sound/main.fev, smain.fev and cXXXX.fev + banks. Event path =
  "project/group/event" with the TAE key as the event ("main/main/c000004010", "c1020/c1020/c102004001");
  `sekiro-extract fev-list <sound dir> <fmod_event64.dll> <x.fev>` lists them (main 1875, smain 50, c1020 65).
  src/fmod.rs (FmodPlugin, NonSend resource; SEKIRO_DIR env overrides the Steam path): EventSystem_Init 128 ch,
  FMOD_INIT_3D_RIGHTHANDED, listener = camera each PostUpdate. sound.rs plays TAE sounds and SoundQueue keys as
  events (x = id/1000*1000 + floor material 4, b = id + defense material 113 Wolf / 108 NPC), PCM samples only as
  fallback. Startup probe found 7/7 key events. Open: HitEffectSeJustGuardParam rows hold 'z' event ids
  (z199999980, z999999960...) - row/column selection (atkMaterial_forSe, atkPow_forSe, attribute x defender
  material) not decoded; deflect anims already fire their own clash events.

## Hit / guard / deflect / effect sounds, gated events (2026-10-08)

- TAE PlaySound StateInfo gates are SpEffect stateInfos: 905 Shuriken LV1 (127000, the prosthetic's
  resident SpEffect - plays), 62 flame enchantment, 358 / 940 wind enchantments, 64 sun/moon sword,
  995 sword-saint iai, 996 / 999 money throw. sound.rs plays a gated event when an active SpEffect
  (TAE or equipment resident) has that stateInfo.
- 12 TAE sound ids are in no sound project (main, smain, c1020, c1010, map m/sm/xm/vm 10-11): unused.
- Deflect / block clang (solved statically): the guard paths use the ATTACK's defSeMaterial1/2
  as rows (the General's sword attacks: 100 / 139), group = atkMaterial_forSe, as for hits. Deflect:
  HitEffectSeJustGuardParam first (row 100 -> z199999980), HitEffectSeParam if empty; block:
  HitEffectSeParam (row 100 Iron_Slash_S -> z200000101). Both events exist. sound.rs guard_sounds.
- Effect sounds: an FFX spawned by TAE (SpawnOneShotFFX / ByFloor / Blade) plays 's' + its id when
  the banks have it (s000404000 Wolf's third slash, s000004065/66 the General's 3020).
- Hit sounds (exe FUN_14092ecb0 / FUN_1410c0cb0 / FUN_1410c0ea0): HitEffectSeParam row = each of
  the defender's two materials (NpcParam materialSe1/2, protector defenseMaterial1/2), group =
  the attack's atkMaterial_forSe (paramdef order Iron, Fire, Wood, Body, ...), index = pow + type*3
  over Slash/Blow/Thrust; the value is a sound of type 12 = 'z'. Wolf's sword on the General:
  z100000103 + z000000108. sound.rs hit_sounds.
- FMOD init flag fix: 0x2 = 3D right-handed; 0x4 (SOFTWARE_DISABLE) had made every event fail (error 16).
