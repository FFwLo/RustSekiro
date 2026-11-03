//! The rest effect at a Sculptor's Idol (#153): Rest plays the idol's burst and darkens the
//! world, and the world resets while it is dark (`Duel::idol_rest`, the sim's clock).
//!
//! The burst is the idol's own: Wolf's rest clip asks the idol (its TAE's SetEzStateRequestID 20
//! at [`REST_BURST`]) and the idol's clip 100 spawns FFX [`REST_FFX`] on its dummy 100, the
//! model's origin: a flash, rings, sound 1020 and a dark disc round the idol. The dark over the
//! screen follows that effect's own black sprites ([`rest_dark`]): it rises over 0.5 s, holds
//! while the foes come back and the bars refill (the sim's [`REST_RESET`]), and lifts by 3 s,
//! under the idol menu, a covering screen of its own while it shows. A rest cut short (Wolf
//! taken off the idol) lifts the dark from wherever it had got to at the same rate.
//! gap: the screen dark is the effect's curve on a black overlay (docs/FIDELITY.md); the game's
//! own fade for the rest request is not traced in the exe.
use bevy::prelude::*;
use sekiro_sim::duel::idol::{
    REST_BURST, REST_DARK_HOLD, REST_DARK_OUT, REST_FFX, REST_RESET, RestClock, rest_dark,
};
use sekiro_sim::kit::Kit;

use super::Sim;
use super::idol::{Idols, kit_change};
use crate::fxr::{FxrPlayer, IDOL_REST};

/// The black's strongest alpha: FFX 1022's dark sprites' `alphaMultiplier`.
const MOST: f32 = 0.99;
/// How often the dark's alpha is logged while it changes (s), for live checks.
const LOG_EVERY: f32 = 0.25;

/// The kit as Rest was taken, logged against the kit once the world has reset.
#[derive(Resource, Default)]
pub(super) struct KitBefore(pub Option<Kit>);

#[derive(Component)]
struct Dark;

pub(super) fn install(app: &mut App) {
    app.init_resource::<KitBefore>()
        .add_systems(Startup, spawn_dark)
        .add_systems(Update, rest_effect.run_if(crate::menu::active));
}

/// The dark is a covering screen of its own (`crate::game_ui::Covers`), so the idol menu's cover
/// does not hide it, and it is shown only while it is dark: up, it hides the HUD as the menu does.
fn spawn_dark(mut commands: Commands) {
    commands.spawn((
        Dark,
        crate::game_ui::Covers,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::NONE),
        // Over the world, under the idol menu (110).
        GlobalZIndex(105),
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
}

/// The dark's share (0 clear to 1 full) this frame: the rest's curve while one is under way,
/// else `last` lifting at the curve's own rate over `dt`.
fn shade(rest: Option<RestClock>, last: f32, dt: f32) -> f32 {
    match rest {
        Some(r) => rest_dark(r.secs),
        None => (last - dt / (REST_DARK_OUT - REST_DARK_HOLD)).max(0.0),
    }
}

/// What the rest's frame-to-frame state carries: the clock last seen, the shade, the bursts
/// played and when the shade was last logged.
#[derive(Default)]
struct Seen {
    secs: Option<f32>,
    shade: f32,
    bursts: i32,
    logged_at: f32,
}

#[allow(clippy::too_many_arguments)]
fn rest_effect(
    time: Res<Time>,
    sim: NonSend<Sim>,
    idols: Res<Idols>,
    fxr: Option<ResMut<FxrPlayer>>,
    mut before: ResMut<KitBefore>,
    mut seen: Local<Seen>,
    mut darks: Query<(&mut BackgroundColor, &mut Visibility), With<Dark>>,
) {
    let rest = sim.duel.idol_rest();
    let now = rest.map(|r| r.secs);
    // The burst, once, the frame the clock passes it.
    if let Some(secs) = now
        && secs >= REST_BURST
        && seen.secs.is_none_or(|s| s < REST_BURST)
    {
        let at = sim.duel.idol().and_then(|i| idols.origins.get(i)).copied();
        match (at, fxr) {
            (Some(at), Some(mut fxr)) => {
                seen.bursts += 1;
                fxr.one_shot(IDOL_REST, REST_FFX, seen.bursts, at);
                info!(
                    "idol: rest burst FFX {REST_FFX} at {:.2?} ({secs:.2} s)",
                    at.translation
                );
            }
            _ => warn!("idol: rest burst FFX {REST_FFX} not played (no idol or no FXR player)"),
        }
    }
    if let Some(r) = rest
        && r.reset
        && let Some(kit) = before.0.take()
    {
        info!(
            "idol: rest: world reset at {:.2} s (shade {:.2}): {}",
            r.secs,
            rest_dark(r.secs),
            kit_change(&kit, &sim.duel.kit)
        );
    }
    if rest.is_none() && seen.secs.is_some_and(|s| s < REST_RESET) {
        before.0 = None;
        info!("idol: rest cut short before the reset; the dark lifts");
    }
    seen.secs = now;
    let shade = shade(rest, seen.shade, time.delta_secs());
    let elapsed = time.elapsed_secs();
    if shade != seen.shade && (elapsed - seen.logged_at >= LOG_EVERY || shade == 0.0) {
        seen.logged_at = elapsed;
        info!(
            "idol: rest dark {shade:.2} at {} s",
            now.map_or("-".into(), |s| format!("{s:.2}"))
        );
    }
    seen.shade = shade;
    for (mut c, mut v) in &mut darks {
        c.set_if_neq(BackgroundColor(Color::BLACK.with_alpha(shade * MOST)));
        v.set_if_neq(if shade > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dark_follows_the_rest_and_lifts_from_where_a_cut_left_it() {
        let at = |secs| shade(Some(RestClock { secs, reset: false }), 0.0, 0.0);
        // Clear until the burst, full by the reset, still full until the hold ends, clear at
        // the end of the curve.
        assert_eq!(at(REST_BURST), 0.0);
        assert!(at(REST_BURST + 0.25) > 0.4 && at(REST_BURST + 0.25) < 0.6);
        assert_eq!(at(REST_RESET), 1.0);
        assert_eq!(at(REST_BURST + REST_DARK_HOLD), 1.0);
        assert!(at(REST_BURST + 2.5) < 0.6);
        assert_eq!(at(REST_BURST + REST_DARK_OUT), 0.0);
        // Cut short at half dark: it lifts from there at the curve's rate, never jumping.
        let lift = REST_DARK_OUT - REST_DARK_HOLD;
        let mut s = shade(None, 0.5, 0.25 * lift);
        assert!((s - 0.25).abs() < 1e-6);
        s = shade(None, s, lift);
        assert_eq!(s, 0.0);
    }
}
