//! Wire HUD sprites loaded only from extracted cache/menu/wire PNGs.
use anyhow::Result;
use bevy::{
    asset::{embedded_asset, embedded_path},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use sekiro_sim::wire::MarkerState;
use std::path::Path;

#[derive(Component)]
pub(super) struct Marker;
#[derive(Component)]
pub(super) struct Fill;
#[derive(Component)]
pub(super) struct Cursor;

/// Normalized radius = central dot radius + authored disc expansion * fill.
const REVEAL_CURVE: Vec2 = Vec2::new(0.11, 0.39);

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(super) struct WireFill {
    /// fill, texture width in texels, central radius, radial expansion.
    #[uniform(0)]
    pub settings: Vec4,
    #[texture(1)]
    pub texture: Handle<Image>,
}

impl UiMaterial for WireFill {
    fn fragment_shader() -> ShaderRef {
        bevy::asset::AssetPath::from_path_buf(embedded_path!("grapple_hud.wesl"))
            .with_source("embedded")
            .into()
    }
}

pub(super) fn plugin(app: &mut App) {
    embedded_asset!(app, "grapple_hud.wesl");
    app.add_plugins(UiMaterialPlugin::<WireFill>::default());
}

#[derive(Resource)]
pub(super) struct Sprites {
    ready: Handle<Image>,
    distance: Handle<Image>,
    ready_ring: Handle<Image>,
    distant_ring: Handle<Image>,
    pub base_size: Vec2,
}

impl Sprites {
    pub(super) fn ring(&self, state: MarkerState) -> Handle<Image> {
        if state == MarkerState::Ready {
            self.ready_ring.clone()
        } else {
            self.distant_ring.clone()
        }
    }

    pub(super) fn draw(
        &self,
        state: MarkerState,
        material: &mut bevy::asset::AssetMut<'_, WireFill>,
    ) {
        let (texture, fill) = match state {
            MarkerState::Distant { fill } => (&self.distance, fill),
            _ => (&self.ready, 1.0),
        };
        if material.texture != *texture || material.settings.x != fill {
            material.texture = texture.clone();
            material.settings.x = fill;
        }
    }
}

pub(super) fn spawn(
    commands: &mut Commands,
    cache: &Path,
    images: &mut Assets<Image>,
    materials: &mut Assets<WireFill>,
) {
    // Load all five before spawning anything, so a missing cache warns once and hides the HUD.
    let load = |name| -> Result<Image> {
        Ok(Image::from_buffer(
            &std::fs::read(cache.join(format!("menu/wire/{name}.png")))?,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            bevy::image::ImageSampler::default(),
            bevy::asset::RenderAssetUsages::default(),
        )?)
    };
    let loaded = (|| -> Result<_> {
        Ok((
            load("MENU_Wire_Marker")?,
            load("MENU_Wire_Distance")?,
            load("MENU_Wire_01")?,
            load("MENU_Wire_02")?,
            load("MENU_Wire_Cursor")?,
        ))
    })();
    let (ready, distance, ring, distant_ring, cursor) = match loaded {
        Ok(sprites) => sprites,
        Err(e) => {
            warn!("wire HUD unavailable: {e:#}; run sekiro-extract menu");
            return;
        }
    };
    let ring_size = ring.size().as_vec2();
    let fill_size = distance.size().as_vec2();
    let cursor_size = cursor.size().as_vec2();
    let fill_ratio = fill_size / ring_size;
    let cursor_ratio = cursor_size / ring_size;
    let sprites = Sprites {
        ready: images.add(ready),
        distance: images.add(distance),
        ready_ring: images.add(ring),
        distant_ring: images.add(distant_ring),
        base_size: fill_size,
    };
    let material = materials.add(WireFill {
        settings: Vec4::new(0.0, fill_size.x, REVEAL_CURVE.x, REVEAL_CURVE.y),
        texture: sprites.distance.clone(),
    });
    // The cursor is the arrow below the ring; NG is not used by this marker.
    info!(
        "wire HUD: five cached SB_FE sprites, ring {ring_size}, fill {fill_size}, cursor {cursor_size}"
    );
    commands
        .spawn((
            Marker,
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
            ImageNode::new(sprites.distant_ring.clone()),
            GlobalZIndex(20),
        ))
        .with_children(|p| {
            p.spawn((
                Fill,
                Node {
                    position_type: PositionType::Absolute,
                    left: percent((1.0 - fill_ratio.x) * 50.0),
                    top: percent((1.0 - fill_ratio.y) * 50.0),
                    width: percent(fill_ratio.x * 100.0),
                    height: percent(fill_ratio.y * 100.0),
                    ..default()
                },
                MaterialNode(material),
            ));
            p.spawn((
                Cursor,
                Node {
                    position_type: PositionType::Absolute,
                    left: percent((1.0 - cursor_ratio.x) * 50.0),
                    top: percent(90.0),
                    width: percent(cursor_ratio.x * 100.0),
                    height: percent(cursor_ratio.y * 100.0),
                    display: Display::None,
                    ..default()
                },
                ImageNode::new(images.add(cursor)),
            ));
        });
    commands.insert_resource(sprites);
}
