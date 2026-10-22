//! Wolf's seat at a Sculptor's Idol or an NPC (on `Player`; idol.rs and sculptor.rs drive it):
//! the idol, the phase of its clips, the menu's command, the last idol prayed at.

/// Where Wolf is in the idol's clips.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Phase {
    #[default]
    Kneeling,
    /// The loop, with the menu open.
    Menu,
    Resting,
    Warping(usize),
    Leaving,
}

/// What the menu asks of the seat.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    Rest,
    Warp(usize),
    Leave,
}

/// The idol Wolf kneels at, the phase, the menu's command, the checkpoint.
#[derive(Default, Clone)]
pub struct Seat {
    pub idol: Option<usize>,
    /// Talking to the Sculptor uses the Workshop, without kneeling, healing or changing idols.
    pub sculptor: bool,
    pub phase: Phase,
    pub cmd: Option<Cmd>,
    /// The menu should open (the kneel or the rest clip is done).
    pub open: bool,
    /// The last idol prayed at.
    pub checkpoint: Option<usize>,
    /// The enemies should come back (a rest; player.rs sets FightReset). Kneeling alone leaves
    /// them.
    pub respawn: bool,
    /// Seconds since the first kneel at this idol lit it, while the "SCULPTOR'S IDOL FOUND"
    /// banner plays (idol_found.rs draws it at this clock); the menu waits for it.
    pub lighting: Option<f32>,
}
