//! The HUD's places, sizes, text sizes and colours and clip timings, from the game's own HUD
//! movie (menu/01_000_fe.gfx, Scaleform). `sekiro-extract hud-layout <extracted>` (tools/
//! sekiro-extract/src/gfx.rs) reads the movie's timelines and writes every placed instance at
//! frame 1 to extracted/hud/layout/01_000_fe.json: its path of instance names (unnamed ones
//! `#<depth>`), world matrix and bounds on the 1920 x 1080 stage, the atlas sprite it draws
//! (DefineExternalImage2 names), its colour transform and, for text fields, the font height and
//! colour; and each sprite's frame labels (FadeIn / Loop / FadeOut) at the movie's 60 fps.
//!
//! [`HudLayout::load`] resolves the HUD's elements by those paths; when the file is missing (CI)
//! the defaults are the same numbers, as read from the movie on 2026-11-04 (the test
//! `the_hud_layout_defaults_are_the_fe_movie` keeps them equal when the data is there).
//!
//! gap: what the movie's ActionScript sets at run time is not in the timelines: the vitality
//! bar's length per maximum Vitality (the bar sprites are drawn at their longest), the posture
//! fill's half length (scaled by script; here the frame's width), the boss orbs' spacing past the
//! first, and the fill colours, which are several layers with blend modes (the single-layer tints
//! are kept in `vitality_cx` / `trail_cx`; the HUD keeps the user's captured colours).

use bevy::prelude::*;
use serde_json::Value;

/// A rectangle on the movie's 1920 x 1080 stage (px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

pub const STAGE_W: f32 = 1920.0;
pub const STAGE_H: f32 = 1080.0;

/// A length of stage px as Vw (the stage camera's viewport is the 16:9 stage, ui_stage.rs), for
/// nodes whose parent has no size to take a Percent of.
pub fn vw(px: f32) -> Val {
    Val::Vw(px / STAGE_W * 100.0)
}

impl StageRect {
    pub const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        StageRect { x0, y0, x1, y1 }
    }
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
    pub fn cx(&self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
    pub fn cy(&self) -> f32 {
        (self.y0 + self.y1) / 2.0
    }
    /// Its left edge as a share of the stage (the HUD's stage camera is 16:9, ui_stage.rs).
    pub fn left(&self) -> Val {
        Val::Percent(self.x0 / STAGE_W * 100.0)
    }
    pub fn top(&self) -> Val {
        Val::Percent(self.y0 / STAGE_H * 100.0)
    }
    pub fn width(&self) -> Val {
        Val::Percent(self.w() / STAGE_W * 100.0)
    }
    pub fn height(&self) -> Val {
        Val::Percent(self.h() / STAGE_H * 100.0)
    }
    /// An absolute node covering it.
    pub fn node(&self) -> Node {
        Node { position_type: PositionType::Absolute, left: self.left(), top: self.top(), width: self.width(), height: self.height(), ..default() }
    }
    pub(crate) fn union(a: Option<StageRect>, b: StageRect) -> StageRect {
        match a {
            None => b,
            Some(a) => StageRect::new(a.x0.min(b.x0), a.y0.min(b.y0), a.x1.max(b.x1), a.y1.max(b.y1)),
        }
    }
}

/// A text field of the movie: its box, font height (px on the stage) and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageText {
    pub rect: StageRect,
    pub px: f32,
    pub color: [u8; 4],
}

impl StageText {
    /// The font height as a share of the stage's height (FontSize::Vh).
    pub fn vh(&self) -> f32 {
        self.px / STAGE_H * 100.0
    }
    pub fn color(&self) -> Color {
        Color::srgba_u8(self.color[0], self.color[1], self.color[2], self.color[3])
    }
}

/// A clip's labels (frames at the movie's rate): FadeIn, Loop and FadeOut, and its length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub fade_in: u32,
    pub hold: u32,
    pub fade_out: u32,
    pub frames: u32,
}

impl Clip {
    /// Seconds from FadeIn until FadeOut starts (a clip that plays through its hold by itself).
    pub fn shown_secs(&self, fps: f32) -> f32 {
        self.fade_out.saturating_sub(self.fade_in) as f32 / fps
    }
}

/// The colour transform of a layer: rgba multipliers and additions (0-255).
pub type ColorX = ([f32; 4], [f32; 4]);

/// Every HUD element the HUD lays out from the FE movie.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct HudLayout {
    pub fps: f32,
    /// Wolf's vitality fill (MENU_HP_bar_L under PlayerHUD/Gauge/HP/.../Current): its left edge
    /// and its top and bottom; its right edge is the longest bar (script sets the length).
    pub hp_fill: StageRect,
    /// Resurrection node 0's frame (MENU_Revive_Window_2 in ResurrectionList/Icon_0) and the
    /// step to the next node (Icon_1).
    pub res_window: StageRect,
    pub res_pitch: f32,
    /// Wolf's posture: the fill's centre (PlayerHUD/Gauge/SP's bar origin) and height
    /// (MENU_Taikan_bar), the frame behind it (SPFrame State_0 MENU_Taikan_base_03).
    pub posture_cx: f32,
    pub posture_fill: StageRect,
    pub posture_base: StageRect,
    /// The poison / burn build-up icon (ConditionGaugeList Item_0_0 MENU_Resist_Poison) and the
    /// step to the next gauge (Item_1_0, upward).
    pub build_up_icon: StageRect,
    pub build_up_pitch: f32,
    /// The item panel: the quick item's box (Item_Bottom MENU_Item_Top) and icon (IconImage), its
    /// stock count; the prosthetic tool's box (Item_Left MENU_Item_Left) and icon; the combat
    /// art's mark (Item_Top MENU_FE_Skill), the ninjutsu's (Item_Right MENU_FE_Ninsatul).
    pub quick_box: StageRect,
    pub quick_icon: StageRect,
    pub quick_text: StageText,
    pub tool_box: StageRect,
    pub tool_icon: StageRect,
    pub art_mark: StageRect,
    pub ninjutsu_mark: StageRect,
    /// The Spirit Emblems (PlayerCount Soul State_0): backing MENU_HP_base_4, icon, count.
    pub emblem_back: StageRect,
    pub emblem_icon: StageRect,
    pub emblem_text: StageText,
    /// The Sen purse (Money Layout): backing MENU_Caption_02, MENU_Money, the total, the gain.
    pub money_back: StageRect,
    pub money_icon: StageRect,
    pub money_text: StageText,
    pub money_gain: StageText,
    /// The skill gauge (SkillPoint Layout): MENU_SkillPoint, the points, MENU_ME_Caption behind
    /// the experience bar (MENU_HP_bar from its left edge to the MENU_HP_bar_mark cap), the gain.
    pub skill_icon: StageRect,
    pub skill_text: StageText,
    pub skill_back: StageRect,
    pub skill_bar: StageRect,
    pub skill_gain: StageText,
    /// The boss (BossList Item_0_0): vitality (HPGauge MENU_HP_bar_Boss), the first deathblow orb
    /// lit (HP Icon_0 MENU_HP_Boss) and spent (Base_0 MENU_HP_Boss_base), the name, the posture
    /// frame (SPFrame State_0), fill height (SP Bar0) and the State_1 glow (MENU_Taikan_ST1).
    pub boss_hp: StageRect,
    pub boss_orb: StageRect,
    pub boss_orb_spent: StageRect,
    pub boss_name: StageText,
    pub boss_posture_base: StageRect,
    pub boss_posture_fill: StageRect,
    pub boss_glow: StageRect,
    /// The over-head bars (EnemyTag0): the vitality frame (MENU_HP_bar_Enemy_base) and the
    /// posture frame (SPFrame State_0).
    pub enemy_hp_base: StageRect,
    pub enemy_posture_base: StageRect,
    /// The single-layer tints of the vitality fill (Current) and its trail (Delay) on the white
    /// MENU_HP_bar_L.
    pub vitality_cx: ColorX,
    pub trail_cx: ColorX,
    /// Clips: the vitality gauge (Timeline_218), the posture gauge (Timeline_268), the purse
    /// (Money_164, which plays through its hold and fades by itself), the skill gauge
    /// (SkillPoint_282), the lock-on mark (Lockon_Ani_297).
    pub hp_clip: Clip,
    pub posture_clip: Clip,
    pub money_clip: Clip,
    pub skill_clip: Clip,
    pub lock_clip: Clip,
}

const TEXT: [u8; 4] = [204, 204, 204, 255];
const fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> StageRect {
    StageRect::new(x0, y0, x1, y1)
}
const fn t(x0: f32, y0: f32, x1: f32, y1: f32, px: f32, color: [u8; 4]) -> StageText {
    StageText { rect: r(x0, y0, x1, y1), px, color }
}
const fn clip(fade_in: u32, hold: u32, fade_out: u32, frames: u32) -> Clip {
    Clip { fade_in, hold, fade_out, frames }
}

impl Default for HudLayout {
    /// menu/01_000_fe.gfx as read on 2026-11-04 (hud-layout).
    fn default() -> Self {
        HudLayout {
            fps: 60.0,
            hp_fill: r(111.8, 976.0, 1599.5, 989.2),
            res_window: r(103.3, 910.9, 154.3, 963.9),
            res_pitch: 46.8,
            posture_cx: 960.0,
            posture_fill: r(889.8, 926.3, 1031.7, 947.1),
            posture_base: r(621.5, 920.3, 1285.4, 953.3),
            build_up_icon: r(767.8, 733.1, 818.8, 800.1),
            build_up_pitch: -54.4,
            quick_box: r(1503.9, 935.7, 1600.4, 1025.5),
            quick_icon: r(1512.0, 941.3, 1589.1, 1018.4),
            quick_text: t(1531.8, 987.6, 1593.8, 1026.6, 24.0, TEXT),
            tool_box: r(1595.3, 874.8, 1743.0, 1031.3),
            tool_icon: r(1607.4, 900.2, 1728.3, 1021.2),
            art_mark: r(1744.6, 897.1, 1784.9, 937.4),
            ninjutsu_mark: r(1744.8, 939.9, 1785.2, 980.2),
            emblem_back: r(1740.7, 983.7, 1848.1, 1021.9),
            emblem_icon: r(1752.0, 986.0, 1783.8, 1017.8),
            emblem_text: t(1781.2, 990.6, 1817.8, 1029.6, 24.0, TEXT),
            money_back: r(1635.9, 728.7, 1921.9, 770.6),
            money_icon: r(1640.9, 733.4, 1673.6, 766.8),
            money_text: t(1673.9, 732.5, 1818.9, 768.5, 24.0, TEXT),
            money_gain: t(1659.2, 769.5, 1819.2, 805.5, 24.0, TEXT),
            skill_icon: r(1427.2, 66.5, 1497.8, 113.5),
            skill_text: t(1434.4, 74.8, 1494.0, 110.4, 22.0, [115, 213, 244, 255]),
            skill_back: r(1473.3, 78.6, 1788.5, 97.9),
            skill_bar: r(1492.4, 84.4, 1755.6, 92.4),
            skill_gain: t(1658.4, 97.8, 1756.4, 136.8, 18.0, [190, 244, 252, 255]),
            boss_hp: r(112.3, 90.9, 523.1, 108.8),
            boss_orb: r(106.1, 44.8, 156.6, 95.1),
            boss_orb_spent: r(111.0, 49.9, 152.3, 91.3),
            boss_name: t(121.4, 116.1, 921.4, 155.1, 25.0, TEXT),
            boss_posture_base: r(573.6, 51.0, 1337.0, 84.0),
            boss_posture_fill: r(869.1, 58.1, 960.1, 78.7),
            boss_glow: r(558.8, 49.2, 1363.8, 83.8),
            enemy_hp_base: r(637.0, 119.1, 770.0, 130.1),
            enemy_posture_base: r(628.4, 129.0, 788.6, 142.2),
            vitality_cx: ([0.602, 0.191, 0.148, 1.0], [-26.0, 12.0, 9.0, 0.0]),
            trail_cx: ([0.5, 0.5, 0.52, 1.0], [0.0, 0.0, 0.0, 0.0]),
            hp_clip: clip(1, 16, 18, 34),
            posture_clip: clip(1, 16, 17, 32),
            money_clip: clip(1, 15, 138, 154),
            skill_clip: clip(1, 15, 17, 33),
            lock_clip: clip(1, 16, 84, 90),
        }
    }
}

/// The movie's instances (path -> entry) with lookups (also menu_layout.rs's).
pub(crate) struct Movie<'a> {
    inst: &'a serde_json::Map<String, Value>,
    sprites: Option<&'a serde_json::Map<String, Value>>,
}

fn rect_of(v: &Value) -> Option<StageRect> {
    let b = v.get("bounds")?.as_array()?;
    let f = |i: usize| b.get(i).and_then(Value::as_f64).map(|x| x as f32);
    Some(StageRect::new(f(0)?, f(1)?, f(2)?, f(3)?))
}

impl<'a> Movie<'a> {
    /// A hud-layout file's instances.
    pub(crate) fn of(v: &'a Value) -> Option<Self> {
        Some(Movie { inst: v["instances"].as_object()?, sprites: v["sprites"].as_object() })
    }
    /// An instance's bounds on the stage.
    pub(crate) fn rect(&self, path: &str) -> Option<StageRect> {
        rect_of(self.inst.get(path)?)
    }
    /// The leaves under `prefix` that draw atlas sprite `image`.
    pub(crate) fn leaves(&self, prefix: &str, image: &str) -> Vec<StageRect> {
        self.inst
            .iter()
            .filter(|(k, _)| *k == prefix || k.starts_with(&format!("{prefix}/")))
            .filter(|(_, v)| v["images"].as_array().is_some_and(|a| a.iter().any(|s| s.as_str() == Some(image))))
            .filter_map(|(_, v)| rect_of(v))
            .collect()
    }
    /// Their union.
    pub(crate) fn image(&self, prefix: &str, image: &str) -> Option<StageRect> {
        self.leaves(prefix, image).into_iter().fold(None, |o, b| Some(StageRect::union(o, b)))
    }
    pub(crate) fn text(&self, path: &str) -> Option<StageText> {
        let v = self.inst.get(path)?;
        let tx = v.get("text")?;
        let c = tx["color"].as_array().map(|a| std::array::from_fn(|i| a.get(i).and_then(Value::as_u64).unwrap_or(255) as u8)).unwrap_or(TEXT);
        Some(StageText { rect: rect_of(v)?, px: tx["height"].as_f64()? as f32, color: c })
    }
    /// The world origin (translation) of an instance.
    fn origin(&self, path: &str) -> Option<(f32, f32)> {
        let m = self.inst.get(path)?.get("m")?.as_array()?;
        Some((m.get(4)?.as_f64()? as f32, m.get(5)?.as_f64()? as f32))
    }
    fn cx(&self, path: &str) -> Option<ColorX> {
        let c = self.inst.get(path)?.get("cx")?;
        let arr = |k: &str| -> Option<[f32; 4]> {
            let a = c.get(k)?.as_array()?;
            Some(std::array::from_fn(|i| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32))
        };
        Some((arr("mul")?, arr("add")?))
    }
    fn clip(&self, class: &str) -> Option<Clip> {
        let s = self.sprites?.iter().find(|(k, _)| k.ends_with(&format!(".{class}")) || *k == class)?.1;
        let l = &s["labels"];
        let f = |n: &str| l.get(n).and_then(Value::as_u64).map(|x| x as u32);
        Some(Clip { fade_in: f("FadeIn")?, hold: f("Loop")?, fade_out: f("FadeOut")?, frames: s["frames"].as_u64()? as u32 })
    }
}

impl HudLayout {
    /// extracted/hud/layout/01_000_fe.json, else the defaults.
    pub fn load() -> Self {
        let path = crate::paths::extracted().join("hud/layout/01_000_fe.json");
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).map_or_else(HudLayout::default, |v| HudLayout::from_json(&v))
    }

    /// The built-in numbers (systems run without the resource in some tests).
    pub fn defaults() -> &'static HudLayout {
        static D: std::sync::OnceLock<HudLayout> = std::sync::OnceLock::new();
        D.get_or_init(HudLayout::default)
    }

    /// Resolves every element in a hud-layout file; what is not found keeps its default.
    pub fn from_json(v: &Value) -> Self {
        let mut l = HudLayout::default();
        let Some(inst) = v["instances"].as_object() else { return l };
        let m = Movie { inst, sprites: v["sprites"].as_object() };
        if let Some(f) = v["fps"].as_f64() {
            l.fps = f as f32;
        }
        let set = |dst: &mut StageRect, src: Option<StageRect>| {
            if let Some(s) = src {
                *dst = s;
            }
        };
        let set_t = |dst: &mut StageText, src: Option<StageText>| {
            if let Some(s) = src {
                *dst = s;
            }
        };
        set(&mut l.hp_fill, m.image("PlayerHUD/Gauge/HP/Fade/Offset/Current", "MENU_HP_bar_L"));
        let res = "PlayerHUD/Gauge/Resurrection2/Fade/ResurrectionList";
        set(&mut l.res_window, m.image(&format!("{res}/Icon_0"), "MENU_Revive_Window_2"));
        if let (Some(a), Some(b)) = (m.origin(&format!("{res}/Icon_0")), m.origin(&format!("{res}/Icon_1"))) {
            l.res_pitch = b.0 - a.0;
        }
        if let Some((x, _)) = m.origin("PlayerHUD/Gauge/SP/Fade/Current/Bar") {
            l.posture_cx = x;
        }
        set(&mut l.posture_fill, m.image("PlayerHUD/Gauge/SP/Fade/Current", "MENU_Taikan_bar"));
        set(&mut l.posture_base, m.image("PlayerHUD/Gauge/SPFrame/Fade/State_0", "MENU_Taikan_base_03"));
        set(&mut l.build_up_icon, m.image("ConditionGaugeList/Item_0_0/Icon", "MENU_Resist_Poison"));
        if let (Some(a), Some(b)) = (m.image("ConditionGaugeList/Item_0_0/Icon", "MENU_Resist_Poison"), m.image("ConditionGaugeList/Item_1_0/Icon", "MENU_Resist_Poison")) {
            l.build_up_pitch = b.y0 - a.y0;
        }
        let ip = "PlayerHUD/ItemPanel";
        set(&mut l.quick_box, m.image(&format!("{ip}/Item_Bottom/Fade/Item"), "MENU_Item_Top"));
        set(&mut l.quick_icon, m.image(&format!("{ip}/Item_Bottom/Fade/Item/ItemIcon/IconImage/State_1"), "MENU_ItemIcon_01000"));
        set_t(&mut l.quick_text, m.text(&format!("{ip}/Item_Bottom/Fade/Text/Text/Stock")));
        set(&mut l.tool_box, m.image(&format!("{ip}/Item_Left/Fade/Item"), "MENU_Item_Left"));
        set(&mut l.tool_icon, m.image(&format!("{ip}/Item_Left/Fade/Item/ItemIcon/IconImage/State_1"), "MENU_ItemIcon_01000"));
        set(&mut l.art_mark, m.image(&format!("{ip}/Item_Top/Fade/Item"), "MENU_FE_Skill"));
        set(&mut l.ninjutsu_mark, m.image(&format!("{ip}/Item_Right/Fade/Item"), "MENU_FE_Ninsatul"));
        let soul = "PlayerCount/Soul/State_0";
        set(&mut l.emblem_back, m.image(soul, "MENU_HP_base_4"));
        set(&mut l.emblem_icon, m.image(&format!("{soul}/Icon"), "MENU_ItemIcon_00001"));
        set_t(&mut l.emblem_text, m.text(&format!("{soul}/Text/Text")));
        set(&mut l.money_back, m.image("Money/Layout", "MENU_Caption_02"));
        set(&mut l.money_icon, m.image("Money/Layout", "MENU_Money"));
        set_t(&mut l.money_text, m.text("Money/Layout/TotalSoul/LostSoul/Text"));
        set_t(&mut l.money_gain, m.text("Money/Layout/GetSoul/Text"));
        let sk = "SkillPoint/Layout";
        set(&mut l.skill_icon, m.image(&format!("{sk}/Gauge/Current"), "MENU_SkillPoint"));
        set_t(&mut l.skill_text, m.text(&format!("{sk}/SkillPoint/SkillPoint/Text")));
        set(&mut l.skill_back, m.image(&format!("{sk}/Gauge/Current"), "MENU_ME_Caption"));
        // The bar runs to its right-hand cap's middle.
        let cap = m.leaves(&format!("{sk}/Gauge/Current"), "MENU_HP_bar_mark").into_iter().max_by(|a, b| a.x0.total_cmp(&b.x0));
        if let (Some(bar), Some(cap)) = (m.image(&format!("{sk}/Gauge/Current"), "MENU_HP_bar"), cap) {
            l.skill_bar = StageRect::new(bar.x0, bar.y0, cap.cx(), bar.y1);
        }
        set_t(&mut l.skill_gain, m.text(&format!("{sk}/AddExpPoint/AddExpPoint/Text")));
        let b = "BossList/Item_0_0";
        set(&mut l.boss_hp, m.image(&format!("{b}/HPGauge"), "MENU_HP_bar_Boss"));
        set(&mut l.boss_orb, m.image(&format!("{b}/HP/Icon_0"), "MENU_HP_Boss"));
        set(&mut l.boss_orb_spent, m.image(&format!("{b}/HP/Base_0"), "MENU_HP_Boss_base"));
        set_t(&mut l.boss_name, m.text(&format!("{b}/Name/Text")));
        set(&mut l.boss_posture_base, m.image(&format!("{b}/SPFrame/State_0"), "MENU_Taikan_base_03"));
        set(&mut l.boss_posture_fill, m.image(&format!("{b}/SP/Current/Bar0"), "MENU_Taikan_bar"));
        set(&mut l.boss_glow, m.image(&format!("{b}/SPFrame/State_1"), "MENU_Taikan_ST1"));
        set(&mut l.enemy_hp_base, m.image("EnemyTag0/#1", "MENU_HP_bar_Enemy_base"));
        set(&mut l.enemy_posture_base, m.image("EnemyTag0/SPFrame/State_0", "MENU_Taikan_base_03"));
        let hp = "PlayerHUD/Gauge/HP/Fade/Offset";
        if let Some(c) = m.cx(&format!("{hp}/Current/Bar/#18/#13")) {
            l.vitality_cx = c;
        }
        if let Some(c) = m.cx(&format!("{hp}/Delay/Bar/#3/#13")) {
            l.trail_cx = c;
        }
        for (dst, class) in [(&mut l.hp_clip, "Timeline_218"), (&mut l.posture_clip, "Timeline_268"), (&mut l.money_clip, "Money_164"), (&mut l.skill_clip, "SkillPoint_282"), (&mut l.lock_clip, "Lockon_Ani_297")] {
            if let Some(c) = m.clip(class) {
                *dst = c;
            }
        }
        l
    }

    /// Wolf's posture bar: centred on the gauge's origin, as wide as its frame, as tall as the fill.
    pub fn posture_bar(&self) -> StageRect {
        let half = self.posture_base.w() / 2.0;
        StageRect::new(self.posture_cx - half, self.posture_fill.y0, self.posture_cx + half, self.posture_fill.y1)
    }
    /// The boss's posture bar: centred on its frame, as wide as the frame, as tall as the fill.
    pub fn boss_posture_bar(&self) -> StageRect {
        let b = &self.boss_posture_base;
        StageRect::new(b.x0, self.boss_posture_fill.y0, b.x1, self.boss_posture_fill.y1)
    }
    /// How long the purse stays up after a change: the Money clip's FadeIn to FadeOut.
    pub fn flash_secs(&self) -> f32 {
        self.money_clip.shown_secs(self.fps)
    }
}

/// A colour transform applied to a texel (0-255 each), as Scaleform does: c * mul + add.
#[cfg(test)]
pub fn apply_cx(cx: &ColorX, texel: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|i| (texel[i] * cx.0[i] + cx.1[i]).clamp(0.0, 255.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.06
    }
    fn close_r(a: &StageRect, b: &StageRect) -> bool {
        close(a.x0, b.x0) && close(a.y0, b.y0) && close(a.x1, b.x1) && close(a.y1, b.y1)
    }

    /// A stage rectangle is placed as shares of the 1920 x 1080 stage, a length as Vw of it,
    /// a font height as Vh.
    #[test]
    fn stage_rects_place_as_shares_of_the_stage() {
        let r = StageRect::new(96.0, 54.0, 288.0, 108.0);
        assert_eq!(r.left(), Val::Percent(5.0));
        assert_eq!(r.top(), Val::Percent(5.0));
        assert_eq!(r.width(), Val::Percent(10.0));
        assert_eq!(r.height(), Val::Percent(5.0));
        assert_eq!(vw(192.0), Val::Vw(10.0));
        let t = StageText { rect: r, px: 27.0, color: TEXT };
        assert!((t.vh() - 2.5).abs() < 1e-5);
    }

    /// The resolver reads a hud-layout file: the union of the leaves that draw a sprite, the step
    /// between two nodes' origins, a text field's height and colour, a clip's labels.
    #[test]
    fn the_hud_layout_reads_a_movie_layout() {
        let res = "PlayerHUD/Gauge/Resurrection2/Fade/ResurrectionList";
        let v = serde_json::json!({
            "fps": 30.0,
            "instances": {
                format!("{res}/Icon_0"): {"m": [1, 0, 0, 1, 100.0, 900.0]},
                format!("{res}/Icon_0/icon/#1"): {"bounds": [80.0, 880.0, 130.0, 930.0], "images": ["MENU_Revive_Window_2"]},
                format!("{res}/Icon_1"): {"m": [1, 0, 0, 1, 150.0, 900.0]},
                "PlayerHUD/Gauge/HP/Fade/Offset/Current/Bar/#18/#13": {"bounds": [110.0, 970.0, 700.0, 990.0], "images": ["MENU_HP_bar_L"]},
                "PlayerHUD/Gauge/HP/Fade/Offset/Current/Bar/#18/#14": {"bounds": [700.0, 970.0, 1500.0, 990.0], "images": ["MENU_HP_bar_L"]},
                "BossList/Item_0_0/Name/Text": {"bounds": [1.0, 2.0, 3.0, 4.0], "text": {"height": 30.0, "color": [1, 2, 3, 255]}},
            },
            "sprites": {"_x_fla.Money_164": {"frames": 100, "labels": {"FadeIn": 1, "Loop": 10, "FadeOut": 61}}},
        });
        let l = HudLayout::from_json(&v);
        assert_eq!(l.fps, 30.0);
        assert_eq!(l.res_window, StageRect::new(80.0, 880.0, 130.0, 930.0));
        assert_eq!(l.res_pitch, 50.0);
        assert_eq!(l.hp_fill, StageRect::new(110.0, 970.0, 1500.0, 990.0));
        assert_eq!(l.boss_name.px, 30.0);
        assert_eq!(l.boss_name.color, [1, 2, 3, 255]);
        assert_eq!(l.money_clip, Clip { fade_in: 1, hold: 10, fade_out: 61, frames: 100 });
        assert!((l.flash_secs() - 2.0).abs() < 1e-5);
        // Not in the file: the default.
        assert_eq!(l.tool_box, HudLayout::default().tool_box);
    }

    /// With the game's data: the defaults are what the FE movie says (so CI without data lays
    /// the HUD out the same), and the movie's own facts hold: the posture gauge on the stage's
    /// centre, the three resurrection nodes 46.8 px apart, the vitality fill white x tint.
    #[test]
    fn the_hud_layout_defaults_are_the_fe_movie() {
        let path = crate::paths::extracted().join("hud/layout/01_000_fe.json");
        let Ok(t) = std::fs::read_to_string(&path) else {
            eprintln!("skipped: no {} (sekiro-extract hud-layout)", path.display());
            return;
        };
        let l = HudLayout::from_json(&serde_json::from_str(&t).unwrap());
        let d = HudLayout::default();
        let mut off = vec![];
        for (name, a, b) in [
            ("hp_fill", l.hp_fill, d.hp_fill),
            ("res_window", l.res_window, d.res_window),
            ("posture_fill", l.posture_fill, d.posture_fill),
            ("posture_base", l.posture_base, d.posture_base),
            ("build_up_icon", l.build_up_icon, d.build_up_icon),
            ("quick_box", l.quick_box, d.quick_box),
            ("quick_icon", l.quick_icon, d.quick_icon),
            ("tool_box", l.tool_box, d.tool_box),
            ("tool_icon", l.tool_icon, d.tool_icon),
            ("art_mark", l.art_mark, d.art_mark),
            ("ninjutsu_mark", l.ninjutsu_mark, d.ninjutsu_mark),
            ("emblem_back", l.emblem_back, d.emblem_back),
            ("emblem_icon", l.emblem_icon, d.emblem_icon),
            ("money_back", l.money_back, d.money_back),
            ("money_icon", l.money_icon, d.money_icon),
            ("skill_icon", l.skill_icon, d.skill_icon),
            ("skill_back", l.skill_back, d.skill_back),
            ("skill_bar", l.skill_bar, d.skill_bar),
            ("boss_hp", l.boss_hp, d.boss_hp),
            ("boss_orb", l.boss_orb, d.boss_orb),
            ("boss_orb_spent", l.boss_orb_spent, d.boss_orb_spent),
            ("boss_posture_base", l.boss_posture_base, d.boss_posture_base),
            ("boss_posture_fill", l.boss_posture_fill, d.boss_posture_fill),
            ("boss_glow", l.boss_glow, d.boss_glow),
            ("enemy_hp_base", l.enemy_hp_base, d.enemy_hp_base),
            ("enemy_posture_base", l.enemy_posture_base, d.enemy_posture_base),
            ("quick_text", l.quick_text.rect, d.quick_text.rect),
            ("emblem_text", l.emblem_text.rect, d.emblem_text.rect),
            ("money_text", l.money_text.rect, d.money_text.rect),
            ("money_gain", l.money_gain.rect, d.money_gain.rect),
            ("skill_text", l.skill_text.rect, d.skill_text.rect),
            ("skill_gain", l.skill_gain.rect, d.skill_gain.rect),
            ("boss_name", l.boss_name.rect, d.boss_name.rect),
        ] {
            if !close_r(&a, &b) {
                off.push(format!("{name}: r({:.1}, {:.1}, {:.1}, {:.1})", a.x0, a.y0, a.x1, a.y1));
            }
        }
        assert!(off.is_empty(), "the defaults differ from the movie:\n{}", off.join("\n"));
        assert!(close(l.res_pitch, d.res_pitch) && close(l.build_up_pitch, d.build_up_pitch) && close(l.posture_cx, d.posture_cx));
        for (a, b) in [(l.quick_text, d.quick_text), (l.emblem_text, d.emblem_text), (l.money_text, d.money_text), (l.skill_text, d.skill_text), (l.skill_gain, d.skill_gain), (l.boss_name, d.boss_name)] {
            assert_eq!((a.px, a.color), (b.px, b.color));
        }
        assert_eq!((l.vitality_cx, l.trail_cx), (d.vitality_cx, d.trail_cx));
        assert_eq!([l.hp_clip, l.posture_clip, l.money_clip, l.skill_clip, l.lock_clip], [d.hp_clip, d.posture_clip, d.money_clip, d.skill_clip, d.lock_clip]);
        assert_eq!(l.fps, 60.0);
        // The movie's own facts.
        assert!(close(l.posture_cx, 960.0), "the posture gauge sits on the stage's centre");
        assert!(close(l.res_pitch, 46.8));
        let fill = apply_cx(&l.vitality_cx, [255.0; 4]);
        assert!(fill[0] > fill[1] && fill[1] > fill[2] - 15.0, "a red fill: {fill:?}");
        // The lock-on clip as hud.rs plays it (lock_clip: FadeIn 1-16, Loop to 83, FadeOut 84-89).
        assert_eq!((l.lock_clip.hold, l.lock_clip.fade_out, l.lock_clip.frames), (16, 84, 90));
    }
}
