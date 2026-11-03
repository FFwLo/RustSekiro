//! Finds textures named by MTD/FLVER paths in the unpacked TPFs and decodes them to PNG.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sekiro_formats::tpf::{self, Dds, DdsFormat};

/// What a texture is used for; decides colour handling.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Usage {
    /// sRGB colour (albedo).
    Color,
    /// Tangent-space normal map; converted to glTF's convention.
    Normal,
    /// glTF metallic-roughness map packed from a character's `_n` shininess and `_m` mask.
    MetalRough,
}

/// Lazily loads TPF files and indexes their textures by lower-case name.
pub struct TextureStore {
    raw: PathBuf,
    loaded: HashMap<PathBuf, Vec<u8>>,
    /// texture name (lower case) -> (tpf file, index in that tpf)
    index: HashMap<String, (PathBuf, usize)>,
    png_cache: HashMap<(String, Usage), Option<Vec<u8>>>,
}

impl TextureStore {
    pub fn new(raw: &Path) -> Self {
        Self {
            raw: raw.to_path_buf(),
            loaded: HashMap::new(),
            index: HashMap::new(),
            png_cache: HashMap::new(),
        }
    }

    /// Makes the textures in `path` findable. Earlier loads win on name clashes.
    pub fn load_tpf(&mut self, path: &Path) -> Result<()> {
        if self.loaded.contains_key(path) || !path.exists() {
            return Ok(());
        }
        let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        for (i, t) in tpf::parse(&data)
            .with_context(|| format!("parsing {}", path.display()))?
            .iter()
            .enumerate()
        {
            self.index
                .entry(t.name.to_ascii_lowercase())
                .or_insert_with(|| (path.to_path_buf(), i));
        }
        self.loaded.insert(path.to_path_buf(), data);
        Ok(())
    }

    /// Loads every TPF in a directory (non-recursive).
    pub fn load_dir(&mut self, dir: &Path) -> Result<()> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(());
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for p in paths {
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tpf")) {
                self.load_tpf(&p)?;
            }
        }
        Ok(())
    }

    /// Loads the TPFs a game texture path most likely lives in, e.g.
    /// `N:\...\Model\chr\c1019\tex\c1019_body_a.tif` -> `chr/c1019.texbnd.d`.
    fn load_for_path(&mut self, game_path: &str) -> Result<()> {
        let lower = game_path.replace('\\', "/").to_ascii_lowercase();
        let raw = self.raw.clone();
        if let Some(rest) = lower.split("/model/chr/").nth(1) {
            let chr = rest.split('/').next().unwrap_or_default();
            self.load_dir(&raw.join(format!("chr/{chr}.texbnd.d")))?;
            self.load_dir(&raw.join(format!("chr/{chr}.chrbnd.d")))?;
        }
        if lower.contains("/model/parts/") {
            self.load_tpf(&raw.join("parts/common_body.tpf"))?;
            let stem = texture_stem(game_path).to_ascii_lowercase();
            // Part textures are named after their part, e.g. BD_M_9000_xxx_a.
            if stem.len() >= 9 {
                self.load_dir(&raw.join(format!("parts/{}.partsbnd.d", &stem[..9])))?;
            }
        }
        Ok(())
    }

    /// Decoded PNG for a texture path or bare name, or None when it cannot be found.
    pub fn png(&mut self, game_path: &str, usage: Usage) -> Result<Option<Vec<u8>>> {
        let name = texture_stem(game_path).to_ascii_lowercase();
        if name.is_empty() {
            return Ok(None);
        }
        if let Some(cached) = self.png_cache.get(&(name.clone(), usage)) {
            return Ok(cached.clone());
        }
        if !self.index.contains_key(&name) {
            self.load_for_path(game_path)?;
        }
        let result = match self.index.get(&name) {
            Some((file, i)) => {
                let data = &self.loaded[file];
                let tex = &tpf::parse(data)?[*i];
                let header = tex.header()?;
                match decode_rgba(&header, tex.dds) {
                    Ok(mut rgba) => {
                        if usage == Usage::Normal {
                            convert_normal_map(&mut rgba);
                        }
                        Some(encode_png(header.width, header.height, &rgba)?)
                    }
                    Err(e) => {
                        eprintln!("  warning: {name}: {e:#}");
                        None
                    }
                }
            }
            None => {
                eprintln!("  warning: texture {name} not found");
                None
            }
        };
        self.png_cache.insert((name, usage), result.clone());
        Ok(result)
    }

    /// Mip 0 of a texture as `(width, height, RGBA8)` without any channel conversion.
    fn raw_rgba(&mut self, game_path: &str) -> Result<Option<(usize, usize, Vec<u8>)>> {
        let name = texture_stem(game_path).to_ascii_lowercase();
        if name.is_empty() {
            return Ok(None);
        }
        if !self.index.contains_key(&name) {
            self.load_for_path(game_path)?;
        }
        let Some((file, i)) = self.index.get(&name) else {
            return Ok(None);
        };
        let tex = &tpf::parse(&self.loaded[file])?[*i];
        let header = tex.header()?;
        match decode_rgba(&header, tex.dds) {
            Ok(rgba) => Ok(Some((header.width as usize, header.height as usize, rgba))),
            Err(e) => {
                eprintln!("  warning: {name}: {e:#}");
                Ok(None)
            }
        }
    }

    /// The glTF metallic-roughness PNG of a character material: roughness (green) from the
    /// shininess in the blue channel of the `_n` map, metallic (blue) from the `_m` mask.
    /// None when neither texture exists.
    pub fn metal_rough_png(
        &mut self,
        normal: Option<&str>,
        metallic: Option<&str>,
    ) -> Result<Option<Vec<u8>>> {
        let stem = |p: Option<&str>| p.map(|p| texture_stem(p).to_ascii_lowercase());
        let key = (
            format!(
                "{}+{}",
                stem(normal).unwrap_or_default(),
                stem(metallic).unwrap_or_default()
            ),
            Usage::MetalRough,
        );
        if let Some(cached) = self.png_cache.get(&key) {
            return Ok(cached.clone());
        }
        let n = match normal {
            Some(p) => self.raw_rgba(p)?,
            None => None,
        };
        let m = match metallic {
            Some(p) => self.raw_rgba(p)?,
            None => None,
        };
        let result = match pack_metal_rough(
            n.as_ref().map(|t| (t.0, t.1, &t.2[..])),
            m.as_ref().map(|t| (t.0, t.1, &t.2[..])),
        ) {
            Some((w, h, px)) => Some(encode_png(w as u32, h as u32, &px)?),
            None => None,
        };
        self.png_cache.insert(key, result.clone());
        Ok(result)
    }
}

/// Lowest perceptual roughness the engine's specular lobe uses.
const MIN_ROUGHNESS: f32 = 0.089;

/// Packs a character material's glTF metallic-roughness texture (R unused, G roughness, B
/// metallic). Sekiro's `Character_AMSN` shaders keep the shininess (gloss) in the blue channel of
/// the normal map (cloth 0-36 of 255, armour up to 128) and a single-channel metallic mask in
/// `_m`; roughness is 1 - gloss. The output has the size of the larger input; the smaller one is
/// sampled nearest. Without a normal map the roughness stays at the exporter's default 0.75.
pub fn pack_metal_rough(
    normal: Option<(usize, usize, &[u8])>,
    metallic: Option<(usize, usize, &[u8])>,
) -> Option<(usize, usize, Vec<u8>)> {
    let (w, h) = match (normal, metallic) {
        (None, None) => return None,
        (Some(n), None) => (n.0, n.1),
        (None, Some(m)) => (m.0, m.1),
        (Some(n), Some(m)) => {
            if n.0 * n.1 >= m.0 * m.1 {
                (n.0, n.1)
            } else {
                (m.0, m.1)
            }
        }
    };
    let at = |t: (usize, usize, &[u8]), x: usize, y: usize, c: usize| -> u8 {
        let (tx, ty) = (x * t.0 / w, y * t.1 / h);
        t.2[(ty * t.0 + tx) * 4 + c]
    };
    let default_rough = (0.75f32 * 255.0).round() as u8;
    let min_rough = (MIN_ROUGHNESS * 255.0).round() as u8;
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let rough = match normal {
                Some(n) => (255 - at(n, x, y, 2)).max(min_rough),
                None => default_rough,
            };
            let metal = metallic.map_or(0, |m| at(m, x, y, 0));
            out.extend_from_slice(&[255, rough, metal, 255]);
        }
    }
    Some((w, h, out))
}

/// `N:\a\b\c1010_body_a.tif` -> `c1010_body_a`.
pub fn texture_stem(path: &str) -> &str {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    match file.rfind('.') {
        Some(dot) => &file[..dot],
        None => file,
    }
}

/// Decodes mip 0 of a DDS to tightly packed RGBA8.
pub fn decode_rgba(header: &Dds, dds: &[u8]) -> Result<Vec<u8>> {
    let (w, h) = (header.width as usize, header.height as usize);
    let src = header.top_mip(dds)?;
    let mut pixels = vec![0u32; w * h];
    let r = match header.format {
        DdsFormat::Bc1 => texture2ddecoder::decode_bc1a(src, w, h, &mut pixels),
        DdsFormat::Bc2 => texture2ddecoder::decode_bc2(src, w, h, &mut pixels),
        DdsFormat::Bc3 => texture2ddecoder::decode_bc3(src, w, h, &mut pixels),
        DdsFormat::Bc4 => texture2ddecoder::decode_bc4(src, w, h, &mut pixels),
        DdsFormat::Bc5 => texture2ddecoder::decode_bc5(src, w, h, &mut pixels),
        DdsFormat::Bc6hUf16 => texture2ddecoder::decode_bc6_unsigned(src, w, h, &mut pixels),
        DdsFormat::Bc6hSf16 => texture2ddecoder::decode_bc6_signed(src, w, h, &mut pixels),
        DdsFormat::Bc7 => texture2ddecoder::decode_bc7(src, w, h, &mut pixels),
        DdsFormat::Bgra8 => {
            return Ok(src
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect());
        }
        DdsFormat::Rgba8 => return Ok(src.to_vec()),
        DdsFormat::Unknown(f) => bail!("unsupported DDS format {f}"),
    };
    r.map_err(|e| anyhow::anyhow!("BC decode failed: {e}"))?;
    let single_channel = header.format == DdsFormat::Bc4;
    Ok(pixels
        .iter()
        .flat_map(|p| {
            // texture2ddecoder packs pixels as little-endian BGRA.
            let [b, g, r, a] = p.to_le_bytes();
            if single_channel {
                [r, r, r, 255]
            } else {
                [r, g, b, a]
            }
        })
        .collect())
}

/// Sekiro normal maps store X in red and Y in green, DirectX style (+Y pointing down the
/// texture); the other channels hold unrelated data. glTF wants XYZ in RGB with +Y up.
fn convert_normal_map(rgba: &mut [u8]) {
    for p in rgba.as_chunks_mut::<4>().0 {
        let x = p[0] as f32 / 255.0 * 2.0 - 1.0;
        let y = -(p[1] as f32 / 255.0 * 2.0 - 1.0);
        let z = (1.0 - x * x - y * y).max(0.0).sqrt();
        let enc = |v: f32| ((v * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
        p[1] = enc(y);
        p[2] = enc(z);
        p[3] = 255;
    }
}

pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header()?;
        w.write_image_data(rgba)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roughness_is_inverse_gloss_and_metal_is_the_mask() {
        // One texel: gloss 36 in blue of the normal map, metallic 255 in the mask.
        let n = [128u8, 128, 36, 255];
        let m = [255u8, 255, 255, 255];
        let (w, h, px) = pack_metal_rough(Some((1, 1, &n)), Some((1, 1, &m))).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(px, vec![255, 219, 255, 255]);
    }

    #[test]
    fn full_gloss_clamps_to_the_minimum_roughness_and_masks_are_resampled() {
        let n = [
            0u8, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255,
        ];
        let m = [200u8, 0, 0, 255];
        let (w, h, px) = pack_metal_rough(Some((2, 2, &n)), Some((1, 1, &m))).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&px[..4], &[255, 23, 200, 255]);
        assert_eq!(&px[12..], &[255, 23, 200, 255]);
        assert!(pack_metal_rough(None, None).is_none());
    }
}
