//! Resting at a Sculptor's Idol (#128): Wolf kneels, prays, rests and stands up with the game's
//! own behaviour states (StatueAttachStart a000_710310, Loop 710320, Rest 710340, End 710330).
//! The engine only sends their events (`W_StatueAttach*`); c0000.hks moves on from Start and
//! Rest to the Loop, and from End to standing, by itself.
//!
//! The action button at an idol's kneeling spot kneels Wolf there (unless a map item is nearer,
//! `super::pickup`); while he is at it his own
//! input is held. Kneeling refills Vitality and makes the idol the checkpoint: the next respawn
//! puts Wolf at its warp point ([`IdolSpot::checkpoint`]). Rest refills him again, refills the
//! Healing Gourd and restocks the carried goods from the Storage (`Kit::restock`, #186; a
//! respawn after death does too) and brings the map's foes back on the next step.
//! After FwLo's RustSekiro `src/idol.rs` (Fireguy32's idols): there kneeling leaves the foes
//! alone and only Rest revives them.
//! gap: no resurrection yet to refill.
//!
//! Rest is not instant (#153): the rest clip plays the idol's burst and the world goes dark
//! ([`rest_dark`]), and the reset happens only once it is fully dark ([`REST_RESET`]), so the
//! foes reappear and the bars refill unseen; the dark then lifts while Wolf still rests.
//! [`Duel::idol_rest`] is the effect's clock for the game's burst and fade.
//!
//! The first kneel at an idol lights it (#152): its bonfire entity goes into the kit's found
//! idols (`Kit::found_idols`, #148), which a respawn keeps, and Wolf stays at the idol in
//! [`IdolPhase::Lighting`] while the game's "SCULPTOR'S IDOL FOUND" banner plays
//! ([`LIGHTING_SECS`]); only then does he pray and the idol menu open. Kneeling at a lit idol
//! goes straight to the prayer.
//!
//! Travel (#148): from the prayer, [`Duel::travel_to_idol`] plays the warp state
//! (StatueAttachWarp a000_710360) and [`WARP_CUT`] into it Wolf leaves. To an idol of this
//! play world he arrives as after a death: at its warp point, the world reset (Vitality, the
//! gourd and goods, the foes back) and the idol the checkpoint. To one on another map the sim
//! only hands the idol over ([`Duel::take_departure`]); the game loads that map.

use super::Duel;
use super::pickup::ActionTarget;
use crate::deathblow::Placement;
use crate::input::InputFrame;

/// How near an idol's kneeling spot Wolf can kneel (m across, m up or down).
const REACH: f32 = 1.3;
const REACH_Y: f32 = 0.6;

/// How long lighting an idol holds Wolf before he prays (s): the `BonfireLit` clip of the text
/// effects in `menu/01_000_FE.gfx` (the "SCULPTOR'S IDOL FOUND" banner), played at 60 frames a
/// second from its `Normal` label on frame 1 to its last frame, 197.
pub const LIGHTING_SECS: f32 = (197.0 - 1.0) / 60.0;

/// How far into the warp clip (StatueAttachWarp, 6.3 s) Wolf leaves for the chosen idol (s):
/// FwLo's RustSekiro `src/idol.rs` `WARP_CUT`, where the game cuts to its loading screen.
pub const WARP_CUT: f32 = 2.0;

/// When Rest's burst plays, seconds after Rest is taken: Wolf's rest clip (StatueAttachRest
/// a000_710340) sends its TAE's SetEzStateRequestID 20 at 0.2 s, and the idol answers with its
/// own clip 100, whose TAE spawns the one-shot FFX [`REST_FFX`] on its dummy 100 at once
/// (docs/FIDELITY.md: the request-to-clip link is inferred, not traced in the exe).
pub const REST_BURST: f32 = 0.2;
/// The idol's rest burst: FFX 1022 (common effects), its flash, rings and sound 1020, and the
/// dark around the idol.
pub const REST_FFX: i32 = 1022;
/// The dark's curve, seconds after the burst: FFX 1022's black sprites (its 25 m dark disc,
/// node 1/7/0) rise from clear over 0.5 s, hold to 2.0 s and are gone at 3.0 s.
pub const REST_DARK_IN: f32 = 0.5;
pub const REST_DARK_HOLD: f32 = 2.0;
pub const REST_DARK_OUT: f32 = 3.0;
/// When the world resets, seconds after Rest is taken: the moment the dark is full.
pub const REST_RESET: f32 = REST_BURST + REST_DARK_IN;
/// When the rest effect is over, seconds after Rest is taken: the dark has lifted.
pub const REST_END: f32 = REST_BURST + REST_DARK_OUT;

/// How dark the rest makes the world (0 clear to 1 full), `secs` after Rest is taken.
pub fn rest_dark(secs: f32) -> f32 {
    let t = secs - REST_BURST;
    if t <= 0.0 || t >= REST_DARK_OUT {
        0.0
    } else if t < REST_DARK_IN {
        t / REST_DARK_IN
    } else if t <= REST_DARK_HOLD {
        1.0
    } else {
        (REST_DARK_OUT - t) / (REST_DARK_OUT - REST_DARK_HOLD)
    }
}

/// A rest under way: seconds since Rest was taken, and whether the world has reset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestClock {
    pub secs: f32,
    pub reset: bool,
}

/// Where Wolf is at an idol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdolPhase {
    Kneeling,
    /// The first kneel at an unlit idol, while its "found" banner plays ([`LIGHTING_SECS`]):
    /// he kneels, then prays, held and with the menu shut.
    Lighting,
    /// The prayer loop: the idol menu is open.
    Praying,
    Resting,
    /// The warp clip of a Travel, until Wolf leaves ([`WARP_CUT`]).
    Traveling,
    Leaving,
}

/// One idol of the play world.
#[derive(Debug, Clone, Copy)]
pub struct IdolSpot {
    /// Where Wolf kneels, facing it.
    pub kneel: Placement,
    /// Where a respawn or a Travel here stands him: the idol's warp point.
    pub checkpoint: Placement,
    /// Its BonfireWarpParam `bonfireEntityId`, what the kit's found idols hold.
    pub entity: i32,
}

/// Where a Travel goes: an idol of this play world (an index of its spots), or the bonfire
/// entity of one on another map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Here(usize),
    Away(i32),
}

/// The play world's idols and Wolf's place at them.
#[derive(Debug, Default)]
pub(super) struct Idols {
    spots: Vec<IdolSpot>,
    /// The idol Wolf was sent to kneel at, until he stands again.
    at: Option<usize>,
    /// Where he comes back after a death, once he knelt at an idol.
    pub(super) checkpoint: Option<Placement>,
    action_held: bool,
    /// Rest was taken: the foes come back on the next population step.
    pub(super) revive: bool,
    /// Seconds since Wolf lit the idol he is at, while its banner plays.
    lighting: Option<f32>,
    /// The Travel under way and the seconds since its warp began.
    travel: Option<(Destination, f32)>,
    /// The rest under way, until its dark has lifted.
    rest: Option<RestClock>,
    /// The idol on another map a Travel left for, until the game takes it.
    departure: Option<i32>,
    /// A Travel within the play world is arriving: the respawn it asked for resets the world
    /// but ends no fight (a fight's end is a death or a defeat, never a Travel).
    pub(super) arriving: bool,
}

impl Duel {
    pub fn set_idols(&mut self, spots: Vec<IdolSpot>) {
        self.idols.spots = spots;
        // `at` indexes the old spots; the found idols are keyed by entity, not index, and stay.
        self.idols.at = None;
        self.idols.lighting = None;
        self.idols.travel = None;
        self.idols.rest = None;
    }

    /// The play world's idols.
    pub fn idol_spots(&self) -> &[IdolSpot] {
        &self.idols.spots
    }

    /// Stands Wolf at idol `i`'s warp point and makes it the checkpoint: where a run that
    /// Travel started on another map begins (`--at-idol`). It only places him: the kit that
    /// Travel carried over (`--kit`) was restocked before it left, so nothing is given twice.
    pub fn start_at_idol(&mut self, i: usize) {
        let Some(spot) = self.idols.spots.get(i) else {
            return;
        };
        let p = spot.checkpoint;
        self.player.body.position = p.position;
        self.player.body.velocity = [0.0; 3];
        self.player.body.fall_top = p.position[1];
        self.player.snap_yaw(p.yaw);
        self.idols.checkpoint = Some(p);
    }

    /// The idol whose kneeling spot Wolf is at, if he can kneel now (the "Rest" guide).
    /// An item nearer than the idol takes the press instead ([`Duel::action_target`]).
    pub fn idol_in_reach(&self) -> Option<usize> {
        match self.action_target()? {
            ActionTarget::Idol(i) => Some(i),
            ActionTarget::Pickup(_) => None,
        }
    }

    /// The nearest idol whose kneeling spot Wolf's feet are at, and how far it is.
    pub(super) fn idol_near(&self) -> Option<(usize, f32)> {
        let feet = self.player.body.position;
        self.idols
            .spots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let [x, y, z] = s.kneel.position;
                let d = (x - feet[0]).hypot(z - feet[2]);
                (d < REACH && (y - feet[1]).abs() < REACH_Y).then_some((i, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Whether idol `i` of the play world is lit: Wolf has knelt at it once (its bonfire entity
    /// is in the kit's found idols). Its guide reads "Rest" then, "Commune with Sculptor's Idol"
    /// before (#187).
    pub fn idol_lit(&self, i: usize) -> bool {
        self.idols
            .spots
            .get(i)
            .is_some_and(|s| self.kit.found_idols.contains(&s.entity))
    }

    /// The idol Wolf kneels or prays at.
    pub fn idol(&self) -> Option<usize> {
        self.idols.at
    }

    /// Standing or crouching, alive, on the ground and in no deathblow or throw.
    pub(super) fn can_act(&self) -> bool {
        let runtime = &self.player.behavior.runtime;
        let state = runtime.main_state_name();
        (state.starts_with("Stand") || state.starts_with("Crouch"))
            && !runtime.has_pending_events()
            && self.player.body.grounded
            && !self.rules.fighters[0].dead()
            && self.respawn_in.is_none()
            && self.deathblow.is_none()
            && self.enemy_throw.is_none()
    }

    /// Kneels Wolf on the action press at an idol and holds his input while he is at one: the
    /// frame the step uses instead of `input`, if any. Also moves a Travel on.
    pub(super) fn idol_input(&mut self, input: &InputFrame, dt: f32) -> Option<InputFrame> {
        let pressed = input.buttons.action && !self.idols.action_held;
        self.idols.action_held = input.buttons.action;
        self.idols.lighting = self
            .idols
            .lighting
            .map(|t| t + dt)
            .filter(|t| *t < LIGHTING_SECS);
        // Off the idol once he stood up, something else took him (a hit, death), or the kneel
        // event was never taken.
        if self.idols.at.is_some()
            && self.idol_phase().is_none()
            && !self.player.behavior.runtime.has_pending_events()
        {
            self.idols.at = None;
            self.idols.lighting = None;
            self.idols.travel = None;
            // A rest cut short before the dark was full resets nothing; the game lifts the
            // dark from where it got to.
            self.idols.rest = None;
        }
        self.step_rest(dt);
        if let Some((to, t)) = self.idols.travel.as_mut() {
            *t += dt;
            if *t >= WARP_CUT {
                let to = *to;
                self.idols.travel = None;
                self.leave_for(to);
            }
        }
        if pressed {
            match self.action_target() {
                Some(ActionTarget::Idol(i)) => self.kneel_at(i),
                Some(ActionTarget::Pickup(p)) => self.pick_up(p),
                None => {}
            }
        }
        self.idols.at.map(|_| InputFrame {
            camera_yaw: input.camera_yaw,
            ..InputFrame::default()
        })
    }

    fn kneel_at(&mut self, i: usize) {
        if !self
            .player
            .behavior
            .runtime
            .fire_event("W_StatueAttachStart")
        {
            return;
        }
        let spot = self.idols.spots[i];
        self.player.body.position = spot.kneel.position;
        self.player.body.velocity = [0.0; 3];
        self.player.snap_yaw(spot.kneel.yaw);
        self.player.lock_target = None;
        self.rules.fighters[0].reset();
        self.idols.at = Some(i);
        self.idols.checkpoint = Some(spot.checkpoint);
        self.idols.lighting = self.kit.found_idols.insert(spot.entity).then_some(0.0);
    }

    /// Seconds since Wolf lit the idol he is at, while he is [`IdolPhase::Lighting`]: the
    /// banner's clock.
    pub fn idol_lighting(&self) -> Option<f32> {
        self.idols
            .lighting
            .filter(|_| self.idol_phase() == Some(IdolPhase::Lighting))
    }

    /// Rests at the idol Wolf prays at: the rest clip plays the burst and the dark, and once it
    /// is fully dark ([`REST_RESET`]) Vitality and the gourd are refilled, the carried goods
    /// restocked from the Storage, and the map's foes back.
    pub fn rest_at_idol(&mut self) {
        if self.praying()
            && self
                .player
                .behavior
                .runtime
                .fire_event("W_StatueAttachRest")
        {
            self.idols.rest = Some(RestClock {
                secs: 0.0,
                reset: false,
            });
        }
    }

    /// The rest under way, until its dark has lifted: the burst's and the fade's clock.
    pub fn idol_rest(&self) -> Option<RestClock> {
        self.idols.rest
    }

    /// Moves the rest on: the world resets the step the dark is full, and the clock stops once
    /// the dark has lifted.
    fn step_rest(&mut self, dt: f32) {
        let Some(mut rest) = self.idols.rest else {
            return;
        };
        rest.secs += dt;
        if !rest.reset && rest.secs >= REST_RESET {
            rest.reset = true;
            self.refresh_at_idol();
            self.idols.revive = self.population.is_some();
        }
        self.idols.rest = (rest.secs < REST_END).then_some(rest);
    }

    /// What a rest gives Wolf: full Vitality, the gourd refilled and the carried goods restocked
    /// from the Storage.
    fn refresh_at_idol(&mut self) {
        self.rules.fighters[0].reset();
        self.restock_kit();
    }

    /// Travels from the idol Wolf prays at to `to`, an idol of this play world or one on another
    /// map: the warp clip plays, and [`WARP_CUT`] into it he leaves. Returns false, and nothing
    /// happens, unless he prays with no other event on its way, or when `to` is
    /// [`Destination::Here`] the idol he is at or an index past this play world's idols. An
    /// [`Destination::Away`] entity is not checked here: the game picks only idols whose map it
    /// can start.
    pub fn travel_to_idol(&mut self, to: Destination) -> bool {
        if let Destination::Here(i) = to
            && (i >= self.idols.spots.len() || self.idols.at == Some(i))
        {
            return false;
        }
        if !self.praying()
            || !self
                .player
                .behavior
                .runtime
                .fire_event("W_StatueAttachWarp")
        {
            return false;
        }
        self.idols.travel = Some((to, 0.0));
        true
    }

    /// Seconds into the warp clip of the Travel under way: the loading fade's clock.
    pub fn idol_travel(&self) -> Option<f32> {
        self.idols.travel.map(|(_, t)| t)
    }

    /// The idol on another map a Travel has left for, once: the game loads its map.
    pub fn take_departure(&mut self) -> Option<i32> {
        self.idols.departure.take()
    }

    /// The end of a Travel's warp. Both ways Wolf leaves refreshed as a rest leaves him (full
    /// Vitality, the gourd and goods restocked). Here, he arrives at the idol's warp point the
    /// way a respawn after death puts him at the checkpoint (this step resets the world: the
    /// foes back, the camera behind him), without the respawn ending a fight. Away, the idol is
    /// handed over with the restocked kit and Wolf stays in the warp until the game leaves.
    fn leave_for(&mut self, to: Destination) {
        self.refresh_at_idol();
        match to {
            Destination::Here(i) => {
                self.idols.checkpoint = Some(self.idols.spots[i].checkpoint);
                self.idols.at = None;
                self.idols.lighting = None;
                self.idols.arriving = true;
                self.respawn_in = Some(0.0);
            }
            Destination::Away(entity) => self.idols.departure = Some(entity),
        }
    }

    /// A Travel within the play world is arriving: its respawn is no fight's end.
    pub(crate) fn idol_arriving(&self) -> bool {
        self.idols.arriving
    }

    /// Stands Wolf up from the idol he prays at.
    pub fn leave_idol(&mut self) {
        if self.praying() {
            self.player.behavior.runtime.fire_event("W_StatueAttachEnd");
        }
    }

    /// In the prayer loop with no other event on its way (a second press in the same frame).
    fn praying(&self) -> bool {
        self.idol_phase() == Some(IdolPhase::Praying)
            && !self.player.behavior.runtime.has_pending_events()
    }

    /// Where Wolf is at an idol, `None` when he is not at one.
    pub fn idol_phase(&self) -> Option<IdolPhase> {
        let phase = match self.player.behavior.runtime.main_state_name() {
            "StatueAttachStart" => IdolPhase::Kneeling,
            "StatueAttachLoop" => IdolPhase::Praying,
            "StatueAttachRest" => IdolPhase::Resting,
            "StatueAttachWarp" => IdolPhase::Traveling,
            "StatueAttachEnd" => IdolPhase::Leaving,
            _ => return None,
        };
        let lighting = self.idols.lighting.is_some()
            && matches!(phase, IdolPhase::Kneeling | IdolPhase::Praying);
        Some(if lighting { IdolPhase::Lighting } else { phase })
    }
}
