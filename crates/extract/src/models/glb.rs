//! A minimal binary glTF 2.0 writer: one buffer, everything embedded.

use serde_json::{Value, json};

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_SHORT: u32 = 5123;
const UNSIGNED_INT: u32 = 5125;

#[derive(Default)]
pub struct Glb {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    pub nodes: Vec<Value>,
    pub meshes: Vec<Value>,
    pub materials: Vec<Value>,
    pub skins: Vec<Value>,
    images: Vec<Value>,
    textures: Vec<Value>,
}

impl Glb {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut v = json!({
            "buffer": 0,
            "byteOffset": self.bin.len(),
            "byteLength": bytes.len(),
        });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.bin.extend_from_slice(bytes);
        self.views.push(v);
        self.views.len() - 1
    }

    fn accessor(&mut self, view: usize, component: u32, count: usize, kind: &str) -> usize {
        self.accessors.push(json!({
            "bufferView": view,
            "componentType": component,
            "count": count,
            "type": kind,
        }));
        self.accessors.len() - 1
    }

    fn floats<const N: usize>(&mut self, data: &[[f32; N]], kind: &str, target: bool) -> usize {
        let bytes: Vec<u8> = data
            .iter()
            .flat_map(|v| v.iter().flat_map(|f| f.to_le_bytes()))
            .collect();
        let view = self.view(&bytes, target.then_some(ARRAY_BUFFER));
        self.accessor(view, FLOAT, data.len(), kind)
    }

    pub fn vec3(&mut self, data: &[[f32; 3]], with_bounds: bool) -> usize {
        let a = self.floats(data, "VEC3", true);
        if with_bounds {
            let mut min = [f32::MAX; 3];
            let mut max = [f32::MIN; 3];
            for v in data {
                for k in 0..3 {
                    min[k] = min[k].min(v[k]);
                    max[k] = max[k].max(v[k]);
                }
            }
            self.accessors[a]["min"] = json!(min);
            self.accessors[a]["max"] = json!(max);
        }
        a
    }

    pub fn vec2(&mut self, data: &[[f32; 2]]) -> usize {
        self.floats(data, "VEC2", true)
    }

    pub fn vec4(&mut self, data: &[[f32; 4]]) -> usize {
        self.floats(data, "VEC4", true)
    }

    pub fn joints(&mut self, data: &[[u16; 4]]) -> usize {
        let bytes: Vec<u8> = data
            .iter()
            .flat_map(|v| v.iter().flat_map(|j| j.to_le_bytes()))
            .collect();
        let view = self.view(&bytes, Some(ARRAY_BUFFER));
        self.accessor(view, UNSIGNED_SHORT, data.len(), "VEC4")
    }

    pub fn indices(&mut self, data: &[u32]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|i| i.to_le_bytes()).collect();
        let view = self.view(&bytes, Some(ELEMENT_ARRAY_BUFFER));
        self.accessor(view, UNSIGNED_INT, data.len(), "SCALAR")
    }

    pub fn mat4s(&mut self, data: &[glam::Mat4]) -> usize {
        let cols: Vec<[f32; 16]> = data.iter().map(|m| m.to_cols_array()).collect();
        self.floats(&cols, "MAT4", false)
    }

    /// Embeds a PNG and returns its texture index.
    pub fn texture(&mut self, png: &[u8], name: &str) -> usize {
        let view = self.view(png, None);
        self.images.push(json!({
            "bufferView": view,
            "mimeType": "image/png",
            "name": name,
        }));
        self.textures
            .push(json!({ "source": self.images.len() - 1, "sampler": 0 }));
        self.textures.len() - 1
    }

    /// References an external image file (relative URI, e.g. `../tex/x.dds`) and returns its
    /// texture index. Used by the map export, which shares textures between many GLBs.
    pub fn texture_uri(&mut self, uri: &str, name: &str) -> usize {
        self.images.push(json!({ "uri": uri, "name": name }));
        self.textures
            .push(json!({ "source": self.images.len() - 1, "sampler": 0 }));
        self.textures.len() - 1
    }

    pub fn finish(mut self, roots: &[usize], extras: Value) -> Vec<u8> {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut doc = json!({
            "asset": { "version": "2.0", "generator": "sekiro-extract" },
            "scene": 0,
            "scenes": [{ "nodes": roots, "extras": extras }],
            "nodes": self.nodes,
            "buffers": [{ "byteLength": self.bin.len() }],
            "bufferViews": self.views,
            "accessors": self.accessors,
            "samplers": [{ "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497 }],
        });
        for (key, list) in [
            ("meshes", self.meshes),
            ("materials", self.materials),
            ("skins", self.skins),
            ("images", self.images),
            ("textures", self.textures),
        ] {
            if !list.is_empty() {
                doc[key] = Value::Array(list);
            }
        }
        let mut json_bytes = serde_json::to_vec(&doc).expect("serialisable");
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let total = 12 + 8 + json_bytes.len() + 8 + self.bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json_bytes);
        out.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.bin);
        out
    }
}
