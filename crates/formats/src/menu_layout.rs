//! The XML atlas regions in menu `.layout` binder entries (not the FE movie).
use crate::reader::{Error, Result};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The texture is stored at half its logical dimensions.
    pub half: bool,
}

pub struct Atlas {
    pub image: String,
    pub regions: HashMap<String, Region>,
}

pub fn parse(bytes: &[u8]) -> Result<Atlas> {
    let error = |e: &dyn std::fmt::Display| Error::Format(format!("menu layout: {e}"));
    let text = std::str::from_utf8(bytes).map_err(|e| error(&e))?;
    let doc = roxmltree::Document::parse(text).map_err(|e| error(&e))?;
    let root = doc.root_element();
    let attr = |n: roxmltree::Node<'_, '_>, key: &str| {
        n.attribute(key)
            .map(str::to_owned)
            .ok_or_else(|| error(&format!("missing {key}")))
    };
    if root.tag_name().name() != "TextureAtlas" {
        return Err(error(&"expected TextureAtlas"));
    }
    let mut regions = HashMap::new();
    for n in root.children().filter(|n| n.has_tag_name("SubTexture")) {
        let number = |key| -> Result<u32> { attr(n, key)?.parse().map_err(|e| error(&e)) };
        let region = Region {
            x: number("x")?,
            y: number("y")?,
            width: number("width")?,
            height: number("height")?,
            half: n
                .attribute("half")
                .map(str::parse::<u32>)
                .transpose()
                .map_err(|e| error(&e))?
                .unwrap_or(0)
                != 0,
        };
        if region.width == 0 || region.height == 0 {
            return Err(error(&"empty region"));
        }
        regions.insert(attr(n, "name")?, region);
    }
    Ok(Atlas {
        image: attr(root, "imagePath")?,
        regions,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn atlas_regions_preserve_dimensions_and_reject_malformed_entries() {
        let atlas = super::parse(br#"<TextureAtlas imagePath="test.png"><SubTexture name="ring.png" x="3" y="4" width="66" height="38" half="1"/></TextureAtlas>"#).unwrap();
        let r = atlas.regions["ring.png"];
        assert_eq!(
            (atlas.image.as_str(), r.x, r.y, r.width, r.height, r.half),
            ("test.png", 3, 4, 66, 38, true)
        );
        assert!(super::parse(br#"<TextureAtlas imagePath="test.png"><SubTexture name="bad" x="0"/></TextureAtlas>"#).is_err());
        assert!(super::parse(b"<TextureAtlas").is_err());
        let optional = super::parse(br#"<TextureAtlas imagePath="test.png"><SubTexture name="dot.png" x="0" y="0" width="2" height="2"/></TextureAtlas>"#).unwrap();
        assert!(!optional.regions["dot.png"].half);
    }
}
