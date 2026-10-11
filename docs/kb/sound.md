# sound: Sound: FSB/FMOD decoding, sound events

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: NEXT 9 done: sound. sound/*.fsb are FSB5, codec 15 (Vorbis) with stripped setup headers; FMOD Ex keeps
  them in fmodex64.dll (16-byte {ptr,size,crc} entries; some headers are split codebook/rest blobs). Rather than
  reimplementing that, `sekiro-extract sounds-fmod` drives the game's own fmodex64.dll (FMOD_System_Create /
  CreateSound OPENONLY / GetSubSound / ReadData, NOSOUND output) -> extracted/sound_pcm/<name>.pcm (SPCM header + i16).
  FMOD segfaults on ~140 of 1604 subsounds; progress file + rerun loop skips them (1467 decoded). smain.fsb is
  encrypted (skipped). 2026-10-11: FMOD can also hang (c1080.fsb subsound 0 spun at full CPU for half an hour
  in a clean-folder extraction, no crash, so the rerun loop never came round): `sounds-fmod` now runs a
  watchdog thread that exits the process (code 3) when no subsound has started for 30 s; the progress file
  already names the hung one, so the rerun skips it like a crash. The cause (watchdog message with the FMOD
  call): ReadData never returns, on c1080's 962-byte subsounds and its 30 KB ones alike. The pure-Rust
  `sounds` path says why: c1080 has "2 setup headers found of 6", i.e. most of the bank's Vorbis codebooks
  are not in fmodex64.dll's table, and FMOD spins instead of failing on a stream whose setup header it
  cannot find (c1450 / c5040 / c5060 / c5090 hung the same way; 7 of c1080's 112 decode). Subsounds under
  1024 bytes of PCM are skipped before the read anyway ("N under 1024 bytes skipped" in the bank line), and
  `.hangs_<bank>` counts a bank's hangs: after three the rest of the bank is skipped (progress set past its
  end), so a full extraction loses those banks' sounds but finishes. gap: where the game gets those
  codebooks (another dll, or the exe). The repo's own sound_pcm was decoded from c1010 / c1020 / main only
  (their `.progress_*` files); the packaged extract.ps1 decodes every bank in extracted/sound. (`sounds` = experimental pure-Rust Ogg remux, works only for full-header CRCs.)
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
- 2026-10-09 mix from the exe: SoundMan (NS_SPRJ, DAT_143d6ce08) volume slots Master, InGameMaster, Bgm, Se,
  Voice, Menu, OtherMenu (+0x148 + i*0xc: cur/target/remaining). Bgm/Se/Voice = GameDataMan options (+0x50)
  bytes 4/5/6 / 10, defaults 10/10/10 (FUN_1407bc470). FMOD categories (master > music, Menu, SE, Voice,
  Default) get slot * menu * InGameMaster (command 6, FUN_141c7b1d0; the table at 0x14092d489:
  Default / Menu / SE <- slot 3 Se, Music <- slot 2 Bgm, Voice <- slot 4 Voice; Menu multiplies by
  slot 5, the rest by slot 6; built as Options sliders in fmod.rs set_mix 2026-10-11); the master category gets Master * 0.7
  (command 10, FUN_141c7b760). fmod.rs sets master 0.7 (was 1.0, ~3 dB too loud). Event parameters of the
  common events are only the automatic "(distance)" (0-29 m swings, 60 m deflect clang).
  With FMOD running, events its projects lack no longer fall back to raw PCM samples (the game is silent there).
  gap: sounds play at the actor root, not the TAE event's DummyPoly; no map reverb (setReverbProperties).

## Sound placement and reverb (2026-10-11, plan B4)
- Placement (sound.rs `place`): PlaySound_BySlot 129 / 131 and _ByDummyPoly_PlayerVoice 130 play at
  their DummyPolyID, an FFX's sound (96) at the FFX's dummy, SpawnFFX_Blade's (118) at its
  DummyPolyBladeBaseID (arg names: DSAnimStudio TAE.Template.SDT); CenterBody 128 / Weapon 132 at the
  character. Unmodelled weapon / tool dummies fall back as the effects do (ffx::dummy_candidates);
  floor sounds ('x') take the floor material under the dummy's x / z. SpawnFFX_ByFloor (112) no
  longer plays 's' + FFXID (its FFXID is a FootSfxParam column).
- Reverb, traced but not built: the space type 0-8 (sound_space_type_no_reverb, small / middle /
  large / extralarge _reverb_a / _b; FUN_140c19810 names) eases to its target over a time
  (FUN_14092fc30: +0x1f4 target char, +0x1f8 current, +0x1fc time left) and goes to the sound
  thread as the parameter "sd_reverb_size" (FUN_1409312b0 -> SMOM command 0x14 by name,
  FUN_141c7b610); mixer.fev holds the "SFX Reverb" DSP with that parameter. gap: where the type
  comes from (the MSB collision part's SoundSpaceType is the likely source: needs the map export,
  Map) and how the sound thread applies the parameter to mixer.fev are not traced; nothing is
  reverbed here.
- mixer.fev (6.9 KB, with multich.mch / multimix.mix / multirpc.rpc) is a loose file in the game's
  sound/ folder, not in the archives: extract.ps1 copies it (a clean install without it logs
  "mixer.fev not loaded (error 23)" and plays dry; found by the 2026-10-11 clean-folder run).
- Reverb, built (2026-10-11): mixer.fev's events (ntc_rev, ntc_main, ntc_menu_rev, ntc_through,
  ntc_enemy_rev, ntc_battle_bgm, ntc_boss_bgm: parameter sd_reverb_size, ntc_rev's range 0-8, the
  others 0-1; ntc_ninsatsu1-3 their own 0-1 parameter; test fmod_mixer). Starting ntc_rev adds the
  chain FMOD Echo -> Lowpass Simple -> SFX Reverb under ChannelGroup:Default (test fmod_mixer_dsp
  prints the DSP tree). fmod.rs `set_reverb` starts it and sets sd_reverb_size = the arena's
  SoundSpaceType: MSB collision part type data +0x01 (SoulsFormats MSBS Collision), exported as map
  json "sound_space_type" of the floor's collision part (msb.rs / map.rs), read into
  MapLighting.sound_space. Values: most arenas 0; map_boss_50601010 / 50900000 (m10 h016200) 3,
  map_boss_51000100 (m17 h005500) 5; per map, m17 has 0 / 2-6 (caves), m25 0-2. No map: 0.
  `SHINOBI_REVERB=<0-8>` overrides. Easing (done, `fmod::ParamEase`): the first type is set at once,
  a change goes in a straight line over 2.6 s (FUN_14092eb30 target, FUN_14092fc30 per frame, fade
  time +0x200 = 2.6 from the constructor FUN_14092cec0). Gaps: which other mixer events the game runs
  (ntc_main, battle / boss bgm); the space type changes only per
  arena here (the game's follows the collision under the player).
- The exe's global sound parameter slots (FUN_14092cec0 -> FUN_141c7ad30(name, slot)): 0
  sd_reverb_size, 1 sd_dimension, 2 room, 3 deep, 4-6 the table
  at 0x143b139b8 (ntc_ninsatsu2, ...: the deathblow mixes; FUN_140ed78c0 reads slot i's value at
  +0x10 + i * 0x14).
- **Deathblow mix (done 2026-10-11, `sound::DeathblowMix`, `Fmod::set_deathblow_mix`).** Wolf's
  TAE 970 ("OldSoundDebug", 188 deathblow anims a200 / a201_51xxxx, from 0 to the anim's end)
  asks for it each frame (dispatcher FUN_140b59170 -> FUN_140b55260, player only ->
  FUN_140ed78f0). Args: a type byte, then (fade-in, fade-out) for slots 0 / 1 / 2. Type 1 -> slot
  2, 2 -> 0, 3 -> 1, 0 -> slot 0 (slot 1 when GameMan +0x11fc is 3; gap, looks online-only).
  The asked slot eases to 1 over its fade-in, the others to 0 over their fade-out
  (FUN_140ed7980). Slots = ntc_ninsatsu2 / 1 / 3, each a lowpass on ChannelGroup:Default at 1:
  445 / 1332 / 10 Hz (test fmod_ninsatsu_mix). Typical deathblow: type 0, in 0.3 s, out 2 s, so a
  strong muffle that fades back in 2 s after the anim. a200_510900 type 1, a200_516510 type 3.
- **room / deep / sd_dimension: unused (2026-10-11).** `room` gets the same space type as the
  reverb (both from the player's chr +0x1acc or FUN_140ed1980, 140a7.c ~3937; eased the same way,
  sound manager +0x204..+0x210). But no event of main, mixer, m11, sm11, xm11, vm11, c1020 has a
  room / deep / sd_dimension parameter (test fmod_global_params): the only event parameters are
  "(distance)", "Shahei" (遮蔽, occlusion; 116 main events, most c-events) and the mixer's. So
  nothing to port.
- **Shahei (occlusion), not ported.** Per sound instance (141ca.c): on start, events with a Shahei
  parameter set it to 0 and read the event property "UpdateDefilade" (FUN_141ca13c0: keep
  updating); a query (FUN_141c9a160 on the instance's +0x278 job, likely a ray from the listener)
  gives the target (+0x28c); FUN_141ca1660(dt step) moves the value (+0x290) toward it by at most
  the step, FUN_141ca1300 sets it. gap: the query (rays, what counts as blocked), the step, and
  what each event's Shahei curve does (volume / lowpass, in the .fev). Our sounds are PCM through
  Bevy, so it would need those curves too. Low value for open boss arenas.

## Boss music (built 2026-10-11)
- Banks sm10..sm25 (.fev / .itl in the archives, .fsb loose in the install's sound folder, the
  .fsb FMOD-encrypted: bit-reversed bytes XOR a key the exe holds as UTF-16; tools/bgm_decrypt.py
  finds it in the player's exe). Events m<id>, path "sm11/sm11/m110000001", one parameter
  "(distance)" 0-10 m.
- Start: map EMEVD InitializeCommonEvent 20005830 [defeat flag, arena region, start flag, BGM
  region, stop flag] / 20005831 [defeat, arena, start, BGM 1, BGM 2, phase flag, end flag];
  BGM region = MSB Sound region (PointParam type 4) SoundType 6, SoundID. 2010[8] (region, 1) on,
  (region 2, 257) the phase switch, (-1, 256) all off. common 410 does the same for the areas'
  battle / stealth tracks (not built).
- tools/boss_bgm.py -> extracted/enemies/boss_bgm.json; fmod.rs boss_music.
