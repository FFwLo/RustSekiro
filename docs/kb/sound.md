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

## Mixer routing (decoded and ported 2026-10-12)
- Each event instance names its mixers in user properties "mixerNum" / "mixerID_<k>" (read on
  start, 141c9.c ~11836: a "%s_%x_DummyGroup" channel group under master, its DSP head cut off and
  fed as input to mixer k's DSP, FUN_141ca2ff0 (+0x990 table) -> FUN_141cab500). Test
  `fmod_event_mixers` (ignored) lists them per project.
- multimix.mix ("MFOM", 1156 bytes): 10 mixers; a table of (u16 id, u16 1, u32 offset) at 0x1c;
  each entry u16 output mixer (0 = master), u16 8 channels, an 8 x 8 matrix (identity, 1.0); the
  names by id after 0x33c: 1 ntc_ninsatsu2, 2 ntc_menu_rev, 3 ntc_through, 4 ntc_main, 5
  ntc_enemy_rev, 6 ntc_ninsatsu1, 7 ntc_ninsatsu3, 8 ntc_rev, 9 ntc_battle_bgm, 10 ntc_boss_bgm.
  Outputs: 1 -> 6 -> 7 -> 8 -> master; 4 -> 8; 9 -> 6; 10 -> 7; 2, 3, 5, 8 -> master (the
  "output" reading of the first u16 is inferred: the four end mixers all have 0).
- Which events use which (FMOD_Event_GetProperty): main 1875 events: 902 mixer 4 (ntc_main:
  reverb, no deathblow muffle), 826 mixer 1 (ntc_ninsatsu2: all three deathblow lowpasses, then
  reverb), 95 mixer 5 (enemy_rev: dry to master), 32 mixer 2 (menu, f0000000xx: dry), 8 mixer 3
  (through). Enemies c1010 / c1020: mostly 1, voices (c10x008xxx) 4. Voices vm11: 3897 mixer 4.
  Music sm11: battle tracks (m110000004, m110102025 ...) mixer 9 (ntc_ninsatsu1 -> 3 -> reverb),
  boss / event tracks (m110000001 / 2 / 50, m110100002) mixer 10 (ntc_ninsatsu3 -> reverb), 52
  field / ambience tracks none. m11 map objects mostly 1, 59 with none.
- So the deathblow muffle: type 0 / 2 (slot 0, ntc_ninsatsu2 = mixer 1) muffles the mixer-1 SFX
  only; type 3 (slot 1, ntc_ninsatsu1 = mixer 6) those and the battle music; type 1 (slot 2,
  ntc_ninsatsu3 = mixer 7) those, the battle and the boss music. Voices, menu and enemy_rev
  sounds are never muffled; menu / through / enemy_rev get no reverb.
- Before: fmod.rs started ntc_rev / ntc_ninsatsu1-3 alone; their effect chains hung under
  ChannelGroup:Default with nothing feeding their input end (DSP tree, test fmod_mixer_dsp), so
  the reverb and the deathblow muffle changed nothing (measured: unrouted SFX 0.60 RMS, with the
  muffle at 1 0.71).
- Ported (fmod.rs `mixers` / `build_mixers` / `route`, `read_mix`): all 10 mixer events run;
  each chain's input end is the deepest DSP under its event group (a pass-through channel group
  for a mixer with no effects), its output is cut from Default and fed to its output mixer's
  input or the system master. Every started event's channel group head is cut from its category
  and fed to its mixerID_0's input. Category volumes still apply (they scale the channels, not
  the DSP path). Test `fmod_mixer_routing` (ignored, needs the game's DLLs): routed mixer-1 SFX
  0.78 RMS, with the muffle 0.61, master category at 0 -> 0.02.
- Events with no mixer (traced 2026-10-12, FUN_141c9eef0 at 141c9.c 11489): with the instance's
  mixer list (+0x178..+0x180) empty, its "%s_%x_DummyGroup" goes under the master channel group;
  then the event group's DSP head is cut from its parent (the category) and fed into the
  DummyGroup's head, so the sound reaches master directly. Ours leaves it on its category,
  which feeds master: the same, as FMOD Ex categories carry volume / pitch, no DSP effects.
- Gaps: only mixerID_0 is used (9 main events list more, all -1); reading
  the first u16 of a mix entry as the output mixer is inferred.

## Area battle music (built 2026-10-12, src/area_music.rs)
- common.emevd event 410 (started once by event 0, restarts itself) plays the map's MSB BGM
  regions (2010[8] region, 1, 0) by the engine's map music state, EMEVD condition 12[2]
  (CSEcTestIsMapBgmState, evaluate 0x14065b2b0: byte DAT_143d5d638 + 0x11fc, WorldSoundManImp,
  not GameMan; names 通常 / 発見前 / 警戒 / 戦闘). FUN_1408aa5c0 keeps this frame's maximum request,
  FUN_1408aa950 (from FUN_140a771a0) copies +0x11fd -> +0x11fc. Each non-player chr requests its
  CSChrSoundModule byte (chr modules +0x140, byte +0x10; FUN_140a04760 / FUN_140a05310), set by
  HKS act(3014) (FUN_140b32410 case 0xbc6, c9997.hks AI transitions): 3 in AI state 2 / 3, AI
  state 1 -> 2 with a type-0xc (search) goal else 1, 0 when dead (HP < 1, ...).
- 410 dispatch: state 3 (or flag 9208) -> combat region <area><block>2680 (m11_01: 1112680,
  m110100020 "戦闘_平常時"; raid variants 2685 / 2686 with flags 8301 / 8302); state 1 -> 2688;
  state 2 -> 2689; flag 9209 -> silence (3.0 s fade); else the outdoor field track 2690 while
  flag 9100 is on (outside the 20004010 regions; 1.7 s fade-out indoors). Combat end (label 13): 2010[9] 0.7 s + all off, then
  wait 2.0 s (or combat again / 9208 / 9200 / 9207 / swimming), then up to 1.0 s (or state 1 /
  2), restart. States 1 / 2 swap with no fade. Boss music (9200), mini-boss tracks 9201-9204
  and 9207 take over (3.0 s fade). Swimming: 1.8 s fade out. WorldSoundManImp defaults: fade-in
  2.0 s (+0x11f4), fade-out 0.7 s (+0x11f8).
- Every map numbers its regions the same way (2680 / 2688 / 2689 / 2690); tools/area_bgm.py ->
  extracted/enemies/area_bgm.json (9 maps; m11_00 / m11_02 use m110000004-6). ntc_battle_bgm /
  ntc_boss_bgm (mixer.fev) are not started by 410 and the exe has no string for them.
- Built: area_music.rs `Music::step` (test `event_410_holds_after_combat`), the state = max
  over live enemies of FIND / BATTLE -> 3, CAUTION -> 1; MapLighting.source_map (json "map")
  picks the tracks; off while a boss script runs undefeated. Checked: a c1010 fight on the gate
  starts m110100020 the moment he finds Wolf. Gaps: state 2 (no search-goal flag in our
  targeting), raids, swimming, the exact 0.7 s / 2.0 s fades
  (FMOD's event fade on stop), who writes the AI state / flag 9208, any distance limit on which
  enemies count. Decoded 410 dump: scout scratch (session VXf).
- **Field track (built 2026-10-12, corrected the same day).** "field" (region 2690,
  "BGM_屋外フィールド_平常時") is the outdoor music. 410's default branch (label 8, no state, no
  9209) does GotoIfEventFlag(label 10, OFF, 9100): with 9100 off, label 10 fades all music out
  (2010[9] 1.7 s) and restarts. With it on, it plays the map's 2690 region; 8301 / 8302 raids
  pick 2691 / 2692. Before that, m10 / m11_00 / m17 jump to label 9 (2691) when story flag
  11000695 is on or the player stands in 1102695 / 1702695. common_func 20004010: OR(-2) =
  player (10000) inside region k (a 0 arg skipped) or 9100 on; GotoIfConditionGroup(label 0,
  PASS, -2) (packed 0x00FE0100), else SetEventFlag 9100 ON and wait for "inside any"; label 0
  sets 9100 OFF and waits for "outside every". So 9100 = outside every region. (First read
  inverted: the packed goto was taken as FAIL; bug-fix round 8.) Each map's event 0 passes
  its regions (names 屋内判定 / 屋内外判定, "indoor check"); m11_01 passes 1112611-14, 21,
  26, 30 and 70 (the keep's floors, the hidden-prison hub tower, the Sword Saint's room).
  tools/area_bgm.py writes them into area_bgm.json "indoor" in map space, composites (shape 6,
  8 x (region index, unk)) flattened, boxes standing on their position (DSMapStudio draws the
  region box over y 0..1). area_music.rs turns Wolf's feet back into map space
  (map.rs `arena_to_map`: X un-mirrored, -arena_yaw, + json origin) and tests the regions:
  outside all of them (on a map with a list) is 9100 on. After combat, 410 label 13 holds
  2.0 s (only combat cuts it), then up to 1.0 s more that states 1 / 2 / 3 cut short, so the
  field track resumes 3.0 s after combat.
  Tests: indoor_zones_contain, arena_points_back_to_map_space. Gaps: label 9's 2691 variants;
  m25's 2502890 is not a region in m25_00_00_00.msb.

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
