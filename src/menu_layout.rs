//! The in-game menu's places, sizes and text sizes, from the game's own menu movie
//! (menu/02_200_mainmenu.gfx, Scaleform), as hud_layout.rs is the HUD's: `sekiro-extract
//! hud-layout` writes the movie's instances at frame 1 to extracted/hud/layout/02_200_mainmenu.json
//! and [`MenuLayout::get`] resolves the menu's elements by their instance paths (docs/kb/hud.md):
//! - `TabList`: the four tab brushes (MENU_Caption_01, the open one's MENU_Cursor_01), the tab
//!   arrow and its key-top, the rule under them (MENU_Base_00_FUTA);
//! - `State_0` (Equipment): the four section bars (MENU_Caption_02) with their labels, the ten
//!   slots (`grid_0/ItemList/ItemList/Item_r_c`: three tools, the art, five quick items, the
//!   ninjutsu) with their frames, icons, cursors, equipped marks and counts, and the parchment;
//! - `State_1` (the list screens): the category name and icons, six rows (icon box MENU_Item_Top,
//!   plate MENU_ItemBase_09, cursor, count, name), the scroll bar, the details (`ItemText_0`:
//!   MENU_ItemDetails, the name bar, the icon box, the stat lines, the ruled text);
//! - `State_2` (Options): nine rows (MENU_ItemBase_01, cursor MENU_Cursor_03);
//! - `PlayerStatus` (the status bar's five plates, MENU_BlackPlate) and `StatusBar` (the help
//!   line and the key guide).
//! When the file is missing the defaults are the same numbers, read from the movie on
//! 2026-11-04 (`the_menu_layout_defaults_are_the_main_menu_movie` keeps them equal).
//!
//! gap: what the movie's ActionScript places at run time is not in the timelines: the tab and
//! category right-hand arrows (frame 1 has them next to the first; here mirrored past the last),
//! the list's row pitch past the six placed rows, the scroll thumb's length; and the plates'
//! contents of the status bar are placed inside the movie's plates by eye.

use crate::hud_layout::{Movie, StageRect, StageText};
use serde_json::Value;

/// One Equipment slot: frame, icon, cursor, equipped mark, count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotLayout {
    pub base: StageRect,
    pub icon: StageRect,
    pub cursor: StageRect,
    pub equipped: StageRect,
    pub stock: StageText,
}

/// A list row's parts (the first row's; the others are moved down to their row).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowLayout {
    pub row: StageRect,
    pub icon_box: StageRect,
    pub icon: StageRect,
    pub plate: StageRect,
    pub cursor: StageRect,
    pub equipped: StageRect,
    pub stock: StageText,
    pub name: StageText,
}

/// The details parchment of the list screens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailsLayout {
    pub base: StageRect,
    pub name_bar: StageRect,
    pub name: StageText,
    pub icon_box: StageRect,
    pub icon: StageRect,
    /// The first stat line and the next one's distance.
    pub stat: StageRect,
    pub stat_pitch: f32,
    pub stat_text: StageText,
    pub desc: StageText,
    /// The ruled lines (MENU_ME_Line): the first, the pitch, the last's top.
    pub line: StageRect,
    pub line_pitch: f32,
    pub line_last: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuLayout {
    pub base: StageRect,
    pub tabs: [StageRect; 4],
    pub tab_cursors: [StageRect; 4],
    pub tab_text: StageText,
    pub tab_arrow: StageRect,
    pub tab_key: StageRect,
    pub tab_rule: StageRect,
    /// Prosthetic Tools, Combat Arts, Quick Items, Ninjutsu.
    pub sections: [StageRect; 4],
    pub section_text: [StageText; 4],
    /// The three tools, the art, the five quick items, the ninjutsu (game_menu.rs `slots`).
    pub slots: [SlotLayout; 10],
    pub parchment: StageRect,
    pub details: DetailsLayout,
    pub cat_bar: StageRect,
    pub cat_name: StageText,
    pub cat_icons: [StageRect; 4],
    pub cat_cursors: [StageRect; 4],
    pub cat_arrow: StageRect,
    pub cat_key: StageRect,
    pub cat_rule: StageRect,
    pub row: RowLayout,
    /// The six rows' boxes.
    pub rows: [StageRect; 6],
    pub scroll_track: StageRect,
    pub scroll_bar: StageRect,
    pub options: [StageRect; 9],
    pub option_text: StageText,
    /// The status bar's plates: resurrection and Vitality, Unseen Aid, Vitality / Attack Power,
    /// the skill level, Spirit Emblems and Sen; the rule over them.
    pub status: [StageRect; 5],
    pub status_rule: StageRect,
    pub status_text: StageText,
    pub help: StageText,
    pub keys: StageText,
}

const TEXT: [u8; 4] = [204, 204, 204, 255];
const fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> StageRect {
    StageRect::new(x0, y0, x1, y1)
}
const fn t(x0: f32, y0: f32, x1: f32, y1: f32, px: f32) -> StageText {
    StageText {
        rect: r(x0, y0, x1, y1),
        px,
        color: TEXT,
    }
}
const fn slot(base: StageRect, icon: StageRect, cursor: StageRect, equipped: StageRect, stock: StageText) -> SlotLayout {
    SlotLayout { base, icon, cursor, equipped, stock }
}

/// The equipment grid's instances for game_menu.rs's slot order.
const SLOT_PATHS: [&str; 10] = ["Item_0_0", "Item_0_2", "Item_0_4", "Item_0_5", "Item_1_0", "Item_1_1", "Item_1_2", "Item_1_3", "Item_1_4", "Item_1_5"];
/// The section labels in the same order as `sections`.
const SECTION_TEXTS: [&str; 4] = ["ShinobiWepText", "KatanaSkillText", "UseItemText", "ThrowSkillText"];

impl Default for MenuLayout {
    /// menu/02_200_mainmenu.gfx as read on 2026-11-04 (hud-layout).
    fn default() -> Self {
        MenuLayout {
            base: r(0.0, 0.0, 1920.0, 1080.0),
            tabs: [
                r(233.2, 45.0, 580.2, 143.1),
                r(595.0, 45.0, 942.0, 143.1),
                r(956.8, 45.0, 1303.8, 143.1),
                r(1318.6, 45.0, 1665.6, 143.1),
            ],
            tab_cursors: [
                r(231.8, 44.9, 580.8, 166.9),
                r(593.6, 44.9, 942.6, 166.9),
                r(955.4, 44.9, 1304.4, 166.9),
                r(1317.2, 44.9, 1666.2, 166.9),
            ],
            tab_text: t(250.4, 91.8, 530.5, 127.5, 24.0),
            tab_arrow: r(102.5, 88.0, 138.5, 135.1),
            tab_key: r(135.5, 85.8, 203.5, 149.8),
            tab_rule: r(134.6, 146.6, 1766.0, 154.6),
            sections: [
                r(130.2, 176.4, 599.2, 228.4),
                r(748.5, 177.2, 1054.4, 229.2),
                r(130.2, 483.0, 599.2, 535.0),
                r(748.5, 483.1, 1054.5, 535.1),
            ],
            section_text: [
                t(149.2, 185.5, 572.7, 224.5, 24.0),
                t(765.6, 185.5, 1025.7, 224.5, 24.0),
                t(149.2, 491.8, 572.7, 530.8, 24.0),
                t(765.6, 491.8, 1025.7, 530.8, 24.0),
            ],
            slots: [
                slot(
                    r(135.7, 224.9, 317.4, 410.0),
                    r(151.4, 242.5, 302.5, 393.7),
                    r(135.6, 231.3, 327.6, 407.3),
                    r(197.7, 275.2, 234.7, 312.2),
                    t(253.4, 366.1, 298.4, 405.1, 24.0),
                ),
                slot(
                    r(331.9, 224.9, 513.7, 410.0),
                    r(347.7, 242.5, 498.8, 393.7),
                    r(331.9, 231.3, 523.9, 407.3),
                    r(393.9, 275.2, 430.9, 312.2),
                    t(449.7, 366.1, 494.7, 405.1, 24.0),
                ),
                slot(
                    r(528.3, 224.9, 710.0, 410.0),
                    r(544.0, 242.5, 695.2, 393.7),
                    r(528.2, 231.3, 720.2, 407.3),
                    r(590.3, 275.2, 627.3, 312.2),
                    t(646.0, 366.1, 691.0, 405.1, 24.0),
                ),
                slot(
                    r(754.8, 225.5, 943.9, 419.8),
                    r(772.5, 244.1, 929.6, 401.3),
                    r(759.8, 228.7, 951.8, 404.6),
                    r(821.8, 272.6, 858.8, 309.6),
                    t(865.0, 349.5, 910.0, 388.5, 24.0),
                ),
                slot(
                    r(137.8, 538.7, 245.9, 648.7),
                    r(146.4, 548.3, 237.1, 639.1),
                    r(136.9, 541.6, 252.1, 647.2),
                    r(174.2, 568.0, 196.4, 590.2),
                    t(194.2, 618.5, 238.6, 653.5, 40.0),
                ),
                slot(
                    r(253.2, 538.7, 361.3, 648.7),
                    r(261.7, 548.3, 352.5, 639.1),
                    r(252.3, 541.6, 367.5, 647.2),
                    r(289.5, 568.0, 311.7, 590.2),
                    t(309.6, 618.5, 354.0, 653.5, 40.0),
                ),
                slot(
                    r(368.5, 538.7, 476.6, 648.7),
                    r(377.1, 548.3, 467.8, 639.1),
                    r(367.6, 541.6, 482.8, 647.2),
                    r(404.9, 568.0, 427.1, 590.2),
                    t(424.9, 618.5, 469.3, 653.5, 40.0),
                ),
                slot(
                    r(483.9, 538.7, 592.0, 648.7),
                    r(492.4, 548.3, 583.2, 639.1),
                    r(483.0, 541.6, 598.2, 647.2),
                    r(520.2, 568.0, 542.4, 590.2),
                    t(540.3, 618.5, 584.7, 653.5, 40.0),
                ),
                slot(
                    r(599.3, 538.7, 707.4, 648.7),
                    r(607.8, 548.3, 698.6, 639.1),
                    r(598.4, 541.6, 713.6, 647.2),
                    r(635.6, 568.0, 657.8, 590.2),
                    t(655.7, 618.5, 700.1, 653.5, 40.0),
                ),
                slot(
                    r(753.0, 531.9, 942.0, 726.2),
                    r(770.7, 550.5, 927.8, 707.7),
                    r(758.0, 535.0, 950.0, 711.0),
                    r(820.0, 579.0, 857.0, 616.0),
                    t(863.1, 655.9, 908.1, 694.9, 24.0),
                ),
            ],
            parchment: r(1057.2, 153.4, 1796.2, 871.4),
            details: DetailsLayout {
                base: r(1057.2, 153.4, 1796.2, 851.2),
                name_bar: r(1080.0, 167.4, 1770.7, 219.4),
                name: t(1104.2, 174.9, 1746.2, 213.9, 24.0),
                icon_box: r(1537.2, 218.3, 1751.5, 417.3),
                icon: r(1567.7, 242.2, 1718.8, 393.4),
                stat: r(1087.6, 276.0, 1526.6, 312.1),
                stat_pitch: 47.75,
                stat_text: t(1107.3, 276.6, 1372.6, 315.6, 24.0),
                desc: t(1106.8, 440.9, 1748.8, 816.9, 24.0),
                line: r(1097.2, 474.4, 1758.2, 477.4),
                line_pitch: 33.65,
                line_last: 811.25,
            },
            cat_bar: r(177.5, 160.9, 648.5, 189.3),
            cat_name: t(188.1, 157.8, 653.9, 196.8, 24.0),
            cat_icons: [
                r(191.1, 197.8, 261.1, 268.8),
                r(282.6, 197.8, 352.6, 268.8),
                r(374.1, 197.8, 444.1, 268.8),
                r(465.6, 197.8, 535.6, 268.8),
            ],
            cat_cursors: [
                r(191.4, 197.0, 261.4, 268.0),
                r(282.9, 197.0, 352.9, 268.0),
                r(374.4, 197.0, 444.4, 268.0),
                r(465.9, 197.0, 536.0, 268.0),
            ],
            cat_arrow: r(102.3, 212.3, 138.3, 259.4),
            cat_key: r(135.3, 210.1, 203.3, 274.1),
            cat_rule: r(135.6, 277.5, 925.7, 280.5),
            row: RowLayout {
                row: r(118.7, 283.8, 939.0, 371.5),
                icon_box: r(136.4, 283.8, 230.9, 371.5),
                icon: r(153.2, 298.1, 212.9, 357.8),
                plate: r(221.8, 300.4, 939.0, 366.4),
                cursor: r(221.1, 299.8, 926.7, 366.6),
                equipped: r(133.2, 284.6, 170.2, 321.6),
                stock: t(118.7, 335.4, 222.7, 364.9, 21.0),
                name: t(266.1, 316.1, 811.1, 353.1, 24.0),
            },
            rows: [
                r(118.7, 283.8, 939.0, 371.5),
                r(118.7, 372.6, 939.0, 460.4),
                r(118.7, 461.6, 939.0, 549.3),
                r(118.7, 550.5, 939.0, 638.2),
                r(118.7, 639.4, 939.0, 727.1),
                r(118.7, 728.5, 939.0, 816.2),
            ],
            scroll_track: r(954.4, 309.1, 981.4, 798.7),
            scroll_bar: r(960.0, 309.1, 974.0, 798.7),
            options: [
                r(131.2, 198.8, 558.2, 272.9),
                r(131.2, 280.0, 558.2, 354.0),
                r(131.2, 361.1, 558.2, 435.1),
                r(131.2, 442.3, 558.2, 516.3),
                r(131.2, 523.5, 558.2, 597.5),
                r(131.2, 604.6, 558.2, 678.6),
                r(131.2, 685.8, 558.2, 759.8),
                r(131.2, 767.0, 558.2, 841.0),
                r(131.2, 848.4, 558.2, 922.4),
            ],
            option_text: t(145.2, 220.5, 545.2, 259.5, 24.0),
            status: [
                r(128.9, 848.6, 510.3, 961.5),
                r(512.8, 848.0, 810.2, 962.8),
                r(827.9, 848.1, 1129.2, 962.0),
                r(1127.4, 848.6, 1520.1, 961.6),
                r(1528.2, 840.7, 1786.0, 968.1),
            ],
            status_rule: r(128.3, 840.9, 1759.8, 845.6),
            status_text: t(872.0, 865.5, 1091.7, 911.0, 24.0),
            help: t(99.9, 975.0, 1239.9, 1014.0, 24.0),
            keys: t(679.5, 975.0, 1819.5, 1014.0, 24.0),
        }
    }
}

impl MenuLayout {
    /// extracted/hud/layout/02_200_mainmenu.json (read once), else the defaults.
    pub fn get() -> &'static MenuLayout {
        static L: std::sync::OnceLock<MenuLayout> = std::sync::OnceLock::new();
        L.get_or_init(|| {
            let path = crate::paths::extracted().join("hud/layout/02_200_mainmenu.json");
            std::fs::read_to_string(path)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .map_or_else(MenuLayout::default, |v| MenuLayout::from_json(&v))
        })
    }

    /// Resolves every element in a hud-layout file of the menu movie; what is not found keeps
    /// its default.
    pub fn from_json(v: &Value) -> Self {
        let mut l = MenuLayout::default();
        let Some(m) = Movie::of(v) else { return l };
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
        set(&mut l.base, m.image("#1", "MENU_Base_00"));
        for i in 0..4 {
            set(&mut l.tabs[i], m.image(&format!("TabList/Item_0_{i}/Caption"), "MENU_Caption_01"));
            set(&mut l.tab_cursors[i], m.image(&format!("TabList/Item_0_{i}/Cursor"), "MENU_Cursor_01"));
        }
        set_t(&mut l.tab_text, m.text("TabList/Item_0_0/Caption/Text_0"));
        set(&mut l.tab_arrow, m.image("TabList/LeftArrow", "MENU_Arrow"));
        set(&mut l.tab_key, m.rect("TabList/LeftArrow/Text"));
        set(&mut l.tab_rule, m.image("#1/#2", "MENU_Base_00_FUTA"));
        // The section bars: the Caption_02 under each label.
        let bars = m.leaves("State_0/WindowList", "MENU_Caption_02");
        for (i, name) in SECTION_TEXTS.iter().enumerate() {
            let Some(text) = m.text(&format!("State_0/WindowList/{name}/Text_0")) else { continue };
            l.section_text[i] = text;
            let (cx, cy) = (text.rect.cx(), text.rect.cy());
            if let Some(b) = bars.iter().find(|b| b.x0 <= cx && cx <= b.x1 && b.y0 <= cy && cy <= b.y1 && b.y1 < 600.0) {
                l.sections[i] = *b;
            }
        }
        let grid = "State_0/WindowList/grid_0/ItemList/ItemList";
        for (i, p) in SLOT_PATHS.iter().enumerate() {
            let s = &mut l.slots[i];
            let base = format!("{grid}/{p}");
            set(
                &mut s.base,
                m.image(&format!("{base}/#1"), "MENU_IconBase_02").or_else(|| m.image(&format!("{base}/#1"), "MENU_ItemBase_Skill")),
            );
            set(&mut s.icon, m.rect(&format!("{base}/ItemIcon")));
            set(&mut s.cursor, m.rect(&format!("{base}/Cursor")));
            set(&mut s.equipped, m.rect(&format!("{base}/Equipped")));
            set_t(&mut s.stock, m.text(&format!("{base}/StockNum")));
        }
        set(&mut l.parchment, m.rect("State_0/WindowList/ItemText"));
        let d = "State_1/ItemText/ItemText_0";
        set(&mut l.details.base, m.image(d, "MENU_ItemDetails"));
        // The name bar is two Caption_02 halves.
        let name_bars = m
            .leaves(d, "MENU_Caption_02")
            .into_iter()
            .filter(|b| b.y1 < l.details.icon_box.y0 + 10.0)
            .fold(None, |o, b| Some(StageRect::union(o, b)));
        set(&mut l.details.name_bar, name_bars);
        set_t(&mut l.details.name, m.text(&format!("{d}/Text_0/Text_0")));
        set(&mut l.details.icon_box, m.image(d, "MENU_Item_Top"));
        set(&mut l.details.icon, m.rect(&format!("{d}/ItemIcon")));
        if let (Some(a), Some(b)) = (m.rect(&format!("{d}/InventoryNum/#3")), m.rect(&format!("{d}/InventoryNum/#1"))) {
            l.details.stat = a;
            l.details.stat_pitch = b.y0 - a.y0;
        }
        set_t(&mut l.details.stat_text, m.text(&format!("{d}/InventoryNum/InventoryNumText")));
        set_t(&mut l.details.desc, m.text(&format!("{d}/Desc/Text_0")));
        let mut lines = m.leaves(d, "MENU_ME_Line");
        lines.sort_by(|a, b| a.y0.total_cmp(&b.y0));
        if let (Some(a), Some(b), Some(z)) = (lines.first(), lines.get(1), lines.last()) {
            l.details.line = *a;
            l.details.line_pitch = b.y0 - a.y0;
            l.details.line_last = z.y0;
        }
        let tl = "State_1/TabList";
        set(&mut l.cat_bar, m.image(&format!("{tl}/CategoryName"), "MENU_Caption_02"));
        set_t(&mut l.cat_name, m.text(&format!("{tl}/CategoryName/Text_0")));
        for i in 0..4 {
            set(&mut l.cat_icons[i], m.rect(&format!("{tl}/Item_0_{i}/Caption")));
            set(&mut l.cat_cursors[i], m.rect(&format!("{tl}/Item_0_{i}/Cursor")));
        }
        set(&mut l.cat_arrow, m.image(&format!("{tl}/LeftArrow"), "MENU_Arrow"));
        set(&mut l.cat_key, m.rect(&format!("{tl}/LeftArrow/Text")));
        set(&mut l.cat_rule, m.image(tl, "MENU_ME_Line"));
        let list = "State_1/grid_0/ItemList/ItemList";
        let r0 = format!("{list}/Item_0_0");
        set(&mut l.row.row, m.rect(&r0));
        set(&mut l.row.icon_box, m.image(&r0, "MENU_Item_Top"));
        set(&mut l.row.icon, m.rect(&format!("{r0}/ItemIcon")));
        set(&mut l.row.plate, m.image(&r0, "MENU_ItemBase_09"));
        set(&mut l.row.cursor, m.rect(&format!("{r0}/Cursor")));
        set(&mut l.row.equipped, m.rect(&format!("{r0}/Equipped")));
        set_t(&mut l.row.stock, m.text(&format!("{r0}/StockNum/StockNum")));
        set_t(&mut l.row.name, m.text(&format!("{r0}/Text_0/Text_0")));
        for i in 0..6 {
            set(&mut l.rows[i], m.rect(&format!("{list}/Item_{i}_0")));
        }
        // The track: the scroll images' width, the bar shape's height.
        let sb = "State_1/grid_0/ScrollBarV";
        if let (Some(base), Some(bar), Some(span)) = (m.image(sb, "MENU_Scroll_Base"), m.image(sb, "MENU_Scroll_Bar"), m.rect(&format!("{sb}/Bar"))) {
            l.scroll_track = StageRect::new(base.x0, span.y0, base.x1, span.y1);
            l.scroll_bar = StageRect::new(bar.x0, span.y0, bar.x1, span.y1);
        }
        for i in 0..9 {
            set(&mut l.options[i], m.image(&format!("State_2/ItemList/Item_{i}_0"), "MENU_ItemBase_01"));
        }
        set_t(&mut l.option_text, m.text("State_2/ItemList/Item_0_0/Text_0/Text"));
        for (i, p) in ["#20", "#44", "#34", "#30", "#1"].iter().enumerate() {
            set(&mut l.status[i], m.image(&format!("PlayerStatus/{p}"), "MENU_BlackPlate"));
        }
        set(&mut l.status_rule, m.image("PlayerStatus/#50", "MENU_Base_00_FUTA"));
        set_t(&mut l.status_text, m.text("PlayerStatus/Grow_0/StaticText_10101"));
        set_t(&mut l.help, m.text("StatusBar/LineHelp"));
        set_t(&mut l.keys, m.text("StatusBar/KeyGuide"));
        l
    }

    /// The tab arrow on the right: the left one mirrored past the last tab.
    pub fn tab_arrow_right(&self) -> (StageRect, StageRect) {
        mirror(self.tabs[0].x0, self.tabs[3].x1, self.tab_arrow, self.tab_key)
    }
    /// The category arrow on the right of `n` icons.
    pub fn cat_arrow_right(&self, n: usize) -> (StageRect, StageRect) {
        let last = self.cat_icons[n.clamp(1, 4) - 1];
        mirror(self.cat_icons[0].x0, last.x1, self.cat_arrow, self.cat_key)
    }
    /// List row `k`'s box (past the six placed: at the last pitch).
    pub fn row_at(&self, k: usize) -> StageRect {
        if let Some(r) = self.rows.get(k) {
            return *r;
        }
        let pitch = self.rows[5].y0 - self.rows[4].y0;
        let dy = pitch * (k - 5) as f32;
        StageRect::new(self.rows[5].x0, self.rows[5].y0 + dy, self.rows[5].x1, self.rows[5].y1 + dy)
    }
    /// `part` of the first row moved to row `k`.
    pub fn in_row(&self, part: StageRect, k: usize) -> StageRect {
        let dy = self.row_at(k).y0 - self.row.row.y0;
        StageRect::new(part.x0, part.y0 + dy, part.x1, part.y1 + dy)
    }
}

/// `arrow` and `key`, left of a span from `x0`, mirrored to the span's right end `x1`.
fn mirror(x0: f32, x1: f32, arrow: StageRect, key: StageRect) -> (StageRect, StageRect) {
    let flip = |b: StageRect| StageRect::new(x1 + (x0 - b.x1), b.y0, x1 + (x0 - b.x0), b.y1);
    (flip(arrow), flip(key))
}

/// `inner` as shares of `outer` (left, top, width, height, %), for a child node of `outer`'s.
pub fn within(outer: &StageRect, inner: &StageRect) -> (f32, f32, f32, f32) {
    let (w, h) = (outer.w().max(1e-3), outer.h().max(1e-3));
    ((inner.x0 - outer.x0) / w * 100.0, (inner.y0 - outer.y0) / h * 100.0, inner.w() / w * 100.0, inner.h() / h * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close_r(a: &StageRect, b: &StageRect) -> bool {
        [(a.x0, b.x0), (a.y0, b.y0), (a.x1, b.x1), (a.y1, b.y1)].iter().all(|(x, y)| (x - y).abs() < 0.06)
    }

    /// A small menu movie: its tabs, a slot, a row and the options resolve by instance path,
    /// the section bar is the Caption_02 under its label, and what is missing keeps the default.
    #[test]
    fn the_menu_layout_reads_a_menu_movie() {
        let img = |b: [f32; 4], name: &str| serde_json::json!({"kind": "image", "bounds": b, "images": [name]});
        let txt = |b: [f32; 4], h: f32| serde_json::json!({"kind": "text", "bounds": b, "text": {"height": h, "color": [204, 204, 204, 255]}});
        let v = serde_json::json!({"instances": {
            "TabList/Item_0_2/Caption/#1": img([900.0, 40.0, 1250.0, 140.0], "MENU_Caption_01"),
            "State_0/WindowList/#9/#1": img([100.0, 170.0, 600.0, 230.0], "MENU_Caption_02"),
            "State_0/WindowList/ShinobiWepText/Text_0": txt([150.0, 180.0, 500.0, 220.0], 30.0),
            "State_0/WindowList/grid_0/ItemList/ItemList/Item_1_0/#1": img([10.0, 500.0, 110.0, 600.0], "MENU_IconBase_02"),
            "State_1/grid_0/ItemList/ItemList/Item_3_0": {"kind": "sprite", "bounds": [100.0, 600.0, 900.0, 690.0]},
            "State_2/ItemList/Item_8_0/#1/#1": img([130.0, 850.0, 560.0, 920.0], "MENU_ItemBase_01"),
        }});
        let l = MenuLayout::from_json(&v);
        let d = MenuLayout::default();
        assert!(close_r(&l.tabs[2], &r(900.0, 40.0, 1250.0, 140.0)));
        assert_eq!(l.tabs[0], d.tabs[0], "missing: the default");
        assert!(close_r(&l.sections[0], &r(100.0, 170.0, 600.0, 230.0)), "the bar under the label");
        assert_eq!(l.section_text[0].px, 30.0);
        assert!(close_r(&l.slots[4].base, &r(10.0, 500.0, 110.0, 600.0)), "Item_1_0 is the first quick item");
        assert!(close_r(&l.rows[3], &r(100.0, 600.0, 900.0, 690.0)));
        assert!(close_r(&l.options[8], &r(130.0, 850.0, 560.0, 920.0)));
    }

    /// The right-hand arrows mirror the left ones past the span; rows past the sixth go on at
    /// the last pitch.
    #[test]
    fn menu_arrows_mirror_and_rows_go_on_at_the_pitch() {
        let l = MenuLayout::default();
        let (a, k) = l.tab_arrow_right();
        assert!((a.x1 - l.tabs[3].x1 - (l.tabs[0].x0 - l.tab_arrow.x0)).abs() < 1e-3);
        assert!(k.x1 <= a.x1 && k.x0 < a.x0, "the key-top inside the arrow");
        let pitch = l.rows[5].y0 - l.rows[4].y0;
        assert!((l.row_at(7).y0 - l.rows[5].y0 - 2.0 * pitch).abs() < 1e-3);
        let moved = l.in_row(l.row.plate, 2);
        assert!((moved.y0 - l.row.plate.y0 - (l.rows[2].y0 - l.rows[0].y0)).abs() < 1e-3);
        let (x, y, w, h) = within(&r(100.0, 100.0, 300.0, 200.0), &r(150.0, 125.0, 250.0, 175.0));
        assert_eq!((x, y, w, h), (25.0, 25.0, 50.0, 50.0));
    }

    /// With the exported movie, the built-in numbers are the movie's (skipped without the data).
    #[test]
    fn the_menu_layout_defaults_are_the_main_menu_movie() {
        let path = crate::paths::extracted().join("hud/layout/02_200_mainmenu.json");
        let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
            eprintln!("skip: no extracted/hud/layout/02_200_mainmenu.json");
            return;
        };
        let read = MenuLayout::from_json(&v);
        let d = MenuLayout::default();
        let (a, b) = (format!("{read:?}"), format!("{d:?}"));
        if a != b {
            // Rounded to 0.1 px, the defaults may differ by that much.
            let nums = |s: &str| {
                s.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                    .filter_map(|x| x.parse::<f32>().ok())
                    .collect::<Vec<_>>()
            };
            let (na, nb) = (nums(&a), nums(&b));
            assert_eq!(na.len(), nb.len(), "{a}");
            for (x, y) in na.iter().zip(&nb) {
                assert!((x - y).abs() < 0.06, "movie {x} vs default {y}:\n{a}");
            }
        }
    }
}
