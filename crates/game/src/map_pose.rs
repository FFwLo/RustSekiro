//! File-queue camera search for the external real-game reference collection.
//! Requests and captures stay in the caller's output directory, never in source.
use super::*;
use std::collections::VecDeque;

#[derive(Resource)]
struct ShaderClock(Time);

/// The requested player position: a plain [`MapFocus`] beside the sweep camera's, so a
/// capture keeps the free-camera probe under both (never `map::PlayerFocus`, which would
/// switch it to the play-time standing part).
#[derive(Component)]
struct PoseFocus;

#[derive(Resource)]
pub(super) struct Server {
    directory: PathBuf,
    queue: VecDeque<Value>,
    current: Option<Value>,
    batch: Option<String>,
    completed: Vec<Value>,
    width: u32,
    height: u32,
}

pub(super) fn install(app: &mut App, directory: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&directory)?;
    let params = sekiro_sim::camera::CameraParams::load(&sekiro_sim::cache::root());
    std::fs::write(
        directory.join("camera-params.json"),
        serde_json::to_vec_pretty(&json!({
            "distance":params.cam_dist,"focus_height":params.chr_org_offset_y,
            "fov":params.cam_fov_y,"pitch_min":params.rot_range_x_min,
            "pitch_max":params.rot_range_x_max
        }))?,
    )?;
    let options = &app.world().resource::<Sweep>().options;
    let (width, height) = (options.width, options.height);
    let focuses: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, (With<MapFocus>, Without<SweepCamera>)>()
        .iter(app.world())
        .collect();
    for entity in focuses {
        app.world_mut().entity_mut(entity).remove::<MapFocus>();
    }
    app.world_mut().spawn((
        PoseFocus,
        MapFocus,
        Transform::default(),
        GlobalTransform::default(),
    ));
    // Let CPU lighting/LOD selectors advance normally between poses while all
    // candidates see one shader animation phase. Extracted shader time reads the
    // generic clock after Last; First restores the advancing virtual clock.
    let mut clock = Time::default();
    clock.advance_by(Duration::from_secs(1));
    clock.advance_by(Duration::ZERO);
    app.insert_resource(ShaderClock(clock))
        .add_systems(Last, |world: &mut World| {
            let clock = world.resource::<ShaderClock>().0;
            world.insert_resource(clock);
        });
    app.insert_resource(Server {
        directory,
        queue: VecDeque::new(),
        current: None,
        batch: None,
        completed: Vec::new(),
        width,
        height,
    });
    Ok(())
}

pub(super) fn tick(world: &mut World) {
    let mut server = world.remove_resource::<Server>().unwrap();
    let mut sweep = world.remove_resource::<Sweep>().unwrap();
    if let Err(error) = step(world, &mut server, &mut sweep) {
        error!("pose server: {error:#}");
        let _ = std::fs::write(server.directory.join("error.txt"), format!("{error:#}"));
        world.write_message(AppExit::error());
    }
    world.insert_resource(sweep);
    world.insert_resource(server);
}

fn step(world: &mut World, server: &mut Server, sweep: &mut Sweep) -> Result<()> {
    anyhow::ensure!(
        world.resource::<Pipelines>().status().failed == 0,
        "render pipeline failed"
    );
    if server.directory.join("stop").exists() {
        world.write_message(AppExit::Success);
        return Ok(());
    }
    if !world.resource::<Pending>().loaded() || world.resource::<Pending>().loaded_for() < 3. {
        anyhow::ensure!(
            sweep.started.elapsed() < Duration::from_secs(600),
            "map load timed out"
        );
        return Ok(());
    }
    if !sweep.initialized {
        let pending = world
            .query_filtered::<Entity, With<Mesh3d>>()
            .iter(world)
            .any(|e| {
                map_root(world, e)
                    && (map_material::sweep_base(world, e).is_some_and(|m| m.pending)
                        || crate::map_fog_surface::sweep_pending(world, e))
            });
        if pending {
            anyhow::ensure!(
                sweep.started.elapsed() < Duration::from_secs(600),
                "textures timed out"
            );
            return Ok(());
        }
        sweep.initialized = true;
        std::fs::write(server.directory.join("ready"), b"windowless")?;
    }
    if let Some(image) = world.resource_mut::<Received>().0.take() {
        let request = server.current.take().context("unexpected pose image")?;
        let id = request["id"].as_u64().context("pose id")?;
        image
            .clone()
            .try_into_dynamic()?
            .save(server.directory.join(format!("{id:07}.png")))?;
        let mut result = json!({"id":id,"view":request["view"],"fov":request["fov"]});
        if request["manifest"].as_bool() == Some(true) {
            let selected = world
                .get_resource::<super::super::world::MapWorld>()
                .and_then(|w| w.selected.clone());
            let mut query = world.query::<(
                &super::super::visibility::VisibilityPart,
                &super::super::visibility::VisibilityDecision,
            )>();
            let blocks = world.resource::<super::super::visibility::VisibilityBlocks>();
            let decisions: Vec<_> = query
                .iter(world)
                .filter_map(|(part, decision)| {
                    blocks
                        .0
                        .get(&part.map)
                        .map(|block| super::super::visibility::diagnostic(part, block, decision))
                })
                .collect();
            result["visibility"] = json!({"selected_collision":selected,"parts":decisions});
        }
        server.completed.push(result);
        sweep.waiting = false;
    }
    if server.current.is_none() {
        if let Some(request) = server.queue.pop_front() {
            let player: [f32; 3] = serde_json::from_value(request["player"].clone())?;
            anyhow::ensure!(
                player.iter().all(|n| n.is_finite()),
                "invalid player position"
            );
            for (mut transform, mut global) in world
                .query_filtered::<(&mut Transform, &mut GlobalTransform), With<PoseFocus>>()
                .iter_mut(world)
            {
                transform.translation = Vec3::from_array(player);
                *global = GlobalTransform::from(*transform);
            }
            let view = fixed_views(&json!([request["view"]]))?.remove(0);
            let fov = request["fov"].as_f64().context("pose fov")? as f32;
            anyhow::ensure!((10. ..150.).contains(&fov), "invalid fov");
            let width = request["width"].as_u64().unwrap_or(u64::from(server.width)) as u32;
            let height = request["height"]
                .as_u64()
                .unwrap_or(u64::from(server.height)) as u32;
            anyhow::ensure!(
                (16..=4096).contains(&width) && (16..=4096).contains(&height),
                "invalid size"
            );
            if (width, height) != (server.width, server.height) {
                world.resource_mut::<Assets<Image>>().insert(
                    sweep.target.id(),
                    Image::new_target_texture(
                        width,
                        height,
                        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                        None,
                    ),
                )?;
                server.width = width;
                server.height = height;
            }
            for mut projection in world
                .query_filtered::<&mut Projection, With<SweepCamera>>()
                .iter_mut(world)
            {
                *projection = PerspectiveProjection {
                    fov: fov.to_radians(),
                    aspect_ratio: width as f32 / height as f32,
                    ..default()
                }
                .into();
            }
            sweep.views[0] = view;
            sweep.index = 0;
            sweep.phase = 0;
            // Apply before the next Update, as the canonical sweep does, so
            // light selection and LOD see the same pose-settling time step.
            configure(world, sweep);
            server.current = Some(request);
        } else if let Some(batch) = server.batch.take() {
            let tmp = server.directory.join("result.tmp");
            std::fs::write(&tmp, serde_json::to_vec(&server.completed)?)?;
            std::fs::rename(tmp, server.directory.join(format!("{batch}.done.json")))?;
            server.completed.clear();
        } else {
            let path = server.directory.join("request.json");
            if path.exists() {
                let value: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
                let batch = value["batch"].as_str().context("batch name")?;
                anyhow::ensure!(
                    batch.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                    "invalid batch name"
                );
                server.batch = Some(batch.to_string());
                server.queue = value["requests"]
                    .as_array()
                    .context("requests array")?
                    .iter()
                    .cloned()
                    .collect();
                std::fs::remove_file(path)?;
            }
        }
    }
    if server.current.is_none() {
        for mut camera in world
            .query_filtered::<&mut Camera, With<SweepCamera>>()
            .iter_mut(world)
        {
            camera.is_active = false;
        }
    }
    for mut post in world
        .query_filtered::<&mut crate::lighting::tonemap::GamePost, With<SweepCamera>>()
        .iter_mut(world)
    {
        post.controls.w = f32::from(sweep.waiting);
        post.adaptation.z = 4.;
        post.adaptation.w = 4.;
    }
    if server.current.is_some() && !sweep.waiting && !sweep.configure_pending {
        anyhow::ensure!(
            sweep.phase_started.elapsed() < Duration::from_secs(180),
            "pose capture timed out"
        );
        if sweep.countdown > 0 {
            sweep.countdown -= 1;
        } else if world.resource::<Pipelines>().status().pending == 0 {
            sweep.stable_pipelines += 1;
            if sweep.stable_pipelines >= 3 {
                world
                    .spawn(Screenshot::image(sweep.target.clone()))
                    .observe(
                        |event: On<ScreenshotCaptured>, mut received: ResMut<Received>| {
                            received.0 = Some(event.image.clone());
                        },
                    );
                sweep.waiting = true;
            }
        } else {
            sweep.stable_pipelines = 0;
        }
    }
    Ok(())
}
