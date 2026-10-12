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
- 2026-10-11 the "ref gap" line is meaningless for a culled cloth: in the sandbox line-up the
  Chained Ogre 40 m from Wolf reported a 1.4-2.2 m gap on every cloth (particles' centroid
  y 3.05, references' y 0.95). Beyond `SHINOBI_CLOTH_RANGE` (15 m) a cloth is carried rigidly in
  its bind pose on the root while the skeleton animates (the Ogre's idle sits), so the
  references (the skinned skeleton) and the particles (the bind pose) part by the pose
  difference; the debug now also prints each reference bone with its position ("ref bones"),
  which is what showed it (Head at y 0.76 for a 3.5 m ogre). Within range the Ogre's cloths
  sit at 0.1-0.3 m like everyone else's. The "particle 8" blow-up (4.5 m from the root, inv
  mass 214) does come back in the sandbox line-up (`SHINOBI_SANDBOX_ENEMIES=all:0`) with the
  Ogre within range and moving, on c1040_fur2 / fur3 / ropu / fur4 at once: the survey
  (`sekiro-extract cloth`, now with each sim's address) shows those four cloth datas each own
  a separate but identical 325-particle "c1040_meshsim_cloth" sim (masses 0.0047, fixed ones 0,
  radius 0.1, damping/s 0.98, gravity -10; only the display mesh differs), so the game runs four
  copies too and the export is right. FIXED the same day: the cause was an empty
  `hclSimulateOperator::Config.constraintExecution` in the game data. 15 cloths have it empty
  (c1040 fur2/fur3/ropu/fur4, c1070 #10#_hair1, c1190 mino/mino2-5/parts, c5020 hair, c5021
  hair1, c5050 beard2, c5060 coat); Havok treats an empty list as "every constraint set in
  stored order" (the collision pass last), but our loader took it literally and solved no
  constraint at all, so the almost massless particle 8 fell free. The loader now defaults an
  empty list to `0..n_sets` plus -1 (src/cloth.rs `execution`). The survey prints each
  Simulate op's config (substeps, iterations, execution, adapt) to see which cloths hit this.
  After the fix the 90 s line-up logs no blow-up; cloth cost 4.4 ms for 139 cloths (the Ogre's
  four fur sims now really solve and skin: fur3 1.56 ms, fur4 0.65, ropu 0.52). The only
  restarts left are the Guardian Ape's fur at 17 s into its idle (a jump), 34 info lines once.
- 2026-10-11 GUARDIAN APE FUR DRAWN: its 12 fur cloths simulated 16k particles for nothing
  ("0 display meshes"). They have no MeshMeshDeform display: the sim buffer ("c5100_fur02",
  869 verts) is gathered straight into the display buffer of the same name and converted out,
  so vertex i of the FLVER mesh of that node name IS particle i. The exporter (cloth_export.rs,
  after the operator loop) now emits such a display when a buffer's name and vertex count
  match a FLVER mesh whose bind positions equal the sim pose (< 0.01): each vertex rides its
  own triangle's centroid frame at a corner (local (1,0,0), (0,1,0) or (-1,-1,0), exact for
  any deformation) with the FLVER bind normal taken into that frame (`inv3_apply`); the
  runtime needs nothing new. The gather into the display buffer is a `hclCopyVerticesOperator`
  (vertex i = particle i) or a `hclGatherAllVerticesOperator` whose `vertexInputFromVertexOutput`
  maps each display vertex to its particle (`#15#_c5100_neckfur`: 796 display verts from 762
  particles, seam duplicates); the exporter follows either. This was not just the Ape: a
  re-export of every character (scratchpad reexport_models.ps1, the extract.ps1 arguments)
  gave 108 new displays on ~40 chrs: hair (c1010, c1070, c1080, c1340, c1350, c1450, c5000,
  c5005, c5020/21, c7020 Demon of Hatred mane + beard, c7110, c7200/c7210, c7440), kimonos and
  hakama (c1180/c7470, c1191, c1260 monkeys' suso, c5430, c7210, c7400/01, c7520), bandages
  (c1030, c1040), the Shura's zukin, c5040's limbs, c5200's veils, horse hair, and Wolf's
  face hair FC_M_0200_hairB-E. One miss: `#12#_c1010_hatimaki` (88 verts, no FLVER mesh of
  that name / count; // gap). Checked in the sandbox (`SHINOBI_SANDBOX_ENEMIES=c5100`): the
  Ape's fur strips hang from the arms and back. Line-up: 139 cloths, 109 display meshes
  (was 78), 4.0 ms, no blow-up. The Demon of Hatred's kami1/kami2 restarted every frame
  straight from the bind pose (particle 10.2 m from the root vs a reach of 7.1 + 3 m: the
  mane rests further out than the crouched references reach): the blow-up threshold now also
  covers the rest pose (`rest_far`). The range cull measures camera-to-root plus that rest
  reach too (the Demon's root is 5 m behind its head: at 16 m its mane was carried in the
  bind pose, floating above the crouched body). Mind that the sandbox catalog shot
  (`SHINOBI_SANDBOX_SHOTS`) only moves Wolf and the orbit camera for the picture while the
  simulate step saw the camera 36 m from the Demon's root: cloth in those shots is culled
  unless `SHINOBI_CLOTH_RANGE=0`; the debug line now prints the camera distance, the range
  and whether the cloth is culled.
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
  `SHINOBI_CLOTH_NO_UPLOAD=1`.
- 2026-10-11 the Guardian Ape's fur (41 cloths on one character, three of 2000+ particles)
  cost 4.8 ms a frame with 0.6 ms per fur cloth outside every phase timer: the reference
  vertex pass (step 1) looked each entry's bone up by name (a String hash per entry, several
  thousand per fur cloth per frame) and converted the bone's world matrix once per entry.
  `ClothDef::refs` now holds an index into a `ref_bones` table (resolved once in `load`), and
  `simulate` builds one matrix per reference bone; the phase timer starts at the top of the
  cloth so "refs" covers that pass. Ape arena: simulate 4.79 -> 1.27 ms a frame, 60 fps in
  play (was 53).
- 2026-10-11 the every-frame restarts of c1060_mino1 and c1100_sleeve (c1110 / c1111 / c7520): their
  cloth containers hold two sim cloths; the exporter exported the first but took the
  hclMoveParticlesOperator pairs of the second (406 pairs for 59 particles), and a pair whose
  particle does not exist counted as "fixed particle jumped" every frame. cloth_export.rs keeps
  the operator with simClothIndex 0 and drops pairs beyond the particle count; cloth.rs drops
  such pairs at load. gap: the second sim cloth (the left sleeve, the other mino part) is not
  exported; its bones bind with err -1 and follow the animation. The teleport test for fixed
  particles is now half the cloth's bind extent (1 m at least): the Guardian Ape's 4 m fur
  restarted every few frames when it turned.
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
- 2026-10-11 (VXf) enemy effect audit: every extracted/enemies/c*.json TAE FFXID (with the
  ReferenceNode 2001 chains) against what src/fxr.rs draws. Script: scratch `fx_audit.py`
  (reads fxr/<id>.json, counts appearance / emitter / emitterShape / directionSpread /
  nodeMovement / particleMovement / nodeTransform types). Over all enemies, 13,000 configs.
  Added: NodeTranslation 15 (73 uses: c5400, c1310, c7100/7110, c1080 ...), NodeSpeed 120,
  NodeAccelerationRandomTurns 83 (gap: turn axes / distribution), RectangleEmitterShape 402
  (gap: distribution). Field meanings from reference/fxr src/fxr.ts.
  Not drawn, and nothing to draw: WindForce 10300 (88: pushes particles, no picture; gap: the
  push itself), WaterInteraction 10013 (3).
  Not drawn (gaps):
  - GPUStandardParticle 10000 (8): c7020 3043/3044/3047 embers 670240/670271, the carp
    c5050 650541, the Serpent 650133. Field docs exist, but not the GPU update (speed 15-22 m/s
    with random turns only make sense with the 10300 wind node).
  - Line 601 (1: c1700 120014): drawn since 2026-10-12 as one-pixel quads (fxr.rs 601; length along
    the particle direction, colour x mean(startColor, endColor); gap: the start -> end gradient,
    alphaThreshold). 120014 draws 200 lines (SHINOBI_FXR_LOG); its look is not checked against the game.
  - NodeAttachToCamera 46 (16: the Serpent c5010 set piece, c1000, c5000).
  Missing archives: 2, 3, 5, 21 (c1120, c7540-7600), 615102, 654061, 2255505.

## Hit-blood stains (2026-10-11)

Exe FUN_140888960 -> FUN_14088a7f0: DecalParam id = 600000000 + defender materialSfx1 * 1000 +
AtkParam decalBaseId1/2 (0 / 100) + atkPow_forSfx. Flesh 109, base 0, size L = 600109002. The rows
project from the hit along the blow (appearDirType 2, pitchAngle), red 191/54/19, mask 100030
(base 0) / 160000 (base 100), sizes 0.15 -> 3 m over 10 m. Code: gore.rs `hit_stain_id`,
`HitStains`; see HANDOFF 4.13.

- **Cloth setting and coat cleanup** (2026-10-11, the user: "their coat still in arena after i fight
  them", "turn down the cloth physics" -> an Options switch): Options "Cloth physics" On / Near
  only / Off = config `[video] cloth = "on"|"near"|"off"` (15 m, 6 m from the camera, none:
  every coat carried in its bind pose); `SHINOBI_CLOTH_RANGE` still overrides it. The cloth
  display meshes have no parent (world-space CPU vertices), so model.rs `follow_owner` despawns
  them with their actor and hides them while it is hidden (`FreeMesh`); before, a boss's coat
  stayed on the arena after the fight or an encounter swap.

## Lock-on mark (2026-10-11)

MENU_Lockon01 in menu/hi/01_common.tpf: standalone (no .layout entry), BC7 64 x 32, a white disc
about 10 px across centred at (16, 16), the rest clear. Scaleform clip Lockon_Ani_297 in
menu/01_000_fe.gfx plays it. hud.rs `LockMark` draws it at the locked target's dummy 220 with the
disc centre on the point, sized texture px x window height / 1080. 2026-10-12: the clip's timeline is decoded (shape 1042,
32 x 32 centred, the texture's left half untinted; FadeIn f1-16 scale 3 -> 1, alpha 0 -> 1; Loop
f16-83 alpha 1 -> 0.7383 (f50) -> 0.9922; FadeOut f84-89 alpha -0.2 / frame) and drawn so
(hud.rs lock_clip). The deathblow (MenuVFX State_1) and stealth (MarkerTop_334) clips likewise,
see HANDOFF "FE movie marks". dummy 220 as the anchor is still ours.

## HitEffectSfxAngleParam (2026-10-12, parked)

Param idx 0x3e, keyed by the HitEffectSfxParam row id; AtkParam+0x88 atkDir_forSfx (1 Upper, 2
Bottom, 3 Left, 4 Right) selects the angle triple; atkDir<Dir> can swap the row id but only
210 -> 211 uses it. Rows 200 / 290 / 300 / 310 / 550: Upper X -75, Bottom X +45, Left Y +65,
Right Y -65 (degrees); 570 / 610 half. Base orientation: +Z from the hit toward a source node, at
hitpos + hit[+0x60] x dir. FUN_14092b6c0 replaces the look-at basis with R = Ry(Y) Rx(X) Rz(Z)
(absolute world); the event path FUN_140b63fe0 uses Euler(E - A) relative to a character. The
two disagree, so vfx.rs keeps the plain look-at (gap).

## DetailBlend (character fabric layers), decoded and ported 2026-10-12

`Character_AMSN_[DetailBlend].shaderbdle` (shader/shaderbdle.shaderbdlebnd.dcx; the `_cloth`
bundle's pixel shaders are byte-identical), 25 Gbuf variants by the MTD's `GXFT_SATStaticSwitch`
(as M[Multiple], kb/map.md). Wolf (BD_M_9040): tops1 / tops1[a] / tops1_cloth switch 13, the
muffler and tilingset1 switch 1. The coat ("Court") is another shader,
`Character_AN_Blend_AMSN_[Fresnel]` (not decoded).
- Slots (all on uv0, each with its group's tiling, `group_N_CommonUV-UVParam`, inferred to feed
  FC_TextureTilingScale): base albedo 7, normal 0, metal 3 (.R); layer A (always) 1 / 6; B 2 / 5
  (mask: base normal B >= 1/22, wB = min((22 n0.b)^260, 1)); C 10 / 12 (wC = (1 - n0.a)^8);
  D 4 / 9 (wD = n0.a^8); back faces 8 / 13 / 11. No vertex-colour weights.
- Albedo: a1 x color_0 -> lerp to a2 x color_1 by (1 - wB) -> a10 x color_5 by wC -> a4 x
  color_2 by wD; then sat(albedo x a7 x 4.55). Normal: base FLV(n0) + FLV(layer normal xy, same
  weights) x float_10. Gloss sat(2 x n0.b x layer gloss) (layer gloss = n.b + float_0 / 1 / 15 /
  13). Metal layer = m3 + float_2 / float_8 / float_16 / float_5. Variants differ in which layers
  exist and which "replace" (blend after the a7 x 4.55 modulation): table in
  extracted/ref_shaders/detailblend.md.
- Wolf: tops1 float_10 0.5, color_* white; slot 1 / 6 FC_A_0000_Fabric_blend04, 10 / 12
  Fabric_blend02. Muffler color_0 0.851, color_1 (0.906, 0.894, 0.894), float_1 -0.5, group_3 UV
  [51, 17]; slots 1 / 6 Fabric_blend01, 2 / 5 Fabric_blend05.
- Why it matters: we draw a7 alone; BD_M_9040_tops_a averages 0.11 (sRGB), the fabric ~0.62, so
  the game's tops were read as ~1.7x brighter (wrong: their layer A, Fabric_blend04, is a flat
  0.2 linear grey, so 4.55 x 0.2 = 0.91, almost unchanged; the scarf's Fabric_blend01 is 0.37:
  about 1.4x brighter and warmer). Colours carry x Intensity (the engine multiplies, kb/map.md).
- Port: model format v6 (extractor `export_model`: `DETAIL_LAYERS`, `detail_params`; also the
  `_cloth` twin shader), model.rs `DetailData` / `SekiroExt` layer textures + `DetailUniform`,
  sekiro_material.wgsl `detail_layers`. v5 models load as before; `SHINOBI_NO_DETAIL=1` draws
  the base alone. Re-exported 2026-10-12: Wolf's model_c0000_bd_m_9040 and the 69 roster models
  with DetailBlend materials (c1010-c1700, c5000-c5430, c7010-c7600; list printed by
  scratch detail_models.py). Shots (SHINOBI_PHOTO, before = SHINOBI_NO_DETAIL):
  extracted/ref_frames/detail_{wolf,gen}_{before,after}_*.png; against real_002500 /
  real_005230 (Wolf small, back view): the scarf warmer and lighter, the tops nearly the same,
  still paler than the real orange-red (scene light). Nothing broken; the General's cloth shows
  the weave.
- Tiling (fixed 2026-10-12): each MTD texture entry (version 5) carries its own scale, the 2
  floats after the path (mtd.rs `Texture.scale`; the mtd/*.txt dumps leave them out). A non-zero
  scale is used; (0, 0) falls back to the group's `group_N_CommonUV-UVParam`. Wolf: tops1
  blend04 32 x 16, blend02 100 x 50; scarf blend01 18 x 6 (its cloth twin: (0, 0), group_2
  [18, 6]); tilingset1 blend01 8 x 4. Read as FC_TextureTilingScale (inferred: the cb1 index =
  the t-register, never seen being filled).
- Gaps: the SYSTEX_Dummy defaults for empty slots (white / flat / gloss 0.5 here),
  the "after" layers' gloss, back faces (variants 15-20), variant 19's order and 21's lone normal,
  o1 gloss / occlusion reading.

## Coat shader Character_AN_Blend_AMSN_[Fresnel], decoded and ported 2026-10-12

- Wolf's coat (BD_M_9040 Court, Court[a], their `_cloth` twins) and 66 MTDs in all (9000 / 9030
  coats, 9110 kimono, 9120 haori, many NPC kimonos). One pixel shader (no static switch);
  `_Gbuf_[A]` adds the alpha test. Decoded statically (D3DDisassemble).
- Slots, all on uv0 x the texture's own MTD scale: 2 base albedo (alpha = cutout), 7 base normal
  (B gloss, **A picks the fabric**), 0 reflectance -> metal R (empty: black), 3 / 1 fabric A
  (blend05) albedo / normal, 4 / 5 fabric B (blend01). Wolf: fabric 25 x 25 (the cloth MTDs tile
  blend05_n at 20), the base 1 x 1.
- Math: w = n7.a^10; normal = base + lerp(FLV(n5) x float_11, FLV(n1) x float_1, w);
  gloss = sat(2 n7.b lerp(n5.b, n1.b x float_3, w)); albedo = 4.55 a2 lerp(a4 color_3, a3
  color_1, w); then the "Fresnel": a rim tint of the albedo, fres = (1 - sat(N.V - float_9))^
  float_8 on the final normal, albedo = sat(lerp(albedo, lerp(color_2, albedo x color_0,
  float_5), fres)); back faces: sat(a4 x color_4) (no base, no rim); metal = sat(t0.r +
  float_0). Wolf: color_2 (0.78, 0.65, 0.57) x 0.5, float_8 2.36, float_9 -0.29 (rim 0.45 at
  grazing, gone above N.V 0.71); color_3 white x 0.6, color_1 white; color_4 (0.5, 0.42, 0.4).
- Port: model v6 variant byte 100 (exporter `FRESNEL`, `fresnel_params`; layer names a3, n1, a4,
  n5), model.rs `Detail.flags.x = 2`, sekiro_material.wgsl `fresnel_layers`. Front / back by the
  vertex normal, not by the winding (ours is reversed on the X-mirrored meshes).
- Cloth normals (fixed with it): the coat's cloths (bodybese01 / bodybase02 / bodybese03) and
  c7400's haori deform with the position-only hclObjectSpaceMeshMeshDeformOperator; the game
  rebuilds their normals afterwards (hclUpdateAllVertexFramesOperator, in BD_M_9040_c.hkx). Ours
  exported them as 0, so the coat drew with zero normals: that was the old black speckle.
  cloth.rs `deform_display` / `rebuild_normals` now rebuild them from the deformed triangles
  (area-weighted, on the bind normal's side). Test `coat_normals_after_deform`.
- Re-exported 2026-10-12 in one pass (exporter tools/sekiro-extract, `cargo test --release`
  passed): Wolf's model_c0000_{am_m_9000, bd_m_9040, fc_m_0200, lg_m_9000, wp_a_0300} and the 73
  roster models with DetailBlend or Fresnel materials (the 69 before plus c1110, c5410, c7520,
  c7530). Old files: scratchpad models_v5_backup. Shots: extracted/ref_frames/
  coat_wolf_{before,after}_*.png. Before: the coat brown with black speckle (zero normals). After:
  clean brown cloth with a fine weave, no speckle; the sleeves and skirt lit evenly.
- Gaps: the pow 2.2 flag for colours (taken as linear), o1 gloss / occlusion reading, the Fwd /
  Paint passes, the alpha dither of `_Gbuf_[A]` (alpha-to-coverage here).
