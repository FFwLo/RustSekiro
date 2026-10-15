# Sv1 handoff: current state (slimmed 2026-10-15)

Start with `CLAUDE.md` (rules, commands, file map), then `docs/PLAN.md` / `docs/PROGRESS.md` for
what is open, then only the `docs/kb/<topic>.md` for the topic you work on.

Where the old sections went (nothing was deleted):
- Section 4.x (what each session changed and why, 4.1-4.63) and the 2026-10-10 bug list:
  `docs/history/2026-10.md`. Grep it; do not read it whole.
- Skills plan and progress (old section 8): `docs/kb/skills.md`.
- FXR effects player (old section 9): `docs/kb/fxr.md`.
- Other "section 4.x" references below point into the history file.


## 0. Session of 2026-10-09 / 10 in one page
Everything below is in the code with citations; the kb files have the details.
- **Stealth** (`src/stealth.rs`, kb/enemy.md): the exe's SprjTargetingSystem (sight cones, meter,
  caution / find / battle, forgetting), the AI logic scripts as the planner, crouch (C), HUD bar.
- **Root-motion yaw sign** (kb/damage.md): `export.rs` writes `-yaw` (X is mirrored, so turns are
  too). Verified against the live recording: the c1010 behind deathblow matches tick for tick; the
  c1020 360-degree spin after plunge / kick-down deathblows is gone.
- **Gore** (`src/gore.rs`, kb/visuals.md): blood sprays from the TAE's blood FFX events on their
  dummies, floor stains from TAE 138 DecalParam rows with the game's decal textures.
- **Clash effects + bloom** (`src/vfx.rs`), **Sekiro-style HUD** (`src/hud.rs`: vitality, posture,
  resurrection nodes, enemy bars, debug text only with F1), **deathblow mark** (the game's FE sprite).
- **Fixes from the user's list:** crouch moves at once (CrouchStart is a standby state), the idle
  twist follows only the lock-on target (Wolf's chest swung at an enemy behind him), the sprint
  deflect faces the target and its 3 m slide stops against the enemy's body.
- **Skills (section 8):** every tool / art / skill exported with official names; prosthetic
  switching (Z), F1 SKILLS menu, the prosthetic core and the Firecracker work.
- **Cape** (section 7 item 2): every cloth value checked against the game files and exe; still
  moves too much. Needs a close real-game clip.
- **After push 60c08dd (committed and released as v0.2.0 on 2026-10-11):** the game's own effects (FXR player `src/fxr.rs`,
  section 9), Mortal Draw's sound no longer loops forever (slotted FFX sounds stop with their
  event), the Mortal Blade model + its effect dummies (TAE 715 WeaponModelType 2 / Model2).
- **The game's own map** (section 4.8, kb/map.md): the Outskirts gate around the General from
  the MSB, map pieces, hit collision and draw params; `config.toml [world] map`.

## 0.1 The bosses belong to an outside contributor (2026-10-12)
**Changed 2026-10-13 (docs/PLAN.md "Update 2026-10-13"): the 22 main-boss rows are ours again; the
outside contributor keeps the mini-bosses and generals (35 rows). The text below is the
2026-10-12 state.**
The user: "exclude these bosses name list From the work because i gave it to someone", pointing at
`extracted/enemies/npc_names.json` "rows": all 57 boss and mini-boss NpcParam rows (generals,
mini-bosses and every main boss). The Claude sessions no longer work on those fights: their
scripts, phases, arenas, per-fight lighting, deathblows, clip checks and boss-only effects.
They keep the regular enemies, even when one shares a chr with a boss (c1020, c1010, c1040,
c1150), and the shared engine (combat.rs, the AI runner, the script interpreter, the renderer
and the exporter). Code already written for those fights stays as it is. The contributor's
per-boss status sheet: `docs/reports/bosses-handover.md`. Sessions renamed the same day:
"bug fixes" (was Map), "VFX and Maps" (was VXf), "Bosees" (was Bosses).

## 1. The project
- **Sv1** is a 1:1 recreation of Sekiro's combat in Rust and Bevy 0.19, at
  `C:\DeadlockModding\shinobi-combat`. The crate and exe are both named `sv1`.
- **The data is the source of truth:** the user's own Sekiro install (TAE events, HKS scripts,
  params, Havok behaviour graphs and cloth, FLVER models, FMOD sound projects), plus a full
  static Ghidra decompile of the exe (`extracted/decomp/*.c`).
- **Enemies:** c1010 is the Ochimusha (the spear/naginata enemy) and c1020 is the Samurai
  General. `config.toml` sets `[enemy] chr` and `npc_row`.
- **`config.toml` is the user's own file: never commit it** (it shows as modified in git). Their
  setting on 2026-10-10: `chr = "c1020"`, `npc_row = 10203010`. To trace another enemy without
  touching it, run with `SHINOBI_ROOT=<dir>` where `<dir>` holds a copied `config.toml` and a
  junction `extracted` -> the real one (`cmd /c mklink /J <dir>\extracted <repo>\extracted`).
- **GitHub:** `FFwLo/RustSekiro` (public), branch `main`.

## 3. State at handoff
- **Tests:** `cargo test --release` passes 198 tests (33 ignored) on 2026-10-11 (older counts
  below in this file are from their day).
- **New files this session:** `src/stealth.rs`, `src/vfx.rs`, `src/gore.rs`,
  `tools/sekiro-extract/src/fmg.rs`. After `sekiro-extract export extracted` you need these unpacks
  too: `sekiro-extract unpack <Sekiro dir> extracted 'decaltex'` (blood stains) and
  `'menu/hi/01_common'` (HUD sprites: the export cuts them to `extracted/hud/`). Regex note: a
  leading `^/` does not match in Git Bash (path conversion) - leave it out. The map needs the
  "Map" block of `tools/extract.ps1` (MSBs, m11_01_00_00, m11 textures, drawparam, maptex, the
  `bxf` expand of the hit binder, then `sekiro-extract map`). Build the extractor with
  `CARGO_TARGET_DIR=target/map` when another session's `sekiro-extract.exe` locks target/debug.
- **Working tree:** this session's work is committed (see `git log`); only `config.toml` (the
  user's) stays modified. Older complaints from 2026-10-09 (head-kick shake, cloth flicker, sound
  levels, vault) were fixed then and not raised again; the current list is section 7.
- **The user asked for "add all of the stealth system".** Core done 2026-10-10 (see 4.7); the
  rest is in 5.1.

## 5. Open work, in rough priority
- **Live checks wanted** (the real Sekiro running, memscope; the overseer collects them for one
  user session):
  - **guardCutCancelRate (bug fixes, round 75).**
    - Setup: Wolf blocks (no deflect, guard held) one plain physical hit from an ordinary NPC
      whose AtkParam_Npc row has guardCutCancelRate -30 (2017 of 2396 rows; e.g. an Ashina
      soldier's sword swing).
    - Read: Wolf's HP before and after the block. The player ChrIns -> the HP field the existing
      memscope scripts already read.
    - Expect: no change = no leak (our model). About 30 % of the atkPhys gone = the exe applies
      (rate + 100) / 100 to Kusabimaru's 100 % physGuardCutRate.
    - Static side: getters FUN_140a12d60 (NPC) / FUN_140a24ba0 (PC); the caller (a vtable slot)
      is not found yet.
  - **Wolf's deathblow blade blood, TAE 117 row (VFX and Maps, HANDOFF 4.57).**
    - Setup: a front deathblow on an Ashina soldier (c1010, armoured: materialSfx1 114) and on a plain bandit or monk (meat, 109).
    - Read: the FXR ids spawned on Wolf's sword during a200_510000 at about 0.85 s and 1.48 s. That is the sfx slot list at chr+0x1b40 (ChrThrowDirectionSfxSlot: the id at slot+0x24), or the FXR spawn call FUN_14092afe0's id argument.
    - Decides: 229501 / 220503 on the armoured soldier and 220501 / 220503 on the plain one means our row = the target's materialSfx1 is right. A different id means the row comes from somewhere else, so note which row (ThrowDirectionSfxParam) it belongs to.
  - **Thin boss cloth swing (VFX and Maps, HANDOFF 4.55).**
    - Setup: Way of Tomoe (hair02), Owl (the haori_new tail) and Isshin Ashina (beard). Fight them and film close with the camera near during their dash and leap attacks (the 3000-series), or pause on frames.
    - Read: the angle of those pieces from vertical at the peak of a leap or dash. A clip at 60 fps close-up is enough; the cloth particle positions in the hkx sim buffer are a bonus.
    - Decides: if they pass horizontal (over 90 deg, as in Sv1: up to 160-170 deg), our solver matches. If they stay within about 60-80 deg, something damps them in the exe beyond globalDampingPerSecond (look at the cloth's maxVelocity / the sim's collidable inertia).
  - **Deathblow blood look, FXR 220505 (VFX and Maps, HANDOFF 4.57).**
    - Setup: a front deathblow on an Ashina soldier in daylight, with the camera close. Take a still at about 0.3-1.0 s after the stab.
    - Read: the spray's colour (bright red vs dark brown-red), its size on screen (compact jet vs strands reaching ~2 m up) and its shape. Bonus: the particle colour after lighting, if a frame capture tool is at hand.
    - Decides two things. Our lit particles are drawn unlit at rgbMultiplier / 8 (fxr.rs particle_color); bright red in the game means they need the real light at the particle. Our 603 orientation-7 sprites stand along the node's Y axis; a compact jet means mode 7's axis is not the node's Y.
  - **Lit particle light scale and billboard constants (VFX and Maps, HANDOFF 4.58).**
    - Setup: any fight with a deathblow (blood 220505) in daylight. Attach memscope during the FFX draw.
    - Read:
      - On a lit particle draw: PSCommonBuffer cb0 offset 752 (g_ps_envCbColScale), 208 (g_vLightMask.x), 180 (g_ps_envCbIndex), 556 (g_ps_preExposure) and 560 (billboard shadow intensity).
      - The floats at 0x143f083c0, 0x143f085d0 and 0x143f08390 (the billboard builder's constant axes).
      - For one mode-7 particle of 220505, the vector at pfVar10[2..4] in FUN_140440a60, and how it compares with the particle's velocity.
    - Decides: the lit scale that `SHINOBI_FX_LIT` needs (then it goes on by default), and whether mode 7's axis is the particle direction (then `SHINOBI_FX_ORIENT` goes on).
  - **The Monk's Todome length (Bosees, HANDOFF 4.57).**
    - Setup: Corrupted Monk, Mibu (m25), her third deathblow (the Todome with the Mortal Blade).
    - Read: how long she stays in a100_013700 before a100_020200. That is her ChrIns anim id, or count frames on video.
    - Decides: we cut at 1.25 s (1.33 s before 2026-10-14), so her own blood 220606 at 4.5 s and the stains never play. If the game holds it past 4.5 s, the 20200 request in the script path is early.
    - Update (Bosees 2026-10-14): the all-bosses clip already fits the 1.25 s cut (4.57); this check is now only a confirmation.
    - Also read (memscope, Bosees 2026-10-13): Wolf's ChrIns -> +0x50 -> +0x50 -> +0x508, the event message EMEVD 4[8] reads, during a211_511700. Expect 10 latched from his earlier deathblow (a211_510000 TAE 231 = 10) and no new write in 511700 (it has no TAE 231). If it is 10 from the stab's first frame, the game cuts her at the same point we do and nothing is wrong; if it is 0 / other, the message is cleared per throw and the 20200 waits for something else.
  - **An ending FFX's sound (bug fixes, round 84).**
    - Setup: equip the Flame Vent (Spring-Flame or Lazulite, so the a072_400400 loop runs) and hold its use for 3-4 s next to a wall, with the music low.
    - Read: by ear, whether the flame "whoosh" (sound events s000300161 / s000300261, FFX 300161 / 300261, each sample ~1 s+) restarts cleanly on every 1.13 s loop or piles up into a growing roar. Bonus (memscope): the SoundIns state byte at +0x12 of the FFX's sound host entries (GXFfxSoundHost at ffx+0xb8, 14040.c FUN_140406010) going to 5 when the effect ends.
    - Decides: clean restarts = our new stop (sound.rs, slot -1 tracking) is right. A pile-up means the game lets them play out; revert to the untracked f.play for unslotted FFX sounds.
  - **The air-velocity blend's clock (bug fixes, round 81, low priority).**
    - Setup: Sakura Dance or Mist Raven (TAE 922 launch) hitting an enemy right at the start of the hop, so a hit stop lands while the hop's blend is easing in. Film it at 60 fps or step frames.
    - Read: whether the hop's height and arc are the same with and without the hit (hitting vs whiffing the same art from the same spot).
    - Decides: same arc = the blend runs on the character's hit-stop clock (our round 81 choice, player.rs `vs.elapsed += a.clock(dt)`). A visibly shorter or flatter arc on a hit = it runs on real time; switch back to the raw dt.
  - **ThrowParam throwFollowingType (bug fixes, round 94).**
    - Setup: two front deathblows that both use atk dummy 266, one on a row with throwFollowingType 1 (most front rows, e.g. the Ashina General's) and one with 0. Then a deflect-break (246) or a Mikiri (523), all type 1.
    - Read: the distance between Wolf's and the enemy's root positions every frame through the throw (the ChrIns positions memscope already reads). The existing record (player.rs ~1112) shows a type-1 front throw at 1.50 -> 0.85 -> 2.35 m after the snap.
    - Decides: if type 1 keeps the pair locked to the dummy every frame (no drift) while type 0 drifts, follow_throw must re-snap each frame for type 1; today it snaps once and lets go for every type. The same drift on both = the field means something else (note what).
- **VFX and Maps open items (2026-10-12, after 4.44):**
  - **PAUSED 2026-10-13 by the user ("stop the work on map tone and lighting for now"):** B4,
    map emissives, sky, GIIV and bloom below wait until the user resumes them.
  - **B4 brightness / warmth vs the real game:** 2026-10-13 compared against the all-bosses clip's
    outskirts-gate frames on the exported `look_m11_00_gate` (4.44 table): 18 h exposure and
    warmth match; the dark end (~2 stops lifted) and saturation (about half) are left, with no
    Sekiro gparam field to tune by. 2026-10-13 traced: the curtain (m11_camp_curtain_*_a) is a
    plain M[AMSN_light]_e (MetallicValue.spx) whose texture is itself brown (mean sRGB 97 / 79 /
    57, sat 0.41), and the game's combine GXLightCmbAlbedo is `irradiance x GBuffer2` with no
    other term (gxrenderershader; GBuffer2 = sat(albedo x vertex colour) x DifMapMulCol x
    InstanceData.aColor, kb/map.md 645), so its vivid orange in the real frame is light and
    post, not the material: B4 waits for a real frame at a known hour (ask the user).
  - **Map emissives (new, 2026-10-13):** no map material emits light in Sv1. The gate's 60 m
    (MAP_LOG scan) has ~110k triangles of emissive MTDs: M[Multiple]_Emissive (m11_00
    Emissive_wood1 / roof2 / roof_thatch1_e / strawsheaf1: charcoal embers, color_3 / color_9
    orange x 3-7, painted by the vertex blend weights TEXCOORD5 z / w against height masks,
    scrolling ember masks; GXFT_SATStaticSwitch 33, shaderbdle `M[Multiple]_Emissive_33_Gbuf`)
    and M[Emissive]_Simple (Emissive_simple1, EmissiveCloth_simple: decoded, kb/map.md "Map
    emissives"). The game writes them to the fifth GBuffer target (o4, x SC_PreExposure).
    a port is the M[Multiple] game path plus the emissive layer. 2026-10-13 checked (scratch
    tools scratchpad/burnt/msb.py, ev.py): the burnt (x9xx) and intact (x8xx) castle pieces
    share positions and differ only in the part's EntityGroupIDs (entity data +0x1C, 8 x i32;
    msb.rs doesn't read them). m11_01 event 50 calls common events 20004110 / 4112 / 4114
    (activate map part / object / hit group) per group with flags f1-f4 and states s1-s4: the
    group is hidden while any non-zero fi == si. Flags: 8306 prologue, 8301 invasion, 8302 the
    burnt castle (event 911 after 9310). First visit (all off) shows 1117200 / 1201, 1116200 /
    1201, 1114820, 1114330 and hides 1117202-4, 1117210, 1116202-4, 1114310 / 11, 1114821,
    1114400, 1114501: the burnt and invaded sets. So the embers don't show in our arenas'
    state (skip them), and the exporter draws both sets today: fix = read EntityGroupIDs and
    drop parts in hidden groups (parse the map's emevd calls; only m11_01 checked).
  - **Sky 18 h look:** the game's SkyDome_Ver2 shader decoded and matched 2026-10-13 (4.44);
    cloud drift and the fog on the sky ported the same day; left: the glow target.
  - **GIIV, the rest:** several boxes done 2026-10-13 (4.44, kb/map.md "GIIV boxes": the
    game's box shader, MSB fade / priority, up to 16 cropped boxes per arena); left: the
    burning-hall box's event switch, FadeRate / DiffuseColorScale; characters
    (sekiro_material.wgsl, not this track's file) still take the cube's diffuse.
  - **Fog walls on the other maps: done 2026-10-13:** only boss_50900000 (m10)
    had a wall box baked into its `.hit`; o001400 / o001402 were never unpacked (draw groups
    skipped them first), so those walls had no collision at all. The exporter now takes every
    fog-wall object's hkx box into the gates before the draw-group check (and never draws its
    mesh); m17, m15, m11_02, m25, m10, m11_01 re-exported with it (gates: m17 3 walls, m25 2,
    m15 / m11_02 / m10 1 each, the castle roof its box + h900800). Map and fog-wall tests pass
    (14, incl. every_arena_start_stands_and_walks). Disk: C: had 12 GB free.
  - **Bloom:** could take a little more glow on sky / snow by eye; not tuned (no source value).
- **Armoured enemies: done 2026-10-12 (Bosses).** The c1180 servants' EMEVD armour part
  (common_func 20005410) -> tools/armor_parts.py -> enemy.rs Armor: x0.35 HP / x0.5 posture,
  broken by the Loaded Spear (kb/damage.md); the break anim's TAE 233 hides the armour mesh.
  Gap: every hit counts as on the armour (live 7 of 8 were cut; per-part groups exist in the
  hurtboxes, PelvisArmor_00-02 = group 2, but limiting the cut to them is not backed by a trace).
0. **Map, the rest** (kb/map.md "Open"): check stairs / walls / low ceilings and the spawn
   height in play, compare the gate lighting with the real game (light set 100 at noon; sun
   and ambient scales are by eye), far view (exporter `MAP_FAR`, 4.17; unused on the gate), the
   layer blends use the base `_3m` mask as height (soft knob `SHINOBI_MAP_SOFT`); byte 1 of the
   blend data is unused; frame rate is settled at 72-80 fps (kb/map.md).
1. **Stealth, the rest** (core done, 4.7): crouch attacks / steps / reactions (HKS Crouch*
   states); grass / shadow hiding (109201 no setter; MapVisibility Dark not exported, none on the
   gate); wall hug, hanging (limit types); patrol routes done for config `msb` enemies (4.22;
   stepping traced and ported 2026-10-12, type 5 routes end; their map AI hold done 2026-10-12); the real HUD art; the
   user's verdict. (The listener's ear params were done 2026-10-11, kb/enemy.md.)
2. **Confirm with the user** (ask for clips if still wrong):
   - head-kick camera feel with the mouse fix;
   - cloth on jumps;
   - sound volume;
   - vault.
3. **Kick-launch slide: done 2026-10-11.** It was the clip's own root (213111's hkx: 0.27 m
   forward, back to 0 at frames 17-20). Live, 30 head kicks (rec_* recordings) never show the
   step back: actor.rs advance now skips horizontal root motion while airborne unless flag 27
   SetNoGravity holds. Test `the_head_kick_relaunch_goes_straight_up`. Also actor.rs separate
   skips pairs whose heights don't meet (NpcParam hitHeight). The blend-in weighting of the
   root motion (live 0.040 m in frames 0-6 vs the clip's 0.27) is PORTED 2026-10-14 from the exe
   (4.63 item 8). The `runkick` throw trace no longer sets up (Wolf reaches the enemy before the
   jump); use the sim test instead.
4. **Sound gaps:** the other mixer events (ntc_main, bgm); the per-event "Shahei" (occlusion) parameter (placement, reverb + its 2.6 s easing, deathblow mix and boss music done; room / deep / sd_dimension are read by no event).
5. ~~**Coat second texture layer**~~ done 2026-10-12 (kb/visuals.md "Coat shader").
   The shader math isn't known; it is in the compiled SPX shaders.
6. **Camera gaps:** lockCamAdjustRot manual offset (no reader found); the obstacle fade of
   enemies between the camera and Wolf (one byte unidentified, kb/camera.md). The fulcrum pitch
   resync is done (2026-10-12, camera.rs chases_pitch). The fulcrum yaw resync is done
   (2026-10-11, camera.rs drag_fulcrum / place_fulcrum): the free camera swings round when Wolf
   runs sideways (about 30 deg in 0.5 s, 53 deg in 1 s at 6 m/s). Ask the user how it feels.
7. **Clean-up: done 2026-10-11.** `SHINOBI_CLOTH_FLIP_N` removed; the "ref gap" log only fires
   at the `SHINOBI_CLOTH_DEBUG=<s>` second, not per frame.
8. **c1010 spear cloth** `#04#_c1010_yarinuno`: no restart warning in the 2026-10-11 runs
   (the jump threshold now scales with the cloth, 4.12); closed unless it comes back.

## 6. How to verify things
- **Gameplay logic:** `cargo test --release`; add sim tests in `src/sim_tests.rs`.
- **Geometry vs the real game:** compare `SHINOBI_THROW_TRACE` output with the memscope
  recordings.
  - Recordings: `C:\DeadlockModding\memscope-data\logs\rec_*`. `index.txt` lists the blocks.
  - Slot 0 is Wolf, 1-2 are the nearest enemies.
  - physics +0x80 = position, +0x74 = yaw, +0x84 = y.
  - chr_head +0x68 = NpcParam id, +0x6C = chr id.
  - Readers: `tools/rec_read.py`, `rec_throw.py`, `rec_timeline.py`.
  - Live vaults: `rec_c1020_b_20261009` tick 13131 (clean) and `rec_c1010_20261009` tick 3600
    (slot swap, messy).
  - Live jump physics: `docs/kb/movement.md` (vertical jump 1.98 m, 9.4-9.8 m/s, gravity 23.52).
- **Visuals:** throw-trace screenshots tiled with ffmpeg (`tile=6x5`), or `tools/photo.ps1`.
- **Exe research:**
  - `tools/fn.sh <addr>` prints a function;
  - `extracted/rtti_vtables.txt` maps classes to vtables;
  - read exe bytes from `extracted/sekiro_steamless_dearxan.exe` (map VA via the PE sections);
  - capstone is installed for Python. Don't name a script `dis.py`: it shadows the stdlib.


## 5. Live checks wanted (HUD, R97)
- Over-head bars on regular enemies. Setup: an Ashina soldier, alert it, hit it once, step away. Read: when the bars appear, how long they stay, any fade, the offset over the head. Decides: update_enemy_hud show rule.
- Found-mark hold/fade and the posture "<" ">" arrows. Setup: be spotted by a soldier; damage a boss's posture. Read: the times; the arrow placement. Decides: update_stealth, the boss posture art.
- The Dead screen. Setup: die once. Read: the fade and the 死 timing. Decides: the menu Dead screen (2.5 s fade data).
- R97e: the over-head bars now follow the all-bosses clip (Folding Screen Monkeys, 30:59): HP above posture, both 6.8 vw wide, 5 / 4 px at 1080p, 2 px gap. The posture "< >" arrows show at the bar ends while posture > 0 on both bars (Butterfly 15:52). They are not drawn yet, so that is the next HUD item. The clips.md chapter starts from ch.9 run about +1:00-1:15 late, back near 0 by Shirafuji (32:20).
- Live checks still wanted: when the over-head bars hide and whether they fade; the found-mark hold; the Dead screen (no death in either clip).
- Suite: 544 passed, 1 failed (the_corrupted_monks_last_deathblow_is_her_todome, Bosees' TAE_EVENT_MESSAGE WIP, acknowledged by Bosees).
- R97f: the posture "< >" arrows (MENU_Taikan_Arrow, the left one flipped) sit at both posture bars' ends. The boss's show while posture > 0; Wolf's go with his bar. Fixed the sprite bars drawing as hairlines: Bevy's NodeImageMode::Auto kept the texture's aspect ratio, so every bar image is now Stretch. The boss name is 15 px, and the gourd counter moved clear of the icons. Checked with SHINOBI_HUD_SHOTS (c7100).
- **STOPPED 2026-10-14 by the user (via the overseer) after R97f.** The tree compiles. Suite: 544 passed / 1 failed (the Monk todome test, Bosees' WIP). The clips.md fight moments were added (the chapter starts are not shifted: the chapters include the walk-ins and cutscenes).
  Next, when resumed:
  (1) The Dead screen timing from the Medal recordings (C:/Medal/Clips/Screen Recording, 20261007 / 20261010; the search was stopped unfinished). Then the menu Dead screen: the 2.5 s fade + MENU_Death_02.
  (2) The sound gaps: the mixer event parameters besides sd_reverb_size, the reverb's 2.6 s easing, and a Shahei default for open arenas (the re-scout was stopped before reporting).
  (3) The camera gaps (section 4 item 6).
  (4) HUD leftovers: the emblem counter art, the right-middle 0 counter, the posture state 3 / ST3 glow, and the text backing on bright ground.
