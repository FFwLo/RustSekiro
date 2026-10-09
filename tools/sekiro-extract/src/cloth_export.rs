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
        let Some((sim_ty, sim)) = ptr_list(o + tf.field(cd, "simClothDatas")).into_iter().next() else { continue };
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
                let (inv, rad) = pd.map(|(a, _, s)| (r.f32(a + i * s + 4), r.f32(a + i * s + 8))).unwrap_or((1.0, 0.01));
                json!([round(q[0]), round(q[1]), round(q[2]), round(inv), round(rad)])
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
                ref_buf = r.i32(p + tf.field(ty, "refBufferIdx"));
                let pairs: Vec<Value> = tf.array(p + tf.field(ty, "vertexParticlePairs")).map(|(a, n, s)| (0..n).map(|i| json!([r.u16(a + i * s), r.u16(a + i * s + 2)])).collect()).unwrap_or_default();
                c["move"] = json!(pairs);
            } else if ty == "hclSimulateOperator" {
                if let Some((a, _, _)) = tf.array(p + tf.field(ty, "simulateOpConfigs")) {
                    let ct = "hclSimulateOperator::Config";
                    let exec: Vec<i32> = tf.array(a + tf.field(ct, "constraintExecution")).map(|(e, n, _)| (0..n).map(|i| r.i32(e + i * 4)).collect()).unwrap_or_default();
                    c["substeps"] = json!(r.u8(a + tf.field(ct, "subSteps")));
                    c["iterations"] = json!(r.u8(a + tf.field(ct, "numberOfSolveIterations")));
                    c["execution"] = json!(exec);
                }
            } else if ty.contains("MeshMeshDeform") {
                let out = r.i32(p + tf.field(ty, "outputBufferIdx"));
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
        println!("  cloth {name}: {} particles, {} ref verts (bind err {ref_err:.4}), {} displays, {} bones", pose.len(), refv.len(), displays.len(), bone_out.len());
        c["ref"] = json!(refv);
        c["displays"] = json!(displays);
        c["bones"] = json!(bone_out);
        cloths.push(c);
    }
    json!({"cloths": cloths})
}
