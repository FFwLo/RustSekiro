//! Havok 2016 tagfiles (TAG0) and type compendiums (TCM0), as used by
//! Sekiro's behavior (.hkx in behbnd) and animation files (anibnd).
//!
//! Only what the exporter needs: type names, member offsets, and the item
//! table (pointers, arrays and strings in DATA are stored as item indices).

use std::collections::HashMap;

use crate::bin::Reader;

#[derive(Default, Clone)]
pub struct Types {
    pub names: Vec<String>,
    pub parents: HashMap<usize, usize>,
    pub members: HashMap<usize, Vec<(String, usize)>>,
}

impl Types {
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Byte offset of a member, searching parent types too.
    pub fn member_offset(&self, ty: usize, member: &str) -> Option<usize> {
        let mut t = ty;
        while t != 0 {
            if let Some(m) = self.members.get(&t).and_then(|ms| ms.iter().find(|(n, _)| n == member)) {
                return Some(m.1);
            }
            t = *self.parents.get(&t)?;
        }
        None
    }
}

pub struct Item {
    pub ty: usize,
    pub offset: usize,
    pub count: usize,
}

pub struct Tagfile<'a> {
    pub d: &'a [u8],
    pub types: Types,
    pub data: usize,
    pub items: Vec<Item>,
}

fn sections(d: &[u8], start: usize, end: usize) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut o = start;
    while o + 8 <= end {
        let size = (u32::from_be_bytes(d[o..o + 4].try_into().unwrap()) & 0x3FFF_FFFF) as usize;
        if size < 8 {
            break;
        }
        out.push((String::from_utf8_lossy(&d[o + 4..o + 8]).into_owned(), o + 8, o + size));
        o += size;
    }
    out
}

fn section_map(d: &[u8]) -> HashMap<String, (usize, usize)> {
    fn walk(d: &[u8], s: usize, e: usize, out: &mut HashMap<String, (usize, usize)>) {
        for (tag, a, b) in sections(d, s, e) {
            if matches!(tag.as_str(), "TAG0" | "TCM0" | "TYPE" | "INDX") {
                walk(d, a, b, out);
            }
            out.insert(tag, (a, b));
        }
    }
    let mut out = HashMap::new();
    walk(d, 0, d.len(), &mut out);
    out
}

fn varint(d: &[u8], o: &mut usize) -> usize {
    let b = d[*o] as usize;
    let (v, n) = if b < 0x80 {
        (b, 1)
    } else if b & 0xC0 == 0x80 {
        (((b & 0x3F) << 8) | d[*o + 1] as usize, 2)
    } else if b & 0xE0 == 0xC0 {
        (((b & 0x1F) << 16) | (d[*o + 1] as usize) << 8 | d[*o + 2] as usize, 3)
    } else if b & 0xF8 == 0xE0 {
        (((b & 0x07) << 24) | (d[*o + 1] as usize) << 16 | (d[*o + 2] as usize) << 8 | d[*o + 3] as usize, 4)
    } else if b & 0xF8 == 0xE8 {
        // 5 bytes: 3 + 32 bits.
        (((b & 0x07) << 32) | (u32::from_be_bytes(d[*o + 1..*o + 5].try_into().unwrap()) as usize), 5)
    } else if b & 0xF8 == 0xF0 {
        // 6 bytes: 3 + 40 bits.
        let mut v = b & 0x07;
        for k in 1..6 {
            v = (v << 8) | d[*o + k] as usize;
        }
        (v, 6)
    } else if b == 0xF8 {
        // 8 bytes: 56 bits.
        let mut v = 0;
        for k in 1..8 {
            v = (v << 8) | d[*o + k] as usize;
        }
        (v, 8)
    } else if b == 0xF9 {
        (u64::from_be_bytes(d[*o + 1..*o + 9].try_into().unwrap()) as usize, 9)
    } else {
        panic!("unsupported tagfile varint prefix {b:#x}");
    };
    *o += n;
    v
}

fn strings(d: &[u8], s: Option<&(usize, usize)>) -> Vec<String> {
    s.map(|&(a, b)| d[a..b].split(|&c| c == 0).map(|x| String::from_utf8_lossy(x).into_owned()).collect())
        .unwrap_or_default()
}

/// Parses the TYPE section of a tagfile or compendium.
pub fn read_types(d: &[u8]) -> Types {
    let s = section_map(d);
    let tst = strings(d, s.get("TSTR").or(s.get("TST1")));
    let fst = strings(d, s.get("FSTR").or(s.get("FST1")));
    let mut t = Types::default();
    let Some(&(a, _)) = s.get("TNA1") else { return t };
    let mut o = a;
    let n = varint(d, &mut o);
    t.names.push(String::new());
    for _ in 1..n {
        let si = varint(d, &mut o);
        let tc = varint(d, &mut o);
        for _ in 0..tc {
            varint(d, &mut o);
            varint(d, &mut o);
        }
        t.names.push(tst.get(si).cloned().unwrap_or_default());
    }
    let Some(&(mut o, b)) = s.get("TBDY") else { return t };
    while o < b {
        let ti = varint(d, &mut o);
        if ti == 0 {
            continue;
        }
        let parent = varint(d, &mut o);
        let flags = varint(d, &mut o);
        t.parents.insert(ti, parent);
        if flags & 0x01 != 0 { varint(d, &mut o); }
        if flags & 0x02 != 0 { varint(d, &mut o); }
        if flags & 0x04 != 0 { varint(d, &mut o); }
        if flags & 0x08 != 0 { varint(d, &mut o); varint(d, &mut o); }
        if flags & 0x10 != 0 { varint(d, &mut o); }
        if flags & 0x20 != 0 {
            let count = varint(d, &mut o);
            let mut ms = Vec::with_capacity(count);
            for _ in 0..count {
                let name = varint(d, &mut o);
                varint(d, &mut o); // member flags
                let offset = varint(d, &mut o);
                varint(d, &mut o); // member type
                ms.push((fst.get(name).cloned().unwrap_or_default(), offset));
            }
            t.members.insert(ti, ms);
        }
        if flags & 0x40 != 0 {
            let count = varint(d, &mut o);
            for _ in 0..count {
                varint(d, &mut o);
                varint(d, &mut o);
            }
        }
        if flags & 0x80 != 0 { varint(d, &mut o); }
    }
    t
}

impl<'a> Tagfile<'a> {
    /// `compendium` supplies the types when the file only has a TCRF reference.
    pub fn new(d: &'a [u8], compendium: Option<&Types>) -> Self {
        let s = section_map(d);
        let types = if s.contains_key("TNA1") { read_types(d) } else { compendium.cloned().unwrap_or_default() };
        let data = s["DATA"].0;
        let (a, b) = s["ITEM"];
        let r = Reader::new(d);
        let items = (a..b)
            .step_by(12)
            .map(|k| Item { ty: (r.u32(k) & 0xFF_FFFF) as usize, offset: r.u32(k + 4) as usize, count: r.u32(k + 8) as usize })
            .collect();
        Self { d, types, data, items }
    }

    /// Every element of every item of the type (arrays expanded; stride = item span / count).
    pub fn elements_of(&self, type_name: &str) -> Vec<usize> {
        let Some(t) = self.types.index_of(type_name) else { return Vec::new() };
        let mut out = Vec::new();
        for (i, it) in self.items.iter().enumerate() {
            if it.ty != t || it.count == 0 {
                continue;
            }
            let next = self.items.iter().skip(i + 1).map(|n| n.offset).find(|&o| o > it.offset);
            let stride = next.map(|n| (n - it.offset) / it.count).unwrap_or(0);
            for k in 0..it.count {
                out.push(self.data + it.offset + k * stride);
            }
        }
        out
    }

    pub fn objects_of(&self, type_name: &str) -> Vec<usize> {
        let Some(t) = self.types.index_of(type_name) else { return Vec::new() };
        self.items.iter().filter(|i| i.ty == t).map(|i| self.data + i.offset).collect()
    }

    pub fn field(&self, type_name: &str, member: &str) -> usize {
        let t = self.types.index_of(type_name).unwrap_or_else(|| panic!("no type {type_name}"));
        self.types.member_offset(t, member).unwrap_or_else(|| panic!("no member {type_name}.{member}"))
    }

    /// (absolute offset, element count) of the item a pointer/array field refers to.
    pub fn deref(&self, at: usize) -> Option<(usize, usize)> {
        let idx = Reader::new(self.d).u64(at) as usize;
        (idx != 0 && idx < self.items.len()).then(|| (self.data + self.items[idx].offset, self.items[idx].count))
    }

    /// (type name, absolute offset) of the object a pointer field refers to.
    pub fn deref_obj(&self, at: usize) -> Option<(String, usize)> {
        let idx = Reader::new(self.d).u64(at) as usize;
        (idx != 0 && idx < self.items.len()).then(|| (self.types.names[self.items[idx].ty].clone(), self.data + self.items[idx].offset))
    }

    /// A member of an object of the given type (searching parents), if the type has it.
    pub fn try_field(&self, type_name: &str, member: &str) -> Option<usize> {
        self.types.member_offset(self.types.index_of(type_name)?, member)
    }

    pub fn string(&self, at: usize) -> String {
        self.deref(at)
            .map(|(o, n)| String::from_utf8_lossy(&self.d[o..o + n]).trim_end_matches('\0').to_string())
            .unwrap_or_default()
    }
}

/// Behavior file: CustomManualSelectorGenerator name -> (animId, offsetType).
pub fn cmsg_map(d: &[u8]) -> HashMap<String, (i32, i32)> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let ty = "CustomManualSelectorGenerator";
    if tf.types.index_of(ty).is_none() {
        return HashMap::new();
    }
    let (name, anim, offset_type) = (tf.field(ty, "name"), tf.field(ty, "animId"), tf.field(ty, "offsetType"));
    tf.objects_of(ty)
        .into_iter()
        .map(|o| {
            let n = tf.string(o + name);
            (n.trim_end_matches("_CMSG").to_string(), (r.i32(o + anim), r.i32(o + offset_type)))
        })
        .collect()
}

/// Transition effects: (type, name, duration, flags, endMode, blendCurve).
pub fn transition_effects(d: &[u8]) -> Vec<(String, String, f32, u16, u8, u8)> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let mut out = Vec::new();
    for ty in ["hkbBlendingTransitionEffect", "CustomTransitionEffect"] {
        if tf.types.index_of(ty).is_none() {
            continue;
        }
        let f = |n: &str| tf.field(ty, n);
        let (name, dur, flags, em, bc) = (f("name"), f("duration"), f("flags"), f("endMode"), f("blendCurve"));
        for o in tf.objects_of(ty) {
            out.push((ty.to_string(), tf.string(o + name), r.f32(o + dur), r.u16(o + flags), r.u8(o + em), r.u8(o + bc)));
        }
    }
    out
}

/// Blender generators: name, blendParameter, flags, and children (generator type/name, weight),
/// plus the behavior variables bound to the blender's members.
pub fn blenders(d: &[u8]) -> Vec<String> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let ty = "hkbBlenderGenerator";
    let mut out = Vec::new();
    if tf.types.index_of(ty).is_none() {
        return out;
    }
    // Variable names (hkbBehaviorGraphStringData.variableNames).
    let var_names: Vec<String> = tf
        .objects_of("hkbBehaviorGraphStringData")
        .first()
        .and_then(|&o| tf.deref(o + tf.field("hkbBehaviorGraphStringData", "variableNames")))
        .map(|(a, n)| (0..n).map(|i| tf.string(a + i * 8)).collect())
        .unwrap_or_default();
    let name_of = |t: &str, o: usize| tf.try_field(t, "name").map(|f| tf.string(o + f)).unwrap_or_default();
    for o in tf.objects_of(ty) {
        let mut line = format!(
            "{} blendParameter={} minCyclic={} maxCyclic={} flags={}",
            name_of(ty, o),
            r.f32(o + tf.field(ty, "blendParameter")),
            r.f32(o + tf.field(ty, "minCyclicBlendParameter")),
            r.f32(o + tf.field(ty, "maxCyclicBlendParameter")),
            r.u16(o + tf.field(ty, "flags"))
        );
        if let Some((bs_ty, bs)) = tf.deref_obj(o + tf.field(ty, "variableBindingSet")) {
            if let Some((a, n)) = tf.deref(bs + tf.field(&bs_ty, "bindings")) {
                let bty = "hkbVariableBindingSet::Binding";
                let (mp, vi) = (tf.field(bty, "memberPath"), tf.field(bty, "variableIndex"));
                let size = tf.types.index_of(bty).map(|_| 40).unwrap_or(40);
                for i in 0..n {
                    let b = a + i * size;
                    let v = r.i32(b + vi);
                    line += &format!("
    bind {} <- {}", tf.string(b + mp), var_names.get(v as usize).cloned().unwrap_or(v.to_string()));
                }
            }
        }
        if let Some((a, n)) = tf.deref(o + tf.field(ty, "children")) {
            for i in 0..n {
                if let Some((cty, c)) = tf.deref_obj(a + i * 8) {
                    let w = r.f32(c + tf.field(&cty, "weight"));
                    let generator = tf.deref_obj(c + tf.field(&cty, "generator")).map(|(gt, g)| format!("{gt} {}", name_of(&gt, g))).unwrap_or_default();
                    line += &format!("
    child weight={w} -> {generator}");
                }
            }
        }
        out.push(line);
    }
    out
}

/// Manual selector generators: name, the variable bound to selectedGeneratorIndex, and the
/// children by index (generator type and name).
pub fn selectors(d: &[u8]) -> Vec<String> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let mut out = Vec::new();
    let var_names: Vec<String> = tf
        .objects_of("hkbBehaviorGraphStringData")
        .first()
        .and_then(|&o| tf.deref(o + tf.field("hkbBehaviorGraphStringData", "variableNames")))
        .map(|(a, n)| (0..n).map(|i| tf.string(a + i * 8)).collect())
        .unwrap_or_default();
    let name_of = |t: &str, o: usize| tf.try_field(t, "name").map(|f| tf.string(o + f)).unwrap_or_default();
    for ty in ["hkbManualSelectorGenerator", "CustomManualSelectorGenerator"] {
        if tf.types.index_of(ty).is_none() {
            continue;
        }
        for o in tf.objects_of(ty) {
            let mut line = format!("{ty} {}", name_of(ty, o));
            if let Some((bs_ty, bs)) = tf.deref_obj(o + tf.field(ty, "variableBindingSet")) {
                if let Some((a, n)) = tf.deref(bs + tf.field(&bs_ty, "bindings")) {
                    let bty = "hkbVariableBindingSet::Binding";
                    let (mp, vi) = (tf.field(bty, "memberPath"), tf.field(bty, "variableIndex"));
                    for i in 0..n {
                        let b = a + i * 40;
                        let v = r.i32(b + vi);
                        line += &format!("
    bind {} <- {}", tf.string(b + mp), var_names.get(v as usize).cloned().unwrap_or(v.to_string()));
                    }
                }
            }
            if let Some(f) = tf.try_field(ty, "generators") {
                if let Some((a, n)) = tf.deref(o + f) {
                    for i in 0..n {
                        let g = tf.deref_obj(a + i * 8).map(|(gt, g)| format!("{gt} {}", name_of(&gt, g))).unwrap_or_default();
                        line += &format!("
    [{i}] {g}");
                    }
                }
            }
            out.push(line);
        }
    }
    out
}

/// Every object of `type_name`: its members (parents first) with the raw value read as
/// i32 / f32, and the string when the member is a string item.
pub fn objects(d: &[u8], type_name: &str) -> Vec<String> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let Some(ty) = tf.types.index_of(type_name) else { return Vec::new() };
    let mut chain = vec![ty];
    while let Some(&p) = tf.types.parents.get(chain.last().unwrap()) {
        if p == 0 {
            break;
        }
        chain.push(p);
    }
    chain.reverse();
    let mut out = Vec::new();
    for o in tf.elements_of(type_name) {
        let mut line = String::from(type_name);
        for t in &chain {
            for (name, off) in tf.types.members.get(t).cloned().unwrap_or_default() {
                let (i, f) = (r.i32(o + off), r.f32(o + off));
                let s = if name.to_lowercase().contains("name") { format!(" \"{}\"", tf.string(o + off)) } else { String::new() };
                line += &format!("
    {name}@{off:#x} = {i} / {f}{s}");
                // Array / pointer items: element type, count and the raw words.
                let idx = r.u64(o + off) as usize;
                if name != "name" && idx != 0 && idx < tf.items.len() && tf.items[idx].count > 0 && i > 16 {
                    let it = &tf.items[idx];
                    let start = tf.data + it.offset;
                    let words: Vec<String> = (0..(it.count * 4).min(32)).map(|k| {
                        let (wi, wf) = (r.i32(start + k * 4), r.f32(start + k * 4));
                        if wf.abs() > 1e-6 && wf.abs() < 1e6 { format!("{wf}") } else { format!("{wi}") }
                    }).collect();
                    line += &format!(" -> [{} x{}] {}", tf.types.names[it.ty], it.count, words.join(" "));
                }
            }
        }
        out.push(line);
    }
    out
}

/// CustomLookAtTwistModifier (TAE 700 upper-body aim): name, limits [deg] (up, down, right, left)
/// and the bone chains {startBoneIndex, endBoneIndex, targetRotationRate, newTargetGain, onGain,
/// offGain} (CustomLookAtTwistModifier::TwistParam, 0x18 bytes).
pub fn twist_modifiers(d: &[u8]) -> Vec<(String, [f32; 4], Vec<(i16, i16, f32, f32, f32, f32)>)> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let ty = "CustomLookAtTwistModifier";
    if tf.types.index_of(ty).is_none() {
        return Vec::new();
    }
    let f = |n: &str| tf.field(ty, n);
    let pt = "CustomLookAtTwistModifier::TwistParam";
    let pf = |n: &str| tf.field(pt, n);
    tf.objects_of(ty)
        .into_iter()
        .map(|o| {
            let limits = [r.f32(o + f("UpLimitAngle")), r.f32(o + f("DownLimitAngle")), r.f32(o + f("RightLimitAngle")), r.f32(o + f("LeftLimitAngle"))];
            let params = tf
                .deref(o + f("twistParam"))
                .map(|(a, n)| {
                    (0..n)
                        .map(|i| {
                            let e = a + i * 0x18;
                            (
                                r.i16(e + pf("startBoneIndex")),
                                r.i16(e + pf("endBoneIndex")),
                                r.f32(e + pf("targetRotationRate")),
                                r.f32(e + pf("newTargetGain")),
                                r.f32(e + pf("onGain")),
                                r.f32(e + pf("offGain")),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (tf.string(o + f("name")), limits, params)
        })
        .collect()
}

/// Twist modifiers of a shared NPC graph (c9997.hkx) with their character-property bindings
/// resolved: hkbVariableBindingSet bindings of type 1 (character property) such as
/// "twistParam:0/startBoneIndex" -> RefTwistNeckStart take the value at `variableIndex` in the
/// character's hkbCharacterData.characterPropertyValues (`props`, raw 32-bit words).
pub fn twist_modifiers_bound(d: &[u8], props: &[u32]) -> Vec<(String, [f32; 4], Vec<(i16, i16, f32, f32, f32, f32)>)> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let ty = "CustomLookAtTwistModifier";
    if tf.types.index_of(ty).is_none() {
        return Vec::new();
    }
    let mut out = twist_modifiers(d);
    let mut enabled = vec![true; out.len()];
    let bt = "hkbVariableBindingSet::Binding";
    for ((o, m), on) in tf.objects_of(ty).into_iter().zip(out.iter_mut()).zip(enabled.iter_mut()) {
        *on = r.u8(o + tf.field(ty, "enable")) != 0;
        let Some((_, set)) = tf.deref_obj(o + tf.field(ty, "variableBindingSet")) else { continue };
        let Some((a, n, s)) = tf.array(set + tf.field("hkbVariableBindingSet", "bindings")) else { continue };
        for i in 0..n {
            let e = a + i * s;
            if r.u8(e + tf.field(bt, "bindingType")) != 1 {
                continue;
            }
            let Some(&word) = props.get(r.i32(e + tf.field(bt, "variableIndex")) as usize) else { continue };
            let (int, float) = (word as i32, f32::from_bits(word));
            let path = tf.string(e + tf.field(bt, "memberPath"));
            match path.as_str() {
                // RefTwistEnable / RefQuadrupedTwistEnable: off modifiers are dropped.
                "enable" => *on = int != 0,
                "UpLimitAngle" => m.1[0] = float,
                "DownLimitAngle" => m.1[1] = float,
                "RightLimitAngle" => m.1[2] = float,
                "LeftLimitAngle" => m.1[3] = float,
                p => {
                    let Some((k, field)) = p.strip_prefix("twistParam:").and_then(|p| p.split_once('/')) else { continue };
                    let Some(c) = k.parse::<usize>().ok().and_then(|k| m.2.get_mut(k)) else { continue };
                    match field {
                        "startBoneIndex" => c.0 = int as i16,
                        "endBoneIndex" => c.1 = int as i16,
                        "targetRotationRate" => c.2 = float,
                        "newTargetGain" => c.3 = float,
                        "onGain" => c.4 = float,
                        "offGain" => c.5 = float,
                        _ => {}
                    }
                }
            }
        }
    }
    out.into_iter().zip(enabled).filter(|(_, on)| *on).map(|(m, _)| m).collect()
}

/// Character properties of a character file (Characters\cXXXX.hkx, unpacked as
/// Characters_cXXXX.hkx): names (hkbCharacterStringData.characterPropertyNames) and raw 32-bit
/// values (hkbCharacterData.characterPropertyValues.wordVariableValues), in property order.
pub fn character_properties(d: &[u8]) -> (Vec<String>, Vec<u32>) {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let cd = "hkbCharacterData";
    let Some(&o) = tf.objects_of(cd).first() else { return (Vec::new(), Vec::new()) };
    let words = tf
        .deref_obj(o + tf.field(cd, "characterPropertyValues"))
        .and_then(|(_, vs)| tf.deref(vs + tf.field("hkbVariableValueSet", "wordVariableValues")))
        .map(|(a, n)| (0..n).map(|i| r.u32(a + i * 4)).collect())
        .unwrap_or_default();
    let names = tf
        .deref_obj(o + tf.field(cd, "stringData"))
        .and_then(|(_, sd)| tf.deref(sd + tf.field("hkbCharacterStringData", "characterPropertyNames")))
        .map(|(a, n)| (0..n).map(|i| tf.string(a + i * 8)).collect())
        .unwrap_or_default();
    (names, words)
}

/// Behavior clip generators: (animationName, playbackSpeed, cropStart, cropEnd, startTime, enforcedDuration).
pub fn clip_generators(d: &[u8]) -> Vec<(String, f32, f32, f32, f32, f32)> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let ty = "hkbClipGenerator";
    if tf.types.index_of(ty).is_none() {
        return Vec::new();
    }
    let f = |n: &str| tf.field(ty, n);
    let (name, speed, cs, ce, st, ed) =
        (f("animationName"), f("playbackSpeed"), f("cropStartAmountLocalTime"), f("cropEndAmountLocalTime"), f("startTime"), f("enforcedDuration"));
    tf.objects_of(ty)
        .into_iter()
        .map(|o| (tf.string(o + name), r.f32(o + speed), r.f32(o + cs), r.f32(o + ce), r.f32(o + st), r.f32(o + ed)))
        .collect()
}

/// Animation file: (duration seconds, root motion samples [x, y, z, yaw]) if present.
pub fn root_motion(d: &[u8], compendium: &Types) -> (Option<f32>, Vec<[f32; 4]>) {
    let tf = Tagfile::new(d, Some(compendium));
    let r = Reader::new(d);
    let ty = "hkaDefaultAnimatedReferenceFrame";
    let Some(&o) = tf.objects_of(ty).first() else {
        let dur = tf.objects_of("hkaSplineCompressedAnimation").first().map(|&a| r.f32(a + tf.field("hkaAnimation", "duration")));
        return (dur, Vec::new());
    };
    let dur = r.f32(o + tf.field(ty, "duration"));
    let mut samples: Vec<[f32; 4]> = tf
        .deref(o + tf.field(ty, "referenceFrameSamples"))
        .map(|(a, n)| (0..n).map(|i| std::array::from_fn(|k| r.f32(a + i * 16 + k * 4))).collect())
        .unwrap_or_default();
    unwrap_yaw_runs(&mut samples);
    (Some(dur), samples)
}

/// The reference frame's yaw (w) is stored in [-pi, pi] and resampled linearly between keys, so a
/// key pair across the seam becomes a long turn the wrong way: a200_511200 (the Ochimusha's behind
/// deathblow) goes +90 -> -180 deg in 5 frames (-270) while the enemy's ThrowDef13200 turns +180.
/// A linear run between keys turning more than pi is rewritten the short way, and the samples after
/// it shift with it so the track stays continuous.
pub fn unwrap_yaw_runs(s: &mut [[f32; 4]]) {
    use std::f32::consts::{PI, TAU};
    if s.len() < 3 {
        return;
    }
    let mut keys = vec![0];
    for i in 1..s.len() - 1 {
        if ((s[i + 1][3] - s[i][3]) - (s[i][3] - s[i - 1][3])).abs() > 1e-3 {
            keys.push(i);
        }
    }
    keys.push(s.len() - 1);
    let orig: Vec<f32> = s.iter().map(|v| v[3]).collect();
    let mut shift = 0.0;
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = orig[b] - orig[a];
        let fix = if d.abs() > PI + 1e-3 { -d.signum() * TAU } else { 0.0 };
        for i in a + 1..=b {
            s[i][3] = orig[i] + shift + fix * (i - a) as f32 / (b - a) as f32;
        }
        shift += fix;
    }
}

pub struct Bone {
    pub name: String,
    pub parent: i16,
    pub pose: crate::spline::Qs,
}

fn read_qs(r: &Reader, o: usize) -> crate::spline::Qs {
    crate::spline::Qs {
        t: [r.f32(o), r.f32(o + 4), r.f32(o + 8)],
        r: [r.f32(o + 16), r.f32(o + 20), r.f32(o + 24), r.f32(o + 28)],
        s: [r.f32(o + 32), r.f32(o + 36), r.f32(o + 40)],
    }
}

/// skeleton.hkx: bones with parent index and reference pose.
pub fn skeleton(d: &[u8], compendium: &Types) -> Vec<Bone> {
    let tf = Tagfile::new(d, Some(compendium));
    let r = Reader::new(d);
    let ty = "hkaSkeleton";
    let Some(&o) = tf.objects_of(ty).first() else { return Vec::new() };
    let (parents, n) = tf.deref(o + tf.field(ty, "parentIndices")).unwrap_or((0, 0));
    let (bones, _) = tf.deref(o + tf.field(ty, "bones")).unwrap_or((0, 0));
    let (pose, _) = tf.deref(o + tf.field(ty, "referencePose")).unwrap_or((0, 0));
    let name_off = tf.field("hkaBone", "name");
    (0..n)
        .map(|i| Bone {
            name: tf.string(bones + i * 16 + name_off),
            parent: r.u16(parents + i * 2) as i16,
            pose: read_qs(&r, pose + i * 48),
        })
        .collect()
}

pub struct Clip {
    pub frame_duration: f32,
    pub frames: usize,
    pub track_to_bone: Vec<i16>,
    /// frames x tracks
    pub samples: Vec<Vec<crate::spline::Qs>>,
}

/// Animation hkx: decoded spline animation plus its track -> bone binding.
pub fn clip(d: &[u8], compendium: &Types) -> Option<Clip> {
    let tf = Tagfile::new(d, Some(compendium));
    let r = Reader::new(d);
    let ty = "hkaSplineCompressedAnimation";
    let &o = tf.objects_of(ty).first()?;
    let tracks = r.u32(o + tf.field("hkaAnimation", "numberOfTransformTracks")) as usize;
    let frames = r.u32(o + tf.field(ty, "numFrames")) as usize;
    let per_block = r.u32(o + tf.field(ty, "maxFramesPerBlock")) as usize;
    let frame_duration = r.f32(o + tf.field(ty, "frameDuration"));
    let (bo, nb) = tf.deref(o + tf.field(ty, "blockOffsets"))?;
    let block_offsets: Vec<u32> = (0..nb).map(|i| r.u32(bo + i * 4)).collect();
    let (data, len) = tf.deref(o + tf.field(ty, "data"))?;
    let samples = crate::spline::sample(&d[data..data + len], &block_offsets, tracks, frames, per_block.max(1));
    let track_to_bone = tf
        .objects_of("hkaAnimationBinding")
        .first()
        .and_then(|&b| tf.deref(b + tf.field("hkaAnimationBinding", "transformTrackToBoneIndices")))
        .map(|(a, n)| (0..n).map(|i| r.u16(a + i * 2) as i16).collect())
        .unwrap_or_else(|| (0..tracks as i16).collect());
    Some(Clip { frame_duration, frames, track_to_bone, samples })
}

/// Debug: every type that has items, with item counts and member offsets.
pub fn dump(d: &[u8]) {
    let tf = Tagfile::new(d, None);
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for it in &tf.items {
        *counts.entry(it.ty).or_default() += it.count.max(1);
    }
    let mut tys: Vec<_> = counts.into_iter().collect();
    tys.sort();
    for (t, n) in tys {
        let name = tf.types.names.get(t).cloned().unwrap_or_default();
        let mut members = Vec::new();
        let mut cur = t;
        while cur != 0 {
            if let Some(ms) = tf.types.members.get(&cur) {
                members.extend(ms.iter().map(|(m, o)| format!("{m}@{o}")));
            }
            match tf.types.parents.get(&cur) {
                Some(&p) => cur = p,
                None => break,
            }
        }
        println!("{t:4} {name} x{n}: {}", members.join(" "));
    }
}

/// One ragdoll body capsule: endpoints and radius in model space (bind pose).
pub struct HitCapsule {
    pub name: String,
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub radius: f32,
}

fn quat_rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

/// Character physics (chrbnd cXXXX.HKX): hknpRagdollData body capsules, the shapes
/// FromSoftware uses as character hurtboxes, in Havok model space.
pub fn ragdoll_capsules(d: &[u8]) -> Vec<HitCapsule> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let rd = "hknpRagdollData";
    let Some(&o) = tf.objects_of(rd).first() else { return Vec::new() };
    let bc = "hknpPhysicsSystemData::bodyCinfoWithAttachment";
    let Some(bt) = tf.types.index_of(bc) else { return Vec::new() };
    let Some((bodies, n)) = tf.deref(o + tf.field(rd, "bodyCinfos")) else { return Vec::new() };
    let size = tf.items.iter().find(|it| it.ty == bt && it.count == n).map(|_| 0).unwrap_or(0);
    let _ = size;
    // Element stride: distance between consecutive bodyCinfo items in the array.
    let stride = (tf.types.member_offset(bt, "attachedBody").unwrap_or(160) + 16 + 15) / 16 * 16;
    let (f_shape, f_name, f_pos, f_rot) = (tf.field(bc, "shape"), tf.field(bc, "name"), tf.field(bc, "position"), tf.field(bc, "orientation"));
    let cap = "hknpCapsuleShape";
    let (f_a, f_b, f_r) = (tf.field(cap, "a"), tf.field(cap, "b"), tf.field(cap, "convexRadius"));
    let mut out = Vec::new();
    for i in 0..n {
        let b = bodies + i * stride;
        let Some((s, _)) = tf.deref(b + f_shape) else { continue };
        let p = [r.f32(b + f_pos), r.f32(b + f_pos + 4), r.f32(b + f_pos + 8)];
        let q = [r.f32(b + f_rot), r.f32(b + f_rot + 4), r.f32(b + f_rot + 8), r.f32(b + f_rot + 12)];
        let local = |at: usize| [r.f32(s + at), r.f32(s + at + 4), r.f32(s + at + 8)];
        let world = |v: [f32; 3]| {
            let w = quat_rotate(q, v);
            [w[0] + p[0], w[1] + p[1], w[2] + p[2]]
        };
        out.push(HitCapsule { name: tf.string(b + f_name), a: world(local(f_a)), b: world(local(f_b)), radius: r.f32(s + f_r) });
    }
    out
}

/// Rumble camera (other/default.rumblebnd camera_NNN.hkx): an uncompressed Havok
/// animation; returns (duration, per-frame transforms of the first track as
/// [tx, ty, tz, qx, qy, qz, qw]).
pub fn rumble_cam(d: &[u8]) -> Option<(f32, Vec<[f32; 7]>)> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let ty = "hkaInterleavedUncompressedAnimation";
    let &o = tf.objects_of(ty).first()?;
    let duration = r.f32(o + tf.field("hkaAnimation", "duration"));
    let tracks = (r.u32(o + tf.field("hkaAnimation", "numberOfTransformTracks")) as usize).max(1);
    let (t, n) = tf.deref(o + tf.field(ty, "transforms"))?;
    let frames = n / tracks;
    Some((
        duration,
        (0..frames)
            .map(|f| {
                let a = t + f * tracks * 48;
                [r.f32(a), r.f32(a + 4), r.f32(a + 8), r.f32(a + 16), r.f32(a + 20), r.f32(a + 24), r.f32(a + 28)]
            })
            .collect(),
    ))
}

/// Layer generators: each hkbLayer's generator, weight, motion flag and boneWeights
/// (the upper/lower body masks), as `index=weight` runs of the non-zero entries.
pub fn layers(d: &[u8]) -> Vec<String> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let mut out = Vec::new();
    let name_of = |t: &str, o: usize| tf.try_field(t, "name").map(|f| tf.string(o + f)).unwrap_or_default();
    let ty = "hkbLayerGenerator";
    if tf.types.index_of(ty).is_none() {
        return out;
    }
    for o in tf.objects_of(ty) {
        let mut line = name_of(ty, o);
        if let Some((a, n)) = tf.deref(o + tf.field(ty, "layers")) {
            for i in 0..n {
                let Some((lty, l)) = tf.deref_obj(a + i * 8) else { continue };
                let g = tf.deref_obj(l + tf.field(&lty, "generator")).map(|(gt, g)| format!("{gt} {}", name_of(&gt, g))).unwrap_or_default();
                let fl = |m: &str| tf.try_field(&lty, m).map(|f| r.f32(l + f)).unwrap_or(f32::NAN);
                let motion = tf.try_field(&lty, "useMotion").map(|f| d[l + f]).unwrap_or(0);
                line += &format!("\n  [{i}] {g} weight={} fadeIn={} fadeOut={} useMotion={motion}", fl("weight"), fl("fadeInDuration"), fl("fadeOutDuration"));
                if let Some((_, bw)) = tf.deref_obj(l + tf.field(&lty, "boneWeights")) {
                    if let Some((wa, wn)) = tf.deref(bw + tf.field("hkbBoneWeightArray", "boneWeights")) {
                        let w: Vec<String> = (0..wn).filter_map(|k| { let v = r.f32(wa + k * 4); (v != 0.0 || wn < 64).then(|| format!("{k}={v}/{:#x}", r.u32(wa + k * 4))) }).collect();
                        line += &format!("\n      boneWeights x{wn}: {}", w.join(" "));
                    }
                }
            }
        }
        out.push(line);
    }
    out
}

/// State machines: name and each state's name, id and generator (type, name).
pub fn state_machines(d: &[u8]) -> Vec<String> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let mut out = Vec::new();
    let name_of = |t: &str, o: usize| tf.try_field(t, "name").map(|f| tf.string(o + f)).unwrap_or_default();
    let ty = "hkbStateMachine";
    for o in tf.objects_of(ty) {
        let mut line = name_of(ty, o);
        if let Some((a, n)) = tf.deref(o + tf.field(ty, "states")) {
            for i in 0..n {
                let Some((sty, s)) = tf.deref_obj(a + i * 8) else { continue };
                let g = tf.deref_obj(s + tf.field(&sty, "generator")).map(|(gt, g)| format!("{gt} {}", name_of(&gt, g))).unwrap_or_default();
                line += &format!("\n  {} id={} -> {g}", name_of(&sty, s), r.i32(s + tf.field(&sty, "stateId")));
            }
        }
        out.push(line);
    }
    out
}

/// hkbTwistModifier objects: name, members (axis, bones, angle method) and variable bindings.
pub fn twist_mods(d: &[u8]) -> Vec<String> {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let ty = "hkbTwistModifier";
    let mut out = Vec::new();
    if tf.types.index_of(ty).is_none() {
        return out;
    }
    let var_names: Vec<String> = tf
        .objects_of("hkbBehaviorGraphStringData")
        .first()
        .and_then(|&o| tf.deref(o + tf.field("hkbBehaviorGraphStringData", "variableNames")))
        .map(|(a, n)| (0..n).map(|i| tf.string(a + i * 8)).collect())
        .unwrap_or_default();
    let f = |m: &str| tf.try_field(ty, m);
    for o in tf.objects_of(ty) {
        let name = f("name").map(|x| tf.string(o + x)).unwrap_or_default();
        let axis = f("axisOfRotation").map(|x| [r.f32(o + x), r.f32(o + x + 4), r.f32(o + x + 8)]).unwrap_or_default();
        let mut line = format!(
            "{name} axis={axis:?} twistAngle={} start={} end={} setAngle={} coords={} additive={} enable={}",
            f("twistAngle").map(|x| r.f32(o + x)).unwrap_or(f32::NAN),
            f("startBoneIndex").map(|x| r.u16(o + x) as i16).unwrap_or(-1),
            f("endBoneIndex").map(|x| r.u16(o + x) as i16).unwrap_or(-1),
            f("setAngleMethod").map(|x| d[o + x]).unwrap_or(255),
            f("rotationAxisCoordinates").map(|x| d[o + x]).unwrap_or(255),
            f("isAdditive").map(|x| d[o + x]).unwrap_or(255),
            f("enable").map(|x| d[o + x]).unwrap_or(255),
        );
        if let Some((bs_ty, bs)) = tf.deref_obj(o + tf.field(ty, "variableBindingSet")) {
            if let Some((a, n)) = tf.deref(bs + tf.field(&bs_ty, "bindings")) {
                let bty = "hkbVariableBindingSet::Binding";
                let (mp, vi) = (tf.field(bty, "memberPath"), tf.field(bty, "variableIndex"));
                for i in 0..n {
                    let b = a + i * 40;
                    let v = r.i32(b + vi);
                    line += &format!("\n    bind {} <- {}", tf.string(b + mp), var_names.get(v as usize).cloned().unwrap_or(v.to_string()));
                }
            }
        }
        out.push(line);
    }
    out
}
