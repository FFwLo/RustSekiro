//! Havok Cloth (chrbnd / partsbnd `*_c.hkx`, hclClothContainer): the per-character cloth
//! simulations FromSoftware uses for scabbards, sashes, hair and capes.

use crate::bin::Reader;
use crate::hkx::Tagfile;

impl<'a> Tagfile<'a> {
    /// (start, count, stride) of the array a field refers to; stride = item span / count.
    pub fn array(&self, at: usize) -> Option<(usize, usize, usize)> {
        let idx = Reader::new(self.d).u64(at) as usize;
        if idx == 0 || idx >= self.items.len() || self.items[idx].count == 0 {
            return None;
        }
        let it = &self.items[idx];
        let next = self.items.iter().skip(idx + 1).map(|n| n.offset).find(|&o| o > it.offset);
        let stride = next.map(|n| (n - it.offset) / it.count).unwrap_or(0);
        Some((self.data + it.offset, it.count, stride))
    }
}

/// Text survey of every cloth: sim cloths, operators, transform sets, constraint sets.
pub fn survey(d: &[u8]) {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let bones = bone_names(&tf);
    let cd = "hclClothData";
    for o in tf.objects_of(cd) {
        println!("== cloth {}", tf.string(o + tf.field(cd, "name")));
        if let Some((a, n, _)) = tf.array(o + tf.field(cd, "transformSetDefinitions")) {
            for i in 0..n {
                if let Some((ty, p)) = tf.deref_obj(a + i * 8) {
                    println!("  transformSet[{i}] {ty} {} n={}", tf.string(p + tf.field(&ty, "name")), r.i32(p + tf.field(&ty, "numTransforms")));
                }
            }
        }
        if let Some((a, n, _)) = tf.array(o + tf.field(cd, "bufferDefinitions")) {
            for i in 0..n {
                if let Some((ty, p)) = tf.deref_obj(a + i * 8) {
                    println!("  buffer[{i}] {ty} {} verts={} tris={}", tf.string(p + tf.field(&ty, "name")), r.i32(p + tf.field(&ty, "numVertices")), r.i32(p + tf.field(&ty, "numTriangles")));
                }
            }
        }
        if let Some((a, n, _)) = tf.array(o + tf.field(cd, "simClothDatas")) {
            for i in 0..n {
                let Some((ty, p)) = tf.deref_obj(a + i * 8) else { continue };
                let parts = tf.array(p + tf.field(&ty, "particleDatas"));
                let fixed = tf.array(p + tf.field(&ty, "fixedParticles"));
                let fixed: Vec<u16> = fixed.map(|(f, k, s)| (0..k).map(|j| r.u16(f + j * s)).collect()).unwrap_or_default();
                let si = p + tf.field(&ty, "simulationInfo");
                println!("  sim[{i}] {} particles={} gravity=({}, {}, {}) damping/s={} fixed={:?}", tf.string(p + tf.field(&ty, "name")), parts.map(|x| x.1).unwrap_or(0), r.f32(si), r.f32(si + 4), r.f32(si + 8), r.f32(si + 16), fixed);
                if let Some((pa, pn, ps)) = parts {
                    let s: Vec<String> = (0..pn.min(6)).map(|j| format!("m{} r{} f{}", r.f32(pa + j * ps), r.f32(pa + j * ps + 8), r.f32(pa + j * ps + 12))).collect();
                    println!("    particle data {}", s.join(", "));
                }
                if let Some((ca, cn, _)) = tf.array(p + tf.field(&ty, "staticConstraintSets")) {
                    for j in 0..cn {
                        if let Some((cty, cp)) = tf.deref_obj(ca + j * 8) {
                            let links = tf.try_field(&cty, "links").or(tf.try_field(&cty, "localConstraints")).or(tf.try_field(&cty, "perParticleData"));
                            let cnt = links.and_then(|l| tf.array(cp + l)).map(|x| x.1).unwrap_or(0);
                            println!("    constraint {cty} {} n={cnt}", tf.string(cp + tf.field(&cty, "name")));
                        }
                    }
                }
            }
        }
        if let Some((a, n, _)) = tf.array(o + tf.field(cd, "operators")) {
            for i in 0..n {
                if let Some((ty, p)) = tf.deref_obj(a + i * 8) {
                    println!("  op[{i}] {ty} {}", tf.string(p + tf.field(&ty, "name")));
                    if ty == "hclSimpleMeshBoneDeformOperator" {
                        let pairs = tf.array(p + tf.field(&ty, "triangleBonePairs"));
                        let out: Vec<String> = pairs
                            .map(|(a, n, s)| (0..n).map(|j| {
                                let b = r.u16(a + j * s) as usize / 64;
                                format!("{}<-tri{}", bones.get(b).map(String::as_str).unwrap_or("?"), r.u16(a + j * s + 2) / 6)
                            }).collect())
                            .unwrap_or_default();
                        println!("     bones {}", out.join(" "));
                    }
                }
            }
        }
        if let Some((a, n, _)) = tf.array(o + tf.field(cd, "clothStateDatas")) {
            for i in 0..n {
                if let Some((ty, p)) = tf.deref_obj(a + i * 8) {
                    let ops = tf.array(p + tf.field(&ty, "operators")).map(|(f, k, s)| (0..k).map(|j| r.u32(f + j * s)).collect::<Vec<_>>()).unwrap_or_default();
                    println!("  state[{i}] {} ops={ops:?}", tf.string(p + tf.field(&ty, "name")));
                }
            }
        }
    }
}

/// Names of the cloth file's own skeleton (the "Master" transform set indexes it).
pub fn bone_names(tf: &Tagfile) -> Vec<String> {
    let sk = "hkaSkeleton";
    let Some(&o) = tf.objects_of(sk).first() else { return Vec::new() };
    tf.array(o + tf.field(sk, "bones")).map(|(a, n, s)| (0..n).map(|i| tf.string(a + i * s)).collect()).unwrap_or_default()
}

/// Every type whose name contains `pat`: members (name@offset), parents included.
pub fn type_members(d: &[u8], pat: &str) {
    let tf = Tagfile::new(d, None);
    for (i, n) in tf.types.names.iter().enumerate() {
        if !n.contains(pat) {
            continue;
        }
        let mut t = i;
        let mut parts = Vec::new();
        while t != 0 {
            for (m, o) in tf.types.members.get(&t).cloned().unwrap_or_default() {
                parts.push(format!("{m}@{o}"));
            }
            t = *tf.types.parents.get(&t).unwrap_or(&0);
        }
        println!("{n}: {}", parts.join(" "));
    }
}

/// One deformed vertex: (vertex index, [(bone or triangle slot, weight 0..1)], local position (packed
/// or unpacked as stored), local normal).
pub type DeformEntry = (u16, Vec<(u16, f32)>, [f32; 4], [f32; 4]);

/// hclObjectSpaceDeformer: blocks of 16 vertices, N-blend lists picked in controlBytes order
/// (byte = blend count - 1); local positions/normals come in the same block order.
pub fn object_space_deformer(tf: &Tagfile, op: usize, op_ty: &str) -> Vec<DeformEntry> {
    let r = Reader::new(tf.d);
    let def = op + tf.field(op_ty, "objectSpaceDeformer");
    let dty = "hclObjectSpaceDeformer";
    let lists = ["oneBlendEntries", "twoBlendEntries", "threeBlendEntries", "fourBlendEntries", "fiveBlendEntries", "sixBlendEntries", "sevenBlendEntries", "eightBlendEntries"];
    let arrays: Vec<Option<(usize, usize, usize)>> = lists.iter().map(|l| tf.array(def + tf.field(dty, l))).collect();
    let ctrl: Vec<u8> = tf.array(def + tf.field(dty, "controlBytes")).map(|(a, n, _)| (0..n).map(|i| r.u8(a + i)).collect()).unwrap_or_default();
    // Local blocks: packed (hkPackedVector3, 8 bytes) or unpacked (hkVector4).
    let (packed, unpacked) = ["localPNs", "localPNTs", "localPs"].iter().filter_map(|f| tf.try_field(op_ty, f)).next().map(|f| (tf.array(op + f), None::<()>)).unwrap_or((None, None));
    let _ = unpacked;
    let unp = ["localUnpackedPNs", "localUnpackedPNTs", "localUnpackedPs"].iter().filter_map(|f| tf.try_field(op_ty, f)).next().and_then(|f| tf.array(op + f));
    if std::env::var("CLOTH_DEBUG").is_ok() {
        let counts: Vec<usize> = arrays.iter().map(|a| a.map(|x| x.1).unwrap_or(0)).collect();
        let first: Vec<Vec<u16>> = arrays.iter().map(|a| a.map(|(p, _, _)| (0..16).map(|v| r.u16(p + v * 2)).collect()).unwrap_or_default()).collect();
        eprintln!("ctrl {ctrl:?} counts(1..8) {counts:?} start {} end {} first-block verts {first:?}", r.u16(def + tf.field(dty, "startVertexIndex")), r.u16(def + tf.field(dty, "endVertexIndex")));
    }
    // controlBytes give the block order (one byte per 16-vertex block): 3 = one-blend, 2 = two,
    // 1 = three, 0 = four (checked against the sim cloth rest pose: every Sekiro skin uses at
    // most four blends). Local position blocks follow the same order.
    let mut next = [0usize; 8];
    let order: Vec<(usize, usize)> = ctrl
        .iter()
        .filter_map(|&c| {
            let k = 3usize.checked_sub(c as usize)?;
            let i = next[k];
            next[k] += 1;
            Some((k, i))
        })
        .collect();
    if order.len() != ctrl.len() {
        eprintln!("warning: deformer control byte > 3 (more than four blends) not decoded");
    }
    let mut out = Vec::new();
    for (blk, &(k, i)) in order.iter().enumerate() {
        let Some((a, _n, s)) = arrays[k] else { continue };
        let b = a + i * s;
        let nb = k + 1;
        for v in 0..16 {
            let vi = r.u16(b + v * 2);
            let bones: Vec<(u16, f32)> = (0..nb)
                .map(|j| {
                    let bone = r.u16(b + 32 + (v * nb + j) * 2);
                    let w = if nb == 1 { 1.0 } else { r.u8(b + 32 + 16 * nb * 2 + v * nb + j) as f32 / 255.0 };
                    (bone, w)
                })
                .collect();
            let (pos, nrm) = if let Some((ua, _, us)) = unp {
                let e = ua + blk * us;
                let rd = |o: usize| [r.f32(o), r.f32(o + 4), r.f32(o + 8), r.f32(o + 12)];
                (rd(e + v * 16), rd(e + 256 + v * 16))
            } else if let Some((pa, _, ps)) = packed {
                let e = pa + blk * ps;
                (unpack(&r, e + v * 8), unpack(&r, e + 128 + v * 8))
            } else {
                ([0.0; 4], [0.0; 4])
            };
            out.push((vi, bones, pos, nrm));
        }
    }
    out
}

/// hkPackedVector3: three i16 mantissas in the high half of a float, scaled by the float whose
/// high half is values[3].
fn unpack(r: &Reader, at: usize) -> [f32; 4] {
    let v = [r.i16(at), r.i16(at + 2), r.i16(at + 4), r.i16(at + 6)];
    let scale = f32::from_bits((v[3] as u16 as u32) << 16);
    let f = |x: i16| ((x as i32) << 16) as f32 * scale;
    [f(v[0]), f(v[1]), f(v[2]), scale]
}

/// Dev check: sim skin local positions vs the sim cloth rest pose.
pub fn check(d: &[u8]) {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let cd = "hclClothData";
    for o in tf.objects_of(cd) {
        println!("== {}", tf.string(o + tf.field(cd, "name")));
        let sim = tf.array(o + tf.field(cd, "simClothDatas")).and_then(|(a, _, _)| tf.deref_obj(a));
        let pose: Vec<[f32; 3]> = sim.clone()
            .and_then(|(ty, p)| tf.array(p + tf.field(&ty, "simClothPoses")).and_then(|(a, _, _)| tf.deref_obj(a)))
            .and_then(|(ty, p)| tf.array(p + tf.field(&ty, "positions")))
            .map(|(a, n, s)| (0..n).map(|i| [r.f32(a + i * s), r.f32(a + i * s + 4), r.f32(a + i * s + 8)]).collect())
            .unwrap_or_default();
        let Some((ops, n, _)) = tf.array(o + tf.field(cd, "operators")) else { continue };
        for i in 0..n {
            let Some((ty, p)) = tf.deref_obj(ops + i * 8) else { continue };
            if !ty.starts_with("hclObjectSpace") {
                continue;
            }
            if ty.contains("Skin") && r.i32(p + tf.field(&ty, "outputBufferIndex")) == 0 {
                let ps: String = pose.iter().map(|q| format!("{},{},{};", q[0], q[1], q[2])).collect();
                unsafe { std::env::set_var("CLOTH_POSE", ps) };
            } else {
                unsafe { std::env::remove_var("CLOTH_POSE") };
            }
            let e = object_space_deformer(&tf, p, &ty);
            if ty.contains("MeshMeshDeform") && ty.contains("ObjectSpace") {
                frame_check(&tf, sim.as_ref().map(|x| x.1).unwrap_or(0), &pose, p, &ty);
            }
            let ctrl = tf.array(p + tf.field(&ty, "objectSpaceDeformer") + tf.field("hclObjectSpaceDeformer", "controlBytes")).map(|x| x.1).unwrap_or(0);
            let out = tf.try_field(&ty, "outputBufferIndex").or(tf.try_field(&ty, "outputBufferIdx")).map(|f| r.i32(p + f)).unwrap_or(-1);
            print!("  {ty} {} blocks={ctrl} entries={} outBuf={out}", tf.string(p + tf.field(&ty, "name")), e.len());
            if ty.contains("Skin") && !pose.is_empty() {
                let mut worst = 0f32;
                let mut bad = std::collections::BTreeSet::new();
                for (vi, _, lp, _) in &e {
                    if let Some(q) = pose.get(*vi as usize) {
                        let d = (0..3).map(|k| (q[k] - lp[k]).abs()).fold(0.0, f32::max);
                        worst = worst.max(d);
                        if d > 0.01 {
                            bad.insert(*vi);
                        }
                    }
                }
                print!(" vs pose max err {worst:.4} ({} verts off {:?})", bad.len(), bad.iter().take(12).collect::<Vec<_>>());
                for (vi, b, lp, _) in e.iter().take(3) {
                    print!("
     v{vi} {:?} local {:?} pose {:?}", b, &lp[..3], pose.get(*vi as usize));
                }
            }
            println!();
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Dev check: triangle frame convention for MeshMeshDeform (frame(rest) * triangleFromMesh = I).
fn frame_check(tf: &Tagfile, sim: usize, pose: &[[f32; 3]], op: usize, ty: &str) {
    let r = Reader::new(tf.d);
    let tris: Vec<[u16; 3]> = tf.array(sim + tf.field("hclSimClothData", "triangleIndices")).map(|(a, n, _)| (0..n / 3).map(|t| [r.u16(a + t * 6), r.u16(a + t * 6 + 2), r.u16(a + t * 6 + 4)]).collect()).unwrap_or_default();
    let subset: Vec<u16> = tf.array(op + tf.field(ty, "inputTrianglesSubset")).map(|(a, n, _)| (0..n).map(|i| r.u16(a + i * 2)).collect()).unwrap_or_default();
    let Some((ma, mn, ms)) = tf.array(op + tf.field(ty, "triangleFromMeshTransforms")) else { return };
    // Frame = [v0 - c, v1 - c, cross(v0 - c, v1 - c), c] with c the centroid.
    let names = ["centroid frame", "flipped normal"];
    let mut worst = [0f32; 2];
    for i in 0..mn {
        let t = if subset.is_empty() { i } else { subset[i] as usize };
        let Some(tri) = tris.get(t) else { continue };
        let (v0, v1, v2) = (pose[tri[0] as usize], pose[tri[1] as usize], pose[tri[2] as usize]);
        let c = [(v0[0] + v1[0] + v2[0]) / 3.0, (v0[1] + v1[1] + v2[1]) / 3.0, (v0[2] + v1[2] + v2[2]) / 3.0];
        let (e1, e2) = (sub(v0, c), sub(v1, c));
        let n = cross(e1, e2);
        let v0 = c;
        let m = |row: usize, col: usize| r.f32(ma + i * ms + col * 16 + row * 4);
        if i < 2 && std::env::var("CLOTH_DEBUG").is_ok() {
            eprintln!("tri {t} {:?} v0 {v0:?} v1 {v1:?} v2 {v2:?} T(cols) {:?}", tri, (0..4).map(|c| (0..4).map(|rr| m(rr, c)).collect::<Vec<_>>()).collect::<Vec<_>>());
        }
        for (k, n) in [n, [-n[0], -n[1], -n[2]]].iter().enumerate() {
            // F columns: e1, e2, n, v0. (F * T)[row][col].
            let f = [[e1[0], e2[0], n[0], v0[0]], [e1[1], e2[1], n[1], v0[1]], [e1[2], e2[2], n[2], v0[2]], [0.0, 0.0, 0.0, 1.0]];
            for row in 0..4 {
                for col in 0..4 {
                    let v: f32 = (0..4).map(|j| f[row][j] * m(j, col)).sum();
                    let id = if row == col { 1.0 } else { 0.0 };
                    worst[k] = worst[k].max((v - id).abs());
                }
            }
        }
    }
    println!("     frame check over {mn} tris (subset {}): {}", subset.len(), names.iter().zip(worst).map(|(n, w)| format!("{n} {w:.4}")).collect::<Vec<_>>().join(", "));
}

/// Column-major 4x4 (Havok hkMatrix4 layout).
pub type M4 = [[f32; 4]; 4];

pub fn m4_mul(a: &M4, b: &M4) -> M4 {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for rr in 0..4 {
            o[c][rr] = (0..4).map(|k| a[k][rr] * b[c][k]).sum();
        }
    }
    o
}

pub fn m4_apply(m: &M4, v: [f32; 4]) -> [f32; 4] {
    let mut o = [0.0; 4];
    for rr in 0..4 {
        o[rr] = (0..4).map(|k| m[k][rr] * v[k]).sum();
    }
    o
}

fn qs_to_m4(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> M4 {
    let [x, y, z, w] = q;
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w)],
        [2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w)],
        [2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    [
        [r[0][0] * s[0], r[0][1] * s[0], r[0][2] * s[0], 0.0],
        [r[1][0] * s[1], r[1][1] * s[1], r[1][2] * s[1], 0.0],
        [r[2][0] * s[2], r[2][1] * s[2], r[2][2] * s[2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

/// Model-space bind matrices of the cloth file's skeleton (referencePose chained by parentIndices).
pub fn bind_world(tf: &Tagfile) -> Vec<M4> {
    let r = Reader::new(tf.d);
    let sk = "hkaSkeleton";
    let Some(&o) = tf.objects_of(sk).first() else { return Vec::new() };
    let parents: Vec<i16> = tf.array(o + tf.field(sk, "parentIndices")).map(|(a, n, _)| (0..n).map(|i| r.i16(a + i * 2)).collect()).unwrap_or_default();
    let Some((a, n, s)) = tf.array(o + tf.field(sk, "referencePose")) else { return Vec::new() };
    let mut out: Vec<M4> = Vec::with_capacity(n);
    for i in 0..n {
        let e = a + i * s;
        let v = |k: usize| r.f32(e + k * 4);
        let local = qs_to_m4([v(0), v(1), v(2)], [v(4), v(5), v(6), v(7)], [v(8), v(9), v(10)]);
        let p = parents.get(i).copied().unwrap_or(-1);
        out.push(if p >= 0 && (p as usize) < i { m4_mul(&out[p as usize], &local) } else { local });
    }
    out
}

/// One bone-space deformed vertex: (vertex, [(bone/triangle slot, local position with the weight
/// premultiplied, w = weight)]).
pub type BoneSpaceEntry = (u16, Vec<(u16, [f32; 4], [f32; 4])>);

/// hclBoneSpaceDeformer: blocks of 16 (vertex, bone) pairs - 4 verts x 4 bones, 5 x 3, 8 x 2,
/// 16 x 1 - picked in controlBytes order (3 = one-blend .. 0 = four); 16 local vectors per block.
pub fn bone_space_deformer(tf: &Tagfile, op: usize, op_ty: &str, unpacked_override: Option<bool>) -> Vec<BoneSpaceEntry> {
    let r = Reader::new(tf.d);
    let def = op + tf.field(op_ty, "boneSpaceDeformer");
    let dty = "hclBoneSpaceDeformer";
    let lists = ["oneBlendEntries", "twoBlendEntries", "threeBlendEntries", "fourBlendEntries"];
    let arrays: Vec<Option<(usize, usize, usize)>> = lists.iter().map(|l| tf.array(def + tf.field(dty, l))).collect();
    let ctrl: Vec<u8> = tf.array(def + tf.field(dty, "controlBytes")).map(|(a, n, _)| (0..n).map(|i| r.u8(a + i)).collect()).unwrap_or_default();
    let packed = ["localPNs", "localPs"].iter().filter_map(|f| tf.try_field(op_ty, f)).next().and_then(|f| tf.array(op + f));
    let unp = ["localUnpackedPNs", "localUnpackedPs"].iter().filter_map(|f| tf.try_field(op_ty, f)).next().and_then(|f| tf.array(op + f));
    let use_unpacked = unpacked_override.unwrap_or(unp.is_some());
    let mut next = [0usize; 4];
    let mut out = Vec::new();
    for (blk, &c) in ctrl.iter().enumerate() {
        let Some(k) = 3usize.checked_sub(c as usize) else { continue };
        let Some((a, _, s)) = arrays[k] else { continue };
        let b = a + next[k] * s;
        next[k] += 1;
        let nb = k + 1;
        let nv = 16 / nb;
        let bone_off = nv * 2;
        for v in 0..nv {
            let vi = r.u16(b + v * 2);
            let mut bones = Vec::new();
            for j in 0..nb {
                let slot = v * nb + j;
                let bone = r.u16(b + bone_off + slot * 2);
                let (local, normal) = if use_unpacked {
                    // Bone-space local blocks hold full hkVector4s (positions premultiplied by the
                    // weight, w = weight) in both the "packed" and the unpacked arrays.
                    let (ua, _, us) = unp.or(packed).unwrap();
                    let e = ua + blk * us + slot * 16;
                    let rd = |o: usize| [r.f32(o), r.f32(o + 4), r.f32(o + 8), r.f32(o + 12)];
                    (rd(e), if us >= 512 { rd(e + 256) } else { [0.0; 4] })
                } else {
                    let (pa, _, ps) = packed.unwrap();
                    (unpack(&r, pa + blk * ps + slot * 8), [0.0; 4])
                };
                bones.push((bone, local, normal));
            }
            out.push((vi, bones));
        }
    }
    out
}

/// Dev check of the bone-space operators against rest positions.
pub fn check_bone_space(d: &[u8]) {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let bind = bind_world(&tf);
    let cd = "hclClothData";
    for o in tf.objects_of(cd) {
        let sim = tf.array(o + tf.field(cd, "simClothDatas")).and_then(|(a, _, _)| tf.deref_obj(a));
        let pose: Vec<[f32; 3]> = sim
            .clone()
            .and_then(|(ty, p)| tf.array(p + tf.field(&ty, "simClothPoses")).and_then(|(a, _, _)| tf.deref_obj(a)))
            .and_then(|(ty, p)| tf.array(p + tf.field(&ty, "positions")))
            .map(|(a, n, s)| (0..n).map(|i| [r.f32(a + i * s), r.f32(a + i * s + 4), r.f32(a + i * s + 8)]).collect())
            .unwrap_or_default();
        let Some((ops, n, _)) = tf.array(o + tf.field(cd, "operators")) else { continue };
        for i in 0..n {
            let Some((ty, p)) = tf.deref_obj(ops + i * 8) else { continue };
            if ty.starts_with("hclBoneSpaceMeshMeshDeform") {
                let out_buf = r.i32(p + tf.field(&ty, "outputBufferIdx"));
                // Display bind positions from the matching object-space display skin.
                let mut disp: std::collections::HashMap<u16, [f32; 4]> = Default::default();
                for j in 0..n {
                    let Some((ty2, p2)) = tf.deref_obj(ops + j * 8) else { continue };
                    if ty2.starts_with("hclObjectSpaceSkin") && r.i32(p2 + tf.field(&ty2, "outputBufferIndex")) == out_buf {
                        for (vi, _, lp, _) in object_space_deformer(&tf, p2, &ty2) {
                            disp.insert(vi, lp);
                        }
                    }
                }
                let simp = sim.as_ref().map(|x| x.1).unwrap_or(0);
                let tris: Vec<[u16; 3]> = tf.array(simp + tf.field("hclSimClothData", "triangleIndices")).map(|(a, n, _)| (0..n / 3).map(|t| [r.u16(a + t * 6), r.u16(a + t * 6 + 2), r.u16(a + t * 6 + 4)]).collect()).unwrap_or_default();
                let subset: Vec<u16> = tf.array(p + tf.field(&ty, "inputTrianglesSubset")).map(|(a, n, _)| (0..n).map(|k| r.u16(a + k * 2)).collect()).unwrap_or_default();
                let e = bone_space_deformer(&tf, p, &ty, Some(true));
                let mut worst = 0f32;
                let mut cnt = 0;
                for (vi, slots) in &e {
                    let mut acc = [0.0f32; 4];
                    for (sl, l, _) in slots {
                        let t = subset.get(*sl as usize).copied().unwrap_or(*sl) as usize;
                        let Some(tri) = tris.get(t) else { continue };
                        let f = tri_frame(pose[tri[0] as usize], pose[tri[1] as usize], pose[tri[2] as usize]);
                        let q = m4_apply(&f, *l);
                        (0..4).for_each(|k| acc[k] += q[k]);
                    }
                    if let Some(q) = disp.get(vi) {
                        cnt += 1;
                        worst = worst.max((0..3).map(|k| (q[k] - acc[k]).abs()).fold(0.0, f32::max));
                    }
                }
                println!("  {ty} -> buf {out_buf}: {} verts, {cnt} compared, max err {worst:.4}", e.len());
            }
            if ty.starts_with("hclBoneSpaceSkin") {
                let subset: Vec<u16> = tf.array(p + tf.field(&ty, "transformSubset")).map(|(a, n, _)| (0..n).map(|k| r.u16(a + k * 2)).collect()).unwrap_or_default();
                for unpacked in [false, true] {
                    let e = bone_space_deformer(&tf, p, &ty, Some(unpacked));
                    let mut worst = 0f32;
                    for (vi, bones) in &e {
                        let mut acc = [0.0f32; 4];
                        for (b, l, _) in bones {
                            let bi = subset.get(*b as usize).copied().unwrap_or(*b) as usize;
                            let m = bind.get(bi).copied().unwrap_or([[0.0; 4]; 4]);
                            let q = m4_apply(&m, *l);
                            (0..4).for_each(|k| acc[k] += q[k]);
                        }
                        if let Some(q) = pose.get(*vi as usize) {
                            worst = worst.max((0..3).map(|k| (q[k] - acc[k]).abs()).fold(0.0, f32::max));
                        }
                    }
                    println!("  {ty} {} entries={} unpacked={unpacked} max err {worst:.4}", tf.string(p + tf.field(&ty, "name")), e.len());
                }
            }
        }
    }
}

/// Triangle frame used by the mesh-mesh deformers: columns v0 - c, v1 - c, cross(v0 - c, v1 - c), c
/// with c the centroid (checked: frame(rest) * triangleFromMeshTransform = identity).
pub fn tri_frame(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> M4 {
    let c = [(v0[0] + v1[0] + v2[0]) / 3.0, (v0[1] + v1[1] + v2[1]) / 3.0, (v0[2] + v1[2] + v2[2]) / 3.0];
    let (a, b) = (sub(v0, c), sub(v1, c));
    let n = cross(a, b);
    [[a[0], a[1], a[2], 0.0], [b[0], b[1], b[2], 0.0], [n[0], n[1], n[2], 0.0], [c[0], c[1], c[2], 1.0]]
}
