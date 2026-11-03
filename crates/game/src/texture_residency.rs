//! Share identical immutable DDS uploads across connected blocks, preserving sampler identity.
use bevy::platform::hash::FixedHasher;
use bevy::prelude::*;
use bevy::render::render_asset::{AssetExtractionSystems, ExtractedAssets, RenderAssets};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};

#[derive(Resource, Default)]
struct Textures {
    buckets: HashMap<u64, Vec<AssetId<Image>>>,
    aliases: HashMap<AssetId<Image>, Handle<Image>>,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Textures>()
        .add_systems(Last, intern.after(crate::vram::trim_textures));
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .init_resource::<Textures>()
            .add_systems(ExtractSchedule, extract.after(AssetExtractionSystems))
            .add_systems(
                Render,
                share
                    .in_set(RenderSystems::PrepareAssets)
                    .after(bevy::render::render_asset::prepare_assets::<GpuImage>)
                    .before(
                        bevy::render::erased_render_asset::prepare_erased_assets::<
                            MeshMaterial3d<StandardMaterial>,
                        >,
                    )
                    .before(
                        bevy::render::erased_render_asset::prepare_erased_assets::<
                            MeshMaterial3d<crate::map::LayeredMaterial>,
                        >,
                    ),
            );
    }
}

fn fingerprint(image: &Image) -> u64 {
    let mut hash = FixedHasher.build_hasher();
    let d = &image.texture_descriptor;
    d.format.hash(&mut hash);
    d.size.hash(&mut hash);
    d.mip_level_count.hash(&mut hash);
    d.sample_count.hash(&mut hash);
    d.dimension.hash(&mut hash);
    d.usage.hash(&mut hash);
    d.view_formats.hash(&mut hash);
    format!("{:?}", image.sampler).hash(&mut hash);
    image.texture_view_descriptor.is_some().hash(&mut hash);
    if let Some(view) = &image.texture_view_descriptor {
        view.label.hash(&mut hash);
        view.format.hash(&mut hash);
        view.dimension.hash(&mut hash);
        view.usage.hash(&mut hash);
        view.aspect.hash(&mut hash);
        view.base_mip_level.hash(&mut hash);
        view.mip_level_count.hash(&mut hash);
        view.base_array_layer.hash(&mut hash);
        view.array_layer_count.hash(&mut hash);
    }
    image.data_order.hash(&mut hash);
    image.asset_usage.hash(&mut hash);
    image.copy_on_resize.hash(&mut hash);
    image.data.hash(&mut hash);
    hash.finish()
}

fn intern(
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
    mut events: MessageReader<bevy::asset::AssetEvent<Image>>,
    mut state: ResMut<Textures>,
) {
    for event in events.read() {
        let id = match event {
            bevy::asset::AssetEvent::Added { id } => *id,
            bevy::asset::AssetEvent::Removed { id } | bevy::asset::AssetEvent::Unused { id } => {
                state.aliases.remove(id);
                continue;
            }
            _ => continue,
        };
        if !crate::map::world::asset_has_extension(&assets, id, "dds") {
            continue;
        }
        let Some(image) = images.get(id).filter(|image| image.data.is_some()) else {
            continue;
        };
        let key = fingerprint(image);
        let bucket = state.buckets.entry(key).or_default();
        bucket.retain(|candidate| images.contains(*candidate));
        // Compare the complete image on a hash hit, including format, mips and sampler.
        let canonical = bucket
            .iter()
            .copied()
            .find(|candidate| *candidate != id && images.get(*candidate) == Some(image));
        if let Some(canonical) = canonical.and_then(|id| images.get_strong_handle(id)) {
            state.aliases.insert(id, canonical);
            if let Some(mut image) = images.get_mut(id) {
                image.data = None;
            }
        } else if !bucket.contains(&id) {
            bucket.push(id);
        }
    }
}

fn extract(
    main: Extract<Res<Textures>>,
    mut state: ResMut<Textures>,
    mut extracted: ResMut<ExtractedAssets<GpuImage>>,
) {
    if main.is_changed() {
        state.aliases.clone_from(&main.aliases);
    }
    let extracted = &mut *extracted;
    extracted.extracted.retain(|(id, _)| {
        if state.aliases.contains_key(id) {
            extracted.added.remove(id);
            extracted.modified.remove(id);
            extracted.removed.insert(*id);
            false
        } else {
            true
        }
    });
}

fn share(state: Res<Textures>, mut images: ResMut<RenderAssets<GpuImage>>) {
    for (alias, canonical) in &state.aliases {
        if let Some(image) = images.get(canonical.id())
            && images
                .get(*alias)
                .is_none_or(|alias| alias.texture.id() != image.texture.id())
        {
            let image = image.clone();
            images.insert(*alias, image);
        }
    }
}
