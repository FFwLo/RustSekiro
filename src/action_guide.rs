//! The action guide: Sekiro's context prompts at the bottom of the screen (the HKS
//! `act(3030, ACTION_GUIDE_*, button)` guides: ACTION_GUIDE_COVER_START / COVER_END /
//! HANG_START / HAMG_TO_STAND / HAMG_TO_FALL).
//!
//! The texts are the game's own: menu.msgbnd イベントテキスト.fmg 10010310 "<?kgiconKc@20?>: Hug
//! wall", 10010311 "...: Stop hugging wall", 10010320 "...: Ledge Hang", 10010321 "...: Climb",
//! 10010322 "<?kgiconKc@5?>: Drop down". `<?kgiconKc@N?>` is the key of DefaultKeyAssignParam00's
//! gameAction N: its keyboardKeyId is the DirectInput scan code + 69 (player.rs read_input), its
//! mouseKeyId 1 / 2 / 8 the right / left / middle button. Without the extracted texts (an
//! extract before 2026-10-17 left menu.msgbnd out) the English is built in.
//!
//! Look (the user's capture of the real game: "[E] : Ledge Hang" low at the bottom centre, no
//! band): the key is the game's own key-top sprite (01_common sb_keytop KG_Key_<key> /
//! KG_Mouse_*; `sekiro-extract hud-all` cuts them to extracted/menu_sprites), then " : " and the
//! label in the HUD's serif with a shadow. Without the sprites the key is drawn as a boxed letter.
//! `SHINOBI_GUIDE_SHOW=<ids>` forces guides on for look checks.
//! gap: the guide movie's (01_000_fe.gfx) exact position, size and fade timing.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::actor::Actor;
use crate::player::{Player, CAPSULE_HALF_HEIGHT};

/// イベントテキスト.fmg ids of the guides.
pub const HUG_WALL: u32 = 10010310;
pub const STOP_HUGGING: u32 = 10010311;
pub const LEDGE_HANG: u32 = 10010320;
pub const CLIMB: u32 = 10010321;
pub const DROP_DOWN: u32 = 10010322;
/// "Rest" (the Sculptor's Idol, idol.rs).
pub const IDOL_REST: u32 = 15000120;
/// "Pick Up Item" (an item on the ground, items.rs).
pub const PICK_UP: u32 = 10010103;
/// The swim's (swim.rs): "Dive" (Ctrl, gameAction 17), "Underwater Dash" (Shift, 5), "Swim Up"
/// (F, 19), "Swim Down" (Ctrl), and "Jump" (Space, 6) out at the surface.
pub const DIVE: u32 = 10010330;
pub const UNDERWATER_DASH: u32 = 10010340;
pub const SWIM_UP: u32 = 10010341;
pub const SWIM_DOWN: u32 = 10010342;
pub const JUMP: u32 = 10021010;
/// The game's event-text guide for an NPC conversation.
pub const TALK: u32 = 10010200;

/// The built-in English for each guide, when the FMG is not extracted.
const FALLBACK: &[(u32, &str)] = &[
    (TALK, "Talk"),
    (HUG_WALL, "<?kgiconKc@20?>: Hug wall"),
    (STOP_HUGGING, "<?kgiconKc@20?>: Stop hugging wall"),
    (LEDGE_HANG, "<?kgiconKc@20?>: Ledge Hang"),
    (CLIMB, "<?kgiconKc@20?>: Climb"),
    (DROP_DOWN, "<?kgiconKc@5?>: Drop down"),
    (IDOL_REST, "Rest"),
    (PICK_UP, "Pick Up Item"),
    (DIVE, "<?kgiconKc@17?>: Dive"),
    (UNDERWATER_DASH, "<?kgiconKc@5?>: Underwater Dash"),
    (SWIM_UP, "<?kgiconKc@19?>: Swim Up"),
    (SWIM_DOWN, "<?kgiconKc@17?>: Swim Down"),
    (JUMP, "<?kgiconKc@6?>: Jump"),
];

/// The game's message texts and key names.
#[derive(Resource, Default)]
pub struct GuideTexts {
    texts: HashMap<u32, String>,
    /// gameAction -> the PC default key's name.
    keys: HashMap<i64, String>,
}

impl GuideTexts {
    fn load() -> Self {
        let dir = crate::paths::extracted();
        let mut texts: HashMap<u32, String> = std::fs::read(dir.join("msg/engus/menu.msgbnd.d/イベントテキスト.fmg")).ok().map(|d| read_fmg(&d)).unwrap_or_default();
        for (id, t) in FALLBACK {
            texts.entry(*id).or_insert_with(|| t.to_string());
        }
        let mut keys = HashMap::new();
        if let Some(rows) = std::fs::read_to_string(dir.join("json/params/DefaultKeyAssignParam00.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|j| j["rows"].as_array().cloned()) {
            for r in rows {
                let action = r["gameAction"].as_i64().unwrap_or(-1);
                let kb = r["keyboardKeyId"].as_i64().unwrap_or(-1);
                let mouse = r["mouseKeyId"].as_i64().unwrap_or(0);
                let name = if kb >= 0 { dik_name(kb - 69).map(str::to_string) } else { mouse_name(mouse).map(str::to_string) };
                if let Some(n) = name {
                    keys.entry(action).or_insert(n);
                }
            }
        }
        // The two the guides use, if the param is not exported.
        keys.entry(20).or_insert_with(|| "E".into());
        keys.entry(5).or_insert_with(|| "Shift".into());
        keys.entry(6).or_insert_with(|| "Space".into());
        keys.entry(17).or_insert_with(|| "Ctrl".into());
        keys.entry(19).or_insert_with(|| "F".into());
        GuideTexts { texts, keys }
    }

    /// A guide as (key, label): "<?kgiconKc@20?>: Hug wall" -> ("E", "Hug wall").
    pub fn guide(&self, id: u32) -> Option<(String, String)> {
        let t = self.texts.get(&id)?;
        // A plain menu word ("Rest", the idol's): the interact key's.
        let Some(rest) = t.strip_prefix("<?kgiconKc@") else {
            return Some((self.keys.get(&20).cloned().unwrap_or_else(|| "E".into()), t.clone()));
        };
        let (n, label) = rest.split_once("?>")?;
        let key = self.keys.get(&n.parse::<i64>().ok()?).cloned().unwrap_or_else(|| "?".into());
        Some((key, label.trim_start_matches(':').trim().to_string()))
    }
}

/// An FMG (Sekiro: version 2, little endian, UTF-16 strings): id -> text.
pub fn read_fmg(d: &[u8]) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let i32_ = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let i64_ = |o: usize| d.get(o..o + 8).map(|b| i64::from_le_bytes(b.try_into().unwrap()));
    let (Some(groups), Some(offsets)) = (i32_(0x0C), i64_(0x18)) else { return out };
    for g in 0..groups.max(0) as usize {
        let at = 0x28 + g * 16;
        let (Some(index), Some(first), Some(last)) = (i32_(at), i32_(at + 4), i32_(at + 8)) else { break };
        for k in 0..=(last - first).max(-1) {
            let Some(o) = i64_(offsets as usize + (index + k) as usize * 8) else { break };
            if o <= 0 {
                continue;
            }
            let mut e = o as usize;
            let mut units = Vec::new();
            while let Some(b) = d.get(e..e + 2) {
                let u = u16::from_le_bytes([b[0], b[1]]);
                if u == 0 {
                    break;
                }
                units.push(u);
                e += 2;
            }
            out.insert((first + k) as u32, String::from_utf16_lossy(&units));
        }
    }
    out
}

/// DirectInput scan code -> key name (the keys Sekiro's defaults use, and their neighbours).
fn dik_name(code: i64) -> Option<&'static str> {
    Some(match code {
        1 => "Esc",
        15 => "Tab",
        16 => "Q",
        17 => "W",
        18 => "E",
        19 => "R",
        20 => "T",
        21 => "Y",
        22 => "U",
        23 => "I",
        24 => "O",
        25 => "P",
        28 => "Enter",
        29 => "Ctrl",
        30 => "A",
        31 => "S",
        32 => "D",
        33 => "F",
        34 => "G",
        35 => "H",
        36 => "J",
        37 => "K",
        38 => "L",
        42 => "Shift",
        44 => "Z",
        45 => "X",
        46 => "C",
        47 => "V",
        48 => "B",
        49 => "N",
        50 => "M",
        56 => "Alt",
        57 => "Space",
        _ => return None,
    })
}

/// The key-top sprite of a key name (01_common sb_keytop), under the extracted root.
pub fn key_sprite(name: &str) -> String {
    let file = match name {
        "LMB" => "KG_Mouse_Click_L".to_string(),
        "RMB" => "KG_Mouse_Click_R".to_string(),
        "MMB" => "KG_Mouse_Wheel_Click".to_string(),
        n => format!("KG_Key_{n}"),
    };
    format!("menu_sprites/sb_keytop/{file}.png")
}

/// DefaultKeyAssignParam mouseKeyId: 1 right, 2 left (attack row 16 = 2, guard row 18 = 1, the
/// menu's enter 33 = 2), 8 middle (lock-on row 12).
fn mouse_name(id: i64) -> Option<&'static str> {
    Some(match id {
        1 => "RMB",
        2 => "LMB",
        8 => "MMB",
        _ => return None,
    })
}

/// The guides for Wolf's situation now (docking.rs edges and states).
pub fn guides_for(p: &Player, a: &Actor, feet: Vec3, edges: &[crate::map::LedgeEdge]) -> Vec<u32> {
    use crate::docking as dk;
    if a.hp <= 0.0 {
        return Vec::new();
    }
    // Swimming (swim.rs): at the surface Dive (with the Mibu Breathing Technique) and Jump, under
    // water Swim Up / Swim Down / Underwater Dash.
    if let Some(swim) = p.swim.as_ref() {
        return match (swim.diving, swim.mibu) {
            (true, _) => vec![SWIM_UP, SWIM_DOWN, UNDERWATER_DASH],
            (false, true) => vec![DIVE, JUMP],
            (false, false) => vec![JUMP],
        };
    }
    if let Some(dock) = p.dock.as_ref() {
        let held = dk::is_docked_state(&a.state) && !dk::is_start(&a.state) && !dk::is_jump_ready(&a.state);
        return match (held, dock.hug()) {
            (false, _) => Vec::new(),
            (true, true) => vec![STOP_HUGGING],
            (true, false) => vec![CLIMB, DROP_DOWN],
        };
    }
    if !a.airborne && crate::player::is_free(&a.state) {
        if crate::items::pickup_near(feet).is_some() {
            return vec![PICK_UP];
        }
        if crate::sculptor::near(feet) {
            return vec![TALK];
        }
        if crate::idol::near(feet, a.yaw).is_some() {
            return vec![IDOL_REST];
        }
        if dk::find_hug(edges, feet, a.yaw).is_some() {
            return vec![HUG_WALL];
        }
        if dk::find_hang_top(edges, feet, a.yaw).is_some() {
            return vec![LEDGE_HANG];
        }
    } else if a.airborne && dk::find_hang_air(edges, feet, a.yaw).is_some() {
        return vec![LEDGE_HANG];
    }
    Vec::new()
}

#[derive(Component)]
struct GuideRoot;

#[derive(Component)]
struct GuideRow(usize);

/// The key: its sprite, or (without one) a boxed letter.
#[derive(Component)]
struct GuideKeyIcon(usize);

#[derive(Component)]
struct GuideKeyBox(usize);

#[derive(Component)]
struct GuideKey(usize);

#[derive(Component)]
struct GuideLabel(usize);

const ROWS: usize = 3;
/// The key icon's height (px): about the label's cap height again, as in the capture.
const KEY_PX: f32 = 22.0;

fn spawn_guides(mut commands: Commands, fonts: Res<crate::fonts::GameFonts>) {
    let serif = fonts.ui.clone();
    let font = |size: f32| TextFont { font: serif.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(size), ..default() };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Percent(5.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
            GuideRoot,
        ))
        .with_children(|c| {
            for i in 0..ROWS {
                c.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(2.0), ..default() }, Visibility::Hidden, GuideRow(i))).with_children(|r| {
                    r.spawn((Node { height: Val::Px(KEY_PX), ..default() }, ImageNode::default(), Visibility::Hidden, GuideKeyIcon(i)));
                    r.spawn((
                        Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)), border: UiRect::all(Val::Px(1.0)), min_width: Val::Px(KEY_PX), justify_content: JustifyContent::Center, ..default() },
                        BorderColor::all(Color::srgba(0.85, 0.8, 0.7, 0.9)),
                        BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.85)),
                        Visibility::Hidden,
                        GuideKeyBox(i),
                    ))
                    .with_children(|k| {
                        k.spawn((Text::new(""), font(15.0), TextColor(Color::WHITE), GuideKey(i)));
                    });
                    r.spawn((Text::new(""), font(16.5), TextColor(Color::srgba(0.9, 0.87, 0.8, 1.0)), TextShadow { offset: Vec2::new(1.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.9) }, GuideLabel(i)));
                });
            }
        });
}

#[allow(clippy::type_complexity)]
fn update_guides(
    texts: Res<GuideTexts>,
    assets: Res<AssetServer>,
    ledges: Option<Res<crate::map::LedgeEdges>>,
    screen: Option<Res<State<crate::menu::Screen>>>,
    wolf: Query<(&Player, &Actor, &Transform)>,
    mut rows: Query<(&GuideRow, &mut Visibility), (Without<GuideKeyIcon>, Without<GuideKeyBox>)>,
    mut icons: Query<(&GuideKeyIcon, &mut ImageNode, &mut Node, &mut Visibility), (Without<GuideRow>, Without<GuideKeyBox>)>,
    mut boxes: Query<(&GuideKeyBox, &mut Node, &mut Visibility), (Without<GuideRow>, Without<GuideKeyIcon>)>,
    mut keys: Query<(&GuideKey, &mut Text), Without<GuideLabel>>,
    mut labels: Query<(&GuideLabel, &mut Text), Without<GuideKey>>,
) {
    let playing = screen.is_none_or(|s| *s.get() == crate::menu::Screen::Playing);
    let edges: &[crate::map::LedgeEdge] = ledges.as_deref().map(|l| l.0.as_slice()).unwrap_or(&[]);
    let shown: Vec<(String, String)> = match (playing, wolf.single()) {
        (true, Ok((p, a, tf))) => guides_for(p, a, tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT, edges).into_iter().filter_map(|id| texts.guide(id)).collect(),
        _ => Vec::new(),
    };
    // `SHINOBI_GUIDE_SHOW=<fmg id>[,...]`: those guides always (look checks with SHINOBI_PHOTO).
    let shown = match std::env::var("SHINOBI_GUIDE_SHOW") {
        Ok(v) => v.split(',').filter_map(|x| x.trim().parse::<u32>().ok()).filter_map(|id| texts.guide(id)).collect(),
        Err(_) => shown,
    };
    let root = crate::paths::extracted();
    let sprite_of = |key: &str| Some(key_sprite(key)).filter(|f| root.join(f).exists());
    for (row, mut vis) in &mut rows {
        *vis = if row.0 < shown.len() { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (i, mut img, mut node, mut vis) in &mut icons {
        match shown.get(i.0).and_then(|(k, _)| sprite_of(k)) {
            Some(f) => {
                // The node as wide as the sprite's aspect at KEY_PX high (the PNG header's size).
                let aspect = std::fs::read(root.join(&f)).ok().filter(|d| d.len() >= 24).map(|d| u32::from_be_bytes(d[16..20].try_into().unwrap()) as f32 / u32::from_be_bytes(d[20..24].try_into().unwrap()).max(1) as f32).unwrap_or(1.0);
                let w = Val::Px(KEY_PX * aspect);
                if node.width != w {
                    node.width = w;
                }
                let h = assets.load(f);
                if img.image != h {
                    img.image = h;
                }
                *vis = Visibility::Inherited;
            }
            None => {
                *vis = Visibility::Hidden;
                if node.display != Display::None {
                    node.display = Display::None;
                }
                continue;
            }
        }
        if node.display != Display::Flex {
            node.display = Display::Flex;
        }
    }
    // Out of the layout when unused (a hidden node keeps its space).
    for (b, mut node, mut vis) in &mut boxes {
        let used = shown.get(b.0).is_some_and(|(k, _)| sprite_of(k).is_none());
        *vis = if used { Visibility::Inherited } else { Visibility::Hidden };
        let display = if used { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    for (k, mut t) in &mut keys {
        if let Some((key, _)) = shown.get(k.0) {
            if t.0 != *key {
                t.0 = key.clone();
            }
        }
    }
    for (l, mut t) in &mut labels {
        if let Some((_, label)) = shown.get(l.0) {
            let want = format!(" : {label}");
            if t.0 != want {
                t.0 = want;
            }
        }
    }
}

pub struct ActionGuidePlugin;

impl Plugin for ActionGuidePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GuideTexts::load()).add_systems(Startup, spawn_guides).add_systems(Update, update_guides);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn guides_read_the_games_text_and_keys() {
        let g = super::GuideTexts::load();
        assert_eq!(g.guide(super::HUG_WALL), Some(("E".to_string(), "Hug wall".to_string())));
        assert_eq!(g.guide(super::DROP_DOWN), Some(("Shift".to_string(), "Drop down".to_string())));
        assert_eq!(g.guide(super::CLIMB).map(|x| x.1), Some("Climb".to_string()));
    }
}
