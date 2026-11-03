//! Offscreen, deterministic visual coverage measurements using the shared map renderer.
use super::ready::Pipelines;
use super::*;
use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::{embedded_asset, embedded_path};
use bevy::camera::CameraProjection;
use bevy::camera::{RenderTarget, visibility::RenderLayers};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::render::render_resource::AsBindGroup;
use bevy::render::view::screenshot::ScreenshotCaptured;
use bevy::shader::ShaderRef;
use serde_json::json;
use std::collections::BTreeMap;

#[path = "map_pose.rs"]
mod pose;

#[path = "map_sweep_materials.rs"]
mod material_audit;

#[derive(Clone, Asset, TypePath, AsBindGroup)]
struct Diagnostic {
    #[uniform(100)]
    bad: Vec4,
}
impl MaterialExtension for Diagnostic {
    fn fragment_shader() -> ShaderRef {
        bevy::asset::AssetPath::from_path_buf(embedded_path!("map_sweep.wesl"))
            .with_source("embedded")
            .into()
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}
type DiagnosticMaterial = ExtendedMaterial<StandardMaterial, Diagnostic>;

#[derive(Clone, Debug)]
struct View {
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    edge: bool,
}
#[derive(Clone)]
struct Options {
    map: String,
    spacing: f32,
    yaws: usize,
    pitches: Vec<f32>,
    width: u32,
    height: u32,
    settle: usize,
    worst: usize,
    objects: bool,
    out: PathBuf,
    gate: Option<Value>,
    raw_collision: bool,
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let map = crate::arg_value(args, "--map").context("--sweep requires --map <id>")?;
        anyhow::ensure!(
            map.len() == 12
                && map.starts_with('m')
                && map
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '_' || c == 'm'),
            "invalid map id"
        );
        let spacing = crate::arg_f32(args, "--spacing", 80.)?;
        let yaws = crate::arg_f32(args, "--views", 4.)?;
        let width = crate::arg_f32(args, "--sweep-width", 256.)?;
        let height = crate::arg_f32(args, "--sweep-height", 144.)?;
        let settle = crate::arg_f32(args, "--settle-frames", 4.)?;
        let worst = crate::arg_f32(args, "--worst", 20.)?;
        for (name, n, min, max) in [
            ("spacing", spacing, 1., 1000.),
            ("views", yaws, 1., 64.),
            ("width", width, 16., 4096.),
            ("height", height, 16., 4096.),
            ("settle frames", settle, 4., 1000.),
            ("worst", worst, 0., 1000.),
        ] {
            anyhow::ensure!(n.is_finite() && n >= min && n <= max, "invalid {name}");
            if name != "spacing" {
                anyhow::ensure!(n.fract() == 0., "{name} must be an integer");
            }
        }
        let pitches = crate::arg_value(args, "--edge-pitches")
            .unwrap_or_else(|| "-25,-50".into())
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<Vec<f32>, _>>()?;
        anyhow::ensure!(
            pitches
                .iter()
                .all(|p| p.is_finite() && *p >= -85. && *p < 0.),
            "edge pitches must be in [-85,0)"
        );
        let reference_gate = args.iter().any(|a| a == "--reference-gate");
        let render_gate = reference_gate || args.iter().any(|a| a == "--render-gate");
        if render_gate {
            let mut i = usize::from(args.first().is_some_and(|a| !a.starts_with("--")));
            while i < args.len() {
                i += match args[i].as_str() {
                    "--render-gate" | "--reference-gate" | "--sweep" => 1,
                    "--map" | "--cache" | "--sweep-out" => 2,
                    flag => anyhow::bail!(
                        "render gate requires canonical material/story/camera settings, rejected {flag}; use --sweep-views for diagnostic controls"
                    ),
                };
            }
        }
        let objects = render_gate || args.iter().any(|a| a == "--objects");
        let out = PathBuf::from(
            crate::arg_value(args, "--sweep-out").unwrap_or_else(|| "target/alt-hs/sweep".into()),
        )
        .join(&map)
        .join(if objects { "objects" } else { "pieces" });
        let gate = if render_gate {
            anyhow::ensure!(
                matches!(map.as_str(), "m11_00_00_00" | "m11_01_00_00"),
                "render gate supports the two Ashina areas"
            );
            Some(serde_json::from_slice::<Value>(&std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "tests/data/{map}.{}.json",
                    if reference_gate {
                        "reference"
                    } else {
                        "render"
                    }
                )),
            )?)?)
        } else {
            None
        };
        if !reference_gate && let Some(gate) = &gate {
            anyhow::ensure!(
                gate["story"].as_str() == Some(super::story::REFERENCE_STORY),
                "render fixture story must match the canonical reference preset"
            );
        }
        Ok(Self {
            map,
            spacing,
            yaws: yaws as usize,
            pitches,
            width: gate
                .as_ref()
                .map_or(width as u32, |g| g["width"].as_u64().unwrap() as u32),
            height: gate
                .as_ref()
                .map_or(height as u32, |g| g["height"].as_u64().unwrap() as u32),
            settle: settle as usize,
            worst: if reference_gate { 1000 } else { worst as usize },
            objects,
            out,
            gate,
            raw_collision: args.iter().any(|a| a == "--sweep-raw-collision"),
        })
    }
}

fn fixed_views(rows: &Value) -> Result<Vec<View>> {
    rows.as_array()
        .context("fixed views must be an array")?
        .iter()
        .map(|row| {
            let row = row.as_array().context("fixed view must be an array")?;
            anyhow::ensure!(row.len() >= 5, "fixed view needs x,y,z,yaw,pitch");
            let n: Vec<f32> = row
                .iter()
                .take(5)
                .map(|n| n.as_f64().context("invalid view number").map(|n| n as f32))
                .collect::<Result<_>>()?;
            anyhow::ensure!(
                n.iter().all(|n| n.is_finite()) && n[4].abs() < 90.,
                "invalid fixed camera"
            );
            Ok(View {
                eye: Vec3::new(n[0], n[1], n[2]),
                yaw: n[3],
                pitch: n[4],
                edge: n[4] < -20.,
            })
        })
        .collect()
}

/// Sample every horizontal grid cell covered by walkable collision, retaining floors on
/// separate vertical layers. A ceiling/headroom check rejects undersides and enclosed points.
fn sample(mesh: &sekiro_formats::hknp::CollisionMesh, options: &Options) -> Vec<View> {
    let collision = sekiro_sim::collision::CollisionWorld::from_mesh(mesh);
    let mut cells = BTreeMap::new();
    let spacing = options.spacing;
    for tri in mesh.indices.as_chunks::<3>().0 {
        let t = tri.map(|i| Vec3::from(mesh.vertices[i as usize]));
        let normal = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
        if normal.y.abs() < 50f32.to_radians().cos() {
            continue;
        }
        let lo = t[0].min(t[1]).min(t[2]);
        let hi = t[0].max(t[1]).max(t[2]);
        for x in (lo.x / spacing - 0.5).ceil() as i32..=(hi.x / spacing - 0.5).floor() as i32 {
            for z in (lo.z / spacing - 0.5).ceil() as i32..=(hi.z / spacing - 0.5).floor() as i32 {
                let p = Vec3::new((x as f32 + 0.5) * spacing, 0., (z as f32 + 0.5) * spacing);
                if let Some(h) = vertical_hit(p, t[0], t[1], t[2]) {
                    let y = -h;
                    cells
                        .entry((x, (y * 4.).round() as i32, z))
                        .or_insert(Vec3::new(p.x, y, p.z));
                }
            }
        }
    }
    let mut views = Vec::new();
    for feet in cells.into_values() {
        if collision
            .floor(feet.x, feet.z, feet.y + 0.1)
            .is_none_or(|y| (y - feet.y).abs() > 0.15)
            || collision
                .raycast(feet + Vec3::Y * 0.1, Vec3::Y, 1.65)
                .is_some()
        {
            continue;
        }
        let edge = [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y].iter().any(|d| {
            let p = feet.xz() + *d * 2.;
            collision
                .floor(p.x, p.y, feet.y + 0.5)
                .is_none_or(|h| feet.y - h > 3.)
        });
        for yaw in 0..options.yaws {
            for pitch in std::iter::once(&0.).chain(options.pitches.iter().filter(|_| edge)) {
                views.push(View {
                    eye: feet + Vec3::Y * 1.65,
                    yaw: yaw as f32 * 360. / options.yaws as f32,
                    pitch: *pitch,
                    edge,
                });
            }
        }
    }
    views
}

/// A zero-demand collision cannot select scenery under the confirmed mask rule.
/// Do not rank its below-landscape planes as missing walkable visual surfaces.
fn collision_reference(
    mesh: &sekiro_formats::hknp::CollisionMesh,
    layout: &Value,
    enabled: impl Fn(&str) -> bool,
) -> Result<sekiro_formats::hknp::CollisionMesh> {
    let mut keep = vec![false; mesh.triangle_count()];
    for row in layout["parts"]
        .as_array()
        .context("survey requires complete layout; refresh with --layout-only")?
        .iter()
        .filter(|r| r["kind"] == "Collision")
    {
        if !enabled(row["name"].as_str().context("collision name")?)
            || super::visibility::display_mask(row)? == [0; 8]
        {
            continue;
        }
        if let Some(range) = row["triangles"].as_array() {
            let first = range[0].as_u64().context("collision range start")? as usize;
            let count = range[1].as_u64().context("collision range length")? as usize;
            anyhow::ensure!(first + count <= keep.len(), "collision range outside mesh");
            keep[first..first + count].fill(true);
        }
    }
    let mut indices = Vec::new();
    let mut materials = Vec::new();
    for (i, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        if keep[i]
            && !mesh
                .materials
                .get(i)
                .is_some_and(|&m| sekiro_sim::body::deep_water(m))
        {
            indices.extend_from_slice(tri);
            materials.push(mesh.materials.get(i).copied().unwrap_or(u32::MAX));
        }
    }
    Ok(sekiro_formats::hknp::CollisionMesh {
        vertices: mesh.vertices.clone(),
        indices,
        materials,
    })
}

#[derive(Component)]
struct Duplicate {
    source: Entity,
}
#[derive(Component)]
struct SweepCamera;
#[derive(Component)]
struct DepthCamera(usize);
#[derive(Resource)]
struct Received(Option<Image>);
#[derive(Resource)]
struct Sweep {
    options: Options,
    views: Vec<View>,
    index: usize,
    phase: usize,
    countdown: usize,
    waiting: bool,
    initialized: bool,
    frames: Vec<Image>,
    reports: Vec<Value>,
    worst: Vec<(u64, usize, Vec<Image>)>,
    target: Handle<Image>,
    started: Instant,
    phase_started: Instant,
    roots: Vec<(Entity, Visibility)>,
    unculled: Vec<Entity>,
    total_views: usize,
    started_unix_seconds: u64,
    configure_pending: bool,
    collision_oracle: sekiro_sim::collision::CollisionWorld,
    depth_target: Handle<Image>,
    collision_target: Handle<Image>,
    next_loading_log: Instant,
    normal_manifest: Value,
    normal_fog: Option<crate::lighting::fog::GameFog>,
    normal_materials: Value,
    stable_pipelines: usize,
    last_pipeline_frame: u64,
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    let run_started = Instant::now();
    let mut options = Options::parse(args)?;
    let cache = sekiro_sim::cache::root();
    let dir = map_dir(&cache, &options.map)?;
    let raw_mesh = sekiro_formats::hknp::CollisionMesh::from_bytes(&std::fs::read(
        dir.join("collision.bin"),
    )?)?;
    let mesh = if options.raw_collision {
        raw_mesh
    } else {
        let layout: Value = serde_json::from_slice(&std::fs::read(dir.join("layout.json"))?)?;
        let story = super::story::load(&cache, &options.map, &layout)?;
        collision_reference(&raw_mesh, &layout, |name| {
            !story
                .as_ref()
                .and_then(|s| s.part(name))
                .is_some_and(|p| !p.enabled)
        })?
    };
    let mut views = if let Some(gate) = &options.gate {
        fixed_views(&gate["views"])?
    } else if let Some(path) = crate::arg_value(args, "--sweep-views") {
        fixed_views(&serde_json::from_slice(&std::fs::read(path)?)?)?
    } else {
        sample(&mesh, &options)
    };
    let total_views = views.len();
    if let Some(limit) = crate::arg_value(args, "--limit-views") {
        anyhow::ensure!(
            options.gate.is_none(),
            "render gate must run every fixed view"
        );
        let limit: usize = limit.parse()?;
        anyhow::ensure!(limit > 0, "view limit must be positive");
        views.truncate(limit);
        options.out = options.out.join(format!("smoke-{limit}"));
    }
    anyhow::ensure!(!views.is_empty(), "no camera points at this spacing");
    println!(
        "sweep {}: {} views, spacing {} m, {}x{}, objects {}",
        options.map,
        views.len(),
        options.spacing,
        options.width,
        options.height,
        options.objects
    );
    if args.iter().any(|a| a == "--sample-only") {
        return Ok(());
    }
    std::fs::create_dir_all(&options.out)?;
    let mut app = App::new();
    raise_io_stack();
    app.add_plugins(
        DefaultPlugins
            .set(crate::vram::render_plugin())
            .set(layered_gltf_plugin())
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .set(AssetPlugin {
                file_path: cache.to_string_lossy().into_owned(),
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::audio::AudioPlugin>(),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(1)));
    // MaterialPlugin requests its shaders immediately in Bevy 0.20.
    embedded_asset!(&mut app, "map_sweep.wesl");
    app.add_plugins(MaterialPlugin::<DiagnosticMaterial>::default());
    super::capture::install(&mut app);
    // Keep scrolling water/clouds identical in the normal/all comparison.
    // Advance once per pose to allow the shared 0.5 s local-light selection to run.
    let mut clock = app.world_mut().resource_mut::<Time<Virtual>>();
    clock.advance_by(Duration::from_secs(1));
    clock.pause();
    add_map(
        &mut app,
        &dir,
        MapScene {
            prefix: format!("maps/{}/", options.map),
            collision: false,
            no_pieces: false,
            hide: Vec::new(),
            objects: options.objects,
            groups: GroupMode::Draw,
            markers: false,
            hour: f32::NAN,
            gparam: None,
            within: None,
            focus: None,
        },
        Default::default(),
    )?;
    if (options.gate.is_some() || crate::arg_value(args, "--sweep-views").is_some())
        && !args.iter().any(|a| a == "--sweep-all-assets")
    {
        let shadow = app.world().resource::<ShadowSettings>();
        let caster_distance = if shadow.enabled {
            shadow.caster_distance.unwrap_or(shadow.distance)
        } else {
            0.
        };
        let projection = PerspectiveProjection {
            aspect_ratio: options.width as f32 / options.height as f32,
            ..super::viewer_perspective()
        };
        let capture_views = super::world::CaptureViews {
            views: views
                .iter()
                .map(|v| {
                    let camera = GlobalTransform::from(
                        Transform::from_translation(v.eye).with_rotation(Quat::from_euler(
                            EulerRot::YXZ,
                            v.yaw.to_radians(),
                            v.pitch.to_radians(),
                            0.,
                        )),
                    );
                    (projection.compute_frustum(&camera), v.eye)
                })
                .collect(),
            caster_distance,
        };
        if let Some(mut connected) = app.world_mut().get_resource_mut::<super::world::MapWorld>() {
            connected.capture_views = Some(capture_views);
        }
    }
    super::ready::install(&mut app);
    let image = Image::new_target_texture(
        options.width,
        options.height,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        None,
    );
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    let depth_target =
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                options.width,
                options.height,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                None,
            ));
    let collision_target =
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                options.width,
                options.height,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                None,
            ));
    app.world_mut().spawn((
        Camera3d::default(),
        super::viewer_projection(),
        Camera::default(),
        Msaa::Off,
        RenderTarget::Image(target.clone().into()),
        Transform::default(),
        MapFocus,
        SweepCamera,
        bevy::camera::ShadowLodOrigin,
        bevy::core_pipeline::prepass::DepthPrepass,
        bevy::core_pipeline::prepass::MotionVectorPrepass,
    ));
    for (layer, target) in [(1, depth_target.clone()), (2, collision_target.clone())] {
        app.world_mut().spawn((
            Camera3d::default(),
            super::viewer_projection(),
            Camera {
                is_active: false,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            Msaa::Off,
            RenderTarget::Image(target.into()),
            Transform::default(),
            DepthCamera(if layer == 1 { 2 } else { 3 }),
            RenderLayers::layer(layer),
        ));
    }
    // Collision is rendered by the same projection/rasterizer as visual geometry.
    let mut collision_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    collision_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, mesh.vertices.clone());
    collision_mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![[0., 1., 0.]; mesh.vertices.len()],
    );
    collision_mesh.insert_indices(Indices::U32(
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                !mesh
                    .materials
                    .get(*i)
                    .is_some_and(|&m| sekiro_sim::body::deep_water(m))
            })
            .flat_map(|(_, t)| *t)
            .collect(),
    ));
    let mesh_handle = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(collision_mesh);
    let material = app
        .world_mut()
        .resource_mut::<Assets<DiagnosticMaterial>>()
        .add(DiagnosticMaterial {
            base: StandardMaterial {
                cull_mode: None,
                ..default()
            },
            extension: Diagnostic { bad: Vec4::ZERO },
        });
    app.world_mut().spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material),
        Transform::default(),
        RenderLayers::layer(2),
    ));
    app.insert_resource(Received(None))
        .insert_resource(Sweep {
            options,
            views,
            index: 0,
            phase: 0,
            countdown: 0,
            waiting: false,
            initialized: false,
            frames: Vec::new(),
            reports: Vec::new(),
            worst: Vec::new(),
            target,
            started: run_started,
            phase_started: Instant::now(),
            roots: Vec::new(),
            unculled: Vec::new(),
            total_views,
            started_unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            configure_pending: false,
            collision_oracle: sekiro_sim::collision::CollisionWorld::from_mesh(&mesh),
            depth_target,
            collision_target,
            next_loading_log: Instant::now() + Duration::from_secs(15),
            normal_manifest: Value::Null,
            normal_fog: None,
            normal_materials: Value::Null,
            stable_pipelines: 0,
            last_pipeline_frame: 0,
        })
        .add_systems(First, apply_pending_view.after(bevy::time::TimeSystems))
        .add_systems(
            PostUpdate,
            all_visible
                .after(super::visibility::VisibilityWrite)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
        )
        .add_systems(
            PostUpdate,
            tick.after(bevy::transform::TransformSystems::Propagate),
        );
    if let Some(directory) = crate::arg_value(args, "--pose-server") {
        pose::install(&mut app, PathBuf::from(directory))?;
    }
    let exit = app.run();
    anyhow::ensure!(exit == AppExit::Success, "sweep failed; see diagnostics");
    Ok(())
}

fn map_root(world: &World, mut e: Entity) -> bool {
    loop {
        if world.get::<DrawGroups>(e).is_some() {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(e) else {
            return false;
        };
        e = parent.parent();
    }
}
fn all_visible(sweep: Res<Sweep>, mut roots: Query<&mut Visibility, With<DrawGroups>>) {
    if sweep.phase == 1 && !sweep.configure_pending {
        for mut visible in &mut roots {
            *visible = Visibility::Visible;
        }
    }
}

fn initialize(world: &mut World, objects: bool) {
    // Connected exploration normally includes ordinary objects unconditionally.
    // Keep the sweep's explicit pieces/objects controls distinct using authored part kinds.
    let mut object_names = std::collections::HashSet::new();
    if !objects && let Some(connected) = world.get_resource::<super::world::MapWorld>() {
        for (id, block) in &connected.blocks {
            for row in block.json["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["kind"] == "Object")
            {
                if let Some(name) = row["name"].as_str() {
                    object_names.insert((id.clone(), name.to_string()));
                }
            }
        }
    }
    let excluded: std::collections::HashSet<_> = world
        .query::<(Entity, &super::visibility::VisibilityPart)>()
        .iter(world)
        .filter(|(_, p)| object_names.contains(&(p.map.clone(), p.name.clone())))
        .map(|(e, _)| e)
        .collect();
    let sources: Vec<_> = world
        .query::<(Entity, &Mesh3d, &GlobalTransform)>()
        .iter(world)
        .filter(|(e, _, _)| map_root(world, *e))
        .map(|(e, m, t)| (e, m.clone(), t.compute_transform()))
        .collect();
    let mut cache = std::collections::HashMap::new();
    for (entity, mesh, transform) in sources {
        let mut ancestor = entity;
        let mut omit = false;
        loop {
            if excluded.contains(&ancestor) {
                omit = true;
                break;
            }
            let Some(parent) = world.get::<ChildOf>(ancestor) else {
                break;
            };
            ancestor = parent.parent();
        }
        if omit {
            world.entity_mut(entity).insert(RenderLayers::layer(3));
            continue;
        }
        if let Some(material) = crate::map_fog_surface::sweep_material(world, entity) {
            world.spawn((
                mesh,
                MeshMaterial3d(material),
                transform,
                Duplicate { source: entity },
                RenderLayers::layer(1),
            ));
            continue;
        }
        let Some(map_material::SweepMaterial {
            key,
            mut base,
            missing,
            ..
        }) = map_material::sweep_base(world, entity)
        else {
            continue;
        };
        if missing && !cache.contains_key(&key) {
            println!(
                "sweep untextured material: {:?}, base absent={}, base path={:?}",
                world.get::<bevy::gltf::GltfMaterialName>(entity),
                base.base_color_texture.is_none(),
                base.base_color_texture
                    .as_ref()
                    .and_then(|h| world.resource::<AssetServer>().get_path(h.id()))
            );
        }
        // Byte-encoded depth must not be added/multiplied/blended with a second surface.
        // Retain authored cutouts; approximate translucent coverage with a small alpha cutoff.
        if !matches!(base.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) {
            base.alpha_mode = AlphaMode::Mask(0.05);
        }
        let material = cache
            .entry(key)
            .or_insert_with(|| {
                world
                    .resource_mut::<Assets<DiagnosticMaterial>>()
                    .add(DiagnosticMaterial {
                        base,
                        extension: Diagnostic {
                            bad: Vec4::splat(f32::from(missing)),
                        },
                    })
            })
            .clone();
        world.spawn((
            mesh,
            MeshMaterial3d(material),
            transform,
            Duplicate { source: entity },
            RenderLayers::layer(1),
        ));
    }
}

fn configure(_world: &mut World, sweep: &mut Sweep) {
    // Camera activation must precede Bevy's camera and shadow-cascade preparation.
    sweep.configure_pending = true;
    sweep.countdown = sweep.options.settle;
    sweep.phase_started = Instant::now();
    sweep.stable_pipelines = 0;
}

fn apply_pending_view(world: &mut World) {
    let mut sweep = world.remove_resource::<Sweep>().unwrap();
    if sweep.configure_pending {
        sweep.configure_pending = false;
        apply_view(world, &mut sweep);
    }
    world.insert_resource(sweep);
}

fn apply_view(world: &mut World, sweep: &mut Sweep) {
    if sweep.phase == 0 {
        let time = {
            let mut clock = world.resource_mut::<Time<Virtual>>();
            clock.advance_by(Duration::from_millis(600));
            clock.as_generic()
        };
        // Give the shared LOD crossfade its pose-settling step, then hold time for comparison.
        world.insert_resource(time);
    }
    let view = &sweep.views[sweep.index];
    let transform = Transform::from_translation(view.eye).with_rotation(Quat::from_euler(
        EulerRot::YXZ,
        view.yaw.to_radians(),
        view.pitch.to_radians(),
        0.,
    ));
    for mut t in world
        .query_filtered::<&mut Transform, With<Camera3d>>()
        .iter_mut(world)
    {
        *t = transform;
    }
    for (mut camera, depth) in world
        .query::<(&mut Camera, Option<&DepthCamera>)>()
        .iter_mut(world)
    {
        camera.is_active = depth.map_or(sweep.phase < 2, |d| sweep.phase == d.0);
    }
    // Reset the native group selector for each view and comparison, retaining its floor choice.
    let mut groups = world.resource_mut::<Groups>();
    groups.mode = if sweep.phase == 1 {
        GroupMode::Off
    } else {
        GroupMode::Draw
    };
    groups.checked_at = None;
    groups.dirty = true;
    if sweep.phase == 1 {
        sweep.roots = world
            .query_filtered::<(Entity, &Visibility), With<DrawGroups>>()
            .iter(world)
            .map(|(e, v)| (e, *v))
            .collect();
        sweep.unculled = world
            .query_filtered::<Entity, (With<Mesh3d>, Without<NoFrustumCulling>)>()
            .iter(world)
            .collect();
        for e in &sweep.unculled {
            world.entity_mut(*e).insert(NoFrustumCulling);
        }
    } else if sweep.phase == 2 {
        for (e, v) in sweep.roots.drain(..) {
            world.entity_mut(e).insert(v);
        }
        for e in sweep.unculled.drain(..) {
            world.entity_mut(e).remove::<NoFrustumCulling>();
        }
    }
}

fn tick(world: &mut World) {
    if world.contains_resource::<pose::Server>() {
        pose::tick(world);
        return;
    }
    let mut sweep = world.remove_resource::<Sweep>().unwrap();
    // Keep native post operators off diagnostic targets even during asset warm-up.
    let cameras: Vec<_> = world
        .query_filtered::<Entity, With<DepthCamera>>()
        .iter(world)
        .collect();
    for e in cameras {
        world.entity_mut(e).remove::<(
            crate::lighting::tonemap::GamePost,
            crate::lighting::fog::GameFog,
            crate::lighting::grading::GradingLut,
            bevy::light::Skybox,
        )>();
    }
    if world.resource::<Pipelines>().status().failed > 0 {
        error!("sweep aborted: a render pipeline failed");
        world.write_message(AppExit::error());
        world.insert_resource(sweep);
        return;
    }
    // Fail rather than silently report a partially loaded or stalled render as a hole.
    if (!sweep.initialized && sweep.started.elapsed() > Duration::from_secs(600))
        || (sweep.initialized && sweep.phase_started.elapsed() > Duration::from_secs(180))
    {
        error!(
            "sweep timed out at view {} phase {}",
            sweep.index, sweep.phase
        );
        world.write_message(AppExit::error());
        world.insert_resource(sweep);
        return;
    }
    // Keep the logical material inventory fixed between the normal and all-visible poses.
    // Mesh batches install over several frames after scene loading finishes.
    if !super::ready(
        world.get_resource::<Pending>(),
        world.get_resource::<super::StaticBatches>(),
        world.get_resource::<crate::pipeline_warmup::PipelineWarmup>(),
    ) || world.resource::<Pending>().loaded_for() < 3.
    {
        if Instant::now() >= sweep.next_loading_log {
            let pending = world.resource::<Pending>();
            println!(
                "sweep loading: {} / {} scenes ready",
                pending.ready, pending.instances
            );
            sweep.next_loading_log = Instant::now() + Duration::from_secs(15);
        }
        world.insert_resource(sweep);
        return;
    }
    // Check all resident blocks before every phase, not just the first normal view.
    let sources: Vec<_> = world
        .query_filtered::<Entity, With<Mesh3d>>()
        .iter(world)
        .filter(|e| map_root(world, *e))
        .collect();
    if super::ready::textures_pending(world, &sources) {
        if Instant::now() >= sweep.next_loading_log {
            println!("sweep loading: material textures pending");
            sweep.next_loading_log = Instant::now() + Duration::from_secs(15);
        }
        sweep.stable_pipelines = 0;
        world.insert_resource(sweep);
        return;
    }
    if !sweep.initialized {
        let audit = material_audit::snapshot(world);
        if let Err(error) = std::fs::write(
            sweep.options.out.join("texture-audit.json"),
            serde_json::to_vec_pretty(&audit).unwrap(),
        ) {
            error!("sweep texture audit: {error}");
            world.write_message(AppExit::error());
        }
        if sweep.options.gate.is_some() && material_audit::failed_requests(&audit) > 0 {
            error!(
                "render gate aborted: declared textures failed in the connected neighbourhood; see texture-audit.json"
            );
            world.write_message(AppExit::error());
            world.insert_resource(sweep);
            return;
        }
        initialize(world, sweep.options.objects);
        sweep.initialized = true;
        configure(world, &mut sweep);
    }
    let visibility: Vec<_> = world
        .query::<(Entity, &Duplicate)>()
        .iter(world)
        .map(|(e, d)| {
            (
                e,
                world
                    .get::<InheritedVisibility>(d.source)
                    .is_some_and(|v| v.get()),
            )
        })
        .collect();
    // Meter native exposure in the normal view, then hold the same GPU history
    // through its screenshot and the all-visible control. Do not disable it.
    for (mut post, _) in world
        .query::<(&mut crate::lighting::tonemap::GamePost, &SweepCamera)>()
        .iter_mut(world)
    {
        post.controls.w = f32::from(sweep.phase != 0 || sweep.waiting);
        // The recovered native reset argument is four. Settle against this view,
        // independent of frozen shader time and asynchronous scene startup.
        post.adaptation.z = 4.;
        post.adaptation.w = 4.;
    }
    for (e, shown) in visibility {
        world.entity_mut(e).insert(if shown {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    if let Some(image) = world.resource_mut::<Received>().0.take() {
        if sweep.phase == 0 {
            let selected = world
                .get_resource::<super::world::MapWorld>()
                .and_then(|w| w.selected.clone());
            let visible: Vec<_> = world
                .query::<(
                    &super::visibility::VisibilityPart,
                    &super::visibility::VisibilityDecision,
                )>()
                .iter(world)
                .filter(|(_, d)| d.visible)
                .map(|(p, _)| json!([p.map, p.name]))
                .collect();
            sweep.normal_manifest = json!({"selected_collision":selected,"visible_parts":visible});
        }
        sweep.waiting = false;
        sweep.frames.push(image);
        if sweep.phase < 3 {
            sweep.phase += 1;
            configure(world, &mut sweep);
        } else {
            if let Err(e) = validate_depth(
                &sweep.frames[3],
                &sweep.views[sweep.index],
                &sweep.collision_oracle,
            ) {
                error!("sweep collision depth validation: {e:#}");
                world.write_message(AppExit::error());
                world.insert_resource(sweep);
                return;
            }
            match measure(
                &sweep.frames,
                &sweep.views[sweep.index],
                &sweep.options,
                sweep.index,
            ) {
                Ok(mut report) => {
                    report["visibility"] = std::mem::take(&mut sweep.normal_manifest);
                    report["material_blocks"] = material_audit::summary(&sweep.normal_materials);
                    report["normal_all_materials_equal"] = json!(true);
                    let score = report["score"].as_u64().unwrap();
                    sweep.reports.push(report);
                    // Retain progress so interrupted long runs still have useful measurements.
                    if sweep.index.is_multiple_of(20)
                        && let Err(e) = std::fs::write(
                            sweep.options.out.join("partial.json"),
                            serde_json::to_vec(&sweep.reports).unwrap(),
                        )
                    {
                        error!("sweep progress output: {e}");
                        world.write_message(AppExit::error());
                        world.insert_resource(sweep);
                        return;
                    }
                    let index = sweep.index;
                    let frames = std::mem::take(&mut sweep.frames);
                    if sweep.options.worst > 0
                        && (sweep.worst.len() < sweep.options.worst
                            || sweep.worst.last().is_some_and(|w| score > w.0))
                    {
                        sweep.worst.push((score, index, frames));
                        sweep
                            .worst
                            .sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                        sweep.worst.truncate(sweep.options.worst);
                    }
                    sweep.index += 1;
                    if sweep.index.is_multiple_of(20) {
                        println!(
                            "sweep progress {}/{} ({:.1} s)",
                            sweep.index,
                            sweep.views.len(),
                            sweep.started.elapsed().as_secs_f32()
                        );
                    }
                    if sweep.index == sweep.views.len() {
                        if let Err(e) = finish(&sweep) {
                            error!("sweep output: {e:#}");
                            world.write_message(AppExit::error());
                        } else {
                            world.write_message(AppExit::Success);
                        }
                        world.insert_resource(sweep);
                        return;
                    }
                    sweep.phase = 0;
                    configure(world, &mut sweep);
                }
                Err(e) => {
                    error!("sweep measurement: {e:#}");
                    world.write_message(AppExit::error());
                    world.insert_resource(sweep);
                    return;
                }
            }
        }
    }
    if !sweep.waiting {
        if sweep.countdown > 0 {
            sweep.countdown -= 1;
        } else if world.resource::<Pipelines>().status().pending == 0 {
            let frame = world.resource::<Pipelines>().status().frame;
            if frame != sweep.last_pipeline_frame {
                sweep.stable_pipelines += 1;
            }
            sweep.last_pipeline_frame = frame;
            if sweep.stable_pipelines < 3 {
                world.insert_resource(sweep);
                return;
            }
            if sweep.phase < 2 {
                let materials = material_audit::snapshot(world);
                if sweep.phase == 0 {
                    sweep.normal_materials = materials;
                } else if materials != sweep.normal_materials {
                    error!(
                        "sweep normal/all-visible material bindings or texture readiness differ"
                    );
                    world.write_message(AppExit::error());
                    world.insert_resource(sweep);
                    return;
                }
                let fog = world
                    .query_filtered::<Option<&crate::lighting::fog::GameFog>, With<SweepCamera>>()
                    .single(world)
                    .expect("one native sweep camera")
                    .cloned();
                if sweep.phase == 0 {
                    sweep.normal_fog = fog.clone();
                } else if fog != sweep.normal_fog {
                    error!("sweep normal/all-visible fog uniforms differ");
                    world.write_message(AppExit::error());
                    world.insert_resource(sweep);
                    return;
                }
                info!(
                    "sweep view {} phase {} fog={fog:?}",
                    sweep.index, sweep.phase
                );
            }
            world
                .spawn(Screenshot::image(match sweep.phase {
                    2 => sweep.depth_target.clone(),
                    3 => sweep.collision_target.clone(),
                    _ => sweep.target.clone(),
                }))
                .observe(
                    |event: On<ScreenshotCaptured>, mut received: ResMut<Received>| {
                        received.0 = Some(event.image.clone());
                    },
                );
            sweep.waiting = true;
        } else {
            sweep.stable_pipelines = 0;
        }
    }
    world.insert_resource(sweep);
}

fn depth(pixel: &[u8]) -> Option<f32> {
    let code = u16::from(pixel[0]) * 256 + u16::from(pixel[1]);
    (code > 0).then(|| 2f32.powf((f32::from(code) - 1.) / 65534. * 16.) - 1.)
}

/// Independently check the GPU byte encoding and camera projection against real collision.
/// A few rays per view catch post-processing/colour conversion and stale-camera mistakes.
fn validate_depth(
    image: &Image,
    view: &View,
    collision: &sekiro_sim::collision::CollisionWorld,
) -> Result<()> {
    let rgba = image.clone().try_into_dynamic()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        view.yaw.to_radians(),
        view.pitch.to_radians(),
        0.,
    );
    for (x, y) in [
        (width / 2, height / 2),
        (width / 2, height * 3 / 4),
        (width / 4, height * 3 / 4),
        (width * 3 / 4, height * 3 / 4),
    ] {
        let tan = 30f32.to_radians().tan();
        let ray = (rotation
            * Vec3::new(
                (2. * (x as f32 + 0.5) / width as f32 - 1.) * tan * width as f32 / height as f32,
                (1. - 2. * (y as f32 + 0.5) / height as f32) * tan,
                -1.,
            ))
        .normalize();
        let expected = collision.raycast(view.eye, ray, 100.);
        let measured = depth(&rgba.get_pixel(x, y).0);
        if let Some(expected) = expected.filter(|d| *d >= 0.2) {
            anyhow::ensure!(
                measured.is_some_and(|d| (d - expected).abs() <= 0.5 + expected * 0.05),
                "eye {:?}, yaw {}, pitch {}, pixel {x},{y}: CPU {expected:.3} m, GPU {measured:?} m",
                view.eye,
                view.yaw,
                view.pitch
            );
        } else if expected.is_none() {
            anyhow::ensure!(
                measured.is_none_or(|d| d > 100.),
                "pixel {x},{y}: CPU clear to 100 m, GPU {measured:?} m"
            );
        }
    }
    Ok(())
}
fn counts(
    normal: &[u8],
    full: &[u8],
    diagnostic: &[u8],
    collision: &[u8],
    view: &View,
    width: u32,
    height: u32,
) -> [u64; 5] {
    let mut result = [0; 5];
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        view.yaw.to_radians(),
        view.pitch.to_radians(),
        0.,
    );
    let tan = 30f32.to_radians().tan();
    for i in 0..(width * height) as usize {
        let offset = i * 4;
        let observed = depth(&diagnostic[offset..offset + 4]);
        let expected = depth(&collision[offset..offset + 4]);
        let y = i as u32 / width;
        let x = i as u32 % width;
        let ray = rotation
            * Vec3::new(
                (2. * (x as f32 + 0.5) / width as f32 - 1.) * tan * width as f32 / height as f32,
                (1. - 2. * (y as f32 + 0.5) / height as f32) * tan,
                -1.,
            );
        if ray.y < 0. && observed.is_none_or(|d| d > 1000.) {
            result[0] += 1;
        }
        if let Some(d) = expected.filter(|d| *d <= 100.)
            && observed.is_none_or(|render| render > d + 2f32.max(d * 0.1))
        {
            result[1] += 1;
        }
        if observed.is_some() && diagnostic[offset + 2] > 127 {
            result[2] += 1;
        }
        let rgb = &normal[offset..offset + 3];
        let lo = *rgb.iter().min().unwrap();
        let hi = *rgb.iter().max().unwrap();
        if observed.is_some() && hi - lo <= 8 && (70..=210).contains(&lo) {
            result[4] += 1;
        }
        if (0..3).any(|c| normal[offset + c].abs_diff(full[offset + c]) > 12) {
            result[3] += 1;
        }
    }
    result
}
fn measure(images: &[Image], view: &View, options: &Options, id: usize) -> Result<Value> {
    let rgba = images
        .iter()
        .map(|image| {
            image
                .clone()
                .try_into_dynamic()
                .map(|d| d.to_rgba8().into_raw())
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let counts = counts(
        &rgba[0],
        &rgba[1],
        &rgba[2],
        &rgba[3],
        view,
        options.width,
        options.height,
    );
    Ok(
        json!({"id":id,"position":view.eye.to_array(),"yaw":view.yaw,"pitch":view.pitch,"edge":view.edge,"void":counts[0],"depth_gap":counts[1],"untextured":counts[2],"pop_in":counts[3],"grey":counts[4],"score":counts.iter().sum::<u64>()}),
    )
}
fn finish(sweep: &Sweep) -> Result<()> {
    let mut ranked = sweep.reports.clone();
    ranked.sort_by(|a, b| {
        b["score"]
            .as_u64()
            .cmp(&a["score"].as_u64())
            .then(a["id"].as_u64().cmp(&b["id"].as_u64()))
    });
    let totals: BTreeMap<_, _> = ["void", "depth_gap", "untextured", "grey", "pop_in", "score"]
        .into_iter()
        .map(|k| {
            (
                k,
                sweep
                    .reports
                    .iter()
                    .map(|r| r[k].as_u64().unwrap())
                    .sum::<u64>(),
            )
        })
        .collect();
    let report = json!({"version":2,"raw_collision_reference":sweep.options.raw_collision,"map":sweep.options.map,"objects":sweep.options.objects,"spacing":sweep.options.spacing,"yaws":sweep.options.yaws,"edge_pitches":sweep.options.pitches,"eye_height":1.65,"width":sweep.options.width,"height":sweep.options.height,"settle_frames":sweep.options.settle,"seconds":sweep.started.elapsed().as_secs_f64(),"started_unix_seconds":sweep.started_unix_seconds,"views":sweep.reports.len(),"total_sample_views":sweep.total_views,"complete_sample":sweep.reports.len()==sweep.total_views,"totals":totals,"worst20":ranked.iter().take(20).collect::<Vec<_>>(),"points":sweep.reports});
    std::fs::write(
        sweep.options.out.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    // Remove only this tool's previous screenshot names, keeping arbitrary user files.
    for entry in std::fs::read_dir(&sweep.options.out)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some((id, label)) = name.split_once('-')
            && id.len() == 6
            && id.chars().all(|c| c.is_ascii_digit())
            && matches!(
                label,
                "normal.png" | "all.png" | "depth.png" | "collision.png"
            )
            && entry.file_type()?.is_file()
        {
            std::fs::remove_file(entry.path())?;
        }
    }
    for (_, id, images) in &sweep.worst {
        for (image, label) in images.iter().zip(["normal", "all", "depth", "collision"]) {
            image
                .clone()
                .try_into_dynamic()?
                .save(sweep.options.out.join(format!("{id:06}-{label}.png")))?;
        }
    }
    println!("worst 20: id position yaw pitch void depth_gap untextured grey pop_in score");
    for row in ranked.iter().take(20) {
        println!(
            "{} {} {} {} {} {} {} {} {} {}",
            row["id"],
            row["position"],
            row["yaw"],
            row["pitch"],
            row["void"],
            row["depth_gap"],
            row["untextured"],
            row["grey"],
            row["pop_in"],
            row["score"]
        );
    }
    println!(
        "{} views in {:.1}s; totals {}; report {}",
        sweep.reports.len(),
        sweep.started.elapsed().as_secs_f32(),
        json!(totals),
        sweep.options.out.join("report.json").display()
    );
    if let Some(baseline) = &sweep.options.gate {
        let mut failures = compare_gate(baseline, &sweep.reports)?;
        let budget = baseline["max_seconds"].as_u64().unwrap_or(120);
        if sweep.started.elapsed() > Duration::from_secs(budget) {
            failures.push(format!(
                "runtime exceeds {budget}s: {:.1}s",
                sweep.started.elapsed().as_secs_f32()
            ));
        }
        std::fs::write(
            sweep.options.out.join("gate.json"),
            serde_json::to_vec_pretty(
                &json!({"passed":failures.is_empty(),"failures":failures,"seconds":sweep.started.elapsed().as_secs_f64()}),
            )?,
        )?;
        anyhow::ensure!(
            failures.is_empty(),
            "render gate regressions: {}",
            failures.join("; ")
        );
        println!("render gate passed (per-view tolerance: 16 pixels or 1%, whichever is greater)");
    }
    Ok(())
}

fn compare_gate(baseline: &Value, reports: &[Value]) -> Result<Vec<String>> {
    let views = baseline["views"].as_array().context("baseline views")?;
    anyhow::ensure!(
        views.len() == reports.len(),
        "render gate view count changed"
    );
    let mut failures = Vec::new();
    for (i, (row, actual)) in views.iter().zip(reports).enumerate() {
        let expected = row.as_array().context("baseline view")?;
        anyhow::ensure!(
            expected.len() == 10,
            "baseline row requires five camera values and five counts"
        );
        for (j, metric) in ["void", "depth_gap", "untextured", "grey", "pop_in"]
            .iter()
            .enumerate()
        {
            let before = expected[5 + j].as_u64().context("baseline count")?;
            let after = actual[metric].as_u64().context("measured count")?;
            if after > before + 16.max(before.div_ceil(100)) {
                failures.push(format!("view {i} {metric}: {before} -> {after}"));
            }
        }
    }
    Ok(failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn survey_omits_zero_demand_and_disabled_collision_planes() {
        let mesh = sekiro_formats::hknp::CollisionMesh {
            vertices: vec![[0., 0., 0.]; 3],
            indices: vec![0, 1, 2, 0, 1, 2, 0, 1, 2],
            materials: vec![0; 3],
        };
        let layout = json!({"parts":[
            {"kind":"Collision","name":"floor","triangles":[0,1],"display_group":{"condition":1,"disp_groups":[0,0,0,0,0,0,0,0]}},
            {"kind":"Collision","name":"plane","triangles":[1,1],"display_group":{"condition":-1,"disp_groups":[0,0,0,0,0,0,0,0]}},
            {"kind":"Collision","name":"disabled","triangles":[2,1],"display_group":{"condition":2,"disp_groups":[0,0,0,0,0,0,0,0]}}
        ]});
        let actual = collision_reference(&mesh, &layout, |name| name != "disabled").unwrap();
        assert_eq!(actual.indices, [0, 1, 2]);
        assert_eq!(actual.materials, [0]);
    }
    #[test]
    fn render_gate_rejects_a_single_worse_view_even_when_totals_improve() {
        let baseline = json!({"views":[[0,0,0,0,0,100,0,0,0,0],[0,0,0,0,0,100,0,0,0,0]]});
        let rows = [
            json!({"void":117,"depth_gap":0,"untextured":0,"grey":0,"pop_in":0}),
            json!({"void":0,"depth_gap":0,"untextured":0,"grey":0,"pop_in":0}),
        ];
        assert_eq!(
            compare_gate(&baseline, &rows).unwrap(),
            ["view 0 void: 100 -> 117"]
        );
        let within = [
            json!({"void":116,"depth_gap":16,"untextured":0,"grey":0,"pop_in":0}),
            rows[1].clone(),
        ];
        assert!(compare_gate(&baseline, &within).unwrap().is_empty());
        assert!(compare_gate(&baseline, &rows[..1]).is_err());
    }
    #[test]
    fn pixels_distinguish_horizon_collision_texture_and_visibility() {
        let view = View {
            eye: Vec3::ZERO,
            yaw: 0.,
            pitch: 0.,
            edge: false,
        };
        // Four pixels, two above the horizon and two below it; no visual surface.
        let empty = [0u8; 16];
        let collision = [0, 0, 0, 255, 0, 0, 0, 255, 32, 0, 0, 255, 0, 0, 0, 255];
        let mut full = empty;
        full[0] = 13;
        assert_eq!(
            counts(&empty, &full, &empty, &collision, &view, 2, 2),
            [2, 1, 0, 1, 0]
        );
        let mut render = empty;
        render[8] = 32;
        render[11] = 255;
        render[10] = 255;
        assert_eq!(
            counts(&empty, &empty, &render, &collision, &view, 2, 2),
            [1, 0, 1, 0, 0]
        );
        let mut grey = empty;
        grey[8..11].fill(100);
        assert_eq!(
            counts(&grey, &grey, &render, &collision, &view, 2, 2),
            [1, 0, 1, 0, 1]
        );
        let down = View {
            pitch: -50.,
            ..view
        };
        assert_eq!(counts(&empty, &empty, &empty, &empty, &down, 2, 2)[0], 4);
    }
    #[test]
    fn samples_grid_interiors_and_retains_stacked_floors() {
        let mesh = sekiro_formats::hknp::CollisionMesh {
            vertices: vec![
                [0., 0., 0.],
                [0., 0., 4.],
                [4., 0., 0.],
                [0., 4., 0.],
                [0., 4., 4.],
                [4., 4., 0.],
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            materials: vec![0, 0],
        };
        let options = Options::parse(&[
            "--map".into(),
            "m11_01_00_00".into(),
            "--spacing".into(),
            "2".into(),
            "--views".into(),
            "1".into(),
        ])
        .unwrap();
        let views = sample(&mesh, &options);
        assert!(views.iter().any(|v| v.eye == Vec3::new(1., 1.65, 1.)));
        assert!(views.iter().any(|v| v.eye == Vec3::new(1., 5.65, 1.)));
        assert!(views.iter().any(|v| v.pitch < 0.));
    }
}
