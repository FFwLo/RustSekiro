# visuals: Models, textures, animation playback, blending, behaviour graph, lighting

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: NEXT 4 phase A done: real animations. tools/sekiro-extract/src/spline.rs decodes hkaSplineCompressedAnimation
  (port of SoulsAssetPipeline's decoder); export writes extracted/anim_c0000.bin (146 bones, 67 clips) and anim_c1020.bin
  (126 bones, 185 clips). src/anim.rs builds bone hierarchies, poses them from actor.anim/t, draws bone lines, crossfades
  clips (0.12 s, gap: real per-transition blend times), mounts blades (player R_Weapon local -Y; enemy R_Hand -> R_Katana_long).
  Phase B (FLVER meshes + textures) next. tools/screenshot.ps1 captures the window (run with -ExecutionPolicy Bypass).
- 2026-10-06: NEXT 4 phase B done: real meshes. tools/sekiro-extract/src/flver.rs (FLVER2 + TPF), command
  `model <flver> <tpf> <out> <texdir>`; Sekiro FLVERs have empty texture paths, albedo picked from MTD name keywords.
  Exported model_c1020.bin (346 nodes, 36 meshes) and Wolf parts model_c0000_{am_m_9000,bd_m_9040,lg_m_9000,wp_a_0300}.bin
  (default outfit from EquipParamProtector 100000-103000, Kusabimaru = wp_a_0300 from EquipParamWeapon 5000).
  src/model.rs skins them onto the animated skeleton by bone name (compact per-mesh joint lists, 256 limit);
  weapon parts mount on R_Weapon. Asset root = extracted/ (DDS via bevy "dds" feature).
  Open: tiling bandage/rope textures (shared texture pack), normal maps, player head check.
- 2026-10-06: NEXT 5 done: hitboxes from dummy polys. Model format v2 carries FLVER dummies (id, attach bone, model-space pos);
  model.rs parents DummyPoly entities to the attach joints (offset = bind^-1 * pos), Dummies map per actor (weapon wins).
  combat.rs: capsule hit0_DmyPolyId1 -> 2 (radius hit0_Radius) vs body segment feet+0.3..1.5 (r 0.4); reach fallback.
  Player slash = weapon dmy 120 -> 100 r 0.4; c1020 = 11 -> 10. Verified in game (enemy combo hits via capsule).
  Debug draw (H / debug menu): active hit capsules in red, body capsules in grey (combat::draw_hitboxes).
- 2026-10-06: NEXT 6c done: the regen level multiplier chr.f[0x10d0] (set in FUN_1409e6xxx PlayerIns update) is the
  Dark Souls equip-load class: FUN_14084d0c0(ratio) -> 4 if ratio > 1.0 or SpEffect stateInfo 102 (row 500, poison
  DoT), 3 if > 0.7, 2 if > 0.3, else 1/0; multiplier 0.8 (3), 0.7 (4), else 1.0. Max load = SpEffect weight rates x
  (1.0 * stat + 40) (FUN_140a34a80). All EquipParamWeapon/Protector weights are 0 in Sekiro -> always 1.0. Gap closed.
  The same level picks the player's breathing add-blend (cases 5/6/7 -> level + 5/10/15).
- 2026-10-06: real hurtboxes. chrbnd cXXXX.HKX (TAG0) holds hknpRagdollData: 18-19 bodies, each a hknpCapsuleShape
  (a, b, convexRadius) with a bind-pose position/orientation; names "Ragdoll_Ctrl_<bone>" (c1020) / "Ragdoll_<bone>NNN"
  (c0000, NNN -> Spine/Spine1/Spine2). Exported per character ("hurtboxes"), parented to the animated bones in bind
  space (anim.rs), tested capsule-vs-capsule against the AtkParam dummy capsule (combat.rs). H toggles the overlay.
  Tagfile reader now handles 5/6/8/9-byte varints (needed by physics files); `hkxdump` / `ragdoll` debug commands.
- 2026-10-07 FIX: models/textures/sounds missing when the release exe is started directly (not via cargo run): Bevy's
  AssetPlugin path "extracted" resolved against target/release. Now concat!(CARGO_MANIFEST_DIR, "/extracted"), same as
  every other loader. 0 "Path not found" errors; both models render.
- 2026-10-07 NEXT 18 partial: `sekiro-extract clips <behavior.hkx>` (hkx::clip_generators) reads every hkbClipGenerator.
  All 2566 in c0000.hkx and all 388 in c9997.hkx are plain (playbackSpeed 1, no crop / start offset / enforced
  duration), so playing TAE clips at 1x from frame 0 is exact. Left: the 13 transition effects (where the state
  machine uses them) and the 2 locomotion hkbBlenderGenerators.
- 2026-10-07 NEXT 18: `sekiro-extract transitions <behavior.hkx>`: all 16 transition effects (StateToStateBlend,
  Duration0, TaeBlend*, DefaultTransition, MovementResetTransition, ThrowTransition) have duration 0, so crossfade
  time = the target anim's TAE Blend event and anims without one snap in. Was: 0.12 s fallback fade. Now: TAE anims
  without a start Blend (60 player states: deflected/stagger/air reactions, 82 enemy: damage, guard bound, death)
  snap; only procedural locomotion keeps 0.12 s (gap: hkbBlenderGenerator weights). Clips: see previous entry.
- 2026-10-07 NEXT 18 done: `sekiro-extract blenders`: c0000.hkx has only 2 hkbBlenderGenerators (Master Blend over
  Master_SM, and AddHang Blend bound to HangWallAngle) - no locomotion blend tree. Locomotion is CMSG clip selection
  whose transitions are the 0 s TaeBlend effects, so walk/run/idle switches now blend by the shown clip's TAE Blend
  (walk/run 6 frames = 0.2 s, idle 9 = 0.3 s) instead of the 0.12 s fallback (anim.rs proc_clip / clip_key). Gap 8 closed.
- 2026-10-07 TAE 151 CameraLookAtTarget (case 0x97 -> FUN_140b517a0 -> camera singleton +0x3c dmy, +0x40..+0x48
  floats, +0x4c..+0x58 look limits in radians; FUN_14073c260): with dmy >= 0 the focus chases that DummyPoly on Wolf at
  min(+0x40, 1) on all axes (unk1 0.75-0.8 in StandDeflectHardExLarge / deathblows; <= 0 keeps the normal rates; a
  blocked line of sight drops it). Wolf's body dummies were missing (only the sword's were exported): model_c0000.bin
  now holds the base c0000.flver's 563 dummies (0 meshes; tools/extract.ps1 exports it), which also gives body-based
  attacks (kick etc.) their real capsules instead of the reach fallback. Look limits not used yet.
- 2026-10-07 VISUALS (user: "models and animations to be better"): (1) Wolf had no head: EquipParamProtector 100000
  equipModelId 200 (category head) = parts/fc_m_0200 (face + hair; unpacked + exported to model_c0000_fc_m_0200.bin).
  fc_m_0100 is another face (beard). Face decals (fur, lashes, eyeshadow, mouth) use a shared alpha pack that is not
  exported: flver guess_albedo returns "-" and model.rs skips them; damage overlays are never used as albedo.
  (2) Enemy drew 1 of 36 meshes: NpcParam modelDispMaskNN = 1 HIDES group #NN# in practice (under "show", the General's
  8/14/17/24 match almost no mesh); c1020 now 35 meshes. (3) Lighting: warm key 12k + cool fill 2k + ambient 300.
  VISUALS NEXT: normal maps (_n textures not exported; need tangents), Wolf hair colour (hair texture in the shared pack),
  enemy scabbards stick out (physics/cloth bones stay in bind pose), animation polish per user feedback.
- 2026-10-07 docs/HANDOFF_PLAN.md: task packets for helper models (V1-V5 visuals, S1-S4 swordplay), file ownership,
  report format (docs/reports/<ID>.md). Lead keeps player.rs / combat.rs.
- 2026-10-07 helper V1 (normal maps) integrated: SHMD v3 adds a normal-map name per mesh; Sekiro _n = BC7_UNORM
  (4-channel, linear), _a = BC7_UNORM_SRGB; paired by name x_a -> x_n; model.rs generate_tangents + normal_map_texture.
  Enemy armour detail visible in game. Gap: green-channel convention unchecked. Helper V2: FC_M_0200.tpf does hold the
  face pack (eye, hair01, Fur, Mouth, eyeshadow); per-MTD texture names from extracted/mtd/*.mtd. Lashes and head rope
  use the shared FC_A_0000_hairface_a / _Rope_a (not in the dictionary yet) and stay hidden.
- 2026-10-07 CLIPS: TAE ImportOtherAnim (mini header 1) imports the HKX as well as the events, but export only looked
  for "<key>.hkx". Now it falls back to the imported anim's clip: Wolf's anims without a clip 58 -> 1, the enemy's
  179 -> 104 (many were playing a neighbouring id via data.rs alias_missing_clips guesses). E.g. a201_511200 ->
  a201_510200, a201_511400 -> a201_510300.
- 2026-10-07 enemy weapons: c1020 weapon groups #00#-#07# (dominant bones: #00# R_Katana_long, #02#/#04# R_Katana_shot,
  #01#/#03#/#05#/#06#/#07# Sheath01/02[omit]). 98 of the enemy's AtkParam hit dummies hang on R_Katana_shot, so the
  drawn blade is a R_Katana_shot group; model.rs hides in-hand blade groups on other weapon bones (#00#). TAE 233
  ChangeChrDrawMask (FUN_140b54330 -> FDPChrPrimDispMask, Mask0-31: 0 hide / 1 show / 255 keep) in
  TransToBattleFromDefault (a000_001040: #04# on, #06# off) is applied at spawn since the enemy starts in battle.
  Deaths/throw deaths set Mask0-9 = 0 (weapons vanish) - runtime 233/711/713 handling not implemented yet.
  The NpcParam modelDispMask byte layout could not be confirmed from the exe (offset 0x14f-0x154 region); the
  "1 = hide" reading stays (gives complete outfits). Remaining V3: the [omit] sheath bones are physics-driven and
  stay in bind pose (lying flat across the hips); needs a hang/pendulum.
- 2026-10-07 tools/photo.ps1 + src/photo.rs (SHINOBI_PHOTO): deterministic 4-view contact sheet (~400 tokens).
- 2026-10-07 V4 done by lead: TAE 700 upper-body twist. Export: hkx::twist_modifiers -> combat_data "twists" (name ->
  up/down/right/left limits [deg] + chains {start, end, rate, newTargetGain, onGain, offGain}; TwistParam is 0x18
  bytes). Wolf skeleton: 7 RootPos, 44 Spine2, 79 Neck, 80 Head; 0_Twist chains RootRotY..Spine2 (rate 0.3) and
  Neck..Head (0.7); 1xx_Attack chain RootRotY..Spine1 (rate 1, UD only). anim.rs apply_twists (after pose_skeletons):
  target = lock-on else Actor.homing; yaw/pitch clamped, eased by onGain per 1/30 s, back to 0 at a fixed 0.05 when
  the event ends (gap: per-chain offGain after the event; newTargetGain unused). Verified in photo shot 4_lock_front.
  Enemy twists (c1020.hkx modifiers) not done.
- 2026-10-07 runtime draw masks: NPC "#NN#" meshes are all spawned (hidden when off at spawn) with MeshGroup; model.rs
  update_draw_masks applies TAE 233 ChangeChrDrawMask when it starts (kept in the actor's DrawMask; deaths and
  ThrowDefDeath* set Mask0-9 = 0 -> the enemy's weapons vanish) and TAE 711 HideModelMask / 713 ShowModelMask while
  they run (TransToBattleFromDefault: #04# hidden / #06# shown during frames 0-29 of the draw).
- 2026-10-07 normal maps: flip_normal_map_y = true (Sekiro is DirectX; Bevy's flag is for DirectX-authored maps).
  Visual A/B at photo scale is subtle. Plunge target now the defender's DummyPoly 233 (model root, (0, 0, -0.4))
  + ChrPhysicsHomingParam targetOffset (0, 0, -0.3) via the dummy's GlobalTransform (enemy position in tests).
  V3 update: c1020.HKX ragdoll = body only (19 bodies, 14 ragdoll + 4 limited-hinge constraints: knees/elbows); the
  sheaths are Havok cloth in c1020_c.hkx ("Sheath0"), so a cloth/pendulum sim is needed (left for a helper).
  Enemy twists: c9997.hkx modifiers "00_Twist".."NN_*" use generic chain indices (start 0, end 1) resolved per
  character - mapping not found yet; enemy twist not applied.
- 2026-10-07 ADDITIVE LAYER: additive clips are per-bone deltas (frame 0 = identity; e.g. a050_120000 16 f,
  a000_299060 2 f, a000_1000xx 14 f) - anim.rs multiplies them onto the base pose (rotation base*delta, translation
  +delta) from Actor.add_anim / add_t. combat.rs starts them for the HKS add behaviours: minimum-level guard ->
  StandDeflectEasyMinimum (a050_120000), deflect -> StandDeflectHardMinimum (a050_130000), minimum-level body hit ->
  AddDamageStart_{F,B,L,R} (a000_10000x, hkx AddDamageStart_Selector_Dir), ref 228 -> AddHardDeflectGuard. The
  layer runs for max(clip length, last TAE event).

### 2026-10-07 Havok cloth in game (src/cloth.rs)
- sekiro-extract `model` now writes `<out>.cloth.json` when `<flver stem>_c.hkx` sits next to the FLVER
  (cloth_export.rs): particles (rest pose, invMass, radius), sim triangles, constraint sets, the
  `#01#` state's operators flattened to one formula each (ref skin, MeshMeshDeform display verts,
  SimpleMeshBoneDeform bones). Every bind check reproduces the FLVER to 0.0000.
  A bone pair that misses its bind pose (c1020 sholderarmor -> Master, err 2.05) is padding: skipped.
- c1020: haori (161 p), Armor04, Armour01 rope, head rope (display meshes), kusazuri (14 bones),
  hakama (4 bones), sholderarmor (1 bone). Wolf bd_m_9040: 3 scarf cloths, scarf neck, robe 01/02/03.
- Game: Verlet in Havok world space (game mirrored X), gravity / damping / substeps / iterations /
  constraintExecution from the file; standard + stretch links and local range (sphere + normal
  min/max) solved. Display meshes become CPU meshes (no joint attributes, else the GPU skins them
  again with no joints -> the "giant sheet" bug), non-cloth verts CPU-skinned. Cloth bones get their
  GlobalTransform after propagation and re-propagate to children.
- Collisions (2026-10-07): perInstanceCollidables (capsule / tapered capsule) on bones via
  collidableTransformMap (u32 bone indices + offset matrices), staticCollisionMasks bit per collidable,
  particle radius added; solved at the -1 steps of constraintExecution. Transfer motion: disabled in all.
- 2026-10-08 bend links solved (bendMinLength / stretchMaxLength with their stiffnesses). gap: bend_stiffness /
  transition sets; damping read as Havok
  globalDampingPerSecond (velocity fraction removed per second).
- Sheaths (V3): Sheath01/02[omit] are tracked in all 260 c1020 clips (near bind pose), so their pose is
  the game's animation; no physics needed. They are not in the ragdoll (19 Ctrl bodies) or the cloth.
- `SHINOBI_CLOTH_DEBUG=<sec>` logs each cloth (refs, bones, distances) once at that time.
- 2026-10-10 CLOTH COST (the sandbox line-up of 20 enemies ran at 22 fps, cloth 41 ms a frame):
  the cost was not the solver. (1) `std::env::var("SHINOBI_CLOTH_FLIP_N")` was read per display
  vertex (50k+ on a fur mesh): now a OnceLock, like `debug_now`'s variable. (2) the blow-up
  check (a particle > 3 m from the model root -> restart) restarted big characters' cloth every
  frame (c1040's fixed particles sit 3.3 m out): the threshold is now the cloth's own reference
  reach + 3 m, and a cloth that blows up `GIVE_UP` (5) frames running is carried rigidly from
  then on (warned once). (3) `simulate` runs one task per character (`par_iter_mut`), writing
  into each cloth's `ClothOut` (driven bones' GlobalTransforms, display mesh vertices); the
  serial `apply` system (same PostUpdate chain after Propagate) puts them into the world. Inside
  a character, later cloths skin from the bones earlier cloths drove through an `overrides`
  map (`propagate`), as they did from the world before. Display mesh skinning (`skin`) runs in
  4096-vertex chunks on the compute pool. (4) `SHINOBI_CLOTH_RANGE=<m>` (default 15, 0 = all):
  cloth further than that from the camera is carried in its restart pose, not solved.
  Result: 3.9 ms a frame for the 20, 48 fps (empty sandbox 69, gate map 59); the gate map's
  cloth looks the same. The "cloth cost" log (once a minute, only when simulate exceeds 1 ms
  a frame) names the top 3 cloths with a per-phase split: refs / restart / solve / bones /
  skin / verts. Probes: `SHINOBI_CLOTH_NO_MESH=1` (no bone / mesh outputs),
  `SHINOBI_CLOTH_NO_UPLOAD=1`. Still restarting every frame (small): c1060_mino1, c1100_sleeve.
- 2026-10-08 ENEMY TWIST (TAE 700) done. The NPC graph c9997.hkx binds its CustomLookAtTwistModifiers to
  character properties (hkbVariableBindingSet bindings of type 1: "twistParam:0/startBoneIndex" ->
  property 0 ...). The values are in the behbnd's Characters\c1020.hkx (hkbCharacterData
  characterPropertyValues + hkbCharacterStringData names): RefTwistNeckStart/End 88/89 (Neck/Head),
  SpineStart/End 7/59 (Root/Spine2), NewTargetGain 0.06, On/OffGain 0.1, limits up/down 30 right/left 45,
  SpineRate 0.3, NeckRate 0.7, RefTwistEnable 1, RefQuadrupedTwistEnable 0 (quadruped modifiers dropped).
  The unpacker used to overwrite Export\, Characters\ and Behaviors\c1020.hkx with one another; repeated
  names are now also written as <folder>_<name> (plain name = last, as before). Export: CharData.twists.
  anim.rs: both sides; modifiers matched by number ("0: 0_Twist" -> 0_TwistLR + 0_TwistUD / 00_Twist),
  all their chains applied, each clamped by its own modifier's limits (Wolf used to get only one of
  0_TwistLR / 0_TwistUD, chosen by HashMap order). NPC target = Wolf.
  Fixes found on the way: untracked bones now reset to the reference pose every frame, and crossfades
  start from the last pre-twist pose (Skeleton.last); before, a clip change every frame (photo mode's
  hold vs the AI) baked each frame's twist into the fade and the enemy folded forward.
- 2026-10-08 live: a000_7900x0 = AddBlendFace_1..N CMSGs (c0000.hkx) = Wolf's facial-expression additive layer
  (790050 plays during deathblows); it interleaves with the body anim in TimeAct +0x100 -> rec_read.body_anims
  skips it. Not modelled in Shinobi (gap: face layer).

## Outfit masks, scabbard (2026-10-08)

- NpcParam modelDispMask**N** = 1 **draws** the "#N#" mesh group (the game's rule). The old
  "1 = hide" guess came from row 10219000, an Ashina Shitenno sample whose masks (8/14/17/24) fit
  another outfit. The General is now 10203010 (regular variant, behaviorVariationId 10200, the one
  fought live): katana #04 in hand (TransToBattle TAE 233), sheath #07, no haori #12 ("with haori"
  rows such as 10200010 add 12). Debug menu (F1) switches the outfit row (config enemy.npc_row).
- Kusabimaru's scabbard is WP_A_0300_1.flver in the weapon partsbnd. Export by hand:
  `sekiro-extract model extracted/parts/wp_a_0300.partsbnd.d/WP_A_0300_1.flver extracted/parts/wp_a_0300.partsbnd.d/WP_A_0300.tpf extracted/model_c0000_sheath_a_0300.bin extracted/tex`
  It sits on the body FLVER's dummy 147 (attached to the Sheath bone).
- 2026-10-08 weapon placement = WepAbsorpPosParam (EquipParamWeapon 5000 absorpParamId 5000): right_0 20
  (blade), right_1 147 (scabbard); each part's model space is that dummy's frame. Dummy 22 has the
  same position / forward but the opposite up: on 22 (or the bare R_Weapon bone) the katana pointed
  behind Wolf.
- Dummy directions come from the sidecar `model_<chr>.dummies.json` (`sekiro-extract dummies`).
- TAE 715 OverrideWeaponModelLocation (2026-10-08): WeaponModelType 0 = right weapon; Model0 = the
  blade, Model1 = the scabbard go onto body dummies while it runs (20 right hand, 76 / 77
  left hand, 147 the sheath, 149 the large sheath): iai arts a103 / a104 / a106 hold the scabbard in
  the left hand and sheathe the blade. model.rs override_weapon_location (post-propagate,
  part space = the dummy's frame). Photo mode takes raw anim keys as poses.

- 2026-10-09 sheathing (X / debug menu): HKS BEH_R_NON_COMBAT_AREA_ENTER / LEAVE -> GroundNonCombatArea[Move]Enter
  a000_700500 / 700501 (TAE 32 SetWeaponStyle "0: None" at frame 14; TAE 715 puts blade + scabbard on 147 from 14),
  Leave a000_700510 / 700511 (style "1: Right Weapon One-Handed" at frame 5, 715 on 147 frames 0-7). Sheathed, the
  blade sits on WepAbsorpPosParam rightHang_0 (147). An attack / guard / art / prosthetic press while sheathed draws
  the sword instead. AddNonCombat_SM clips 299000 / 299400 / 299401 are identity carriers (events only).
- Guard release -> run: only the upper body crossfades (the legs already run the same loop).
- 2026-10-09 materials (MTD-driven): the FLVER's texture paths are empty; each material's MTD (mtd/allmaterialbnd)
  names its shader (SPX) and the texture per sampler slot. Primary albedo = the "..._AlbedoMap_0" slot (faces also list
  damage / skin-tone layers), normal / metallic = the same texture set (x_a -> x_n / x_m). Shared hair / bandage /
  fabric textures: parts/common_body.tpf. Name guessing had 53 of Ochimusha's 81 meshes, Wolf's coat / tops, the
  katana and the face on wrong textures. Model format v5 adds the metallic stem and an alpha-test flag.
- Character_AMSN = Albedo / Metallic / Shininess / Normal. _n (BC7): RG = tangent normal (DirectX green), B =
  shininess (gloss; Wolf's coat 0-36, armour up to 128), A unused here. _m (BC4): metallic mask (0 / 255).
  Bevy read RGB as XYZ: B ~ 0.1 put every normal into the surface (the flat, washed-out look). sekiro_material.wgsl
  (ExtendedMaterial) rebuilds Z with a clamp (Sekiro XY exceed the unit circle; an unclamped sqrt is NaN = black
  patches), faces the normal by the vertex normal (the X mirror reversed the winding), roughness = 1 - gloss,
  metallic from _m. Cloth display meshes regenerate tangents each frame (world-space vertices).
- Alpha: character albedos are cutouts (BC1 punch-through / 0-255 BC7: Wolf's tops are 42 % holes), except the SSS
  skin and eye shaders, whose alpha is not opacity (Wolf's head albedo is 86 % alpha 0).
- 2026-10-09 cloth from the exe's Havok solver:
  * hclBendStiffnessConstraintSetMx (vtable 0x142c81a68, apply FUN_141526450; singles FUN_14152b490): per link
    v = sum w_i p_i; with useRestPoseConfig add unit(nA/|nA| + nB/|nB|) * hA hB restCurvature (e = D - C,
    nA = e x (A - C), nB = (B - C) x e, hA hB = |nA||nB|/|e|^2); clamp: (hA hB rc)^2 > maxRestPoseHeightSq ->
    stiffness 0; p_i += w_i invMass_i bendStiffness v. Solver stiffness factor 1.0 (FUN_14156d230 table:
    modes 0/1 constant 1.0, mode 2 pow(substeps * scale, -1.725)). All of Wolf's cloths use rest pose + clamp.
    Was unsolved: the coat skirt folded freely and flipped up to the hips on jumps.
  * Fixed particles reach their reference over the substeps (Simulate lerps by (substep + 1) / substeps).
  * Damping = pow(1 - globalDampingPerSecond, h) (+0x54, "Update Effective Damping") and the time-step change
    rescale (prev = x - (x - prev) * h/h_old) match ours.
  * Coat alpha (P_BD_M_9040_Court, g_AlphaRef 128, g_BlendMode 0) is a real cutout: torn hem and collar edge.
    The 2026-10-09 "AN_Blend is a blend mask" change was wrong (holes became solid dark flaps) and is reverted.
- 2026-10-10 vfx.rs: clash effects (deflect burst + flash + light, guard sparks, hit blood) with camera bloom
  (threshold 1.2: only HDR effect colours glow). Later 2026-10-10: the game's FXR effects play via src/fxr.rs
  (HANDOFF section 9); vfx.rs / gore.rs looks are still hand-made until routed through it.
  hud.rs redone: Wolf vitality (damage trail), resurrection nodes, gourd / emblem counts bottom-left, posture
  bottom-centre (shown with damage); enemy bars over its head; debug text and combat log only with F1.
- 2026-10-10 gore.rs: blood driven by each character's TAE. TAE 96 SpawnOneShotFFX with blood FFX (220505 /
  220506 deathblow gush on the neck dummies 800-806, 220502 / 220503 on Wolf's sword dummies 121 / 100):
  drops from the dummy along its +Z for [start, end), thinning out. TAE 138 DecalParamID_DummyPoly: a
  DecalParam stain under the dummy. 710011: mask dp000160000_m (splat, BC4 red), diffuse dp000100000_a
  (grey noise) tinted diffuseColor 191/54/19, pitchAngle -90 (straight down), near/far -0.25..5 m with
  nearSize 0.5 -> farSize 3.5 (read as the size at the floor's depth: ~1.6 m from a 1.7 m neck),
  randomSize 100-120 %, random roll, thin-out 2 within 1 m, lifeTimeSec 999 with bLifeEnable 0.
  Textures: other/decaltex.tpf (67 dp* textures); the export writes extracted/decal/<id>.png (RGB
  diffuse, A mask). gap: blood not yet drawn by the FXR player (src/fxr.rs can play f000220505.fxr: HANDOFF section 9);
  decal blend not traced (mask alpha^0.55 in game). Check: SHINOBI_THROW_TRACE=behind
  SHINOBI_THROW_CAM_YAW=90 SHINOBI_THROW_SHOTS=<dir> SHINOBI_THROW_SHOT_RANGE=130,250,8; SHINOBI_GORE_LOG=1.
- 2026-10-10 deathblow mark (hud.rs): HUD sprites are Scaleform FE (menu/01_000_fe.gfx) over atlases in
  menu/hi/01_common.tpf (46 textures: SB_FE 4096x1024, SB_FE_02, ...) with sprite rects in
  menu/hi/01_common.sblytbnd <atlas>.layout (XML SubTexture name/x/y/width/height). 忍殺 = MENU_ninsatu_02
  (405 px red glow, white-hot core) + MENU_ninsatu_01 (128 px); also MENU_ninjyutu, MENU_Find_01/02,
  MENU_HP_bar*, MENU_Taikan_* (posture bar). Placed on dummy 220 (lock-on point, chest). Check:
  SHINOBI_STEALTH=close (unaware enemy 1.5 m ahead, back turned).
- 2026-10-10 the game's own map around the fight (pieces, hit collision, draw-param lighting): see `docs/kb/map.md`.
  One lesson for every tiled texture: load it with a repeat sampler (Bevy's default clamps to the edge, which smears
  one texel over the whole mesh and looks like a missing texture).
