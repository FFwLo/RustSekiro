//! Compose production WESL with Cargo's Bevy sources, then validate the resulting WGSL.
use bevy::{
    asset::{AssetId, Assets},
    shader::{Shader, ShaderCache, ShaderCacheError, ShaderCacheSource, ValidateShader},
};
use std::path::{Path, PathBuf};

pub fn validate(
    _: &(),
    source: ShaderCacheSource,
    _: &ValidateShader,
) -> Result<(), ShaderCacheError> {
    let ShaderCacheSource::Wgsl(source) = source else {
        panic!("expected WGSL");
    };
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    Ok(())
}

fn load(path: &Path, root: &Path, package: &str, shaders: &mut Vec<Shader>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            load(&path, root, package, shaders);
        } else if path.extension().is_some_and(|s| s == "wesl") {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            shaders.push(Shader::from_wesl(
                std::fs::read_to_string(&path).unwrap(),
                format!("embedded://{package}/{relative}"),
            ));
        }
    }
}

/// Register a production module with the game's embedded asset path.
pub fn register(
    cache: &mut ShaderCache<(), ()>,
    assets: &mut Assets<Shader>,
    name: &str,
    source: &'static str,
) -> AssetId<Shader> {
    let shader = Shader::from_wesl(source, format!("embedded://sekiro_rs/{name}.wesl"));
    let id = assets.add(shader.clone()).id();
    cache.set_shader(id, shader);
    id
}

fn bevy_version() -> &'static str {
    include_str!("../../../../Cargo.lock")
        .split("[[package]]")
        .find(|package| package.lines().any(|line| line == "name = \"bevy\""))
        .and_then(|package| {
            package
                .lines()
                .find_map(|line| line.strip_prefix("version = \""))
        })
        .and_then(|version| version.strip_suffix('"'))
        .expect("locked Bevy version")
}

pub fn cache() -> (ShaderCache<(), ()>, Assets<Shader>) {
    let cargo = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .unwrap(),
            )
            .join(".cargo")
        });
    let mut shaders = Vec::new();
    for registry in std::fs::read_dir(cargo.join("registry/src")).unwrap() {
        for package in std::fs::read_dir(registry.unwrap().path()).unwrap() {
            let package = package.unwrap().path();
            let name = package.file_name().unwrap().to_string_lossy();
            if let Some(name) = name
                .strip_suffix(&format!("-{}", bevy_version()))
                .filter(|s| s.starts_with("bevy_"))
            {
                let root = package.join("src");
                load(&root, &root, name, &mut shaders);
            }
        }
    }
    assert!(
        !shaders.is_empty(),
        "locked Bevy shader sources must be available"
    );
    let mut cache = ShaderCache::new((), validate);
    let mut assets = Assets::<Shader>::default();
    for shader in shaders {
        let id = assets.add(shader.clone()).id();
        cache.set_shader(id, shader);
    }
    (cache, assets)
}
