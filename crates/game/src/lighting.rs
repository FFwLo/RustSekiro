//! Map lighting from the game's own draw params (`param/drawparam/<area>_<block>_0000.gparam`).
//!
//! A light set gives, per light set id (0 outdoors; 100, 110, ... for interiors, chosen by the
//! hit collision under the player through its MSB gparam config) and per time of day:
//! a sun (`Directional Light Angle0`, `DiffColor0`), a second directional fill light
//! (`Angle1`, `DiffColor1`), hemispherical ambient colours (`Hemi Color Up/Down`) and, in the
//! volumetric fog group, the sky colour, a fixed fog density and the colour of light scattered
//! toward the sun (`DirectColor`). Colours are RGB with the intensity in W.
//!
//! Verified colour uploads use squared RGB times W, with one Bevy unit calibration.
//! Map materials retain the directional hemisphere term;
//! fog uses the recovered global shader profile in an HDR post pass. Angles are (pitch, yaw) in radians in the
//! game's space: the vector `Ry(yaw) * Rx(pitch) * +Z` points toward the light, and is mirrored
//! on X into Bevy space like everything else.

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::Context as ContextExt;
use bevy::light::{EnvironmentMapLight, GlobalAmbientLight, Skybox};
#[path = "fog.rs"]
pub(crate) mod fog;
use bevy::prelude::*;
use sekiro_formats::gparam::Gparam;

#[path = "grading.rs"]
pub(crate) mod grading;
use bevy::camera::Hdr;
use bevy::post_process::{auto_exposure::AutoExposure, bloom::Bloom};
use bevy::render::view::ColorGrading;
use bevy::render::view::{DebandDither, Tonemapping};

#[path = "tonemap.rs"]
pub(crate) mod tonemap;

#[derive(Resource, Default)]
struct MapPostEffects {
    luts: std::collections::BTreeMap<u32, Handle<Image>>,
    no_grading: bool,
    no_bloom: bool,
    no_auto: bool,
    tone_override: Option<u32>,
}

struct PostEffectsPlugin;
impl Plugin for PostEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            grading::GradingPlugin,
            fog::FogPlugin,
            tonemap::NativePostPlugin,
        ));
    }
}

/// Load authored strips as unorm data, even though PNG loaders normally assume sRGB.
pub fn install_post_effects(app: &mut App, dir: &std::path::Path) -> anyhow::Result<()> {
    install_probe_residency(app);
    let args: Vec<_> = std::env::args().collect();
    let mut effects = MapPostEffects {
        no_grading: args.iter().any(|a| a == "--no-grading"),
        no_bloom: args.iter().any(|a| a == "--no-bloom"),
        no_auto: args.iter().any(|a| a == "--no-auto-exposure"),
        tone_override: if args.iter().any(|a| a == "--native-tone-reinhard") {
            Some(3)
        } else if args.iter().any(|a| a == "--native-tone-filmic") {
            Some(0)
        } else {
            None
        },
        ..default()
    };
    if let Ok(entries) = std::fs::read_dir(dir.join("grading")) {
        for entry in entries {
            let path = entry?.path();
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_prefix("lut"))
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if path.extension().is_none_or(|s| s != "png") {
                continue;
            }
            let image = Image::from_buffer(
                &std::fs::read(&path)?,
                bevy::image::ImageType::Extension("png"),
                bevy::image::CompressedImageFormats::NONE,
                false,
                bevy::image::ImageSampler::default(),
                bevy::asset::RenderAssetUsages::default(),
            )?;
            anyhow::ensure!(
                image.width() == 16 && image.height() == 256,
                "invalid LUT strip"
            );
            effects.luts.insert(
                id,
                app.world_mut().resource_mut::<Assets<Image>>().add(image),
            );
        }
    }
    app.insert_resource(effects)
        .add_plugins(PostEffectsPlugin)
        .add_systems(Update, apply_post_effects.after(apply_lighting));
    Ok(())
}

#[derive(Debug, Clone)]
struct PostSettings {
    exposure: f32,
    middle: f32,
    min: f32,
    max: f32,
    auto: bool,
    lut: Option<u32>,
    alpha: f32,
    threshold: f32,
    radius: f32,
    glare: f32,
    saturation: f32,
    center: f32,
    freeze: bool,
    mode: u32,
    light_adapt: f32,
    dark_adapt: f32,
    filmic: f32,
    reinhard: f32,
    white_balance: f32,
    bloom_normalizer: f32,
    bloom_levels: u32,
    bloom_smooth: f32,
    correction: Mat4,
}

impl PostSettings {
    fn from_gparam(g: &Gparam, id: i32, hour: f32) -> Self {
        let get = |group, name, fallback| {
            g.param(group, name)
                .and_then(|p| p.at(id, hour))
                .and_then(|v| v.first().copied())
                .filter(|v| v.is_finite())
                .unwrap_or(fallback)
        };
        let lut = (get("ColorGrading", "Enable", 0.) > 0.5).then(|| {
            g.param("ColorGrading", "LutSourceId")
                .and_then(|p| p.nearest(id, hour))
                .and_then(|v| v.first().copied())
                .unwrap_or(0.)
                .max(0.) as u32
        });
        let min = get("Tone Map", "Adaption Min", -8.);
        let max = get("Tone Map", "Adaption Max", 8.);
        let mode = g
            .param("Tone Map", "ToneMapMode")
            .or_else(|| g.param("Tone Map", "ToneMap-Mode"))
            .and_then(|p| p.nearest(id, hour))
            .and_then(|v| v.first().copied())
            .unwrap_or(7.) as u32;
        let mode = match mode {
            0 | 3 | 7 | 8 => mode,
            _ => {
                warn!("unimplemented native tone mode {mode}; using sensitometric 7");
                7
            }
        };
        Self {
            exposure: get("Tone Map", "ToneMap-Exposure", 0.),
            middle: get("Tone Map", "Middle Gray", -2.),
            min,
            max,
            auto: get("Tone Map", "Enable AutoExposure", 0.) > 0.5,
            lut,
            alpha: if get("Bloom", "Enable", 1.) > 0.5 {
                get("Bloom", "GlareBloom Alpha", 0.).max(0.)
            } else {
                0.
            },
            threshold: get("Bloom", "Glare Threshold", 0.).max(0.),
            radius: get("Bloom", "GaussianRadiusScale", 1.).max(0.),
            glare: get("Bloom", "Glare Luminance", 1.).max(0.),
            saturation: get("Bloom", "Sturate Ctrl", 0.).clamp(0., 1.),
            center: get("Tone Map", "Center Weight", 4.).max(0.),
            freeze: get("Tone Map", "FreezeAdaptionFade", 0.) > 0.5,
            mode,
            light_adapt: get("Tone Map", "LightAdapt", 1.).max(0.),
            dark_adapt: get("Tone Map", "DarkAdapt", 0.5).max(0.),
            filmic: get(
                "Tone Map",
                "ToneMap-Contrast",
                get("Tone Map", "FilmicContrast", 0.5),
            ),
            reinhard: get("Tone Map", "ReinhardContrast", 1.6),
            white_balance: get("Tone Map", "ReinhardWhiteBalance", 1.7),
            bloom_levels: (get("Bloom", "GlareNumLevels", 7.) as u32).clamp(1, 8),
            bloom_smooth: get("Bloom", "GlareBloom Smooth", 0.8),
            correction: tone_correction(
                get("Tone Map", "ColorH", 0.),
                get("Tone Map", "ColorS", 1.),
                get("Tone Map", "ColorV", 1.),
                get("Tone Map", "GS Presents - Contrast", 1.),
            ),
            bloom_normalizer: (0..(get("Bloom", "GlareNumLevels", 7.) as i32).clamp(1, 8))
                .map(|i| get("Bloom", "GlareBloom Smooth", 0.8).powi(i))
                .sum(),
        }
    }
    fn upload(&self, effects: &MapPostEffects) -> tonemap::GamePost {
        use tonemap::*;
        GamePost {
            exposure: Vec4::new(
                self.exposure.exp2(),
                1.,
                self.middle,
                f32::from(self.auto && !effects.no_auto),
            ),
            adaptation: Vec4::new(self.min, self.max, 0., 0.),
            bloom: Vec4::new(
                self.threshold,
                if effects.no_bloom {
                    0.
                } else {
                    self.alpha * self.glare * BLOOM_QUALITY_SCALE / self.bloom_normalizer
                },
                self.radius,
                1.,
            ),
            transfer: Vec4::new(
                1.,
                1. / OUTPUT_GAMMA,
                effects.tone_override.unwrap_or(self.mode) as f32,
                self.center,
            ),
            curve: Vec4::new(
                self.light_adapt,
                self.dark_adapt,
                self.filmic,
                self.reinhard,
            ),
            reinhard: Vec4::new(
                self.white_balance,
                self.bloom_levels as f32,
                self.bloom_smooth,
                0.,
            ),
            colour: self.correction,
            controls: Vec4::new(
                self.saturation,
                f32::from(self.freeze),
                f32::from(
                    !effects.no_grading
                        && self.lut.is_some_and(|id| effects.luts.contains_key(&id)),
                ),
                0.,
            ),
        }
    }
}

/// Native 24dca0: hue in a luminance-preserving basis, then saturation,
/// value and contrast about 0.5, all before the ordinary output exponent.
fn tone_correction(hue: f32, saturation: f32, value: f32, contrast: f32) -> Mat4 {
    let weights = Vec3::new(0.29891, 0.58661, 0.11448);
    let complement = Vec3::ONE - weights;
    let basis = Mat3::from_cols(
        Vec3::X - complement * weights.x,
        Vec3::Y - complement * weights.y,
        Vec3::Z - complement * weights.z,
    );
    let hue_matrix = if hue == 0. {
        Mat3::IDENTITY
    } else {
        basis.inverse()
            * Mat3::from_quat(Quat::from_axis_angle(weights.normalize(), hue.to_radians()))
            * basis
    };
    let saturation_matrix = Mat3::from_cols(
        Vec3::splat((1. - saturation) * weights.x) + Vec3::X * saturation,
        Vec3::splat((1. - saturation) * weights.y) + Vec3::Y * saturation,
        Vec3::splat((1. - saturation) * weights.z) + Vec3::Z * saturation,
    );
    let mut matrix = Mat4::from_mat3(saturation_matrix * hue_matrix * (value * contrast));
    matrix.w_axis = Vec3::splat((1. - contrast) * 0.5).extend(1.);
    matrix
}

#[derive(Component)]
struct AppliedPost(i32, f32, String);

#[allow(clippy::type_complexity)]
fn apply_post_effects(
    mut commands: Commands,
    lighting: Res<MapLighting>,
    effects: Res<MapPostEffects>,
    cameras: Query<(Entity, Option<&AppliedPost>, Option<&grading::GradingLut>), With<Camera3d>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("apply_post_effects");
    let pending: Vec<_> = cameras
        .iter()
        .filter(|(_, p, _)| {
            p.is_none_or(|p| {
                p.0 != lighting.light_set || p.1 != lighting.hour || p.2 != lighting.source
            })
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    let set = PostSettings::from_gparam(&lighting.gparam, lighting.light_set, lighting.hour);
    info!(
        "map native post: {set:?}, upload={:?}; CPU mappings from SHADER-INPUTS.md",
        set.upload(&effects)
    );
    for (entity, applied, current_lut) in pending {
        let mut camera = commands.entity(entity);
        // The view setup is per camera, not per row: re-inserting Hdr and Tonemapping at every
        // LightSet change (doorways, #15) made Bevy re-specialise the camera's view pipelines.
        if applied.is_none() {
            camera.insert((Hdr, Tonemapping::None, DebandDither::Disabled));
            camera.remove::<(AutoExposure, Bloom)>();
        }
        camera.insert((
            AppliedPost(lighting.light_set, lighting.hour, lighting.source.clone()),
            ColorGrading::default(),
            set.upload(&effects),
        ));
        let lut = (!effects.no_grading)
            .then(|| set.lut.and_then(|id| effects.luts.get(&id)))
            .flatten();
        match lut {
            Some(lut) if current_lut.is_none_or(|c| c.0 != *lut) => {
                camera.insert(grading::GradingLut(lut.clone()));
            }
            None if current_lut.is_some() => {
                camera.remove::<grading::GradingLut>();
            }
            _ => {}
        }
    }
}

/// The LightSet row map pieces and objects authoring MSB light set 0 are drawn with.
/// Native lights each instance with its own light set; collision rows (100, 110, ...) light
/// the characters. See docs/LIGHTING.md, "Per-part light sets" (#30).
pub const MAP_PART_LIGHT_SET: i32 = 0;

/// Whether a map part with this MSB light set id follows the collision-selected row.
pub fn part_follows_scene(msb_light_set: i32) -> bool {
    msb_light_set > MAP_PART_LIGHT_SET
}

/// The LightSet row a map surface is drawn with: row 0 for the ordinary MSB light set 0, or
/// the collision-selected `scene` row for characters and for the few parts that author another
/// set. `SEKIRO_ABLATE=scenelight` and `SEKIRO_LIGHT_SET` put every surface on the scene row.
pub fn part_light_set(scene: i32, follows_scene: bool) -> i32 {
    if follows_scene || ablate("scenelight") || forced_light_set().is_some() {
        scene
    } else {
        MAP_PART_LIGHT_SET
    }
}

/// Diagnostic ablation switch:
/// `SEKIRO_ABLATE=nosun,nofill,nolocal,nospec,noamb,nogi,nolayers,scenelight` (comma list).
pub fn ablate(name: &str) -> bool {
    static LIST: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        std::env::var("SEKIRO_ABLATE")
            .unwrap_or_default()
            .split(',')
            .map(str::to_owned)
            .collect()
    })
    .iter()
    .any(|n| n == name)
}

/// Diagnostic multiplier from `SEKIRO_SCALE_<NAME>` (default 1).
pub fn scale(name: &str) -> f32 {
    std::env::var(format!("SEKIRO_SCALE_{name}"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.)
}

/// Diagnostic `SEKIRO_LIGHT_SET=<id>` forces one LightSet row for every collision (unset: authored).
pub fn forced_light_set() -> Option<i32> {
    static FORCED: std::sync::OnceLock<Option<i32>> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| std::env::var("SEKIRO_LIGHT_SET").ok()?.parse().ok())
}

/// Bevy lux per native scene unit, traced from the native accumulation arithmetic.
///
/// The deferred accumulators store `Col * N.L` with no `1/pi` and no photometric unit
/// (`GXLightAcc_DDirLight1`), and `GXLightCmbAlbedo` multiplies that by the G-buffer albedo, so a
/// white Lambert surface under `Col * N.L = 1` has native radiance 1. Bevy's diffuse term is
/// `albedo / pi * lux * exposure`, so lux = pi / exposure gives native radiance 1 under Bevy's
/// default camera (`Exposure::default()`, EV100 9.7, exposure `1 / (2^9.7 * 1.2)`).
/// The same relation holds for the sky dome and fog colours, whose native radiance is unity.
/// This replaces the earlier fitted 4500, which was 1.435 times too large; the independent check
/// is the sky at spot-091, whose retail/ours ratio was 1.25 to 1.43 at 4500.
pub const SCENE_TO_BEVY: f32 = 3_135.61;
/// BTL candela mapping shares the bridge; BTL native units remain open.
pub const LOCAL_LUMENS_PER_UNIT: f32 = SCENE_TO_BEVY * 4.0 * std::f32::consts::PI;

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ShadowSettings {
    pub enabled: bool,
    pub cascades: usize,
    pub distance: f32,
    /// `--shadow` set `distance`, so light-file changes keep it (#65).
    pub distance_from_args: bool,
    /// `--shadow-casters` metres: map meshes farther from the focus stop casting. None leaves
    /// caster culling to Bevy's cascade frusta, so nothing gains its shadow on approach (#66).
    pub caster_distance: Option<f32>,
    /// Native cascade cut points (`ShadowParam` `Cascade Dist 0->1/1->2/2->3`) in metres.
    pub splits: [f32; 3],
    pub depth_bias: f32,
    pub normal_bias: f32,
}

/// `default.gparam` `ShadowParam`: cascade cut points 10/30/70 m and far fade start 1000 m.
const NATIVE_SPLITS: [f32; 3] = [10., 30., 70.];
const NATIVE_FAR_FADE_START: f32 = 1000.;

impl ShadowSettings {
    pub fn from_args(g: &Gparam, hour: f32, args: &[String]) -> anyhow::Result<Self> {
        let mut settings = Self {
            enabled: !args.iter().any(|a| a == "--no-shadow"),
            cascades: 4,
            distance: NATIVE_FAR_FADE_START,
            distance_from_args: false,
            caster_distance: None,
            depth_bias: 0.02,
            normal_bias: 0.6,
            splits: NATIVE_SPLITS,
        };
        settings.read_shadow_param(g, hour);
        for (i, arg) in args.iter().enumerate() {
            if arg == "--shadow" {
                let value = args
                    .get(i + 1)
                    .context("--shadow needs cascades,distance")?;
                let (count, distance) = value
                    .split_once(',')
                    .context("--shadow needs cascades,distance")?;
                settings.cascades = count.parse()?;
                settings.distance = distance.parse()?;
                settings.distance_from_args = true;
            }
            if arg == "--shadow-casters" {
                settings.caster_distance = Some(
                    args.get(i + 1)
                        .context("--shadow-casters needs metres")?
                        .parse()?,
                );
            }
            if arg == "--shadow-bias" {
                let value = args
                    .get(i + 1)
                    .context("--shadow-bias needs depth,normal")?;
                let (depth, normal) = value
                    .split_once(',')
                    .context("--shadow-bias needs depth,normal")?;
                settings.depth_bias = depth.parse()?;
                settings.normal_bias = normal.parse()?;
            }
        }
        anyhow::ensure!(
            (1..=4).contains(&settings.cascades),
            "shadow cascades must be 1..4"
        );
        anyhow::ensure!(
            settings.distance.is_finite() && settings.distance >= 1.,
            "shadow distance must be finite and >= 1 m"
        );
        anyhow::ensure!(
            settings
                .caster_distance
                .is_none_or(|d| d.is_finite() && d > 0.),
            "caster distance must be finite and positive"
        );
        anyhow::ensure!(
            settings.depth_bias.is_finite()
                && settings.depth_bias >= 0.
                && settings.normal_bias.is_finite()
                && settings.normal_bias >= 0.,
            "shadow bias must be finite and nonnegative"
        );
        Ok(settings)
    }

    /// Reads the cut points and far fade from the light file's `ShadowParam` at `hour`, keeping
    /// a `--shadow` distance. Runs at start and on every light-file or hour change (#65).
    pub fn read_shadow_param(&mut self, g: &Gparam, hour: f32) {
        let shadow_param = |name: &str, fallback: f32| {
            g.param("ShadowParam", name)
                .and_then(|p| p.at(0, hour))
                .and_then(|v| v.first().copied())
                .filter(|v| v.is_finite() && *v > 0.)
                .unwrap_or(fallback)
        };
        // Receivers keep the sun shadow lookup to the native far fade; see LIGHTING.md (#9).
        if !self.distance_from_args {
            self.distance = shadow_param("Far Fade Start", NATIVE_FAR_FADE_START);
        }
        self.splits = [
            shadow_param("Cascade Dist 0->1", NATIVE_SPLITS[0]),
            shadow_param("Cascade Dist 1->2", NATIVE_SPLITS[1]),
            shadow_param("Cascade Dist 2->3", NATIVE_SPLITS[2]),
        ];
    }

    /// Cascade far bounds in view depth: the native cut points below the shadow distance,
    /// at most `cascades - 1` of them, then the shadow distance itself.
    pub fn cascade_bounds(&self) -> Vec<f32> {
        let mut bounds: Vec<f32> = Vec::with_capacity(self.cascades);
        for split in self.splits {
            if bounds.len() + 1 >= self.cascades {
                break;
            }
            if split < self.distance && bounds.last().is_none_or(|&last| split > last) {
                bounds.push(split);
            }
        }
        bounds.push(self.distance);
        bounds
    }
}
/// Irradiance-to-radiance bridge shared by hemisphere and environment lighting.
pub const ENV_PER_UNIT: f32 = SCENE_TO_BEVY / std::f32::consts::PI;

/// Native RGBA8 coefficient voxels and spatial records, shared by every map/actor material.
#[derive(Resource)]
pub struct MapIrradiance {
    pub buffer: Handle<bevy::render::storage::ShaderBuffer>,
    pub specular: Handle<Image>,
    pub diffuse: Handle<Image>,
    pub voxels: Handle<Image>,
}

/// Pack four coefficient planes side-by-side in one hardware-filtered 3D atlas.
fn irradiance_atlas(records: &mut [[u32; 52]], source: &[u32]) -> anyhow::Result<Image> {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let width = records
        .iter()
        .map(|r| r[16])
        .max()
        .unwrap_or(1)
        .max(128)
        .next_power_of_two();
    let height = records
        .iter()
        .map(|r| r[17])
        .max()
        .unwrap_or(1)
        .max(128)
        .next_power_of_two();
    let (mut x, mut y, mut z, mut row_height, mut layer_depth) = (0, 0, 0, 0, 0);
    let mut locations = vec![[0; 3]; records.len()];
    // Deepest volumes first prevents a shallow box from wasting a deep shelf.
    let mut order: Vec<_> = (0..records.len()).collect();
    order.sort_by_key(|i| std::cmp::Reverse(records[*i][18]));
    for i in order {
        let r = &records[i];
        if x + r[16] > width {
            x = 0;
            y += row_height;
            row_height = 0;
        }
        if y + r[17] > height {
            x = 0;
            y = 0;
            z += layer_depth;
            layer_depth = 0;
            row_height = 0;
        }
        locations[i] = [x, y, z];
        x += r[16];
        row_height = row_height.max(r[17]);
        layer_depth = layer_depth.max(r[18]);
    }
    let depth = (z + layer_depth).max(1);
    anyhow::ensure!(
        width * 4 <= 2048 && height <= 2048 && depth <= 2048,
        "GIIV atlas exceeds portable 3D texture limits"
    );
    let mut data = vec![0u8; (width * 4) as usize * height as usize * depth as usize * 4];
    for (r, location) in records.iter_mut().zip(locations) {
        let [ox, oy, oz] = location;
        for axis in 0..4 {
            let dims = &r[40 + axis * 3..43 + axis * 3];
            for slice in 0..dims[2] {
                for row in 0..dims[1] {
                    let src = (r[20 + axis] + (slice * dims[1] + row) * dims[0]) as usize;
                    let dst = (((oz + slice) * height + oy + row) * (width * 4)
                        + ox
                        + axis as u32 * width) as usize
                        * 4;
                    for (i, pixel) in source[src..src + dims[0] as usize].iter().enumerate() {
                        data[dst + i * 4..dst + i * 4 + 4].copy_from_slice(&pixel.to_le_bytes());
                    }
                }
            }
        }
        r[20..24].copy_from_slice(&[ox, oy, oz, width]);
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: width * 4,
            height,
            depth_or_array_layers: depth,
        },
        TextureDimension::D3,
        TextureFormat::Rgba8Unorm,
        default(),
    );
    image.data = Some(data);
    image.sampler = bevy::image::ImageSampler::linear();
    info!(
        "GIIV atlas: {}x{height}x{depth}, {:.1} MiB",
        width * 4,
        image.data.as_ref().unwrap().len() as f32 / (1024. * 1024.)
    );
    Ok(image)
}

fn probe_array(dir: &std::path::Path, probes: &MapProbes, diffuse: bool) -> anyhow::Result<Image> {
    use anyhow::ensure;
    use bevy::render::render_resource::{
        Extent3d, TextureDataOrder, TextureDimension, TextureFormat, TextureViewDescriptor,
        TextureViewDimension,
    };
    let size: u32 = if diffuse { 16 } else { 128 };
    let levels = if diffuse { 1 } else { 8 };
    let suffix = if diffuse { "_diffuse" } else { "" };
    let count = probes.available.len().max(1) as u32;
    let mut data = Vec::new();
    let face_bytes: usize = (0..levels).map(|m| (size >> m).pow(2) as usize * 8).sum();
    for probe in &probes.available {
        let bytes =
            std::fs::read(dir.join(format!("env/gilm{probe}_{:02}{suffix}.dds", probes.variant)))?;
        ensure!(
            bytes.len() == 128 + face_bytes * 6 && bytes.starts_with(b"DDS "),
            "invalid exported GILM cube"
        );
        data.extend_from_slice(&bytes[128..]);
    }
    if data.is_empty() {
        data.resize(face_bytes * 6, 0);
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: count * 6,
        },
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        default(),
    );
    image.texture_descriptor.mip_level_count = levels;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::CubeArray),
        ..default()
    });
    image.data_order = TextureDataOrder::LayerMajor;
    image.data = Some(data);
    image.sampler = bevy::image::ImageSampler::linear();
    Ok(image)
}

/// Diagnostic control keeps the current renderer available without changing source or fixtures.
struct GiOptions {
    legacy: bool,
    hemisphere: bool,
    local_fallback: bool,
    native_lights: bool,
}

/// Whether map materials add the native four-slot direct lighting (`direct[12].x`).
pub(crate) fn native_lights() -> bool {
    gi_options().native_lights
}

fn gi_options() -> &'static GiOptions {
    static OPTIONS: std::sync::OnceLock<GiOptions> = std::sync::OnceLock::new();
    OPTIONS.get_or_init(|| {
        let legacy = std::env::var_os("SEKIRO_GI_LEGACY").is_some_and(|v| v == "1");
        let enabled = |name| !legacy && std::env::var_os(name).is_none_or(|v| v != "0");
        GiOptions {
            legacy,
            hemisphere: enabled("SEKIRO_GI_HEMISPHERE"),
            local_fallback: enabled("SEKIRO_GI_LOCAL_FALLBACK"),
            native_lights: !legacy
                && !legacy_map_lights()
                && std::env::var_os("SEKIRO_GI_NATIVE_LIGHTS").is_none_or(|v| v != "0"),
        }
    })
}

pub(crate) fn legacy_gi() -> bool {
    gi_options().legacy
}

/// Matched-camera control of this job only, preserving the preceding GI/fog fixes.
pub(crate) fn legacy_map_lights() -> bool {
    std::env::var_os("SEKIRO_MAP_LIGHTS_LEGACY").is_some_and(|v| v == "1")
}

/// Conservative cell lists preserve the original large-to-small blending order.
fn irradiance_grid(data: &mut Vec<u32>, records: &[[u32; 52]]) {
    if records.is_empty() {
        return;
    }
    let bounds: Vec<_> = records
        .iter()
        .map(|r| {
            (
                Vec3::from_array(std::array::from_fn(|i| f32::from_bits(r[24 + i]))),
                Vec3::from_array(std::array::from_fn(|i| f32::from_bits(r[28 + i]))),
            )
        })
        .collect();
    let global: Vec<_> = bounds
        .iter()
        .map(|(lo, hi)| (*hi - *lo).max_element() > 512.)
        .collect();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for ((lo, hi), global) in bounds.iter().zip(&global) {
        if !global {
            min = min.min(*lo);
            max = max.max(*hi);
        }
    }
    let mut size = 32.;
    let (origin, dims) = if min.is_finite() {
        loop {
            let origin = (min / size).floor() * size;
            let dims = ((max - origin) / size).ceil().as_uvec3() + UVec3::ONE;
            if dims.max_element() <= 128 && dims.as_u64vec3().element_product() <= 32768 {
                break (origin, dims);
            }
            size *= 2.;
        }
    } else {
        (Vec3::ZERO, UVec3::ONE)
    };
    let mut lists = vec![Vec::<u32>::new(); dims.element_product() as usize];
    let mut outside = Vec::new();
    for (i, ((lo, hi), global)) in bounds.iter().zip(&global).enumerate() {
        if *global {
            outside.push(i as u32);
            for list in &mut lists {
                list.push(i as u32);
            }
        } else {
            // A cell of padding conservatively covers CPU/GPU boundary rounding.
            let lo = (((*lo - origin) / size).floor() - Vec3::ONE)
                .max(Vec3::ZERO)
                .as_uvec3();
            let hi = (((*hi - origin) / size).floor() + Vec3::ONE)
                .max(Vec3::ZERO)
                .as_uvec3()
                .min(dims - UVec3::ONE);
            for z in lo.z..=hi.z {
                for y in lo.y..=hi.y {
                    for x in lo.x..=hi.x {
                        lists[(x + dims.x * (y + dims.y * z)) as usize].push(i as u32);
                    }
                }
            }
        }
    }
    let grid = data.len();
    data[2] = grid as u32;
    data.extend(origin.to_array().map(f32::to_bits));
    data.push(0);
    data.extend([size.recip().to_bits(); 4]);
    data.extend(dims.to_array());
    data.push(0);
    data.extend([0; 4]);
    let table = data.len();
    data.resize(table + lists.len() * 2, 0);
    data[grid + 12] = data.len() as u32;
    data[grid + 13] = outside.len() as u32;
    data.extend(&outside);
    for (cell, list) in lists.iter().enumerate() {
        data[table + cell * 2] = data.len() as u32;
        data[table + cell * 2 + 1] = list.len() as u32;
        data.extend(list);
    }
    info!(
        "GIIV grid: {dims:?}, cell {size} m, candidates mean {:.2}, max {}, outside {}, {} KiB",
        lists.iter().map(Vec::len).sum::<usize>() as f64 / lists.len() as f64,
        lists.iter().map(Vec::len).max().unwrap_or(0),
        outside.len(),
        (data.len() - grid) * 4 / 1024
    );
}

pub fn install_irradiance(
    app: &mut App,
    dir: &std::path::Path,
    probes: &MapProbes,
) -> anyhow::Result<()> {
    // Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
    use anyhow::{Context as ContextExt, ensure};
    let mut regions: Vec<_> = probes.regions.iter().filter(|r| r.size.is_some()).collect();
    regions.sort_by(|a, b| {
        b.size
            .unwrap()
            .element_product()
            .total_cmp(&a.size.unwrap().element_product())
    });
    let enabled = !std::env::args().any(|a| a == "--no-giiv");
    let indices: std::collections::BTreeMap<_, _> = probes
        .available
        .iter()
        .enumerate()
        .map(|(i, p)| (*p, i as u32))
        .collect();
    let mut records = Vec::<[u32; 52]>::new();
    let mut voxels = Vec::new();
    {
        for region in regions {
            let Some(cube) = indices.get(&region.probe) else {
                continue;
            };
            let mut dimensions = [0u32; 3];
            let mut axis_dimensions = [[0u32; 3]; 4];
            let mut offsets = [0; 4];
            let mut coefficients = Vec::new();
            for (index, (axis, offset)) in ['x', 'y', 'z', 'w']
                .into_iter()
                .zip(&mut offsets)
                .enumerate()
            {
                let path = dir.join(format!(
                    "env/giiv{}_{:02}_{axis}.rgba8",
                    region.probe, probes.variant
                ));
                if !path.exists() {
                    break;
                }
                let bytes =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
                ensure!(bytes.len() >= 12, "truncated GIIV header");
                let dims: [u32; 3] = std::array::from_fn(|i| {
                    u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
                });
                ensure!(
                    dims.iter().all(|d| *d > 0 && *d <= 2048),
                    "invalid GIIV dimensions"
                );
                ensure!(
                    bytes.len() == 12 + dims.iter().map(|d| *d as usize).product::<usize>() * 4,
                    "truncated GIIV voxels"
                );
                axis_dimensions[index] = dims;
                dimensions = std::array::from_fn(|i| dimensions[i].max(dims[i]));
                *offset = coefficients.len() as u32;
                coefficients.extend(
                    bytes[12..]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| u32::from_le_bytes(*b)),
                );
            }
            if axis_dimensions.iter().any(|dims| dims.contains(&0)) {
                continue;
            }
            let size = region.size.unwrap();
            ensure!(
                size.is_finite() && size.min_element() > 0.,
                "invalid GIIV extent"
            );
            // UV is native left-handed box space, Y bottom-centred like MSB.
            let world =
                Mat4::from_scale_rotation_translation(size, region.rotation, region.position);
            let uv = Mat4::from_translation(Vec3::new(0.5, 0., 0.5))
                * Mat4::from_scale(Vec3::new(-1., 1., 1.))
                * world.inverse();
            let mut record = [0; 52];
            record[..16].copy_from_slice(&uv.to_cols_array().map(f32::to_bits));
            record[16..19].copy_from_slice(&dimensions);
            for (axis, dims) in axis_dimensions.iter().enumerate() {
                record[40 + axis * 3..43 + axis * 3].copy_from_slice(dims);
            }
            record[19] = *cube;
            record[20..24].copy_from_slice(&offsets.map(|o| o + voxels.len() as u32));
            let center = region.position + region.rotation * Vec3::Y * size.y * 0.5;
            let extent = Mat3::from_quat(region.rotation).abs() * size * 0.5;
            record[24..27].copy_from_slice(&(center - extent).to_array().map(f32::to_bits));
            record[28..31].copy_from_slice(&(center + extent).to_array().map(f32::to_bits));
            record[32..35].copy_from_slice(&size.to_array().map(f32::to_bits));
            let origin = probes
                .regions
                .iter()
                .find(|r| r.probe == region.probe && r.size.is_none())
                .map_or(center, |r| r.position);
            record[36..39].copy_from_slice(&origin.to_array().map(f32::to_bits));
            records.push(record);
            voxels.extend(coefficients);
        }
    }
    let count = records.len();
    let atlas = irradiance_atlas(&mut records, &voxels)?;
    let mut data = vec![count as u32, u32::from(enabled), 0, 0];
    for record in &records {
        data.extend(record);
    }
    irradiance_grid(&mut data, &records);
    if legacy_gi() || std::env::var_os("SEKIRO_GI_FULL_SCAN").is_some_and(|v| v == "1") {
        data[2] = 0;
    }
    let buffer = app
        .world_mut()
        .resource_mut::<Assets<bevy::render::storage::ShaderBuffer>>()
        .add(bevy::render::storage::ShaderBuffer::from(data));
    let specular = probe_array(dir, probes, false)?;
    let diffuse = probe_array(dir, probes, true)?;
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    let specular = images.add(specular);
    let diffuse = images.add(diffuse);
    let voxels = images.add(atlas);
    info!(
        "GIIV: {count} complete native coefficient volumes, enabled {enabled}, variant {}",
        probes.variant
    );
    app.insert_resource(MapIrradiance {
        buffer,
        specular,
        diffuse,
        voxels,
    });
    Ok(())
}

#[derive(Debug)]
pub struct ProbeRegion {
    pub probe: u32,
    pub position: Vec3,
    pub rotation: Quat,
    pub size: Option<Vec3>,
}

impl ProbeRegion {
    fn contains(&self, point: Vec3) -> bool {
        self.size.is_some_and(|size| {
            let local = self.rotation.inverse() * (point - self.position);
            local.x.abs() <= size.x * 0.5
                && local.z.abs() <= size.z * 0.5
                && local.y >= 0.
                && local.y <= size.y
        })
    }
}

pub fn nearest_variant(hour: f32) -> u32 {
    nearest_available_variant(hour, 0..6).unwrap_or(2)
}

pub fn nearest_available_variant(hour: f32, variants: impl Iterator<Item = u32>) -> Option<u32> {
    const HOURS: [f32; 6] = [0., 6., 12., 18., 22., 2.];
    let distance = |h: f32| {
        let d = (hour.rem_euclid(24.) - h).abs();
        d.min(24. - d)
    };
    variants.min_by(|a, b| distance(HOURS[*a as usize]).total_cmp(&distance(HOURS[*b as usize])))
}

#[derive(Resource, Default)]
pub struct MapProbes {
    pub regions: Vec<ProbeRegion>,
    /// Only complete extracted diffuse/specular pairs are candidates.
    pub available: std::collections::BTreeSet<u32>,
    pub prefix: String,
    pub variant: u32,
    pub active: Option<u32>,
    pub(crate) handles: std::collections::HashMap<u32, (Handle<Image>, Handle<Image>)>,
}

impl MapProbes {
    fn choose(&self, point: Vec3) -> Option<u32> {
        let candidates = self
            .regions
            .iter()
            .filter(|r| self.available.contains(&r.probe));
        candidates
            .clone()
            .filter(|r| r.contains(point))
            .min_by(|a, b| {
                a.size
                    .unwrap()
                    .element_product()
                    .total_cmp(&b.size.unwrap().element_product())
            })
            .or_else(|| {
                candidates.filter(|r| r.size.is_none()).min_by(|a, b| {
                    a.position
                        .distance_squared(point)
                        .total_cmp(&b.position.distance_squared(point))
                })
            })
            .map(|r| r.probe)
    }
}

#[derive(Resource, Clone, Default)]
pub(crate) struct ProbeResidency(
    std::sync::Arc<std::sync::RwLock<std::collections::HashSet<AssetId<Image>>>>,
);

#[derive(Resource, Default)]
struct RequestedProbes(Vec<AssetId<Image>>);

fn install_probe_residency(app: &mut App) {
    let residency = ProbeResidency::default();
    app.insert_resource(residency.clone());
    if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
        render
            .insert_resource(residency)
            .init_resource::<RequestedProbes>()
            .add_systems(bevy::render::ExtractSchedule, extract_probe_requests)
            .add_systems(
                bevy::render::Render,
                prepare_probe_residency.after(bevy::render::RenderSystems::PrepareAssets),
            );
    }
}

fn extract_probe_requests(
    probes: bevy::render::Extract<Res<MapProbes>>,
    mut requested: ResMut<RequestedProbes>,
) {
    requested.0.clear();
    for (specular, diffuse) in probes.handles.values() {
        requested.0.extend([specular.id(), diffuse.id()]);
    }
}

fn prepare_probe_residency(
    requested: Res<RequestedProbes>,
    residency: Res<ProbeResidency>,
    images: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
) {
    let mut ready = residency.0.write().unwrap();
    ready.clear();
    ready.extend(
        requested
            .0
            .iter()
            .copied()
            .filter(|id| images.get(*id).is_some()),
    );
}

fn probe_light(
    current: Option<&EnvironmentMapLight>,
    ready: Option<&(Handle<Image>, Handle<Image>)>,
) -> Option<EnvironmentMapLight> {
    if let Some((specular, diffuse)) = ready {
        current
            .is_none_or(|c| {
                c.specular_map != *specular
                    || c.diffuse_map != *diffuse
                    || c.intensity != ENV_PER_UNIT
            })
            .then(|| EnvironmentMapLight {
                specular_map: specular.clone(),
                diffuse_map: diffuse.clone(),
                intensity: ENV_PER_UNIT,
                ..default()
            })
    } else {
        // Removing the component changes the view key and re-specializes every map draw.
        // Zero intensity has the same fallback lighting while retaining resident textures.
        current
            .filter(|c| c.intensity != 0.)
            .map(|c| EnvironmentMapLight {
                intensity: 0.,
                ..c.clone()
            })
    }
}

/// Spatial reflection probes fade per receiver, so crossing boxes never cuts the whole view.
#[allow(clippy::type_complexity, clippy::too_many_arguments)] // Independent Bevy system parameters.
pub(crate) fn apply_probes(
    mut commands: Commands,
    mut probes: ResMut<MapProbes>,
    assets: Res<AssetServer>,
    mut ambient: ResMut<GlobalAmbientLight>,
    focus: Query<&Transform, With<crate::map::MapFocus>>,
    cameras: Query<(Entity, Option<&EnvironmentMapLight>, Option<&Skybox>), With<Camera3d>>,
    mut probe_sky: Local<Option<bool>>,
    selection: Res<crate::lighting_selection::RegionSelection>,
    mut last_env_bank: Local<Option<i32>>,
    residency: Option<Res<ProbeResidency>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("apply_probes");
    // The probes are captures from points inside the level, so as a sky they show nearby
    // scenery blown up behind the world; the real sky is the map's dome meshes. Lighting only,
    // unless `--probe-sky` asks to see the capture for debugging.
    let show_sky = *probe_sky.get_or_insert_with(|| std::env::args().any(|a| a == "--probe-sky"));
    // Stable enclosing fallback rather than a camera-driven region hard cut.
    if *last_env_bank != Some(selection.0[3]) {
        *last_env_bank = Some(selection.0[3]);
        if selection.0[3] > 0 {
            warn!(
                "unverified nonzero MSB environment bank {}; preserving the bank ID without inventing a GILM probe mapping",
                selection.0[3]
            );
        }
    }
    let largest = || {
        probes
            .regions
            .iter()
            .filter(|r| probes.available.contains(&r.probe) && r.size.is_some())
            .max_by(|a, b| {
                a.size
                    .unwrap()
                    .element_product()
                    .total_cmp(&b.size.unwrap().element_product())
            })
            .map(|r| r.probe)
            .or_else(|| {
                focus
                    .iter()
                    .next()
                    .and_then(|t| probes.choose(t.translation))
            })
    };
    let selected = if !gi_options().local_fallback {
        largest()
    } else {
        focus
            .iter()
            .next()
            .and_then(|t| probes.choose(t.translation))
    };
    let maps = selected.map(|id| {
        if !probes.handles.contains_key(&id) {
            let specular = assets.load(format!(
                "{}env/gilm{id}_{:02}.dds",
                probes.prefix, probes.variant
            ));
            let diffuse = assets.load(format!(
                "{}env/gilm{id}_{:02}_diffuse.dds",
                probes.prefix, probes.variant
            ));
            probes.handles.insert(id, (specular, diffuse));
        }
        probes.handles[&id].clone()
    });
    let ready = maps.filter(|(s, d)| {
        assets.is_loaded_with_dependencies(s.id())
            && assets.is_loaded_with_dependencies(d.id())
            && residency.as_ref().is_none_or(|r| {
                let ready = r.0.read().unwrap();
                ready.contains(&s.id()) && ready.contains(&d.id())
            })
    });
    let active = if ready.is_some() { selected } else { None };
    if probes.active != active {
        info!(
            "environment probe {active:?}, variant {} (intensity {ENV_PER_UNIT:.1} cd/m2)",
            probes.variant
        );
        probes.active = active;
    }
    if !gi_options().hemisphere {
        let brightness = if ready.is_some() { 0. } else { ENV_PER_UNIT };
        if ambient.brightness != brightness {
            ambient.brightness = brightness;
        }
    }
    for (entity, current, current_sky) in &cameras {
        if let Some(light) = probe_light(current, ready.as_ref()) {
            commands.entity(entity).insert(light);
        }
        if let Some((specular, _)) = &ready {
            if show_sky && current_sky.is_none_or(|s| s.image.as_ref() != Some(specular)) {
                commands.entity(entity).insert(Skybox {
                    image: Some(specular.clone()),
                    brightness: ENV_PER_UNIT,
                    ..default()
                });
            }
        } else if current_sky.is_some() {
            commands.entity(entity).remove::<Skybox>();
        }
    }
}

/// The sun, driven by the gparam light set.
#[derive(Component)]
pub struct Sun;

/// The second directional light of the light set.
#[derive(Component)]
pub struct FillLight;

#[derive(Resource)]
pub struct MapLighting {
    pub gparam: Gparam,
    pub hour: f32,
    /// Light set id in force (from the collision under the focus), and the one last applied.
    pub light_set: i32,
    pub applied: Option<i32>,
    pub source: String,
}

/// One light set at one time of day, in Bevy terms.
#[derive(Debug, Clone)]
pub struct LightSet {
    /// Four native direction/diffuse/specular triples, then runtime controls.
    pub direct: [Vec4; 13],
    pub sun_toward: Vec3,
    pub sun_color: Color,
    pub sun_lux: f32,
    pub fill_toward: Vec3,
    pub fill_color: Color,
    pub fill_lux: f32,
    pub ambient: Color,
    pub ambient_brightness: f32,
    pub hemi_axis: Vec3,
    pub hemi_delta: Vec3,
    pub sky: Color,
}

fn colour(v: &[f32]) -> (Color, f32) {
    let get = |i: usize| v.get(i).copied().unwrap_or(0.0);
    (
        // Verified 310630: gamma 2.0; W remains a separate linear multiplier.
        Color::linear_rgb(get(0).powi(2), get(1).powi(2), get(2).powi(2)),
        v.get(3).copied().unwrap_or(1.0),
    )
}

/// Verified fog copy quantizes clamped RGB to bytes before gamma 2.2 times W.
pub(crate) fn fog_colour(rgb: Vec3, w: f32) -> Vec3 {
    // Inferred nearest-byte rounding; the trace verifies quantization, not its tie rule.
    Vec3::from_array(
        rgb.clamp(Vec3::ZERO, Vec3::ONE)
            .to_array()
            .map(|c| ((c * 255.).round() / 255.).powf(2.2)),
    ) * w
}

/// Direction toward a light from a gparam (pitch, yaw) angle, in Bevy space.
fn toward(angle: &[f32]) -> Vec3 {
    let (pitch, yaw) = (
        angle.first().copied().unwrap_or(-0.8),
        angle.get(1).copied().unwrap_or(0.0),
    );
    let v = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Vec3::Z;
    // Mirror into Bevy space, and make sure the light comes from above.
    let v = Vec3::new(-v.x, v.y, v.z);
    if v.y < 0.0 { -v } else { v }
}

impl LightSet {
    pub fn from_gparam(g: &Gparam, id: i32, hour: f32) -> Self {
        // A layered file resolves a row that no layer registered to the default record.
        let id = g.light_row(id);
        let get = |name: &str| g.param("LightSet", name).and_then(|p| p.at(id, hour));
        let fog = |name: &str| g.param("VolumetricFog", name).and_then(|p| p.at(id, hour));
        let (sun_color, sun_w) = colour(&get("DiffColor0").unwrap_or(vec![1.0, 0.95, 0.85, 1.5]));
        let (fill_color, fill_w) = colour(&get("DiffColor1").unwrap_or(vec![0.5, 0.6, 0.7, 0.2]));
        let (up, up_w) = colour(&get("Hemi Color Up").unwrap_or(vec![0.6, 0.7, 0.8, 0.5]));
        let (down, down_w) = colour(&get("Hemi Color Down").unwrap_or(vec![0.4, 0.4, 0.4, 0.2]));
        let sky_raw = fog("SkyColor").unwrap_or(vec![0.5, 0.55, 0.65, 1.0]);
        let sky_rgb = fog_colour(
            Vec3::new(sky_raw[0], sky_raw[1], sky_raw[2]),
            sky_raw.get(3).copied().unwrap_or(1.),
        );
        let sky = Color::linear_rgb(sky_rgb.x, sky_rgb.y, sky_rgb.z);
        let up_l = up.to_linear();
        let down_l = down.to_linear();
        let ambient = Color::linear_rgb(
            (up_l.red * up_w + down_l.red * down_w) * 0.5,
            (up_l.green * up_w + down_l.green * down_w) * 0.5,
            (up_l.blue * up_w + down_l.blue * down_w) * 0.5,
        );
        let mut direct = [Vec4::ZERO; 13];
        let flag = |name: &str| {
            g.param("LightSet", name)
                .and_then(|p| p.nearest(id, hour))
                .and_then(|v| v.first().copied())
                .is_some_and(|v| v != 0.)
        };
        for i in 0..4 {
            let prefix = if i < 2 {
                String::new()
            } else {
                "LocalLight ".into()
            };
            let index = if i < 2 { i } else { i - 1 };
            let enabled = i < 2 || flag(&format!("Enable LocalLight{index}"));
            if !enabled {
                continue;
            }
            let diffuse = get(&format!("{prefix}DiffColor{index}")).unwrap_or_else(|| {
                if i == 0 {
                    vec![1., 0.95, 0.85, 1.5]
                } else if i == 1 {
                    vec![0.5, 0.6, 0.7, 0.2]
                } else {
                    vec![0.; 4]
                }
            });
            let specular =
                get(&format!("{prefix}SpecColor{index}")).unwrap_or_else(|| diffuse.clone());
            let pack = |v: &[f32]| {
                let (c, w) = colour(v);
                Vec3::from_array(c.to_linear().to_f32_array_no_alpha()) * w * SCENE_TO_BEVY
            };
            let follow = i >= 2 || (i == 1 && flag("Light1 FollowedCamera"));
            let angle = get(&format!("{prefix}Angle{index}"));
            // 3114b0/30df80 initialize every slot to asin(-0.7071), atan2(0.7071,0).
            // 311a50 clears colours/enables but retains those constructor directions.
            let angle = angle.unwrap_or_else(|| {
                vec![
                    f32::from_bits(0xbf35_0481).asin(),
                    std::f32::consts::FRAC_PI_2,
                ]
            });
            let native =
                Quat::from_rotation_y(angle[1]) * Quat::from_rotation_x(angle[0]) * Vec3::Z;
            let direction = Vec3::new(-native.x, native.y, native.z);
            direct[i * 3] = direction.extend(if i >= 2 { 2. } else { f32::from(follow) });
            let sharpness_name = if i >= 2 {
                format!("LocalLight Light Sharpness{index}")
            } else {
                format!("Directional Light Sharpness{index}")
            };
            let sharpness = get(&sharpness_name)
                .and_then(|v| v.first().copied())
                .unwrap_or(50.);
            direct[i * 3 + 1] = pack(&diffuse).extend(sharpness);
            direct[i * 3 + 2] = pack(&specular).extend(1.);
        }
        direct[12].x = f32::from(gi_options().native_lights);
        let zero = |direct: &mut [Vec4; 13], slots: &[usize]| {
            for &i in slots {
                direct[i] = direct[i].with_xyz(Vec3::ZERO);
            }
        };
        if ablate("nosun") {
            zero(&mut direct, &[1, 2]);
        }
        if ablate("nofill") {
            zero(&mut direct, &[4, 5]);
        }
        if ablate("nolocal") {
            zero(&mut direct, &[7, 8, 10, 11]);
        }
        if ablate("nospec") {
            zero(&mut direct, &[2, 5, 8, 11]);
        }
        let k = scale("DIRECT");
        for i in [1, 2, 4, 5, 7, 8, 10, 11] {
            direct[i] = direct[i].with_xyz(direct[i].xyz() * k);
        }
        let (sun_w, fill_w) = (
            if ablate("nosun") { 0. } else { sun_w * k },
            if ablate("nofill") { 0. } else { fill_w * k },
        );
        Self {
            direct,
            sun_toward: toward(&get("Angle0").unwrap_or_default()),
            sun_color,
            sun_lux: sun_w * SCENE_TO_BEVY,
            fill_toward: toward(&get("Angle1").unwrap_or_default()),
            fill_color,
            fill_lux: fill_w * SCENE_TO_BEVY,
            ambient,
            ambient_brightness: if ablate("noamb") {
                0.
            } else {
                ENV_PER_UNIT * scale("AMBIENT")
            },
            // Verified coefficient packing; default axis is up. Axis field resolution inferred.
            hemi_axis: get("Hemi Direction")
                .filter(|v| v.len() >= 3)
                .map_or(Vec3::Y, |v| Vec3::new(-v[0], v[1], v[2])),
            hemi_delta: Vec3::new(
                up_l.red * up_w - down_l.red * down_w,
                up_l.green * up_w - down_l.green * down_w,
                up_l.blue * up_w - down_l.blue * down_w,
            ) * 0.5,
            sky,
        }
    }
}

/// Spawns the sun and fill lights; [`apply_lighting`] sets them from the light set.
pub fn spawn_lights(commands: &mut Commands, settings: &ShadowSettings) {
    info!("map shadow budget: {settings:?}; normal bias in shadow texels");
    commands.spawn((
        Sun,
        DirectionalLight {
            shadow_maps_enabled: settings.enabled,
            shadow_depth_bias: settings.depth_bias,
            shadow_normal_bias: settings.normal_bias,
            ..default()
        },
        bevy::light::CascadeShadowConfig {
            bounds: settings.cascade_bounds(),
            ..default()
        },
        Transform::default(),
    ));
    commands.spawn((
        FillLight,
        DirectionalLight {
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::default(),
    ));
}

/// Applies the light set in force to the lights, ambient light, sky and fog when it changes
/// (and to cameras spawned since).
#[allow(clippy::type_complexity)]
pub fn apply_lighting(
    mut lighting: ResMut<MapLighting>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut shadow: Option<ResMut<ShadowSettings>>,
    mut sun: Query<
        (
            &mut DirectionalLight,
            &mut Transform,
            Option<&mut bevy::light::CascadeShadowConfig>,
        ),
        (With<Sun>, Without<FillLight>),
    >,
    mut fill: Query<(&mut DirectionalLight, &mut Transform), (With<FillLight>, Without<Sun>)>,
) {
    let _timer = crate::perf_log::SystemTimer::start("apply_lighting");
    if lighting.applied == Some(lighting.light_set) {
        return;
    }
    let set = LightSet::from_gparam(&lighting.gparam, lighting.light_set, lighting.hour);
    if lighting
        .gparam
        .param("LightSet", "DiffColor0")
        .is_none_or(|p| !p.entries.iter().any(|e| e.id == lighting.light_set))
        && !lighting.gparam.is_layered()
    {
        error!(
            "missing LightSet bank row {} in {}; parameter fallback is in use",
            lighting.light_set, lighting.source
        );
    }
    if lighting.applied != Some(lighting.light_set) {
        info!(
            "light set {} at {:.1} h from {}: {set:?}",
            lighting.light_set, lighting.hour, lighting.source
        );
    }
    lighting.applied = Some(lighting.light_set);
    // The light file or hour may author other cascade cut points; rebuild only on a change.
    let bounds = shadow.as_mut().map(|shadow| {
        let mut next = (**shadow).clone();
        next.read_shadow_param(&lighting.gparam, lighting.hour);
        if next != **shadow {
            **shadow = next;
        }
        shadow.cascade_bounds()
    });
    for (mut l, mut t, cascades) in &mut sun {
        l.color = set.sun_color;
        l.illuminance = set.sun_lux;
        *t = Transform::IDENTITY.looking_to(-set.sun_toward, Vec3::Y);
        if let (Some(bounds), Some(mut cascades)) = (&bounds, cascades)
            && cascades.bounds != *bounds
        {
            info!(
                "sun shadow cascades {:?} -> {bounds:?} from {} at {:.1} h",
                cascades.bounds, lighting.source, lighting.hour
            );
            cascades.bounds.clone_from(bounds);
        }
    }
    for (mut l, mut t) in &mut fill {
        l.color = set.fill_color;
        l.illuminance = set.fill_lux;
        *t = Transform::IDENTITY.looking_to(-set.fill_toward, Vec3::Y);
    }
    ambient.color = set.ambient;
    ambient.brightness = set.ambient_brightness;
    clear.0 = set.sky;
}

#[cfg(test)]
mod probe_tests {
    use super::*;
    #[test]
    fn probe_handoff_keeps_the_view_binding_and_restores_the_authored_intensity() {
        let mut images = Assets::<Image>::default();
        let first = (images.add(Image::default()), images.add(Image::default()));
        let second = (images.add(Image::default()), images.add(Image::default()));
        let light = probe_light(None, Some(&first)).unwrap();
        let waiting = probe_light(Some(&light), None).unwrap();
        assert_eq!(waiting.intensity, 0.);
        assert_eq!(waiting.specular_map, light.specular_map);
        assert_eq!(waiting.diffuse_map, light.diffuse_map);
        assert!(probe_light(Some(&waiting), None).is_none());
        // The same probe can finish uploading again after an environment event restart.
        let restored = probe_light(Some(&waiting), Some(&first)).unwrap();
        assert_eq!(restored.intensity, ENV_PER_UNIT);
        let switched = probe_light(Some(&waiting), Some(&second)).unwrap();
        assert_eq!(switched.specular_map, second.0);
        assert_eq!(switched.diffuse_map, second.1);
        assert_eq!(switched.intensity, ENV_PER_UNIT);
    }

    #[test]
    fn gi_grid_retains_ordered_coverage_at_rotated_bounds_and_cell_faces() {
        let mut records = Vec::new();
        let mut bounds = Vec::new();
        for i in 0..143 {
            let center = Vec3::new((i % 12) as f32 * 32. - 160., -768., (i / 12) as f32 * 32.);
            let size = if i == 0 {
                Vec3::splat(5000.)
            } else if i == 142 {
                // A thin enclosing record can sort after local boxes by native volume.
                Vec3::new(5000., 0.1, 0.1)
            } else {
                Vec3::new(20., 32., 24.)
            };
            let extent = Mat3::from_quat(Quat::from_rotation_y(i as f32 * 0.17)).abs() * size * 0.5;
            let (min, max) = (center - extent, center + extent);
            bounds.push((min, max));
            let mut r = [0; 52];
            r[24..27].copy_from_slice(&min.to_array().map(f32::to_bits));
            r[28..31].copy_from_slice(&max.to_array().map(f32::to_bits));
            records.push(r);
        }
        for records in [&records[..0], &records[..1], &records[..]] {
            let mut data = vec![records.len() as u32, 1, 0, 0];
            for r in records {
                data.extend(r);
            }
            irradiance_grid(&mut data, records);
            if records.is_empty() {
                assert_eq!(data[2], 0);
                continue;
            }
            let grid = data[2] as usize;
            let origin = Vec3::from_array(std::array::from_fn(|i| f32::from_bits(data[grid + i])));
            let inv = f32::from_bits(data[grid + 4]);
            let dims = UVec3::new(data[grid + 8], data[grid + 9], data[grid + 10]);
            let mut positions = vec![Vec3::splat(-10000.), Vec3::splat(10000.)];
            for (min, max) in &bounds[..records.len()] {
                for corner in 0..8 {
                    for e in [-0.001, 0., 0.001] {
                        positions.push(
                            Vec3::new(
                                if corner & 1 == 0 { min.x } else { max.x },
                                if corner & 2 == 0 { min.y } else { max.y },
                                if corner & 4 == 0 { min.z } else { max.z },
                            ) + Vec3::splat(e),
                        );
                    }
                }
                positions.push((*min + *max) * 0.5);
            }
            for z in 0..=dims.z {
                for x in 0..=dims.x {
                    positions.push(origin + Vec3::new(x as f32, 1., z as f32) / inv);
                }
            }
            for p in positions {
                let c = ((p - origin) * inv).floor();
                let entry = if c.cmpge(Vec3::ZERO).all() && c.cmplt(dims.as_vec3()).all() {
                    let c = c.as_uvec3();
                    grid + 16 + 2 * (c.x + dims.x * (c.y + dims.y * c.z)) as usize
                } else {
                    grid + 12
                };
                let offset = data[entry] as usize;
                let candidates = &data[offset..offset + data[entry + 1] as usize];
                assert!(candidates.windows(2).all(|w| w[0] < w[1]));
                let contains = |i: usize| p.cmpge(bounds[i].0).all() && p.cmple(bounds[i].1).all();
                let full: Vec<_> = (0..records.len()).filter(|i| contains(*i)).collect();
                let filtered: Vec<_> = candidates
                    .iter()
                    .map(|i| *i as usize)
                    .filter(|i| contains(*i))
                    .collect();
                assert_eq!(full, filtered, "receiver {p:?}");
            }
            if records.len() > 1 {
                assert!(
                    (0..dims.element_product() as usize)
                        .all(|i| data[grid + 16 + i * 2 + 1] < records.len() as u32 / 2)
                );
            }
        }
    }

    #[test]
    fn atlas_preserves_independent_coefficient_resolutions() {
        let mut record = [0u32; 52];
        record[16..19].copy_from_slice(&[4, 3, 2]);
        let dimensions = [[2, 1, 1], [4, 3, 2], [1, 2, 1], [3, 1, 2]];
        let mut source = Vec::new();
        for (axis, dims) in dimensions.iter().enumerate() {
            record[20 + axis] = source.len() as u32;
            record[40 + axis * 3..43 + axis * 3].copy_from_slice(dims);
            source.extend((0..dims.iter().product::<u32>()).map(|i| i + axis as u32 * 100));
        }
        let atlas = irradiance_atlas(std::slice::from_mut(&mut record), &source).unwrap();
        let data = atlas.data.unwrap();
        let size = atlas.texture_descriptor.size;
        for (axis, [w, h, d]) in dimensions.into_iter().enumerate() {
            for z in 0..d {
                for y in 0..h {
                    for x in 0..w {
                        let index = ((z * size.height + y) * size.width
                            + axis as u32 * record[23]
                            + x) as usize
                            * 4;
                        let actual = u32::from_le_bytes(data[index..index + 4].try_into().unwrap());
                        assert_eq!(actual, (z * h + y) * w + x + axis as u32 * 100);
                    }
                }
            }
        }
    }

    #[test]
    fn cached_volume_upload_matches_native_axes_and_cube_array_layout() {
        let dir = sekiro_sim::cache::root().join("maps/m11_01_00_00");
        if !dir.join("env/giiv0_02_w.rgba8").exists() {
            return;
        }
        let env: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("env.json")).unwrap()).unwrap();
        let r = env["regions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["probe"] == 0 && !r["size"].is_null())
            .unwrap();
        let mut probes = MapProbes {
            variant: 2,
            ..default()
        };
        probes.available.insert(0);
        let position = Vec3::from_array(serde_json::from_value(r["position"].clone()).unwrap());
        let rotation = Quat::from_array(serde_json::from_value(r["rotation"].clone()).unwrap());
        let size = Vec3::from_array(serde_json::from_value(r["size"].clone()).unwrap());
        probes.regions.push(ProbeRegion {
            probe: 0,
            position,
            rotation,
            size: Some(size),
        });
        let mut app = App::new();
        app.insert_resource(Assets::<bevy::render::storage::ShaderBuffer>::default())
            .insert_resource(Assets::<Image>::default());
        install_irradiance(&mut app, &dir, &probes).unwrap();
        let gi = app.world().resource::<MapIrradiance>();
        let buffer = app
            .world()
            .resource::<Assets<bevy::render::storage::ShaderBuffer>>()
            .get(&gi.buffer)
            .unwrap();
        let words = buffer.cast_slice::<u32>().unwrap();
        assert_eq!(words[0], 1);
        assert_eq!(words[23], 0); // First cube array layer, independent of sparse probe IDs.
        let matrix = Mat4::from_cols_array(&std::array::from_fn(|i| f32::from_bits(words[4 + i])));
        assert!((matrix.transform_point3(position) - Vec3::new(0.5, 0., 0.5)).length() < 0.0001);
        assert!(
            (matrix
                .transform_point3(position + rotation * Vec3::X * size.x * 0.5)
                .x)
                .abs()
                < 0.0001
        );
        assert_eq!(&words[24..27], &[0, 0, 0]);
        assert!(words[27] >= words[20]);
        let voxels = app
            .world()
            .resource::<Assets<Image>>()
            .get(&gi.voxels)
            .unwrap();
        assert_eq!(
            voxels.texture_descriptor.dimension,
            bevy::render::render_resource::TextureDimension::D3
        );
        let raw = std::fs::read(dir.join("env/giiv0_02_x.rgba8")).unwrap();
        assert_eq!(&voxels.data.as_ref().unwrap()[..4], &raw[12..16]);
        for (axis, name) in ['x', 'y', 'z', 'w'].into_iter().enumerate() {
            let raw = std::fs::read(dir.join(format!("env/giiv0_02_{name}.rgba8"))).unwrap();
            let (w, h, d) = (words[20], words[21], words[22]);
            let image_width = voxels.texture_descriptor.size.width;
            let image_height = voxels.texture_descriptor.size.height;
            for (native, atlas) in [
                (0, axis as u32 * words[27]),
                (
                    w * h * d - 1,
                    ((d - 1) * image_height + h - 1) * image_width + axis as u32 * words[27] + w
                        - 1,
                ),
            ] {
                assert_eq!(
                    &voxels.data.as_ref().unwrap()[atlas as usize * 4..atlas as usize * 4 + 4],
                    &raw[12 + native as usize * 4..16 + native as usize * 4]
                );
            }
        }
        for handle in [&gi.specular, &gi.diffuse] {
            let image = app.world().resource::<Assets<Image>>().get(handle).unwrap();
            assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 6);
            assert_eq!(
                image.texture_view_descriptor.as_ref().unwrap().dimension,
                Some(bevy::render::render_resource::TextureViewDimension::CubeArray)
            );
        }
    }
    #[test]
    fn outskirts_map_pieces_use_row_zero_without_local_light() {
        let cache = sekiro_sim::cache::root().join("raw");
        let (Ok(msb), Ok(gparam)) = (
            std::fs::read(cache.join("map/mapstudio/m11_00_00_00.msb")),
            std::fs::read(cache.join("param/drawparam/m11_00_0000.gparam")),
        ) else {
            return;
        };
        let msb = sekiro_formats::msb::parse(&msb).unwrap();
        let g = sekiro_formats::gparam::parse(&gparam).unwrap();
        assert_eq!(part_light_set(100, false), MAP_PART_LIGHT_SET);
        let (mut own, mut scene) = (0, 0);
        for p in &msb.parts {
            if !matches!(
                p.kind,
                sekiro_formats::msb::PartKind::MapPiece | sekiro_formats::msb::PartKind::Object
            ) {
                continue;
            }
            let id = p.gparam.map_or(-1, |g| g[0]);
            if part_follows_scene(id) {
                scene += 1;
            } else {
                assert_eq!(id, MAP_PART_LIGHT_SET, "{}", p.name);
                own += 1;
            }
        }
        assert!(own > 5_000 && scene <= 4, "own {own}, scene {scene}");
        // Hour 6 outside the shack: the collision row 100 carries bright local lights, the map
        // parts' row 0 authors them with W 0 (#30).
        let part = LightSet::from_gparam(&g, part_light_set(100, false), 6.);
        let collision = LightSet::from_gparam(&g, part_light_set(100, true), 6.);
        for slot in [7, 10] {
            assert_eq!(part.direct[slot].xyz(), Vec3::ZERO, "slot {slot}");
        }
        assert!(collision.direct[7].xyz().max_element() > 0.);
        assert!(part.direct[1].xyz().max_element() < collision.direct[1].xyz().max_element());
    }
    #[test]
    fn squared_light_colours_and_hemisphere_endpoints() {
        use sekiro_formats::gparam::{Entry, Group, Param, Value};
        let g = Gparam {
            groups: vec![Group {
                name: "LightSet".into(),
                display_name: String::new(),
                params: [
                    ("DiffColor0", [0.5, 0.25, 1., 2.]),
                    ("SpecColor0", [0.25, 0.5, 1., 3.]),
                    ("Enable LocalLight1", [1., 0., 0., 0.]),
                    ("Enable LocalLight2", [1., 0., 0., 0.]),
                    ("LocalLight DiffColor1", [0.5, 1., 0.25, 2.]),
                    ("LocalLight SpecColor1", [0.25, 1., 0.5, 3.]),
                    ("Directional LocalLight Light Sharpness1", [15., 0., 0., 0.]),
                    ("LocalLight DiffColor2", [0., 0., 0., 0.]),
                    ("LocalLight SpecColor2", [0.5, 0.25, 1., 0.5]),
                    ("Light1 FollowedCamera", [1., 0., 0., 0.]),
                    ("Hemi Color Up", [0.5, 1., 0.25, 2.]),
                    ("Hemi Color Down", [1., 0.5, 0.5, 0.5]),
                ]
                .into_iter()
                .map(|(name, value)| Param {
                    name: name.into(),
                    display_name: String::new(),
                    entries: vec![Entry {
                        id: 0,
                        time: 12.,
                        value: Value::Vec4(value),
                    }],
                })
                .collect(),
            }],
        };
        let set = LightSet::from_gparam(&g, 0, 12.);
        assert_eq!(
            set.direct[2].xyz() / SCENE_TO_BEVY,
            Vec3::new(0.1875, 0.75, 3.)
        );
        assert_eq!(
            set.direct[7].xyz() / SCENE_TO_BEVY,
            Vec3::new(0.5, 2., 0.125)
        );
        assert_eq!(
            set.direct[8].xyz() / SCENE_TO_BEVY,
            Vec3::new(0.1875, 3., 0.75)
        );
        assert_eq!(set.direct[10].xyz(), Vec3::ZERO);
        assert!((set.direct[6].xyz().length() - 1.).abs() < 0.00001);
        assert_eq!(set.direct[6].w, 2.);
        assert_eq!(set.direct[7].w, 15.);
        assert_eq!(set.direct[4].w, 50.);
        assert_eq!(set.direct[8].w, 1.);
        assert_eq!(set.direct[9], set.direct[6]);
        assert_eq!(set.direct[11].w, 1.);
        assert_eq!(
            set.direct[11].xyz() / SCENE_TO_BEVY,
            Vec3::new(0.125, 0.03125, 0.5)
        );
        assert_eq!(set.direct[3].w, 1.);
        let c = set.sun_color.to_linear();
        assert_eq!(
            Vec3::new(c.red, c.green, c.blue) * set.sun_lux / SCENE_TO_BEVY,
            Vec3::new(0.5, 0.125, 2.)
        );
        let mean = set.ambient.to_linear();
        let mean = Vec3::new(mean.red, mean.green, mean.blue);
        assert_eq!(mean + set.hemi_delta, Vec3::new(0.5, 2., 0.125));
        assert_eq!(mean - set.hemi_delta, Vec3::new(0.5, 0.125, 0.125));
        let fog = fog_colour(Vec3::new(0.5, 1., -1.), 2.);
        assert!((fog.x - 2. * (128_f32 / 255.).powf(2.2)).abs() < 0.000001);
        assert_eq!(fog.yz(), Vec2::new(2., 0.));
    }
    #[test]
    fn scene_bridge_is_pi_over_the_default_camera_exposure() {
        let exposure = bevy::camera::Exposure::default().exposure();
        let expected = std::f32::consts::PI / exposure;
        assert!(
            (SCENE_TO_BEVY / expected - 1.).abs() < 1e-4,
            "{SCENE_TO_BEVY} vs {expected}"
        );
        // Native radiance of a white Lambert surface at Col * N.L = 1, in Bevy HDR units.
        let radiance = SCENE_TO_BEVY / std::f32::consts::PI * exposure;
        assert!((radiance - 1.).abs() < 1e-4);
        assert!((ENV_PER_UNIT * exposure - 1.).abs() < 1e-4);
    }
    #[test]
    fn shadow_options_reject_unbounded_or_unsupported_configurations() {
        let g = Gparam { groups: vec![] };
        let args = |value: &str| vec!["--shadow".into(), value.into()];
        assert!(ShadowSettings::from_args(&g, 12., &args("0,60")).is_err());
        assert!(ShadowSettings::from_args(&g, 12., &args("5,60")).is_err());
        assert!(ShadowSettings::from_args(&g, 12., &args("2,NaN")).is_err());
        let s = ShadowSettings::from_args(&g, 12., &args("4,250")).unwrap();
        assert_eq!(s.cascades, 4);
        assert_eq!(s.distance, 250.);
        assert_eq!(s.cascade_bounds(), vec![10., 30., 70., 250.]);
        let s = ShadowSettings::from_args(&g, 12., &args("2,60")).unwrap();
        assert_eq!(s.cascade_bounds(), vec![10., 60.]);
        let s = ShadowSettings::from_args(&g, 12., &args("1,5")).unwrap();
        assert_eq!(s.cascade_bounds(), vec![5.]);
    }
    #[test]
    fn shadow_cascades_follow_authored_shadow_param() {
        use sekiro_formats::gparam::{Entry, Group, Param, Value};
        let g = Gparam {
            groups: vec![Group {
                name: "ShadowParam ParamEditor".into(),
                display_name: String::new(),
                params: [
                    ("ShadowParam - Shadow: Cascade Dist 0->1", 25.),
                    ("ShadowParam - Shadow: Cascade Dist 1->2", 50.),
                    ("ShadowParam - Shadow: Cascade Dist 2->3", 100.),
                    ("ShadowParam - Shadow: Far Fade Start", 400.),
                ]
                .into_iter()
                .map(|(name, value)| Param {
                    name: name.into(),
                    display_name: String::new(),
                    entries: vec![Entry {
                        id: 0,
                        time: 0.,
                        value: Value::Float(value),
                    }],
                })
                .collect(),
            }],
        };
        let s = ShadowSettings::from_args(&g, 6., &[]).unwrap();
        assert_eq!(s.cascade_bounds(), vec![25., 50., 100., 400.]);
        // Without authored values the native defaults reach 1000 m, past distant cliffs (#9).
        let s = ShadowSettings::from_args(&Gparam { groups: vec![] }, 6., &[]).unwrap();
        assert_eq!(s.cascade_bounds(), vec![10., 30., 70., 1000.]);
        // No map caster budget by default, so ground shadows don't pop in (#66).
        assert_eq!(s.caster_distance, None);
        let args = ["--shadow-casters", "50"].map(String::from);
        let s = ShadowSettings::from_args(&g, 6., &args).unwrap();
        assert_eq!(s.caster_distance, Some(50.));
    }
    #[test]
    fn post_effects_only_mark_3d_cameras_and_disabled_lut_is_removed() {
        use sekiro_formats::gparam::{Entry, Group, Param, Value};
        let mut app = App::new();
        let lut = Handle::<Image>::default();
        let gparam = Gparam {
            groups: vec![Group {
                name: "ColorGrading[Yebis]".into(),
                display_name: String::new(),
                params: ["Enable", "LutSourceId"]
                    .into_iter()
                    .map(|name| Param {
                        name: name.into(),
                        display_name: String::new(),
                        entries: vec![Entry {
                            id: 0,
                            time: 12.,
                            value: Value::Int(if name == "Enable" { 1 } else { 30 }),
                        }],
                    })
                    .collect(),
            }],
        };
        app.insert_resource(MapLighting {
            gparam,
            hour: 12.,
            light_set: 0,
            applied: None,
            source: "fixture".into(),
        })
        .insert_resource(MapPostEffects {
            luts: [(30, lut)].into(),
            ..default()
        })
        .add_systems(Update, apply_post_effects);
        let scene = app.world_mut().spawn(Camera3d::default()).id();
        let hud = app.world_mut().spawn(Camera2d).id();
        app.update();
        assert!(app.world().get::<Hdr>(scene).is_some());
        assert_eq!(
            *app.world().get::<Tonemapping>(scene).unwrap(),
            Tonemapping::None
        );
        assert!(app.world().get::<tonemap::GamePost>(scene).is_some());
        assert!(app.world().get::<Bloom>(scene).is_none());
        assert!(app.world().get::<AutoExposure>(scene).is_none());
        assert!(app.world().get::<grading::GradingLut>(scene).is_some());
        assert!(app.world().get::<Hdr>(hud).is_none());
        assert!(app.world().get::<tonemap::GamePost>(hud).is_none());
        assert!(app.world().get::<Bloom>(hud).is_none());
        assert!(app.world().get::<AutoExposure>(hud).is_none());
        assert!(app.world().get::<grading::GradingLut>(hud).is_none());
        app.world_mut().resource_mut::<MapPostEffects>().no_grading = true;
        app.world_mut().resource_mut::<MapLighting>().hour = 18.;
        app.update();
        assert!(app.world().get::<grading::GradingLut>(scene).is_none());
    }
    #[test]
    fn recovered_post_preserves_authored_bounds_and_disables_stages() {
        let set = PostSettings {
            exposure: 0.4,
            middle: -4.2,
            min: -0.01,
            max: 8.,
            auto: true,
            lut: None,
            alpha: 0.5,
            threshold: 1.5,
            radius: 3.,
            glare: 1.,
            saturation: 0.,
            center: 1.,
            freeze: false,
            mode: 7,
            light_adapt: 1.,
            dark_adapt: 0.5,
            filmic: 0.5,
            reinhard: 1.6,
            white_balance: 1.7,
            bloom_normalizer: 3.951424,
            bloom_levels: 7,
            bloom_smooth: 0.8,
            correction: Mat4::IDENTITY,
        };
        let upload = set.upload(&MapPostEffects::default());
        assert_eq!(upload.adaptation.y, 8.);
        assert_eq!(upload.bloom.x, 1.5);
        assert_eq!(upload.bloom.z, 3.);
        assert_eq!(upload.bloom.y, 0.5 / 3.951424);
        let disabled = set.upload(&MapPostEffects {
            no_auto: true,
            no_bloom: true,
            ..default()
        });
        assert_eq!(disabled.exposure.w, 0.);
        assert_eq!(disabled.bloom.y, 0.);
    }
    #[test]
    fn authored_colour_controls_reach_the_pre_gamma_matrix() {
        use sekiro_formats::gparam::{Entry, Group, Param, Value};
        let gparam = Gparam {
            groups: vec![Group {
                name: "Tone Map[Yebis]".into(),
                display_name: String::new(),
                params: [
                    ("GS Presents - ToneMap-Contrast", 0.2),
                    ("GS Presents - ColorH", 30.),
                    ("GS Presents - ColorS", 0.),
                    ("GS Presents - ColorV", 0.8),
                    ("GS Presents - Contrast", 0.7),
                ]
                .into_iter()
                .map(|(name, value)| Param {
                    name: name.into(),
                    display_name: String::new(),
                    entries: vec![Entry {
                        id: 0,
                        time: 22.,
                        value: Value::Float(value),
                    }],
                })
                .collect(),
            }],
        };
        let post = PostSettings::from_gparam(&gparam, 0, 22.).upload(&MapPostEffects::default());
        assert_eq!(post.curve.z, 0.2);
        // Native saturation zero preserves weighted luminance, even after hue.
        let red = post.colour * Vec4::new(1., 0., 0., 1.);
        assert!(
            (red.truncate() - Vec3::splat(0.3173896))
                .abs()
                .max_element()
                < 0.000002
        );
        // Hue rotates around the luminance axis after its basis change, preserving grey.
        let grey = tone_correction(67., 0.6, 0.8, 0.7) * Vec4::new(0.5, 0.5, 0.5, 1.);
        assert!((grey.truncate() - Vec3::splat(0.43)).abs().max_element() < 0.000002);
    }
    #[test]
    fn native_constructor_defaults_and_bloom_normalization() {
        let set = PostSettings::from_gparam(&Gparam::default(), 0, 12.);
        assert_eq!((set.mode, set.middle, set.min, set.max), (7, -2., -8., 8.));
        assert_eq!((set.light_adapt, set.dark_adapt, set.center), (1., 0.5, 4.));
        assert!((set.bloom_normalizer - 3.951424).abs() < 0.000001);
        let upload = set.upload(&MapPostEffects::default());
        assert_eq!(upload.exposure.x, 1.);
        assert_eq!(upload.exposure.z, -2.);
        assert_eq!(upload.transfer.x, 1.);
        assert_eq!(upload.transfer.y, 1. / 2.2);
    }
    #[test]
    fn extracted_hdr_cube_loads_as_six_layers() {
        use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
        let dir = sekiro_sim::cache::root().join("maps/m11_00_00_00/env");
        for (file, size, levels) in [("gilm0_02.dds", 128, 8), ("gilm0_02_diffuse.dds", 16, 1)] {
            let path = dir.join(file);
            if !path.exists() {
                return;
            }
            let image = Image::from_buffer(
                &std::fs::read(path).unwrap(),
                ImageType::Extension("dds"),
                CompressedImageFormats::NONE,
                false,
                ImageSampler::default(),
                bevy::asset::RenderAssetUsages::default(),
            )
            .unwrap();
            assert_eq!(image.texture_descriptor.size.width, size);
            assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 6);
            assert_eq!(image.texture_descriptor.mip_level_count, levels);
            assert_eq!(
                image.texture_descriptor.format,
                bevy::render::render_resource::TextureFormat::Rgba16Float
            );
            assert_eq!(
                image.texture_view_descriptor.unwrap().dimension,
                Some(bevy::render::render_resource::TextureViewDimension::Cube)
            );
        }
    }
    #[test]
    fn hour_wrap_and_mapping() {
        for (hour, variant) in [
            (0., 0),
            (6., 1),
            (12., 2),
            (18., 3),
            (22., 4),
            (2., 5),
            (25., 0),
            (-2., 4),
        ] {
            assert_eq!(nearest_variant(hour), variant);
        }
        assert_eq!(nearest_available_variant(12., [4].into_iter()), Some(4));
        assert_eq!(
            nearest_available_variant(0., [1, 2, 3, 4, 5].into_iter()),
            Some(4)
        );
    }
    #[test]
    fn rotated_box_before_nearest_point() {
        let mut probes = MapProbes::default();
        probes.available.extend([1, 2]);
        probes.regions = vec![
            ProbeRegion {
                probe: 1,
                position: Vec3::ZERO,
                rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                size: Some(Vec3::new(2., 4., 8.)),
            },
            ProbeRegion {
                probe: 2,
                position: Vec3::X * 3.,
                rotation: Quat::IDENTITY,
                size: None,
            },
        ];
        assert_eq!(probes.choose(Vec3::X * 3.), Some(1));
        assert_eq!(probes.choose(Vec3::X * 5.), Some(2));
        probes.available.remove(&1);
        assert_eq!(probes.choose(Vec3::ZERO), Some(2));
    }
}
