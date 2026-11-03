//! Start and pause panels. Relaunch after App drops keeps shutdown identical to Quit.
use bevy::app::AppExit;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};
use bevy::window::{CursorGrabMode, CursorOptions};
use std::path::PathBuf;
use std::sync::Mutex;

static NEXT: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// The flags with a value that a relaunch from a run carries over (Back to start menu, Travel
/// to an idol on another map): the cache, the window, the player, the brain and its coach,
/// and the time of day. One-shot automation (`--script`, `--input`, `--shots`, `--exit`,
/// `--perf-seconds`, ...) and the run's own place (`--map`, `--start`, `--spawn`, `--arena`,
/// `--enemy`) stay behind.
pub(crate) const KEPT_VALUES: &[&str] = &[
    "--cache",
    "--window-size",
    "--present-mode",
    "--volume",
    "--vram-budget",
    "--menu-choices",
    "--player",
    "--attack-power",
    "--progression",
    "--hour",
    "--story",
    "--brain",
    "--brain-model",
    "--brain-memory",
    "--brain-stage",
    "--brain-budget",
    "--brain-token-budget",
    "--coach",
    "--coach-model",
    "--coach-effort",
    "--coach-interval",
    "--checkpoint",
    "--brain-moves",
    "--brain-move-every",
    "--coach-review-every",
    "--shadow",
    "--shadow-casters",
    "--shadow-bias",
    "--local-light-scale",
];

/// The switches a relaunch from a run carries over: play and its enemies, the cheats and
/// brain switches, the frame pacing, the perf log and the render and debug toggles.
pub(crate) const KEPT_SWITCHES: &[&str] = &[
    "--play",
    "--perf",
    "--profile-systems",
    "--uncapped",
    "--no-enemy",
    "--passive",
    "--map-enemies",
    "--simple-ai",
    "--god",
    "--immortal",
    "--lock-on",
    "--break-posture",
    "--brain-log",
    "--brain-overlay",
    "--objects",
    "--no-connected-blocks",
    "--no-cloth",
    "--no-fxr",
    "--no-fog",
    "--no-fog-surfaces",
    "--no-shadow",
    "--no-bloom",
    "--no-auto-exposure",
    "--no-grading",
    "--no-giiv",
    "--no-sky-dome",
    "--no-far-scenery",
    "--no-env-probe",
    "--no-map-story",
    "--no-local-lights",
    "--no-layers",
    "--no-camera-fade",
    "--no-arena-effects",
    "--no-pieces",
    "--legacy-camera-fade",
    "--native-tone-filmic",
    "--native-tone-reinhard",
    "--lod-debug",
    "--world-debug",
    "--visibility-debug",
    "--show-blend",
    "--visual-stats",
    "--probe-sky",
];

/// The flags of `args` a relaunch from a run keeps ([`KEPT_VALUES`] with their values,
/// [`KEPT_SWITCHES`]), in their order; everything else is dropped.
pub(crate) fn kept_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut args = args.iter();
    while let Some(a) = args.next() {
        if KEPT_VALUES.contains(&a.as_str()) {
            if let Some(v) = args.next() {
                out.extend([a.clone(), v.clone()]);
            }
        } else if KEPT_SWITCHES.contains(&a.as_str()) {
            out.push(a.clone());
        }
    }
    out
}
/// Starts the game again with `args` once this run exits ([`relaunch`]).
pub(crate) fn relaunch_with(args: Vec<String>) {
    *NEXT.lock().unwrap() = Some(args);
}
pub fn relaunch() -> anyhow::Result<()> {
    if let Some(args) = NEXT.lock().unwrap().take() {
        std::process::Command::new(std::env::current_exe()?)
            .args(args)
            .spawn()?;
    }
    Ok(())
}

/// A primary mouse click on `entity` through picking's press and release events, as if the
/// pointer hit `hit`, for tests with no window or picking backend.
#[cfg(test)]
pub(crate) fn test_click(world: &mut World, entity: Entity, hit: Entity) {
    use bevy::camera::{ManualTextureViewHandle, NormalizedRenderTarget};
    use bevy::picking::{
        backend::HitData,
        events::{Pointer, PointerPress, PointerRelease},
        pointer::{Location, PointerButton, PointerId},
    };
    // Pointer event traversal also queries Window, including in a windowless test.
    world.register_component::<Window>();
    let pointer = Pointer::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::TextureView(ManualTextureViewHandle(5)),
            position: Vec2::ZERO,
        },
    );
    let hit = HitData::new(hit, 0., None, None);
    world.trigger(PointerPress {
        entity,
        pointer: pointer.clone(),
        button: PointerButton::Primary,
        hit: hit.clone(),
        count: 1,
    });
    world.trigger(PointerRelease {
        entity,
        pointer,
        button: PointerButton::Primary,
        hit,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_held_through_a_menu_wait_for_release() {
        let mut held = Vec::new();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(MENU_BACK_KEY);
        track_held(&mut held, &keys, true);
        // Closed, Q still held: ignored, while a key pressed after closing counts.
        keys.press(KeyCode::KeyW);
        track_held(&mut held, &keys, false);
        assert_eq!(held, [MENU_BACK_KEY]);
        keys.release(MENU_BACK_KEY);
        track_held(&mut held, &keys, false);
        assert!(held.is_empty());
    }

    #[test]
    fn game_menu_owns_pause_keys_and_requests() {
        let mut state = PanelState::new(false, &[]);
        state.paused = true;
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::ArrowDown);
        keys.press(KeyCode::Enter);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<KeyboardInput>()
            .insert_resource(keys)
            .insert_resource(state)
            .init_resource::<PauseView>()
            .add_systems(Update, update);
        let panel = app.world_mut().spawn((Panel, Visibility::Visible)).id();
        app.update();
        let state = app.world().resource::<PanelState>();
        assert!(state.paused, "Enter belongs to the game menu");
        assert_eq!(state.selected, 0, "its keys cannot move the hidden panel");
        assert_eq!(
            app.world().get::<Visibility>(panel),
            Some(&Visibility::Hidden)
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut().resource_mut::<PauseView>().request = Some(0);
        app.update();
        assert!(!app.world().resource::<PanelState>().paused);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        assert!(app.world().resource::<PauseView>().request.is_none());
    }

    #[test]
    fn presentation_flags_survive_play_and_return_to_menu() {
        let args = ["--uncapped", "--present-mode", "immediate"].map(String::from);
        let start = PanelState::new(true, &args);
        let mut play = start.choices.args();
        play.extend(start.extras);
        assert!(play.iter().any(|a| a == "--uncapped"));
        assert_eq!(
            crate::arg_value(&play, "--present-mode").as_deref(),
            Some("immediate")
        );

        let mut pause = PanelState::new(false, &play);
        pause.paused = true;
        pause.selected = 1;
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Enter);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<KeyboardInput>()
            .insert_resource(keys)
            .insert_resource(pause)
            .add_systems(Update, update);
        app.update();
        let returned = NEXT.lock().unwrap().take().expect("return to menu");
        let menu = PanelState::new(true, &returned);
        assert!(menu.extras.iter().any(|a| a == "--uncapped"));
        assert_eq!(
            crate::arg_value(&menu.extras, "--present-mode").as_deref(),
            Some("immediate")
        );
    }

    fn equivalent(choices: Choices, flags: &str) {
        let mut menu = vec!["sekiro-rs".into()];
        menu.extend(choices.args());
        let mut cli: Vec<String> = std::iter::once("sekiro-rs")
            .chain(flags.split_whitespace())
            .map(String::from)
            .collect();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for args in [&mut menu, &mut cli] {
            args.extend([
                "--cache".into(),
                sekiro_sim::cache::root().to_string_lossy().into_owned(),
            ]);
        }
        let a = crate::play_options_with(&menu, &root.join("logs/datasets")).unwrap();
        let b = crate::play_options_with(&cli, &root.join("logs/datasets")).unwrap();
        assert_eq!(a.opponent, b.opponent);
        assert_eq!(a.arena.map(|v| v.name), b.arena.map(|v| v.name));
        // An unset hour is NaN (the story picks it), so compare the bits, not the floats.
        assert_eq!(
            (a.map, a.start, a.hour.to_bits()),
            (b.map, b.start, b.hour.to_bits())
        );
        assert_eq!(
            (a.enemy, a.enemy_present, a.god, a.immortal, a.attack_power),
            (b.enemy, b.enemy_present, b.god, b.immortal, b.attack_power)
        );
        assert_eq!(
            (a.brain.name, a.brain_label, a.brain_overlay, a.player),
            (b.brain.name, b.brain_label, b.brain_overlay, b.player)
        );
        assert_eq!(
            (a.window_size, a.coach_interval),
            (b.window_size, b.coach_interval)
        );
        assert_eq!(
            (
                a.cache,
                a.script,
                a.input,
                a.screenshots,
                a.exit_after_script
            ),
            (
                b.cache,
                b.script,
                b.input,
                b.screenshots,
                b.exit_after_script
            )
        );
        assert_eq!(
            (
                a.full_map,
                a.hide,
                a.objects,
                a.lock_on,
                a.simple_ai,
                a.force_act,
                a.break_posture,
                a.brain_log
            ),
            (
                b.full_map,
                b.hide,
                b.objects,
                b.lock_on,
                b.simple_ai,
                b.force_act,
                b.break_posture,
                b.brain_log
            )
        );
        assert_eq!(
            (a.spawn, a.enemy_distance, a.gparam),
            (b.spawn, b.enemy_distance, b.gparam)
        );
        assert_eq!((a.brain.model, a.brain.chr), (b.brain.model, b.brain.chr));
        assert_eq!(
            (
                a.brain.coach.backend,
                a.brain.coach.model,
                a.brain.coach.effort,
                a.brain.coach.checkpoint,
                a.brain.coach.budget_usd,
                a.brain.coach.token_budget
            ),
            (
                b.brain.coach.backend,
                b.brain.coach.model,
                b.brain.coach.effort,
                b.brain.coach.checkpoint,
                b.brain.coach.budget_usd,
                b.brain.coach.token_budget
            )
        );
        match (a.brain_memory, b.brain_memory) {
            (Some(a), Some(b)) => assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(a).unwrap()).unwrap(),
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(b).unwrap()).unwrap()
            ),
            (None, None) => {}
            _ => panic!("memory selection differs"),
        }
    }
    #[test]
    fn soldier_flat_matches_cli() {
        let mut choices = Choices::default();
        choices.values[0] = "Ashina soldier".into();
        equivalent(choices, "--play --enemy soldier --progression castle-roof");
    }
    #[test]
    fn owl_coach_alive_overlay_matches_cli() {
        let mut c = Choices::default();
        for (i, v) in [
            (0, "Great Shinobi Owl"),
            (2, "Arena"),
            (5, "Coach brain (codex)"),
            (6, "alive"),
            (7, "on"),
        ] {
            c.values[i] = v.into();
        }
        equivalent(
            c,
            "--arena owl --brain coach --brain-stage alive --brain-overlay --progression castle-roof",
        );
    }
    #[test]
    fn explore_map_start_matches_cli() {
        let mut c = Choices::default();
        for (i, v) in [(3, "m11_01_00_00"), (4, "2")] {
            c.values[i] = v.into();
        }
        equivalent(
            c,
            "--play --map m11_01_00_00 --start 2 --no-enemy --objects --simple-ai --progression castle-roof",
        );
    }
    #[test]
    fn genichiro_uses_arena_registry_enemy() {
        let mut c = Choices::default();
        c.values[0] = "Genichiro".into();
        c.values[2] = "Arena".into();
        equivalent(c, "--arena genichiro --progression castle-roof");
    }
    #[test]
    fn no_enemy_removes_the_opponent_while_passive_only_disables_ai() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (flag, present, active) in [
            ("--no-enemy", false, false),
            ("--passive", true, false),
            ("--simple-ai", true, true),
        ] {
            let args = vec![
                "sekiro-rs".into(),
                "--play".into(),
                flag.into(),
                "--cache".into(),
                sekiro_sim::cache::root().to_string_lossy().into_owned(),
            ];
            let options = crate::play_options_with(&args, &root.join("logs/datasets")).unwrap();
            assert_eq!(
                (options.enemy_present, options.enemy),
                (present, active),
                "{flag}"
            );
        }
    }
    #[test]
    fn explore_options_cover_every_extracted_map_with_first_visit_defaults() {
        let cache = sekiro_sim::cache::root();
        for id in MAPS {
            let Ok(dir) = crate::map::map_dir(&cache, id) else {
                continue;
            };
            let mut choices = Choices::default();
            choices.values[3] = id.into();
            crate::map::player_start(&dir, 0).unwrap();
            equivalent(
                choices.clone(),
                &format!(
                    "--play --map {id} --start 0 --no-enemy --objects --simple-ai --progression castle-roof"
                ),
            );
            let args = choices.args();
            assert!(!args.iter().any(|flag| matches!(
                flag.as_str(),
                "--hour" | "--story" | "--arena" | "--lock-on"
            )));
            let selection = crate::lighting_selection::select(
                &cache,
                dir.file_name().unwrap().to_str().unwrap(),
                None,
                None,
            )
            .unwrap();
            assert!(selection.hour.is_finite(), "{id}");
        }
    }
    #[test]
    fn map_changes_reset_start_and_start_selection_wraps_authored_points() {
        let cache = sekiro_sim::cache::root();
        let mut choices = Choices::default();
        for id in MAPS {
            let Ok(dir) = crate::map::map_dir(&cache, id) else {
                continue;
            };
            choices.values[3] = id.into();
            choices.values[4] = "0".into();
            choices.change(4, -1, &cache);
            let last = choices.values[4].parse().unwrap();
            crate::map::player_start(&dir, last).unwrap();
            assert!(crate::map::player_start(&dir, last + 1).is_err());
            choices.change(4, 1, &cache);
            assert_eq!(choices.values[4], "0");
            choices.values[4] = "99".into();
            choices.change(3, 1, &cache);
            assert_eq!(choices.values[4], "0");
        }
    }
    #[test]
    fn explore_ignores_hidden_fight_controls() {
        let mut choices = Choices::default();
        for (row, value) in [
            (1, "c5060"),
            (2, "Arena"),
            (5, "Coach brain (codex)"),
            (7, "on"),
            (9, "god"),
            (10, "alice"),
        ] {
            choices.values[row] = value.into();
        }
        equivalent(
            choices,
            "--play --map m11_00_00_00 --start 0 --no-enemy --objects --simple-ai --progression castle-roof",
        );
    }
    #[test]
    fn explore_keyboard_shortcuts_and_navigation_use_live_menu_system() {
        let cache = sekiro_sim::cache::root();
        let mut legacy = Choices::default();
        legacy.values[3] = "m11_01".into();
        let args = vec![
            "--cache".into(),
            cache.to_string_lossy().into_owned(),
            "--menu-choices".into(),
            serde_json::to_string(&legacy.values).unwrap(),
        ];
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<KeyboardInput>()
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(PanelState::new(true, &args))
            .add_systems(Update, update);
        assert_eq!(
            app.world().resource::<PanelState>().choices.values[3],
            "m11_01_00_00"
        );
        let press = |app: &mut App, key| {
            let mut keys = ButtonInput::<KeyCode>::default();
            keys.press(key);
            app.insert_resource(keys);
            app.update();
        };
        for (id, key) in MAPS.iter().zip([
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
            KeyCode::Digit0,
        ]) {
            if crate::map::map_dir(&cache, id).is_err() {
                continue;
            }
            press(&mut app, key);
            let state = app.world().resource::<PanelState>();
            assert_eq!(state.selected, 3);
            assert_eq!(
                crate::arg_value(&state.choices.args(), "--map").as_deref(),
                Some(*id)
            );
            assert_eq!(state.choices.values[4], "0");
        }
        press(&mut app, KeyCode::ArrowDown);
        assert_eq!(app.world().resource::<PanelState>().selected, 4);
        press(&mut app, KeyCode::ArrowDown);
        assert_eq!(app.world().resource::<PanelState>().selected, 12);
        press(&mut app, KeyCode::ArrowDown);
        assert_eq!(app.world().resource::<PanelState>().selected, 0);
        press(&mut app, KeyCode::ArrowRight);
        assert!(!app.world().resource::<PanelState>().choices.explore());
        press(&mut app, KeyCode::ArrowLeft);
        assert!(app.world().resource::<PanelState>().choices.explore());
    }

    #[test]
    fn windowed_pause_cursor_and_picking() {
        use bevy::input::{ButtonState, mouse::MouseButtonInput};
        use bevy::window::{PrimaryWindow, WindowEvent};

        #[derive(Resource, Default)]
        struct Probe {
            frame: usize,
            destination: Option<Vec2>,
            target: Option<Entity>,
            moved: bool,
            clicked: bool,
        }

        #[allow(clippy::too_many_arguments)]
        fn probe(
            mut probe: ResMut<Probe>,
            mut events: MessageReader<WindowEvent>,
            mut commands: Commands,
            mut windows: Query<(Entity, &Window, &mut CursorOptions), With<PrimaryWindow>>,
            rows: Query<(&Row, &Children)>,
            labels: Query<&UiGlobalTransform, With<Text>>,
            panels: Query<Entity, With<Panel>>,
            cameras: Query<Entity, With<Camera3d>>,
            mut state: ResMut<PanelState>,
            hover: Res<bevy::picking::hover::HoverMap>,
            mut exit: MessageWriter<AppExit>,
        ) {
            probe.frame += 1;
            let (entity, window, mut cursor) = windows.single_mut().unwrap();
            if probe.frame == 20 {
                commands.write_message(KeyboardInput {
                    key_code: KeyCode::Escape,
                    logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                    state: ButtonState::Pressed,
                    text: None,
                    repeat: false,
                    window: entity,
                });
            }
            if probe.frame == 25 {
                assert!(state.paused);
                assert!(cursor.visible, "pause must show the native cursor");
                assert_eq!(cursor.grab_mode, CursorGrabMode::None);
                eprintln!("native menu probe: paused, cursor visible and ungrabbed");
            }
            if matches!(probe.frame, 30 | 60) {
                let index = if probe.frame == 30 { 0 } else { 4 };
                let children = rows.iter().find(|(row, _)| row.0 == index).unwrap().1;
                let label = children[0];
                let position = labels.get(label).unwrap().translation / window.scale_factor();
                probe.destination = Some(position);
                probe.target = Some(label);
                probe.moved = false;
                // Enter at winit's Bevy message boundary without taking over the
                // desktop's physical mouse, never targeting a UI entity directly.
                commands.write_message(WindowEvent::CursorMoved(bevy::window::CursorMoved {
                    window: entity,
                    position,
                    delta: None,
                }));
            }
            for event in events.read() {
                if let WindowEvent::CursorMoved(event) = event
                    && probe
                        .destination
                        .is_some_and(|p| p.distance(event.position) < 2.)
                {
                    probe.moved = true;
                    eprintln!("windowed menu probe: cursor event at {:?}", event.position);
                }
            }
            if matches!(probe.frame, 45 | 75) {
                assert!(probe.moved, "must receive the window CursorMoved event");
                assert!(
                    hover
                        .0
                        .values()
                        .any(|hits| hits.contains_key(&probe.target.unwrap())),
                    "the label must be reached through UI hit testing"
                );
                eprintln!("windowed menu probe: picking hits {:?}", hover.0);
                // Enter at the same WindowEvent boundary used by winit, letting the
                // normal pointer input, UI hit testing and event dispatch find Resume.
                for state in [ButtonState::Pressed, ButtonState::Released] {
                    commands.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
                        window: entity,
                        button: MouseButton::Left,
                        state,
                    }));
                }
                probe.clicked = true;
            }
            if probe.frame == 55 {
                assert!(
                    probe.clicked && !state.paused,
                    "window pointer must resume play"
                );
                assert!(!cursor.visible);
                assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
                for panel in &panels {
                    commands.entity(panel).despawn();
                }
                for camera in &cameras {
                    commands.entity(camera).despawn();
                }
                commands.spawn(Camera2d);
                *state = PanelState::new(true, &[]);
                state.choices.values[4] = "0".into();
                cursor.visible = true;
                cursor.grab_mode = CursorGrabMode::None;
                commands.run_system_cached(spawn);
            }
            if probe.frame == 85 {
                assert_eq!(state.selected, 4);
                assert_eq!(
                    state.choices.values[4], "1",
                    "start menu must accept pointer input"
                );
                eprintln!("windowed menu probe: Resume and start-menu mouse paths passed");
                exit.write(AppExit::Success);
            }
        }

        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(bevy::winit::WinitPlugin {
                    run_on_any_thread: true,
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "sekiro-rs menu regression".into(),
                        resolution: (800, 600).into(),
                        ..default()
                    }),
                    primary_cursor_options: Some(CursorOptions {
                        visible: false,
                        grab_mode: CursorGrabMode::Locked,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(crate::play::cinematic_fade::CinematicFadePlugin)
        .insert_resource(PanelState::new(false, &[]))
        .init_resource::<crate::game_ui::GameFonts>()
        .init_resource::<Probe>()
        .add_systems(
            Startup,
            (
                |mut c: Commands| {
                    c.spawn(Camera3d::default());
                },
                spawn,
            ),
        )
        .add_systems(Update, probe);
        install_input(&mut app);
        assert_eq!(app.run(), AppExit::Success);
    }

    #[test]
    fn label_and_row_clicks_survive_release_before_menu_update() {
        for start in [true, false] {
            for label in [true, false] {
                let mut state = PanelState::new(start, &[]);
                state.paused = true;
                state.choices.values[4] = "0".into();
                let mut app = App::new();
                app.add_plugins((MinimalPlugins, bevy::ui_widgets::ButtonPlugin))
                    .init_resource::<crate::game_ui::GameFonts>()
                    .add_message::<KeyboardInput>()
                    .insert_resource(ButtonInput::<KeyCode>::default())
                    .insert_resource(state)
                    .add_systems(Startup, spawn);
                install_input(&mut app);
                app.update();
                let index = if start { 4 } else { 0 };
                let (row, child) = app
                    .world_mut()
                    .query::<(Entity, &Row, &Children)>()
                    .iter(app.world())
                    .find_map(|(e, r, children)| (r.0 == index).then_some((e, children[0])))
                    .unwrap();
                let entity = if label { child } else { row };
                test_click(app.world_mut(), entity, row);
                app.update();
                let state = app.world().resource::<PanelState>();
                if start {
                    assert_eq!(state.selected, 4);
                    assert_eq!(state.choices.values[4], "1");
                } else {
                    assert!(!state.paused, "Resume must accept a click on its label");
                    let mut keys = ButtonInput::<KeyCode>::default();
                    keys.press(KeyCode::Escape);
                    app.insert_resource(keys);
                    app.update();
                    assert!(app.world().resource::<PanelState>().paused);
                    let mut keys = ButtonInput::<KeyCode>::default();
                    keys.press(KeyCode::Enter);
                    app.insert_resource(keys);
                    app.update();
                    assert!(!app.world().resource::<PanelState>().paused);
                }
            }
        }
    }
}

const MAPS: [&str; 10] = [
    "m10_00_00_00",
    "m11_00_00_00",
    "m11_01_00_00",
    "m11_02_00_00",
    "m13_00_00_00",
    "m15_00_00_00",
    "m17_00_00_00",
    "m20_00_00_00",
    "m25_00_00_00",
    "m89_00_00_99",
];
/// The in-game pause rows, in the order `PauseView::request` names them.
pub const PAUSE_ROWS: [&str; 3] = ["Resume", "Back to start menu", "Quit"];

const LABELS: [&str; 13] = [
    "Mode",
    "Enemy id",
    "Place",
    "Map",
    "Start number",
    "AI",
    "Brain stage (Owl)",
    "Overlay",
    "Progression / AP",
    "Player",
    "Player name",
    "",
    "Start",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Choices {
    values: Vec<String>,
}
impl Default for Choices {
    fn default() -> Self {
        Self {
            values: [
                "Explore map",
                "c1010",
                "Flat floor",
                // The real game's first Ashina visit: Outskirts before Gyoubu, morning (#47).
                "m11_00_00_00",
                "0",
                "Lua",
                "none",
                "off",
                "castle-roof",
                "normal",
                "default",
                "",
                "",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
}
impl Choices {
    fn explore(&self) -> bool {
        self.values[0] == "Explore map"
    }
    fn visible_rows(&self) -> Vec<usize> {
        if self.explore() {
            vec![0, 3, 4, 11, 12]
        } else {
            (0..LABELS.len()).collect()
        }
    }
    fn enabled(&self, row: usize) -> bool {
        if self.explore() {
            return matches!(row, 0 | 3 | 4 | 12);
        }
        match row {
            1 => self.values[0] == "Any enemy id",
            3 | 4 => self.values[2] == "Map + start",
            6 => self.owl(),
            _ => true,
        }
    }
    fn owl(&self) -> bool {
        self.values[0] == "Great Shinobi Owl"
            || (self.values[0] == "Any enemy id"
                && self.values[1]
                    .split(':')
                    .next()
                    .is_some_and(|v| v.eq_ignore_ascii_case("c5060")))
    }
    pub fn args(&self) -> Vec<String> {
        let v = &self.values;
        let mut args = vec!["--play".into()];
        let mut pair = |flag: &str, value: &str| {
            args.extend([flag.into(), value.into()]);
        };
        let enemy = match v[0].as_str() {
            "Great Shinobi Owl" => "owl",
            "Genichiro" => {
                sekiro_sim::arena::ArenaSpec::by_name("genichiro")
                    .expect("supported arena")
                    .enemy
            }
            "Any enemy id" => &v[1],
            _ => "soldier",
        };
        pair("--enemy", enemy);
        let arena = if self.owl() {
            Some("owl")
        } else if enemy
            .split(':')
            .next()
            .is_some_and(|v| v.eq_ignore_ascii_case("c7100"))
        {
            Some("genichiro")
        } else {
            None
        };
        if !self.explore()
            && v[2] == "Arena"
            && let Some(arena) = arena
        {
            pair("--arena", arena);
        }
        if self.explore() || v[2] == "Map + start" {
            pair("--map", &v[3]);
            pair("--start", &v[4]);
        }
        pair(
            "--brain",
            match if self.explore() { "Lua" } else { v[5].as_str() } {
                "Coach brain (codex)" => "coach",
                "Scripted test plans" => "scripted",
                _ => "lua",
            },
        );
        if self.owl() && v[6] != "none" {
            pair("--brain-stage", &v[6]);
        }
        pair(
            if v[8] == "castle-roof" {
                "--progression"
            } else {
                "--attack-power"
            },
            &v[8],
        );
        pair("--player", if self.explore() { "default" } else { &v[10] });
        // Map defaults are the area's first visit, selected by its event-authored profile.
        if self.explore() {
            args.push("--objects".into());
            // No enemy means no brain session, coach, or persistent learning side effects.
            args.push("--simple-ai".into());
        }
        if self.explore() || v[0] == "No enemy" {
            args.push("--no-enemy".into());
        }
        if !self.explore() && v[7] == "on" {
            args.push("--brain-overlay".into());
        }
        if !self.explore() && v[9] != "normal" {
            args.push(format!("--{}", v[9]));
        }
        args
    }
    fn change(&mut self, row: usize, delta: i32, cache: &std::path::Path) {
        if delta == 0 || !self.enabled(row) {
            return;
        }
        let maps: Vec<_> = MAPS
            .iter()
            .copied()
            .filter(|id| crate::map::map_dir(cache, id).is_ok())
            .collect();
        let list: &[&str] = match row {
            0 => &[
                "Explore map",
                "Ashina soldier",
                "Great Shinobi Owl",
                "Genichiro",
                "Any enemy id",
                "No enemy",
            ],
            2 => &["Arena", "Flat floor", "Map + start"],
            3 => &maps,
            5 => &["Lua", "Coach brain (codex)", "Scripted test plans"],
            6 if self.owl() => &["none", "lua", "early", "alive", "outsmarted"],
            7 => &["off", "on"],
            9 => &["normal", "god", "immortal"],
            _ => &[],
        };
        if !list.is_empty() {
            let index = list
                .iter()
                .position(|v| *v == self.values[row])
                .unwrap_or(0) as i32;
            self.values[row] = list[(index + delta).rem_euclid(list.len() as i32) as usize].into();
            if row == 3 {
                self.values[4] = "0".into();
            }
        } else if row == 1 {
            let old = self.values[row]
                .trim_start_matches(['c', 'C'])
                .split(':')
                .next()
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(1010);
            self.values[row] = format!("c{:04}", (old + delta).clamp(0, 9999));
        } else if row == 4 {
            let count = crate::map::map_dir(cache, &self.values[3])
                .ok()
                .and_then(|dir| std::fs::read(dir.join("layout.json")).ok())
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|layout| layout["players"].as_array().map(Vec::len))
                .unwrap_or(1)
                .max(1);
            let old = self.values[row].parse::<i32>().unwrap_or(0);
            self.values[row] = (old + delta).rem_euclid(count as i32).to_string();
        } else if row == 8 {
            let old = self.values[row].parse::<i32>().unwrap_or(6);
            self.values[row] = if row == 8 && old == 1 && delta < 0 {
                "castle-roof".into()
            } else {
                (old + delta).clamp(1, 99).to_string()
            };
        }
    }
    fn path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
            .map(|p| PathBuf::from(p).join("sekiro-rs/menu.json"))
    }
    fn load() -> Self {
        let mut result = Self::default();
        if let Some(mut v) = Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        {
            // Migrate the old menu without carrying its testing-only hour into play.
            if v.len() == LABELS.len() + 1 {
                v.remove(9);
            }
            if v.len() == LABELS.len() {
                result.values = v;
                result.values[0] = "Explore map".into();
            }
        }
        result
    }
    fn save(&self) {
        if let Some(path) = Self::path() {
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            if let Ok(bytes) = serde_json::to_vec(&self.values) {
                let _ = std::fs::write(path, bytes);
            }
        }
    }
}

#[derive(Resource)]
pub struct PanelState {
    pub paused: bool,
    start: bool,
    choices: Choices,
    cache: PathBuf,
    selected: usize,
    error: String,
    extras: Vec<String>,
    fight_script: Option<String>,
    return_script: Option<String>,
    frame: usize,
    script: Vec<(usize, String)>,
    stick: IVec2,
    activations: Vec<usize>,
}
impl PanelState {
    fn new(start: bool, args: &[String]) -> Self {
        let script = crate::arg_value(args, "--menu-script")
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (n, action) = l.split_once(' ')?;
                Some((n.parse().ok()?, action.to_string()))
            })
            .collect();
        let mut extras = Vec::new();
        for flag in [
            "--cache",
            "--window-size",
            "--present-mode",
            "--script",
            "--shots",
            "--input",
            "--checkpoint",
            "--brain-moves",
            "--brain-move-every",
            "--coach-review-every",
        ] {
            if let Some(v) = crate::arg_value(args, flag) {
                extras.extend([flag.to_string(), v]);
            }
        }
        for flag in ["--simple-ai", "--exit", "--uncapped"] {
            if args.iter().any(|a| a == flag) {
                extras.push(flag.into());
            }
        }
        let choices = crate::arg_value(args, "--menu-choices")
            .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
            .filter(|v| v.len() == LABELS.len())
            .map(|values| Choices { values })
            .unwrap_or_else(Choices::load);
        let cache = sekiro_sim::cache::resolve(
            crate::arg_value(args, "--cache")
                .map(PathBuf::from)
                .as_deref(),
        );
        let mut choices = choices;
        if let Some(id) = MAPS.iter().find(|id| id.starts_with(&choices.values[3])) {
            choices.values[3] = (*id).into();
        }
        if crate::map::map_dir(&cache, &choices.values[3]).is_err()
            && let Some(id) = MAPS
                .iter()
                .find(|id| crate::map::map_dir(&cache, id).is_ok())
        {
            choices.values[3] = (*id).into();
            choices.values[4] = "0".into();
        }
        if crate::map::map_dir(&cache, &choices.values[3])
            .and_then(|dir| crate::map::player_start(&dir, choices.values[4].parse().unwrap_or(0)))
            .is_err()
        {
            choices.values[4] = "0".into();
        }
        let selected = if choices.explore() { 3 } else { 0 };
        Self {
            paused: start,
            start,
            choices,
            cache,
            selected: if start { selected } else { 0 },
            error: String::new(),
            extras,
            fight_script: crate::arg_value(args, "--menu-fight-script"),
            return_script: crate::arg_value(args, "--menu-return-script"),
            frame: 0,
            script,
            stick: IVec2::ZERO,
            activations: Vec::new(),
        }
    }
}
#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Row(usize);
#[derive(Component)]
struct Help;

fn activate_row(event: On<Activate>, rows: Query<&Row>, mut state: ResMut<PanelState>) {
    if state.paused
        && let Ok(row) = rows.get(event.entity)
    {
        state.activations.push(row.0);
    }
}
pub fn active(state: Res<PanelState>) -> bool {
    !state.paused
}

pub fn paused(state: Res<PanelState>) -> bool {
    state.paused && !state.start
}

/// Set when another screen (the in-game menu) draws the in-game pause: the panel stays hidden,
/// its own row keys are off, `request` takes one of its rows (an index into `PAUSE_ROWS`) as if
/// it were picked, and `movement` passes on this frame's arrow, D-pad and stick presses.
#[derive(Resource, Default)]
pub struct PauseView {
    pub request: Option<usize>,
    pub movement: IVec2,
}

/// Optional screens that gate or draw the pause panel.
type PauseScreens<'w> = (
    Option<Res<'w, crate::play::PlayGate>>,
    Option<Res<'w, Modal>>,
    Option<ResMut<'w, PauseView>>,
);

/// The pause state is settled before screens read it and handle their own keys.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PauseSystems;
pub fn install(app: &mut App) {
    let args: Vec<_> = std::env::args().collect();
    crate::game_ui::GameFonts::insert(app);
    app.insert_resource(PanelState::new(false, &args))
        .add_systems(Startup, spawn);
    install_input(app);
}

/// Set while a screen over the game takes Esc itself (the idol menu): the pause menu does not
/// open then.
#[derive(Resource, Default)]
pub struct Modal(pub bool);

/// The game's menu keys (FwLo's RustSekiro, after DefaultKeyAssignParam00): X / Z switch the
/// tab, E is OK and Q is back.
pub const MENU_NEXT_TAB_KEY: KeyCode = KeyCode::KeyX;
pub const MENU_PREVIOUS_TAB_KEY: KeyCode = KeyCode::KeyZ;
pub const MENU_OK_KEY: KeyCode = KeyCode::KeyE;
pub const MENU_BACK_KEY: KeyCode = KeyCode::KeyQ;

/// OK in a menu over the game: Enter, Space or E, or pad A.
pub fn ok_pressed(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>) -> bool {
    keys.any_just_pressed([KeyCode::Enter, KeyCode::Space, MENU_OK_KEY])
        || pads.iter().any(|p| p.just_pressed(GamepadButton::South))
}

/// Back in a menu over the game: Q, or pad B.
pub fn back_pressed(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>) -> bool {
    keys.just_pressed(MENU_BACK_KEY) || pads.iter().any(|p| p.just_pressed(GamepadButton::East))
}

/// Keys and mouse buttons held while a screen over the game (the pause or game menu, the idol
/// menu) was open. Play ignores each one until it is released, so a menu press (Q back, E OK,
/// Space, Enter) that is still held when the screen closes does not reach Wolf.
#[derive(Resource, Default)]
pub struct HeldThroughMenu {
    keys: Vec<KeyCode>,
    buttons: Vec<MouseButton>,
    was_open: bool,
}

impl HeldThroughMenu {
    pub fn key(&self, keys: &ButtonInput<KeyCode>, key: KeyCode) -> bool {
        keys.pressed(key) && !self.keys.contains(&key)
    }

    pub fn button(&self, mouse: &ButtonInput<MouseButton>, button: MouseButton) -> bool {
        mouse.pressed(button) && !self.buttons.contains(&button)
    }
}

/// Forgets released inputs, then while the screen is open (or on the frame it closed) takes
/// every held one.
fn track_held<T: Copy + Eq + std::hash::Hash + Send + Sync + 'static>(
    held: &mut Vec<T>,
    input: &ButtonInput<T>,
    open: bool,
) {
    held.retain(|i| input.pressed(*i));
    if open {
        for i in input.get_pressed() {
            if !held.contains(i) {
                held.push(*i);
            }
        }
    }
}

fn hold_through_menu(
    state: Res<PanelState>,
    modal: Option<Res<Modal>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut held: ResMut<HeldThroughMenu>,
) {
    let open = state.paused || modal.is_some_and(|m| m.0);
    // The frame a screen closes counts as open: its closing press is still this frame's input.
    let take = open || held.was_open;
    let held = &mut *held;
    track_held(&mut held.keys, &keys, take);
    if let Some(mouse) = mouse {
        track_held(&mut held.buttons, &mouse, take);
    }
    held.was_open = open;
}

fn install_input(app: &mut App) {
    app.init_resource::<HeldThroughMenu>().add_systems(
        PreUpdate,
        (
            update
                .in_set(PauseSystems)
                .after(bevy::input::InputSystems)
                .after(bevy::picking::PickingSystems::Last),
            hold_through_menu.after(PauseSystems),
        ),
    );
}
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let size = crate::arg_value(args, "--window-size").unwrap_or_else(|| "1600x900".into());
    let (w, h) = size
        .split_once('x')
        .ok_or_else(|| anyhow::anyhow!("--window-size wants WIDTHxHEIGHT"))?;
    let size: (u32, u32) = (w.parse()?, h.parse()?);
    anyhow::ensure!(
        size.0 >= 800 && size.1 >= 600,
        "--window-size minimum is 800x600"
    );
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Sekiro-rs".into(),
            resolution: size.into(),
            present_mode: crate::presentation::requested_mode(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.035, 0.042, 0.055)))
    .insert_resource(PanelState::new(true, args))
    .add_systems(
        Startup,
        (
            |mut commands: Commands| {
                commands.spawn(Camera2d);
            },
            spawn,
        ),
    )
    .add_systems(Last, save_on_exit);
    crate::game_ui::GameFonts::insert(&mut app);
    install_input(&mut app);
    app.add_plugins(crate::presentation::PresentationPlugin);
    app.run();
    Ok(())
}
fn save_on_exit(state: Res<PanelState>, mut exits: MessageReader<AppExit>) {
    if exits.read().next().is_some() {
        state.choices.save();
    }
}
fn spawn(mut commands: Commands, fonts: Res<crate::game_ui::GameFonts>, state: Res<PanelState>) {
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            GlobalZIndex(200),
            if state.paused {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(690),
                    max_width: percent(94),
                    padding: UiRect::all(px(24)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.028, 0.035, 0.94)),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("Sekiro-rs"),
                    fonts.ui(32.0),
                    TextColor(Color::srgb(0.91, 0.78, 0.54)),
                    Node {
                        margin: UiRect::bottom(px(12)),
                        ..default()
                    },
                ));
                for i in 0..if state.start {
                    LABELS.len()
                } else {
                    PAUSE_ROWS.len()
                } {
                    p.spawn((
                        Button,
                        ActivateOnPress,
                        Row(i),
                        Node {
                            width: percent(100),
                            min_height: px(30),
                            padding: UiRect::axes(px(10), px(3)),
                            ..default()
                        },
                    ))
                    .observe(activate_row)
                    .with_children(|r| {
                        r.spawn((Text::new(""), fonts.ui(18.0)));
                    });
                }
                p.spawn((
                    Help,
                    Text::new(""),
                    fonts.ui(14.0),
                    TextColor(Color::srgb(0.68, 0.71, 0.75)),
                    Node {
                        margin: UiRect::top(px(12)),
                        ..default()
                    },
                ));
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut typing: MessageReader<KeyboardInput>,
    mut state: ResMut<PanelState>,
    mut panels: Query<&mut Visibility, With<Panel>>,
    mut rows: Query<(&Row, &mut BackgroundColor, &Children, &mut Node)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    help: Query<Entity, With<Help>>,
    mut cursors: Query<&mut CursorOptions>,
    mut exit: MessageWriter<AppExit>,
    mut time: ResMut<Time<Virtual>>,
    (gate, modal, mut view): PauseScreens,
) {
    state.frame += 1;
    let actions: Vec<_> = state
        .script
        .iter()
        .filter(|(n, _)| *n == state.frame)
        .map(|(_, a)| a.clone())
        .collect();
    let mut activate = keys.just_pressed(KeyCode::Enter);
    let drawn = !state.start && view.is_some();
    let modal = !state.paused && modal.is_some_and(|m| m.0);
    if !state.start
        && !modal
        && (keys.just_pressed(KeyCode::Escape)
            || actions.iter().any(|a| a == "pause")
            || pads.iter().any(|p| p.just_pressed(GamepadButton::Start)))
    {
        state.paused = !state.paused;
        state.selected = 0;

        for mut c in &mut cursors {
            c.visible = state.paused;
            c.grab_mode = if state.paused {
                CursorGrabMode::None
            } else {
                CursorGrabMode::Locked
            };
        }
    }
    // Loading can show the pause panel, but its clock must keep advancing until ready.
    if !state.start {
        if state.paused && gate.is_none_or(|gate| gate.open()) {
            time.pause();
        } else {
            time.unpause();
        }
    }
    for mut p in &mut panels {
        *p = if state.paused && !drawn {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for a in &actions {
        if let Some(path) = a.strip_prefix("shot:") {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(PathBuf::from(path)));
        }
        if a == "quit" {
            exit.write(AppExit::Success);
        }
    }
    let pressed_rows = std::mem::take(&mut state.activations);
    if !state.paused {
        if let Some(view) = view.as_mut() {
            view.request = None;
            view.movement = IVec2::ZERO;
        }
        typing.clear();
        return;
    }
    let count = if state.start {
        LABELS.len()
    } else {
        PAUSE_ROWS.len()
    };
    let mut movement = IVec2::new(
        i32::from(keys.just_pressed(KeyCode::ArrowRight))
            - i32::from(keys.just_pressed(KeyCode::ArrowLeft)),
        i32::from(keys.just_pressed(KeyCode::ArrowDown))
            - i32::from(keys.just_pressed(KeyCode::ArrowUp)),
    );
    let mut stick = IVec2::ZERO;
    for p in &pads {
        activate |= p.just_pressed(GamepadButton::South);
        movement.x += i32::from(p.just_pressed(GamepadButton::DPadRight))
            - i32::from(p.just_pressed(GamepadButton::DPadLeft));
        movement.y += i32::from(p.just_pressed(GamepadButton::DPadDown))
            - i32::from(p.just_pressed(GamepadButton::DPadUp));
        let s = p.left_stick();
        stick = IVec2::new(
            if s.x.abs() > 0.6 {
                s.x.signum() as i32
            } else {
                0
            },
            if s.y.abs() > 0.6 {
                -s.y.signum() as i32
            } else {
                0
            },
        );
    }
    if stick != state.stick {
        movement += stick;
    }
    state.stick = stick;
    if drawn {
        if let Some(view) = view.as_mut() {
            view.movement = movement;
        }
        movement = IVec2::ZERO;
        activate = false;
    }
    for a in &actions {
        if let Some(setting) = a.strip_prefix("set:")
            && let Some((row, value)) = setting.split_once(':')
            && let Ok(row) = row.parse::<usize>()
            && row < 11
        {
            state.choices.values[row] = value.into();
        }
        match a.as_str() {
            "down" => movement.y += 1,
            "up" => movement.y -= 1,
            "right" => movement.x += 1,
            "left" => movement.x -= 1,
            "start" => {
                state.selected = count - 1;
                activate = true;
            }
            "enter" => activate = true,
            "back" => {
                state.selected = 1;
                activate = true;
            }
            _ => {}
        }
    }
    if drawn && let Some(row) = view.as_mut().and_then(|v| v.request.take()) {
        state.selected = row;
        activate = true;
    }
    if state.start && state.choices.explore() {
        let shortcuts = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
            KeyCode::Digit0,
        ];
        for (id, key) in MAPS.iter().zip(shortcuts) {
            if keys.just_pressed(key) && crate::map::map_dir(&state.cache, id).is_ok() {
                state.choices.values[3] = (*id).into();
                state.choices.values[4] = "0".into();
                state.selected = 3;
            }
        }
    }
    let visible: Vec<_> = if state.start {
        state
            .choices
            .visible_rows()
            .into_iter()
            .filter(|r| *r != 11)
            .collect()
    } else {
        (0..count).collect()
    };
    if movement.y != 0 || !visible.contains(&state.selected) {
        let index = visible
            .iter()
            .position(|r| *r == state.selected)
            .unwrap_or(0);
        state.selected =
            visible[(index as i32 + movement.y).rem_euclid(visible.len() as i32) as usize];
    }
    let row = state.selected;
    if state.start {
        let cache = state.cache.clone();
        state.choices.change(row, movement.x, &cache);
        if matches!(row, 1 | 10) && state.choices.enabled(row) {
            for input in typing.read().filter(|i| i.state.is_pressed() && !i.repeat) {
                if input.key_code == KeyCode::Backspace {
                    state.choices.values[row].pop();
                }
                if let Key::Character(s) = &input.logical_key {
                    for c in s
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':'))
                    {
                        if state.choices.values[row].len() < 40 {
                            state.choices.values[row].push(c);
                        }
                    }
                }
            }
        } else {
            typing.clear();
        }
    }
    for (r, mut bg, children, mut node) in &mut rows {
        let visible = !state.start || state.choices.visible_rows().contains(&r.0);
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if !visible {
            continue;
        }
        // Button activation includes label hits and a press/release in the same frame.
        if !drawn && pressed_rows.contains(&r.0) {
            state.selected = r.0;
            if state.start && r.0 < count - 1 {
                let cache = state.cache.clone();
                state.choices.change(r.0, 1, &cache);
            } else {
                activate = true;
            }
        }
        let disabled = state.start && r.0 == 6 && !state.choices.owl();
        let muted = state.start && !state.choices.enabled(r.0);
        *bg = BackgroundColor(if r.0 == state.selected {
            Color::srgba(0.55, 0.41, 0.20, 0.40)
        } else {
            Color::NONE
        });
        for child in children.iter() {
            if let Ok((mut t, mut color)) = texts.get_mut(child) {
                color.0 = if muted {
                    Color::srgb(0.42, 0.44, 0.48)
                } else {
                    Color::srgb(0.92, 0.90, 0.85)
                };
                t.0 = if state.start {
                    if r.0 == count - 1 {
                        if state.choices.explore() {
                            "Explore"
                        } else {
                            "Start"
                        }
                        .into()
                    } else if r.0 == 11 {
                        state.error.clone()
                    } else {
                        format!(
                            "{}:  {}",
                            LABELS[r.0],
                            if disabled {
                                "unavailable"
                            } else {
                                &state.choices.values[r.0]
                            }
                        )
                    }
                } else {
                    PAUSE_ROWS[r.0].into()
                };
            }
        }
    }
    for e in &help {
        if let Ok((mut t, _)) = texts.get_mut(e) {
            t.0 = if state.start {
                if state.choices.explore() {
                    let maps = MAPS
                        .iter()
                        .enumerate()
                        .filter(|(_, id)| crate::map::map_dir(&state.cache, id).is_ok())
                        .map(|(i, id)| format!("{}: {}", (i + 1) % 10, &id[..6]))
                        .collect::<Vec<_>>();
                    format!(
                        "Left/Right: map   Down: start point   Enter/A: explore\n{}\n{}\nWASD: walk   Mouse: look   Space: jump   Esc: pause   F3: debug text",
                        maps[..maps.len().min(5)].join("   "),
                        maps[maps.len().min(5)..].join("   ")
                    )
                } else {
                    "Up/Down: select   Left/Right or click: change   Enter/A: start   Type: id/name"
                        .into()
                }
            } else {
                "Up/Down: select   Enter/A or click: confirm   Esc/Start: resume".into()
            };
        }
    }
    if activate {
        if state.start {
            let mut args = state.choices.args();
            args.extend(state.extras.clone());
            args.extend([
                "--menu-choices".into(),
                serde_json::to_string(&state.choices.values).unwrap(),
            ]);
            if let Some(script) = &state.fight_script {
                args.extend(["--menu-script".into(), script.clone()]);
            }
            if let Some(script) = &state.return_script {
                args.extend(["--menu-return-script".into(), script.clone()]);
            }
            let mut cli = vec!["sekiro-rs".into()];
            cli.extend(args.clone());
            let validated = crate::play_options(&cli).and_then(|options| {
                if let Some(id) = options.map.as_deref().filter(|_| options.arena.is_none()) {
                    let dir = crate::map::map_dir(&options.cache, id)?;
                    crate::map::player_start(&dir, options.start)?;
                }
                Ok(options)
            });
            match validated {
                Ok(_) => {
                    state.choices.save();
                    *NEXT.lock().unwrap() = Some(args);
                    exit.write(AppExit::Success);
                }
                Err(e) => state.error = e.to_string(),
            }
        } else if !state.start {
            match state.selected {
                0 => {
                    state.paused = false;
                    time.unpause();
                    for mut c in &mut cursors {
                        c.visible = false;
                        c.grab_mode = CursorGrabMode::Locked;
                    }
                }
                1 => {
                    let mut args = vec![
                        "--menu".into(),
                        "--menu-choices".into(),
                        serde_json::to_string(&state.choices.values).unwrap(),
                    ];
                    args.extend(kept_args(&state.extras));
                    if let Some(script) = &state.return_script {
                        args.extend(["--menu-script".into(), script.clone()]);
                    }
                    *NEXT.lock().unwrap() = Some(args);
                    exit.write(AppExit::Success);
                }
                _ => {
                    exit.write(AppExit::Success);
                }
            }
        }
    }
}
