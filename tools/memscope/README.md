# Live checks with memscope-mcp

Last-step verification only (static research stays the source of truth). Start the game with
`tools/live_check.ps1`, load a save near an enemy, then Claude attaches memscope (`attach sekiro.exe`)
and runs a check; you perform the action once. Results go to `docs/kb/<topic>.md` with the date.

Addresses come from the static decompile of the de-Arxan'd exe (`extracted/decomp`); confirm them in
the live image with an AOB `scan` of the function's first bytes before hooking.

| Check | Question (gap) | Action in game |
|---|---|---|
| art_release | What fires BEH_A_GROUND_SP_ATTACK_RELEASE (art early release: Combo1Release / SprintSpecialAttackRelease)? | Use a combat art, let go of attack early |
| cloth_damping | Is hclSimClothData globalDampingPerSecond applied as (1 - d)^dt per step? | Stand still, then run, with the scarf moving |
| bend_stiffness | Field order / formula of hclBendStiffnessConstraintSet links | Any cloth movement |
| twist_new_target | When does CustomLookAtTwist use newTargetGain instead of onGain? | Lock on, switch target |

Scripts for each check are added here as `<check>.lua` once written and tested against the live game.

WARNING (2026-10-08): inline hooks on FUN_1408439a0 crash sekiro.exe on the first call (posture_hook.lua). Use per-frame reads only.
WARNING (2026-10-08): the PRE hook on FUN_140840870 (guardcut_hook.lua) also crashed sekiro.exe. memscope inline hooks are not usable on this game: per-frame reads only.
