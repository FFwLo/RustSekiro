//! Export of Havok cloth (`*_c.hkx`) for the game: everything in Havok / FLVER model space.
//!
//! Every deformer is flattened so the game needs one formula per kind:
//! - skin: p = sum(boneWorld[bone] * local)       (local w = weight; normals with w = 0)
//! - mesh-mesh deform: p = sum(tri_frame(now) * local)
//! - mesh-bone deform: boneWorld = tri_frame(now) * local

use serde_json::{json, Value};

use crate::bin::Reader;
use crate::cloth::{bind_world, bone_names, bone_space_deformer, m4_apply, m4_mul, object_space_deformer, tri_frame, M4};
use crate::hkx::Tagfile;

type Entries = Vec<(u16, Vec<(u16, [f32; 4], [f32; 4])>)>;

const IDENT: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn u16s(tf: &Tagfile, at: usize) -> Vec<u16> {
    let r = Reader::new(tf.d);
    tf.array(at).map(|(a, n, _)| (0..n).map(|i| r.u16(a + i * 2)).collect()).unwrap_or_default()
}

fn m4_at(r: &Reader, at: usize) -> M4 {
    std::array::from_fn(|c| std::array::from_fn(|k| r.f32(at + c * 16 + k * 4)))
}

fn m4s(tf: &Tagfile, at: usize) -> Vec<M4> {
    let r = Reader::new(tf.d);
    tf.array(at).map(|(a, n, s)| (0..n).map(|i| m4_at(&r, a + i * s)).collect()).unwrap_or_default()
}

fn add4(acc: &mut [f32; 4], v: [f32; 4]) {
    (0..4).for_each(|k| acc[k] += v[k]);
}

fn scale4(v: [f32; 4], w: f32) -> [f32; 4] {
    [v[0] * w, v[1] * w, v[2] * w, v[3] * w]
}

/// Solves the frame's linear part for a direction: m_lin * x = v (None when degenerate).
fn inv3_apply(m: &M4, v: [f32; 3]) -> Option<[f32; 3]> {
    // Columns a, b, c of the linear part.
    let (a, b, c) = ([m[0][0], m[0][1], m[0][2]], [m[1][0], m[1][1], m[1][2]], [m[2][0], m[2][1], m[2][2]]);
    let cross = |p: [f32; 3], q: [f32; 3]| [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
    let dot = |p: [f32; 3], q: [f32; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    let det = dot(a, cross(b, c));
    if det.abs() < 1e-12 {
        return None;
    }
    // Cramer's rule: x_i = det with column i replaced by v, over det.
    Some([dot(v, cross(b, c)) / det, dot(a, cross(v, c)) / det, dot(a, cross(b, v)) / det])
}

fn round(v: f32) -> Value {
    json!((v as f64 * 1e6).round() / 1e6)
}

fn v4(v: [f32; 4]) -> Value {
    Value::Array(v.iter().map(|&x| round(x)).collect())
}

/// Object-space entries (vertex, [(slot, weight)], local p, local n) -> premultiplied locals through
/// the per-slot matrices (boneFromSkinMesh or triangleFromMesh); slots remapped by `subset`.
fn flatten_object(entries: Vec<crate::cloth::DeformEntry>, mats: &[M4], subset: &[u16]) -> Entries {
    let map = |b: u16| subset.get(b as usize).copied().unwrap_or(b);
    entries
        .into_iter()
        .map(|(v, bs, l, n)| {
            let (lp, ln) = ([l[0], l[1], l[2], 1.0], [n[0], n[1], n[2], 0.0]);
            let out = bs
                .into_iter()
                .map(|(b, w)| {
                    let m = mats.get(b as usize).copied().unwrap_or(IDENT);
                    (map(b), scale4(m4_apply(&m, lp), w), scale4(m4_apply(&m, ln), w))
                })
                .collect();
            (v, out)
        })
        .collect()
}

fn flatten_bone(tf: &Tagfile, op: usize, ty: &str, subset: &[u16]) -> Entries {
    let map = |b: u16| subset.get(b as usize).copied().unwrap_or(b);
    bone_space_deformer(tf, op, ty, Some(true)).into_iter().map(|(v, bs)| (v, bs.into_iter().map(|(b, l, n)| (map(b), l, n)).collect())).collect()
}

fn skin_entries(tf: &Tagfile, op: usize, ty: &str) -> Entries {
    let subset = u16s(tf, op + tf.field(ty, "transformSubset"));
    if ty.starts_with("hclBoneSpace") {
        return flatten_bone(tf, op, ty, &subset);
    }
    flatten_object(object_space_deformer(tf, op, ty), &m4s(tf, op + tf.field(ty, "boneFromSkinMeshTransforms")), &subset)
}

fn mesh_deform_entries(tf: &Tagfile, op: usize, ty: &str) -> Entries {
    let subset = u16s(tf, op + tf.field(ty, "inputTrianglesSubset"));
    if ty.starts_with("hclBoneSpace") {
        return flatten_bone(tf, op, ty, &subset);
    }
    // triangleFromMeshTransforms are indexed by subset slot; the subset maps the slot to the triangle.
    let mats = m4s(tf, op + tf.field(ty, "triangleFromMeshTransforms"));
    let map = |b: u16| subset.get(b as usize).copied().unwrap_or(b);
    flatten_object(object_space_deformer(tf, op, ty), &mats, &[]).into_iter().map(|(v, bs)| (v, bs.into_iter().map(|(b, l, n)| (map(b), l, n)).collect())).collect()
}

fn frame_of(tris: &[[u16; 3]], pose: &[[f32; 3]], t: u16) -> Option<M4> {
    let tri = tris.get(t as usize)?;
    Some(tri_frame(*pose.get(tri[0] as usize)?, *pose.get(tri[1] as usize)?, *pose.get(tri[2] as usize)?))
}

fn constraint_set(tf: &Tagfile, cty: &str, cp: usize) -> Value {
    let r = Reader::new(tf.d);
    let arr = |f: &str| tf.array(cp + tf.field(cty, f));
    let rows = |f: &str, row: &dyn Fn(usize) -> Value| -> Vec<Value> { arr(f).map(|(a, n, s)| (0..n).map(|i| row(a + i * s)).collect()).unwrap_or_default() };
    match cty {
        "hclStandardLinkConstraintSet" | "hclStretchLinkConstraintSet" => json!({
            "kind": if cty.contains("Stretch") { "stretch" } else { "standard" },
            "links": rows("links", &|e| json!([r.u16(e), r.u16(e + 2), round(r.f32(e + 4)), round(r.f32(e + 8))])),
        }),
        "hclCompressibleLinkConstraintSet" => json!({
            "kind": "compressible",
            // particleA, particleB, restLength, compressionLength, stiffness
            "links": rows("links", &|e| json!([r.u16(e), r.u16(e + 2), round(r.f32(e + 4)), round(r.f32(e + 8)), round(r.f32(e + 12))])),
        }),
        "hclBendLinkConstraintSet" => json!({
            "kind": "bend",
            "links": rows("links", &|e| json!([r.u16(e), r.u16(e + 2), round(r.f32(e + 4)), round(r.f32(e + 8)), round(r.f32(e + 12)), round(r.f32(e + 16))])),
        }),
        "hclBendStiffnessConstraintSet" => json!({
            "kind": "bend_stiffness",
            // weightA..D, bendStiffness, restCurvature, particleA..D
            "links": rows("links", &|e| json!([round(r.f32(e)), round(r.f32(e + 4)), round(r.f32(e + 8)), round(r.f32(e + 12)), round(r.f32(e + 16)), round(r.f32(e + 20)), r.u16(e + 24), r.u16(e + 26), r.u16(e + 28), r.u16(e + 30)])),
            "maxRestPoseHeightSq": round(r.f32(cp + tf.field(cty, "maxRestPoseHeightSq"))),
            "clamp": r.u8(cp + tf.field(cty, "clampBendStiffness")),
            "useRestPoseConfig": r.u8(cp + tf.field(cty, "useRestPoseConfig")),
        }),
        "hclLocalRangeConstraintSet" => json!({
            "kind": "local_range",
            // particle, referenceVertex, maximumDistance, maxNormalDistance, minNormalDistance
            "items": rows("localConstraints", &|e| json!([r.u16(e), r.u16(e + 2), round(r.f32(e + 4)), round(r.f32(e + 8)), round(r.f32(e + 12))])),
            "stiffness": round(r.f32(cp + tf.field(cty, "stiffness"))),
            "shape": r.u32(cp + tf.field(cty, "shapeType")),
            "normal": r.u8(cp + tf.field(cty, "applyNormalComponent")),
        }),
        "hclTransitionConstraintSet" => json!({
            "kind": "transition",
            // particle, referenceVertex
            "items": rows("perParticleData", &|e| json!([r.u16(e), r.u16(e + 2)])),
        }),
        "hclVolumeConstraintMx" => {
            // Shape matching (exe FUN_141571de0): frames sum the weighted particles to a centre
            // and a covariance, the apply rows pull their particle to centre + R * frameVector.
            // Batches hold 16 lanes (frameVector 16 x 16 bytes, particleIndex u16 x 16 at 256,
            // weight / stiffness f32 x 16 at 288); singles one entry (frameVector, index at 16,
            // weight / stiffness at 20). Padding lanes (weight 0) are dropped.
            let f3 = |e: usize| [round(r.f32(e)), round(r.f32(e + 4)), round(r.f32(e + 8))];
            let mut rows_of = |batch: &str, single: &str| -> Vec<Value> {
                let mut out = Vec::new();
                if let Some((a, n, s)) = arr(batch) {
                    for i in 0..n {
                        let e = a + i * s;
                        for l in 0..16 {
                            let w = r.f32(e + 288 + l * 4);
                            if w != 0.0 {
                                let v = f3(e + l * 16);
                                out.push(json!([v[0], v[1], v[2], r.u16(e + 256 + l * 2), round(w)]));
                            }
                        }
                    }
                }
                if let Some((a, n, s)) = arr(single) {
                    for i in 0..n {
                        let e = a + i * s;
                        let w = r.f32(e + 20);
                        if w != 0.0 {
                            let v = f3(e);
                            out.push(json!([v[0], v[1], v[2], r.u16(e + 16), round(w)]));
                        }
                    }
                }
                out
            };
            json!({
                "kind": "volume",
                "frames": rows_of("frameBatchDatas", "frameSingleDatas"),
                "apply": rows_of("applyBatchDatas", "applySingleDatas"),
            })
        }
        other => json!({"kind": other}),
    }
}

/// Every cloth of a `*_c.hkx` for the model whose FLVER is `flver`. Display buffers are matched to
/// FLVER meshes by node name + vertex count and checked against the FLVER bind positions.
pub fn export(flver: &crate::flver::Flver, d: &[u8]) -> Value {
    let tf = Tagfile::new(d, None);
    let r = Reader::new(d);
    let bones = bone_names(&tf);
    let bind = bind_world(&tf);
    let cd = "hclClothData";
    let ptr_list = |at: usize| -> Vec<(String, usize)> { tf.array(at).map(|(a, n, _)| (0..n).filter_map(|i| tf.deref_obj(a + i * 8)).collect()).unwrap_or_default() };
    let mut cloths = Vec::new();
    for o in tf.objects_of(cd) {
        let name = tf.string(o + tf.field(cd, "name"));
        let buffers = ptr_list(o + tf.field(cd, "bufferDefinitions"));
        let buf_name = |i: i32| buffers.get(i as usize).map(|(t, p)| tf.string(p + tf.field(t, "name"))).unwrap_or_default();
        let buf_verts = |i: i32| buffers.get(i as usize).map(|(t, p)| r.i32(p + tf.field(t, "numVertices"))).unwrap_or(0).max(0) as usize;
        let ops = ptr_list(o + tf.field(cd, "operators"));
        let states = ptr_list(o + tf.field(cd, "clothStateDatas"));
        let Some((st_ty, st)) = states.iter().find(|(t, p)| tf.string(p + tf.field(t, "name")).starts_with("#01#")).or(states.first()) else { continue };
        let state_ops: Vec<usize> = tf.array(st + tf.field(st_ty, "operators")).map(|(a, n, _)| (0..n).map(|i| r.u32(a + i * 4) as usize).collect()).unwrap_or_default();
        // Every sim cloth of the container, each its own cloth (the second as "<name>#1": the
        // Corrupted Monk's / Emma's sleeves, the Owl's coat feathers and chainmail). Its operators:
        // Move / Simulate by simClothIndex, the deformers by their input buffer, the k-th
        // "SimBuffer" of the container (the one gathered from that sim).
        let sim_bufs: Vec<i32> = (0..buffers.len() as i32).filter(|&i| buf_name(i).contains("SimBuffer")).collect();
        let sims = ptr_list(o + tf.field(cd, "simClothDatas"));
        let multi = sims.len() > 1;
        let base_name = name.clone();
        // Each sim's MoveParticles reference buffer (simClothIndex, refBufferIdx): a deformer
        // writing one of them feeds that sim's fixed particles, it is no display (the Owl's
        // MeshDeform_2: the coat sim -> the coat feathers' reference buffer).
        let move_refs: Vec<(u32, i32)> = state_ops
            .iter()
            .filter_map(|&oi| ops.get(oi))
            .filter(|(ty, _)| ty == "hclMoveParticlesOperator")
            .map(|(ty, p)| (tf.try_field(ty, "simClothIndex").map(|f| r.u32(p + f)).unwrap_or(0), r.i32(p + tf.field(ty, "refBufferIdx"))))
            .collect();
        let sim_list = sims.clone();
        for (k, (sim_ty, sim)) in sims.into_iter().enumerate() {
            let k32 = k as u32;
            let name = if k == 0 { name.clone() } else { format!("{name}#{k}") };
            let sim_buf = sim_bufs.get(k).copied();
            // In a container of several sims, an op reading another sim's buffer is not this one's.
            let other_sim = |inp: i32| multi && sim_bufs.contains(&inp) && Some(inp) != sim_buf;
            let pose: Vec<[f32; 3]> = ptr_list(sim + tf.field(&sim_ty, "simClothPoses"))
                .first()
                .and_then(|(t, p)| tf.array(p + tf.field(t, "positions")))
                .map(|(a, n, s)| (0..n).map(|i| [r.f32(a + i * s), r.f32(a + i * s + 4), r.f32(a + i * s + 8)]).collect())
                .unwrap_or_default();
            let pd = tf.array(sim + tf.field(&sim_ty, "particleDatas"));
            let particles: Vec<Value> = pose
                .iter()
                .enumerate()
                .map(|(i, q)| {
                    // hclSimClothData::ParticleData: mass, invMass, radius, friction.
                    let (inv, rad, fr) = pd.map(|(a, _, s)| (r.f32(a + i * s + 4), r.f32(a + i * s + 8), r.f32(a + i * s + 12))).unwrap_or((1.0, 0.01, 0.0));
                    json!([round(q[0]), round(q[1]), round(q[2]), round(inv), round(rad), round(fr)])
                })
                .collect();
            let tris: Vec<[u16; 3]> = u16s(&tf, sim + tf.field(&sim_ty, "triangleIndices")).chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect();
            let si = sim + tf.field(&sim_ty, "simulationInfo");
            let sets: Vec<Value> = ptr_list(sim + tf.field(&sim_ty, "staticConstraintSets")).iter().map(|(t, p)| constraint_set(&tf, t, *p)).collect();
            let mut c = json!({
                "name": name,
                "gravity": [round(r.f32(si)), round(r.f32(si + 4)), round(r.f32(si + 8))],
                "damping": round(r.f32(si + 16)),
                "particles": particles,
                "tris": tris,
                "constraints": sets,
                "substeps": 1, "iterations": 1, "execution": [],
            });
            // Collision capsules: SimClothData.perInstanceCollidables (hclCollidable*) placed by
            // collidableTransformMap (u32 bone index + offset matrix per collidable); staticCollisionMasks
            // has one bit per collidable for every particle.
            let ctm = sim + tf.field(&sim_ty, "collidableTransformMap");
            let ctm_ty = "hclSimClothData::CollidableTransformMap";
            let ctm_bones: Vec<u32> = tf.array(ctm + tf.field(ctm_ty, "transformIndices")).map(|(a, n, _)| (0..n).map(|i| r.u32(a + i * 4)).collect()).unwrap_or_default();
            let ctm_offsets = m4s(&tf, ctm + tf.field(ctm_ty, "offsets"));
            let v3 = |at: usize| json!([round(r.f32(at)), round(r.f32(at + 4)), round(r.f32(at + 8))]);
            let mut colls = Vec::new();
            for (i, (_, cp)) in ptr_list(sim + tf.field(&sim_ty, "perInstanceCollidables")).into_iter().enumerate() {
                let ct = "hclCollidable";
                let cname = tf.string(cp + tf.field(ct, "name"));
                let bone = ctm_bones.get(i).and_then(|&b| bones.get(b as usize)).cloned();
                let shape = match tf.deref_obj(cp + tf.field(ct, "shape")) {
                    Some((t, sp)) if t == "hclCapsuleShape" => json!({"kind": "capsule", "a": v3(sp + tf.field(&t, "start")), "b": v3(sp + tf.field(&t, "end")), "ra": round(r.f32(sp + tf.field(&t, "radius"))), "rb": round(r.f32(sp + tf.field(&t, "radius")))}),
                    Some((t, sp)) if t == "hclTaperedCapsuleShape" => json!({"kind": "capsule", "a": v3(sp + tf.field(&t, "small")), "b": v3(sp + tf.field(&t, "big")), "ra": round(r.f32(sp + tf.field(&t, "smallRadius"))), "rb": round(r.f32(sp + tf.field(&t, "bigRadius")))}),
                    Some((t, _)) => json!({"kind": t}),
                    None => json!({"kind": "none"}),
                };
                println!("  cloth {name}: collidable {cname} on {} {}", bone.as_deref().unwrap_or("?"), shape["kind"]);
                let off = ctm_offsets.get(i).copied().unwrap_or(IDENT);
                colls.push(json!({"name": cname, "bone": bone, "offset": off.iter().map(|col| v4(*col)).collect::<Vec<_>>(), "shape": shape}));
            }
            c["collidables"] = json!(colls);
            c["masks"] = json!(tf.array(sim + tf.field(&sim_ty, "staticCollisionMasks")).map(|(a, n, _)| (0..n).map(|i| r.u32(a + i * 4)).collect::<Vec<_>>()).unwrap_or_default());
            // Transfer motion: how much of a bone's world motion the sim inherits (speed-blended).
            let tm = sim + tf.field(&sim_ty, "transferMotionData");
            let tmt = "hclSimClothData::TransferMotionData";
            let tf32 = |f: &str| round(r.f32(tm + tf.field(tmt, f)));
            c["transfer"] = json!({
                "enabled": r.u8(sim + tf.field(&sim_ty, "transferMotionEnabled")),
                "bone": bones.get(r.u32(tm + tf.field(tmt, "transformIndex")) as usize),
                "translation": r.u8(tm + tf.field(tmt, "transferTranslationMotion")),
                "minTranslationSpeed": tf32("minTranslationSpeed"), "maxTranslationSpeed": tf32("maxTranslationSpeed"),
                "minTranslationBlend": tf32("minTranslationBlend"), "maxTranslationBlend": tf32("maxTranslationBlend"),
                "rotation": r.u8(tm + tf.field(tmt, "transferRotationMotion")),
                "minRotationSpeed": tf32("minRotationSpeed"), "maxRotationSpeed": tf32("maxRotationSpeed"),
                "minRotationBlend": tf32("minRotationBlend"), "maxRotationBlend": tf32("maxRotationBlend"),
            });
            let mut ref_buf = -1;
            let mut displays = Vec::new();
            let mut bone_out = Vec::new();
            let mut skin_ops: Vec<(i32, Entries)> = Vec::new();
            for &oi in &state_ops {
                let Some((ty, p)) = ops.get(oi) else { continue };
                let (ty, p) = (ty.as_str(), *p);
                if ty.contains("Skin") && !ty.contains("MeshMesh") {
                    skin_ops.push((r.i32(p + tf.field(ty, "outputBufferIndex")), skin_entries(&tf, p, ty)));
                } else if ty == "hclMoveParticlesOperator" {
                    // The operator of this container's first sim cloth only (c1060's mino and the
                    // handmaidens' sleeve containers hold a second sim cloth whose operator came
                    // last and overwrote the pairs: 406 pairs for 59 particles restarted the cloth
                    // every frame). Pairs beyond the particle count are dropped either way.
                    let sim_index = tf.try_field(ty, "simClothIndex").map(|f| r.u32(p + f)).unwrap_or(0);
                    if sim_index != k32 || !c["move"].is_null() {
                        continue;
                    }
                    let n_particles = particles.len() as u16;
                    ref_buf = r.i32(p + tf.field(ty, "refBufferIdx"));
                    let pairs: Vec<Value> = tf.array(p + tf.field(ty, "vertexParticlePairs")).map(|(a, n, s)| (0..n).map(|i| (r.u16(a + i * s), r.u16(a + i * s + 2))).filter(|&(_, pi)| pi < n_particles).map(|(v, pi)| json!([v, pi])).collect()).unwrap_or_default();
                    c["move"] = json!(pairs);
                } else if ty == "hclSimulateOperator" {
                    if tf.try_field(ty, "simClothIndex").map(|f| r.u32(p + f)).unwrap_or(0) != k32 {
                        continue;
                    }
                    if let Some((a, _, _)) = tf.array(p + tf.field(ty, "simulateOpConfigs")) {
                        let ct = "hclSimulateOperator::Config";
                        let exec: Vec<i32> = tf.array(a + tf.field(ct, "constraintExecution")).map(|(e, n, _)| (0..n).map(|i| r.i32(e + i * 4)).collect()).unwrap_or_default();
                        c["substeps"] = json!(r.u8(a + tf.field(ct, "subSteps")));
                        c["iterations"] = json!(r.u8(a + tf.field(ct, "numberOfSolveIterations")));
                        c["execution"] = json!(exec);
                    }
                } else if ty.contains("MeshMeshDeform") {
                    let out = r.i32(p + tf.field(ty, "outputBufferIdx"));
                    let inp = r.i32(p + tf.field(ty, "inputBufferIdx"));
                    if other_sim(inp) {
                        continue;
                    }
                    if move_refs.iter().any(|&(si, b)| si != k32 && b == out) {
                        println!("  cloth {name}: {ty} -> buffer {out} ({}) feeds another sim's fixed particles: not a display", buf_name(out));
                        continue;
                    }
                    println!("  cloth {name}: {ty} reads buffer {inp} ({})", buf_name(inp));
                    let entries = mesh_deform_entries(&tf, p, ty);
                    let (bname, nv) = (buf_name(out), buf_verts(out));
                    let mut best: Option<(usize, f32)> = None;
                    for (mi, m) in flver.meshes.iter().enumerate() {
                        let node = flver.nodes.get(m.node.max(0) as usize).map(|n| n.name.as_str()).unwrap_or("");
                        if node != bname || m.vertices.len() != nv {
                            continue;
                        }
                        let mut worst = 0f32;
                        for (vi, slots) in &entries {
                            let mut acc = [0.0f32; 4];
                            for (t, l, _) in slots {
                                if let Some(f) = frame_of(&tris, &pose, *t) {
                                    add4(&mut acc, m4_apply(&f, *l));
                                }
                            }
                            if let Some(v) = m.vertices.get(*vi as usize) {
                                worst = worst.max((0..3).map(|k| (v.pos[k] - acc[k]).abs()).fold(0.0, f32::max));
                            }
                        }
                        if best.is_none_or(|b| worst < b.1) {
                            best = Some((mi, worst));
                        }
                    }
                    let Some((mesh, err)) = best else {
                        println!("  cloth {name}: no FLVER mesh for display buffer {bname} ({nv} verts)");
                        continue;
                    };
                    println!("  cloth {name}: {ty} -> mesh {mesh} ({bname}, {nv} verts, {} deformed), bind err {err:.4}", entries.len());
                    let mut seen = std::collections::HashSet::new();
                    let verts: Vec<Value> = entries
                        .iter()
                        .filter(|(v, _)| seen.insert(*v))
                        .map(|(v, slots)| json!([v, slots.iter().map(|(t, l, n)| json!([t, v4(*l), v4(*n)])).collect::<Vec<_>>()]))
                        .collect();
                    displays.push(json!({"mesh": mesh, "verts": verts}));
                } else if ty == "hclSimpleMeshBoneDeformOperator" {
                    if other_sim(r.i32(p + tf.field(ty, "inputBufferIdx"))) {
                        continue;
                    }
                    let locals = m4s(&tf, p + tf.field(ty, "localBoneTransforms"));
                    if let Some((a, n, s)) = tf.array(p + tf.field(ty, "triangleBonePairs")) {
                        for i in 0..n {
                            // boneOffset / triangleOffset are byte offsets (hkMatrix4 = 64, triangle = 3 x u16).
                            let b = r.u16(a + i * s) as usize / 64;
                            let t = r.u16(a + i * s + 2) / 6;
                            let l = locals.get(i).copied().unwrap_or(IDENT);
                            let err = frame_of(&tris, &pose, t).zip(bind.get(b)).map(|(f, bw)| {
                                let m = m4_mul(&f, &l);
                                (0..4).flat_map(|c| (0..3).map(move |k| (c, k))).map(|(c, k)| (m[c][k] - bw[c][k]).abs()).fold(0.0f32, f32::max)
                            });
                            println!("  cloth {name}: bone {} <- tri {t}, bind err {:.4}", bones.get(b).map(String::as_str).unwrap_or("?"), err.unwrap_or(-1.0));
                            // A pair that does not reproduce its bone's bind pose (c1020 sholderarmor's
                            // second pair -> Master, err 2.05) is padding, not a driven bone.
                            if err.is_none_or(|e| e > 0.01) {
                                continue;
                            }
                            bone_out.push(json!({"bone": bones.get(b), "tri": t, "local": l.iter().map(|col| v4(*col)).collect::<Vec<_>>()}));
                        }
                    }
                }
            }
            // No MeshMeshDeform display: the sim mesh is the drawn mesh itself (the Guardian Ape's
            // 12 fur cloths: the sim buffer is gathered straight into the display buffer of the
            // FLVER mesh's name and converted out, no deformer). The gather is a Copy op (vertex i
            // = particle i) or a GatherAllVertices op (vertexInputFromVertexOutput: neckfur's 796
            // display vertices from 762 particles, seam duplicates). Emitted as a display whose
            // vertex rides its particle's own triangle at a corner of the centroid frame (local
            // (1,0,0), (0,1,0) or (-1,-1,0): exact for any deformation), with the FLVER bind
            // normal taken into that frame.
            if displays.is_empty() && !tris.is_empty() {
                let n = pose.len();
                let is_sim = |i: i32| buf_name(i).contains("SimBuffer");
                for &oi in &state_ops {
                    let Some((ty, p)) = ops.get(oi) else { continue };
                    let (ty, p) = (ty.as_str(), *p);
                    if ty != "hclCopyVerticesOperator" && ty != "hclGatherAllVerticesOperator" {
                        continue;
                    }
                    let (inp, out) = (r.u32(p + tf.field(ty, "inputBufferIdx")) as i32, r.u32(p + tf.field(ty, "outputBufferIdx")) as i32);
                    let (bname, nv) = (buf_name(out), buf_verts(out));
                    if !is_sim(inp) || other_sim(inp) || is_sim(out) || bname.starts_with("Previous Positions") || bname.is_empty() {
                        continue;
                    }
                    let map: Vec<Option<u16>> = if ty == "hclGatherAllVerticesOperator" {
                        u16s(&tf, p + tf.field(ty, "vertexInputFromVertexOutput")).into_iter().map(|i| (usize::from(i) < n).then_some(i)).collect()
                    } else {
                        (0..nv.min(n) as u16).map(Some).collect()
                    };
                    let Some(mi) = flver.meshes.iter().enumerate().position(|(_, m)| {
                        m.vertices.len() == nv
                            && flver.nodes.get(m.node.max(0) as usize).is_some_and(|nd| nd.name == bname)
                            && m.vertices.iter().zip(&map).all(|(v, pi)| pi.is_none_or(|pi| (0..3).all(|k| (v.pos[k] - pose[pi as usize][k]).abs() < 0.01)))
                    }) else {
                        println!("  cloth {name}: no FLVER mesh for the gathered sim buffer {bname} ({nv} verts)");
                        continue;
                    };
                    let corner = [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [-1.0, -1.0, 0.0, 1.0]];
                    let verts: Vec<Value> = map
                        .iter()
                        .enumerate()
                        .filter_map(|(v, pi)| {
                            let pi = (*pi)?;
                            let (t, k) = tris.iter().enumerate().find_map(|(t, tri)| tri.iter().position(|&c| c == pi).map(|k| (t as u16, k)))?;
                            let f = frame_of(&tris, &pose, t)?;
                            let ln = inv3_apply(&f, flver.meshes[mi].vertices[v].normal)?;
                            Some(json!([v, [json!([t, v4(corner[k]), v4([ln[0], ln[1], ln[2], 0.0])])]]))
                        })
                        .collect();
                    println!("  cloth {name}: sim mesh is the display -> mesh {mi} ({bname}: {} of {nv} verts on a particle's triangle, {ty})", verts.len());
                    displays.push(json!({"mesh": mi, "verts": verts}));
                    break;
                }
            }
            // Reference buffer skin: drives the fixed particles and the local-range / transition targets.
            let mut refv: Vec<Value> = vec![Value::Null; buf_verts(ref_buf)];
            let mut ref_err = 0f32;
            for (_, entries) in skin_ops.iter().filter(|(o, _)| *o == ref_buf) {
                for (vi, bs) in entries {
                    let mut acc = [0.0f32; 4];
                    for (b, l, _) in bs {
                        add4(&mut acc, m4_apply(bind.get(*b as usize).unwrap_or(&IDENT), *l));
                    }
                    if let Some(q) = pose.get(*vi as usize).filter(|_| refv.len() == pose.len()) {
                        ref_err = ref_err.max((0..3).map(|k| (q[k] - acc[k]).abs()).fold(0.0, f32::max));
                    }
                    if let Some(slot) = refv.get_mut(*vi as usize) {
                        *slot = json!(bs.iter().map(|(b, l, n)| json!([bones.get(*b as usize), v4(*l), v4(*n)])).collect::<Vec<_>>());
                    }
                }
            }
            // A later sim whose reference buffer no skin op fills is driven by another sim's output
            // (the Owl's coat feathers c5060_coat#1: MeshDeform_2 of the coat sim writes their
            // reference buffer); chained cloths are not supported: left out, the first sim's
            // deformer keeps carrying that mesh. gap.
            // A sim whose reference buffer no skin op fills follows another sim: the deformer
            // writing that buffer from the other sim's SimBuffer (the Owl's coat feathers
            // c5060_coat#1 <- the coat's MeshDeform_2). Exported as "ref_deform": the source
            // cloth and, per reference vertex, its triangle slots in the source sim (cloth.rs
            // computes the references from that cloth's particles each frame).
            if k > 0 && refv.iter().all(Value::is_null) {
                let feed = state_ops.iter().filter_map(|&oi| ops.get(oi)).find(|(ty, p)| ty.contains("MeshMeshDeform") && r.i32(p + tf.field(ty, "outputBufferIdx")) == ref_buf);
                let src = feed.and_then(|(ty, p)| {
                    let inp = r.i32(p + tf.field(ty, "inputBufferIdx"));
                    let j = sim_bufs.iter().position(|&b| b == inp)?;
                    (j != k).then_some((j, ty.as_str(), *p))
                });
                let Some((j, fty, fp)) = src else {
                    println!("  cloth {name}: reference buffer {ref_buf} ({}) is not skinned and no other sim feeds it: left out", buf_name(ref_buf));
                    continue;
                };
                let (src_ty, src_sim) = &sim_list[j];
                let src_pose: Vec<[f32; 3]> = ptr_list(src_sim + tf.field(src_ty, "simClothPoses"))
                    .first()
                    .and_then(|(t, p)| tf.array(p + tf.field(t, "positions")))
                    .map(|(a, n, s)| (0..n).map(|i| [r.f32(a + i * s), r.f32(a + i * s + 4), r.f32(a + i * s + 8)]).collect())
                    .unwrap_or_default();
                let src_tris: Vec<[u16; 3]> = u16s(&tf, src_sim + tf.field(src_ty, "triangleIndices")).chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect();
                let entries = mesh_deform_entries(&tf, fp, fty);
                // How well the source sim's bind pose through the deformer lands on this sim's
                // fixed particles.
                let mut err = 0f32;
                let at: std::collections::HashMap<u16, [f32; 4]> = entries
                    .iter()
                    .map(|(v, slots)| {
                        let mut acc = [0.0f32; 4];
                        for (t, l, _) in slots {
                            if let Some(f) = frame_of(&src_tris, &src_pose, *t) {
                                add4(&mut acc, m4_apply(&f, *l));
                            }
                        }
                        (*v, acc)
                    })
                    .collect();
                for m in c["move"].as_array().cloned().unwrap_or_default() {
                    let (v, pi) = (m[0].as_u64().unwrap_or(0) as u16, m[1].as_u64().unwrap_or(0) as usize);
                    if let (Some(a), Some(q)) = (at.get(&v), pose.get(pi)) {
                        err = err.max((0..3).map(|i| (a[i] - q[i]).abs()).fold(0.0, f32::max));
                    }
                }
                let src_name = if j == 0 { base_name.clone() } else { format!("{base_name}#{j}") };
                println!("  cloth {name}: references from cloth {src_name} through {fty} ({} verts), fixed particle bind err {err:.4}", entries.len());
                let mut seen = std::collections::HashSet::new();
                let verts: Vec<Value> = entries
                    .iter()
                    .filter(|(v, _)| seen.insert(*v))
                    .map(|(v, slots)| json!([v, slots.iter().map(|(t, l, n)| json!([t, v4(*l), v4(*n)])).collect::<Vec<_>>()]))
                    .collect();
                c["ref_deform"] = json!({"cloth": src_name, "verts": verts});
            }
            println!("  cloth {name}: {} particles, {} ref verts (bind err {ref_err:.4}), {} displays, {} bones", pose.len(), refv.len(), displays.len(), bone_out.len());
            c["ref"] = json!(refv);
            c["displays"] = json!(displays);
            c["bones"] = json!(bone_out);
            cloths.push(c);
        }
    }
    json!({"cloths": cloths})
}
