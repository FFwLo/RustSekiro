//! The loading screen and the point where play starts.
//!
//! Like the game's own loading screen, an opaque screen with a progress bar covers the view and
//! the simulation does not step (so the player cannot move) until loading has finished: Wolf's
//! model is bound, every map GLB and texture has loaded and every scene has spawned
//! ([`crate::map::Pending::loaded`]), no render pipeline is still compiling, every image and
//! spawned mesh is prepared on the GPU and the drawn meshes within 60 m have their textures
//! ([`check_render`]), and then 60 consecutive frames are steady ([`FrameSettle`]), so uploads and first-use work that follow loading also
//! finish behind the screen. Steady is relative to the area's own frame times so an area that
//! is simply slow to draw (the Genichiro arena at about 47 ms, #75) starts once it is steady
//! instead of at the timeout. Scripted, replayed and interactive runs all wait for the whole
//! map. The time to that point is logged as `play ready`; a machine that never settles starts
//! after 60 s with a warning instead of waiting forever. A start with no collision floor under
//! Wolf fails before loading (`play::require_floor`), as it could never become playable.
use super::{Actor, Sim};
use crate::character::Rig;
use bevy::prelude::*;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Resource, Default)]
pub(crate) struct PlayGate {
    /// When loading finished and play may start.
    pub(super) ready_at: Option<Instant>,
    /// When everything had first loaded; the settle timeout counts from here.
    loaded_at: Option<Instant>,
    /// Frame steadiness since everything loaded, reset whenever a readiness signal drops.
    settle: FrameSettle,
    /// The last GPU readiness check ([`check_render`]); `None` before the first.
    render: Option<RenderWait>,
    /// Drawn meshes near Wolf, listed once when the upload counts first reach zero (Wolf does
    /// not move before play).
    near: Option<Vec<Entity>>,
    last_log: Option<Instant>,
    /// The fullest bar shown so far, so the bar never goes back. Each load (a map change
    /// relaunches the process) starts with a fresh gate.
    shown: f32,
}

/// `done` of `total` as a fraction from 0 to 1, for the loading bar; 1 when there is nothing to
/// do.
pub(crate) fn fraction(done: usize, total: usize) -> f32 {
    if total == 0 {
        1.0
    } else {
        done.min(total) as f32 / total as f32
    }
}

/// What the GPU still waits for before the world is drawable.
#[derive(Clone, Copy, Debug, Default)]
struct RenderWait {
    /// A drawn mesh near Wolf still waits for its material textures.
    textures: bool,
    /// Images queued behind the upload budget.
    images: usize,
    /// Meshes the world spawned that have not been prepared yet.
    meshes: usize,
}

impl RenderWait {
    fn ready(&self) -> bool {
        !self.textures && self.images == 0 && self.meshes == 0
    }
}

/// Drawn meshes within this distance of Wolf must have their textures before play starts.
const RENDER_RADIUS: f32 = 60.0;

/// A frame within this always counts as steady: 30 Hz, so a vsync-limited 60 Hz frame always
/// counts and a fast area keeps the original budget.
const STABLE_FRAME_SECONDS: f32 = 1.0 / 30.0;
/// A slower frame counts when it is within this many times the median of the recent frames:
/// the arena's steady frames sit within 1.1 times their median, while loading hitches (uploads,
/// scene spawning, shader compiles) are several times it.
const SPIKE_RATIO: f32 = 1.5;
/// Drawable frames the median needs before the relative rule applies, so the first few
/// frames, which may all be hitches, are not judged against each other.
const MIN_SAMPLE: usize = 15;
/// A frame over this is never steady, however slow the area: such an area waits for the
/// timeout instead.
const MAX_FRAME_SECONDS: f32 = 0.1;
/// Consecutive steady frames that mark the end of loading, and the frames the median is over.
const STABLE_FRAMES: u32 = 60;
/// A machine that never settles still starts, with a warning.
const STABLE_TIMEOUT_SECONDS: f32 = 60.0;

/// Whether frame times have settled after loading: [`STABLE_FRAMES`] consecutive steady
/// frames. A frame is steady when the world is drawable, it is at most [`MAX_FRAME_SECONDS`],
/// and it is within [`STABLE_FRAME_SECONDS`] or, once at least [`MIN_SAMPLE`] drawable frames
/// came before it, within [`SPIKE_RATIO`] times the median of the last [`STABLE_FRAMES`] of
/// them. The relative rule lets an area that is steady but slower than 30 Hz settle, while a
/// fast area still waits out every spike above 1/30 s; any other frame starts the count again.
#[derive(Default)]
struct FrameSettle {
    /// The last [`STABLE_FRAMES`] drawable frame times, in seconds; cleared while the world is
    /// not drawable, so slow upload frames never raise the median.
    recent: VecDeque<f32>,
    /// Consecutive steady frames.
    stable: u32,
}

impl FrameSettle {
    /// Records one frame; `drawable` is false while the GPU still waits for uploads.
    fn push(&mut self, seconds: f32, drawable: bool) {
        if !drawable {
            *self = Self::default();
            return;
        }
        let steady = seconds <= MAX_FRAME_SECONDS
            && (seconds <= STABLE_FRAME_SECONDS
                || (self.recent.len() >= MIN_SAMPLE && seconds <= self.median() * SPIKE_RATIO));
        self.stable = if steady { self.stable + 1 } else { 0 };
        if self.recent.len() == STABLE_FRAMES as usize {
            self.recent.pop_front();
        }
        self.recent.push_back(seconds);
    }

    /// The median of the recent drawable frames, 0 for none.
    fn median(&mut self) -> f32 {
        sekiro_sim::brain::metrics::percentile(self.recent.make_contiguous(), 0.5)
    }

    fn settled(&self) -> bool {
        self.stable >= STABLE_FRAMES
    }

    /// How far through the settling wait, from 0 to 1 (the gate opens when it reaches 1).
    fn progress(&self) -> f32 {
        self.stable as f32 / STABLE_FRAMES as f32
    }
}

impl PlayGate {
    /// Whether the simulation may step now. The 60 stable frames before readiness also cover
    /// the presentation stall that once followed the map upload; a further 2.5 s settle made
    /// no difference to the first frames of play (about 80 ms either way, measured 2026-10-16).
    pub(crate) fn open(&self) -> bool {
        self.ready_at.is_some()
    }

    fn render_ready(&self) -> bool {
        self.render.is_some_and(|r| r.ready())
    }
}

/// How far each loading step is, from 0 to 1, for the bar.
#[derive(Clone, Copy, Debug, Default)]
struct Steps {
    /// Map GLBs, textures and scene instances ([`crate::map::Pending::progress`]).
    map: f32,
    /// Static map merges installed ([`crate::map::StaticBatches::progress`]).
    batches: f32,
    /// Resident pipeline warmup ([`crate::pipeline_warmup::PipelineWarmup::progress`]).
    warmup: f32,
    rigs: bool,
    /// No render pipeline is compiling, counted only once the map is ready: before the warmup
    /// queues its compiles nothing is compiling either, and the bar must not count that.
    compiled: bool,
    settle: f32,
}

impl Steps {
    /// The bar's fill from 0 to 1. The merges and the warmup follow the map load and took about
    /// 14 s of a 51 s load on m11_00 (#171), so they have their own share and move the bar.
    fn bar(&self) -> f32 {
        0.8 * self.map
            + 0.03 * self.batches
            + 0.07 * self.warmup
            + 0.04 * f32::from(u8::from(self.rigs))
            + 0.03 * f32::from(u8::from(self.compiled))
            + 0.03 * self.settle
    }
}

#[derive(Component)]
pub(super) struct LoadingScreen;

#[derive(Component)]
pub(super) struct LoadingBar;

/// Run condition: the loading screen is still up. Once play starts it is despawned and the
/// gate's systems stop running.
pub(super) fn loading(screens: Query<(), With<LoadingScreen>>) -> bool {
    !screens.is_empty()
}

pub(super) fn spawn_screen(mut commands: Commands) {
    commands
        .spawn((
            LoadingScreen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::FlexEnd,
                padding: UiRect::all(px(48)),
                row_gap: px(10),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(100),
        ))
        .with_children(|screen| {
            screen.spawn((
                Text::new("Loading"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(22.0),
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.82, 0.75)),
            ));
            screen
                .spawn((
                    Node {
                        width: px(320),
                        height: px(4),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
                ))
                .with_child((
                    LoadingBar,
                    Node {
                        width: percent(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.85, 0.82, 0.75)),
                ));
        });
}

type CharacterRigs = Or<(With<Actor>, With<super::map_enemies::FoeActor>)>;

#[allow(clippy::too_many_arguments)]
pub(super) fn update(
    mut commands: Commands,
    mut gate: ResMut<PlayGate>,
    time: Res<Time<Real>>,
    rigs: Query<&Rig, CharacterRigs>,
    pending: Option<Res<crate::map::Pending>>,
    pipelines: Option<Res<crate::map::ready::Pipelines>>,
    warmup: Option<Res<crate::pipeline_warmup::PipelineWarmup>>,
    batches: Option<Res<crate::map::StaticBatches>>,
    screens: Query<Entity, With<LoadingScreen>>,
    mut bars: Query<&mut Node, With<LoadingBar>>,
) {
    if gate.open() {
        for screen in &screens {
            commands.entity(screen).despawn();
        }
        return;
    }
    let progress = if gate.ready_at.is_none() {
        let rigs_bound = !rigs.is_empty() && rigs.iter().all(|r| r.bound);
        let map = crate::map::ready(pending.as_deref(), batches.as_deref(), warmup.as_deref());
        let compiled = pipelines.as_ref().is_none_or(|p| {
            let status = p.status();
            status.pending == 0 && status.ready > 0
        });
        // GPU uploads trail the CPU load; then uploads, scene spawning and shader compiles show
        // up as slow frames, so the area is ready once the world is drawable and frames are
        // steady.
        if !(rigs_bound && map && compiled) {
            // A late shader compile or a dropped readiness signal restarts the count.
            gate.settle = FrameSettle::default();
        } else {
            let since = *gate.loaded_at.get_or_insert_with(Instant::now);
            let drawable = gate.render_ready();
            gate.settle.push(time.delta_secs(), drawable);
            let settled = gate.settle.settled();
            if settled || since.elapsed().as_secs_f32() >= STABLE_TIMEOUT_SECONDS {
                gate.ready_at = Some(Instant::now());
                let launch = crate::since_start();
                let settling = since.elapsed().as_secs_f32();
                if settled {
                    let median = gate.settle.median();
                    if median > STABLE_FRAME_SECONDS {
                        warn!(
                            "play ready: map loaded and {STABLE_FRAMES} steady frames, but a slow area (median {:.1} ms, over {:.1} ms), {launch:.1} s after launch ({settling:.1} s settling)",
                            median * 1000.0,
                            STABLE_FRAME_SECONDS * 1000.0
                        );
                    } else {
                        info!(
                            "play ready: map loaded and {STABLE_FRAMES} steady frames (median {:.1} ms), {launch:.1} s after launch ({settling:.1} s settling)",
                            median * 1000.0
                        );
                    }
                } else {
                    warn!(
                        "play ready: not settled within {STABLE_TIMEOUT_SECONDS} s of loading ({:?}), starting anyway {launch:.1} s after launch",
                        gate.render
                    );
                }
            }
        }
        Steps {
            map: pending.as_ref().map_or(1.0, |p| p.progress()),
            batches: batches.as_ref().map_or(1.0, |b| b.progress()),
            warmup: warmup.as_ref().map_or(1.0, |w| w.progress()),
            rigs: rigs_bound,
            compiled: map && compiled,
            settle: gate.settle.progress(),
        }
        .bar()
    } else {
        1.0
    };
    // The steps can dip (a late plan adds merges, a late compile starts), but the bar only rises.
    let progress = progress.max(gate.shown);
    gate.shown = progress;
    let width = percent(progress * 100.0);
    for mut bar in &mut bars {
        if bar.width != width {
            bar.width = width;
        }
    }
}

/// Once everything has loaded, checks that the world is drawable: no image upload is still
/// queued behind the upload budget, every mesh the world spawned has been prepared on the GPU
/// once ([`crate::map::ready::LateMeshes::loading`]; effect batches and meshes rewritten every
/// frame do not count, as they keep changing during play), and the
/// materials of the drawn meshes within [`RENDER_RADIUS`] of Wolf have their textures (the
/// capture readiness check). The nearby list is made once, when the upload counts first reach
/// zero.
pub(super) fn check_render(world: &mut World) {
    if !world.contains_resource::<crate::map::ready::LateMeshes>() {
        world.resource_mut::<PlayGate>().render = Some(RenderWait::default());
        return;
    }
    let gate = world.resource::<PlayGate>();
    if gate.ready_at.is_some() || gate.loaded_at.is_none() {
        return;
    }
    let (images, meshes) = crate::map::ready::pending_uploads(world);
    let textures = images == 0 && meshes == 0 && {
        let near = match world.resource_mut::<PlayGate>().near.take() {
            Some(near) => near,
            None => near_meshes(world),
        };
        let pending = crate::map::ready::textures_pending(world, &near);
        world.resource_mut::<PlayGate>().near = Some(near);
        pending
    };
    let wait = RenderWait {
        textures,
        images,
        meshes,
    };
    let mut gate = world.resource_mut::<PlayGate>();
    gate.render = Some(wait);
    if gate
        .last_log
        .is_none_or(|t| t.elapsed().as_secs_f32() >= 1.0)
    {
        gate.last_log = Some(Instant::now());
        info!("loading: waiting for the GPU: {wait:?}");
    }
}

/// The drawn meshes whose bounds come within [`RENDER_RADIUS`] of Wolf.
fn near_meshes(world: &mut World) -> Vec<Entity> {
    let wolf = Vec3::from_array(world.non_send::<Sim>().duel.player.body.position);
    let mut query = world.query_filtered::<(
        Entity,
        &GlobalTransform,
        &InheritedVisibility,
        &bevy::camera::primitives::Aabb,
    ), With<Mesh3d>>();
    query
        .iter(world)
        .filter(|(_, transform, visible, aabb)| {
            let (lo, hi) = crate::map::world_bounds(transform, aabb);
            visible.get() && wolf.distance(wolf.clamp(lo, hi)) <= RENDER_RADIUS
        })
        .map(|(entity, ..)| entity)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_pause_keeps_the_clock_running_until_the_gate_opens() {
        for gamepad in [false, true] {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<Assets<Font>>()
                .init_resource::<ButtonInput<KeyCode>>()
                .add_message::<bevy::input::keyboard::KeyboardInput>();
            crate::menu::install(&mut app);
            app.init_resource::<PlayGate>();
            let pad = app.world_mut().spawn(Gamepad::default()).id();
            app.update();
            let press_pause = |app: &mut App| {
                if gamepad {
                    let mut pad = app.world_mut().get_mut::<Gamepad>(pad).unwrap();
                    pad.digital_mut().reset_all();
                    pad.digital_mut().press(GamepadButton::Start);
                } else {
                    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                    keys.reset_all();
                    keys.press(KeyCode::Escape);
                }
            };
            press_pause(&mut app);
            app.update();
            assert!(
                app.world().resource::<crate::menu::PanelState>().paused,
                "loading must leave Back and Quit reachable in the pause panel"
            );
            assert!(!app.world().resource::<Time<Virtual>>().is_paused());

            app.world_mut().resource_mut::<PlayGate>().ready_at = Some(Instant::now());
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .get_mut::<Gamepad>(pad)
                .unwrap()
                .digital_mut()
                .reset_all();
            app.update();
            assert!(app.world().resource::<crate::menu::PanelState>().paused);
            assert!(app.world().resource::<Time<Virtual>>().is_paused());
            press_pause(&mut app);
            app.update();
            assert!(!app.world().resource::<crate::menu::PanelState>().paused);
            assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        }
    }

    /// Feeds (seconds, drawable) frames until the frames settle or run out.
    fn run(frames: impl IntoIterator<Item = (f32, bool)>) -> FrameSettle {
        let mut settle = FrameSettle::default();
        for (seconds, drawable) in frames {
            settle.push(seconds, drawable);
            if settle.settled() {
                break;
            }
        }
        settle
    }

    fn drawn(frames: impl IntoIterator<Item = f32>) -> impl Iterator<Item = (f32, bool)> {
        frames.into_iter().map(|seconds| (seconds, true))
    }

    #[test]
    fn bar_rises_through_every_step_of_a_load() {
        // A load in its real order: rigs bind while the map streams, then the merges, the
        // warmup and its compiles, and the settle.
        let mut s = Steps::default();
        let mut states = Vec::new();
        for map in [0.0, 0.5, 0.99] {
            s.map = map;
            s.rigs = map > 0.4;
            states.push(s);
        }
        s.map = 1.0;
        for step in [0.0, 0.5, 1.0] {
            s.batches = step;
            states.push(s);
        }
        let warmup_start = states.len();
        for step in [0.2, 0.4, 0.7, 1.0] {
            s.warmup = step;
            states.push(s);
        }
        let warmup_end = states.len() - 1;
        s.compiled = true;
        for step in [0.0, 0.5, 1.0] {
            s.settle = step;
            states.push(s);
        }
        let bars: Vec<f32> = states.iter().map(Steps::bar).collect();
        assert!(bars.windows(2).all(|w| w[1] >= w[0]), "{bars:?}");
        assert!((bars[bars.len() - 1] - 1.0).abs() < 1e-6, "{bars:?}");
        // The warmup takes seconds, so it must visibly move the bar.
        assert!(
            bars[warmup_end] - bars[warmup_start - 1] >= 0.05,
            "{bars:?}"
        );
    }

    #[test]
    fn steady_slow_frames_settle() {
        // The Genichiro arena: about 47 ms a frame with a little jitter, never within 1/30 s.
        assert!(run(drawn((0..120).map(|i| 0.044 + 0.002 * (i % 4) as f32))).settled());
    }

    #[test]
    fn steady_fast_frames_settle() {
        assert!(run(drawn(std::iter::repeat_n(1.0 / 60.0, 60))).settled());
    }

    #[test]
    fn spiky_frames_do_not_settle() {
        // A 90 ms loading hitch every 20 frames on a 47 ms area: under the ceiling but over
        // 1.5 times the median.
        let frames = (0..600).map(|i| if i % 20 == 19 { 0.090 } else { 0.047 });
        assert!(!run(drawn(frames)).settled());
    }

    #[test]
    fn fast_area_keeps_the_absolute_budget() {
        // On a 60 Hz area a 40 ms frame is still a hitch: over 1/30 s and over 1.5 times the median.
        let frames = (0..600).map(|i| if i % 30 == 29 { 0.040 } else { 1.0 / 60.0 });
        assert!(!run(drawn(frames)).settled());
    }

    #[test]
    fn frames_before_the_world_is_drawable_do_not_count() {
        let frames = std::iter::repeat_n((1.0 / 60.0, false), 100)
            .chain(std::iter::repeat_n((1.0 / 60.0, true), 59));
        assert!(!run(frames).settled());
        let frames = std::iter::repeat_n((1.0 / 60.0, false), 100)
            .chain(std::iter::repeat_n((1.0 / 60.0, true), 60));
        assert!(run(frames).settled());
    }

    #[test]
    fn slow_upload_frames_do_not_raise_the_median() {
        // 100 ms frames while the GPU uploads, then a 47 ms area with 90 ms hitches in its
        // early drawable frames: judged against the upload frames, the hitches would count.
        let uploads = std::iter::repeat_n((0.100, false), 60);
        let early = (0..600).map(|i| (if i % 20 == 10 { 0.090 } else { 0.047 }, true));
        assert!(!run(uploads.chain(early)).settled());
    }

    #[test]
    fn steady_very_slow_area_never_settles() {
        // Steady at 150 ms: over the ceiling, so it waits for the timeout.
        assert!(!run(drawn(std::iter::repeat_n(0.150, 600))).settled());
    }

    #[test]
    fn small_sample_uses_only_the_absolute_budget() {
        // 40 ms frames are within 1.5 times their median but not within 1/30 s, so they count
        // only once MIN_SAMPLE frames came before.
        let settle = run(drawn(std::iter::repeat_n(0.040, MIN_SAMPLE)));
        assert_eq!(settle.stable, 0);
        let settle = run(drawn(std::iter::repeat_n(0.040, MIN_SAMPLE + 1)));
        assert_eq!(settle.stable, 1);
    }
}
