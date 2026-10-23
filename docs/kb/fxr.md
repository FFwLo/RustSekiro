# fxr: the game's own effects (FXR player)

Moved out of `docs/HANDOFF.md` section 9 on 2026-10-15. Code: `src/fxr.rs`, `src/ffx.rs`.

## 9. Game effects from the FXR files (2026-10-10)
Every TAE effect (FFXID in TAE 96 / 118 / 120..) whose `f<id>.fxr` exists now plays the game's own
effect, drawn by our FXR player. The hand-made looks in `ffx.rs` are only a fallback for ids with
no FXR.

**Pipeline (run once after `sekiro-extract export`):**
- `sekiro-extract unpack <Sekiro dir> extracted 'sfxbnd_commoneffects'` (2349 FXRs + their TPFs).
- `cd tools/fxr-dump && npm i` (@cccode/fxr, public domain, github.com/EvenTorset/fxr; a clone is
  in `../reference/fxr`), then `python tools/fxr_extract.py`: every FFX id the
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
   2026-10-15 pass (clip sekiro_all_skills 6:44.8-6:46): two fixes in `fxr.rs`.
   (a) Tracer width direction, from the exe: the tracer builder FUN_14042c4d0 (orientation switch
   at 14042da96) writes per point the vector the domain shader spreads the width along
   (GXFfxtessellateTracerTexture.dpo: pos = lerp(p) + lerp(TEXCOORD4.xyz) x 2(v - 0.5) x width).
   Mode 0 = travel x camera, 1 = the stored node Z itself (it was travel x Z before), 2 / 3 / 4 =
   constants (DAT_143f083c0 / DAT_143f08390; 4 adds a crossed second ribbon). Weapon dummies point
   +Z along the blade (FLVER dummies 300/301/311, mortal 200/240/280), so mode-1 arcs now spread
   along the blade: Mortal Draw is a broad red sheet from the side, as in the clip.
   (b) Additive tracers (606 / 10012, blend 0 / 4 / 7) no longer get the lit 1/8: the 48 lit/unlit
   pairs behind the 1/8 are all normal blend, and the tracer pixel shader lights by
   saturate(colour) x light (cb0[34].y g_ps_bEnableLighting), not a fixed scale. The Mortal Draw
   red (26030 / 2010, rgbM 2.5) was near invisible; it now reads red, not pink. gap: the game's
   light term (envCbColScale x FC_DirLight Col_0 x g_vLightMask.x + v3); the game's band is still a
   brighter orange-red.
   Shots: `SHINOBI_SKILL_SHOTS` now runs on a fixed 1/60 s step (`ffx.rs`
   TimeUpdateStrategy::ManualDuration), so frame N is the same anim time on every run (the first
   press hitches while the art's effects load). Use SHINOBI_MAP=off; for white / additive effects
   aim the camera at the dark wall (SHINOBI_SKILL_CAM=200,-3): the pale floor hides them.
2. Every art shot (5100-6100, 7700; 2026-10-15, flat floor). Ashina Cross 440070 / 440071 reads as
   wide white crescents on the dark wall (faint only over the pale floor). Ichimonji's dust 440081
   looks large but follows the file: 603 width 0.25 -> 3.75 m over 1.5 s, particleModifier scaleX
   up to 2.25 on orientation-7 ground layers, speeds 6.6-26 m/s decaying. The one shared mismatch
   across arts (Ichimonji, Shadowrush 5800, 6000, the foot dust 101141 grey 0.5) is that lit
   ground dust draws dark grey: it is lighting -2 at the 1/8 stand-in. gap: the lit light scale
   (SHINOBI_FX_LIT port waits on g_ps_envCbColScale / g_vLightMask.x, HANDOFF 4.58).
   Every tool shot (70000-79200, 2026-10-15). Fixed: white star-burst sparks around Wolf on the
   whistle, flame vent, umbrellas, axes and spears were `prosthetic.rs` spawn_row's firecracker
   stand-in (vfx.rs Clash x4), which ran for any still area bullet (radius >= 1 m) without an FXR,
   so also for the 27 rows that name no effect at all (sfxId_Bullet -1: 790004 / 79001x, 730201,
   760201 / 760300, 710013 ...). It now needs sfxId > 0. Firecracker vs the clip (10:27): bright
   orange bursts match; the game shows almost no smoke, ours leaves dark clouds (the lit gap
   above; SHINOBI_FX_LIT=1 turns the same smoke blinding white, so the true scale is in between).
   Divine Abduction's swirl reads right on a dark background. Sabimaru's swing trail 300213 is
   faint by data (rgbMultiplier 0.3-0.4). 70500's shot froze on the Options menu (stray input):
   not checked. `SHINOBI_LOG_DUMMIES=<frame>` (default 200) now also lists the tool dummies.
   sekiro-rs hints (2026-10-16), each checked against GXFfxtexture.ppo (disassembled:
   rgb = |c| x (tex.rgb x a? x c/|c|)^2 x g_vColorScale, a = sat(tex.a + v3.w) x c.a):
   (1) "drop the sqrt 3": A/B shots (Mortal Draw 5700, Firecracker 71000, dark wall) against
   the clip (6:45, 10:27): without it both read dimmer and further from the game's bright
   orange band / bursts. Kept. g_vColorScale (cb0[9]) has no static writer found (the exe's
   "ColorScale" strings are the glow filter's). (2) premultiply before the square: confirmed
   in the shader, behind g_ps_bPreAlphaMult (cb0[34].x); which draws set it is not in the FXR
   data (gap: read it live with the light scale, HANDOFF 4.58). Not applied. (3) "load sprite
   textures UNORM, sRGB is double gamma": does not apply here; our sprites load sRGB and the
   shader does not square again, so the sRGB decode (~t^2.2) is the game's single t^2.
   (4) "negate node X rotation, direction 6 = world up": reference/fxr InitialDirection 6 is
   LocalNorth (+Z), as we use; our X mirror (game_vec -x) keeps X rotations and flips Y / Z
   (game_rot), which is right for that mirror. Not applied.
   Tool effects vs the recording rec_20261015_203558_skills_hooks (fx rows, 2026-10-16):
   `SHINOBI_FFX_LOG` now prints each effect's ahead / side / up of the character (forward is
   -Z, `Actor::forward`; the recorder's yaw forward is (-sin, -cos)) and each bullet's
   sfxId_Bullet. Axe 300211 on 21300: game 1.1 m behind / 0.6 side / 1.07 up, ours 1.19 / 0.61,
   so the tool parts' placement on dummy 21 is right (an earlier "spear behind Wolf" was the
   +Z misread). The spear's tip spark 300185 / 304185 on 21301 sat 6.2 m ahead (game 4.4 m in all
   6 uses) and the head flew off the shaft: its tubes' clips carry scale (poleC_01 y x2, poleB_02
   y x0.5) and Havok does not scale a child's offset by its parent's (hkQsTransform::setMul);
   `tool.rs havok_locals` now composes it so (4.26 m). The flame spear (78400) showed no fire
   and the axes' trails were missing: the tool pivot's own Transform stayed at the origin, so
   the effects' tick read the part's dummies 1.6 m under the floor at the world's centre; the
   pivot's Transform is now set with its global (`place_tool_models`). Sparking / Lazulite Axe
   trails and bursts, the flame spear's burning head and sweep now show. With every tool held
   (`SHINOBI_SKILL_HOLD=<frames>`, the shot tool now runs after the input read) the TAE and
   bullet effects of every move match the recording's, except two: Divine Abduction's "wind
   collecting" 107700 / 107702 (TAE 940, 3 s) -> SpEffectVfx 7700 -> 300190 was never shown,
   now is (status.rs `lasting_glow` also shows `Player::timed`; game 460-493 ms, ours ~0.5 s);
   300052 is Bullet 1000000's sfxId_Hit (a hit, not the tool). The other only-game ids are
   enemy hits (blood 201001 / 220001, 130xxx, 401000, 617030).
   One Mind (6100): the recording plays 500530 -> 500531 -> 440131 once per use, as ours does; the
   storm is that one FXR (two NodeEmitters, 9 emissions 1/30 s apart). Its slashes (type 10012,
   normal blend, lighting -2/0) were drawn at 1/8 brightness; `particle_color` `lit_tracer` now
   skips the 1/8 (gap: light term taken as 1). 2026-10-16 quick pass over all 19 arts and the
   tools (held): every effect the TAE / bullets name spawns and shows; the whistle 300200 /
   304200 are small swirls by design.
   Mortal Draw (a106): its 440005-440008 are StateInfo 993 ("has emblems", art_emblem_gate) and
   never played - ffx.rs built its gate list without player.rs's resident_gates; now it adds them
   (the skills recording plays 440005 / 440007 with every 440001 / 440003).
   Lingering lines (2026-10-16): tracers ignored fadeOutTime (10012 / 606: once the particle ends
   the trail fades to 0 over it; 908 tracers 1 s, 429 0.2 s) and kept full colour until their
   points aged out (segmentDuration, up to 4 s): lines of the effect's colour hung where a swing
   had been. Now `tracer_end_fade`. A following effect whose anchor goes keeps its last place
   (it fell back to its spawn point / the origin). A stopped effect (the TAE event's window over,
   e.g. Mortal Draw's IsRestrictToDummyPoly 440003 / 440007 f10-20) stops emitting but a node
   with its own duration plays out: the brush strokes (26041, 0.8-2.5 s) were cut at ~0.3 s; the
   clip (sekiro_all_skills 7:00-7:12) keeps them ~1-2 s. 440130's thin glint after its arc is the
   data's (width 0.001, colour3 10 x rgbMultiplier 2).
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
- Sound fix (Mortal Draw's sound kept playing): slotted FFX sounds s000440000/1/2/5/6 are
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
- 2026-10-23 LevelsOfDetail (2002): its 1002 config's thresholds are distances in metres, child i shown from threshold i to threshold i + 1 (threshold 0 is always 1; the detailed level is child 0; far children are mostly empty, a few are far-only glows such as 811052 from 400 m). fxr.rs keeps every level and `lod_level` picks one per step by the camera's distance to the node; the others are neither stepped nor drawn, and end when the effect stops.
