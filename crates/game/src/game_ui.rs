//! The game's own look for every screen: its fonts, texts and menu art, read from the unpacked
//! install in `cache/raw` when the app starts (nothing is extracted ahead and nothing of the
//! game is committed).
//! - Font: `font/engus_std/font.gfx` (FOT-Matisse ProN, the menus, HUD and prompts) converted
//!   to TrueType (`sekiro_formats::gfx_font`), else a Windows font, else Bevy's.
//! - Texts: the menu FMG banks (`msg/engus/menu.msgbnd.d/*.fmg`).
//! - Menu art: a sprite of the menu atlases (`menu/hi/01_common.tpf`, cut by the regions in
//!   `01_common.sblytbnd.d/<atlas>.layout`), drawn from the atlas's own DDS with the sprite's
//!   rectangle, so nothing is decoded on the CPU.
//!
//! After FwLo's RustSekiro `src/fonts.rs` and `tools/gfx_font.py` (the font and its fallbacks)
//! and its `export_hud` sprite cuts, here done at load time.
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use bevy::prelude::*;
use bevy::text::FontSource;

/// The game's fonts, converted once when the app is built.
#[derive(Resource, Default, Clone)]
pub struct GameFonts {
    pub ui: Option<Handle<Font>>,
}

impl GameFonts {
    /// Converts the fonts and inserts them, unless an earlier plugin already did; call after
    /// the text plugin is added and before `Startup`.
    pub fn insert(app: &mut App) {
        if app.world().contains_resource::<Self>() {
            return;
        }
        let bytes = font_bytes(
            &sekiro_sim::cache::root(),
            "engus_std",
            &["georgia.ttf", "segoeui.ttf"],
        );
        let ui = bytes.map(|b| {
            app.world_mut()
                .resource_mut::<Assets<Font>>()
                .add(Font::from_bytes(b))
        });
        app.insert_resource(Self { ui });
    }

    pub fn ui(&self, px: f32) -> TextFont {
        self.sized(bevy::text::FontSize::Px(px))
    }

    /// The UI font `px` high on the game's 1920 x 1080 screen, scaled with the window's height.
    pub fn stage(&self, px: f32) -> TextFont {
        self.sized(bevy::text::FontSize::Vh(px / 10.8))
    }

    /// Menu text at `stage` size with the game's soft drop shadow.
    pub fn shadowed(
        &self,
        text: impl Into<String>,
        px: f32,
        color: Color,
    ) -> (Text, TextFont, TextColor, TextShadow) {
        (
            Text::new(text),
            self.stage(px),
            TextColor(color),
            TextShadow {
                offset: Vec2::splat(1.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.8),
            },
        )
    }

    fn sized(&self, font_size: bevy::text::FontSize) -> TextFont {
        TextFont {
            font: self.ui.clone().map(FontSource::Handle).unwrap_or_default(),
            font_size,
            ..default()
        }
    }
}

/// `px` pixels of the game's 1920 x 1080 screen, scaled with the window's height.
pub fn stage(px: f32) -> Val {
    vh(px / 10.8)
}

/// The game's font `name` (`cache/raw/font/<name>/font.gfx`) as TrueType, else the first of
/// the Windows `fallbacks` found.
fn font_bytes(cache: &Path, name: &str, fallbacks: &[&str]) -> Option<Vec<u8>> {
    let game = std::fs::read(cache.join(format!("raw/font/{name}/font.gfx")))
        .ok()
        .and_then(|gfx| {
            sekiro_formats::gfx_font::to_sfnt(&gfx)
                .inspect_err(|e| warn!("game font {name}: {e}"))
                .ok()
        });
    game.or_else(|| {
        fallbacks
            .iter()
            .find_map(|f| std::fs::read(Path::new("C:/Windows/Fonts").join(f)).ok())
    })
}

/// A menu text bank (`イベントテキスト` and kin, `cache/raw/msg/engus/menu.msgbnd.d`), empty
/// when it is missing.
pub fn menu_texts(cache: &Path, bank: &str) -> BTreeMap<i32, String> {
    texts(cache, "menu", bank)
}

/// A text bank from `raw/msg/engus/{bnd}.msgbnd.d`, empty when missing.
pub fn texts(cache: &Path, bnd: &str, bank: &str) -> BTreeMap<i32, String> {
    let path = cache.join(format!("raw/msg/engus/{bnd}.msgbnd.d/{bank}.fmg"));
    std::fs::read(&path)
        .ok()
        .and_then(|b| {
            sekiro_formats::fmg::parse(&b)
                .inspect_err(|e| warn!("menu texts {bank}: {e}"))
                .ok()
        })
        .unwrap_or_default()
}

/// The menu atlases the screens draw from, loaded once when the app is built.
#[derive(Resource, Default)]
pub struct MenuArt(HashMap<&'static str, MenuAtlas>);

impl MenuArt {
    const ATLASES: [&'static str; 3] = ["SB_TextEffect", "SB_KeyTop", "SB_In_Game"];

    /// Loads the atlases and inserts them, unless an earlier screen already did; call after the
    /// asset plugins are added and before `Startup`.
    pub fn insert(app: &mut App) {
        Self::add(app, &Self::ATLASES);
    }

    /// `insert` with more atlases, in the same single read of the texture file; call before
    /// the screens that only `insert`.
    pub fn insert_with(app: &mut App, more: &[&'static str]) {
        Self::add(app, &[&Self::ATLASES[..], more].concat());
    }

    /// Adds only atlases not already loaded, reading their shared texture file once.
    pub fn add(app: &mut App, names: &[&'static str]) {
        app.init_resource::<Self>();
        let names: Vec<_> = names
            .iter()
            .copied()
            .filter(|n| !app.world().resource::<Self>().0.contains_key(n))
            .collect();
        if names.is_empty() {
            return;
        }
        let cache = sekiro_sim::cache::root();
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let atlases = MenuAtlas::load_many(&cache, &names, &mut images);
        let art = names
            .into_iter()
            .zip(atlases)
            .filter_map(|(name, atlas)| Some((name, atlas?)))
            .collect::<Vec<_>>();
        app.world_mut().resource_mut::<Self>().0.extend(art);
    }

    /// The sprite `name` of `atlas` (`SB_TextEffect`, `MENU_Death`) as a UI image, with its
    /// width over height.
    pub fn sprite(&self, atlas: &str, name: &str) -> Option<(ImageNode, f32)> {
        self.0.get(atlas)?.sprite(name)
    }

    /// The first atlas in order containing this sprite, with its width over height.
    pub fn find(&self, atlases: &[&str], name: &str) -> Option<(ImageNode, f32)> {
        atlases.iter().find_map(|a| self.sprite(a, name))
    }
}

/// A box at (left, top) of (width, height), all in percent of the screen.
pub fn abs(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: percent(left),
        top: percent(top),
        width: percent(width),
        height: percent(height),
        ..default()
    }
}

/// The colour of an action guide's text.
const GUIDE_TEXT: Color = Color::srgb(0.9, 0.87, 0.8);

/// An action guide low in the middle of the screen, "[E] : `label`" (the idol's "Rest", an
/// item's "Pick Up Item"), hidden, with `marker` on its root. Returns its label's text, for a
/// guide whose label changes ([`action_guide_label`]).
pub fn spawn_action_guide(
    commands: &mut Commands,
    fonts: &GameFonts,
    art: &MenuArt,
    label: &str,
    marker: impl Bundle,
) -> Entity {
    let text = |t: String| fonts.shadowed(t, 16.5, GUIDE_TEXT);
    let mut label_text = Entity::PLACEHOLDER;
    commands
        .spawn((
            marker,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                bottom: percent(5),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(2),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|r| {
            match art.sprite("SB_KeyTop", "KG_Key_E") {
                Some((image, aspect)) => r.spawn((
                    image,
                    Node {
                        height: stage(22.0),
                        aspect_ratio: Some(aspect),
                        ..default()
                    },
                )),
                None => r.spawn(text("E".into())),
            };
            label_text = r.spawn(text(action_guide_label(label))).id();
        });
    label_text
}

/// An action guide's label text after its key: " : `label`".
pub fn action_guide_label(label: &str) -> String {
    format!(" : {label}")
}

/// A screen root that shows over the world alone.
#[derive(Component)]
pub struct Covers;

/// A screen layer hidden under a covering screen, with the visibility it had.
#[derive(Component)]
pub struct Covered(Visibility);

#[derive(Resource)]
struct CoverInstalled;

/// Installs the shared cover once, before visibility propagates to UI children.
pub fn install_cover(app: &mut App) {
    if app.world().contains_resource::<CoverInstalled>() {
        return;
    }
    app.insert_resource(CoverInstalled).add_systems(
        PostUpdate,
        cover_layers.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
}

/// The menu shows over the world alone: every other screen layer (the HUD, the debug
/// text) is hidden while it is up and gets back the visibility its own systems set after.
/// After FwLo's RustSekiro `hide_hud_on_title`, from the game's capture.
#[allow(clippy::type_complexity)]
pub fn cover_layers(
    mut commands: Commands,
    covers: Query<&Visibility, (With<Node>, Without<ChildOf>, With<Covers>)>,
    mut layers: Query<
        (Entity, &mut Visibility, Option<&mut Covered>),
        (With<Node>, Without<ChildOf>, Without<Covers>),
    >,
) {
    let open = covers.iter().any(|v| *v != Visibility::Hidden);
    for (e, mut v, covered) in &mut layers {
        match (open, covered) {
            (true, Some(mut c)) => {
                if *v != Visibility::Hidden {
                    c.0 = *v;
                    *v = Visibility::Hidden;
                }
            }
            (true, None) => {
                commands.entity(e).insert(Covered(*v));
                *v = Visibility::Hidden;
            }
            (false, Some(c)) => {
                *v = c.0;
                commands.entity(e).remove::<Covered>();
            }
            (false, None) => {}
        }
    }
}

/// Sprites of one menu atlas (`SB_TextEffect`, `SB_Title`, ...): its texture and regions.
pub struct MenuAtlas {
    image: Handle<Image>,
    regions: HashMap<String, sekiro_formats::menu_layout::Region>,
}

impl MenuAtlas {
    /// The atlases `names` from `cache/raw/menu/hi/01_common`, in order, `None` (with a
    /// warning) for one that does not load; the 94 MB texture file is read once.
    pub fn load_many(
        cache: &Path,
        names: &[&str],
        images: &mut Assets<Image>,
    ) -> Vec<Option<Self>> {
        let dir = cache.join("raw/menu/hi");
        let tpf = std::fs::read(dir.join("01_common.tpf")).map_err(|e| e.to_string());
        let textures = tpf
            .as_deref()
            .map_err(Clone::clone)
            .and_then(|b| sekiro_formats::tpf::parse(b).map_err(|e| e.to_string()));
        names
            .iter()
            .map(|name| {
                let mut load = || -> Result<Self, String> {
                    let textures = textures.as_ref().map_err(Clone::clone)?;
                    let layout =
                        std::fs::read(dir.join(format!("01_common.sblytbnd.d/{name}.layout")))
                            .map_err(|e| e.to_string())?;
                    let atlas =
                        sekiro_formats::menu_layout::parse(&layout).map_err(|e| e.to_string())?;
                    let texture = textures
                        .iter()
                        .find(|t| format!("{}.png", t.name).eq_ignore_ascii_case(&atlas.image))
                        .ok_or_else(|| format!("no texture {}", atlas.image))?;
                    let image = Image::from_buffer(
                        texture.dds,
                        bevy::image::ImageType::Extension("dds"),
                        bevy::image::CompressedImageFormats::all(),
                        true,
                        bevy::image::ImageSampler::linear(),
                        bevy::asset::RenderAssetUsages::RENDER_WORLD,
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(Self {
                        image: images.add(image),
                        regions: atlas.regions,
                    })
                };
                load().inspect_err(|e| warn!("menu atlas {name}: {e}")).ok()
            })
            .collect()
    }

    /// The sprite `name` (`MENU_Death`) as a UI image, with its width over height.
    pub fn sprite(&self, name: &str) -> Option<(ImageNode, f32)> {
        let r = self.regions.get(&format!("{name}.png"))?;
        let (x, y, w, h) = (r.x as f32, r.y as f32, r.width as f32, r.height as f32);
        let mut node = ImageNode::new(self.image.clone());
        node.rect = Some(Rect::new(x, y, x + w, y + h));
        Some((node, w / h))
    }
}
