//! The game's menu texts and fonts from the unpacked cache (skipped without it).
use std::path::PathBuf;

fn raw() -> Option<PathBuf> {
    let root = sekiro_formats::cache::root().join("raw");
    root.join("font").is_dir().then_some(root)
}

#[test]
fn menu_fmg_and_fonts_read_from_the_install() {
    let Some(raw) = raw() else {
        eprintln!("cache/raw unavailable");
        return;
    };
    let events = std::fs::read(raw.join("msg/engus/menu.msgbnd.d/イベントテキスト.fmg")).unwrap();
    let events = sekiro_formats::fmg::parse(&events).unwrap();
    assert!(
        events[&10010310].contains("Hug wall"),
        "{:?}",
        events.get(&10010310)
    );
    // The death screen's caption.
    assert_eq!(events[&10010902], "DEATH");

    // The menu font draws Latin text; the Japanese one also the death mark.
    for (font, text) in [("engus_std", "Resurrect"), ("jpnjp_std", "死")] {
        let gfx = std::fs::read(raw.join(format!("font/{font}/font.gfx"))).unwrap();
        let ttf = sekiro_formats::gfx_font::to_sfnt(&gfx).unwrap();
        let face = ttf_parser::Face::parse(&ttf, 0).unwrap();
        assert_eq!(face.units_per_em(), 2048);
        for c in text.chars() {
            let glyph = face
                .glyph_index(c)
                .unwrap_or_else(|| panic!("{font} lacks {c}"));
            let bounds = face.glyph_bounding_box(glyph).unwrap();
            assert!(
                bounds.width() > 50 && bounds.height() > 50,
                "{font} {c}: {bounds:?}"
            );
            assert!(face.glyph_hor_advance(glyph).unwrap() > 0);
        }
    }
}
