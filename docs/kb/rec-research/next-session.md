# Next recording session: checklist

The new hooks (2026-10-21) have not run in the game yet. Each recording below is one
`python -m tools.rec new <name> --note "..." --cats ...` run (see docs/kb/recordings.md for the steps).

## 1. Start and check
- Steam running, then from `..\me3`: `bin\me3.exe launch -p sekiro-rec.me3`.
- `me3\sekiro-rec\sv1_rec_hooks.log`: every hook line `ok`. A `SKIPPED` line (first bytes differ) costs
  nothing; a crash at load means one hook is bad: the session turns hooks off in `sv1_rec_hooks.txt`
  by halves until it starts (the old list is `sv1_rec_hooks_20261021a.txt.bak`).
- New this time: hks_state, grapple, cutscene, talk, vfx_end, pad_play, anim, pose, bullet_pos,
  obj_broken, obj_destroy, objact, fmod_param, fmod_param_i, fmod_global, noise, stick.

## 2. Recordings (short, one topic each)
| name | what to do | checks |
|---|---|---|
| hooks_smoke | 60 s: walk, jump, one attack, one grapple | every new hook writes lines; anim / pose / stick fill |
| play_walk | stand still; the session runs `play <rec> walk_check` | scripted stick moves Wolf (fwd 2 s, right 1 s) |
| objects | break pots / crates, open a door or gate, pull a lever, open a chest | obj_broken, obj_destroy, objact rows |
| sound_mix | walk from outdoors into a building and back; one deathblow | fmod_global reverb / deathblow values |
| noise | near an unaware enemy: crouch walk, walk, run, land a jump, get noticed | noise rows per action, the enemy's alert |
| stick | slow tilt 0 -> full in 8 directions, walk key, a full look-stick circle | stick rows: deadzone, walk halving |
| slide | sprint -> crouch slide 3 times holding the step, then let go | when the slide hands back to moving (ours: 3-7 frames early) |
| landings | vertical jumps on flat ground and on slopes, a 5 m and a 15 m drop | land-ready height and speed (env 248 probe reach) |

## 3. After
- `python -m tools.rec summary <rec> --cat every`, then `python -m tools.rec replay <rec>` for the
  movement ones, then `python -m tools.rec list`.
- New data without a summary category yet (objects, FMOD, noise, stick): add one once real rows exist.
