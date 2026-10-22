//! The game's WESL shaders composed against the locked Bevy's own shader sources (cargo registry)
//! and validated with naga, for the shader-def sets the pipelines use. Shader errors otherwise only
//! show at run time. After sekiro-rs crates/game/tests/it/shader_support.rs (f3be187).
use bevy::asset::Assets;
use bevy::shader::{Shader, ShaderCache, ShaderCacheError, ShaderCacheSource, ShaderDefVal, ValidateShader};
use std::path::{Path, PathBuf};

fn validate(_: &(), source: ShaderCacheSource, _: &ValidateShader) -> Result<(), ShaderCacheError> {
    let ShaderCacheSource::Wgsl(source) = source else {
        panic!("expected WGSL");
    };
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    Ok(())
}

fn load(path: &Path, root: &Path, package: &str, shaders: &mut Vec<Shader>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            load(&path, root, package, shaders);
        } else if path.extension().is_some_and(|s| s == "wesl") {
            let relative = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            shaders.push(Shader::from_wesl(std::fs::read_to_string(&path).unwrap(), format!("embedded://{package}/{relative}")));
        }
    }
}

fn bevy_version() -> &'static str {
    include_str!("../Cargo.lock")
        .split("[[package]]")
        .find(|p| p.lines().any(|l| l.trim() == "name = \"bevy\""))
        .and_then(|p| p.lines().find_map(|l| l.trim().strip_prefix("version = \"")))
        .and_then(|v| v.strip_suffix('"'))
        .expect("locked Bevy version")
}

fn cache() -> (ShaderCache<(), ()>, Assets<Shader>) {
    let cargo = std::env::var_os("CARGO_HOME").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap()).join(".cargo")
    });
    let mut shaders = Vec::new();
    for registry in std::fs::read_dir(cargo.join("registry/src")).unwrap() {
        for package in std::fs::read_dir(registry.unwrap().path()).unwrap() {
            let package = package.unwrap().path();
            let name = package.file_name().unwrap().to_string_lossy().into_owned();
            if let Some(name) = name.strip_suffix(&format!("-{}", bevy_version())).filter(|s| s.starts_with("bevy_")) {
                let root = package.join("src");
                load(&root, &root, name, &mut shaders);
            }
        }
    }
    assert!(!shaders.is_empty(), "the locked Bevy's shader sources must be in the cargo registry");
    let mut cache = ShaderCache::new((), validate);
    let mut assets = Assets::<Shader>::default();
    for shader in shaders {
        let id = assets.add(shader.clone()).id();
        cache.set_shader(id, shader);
    }
    (cache, assets)
}

/// Composes `name` (a file in src/) with each def set; panics with naga's message on an error.
fn check(name: &str, source: &'static str, sets: &[Vec<ShaderDefVal>]) {
    let (mut cache, mut assets) = cache();
    let shader = Shader::from_wesl(source, format!("embedded://rustsekiro/{name}"));
    let id = assets.add(shader.clone()).id();
    cache.set_shader(id, shader);
    for (i, defs) in sets.iter().enumerate() {
        if let Err(e) = cache.get(i, id, defs) {
            panic!("{name} with {defs:?}: {e}");
        }
    }
}

/// The view / mesh defs the mesh pipeline always sets, plus the material group.
fn mesh_defs(extra: &[&'static str]) -> Vec<ShaderDefVal> {
    let mut d = vec![
        ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3),
        ShaderDefVal::UInt("MAX_DIRECTIONAL_LIGHTS".into(), 10),
        ShaderDefVal::UInt("MAX_CASCADES_PER_LIGHT".into(), 4),
        ShaderDefVal::UInt("MAX_RECT_LIGHTS".into(), 8),
        ShaderDefVal::Int("PER_OBJECT_BUFFER_BATCH_SIZE".into(), 1),
        "VERTEX_POSITIONS".into(),
        "VERTEX_NORMALS".into(),
        "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
    ];
    d.extend(extra.iter().map(|&s| ShaderDefVal::from(s)));
    d
}

/// The vertex-attribute combinations the game's meshes come with.
fn attr_sets(base: &[&'static str]) -> Vec<Vec<ShaderDefVal>> {
    let mut sets = Vec::new();
    for extra in [&[][..], &["VERTEX_UVS", "VERTEX_UVS_A"], &["VERTEX_UVS", "VERTEX_UVS_A", "VERTEX_TANGENTS"], &["VERTEX_UVS", "VERTEX_UVS_A", "VERTEX_UVS_B", "VERTEX_TANGENTS", "VERTEX_COLORS"]] {
        let mut v: Vec<&'static str> = base.to_vec();
        v.extend_from_slice(extra);
        sets.push(mesh_defs(&v));
    }
    sets
}

#[test]
fn shader_fullscreen_passes_validate() {
    check("height_fog.wesl", include_str!("height_fog.wesl"), &[vec![], vec!["MULTISAMPLED_DEPTH".into()]]);
    check("grading_lut.wesl", include_str!("grading_lut.wesl"), &[vec![]]);
}

#[test]
fn shader_fxr_materials_validate() {
    // Every Camera3d carries ScreenSpaceTransmission (fxr.rs), so its views bind the texture.
    check("fx_distort.wesl", include_str!("fx_distort.wesl"), &attr_sets(&["VIEW_TRANSMISSION_TEXTURE"]));
    check("fx_multi.wesl", include_str!("fx_multi.wesl"), &attr_sets(&[]));
    let mut soft = attr_sets(&[]);
    soft.extend(attr_sets(&["MAY_DISCARD"]));
    check("fx_soft.wesl", include_str!("fx_soft.wesl"), &soft);
}

#[test]
fn shader_sky_validates() {
    check("sky.wesl", include_str!("sky.wesl"), &[mesh_defs(&["VERTEX_UVS", "VERTEX_UVS_A", "VERTEX_UVS_B"])]);
}

#[test]
fn shader_character_material_validates() {
    let mut sets = attr_sets(&[]);
    sets.extend(attr_sets(&["MAY_DISCARD"]));
    check("sekiro_material.wesl", include_str!("sekiro_material.wesl"), &sets);
}

#[test]
fn shader_map_material_validates() {
    let mut sets = attr_sets(&[]);
    sets.extend(attr_sets(&["MAP_GAME_ATTRS"]));
    sets.extend(attr_sets(&["BINDLESS"]));
    sets.extend(attr_sets(&["BINDLESS", "MAP_GAME_ATTRS", "VISIBILITY_RANGE_DITHER"]));
    check("map_material.wesl", include_str!("map_material.wesl"), &sets);
}
