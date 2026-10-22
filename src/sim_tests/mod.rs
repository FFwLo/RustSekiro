//! Headless simulation of real combat exchanges: the game's own plugins run
//! under MinimalPlugins with a fixed 60 Hz clock and scripted input, so timing
//! rules can be verified end to end without a window. Needs extracted/combat_data.json.

pub(crate) use bevy::prelude::*;
pub(crate) use bevy::time::TimeUpdateStrategy;
pub(crate) use std::time::Duration;

pub(crate) use crate::actor::{Actor, ActorPlugin};
pub(crate) use crate::combat::CombatPlugin;
pub(crate) use crate::config::ConfigPlugin;
pub(crate) use crate::data::{Combat, DataPlugin, TAE_FPS};
pub(crate) use crate::enemy::{Enemy, EnemyPlugin};
pub(crate) use crate::hud::CombatLog;
pub(crate) use crate::player::{Action, PadInput, Player, PlayerPlugin, CAPSULE_HALF_HEIGHT};

// Index of the parts (tests in file order). Grep for a test name to find its part:
// part01: 49 tests, timed_guard_deflects_and_bounces_the_enemy .. posture_break_staggers_then_a_second_hit_knocks_wolf_down
// part02: 45 tests, locked_slash_lunges_toward_a_far_target .. a_jump_press_leaves_the_hold_art
// part03: 44 tests, hold_art_sheathes_and_releases .. a_feigning_wolf_is_dropped_by_the_enemy
// part04: 24 tests, crouching_cuts_how_far_enemies_see_wolf .. a_blocked_shuriken_plays_the_guard_reaction
// part05: 35 tests, a_guarding_enemy_takes_its_npc_posture_rate .. blue_lightning_leaves_an_air_art_going
// part06: 35 tests, lightning_on_the_ground_shocks_wolf .. the_bandits_torch_sets_wolf_on_fire
// part07: 43 tests, reigniting_a_burn_refreshes_it .. no_gourd_while_guarding_and_a_tool_switch_in_the_air
// part08: 49 tests, a_jump_in_the_break_pose_vaults_without_killing .. a_patrol_point_with_a_zero_region_is_reached_by_is_arrived
// part09: 47 tests, a_blocked_hit_does_not_wake_a_held_enemy .. an_attack_out_of_a_run_keeps_the_runs_momentum
// part10: 38 tests, an_attack_at_the_run_loops_wrap_keeps_the_momentum_too .. hit_stop_holds_posture_regen_and_sp_effect_timers
// part12: 12 tests, wall hug, ledge hang and wall jumps (docking.rs)
// part13: atlas movement pass tests (atlas ids in the names)
// part14: atlas combat pass tests (atlas ids in the names)
// part15: atlas vfx + lighting pass tests (atlas ids in the names)
// part16: atlas bosses pass tests (atlas ids in the names)
// part17: replay_recording (ignored; tools/rec/replay.py and enemies.py drive it), a map event's anim set, a soldier under Wolf's sword guards
// part11: 15 tests, a_mid_stride_loop_swap_does_not_burst_its_sounds .. throw_direction_effects_come_from_the_material_row

mod part01;
mod part02;
mod part03;
mod part04;
mod part05;
mod part06;
mod part07;
mod part08;
mod part09;
mod part10;
mod part11;
mod part12;
mod part13;
mod part14;
mod part15;
mod part16;
mod part17;
pub(crate) use part01::*;
pub(crate) use part02::*;
pub(crate) use part03::*;
pub(crate) use part04::*;
pub(crate) use part05::*;
pub(crate) use part06::*;
pub(crate) use part07::*;
pub(crate) use part08::*;
pub(crate) use part09::*;
pub(crate) use part10::*;
pub(crate) use part11::*;
