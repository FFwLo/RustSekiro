//! The icon's pixels, shared by `icon.rs` (the window icon at run time) and `build.rs` (the
//! exe's icon resource): one function, no image file, nothing from the game's assets.

/// A `size` x `size` RGBA disk: dark lacquer with a thin pale rim and a red centre dot, the
/// look of the HUD's life dots (hud.rs); transparent outside the disk.
pub fn dot(size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let c = (size as f32 - 1.0) / 2.0;
    let r = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt() / r;
            let edge = ((1.0 - d) * r).clamp(0.0, 1.0); // one-pixel anti-aliased rim
            let (rgb, a) = if d > 1.0 {
                ([0, 0, 0], 0.0)
            } else if d > 0.86 {
                ([214, 200, 170], edge)
            } else if d < 0.52 {
                let t = (d / 0.52).powi(2);
                ([200 + (20.0 * (1.0 - t)) as u8, (30.0 + 10.0 * (1.0 - t)) as u8, 24], 1.0)
            } else {
                ([28, 22, 20], 1.0)
            };
            out.extend([rgb[0], rgb[1], rgb[2], (a * 255.0) as u8]);
        }
    }
    out
}
