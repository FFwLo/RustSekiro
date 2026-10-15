# fxr: the game's own effects (FXR player)

Moved out of `docs/HANDOFF.md` section 9 on 2026-10-15. Code: `src/fxr.rs`, `src/ffx.rs`.

## 9. Game effects from the FXR files (2026-10-10; user: "pull them from the game, they are not 1:1")
Every TAE effect (FFXID in TAE 96 / 118 / 120..) whose `f<id>.fxr` exists now plays the game's own
effect, drawn by our FXR player. The hand-made looks in `ffx.rs` are only a fallback for ids with
no FXR.

**Pipeline (run once after `sekiro-extract export`):**
- `sekiro-extract unpack <Sekiro dir> extracted 'sfxbnd_commoneffects'` (2349 FXRs + their TPFs).
- `cd tools/fxr-dump && npm i` (@cccode/fxr, public domain, github.com/EvenTorset/fxr; a clone is
  in `C:/DeadlockModding/reference/fxr`), then `python tools/fxr_extract.py`: every FFX id the
  exported characters use (TAE, bullet sfxId_*, blood 220502/3/5/6) -> `extracted/fxr/<id>.json`,
  following ReferenceNode (2001 "sfx") chains; every texture they name -> `extracted/fxr_tex/<id>.png`
  (s<id:05>.tpf). ~280 effects.
- Effect models (Model appearance 605: Abduction's leaves s04010, the shuriken s08050, ...): per
  model `sekiro-extract model extracted/sfx/sfxbnd_commoneffects.ffxbnd.d/s<id:05>.flver <its
  diffuse's .tpf in the same folder, or -> extracted/fxr_model/model_<id>.bin extracted/tex` (done
  by hand on 2026-10-10 for 4010 4011 4017 4018 4070 4100 4140 4141 8050 8060 8130 8131 8180 11601
  11602; the 8050 / 8130 / 8131 / 4140 / 4141 packs are s08050_a / s08130_a / s04133_a). Drawn
  by `fxr.rs` (sizeX/Y/Z, uniformScale, rotation + angular speed; unlit, blend 0 drawn as normal).
- The Mortal Blade parts: `tools/extract.ps1` now exports `model_c0000_mortal_a_0300.bin`
  (WP_A_0300_2: no meshes, the effect dummies), `model_c0000_mblade_a_0310.bin` and
  `model_c0000_mbsheath_a_0310.bin` (WP_A_0310 + _1). Unpack `wp_a_0310.partsbnd` too.

**Player (`src/fxr.rs`, FxrPlugin):** nodes 2000 / 2200 / 2001 (refs) / 2202 (LOD: first child);
configs 1004 / 1005 (node emitter templates); emitters 399 / 300 / 301; shapes 400-405; spread
500-503; particle movement 55 / 60 / 84 / 105 / 64 / 65; node movement 1 / 34 / 106 / 122
(followFactor: a node detaches from its anchor when it drops to 0); appearances 600 / 602 / 603 /
604 (sprites with OrientationMode 0/1/2/4/6/7), 606 / 10012 (tracers: TracerOrientationMode 0-5,
segmentInterval / segmentDuration / concurrentSegments), 609 (lights: top 8, intensity x4000 =
gap). Properties: Linear / Stepped / Hermite / Bezier keyframes, RandomDelta / RandomFraction /
RandomRange; arguments Constant0 / ParticleAge / EmissionTime / ActiveTime. Blend modes 0/4/7 add,
1 source, 2/6 normal, 3 multiply, 5 subtract. Space: X mirrored (`game_vec`), Euler Z->X->Y with Y/Z
flipped (`game_rot`). A negative particle duration = lives as long as its node. Batched: one
dynamic mesh per (texture, blend). Screen effects (Distortion 607, RadialBlur 608, tracers'
distortionIntensity): `DistortMaterial` + src/fx_distort.wgsl, the game's GXFfxdistortionBump /
GXFfxradialBlur pixel shaders ported (frame read from Bevy's view transmission texture, so they
render in the Transmissive3d phase; one mesh per normal map / mask; a warm-up batch compiles the
pipeline at start and an effect's textures load when it is first read, else a short effect ends
before it shows). MultiTextureBillboardEx (604, nearly every tool effect): `MultiMaterial` +
src/fx_multi.wgsl, the game's GXFfxtessellateBlendMultiTexture ported: three layers with their
own scroll / scale / colour, flipbook frame blending, layer ops by unk_ds3_f2_10..12 (read as the
shader's g_ps_TexBlendType / 2 / 3: 1 / 2 = layers 2-3 / 2 set the alpha by brightness, then 0
multiply, 1 add, 2 overlay). The Flame Vent's fireball went from a flat glow to textured flames.
Camera-distance fade (`view_fade`, every appearance): hidden nearer than minDistance, fading in to
minFadeDistance, the same toward maxDistance, hard cut-offs min/maxDistanceThreshold (fxr docs
603.yml); without it Ichimonji's ground dust (440081, 2.5 / 5 m) filled the screen. Tracers:
segmentSubdivision splits each completed segment (`subdivide`, Catmull-Rom; gap: the game's curve
kind), so a fast iai swing (Ashina Cross 440071: 5) draws arcs, not flat angular sheets.
`SHINOBI_FFX_LOG=1` also logs each game FXR spawned (`fxr <id> on <dummy>`).
Gaps: 607 shape (mode: all 45 are 1 NormalMap, drawn), normal maps on sprites, light intensity scale. Done 2026-10-10: Hermite
keyframes now blend as the exe does (`game_hermite`; property evaluators are a table of 44 ids
`function << 4 | type (| 0x1000 loop)` registered in 1400f by FUN_141d98ff0, float Hermite body
FUN_141d94960: two fixed Hermite curves bent into circle arcs, blended by the earlier keyframe's
t1 / t2 angles, pi/4 = straight), replacing fxr.ts's guess; Bezier was already the exe's curve
(FUN_141d94300). Also done: the distortion's depth test (GXFfxdistortionBump.ppo: where the
scene at the bent spot is not farther than the particle, the straight read is kept; fx_distort.wgsl,
via the same `FxDepth` copy, which runs before the transmissive pass).

**Soft particles (2026-10-10):** billboards (603 / 604) with `depthBlend` set (1102 of 1596) fade
where they meet the scene instead of cutting a hard line through floors and walls, as the game's
soft shaders do (GXFfxtessellateSoftTexture .gpo / .ppo, GXFfxsoftTracer .vpo / .ppo, disassembled
as above). Vertex: h = scale x size / 2, the view depth w moves to w - (h + offset), back toward w
by up to 1 if that is nearer than near + 1; fragment: alpha x saturate(min(scene - own, soft) /
soft), soft = 2 offset + h. `fxr.rs soften` does the pull on the CPU, fx_soft.wgsl (sprites,
an ExtendedMaterial on StandardMaterial) / fx_multi.wgsl (604) the fade. The scene depth is the
camera's depth buffer copied after the opaque pass (`FxDepth`, `copy_fx_depth` in the Core3d
schedule; no measurable frame cost). Not Bevy's DepthPrepass: it draws the 4 M-triangle map twice
(~2.4 ms) and, with MSAA, writes alpha-to-coverage hair / leaves solid, so hair edges showed the
clear colour. `SHINOBI_NO_SOFT=1` turns it off; `SHINOBI_FPS_LOG=1` logs the mean frame time
(vsync off). Gaps: (scale, offset) read as unkDepthBlend1 / unkDepthBlend2, size as the larger
side; trails / lines / point sprites have no depthBlend field and stay hard (what picks
GXFfxsoftTracer is not traced).
WindForce (10300, in 24 FXRs): needs nothing more. Its unk_ds3_f1_31 = 1 (plants only) in all but
400 / 401, and the particle force speed there (732) is ~0.1 in the 8 effects that use it.
- `ffx.rs emit()`: spawns `FxEffect` (Anchor::Entity on the dummy, or Anchor::Blade(300, 301) for a
  weapon dummy that is not loaded); SlotID >= 0 or TAE 118 effects stop when their event ends.
- Weapon dummies 1<model><dummy>: Model0 dummies are kept under the bare id (10301 -> 301),
  Model2's under the full id (12200 = WP_A_0300_2 dummy 200; its 240-280 reach 2-6 m along the
  blade for the upgraded art). `dummy_candidates` tries the full id first.
- `model.rs`: right weapon Model2 follows TAE 715 Model2DummyPolyID (rest: right_2 / rightHang_2 =
  149); WeaponModelType 2 (the Mortal Blade, WP_A_0310 blade on Model0 / scabbard on Model1) is
  shown only while its 715 event places it (only a106 Mortal Draw uses Model2 / type 2).

**Check tools:** `SHINOBI_SKILL=<art or tool id> SHINOBI_SKILL_SHOTS=<dir>
SHINOBI_SKILL_SHOT_RANGE=from,to,step` (frames after the press; logs `shot sNNN: <anim> t <s>`),
`SHINOBI_SKILL_CAM=yaw,pitch` (deg, holds the camera), `SHINOBI_FXR_LOG=1` (quads per texture /
blend each frame), `SHINOBI_FFX_LOG=1`, `SHINOBI_LOG_DUMMIES=1`. One effect on its own:
`SHINOBI_FXR_TEST=<ffx id> SHINOBI_FXR_SHOTS=<dir> SHINOBI_FXR_SHOT_RANGE=from,to,step` (plays it
1.5 m in front of Wolf, sideways, 3 s in; shots xNNN.png, then exits).

**Lit particles (2026-10-10):** 935 appearances have `lighting` -2 (LightingMode Lit-like), drawn
unlit here. Where a node ships a lit config next to an unlit one for another state (same texture),
the unlit rgbMultiplier is the lit one x 0.125 in all 48 such pairs, so lit particles are drawn at
rgbMultiplier / 8 (`particle_color`). Before, they were 8x too bright: the deathblow blood 220505
(rgbMultiplier 15) read as salmon-pink spray; now dark red. Gap: real lighting / shadowDarkness.

**State / next:**
1. Mortal Draw (5700): red aura, ink particles, the Mortal Blade in hand now right. The art plays
   `a106_316100` here (not 316000): its slash arc is 440003 / 440007 at 0.33-0.67 s on 12200.
   Check the arc's place / size / colour against the game (tracer width along the dummy's local Z,
   offsets 0.5-1.5 m, followFactor detach 0.3-0.4 s), then the colours (red reads pinkish).
   The pink is 440000's unlit 604 smoke (11633, color1 [1, 0.25, 0.25] x rgbMultiplier 7.5) and its
   tracer (26030, [1, 0.125, 0.125]). Fixed from the game's shaders: unpack `gxffxshader` (shader/
   gxffxshader.shaderbnd: DXBC .ppo / .vpo) and disassemble with d3dcompiler_47 D3DDisassemble
   (scratch script, ctypes). GXFfxtexture.ppo / GXFfxsoftTracer.ppo take the colour's length and
   direction, square texture x direction and scale by the length, so `fxr.rs hue_squared` draws
   c_i^2 / |c| x sqrt 3 (greys unchanged; the sqrt 3 stands for g_vColorScale, a gap). Mortal Draw
   now reads crimson, the blood red, the Flame Vent orange.
   Praying Strikes (5900 / 7500): the pale curved sheet was our hand-made default weapon trail.
   Now the game's: EquipParamWeapon traceSfxId0 / traceDmyIdHead0 / traceDmyIdTail0 (every sword
   row: FXR 401000 on 300 -> 301), started while an AttackBehavior (TAE 1) window runs and stopped
   when it ends (`ffx.rs` weapon_trail). 401000 is a faint additive glow (alpha 0.14) plus a
   distortion-only tracer (rgbMultiplier 0, distortionIntensity 0.3: drawn as distortion only). Tracers now
   keep their first point where it was laid (a 2-frame window still leaves a segment) and a stopped
   tracer's segments fade out over segmentDuration instead of vanishing (`fxr.rs` Particle.ended).
   The hand-made ribbon is left only for art ribbons and weapons without that FXR.
   The red hit capsules (combat.rs draw_hitboxes) now show only with the H / debug-menu overlay.
2. Then every other art and tool through the shots (list: section 8), fix what looks wrong.
3. Done: prosthetic bullets play their Bullet sfxId_Bullet on the bullet (FxEffect anchored to it;
   isInheritSfxToChild passes it to the children; when the bullet ends the effect stops emitting
   there), sfxId_Hit where they land / are blocked, sfxId_Flick where deflected (`prosthetic.rs`
   spawn_row / fly_bullets). The firecracker burst 710003 is the game's 300071 (smoke + sparks); the
   vfx.rs sparks only stand in when its FXR is not extracted. Gore: the blood FFX 220502/3/5 play
   their FXR (ffx.rs); `gore.rs`'s hand-made spray now runs only when that FXR is missing (it used to
   draw on top); the TAE 138 floor stains are unchanged.
5. Clash effects (deflect / guard / hit): `vfx::hit_sfx` reads the HitEffectSfx tables (attack's
   defSfxMaterial1/2 or the defender's materialSfx1/2 -> HitEffectSfxConcept(JustGuard)Param
   atk<group>_1/_2 by atkMaterial_forSfx -> HitEffectSfxParam <Slash|Blow|Thrust>_<S..LLL> by
   atkPow_forSfx): the General's sword deflected = 252001 "Jasuga sparks", guarded = 201002, Wolf's
   cut on his armour = 201001 + 229001. combat.rs passes the ids, vfx.rs plays the FXRs (hand-made
   sparks only when none). The fields and tables are exported (2026-10-10); sim test
   `clash_effects_follow_the_hit_effect_tables`. fxr_extract.py's used_ids() includes every
   HitEffectSfxParam id.
4. A `slab_allocator Use-after-free` error line can appear once at start (dynamic mesh churn):
   harmless so far, not traced.
- Sound fix (user: Mortal Draw's sound "stays playing"): slotted FFX sounds s000440000/1/2/5/6 are
  FMOD loops; `sound.rs` now plays them tracked (`fmod.rs` EventHandle / play_tracked / stop) and
  stops them when the event ends or the anim changes. Ignored test `fmod_loops` lists the loops.

### R97 HUD pass (bug fixes, 2026-10-14)
- hud.rs: boss bars added (an enemy with ninsatsu > 0): the HP bar top left (MENU_HP_base_Boss / MENU_HP_bar_Boss), deathblow orbs (MENU_HP_Boss lit, _base spent), the name from npc_names.json, and the posture top centre (MENU_Taikan_base / _bar, centre-out, State tint, MENU_Sp_Center). Positions are from clip r_000230. A boss gets no over-head bars.
- Wolf: the HP bar is 9.5 vw at x 5.7 %, y 91 % (MENU_HP_base_2 + a tinted MENU_HP_bar). The posture is bottom centre, 27 vw at y 86.7 %, in Taikan art. The bottom-left text is gone. Counters: emblems top right, gourd bottom right.
- gaps: the Taikan ST1-3 glow / Effect / Crash layers and the "<" ">" arrows are not drawn yet. The emblem counter art (icon + bar) is not drawn. The 0 counter at right-middle (x 85-94 %, y 69 %) is missing. The item icons are about 2x too large. Text needs a backing on bright ground.
- Suite: 540 / 2 failed. The 2 failures are not HUD-related (ai observe_slots_follow_the_exe_list, a_growing_bullet_reissues_its_appear_sound 11903311); they come from other sessions' WIP.
- R97b: the boss posture glow (MENU_Taikan_ST1/ST2 by posture_state 1/2; ST3 waits on state 3, a gap) sits behind the bar. Item icons are now 4.5 / 3 vw at right 17 %, bottom 6 % (clip x 78.5-83 %). The 2 failures above were Bosees' mid-edit WIP; Bosees reports them green now.
- R97c (clips, 10 fps strips): the defeat banner fades in 0.4 s, holds 2.5 s and fades out 0.4 s (Gyoubu 忍殺 at 12:16.8). The kanji is centred over y 24-52 % with the name in small caps under it. Banner 5 now uses MENU_Fusigiri. Test the_defeat_banner_fades_as_the_clip_does, mutation-checked. Mini-bosses get the same top-left bar block as bosses (Shigenori), which matches the ninsatsu > 0 rule. The bars vanish at the fatal deathblow with no fade (as ours do). The 危 shows 0.6-0.8 s on the chest (ours is 0.8 s).
- R97d: the banner now waits for the boss's death and Wolf's finisher to play out (menu::finisher_done, FINISHER_CAP fallback), as in the clip (Gyoubu: 9.5 s after the fatal deathblow). Won waits for the banner to finish (BannerDone).
- R97d: the HP damage trail, measured at 60 fps on Gyoubu (4 trails): it holds 0.5 s from the first hit, later hits do not restart it, and it collapses in about 0.05 s. The colour is about RGB 165,130,120. This is now on both Wolf's bar and the boss bar (hud::step_trail). Test the_damage_trail_holds_then_collapses, mutation-checked on the hold and the collapse.
- The combat_flow clip is HUD-off, so these stay open.
