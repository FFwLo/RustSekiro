# Manual test, 2026-10-13

Bugs found while playing the release build are logged here and routed to an owner on RESUME.
Owner: **BF** = bug fixes (player, combat, HUD, camera, sound), **BO** = Bosees (enemies, AI, damage,
deathblows), **VM** = VFX and Maps (effects, map, sky, lighting; map lighting paused).

Build: `target/release/sv1.exe`, built 2026-10-13 from the uncommitted tree on top of 265e142.

| # | Where / how | Owner | Status |
|---|---|---|---|
| 1 | Debug option: Spirit Emblems never run out (F1 cheats, next to god mode / infinite posture) | BF | done (F1 PLAYER "infinite Spirit Emblems", or config `[player] infinite_emblems = true`) |
| 2 | Wolf's cape and the bosses' cloth (robes, scarves, coats) move differently from the game | VM | fixed (VM), retest: cloth now simulates in model space like the exe (no world drag; Wolf's cape 5-25 deg in a run like the clip, was 30-60), all characters; bosses: Genichiro's mantle checked against the clips, armour skirts / kimonos now use the game's compressible links (69 cloths; fewer flips), rare lunge flips remain; HANDOFF 4.44 |
| 3 | Boss phase changes (bars, Genichiro -> Tomoe, Isshin, Monk phantoms, Ape, Butterfly, Emma) | BO | fixed (BO, sim): every boss row reaches its defeat (Isshin Ashina's and Emma's defeat events were blocked by world flags); new tests for Isshin's spear / lightning phases, the Ape's headless rise, Lady Butterfly's change (HANDOFF 4.42 "Boss transitions"); in-game check |
| 4 | Effects from arts / prosthetics / skills stay on screen after the move ends | VM | fixed (VM): effects from IsRestrictToDummyPoly events now end with their TAE event (game External(0)), HANDOFF 4.44 |
| 5 | Genichiro / Tomoe lightning look (and timing) differs from the game | VM (look), BO (timing/AI) | look partly fixed (VM): bolts now jagged like the game (tracer flipbook fix); open: in-fight check of the sword lightning, colour (lighting paused); Wolf's charge glow added (status.rs electro_glow, retest); sword lightning checked in a fight: the bolt is now a forked web like the clip (particle random turns, retest), the blade charge is orange (tonemap, paused); timing BO checked (sim): the Way of Tomoe casts 3043 when Wolf is knocked down (AI 109031 / 110125, 15 s cooldown), bullets 986 / 988 at 1.17 / 0.83 s, reversal window as before; no timing change needed (HANDOFF 4.42) |
| 6 | The final deathblow plays twice / needs an extra one at the end of the fight (Isshin Sword Saint, and others) | BO | fixed (BO, sim): the game ends with two blows (Todome, kneel, finisher from behind; finishers clip 5:32-5:45), but our map script cut the finisher with a second kneel, so it never ended; NPC EzState requests now follow c9997.hks (no event transition in ThrowDef12900); test isshins_finisher_ends_the_fight_once; in-game check |
| 7 | menu.rs watch_fight opens the Won screen 4.0 s (invented) after Handle Boss Defeat, cutting the finisher anims | BF | fixed (BF): Won waits until Wolf is free and every enemy clip has ended (cap 30 s); the defeat banner (Shinobi Execution / Immortality Severed) shows from EMEVD 2003[74] without a pause; retest |

## Retest after the Fable handler round 1 (2026-10-14, release built from the uncommitted tree)
- Isshin Sword Saint / Way of Tomoe: the red 危 shows on their 986 sweeps; the Ape's phase-1 grabs too.
- Tomoe's lightning arrow on the ground shocks Wolf (GroundDamageElectricShockWeak, HP loss).
- Lightning Reversal on Genichiro / Tomoe: about a fifth of the bar and posture damage.
- Gyoubu: the last deathblow's banner comes after the mounted finisher (about 6.6 s), the last dot goes dark at the hit.
- Castle Genichiro: the Tomoe swap starts after the deathblow hit, not at the lunge.
- Old Dragons: the shared bar shows; when it empties the Divine Dragon phase starts (no banner / restart).
- Boss music keeps playing across Genichiro -> Tomoe and Emma -> Isshin.
- Grapple onto a drifting Gyoubu / Ape lands on him.

## Retest after the Fable handler round 2 (special clash rows)

- Castle Owl (50600000): deflect his leaping / dash slash out of the smoke (the warp slashes). He
  should take a different, longer stagger (AttackBoundEnemy2, a000_008603) and answer with the
  perilous spin sweep (3090) or 3091 about half a second in. Jump it.
- Butterfly: deflect a kick. She staggers into her special bound and her Kengeki answer follows
  (200227 / 200228 branches).
- Gyoubu: deflect a charge (category 8): the AttackBoundEnemy3 stagger.
- Any enemy in its guard stance (200230, e.g. the Ochimusha's Common_Parry): hit it and it plays
  GuardDamageSpecial; its answer should differ from a plain block.
- Momentum into actions: attack, step or guard straight out of a sprint / run. Wolf should carry
  the run's speed through the first fraction of a second (no dead stop on the first frame), as in
  the game. Also check attacks from standing still start a touch slower than before.
- Gyoubu's defeat banner: after the mounted finisher, the Shinobi Execution banner should come
  about 9.5 s after the lunge (he falls from the horse first), not right after the stab.
- HUD layout (from the HUD movie): resurrection lotus above Wolf's HP bar (bottom left), the
  Spirit Emblem count and the gourd with its stock bottom right beside the sword, a skill-point
  blob with an empty bar top right, the Sen coin with 0 right middle. Say if anything sits wrong
  against the real game or reads badly on bright ground.
- Enemies turn before they move: an enemy sent to a point (or at you) that is behind it plays a
  pivot turn in place first, then runs, instead of steering in a wide arc. Also watch for any
  enemy that now stalls in a turn or turns when it should not (e.g. right after a step or hit).

