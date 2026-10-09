# NEXT list (history and open items)

1. [x] Camera from data: CameraParam / LockCamParam (distance, height, lock-on behaviour, FOV).
2. [x] Hit stop / hit feedback from data (AtkParam / HitEffect params / TAE events) incl. deflect freeze.
3. [x] Enemy defence: c1020 guard/deflect of player attacks + reactions (deflected, guard, damage, posture break,
       stagger) from c1020 TAE + NPC HKS (extracted/hks_src/c1020.lua) + NpcParam guard fields.
4. [x] Real animation playback + meshes: decode hkaSplineCompressedAnimation (+skeleton) and draw the skeleton, then
       FLVER mesh + textures for Wolf and the Samurai General (chrbnd/partsbnd/texbnd).
5. [x] Hitboxes from dummy polys (FLVER dmy + animated bones) instead of reach approximations.
6. [x] Ghidra: jump launch/gravity, ActionRequest input buffer, the 0.8/0.7 regen level input.
7. [x] Enemy AI: decompile c1020 Lua AI (script/aicommon + c1020 battle logic) and port its decisions.
8. [x] Remaining player rules: deathblow (ThrowParam), mikiri counter, perilous attacks, guard walk, air deflect.
9. [x] Sound: extract deflect/guard/hit SFX from the FMOD banks (sound/).
Round 2 (after NEXT 1-9):
10. [x] Superseded (2026-10-08): the game's own FMOD plays the events (fmod.rs, init flag fixed); armour / clash /
        floor ids come from the exe's param lookups (kb/sound.md); event -> sample by length (sound_events test).
11. [x] Exe: writer of ActionRequest accept mask (+0xd0) and clear flag (+0x188) -> exact input buffer windows.
12. [x] Exe: posture value after an unused deathblow window (no restore; regen through the collapse + debt).
13. [x] More enemies through the same pipeline (e.g. Ashina soldier c1000 / c1010 with their battle Lua).
        Plan: (1) unpack chr/c1000.* + its map AI luabnd (script/ai/*.luabnd holding the c1000 battle goal) and
        sound/c1000.*; (2) sekiro-extract export: a CharConfig for c1000 (NpcParam row, behavior/atk params, think id
        from NpcThinkParam); (3) game: replace the c1020 constants (NpcParam 10219000 in combat.rs/enemy.rs/actor
        setup, THINK_ID 10200000, BATTLE_GOAL 102000, model/anim bin names) with a config enemy.chr selection.
        Candidates (NpcParam): c1010 Ochimusha one-handed sword 10100000 (hp 195, posture 75; basic sword enemy),
        c1050 Spear Monk 10500000, c1070 Shura Samurai 10700000. c1000 is only dummies. Map AI luabnds
        (extracted/script/m*.luabnd.d) are unpacked; decompile the battle goal from NpcThinkParam with DSLuaDecompiler.
14. [~] Healing gourd (done) (item use TAE + SpEffect) and combat arts (e.g. Whirlwind Slash) for the full player kit.
Round 3 (2026-10-07, Wolf feel; ideas from studying the ER/Mirror's Edge/FNV rewrites):
15. [x] Swept hit test: sample the attack capsule between the previous and current tick (fast slashes tunnel today).
16. [x] TAE 760 BoostRootMotionToReachTarget: root-motion scale chr+0x2fc from FUN_140842980 (asm: needed, return is
        in xmm0). Fields ReferenceDist/RangeMin/RangeMax/ArriveAngle/ArriveDist (+0x338..+0x348); debug path uses Max/Ref.
17. [x] TAE 226 SetKnockbackPercent: knockback module +0x40 = percent/100 on the actor running the anim (scales the
        knockback it receives?) - confirm the consumer of +0x40 in the decompile, then apply.
18. [x] Behaviour-graph blend data from c0000.hkx (hkbBlendingTransitionEffect durations, locomotion blender weights,
        play rate by speed) -> real crossfades and locomotion blending (gap 8).
        Survey (hkxdump c0000.hkx): only 3 hkbBlendingTransitionEffect + 10 CustomTransitionEffect (duration@184), so most
        crossfades really are TAE Blend events. Worth reading next: hkbClipGenerator x2566 (playbackSpeed@184,
        cropStart/End, startTime, enforcedDuration) -> any clip not at speed 1.0 plays wrong today; the 2
        hkbBlenderGenerators (blendParameter@156, children weights@80) = locomotion blend; CustomManualSelectorGenerator
        x2041 (offsetType/animId, generatorChangedTransitionEffect). Needs a field reader in tools/sekiro-extract/src/hkx.rs.
        Checked 2026-10-08 (sekiro-extract objects): all 2566 c0000 and 388 c9997 hkbClipGenerators have playbackSpeed 1,
        no crop, startTime 0, enforcedDuration 0 - every clip plays as authored.
19. [x] gamedb: join split Ghidra signatures (return type on its own line) before indexing (~6k functions missed).
20. [x] Name the exe from its own RTTI (Ghidra RecoverClassesFromRTTIScript on a copy) for faster research.

## Open items from the log

SWORDPLAY NEXT (for the next session): (a) TAE 700 twists (vertical aim of slashes at height differences, idle torso
  twist to the lock target); (b) TAE 151 look limits (+0x4c..+0x58, used by FUN_140741?/14074.c:401); (c) user feedback.
