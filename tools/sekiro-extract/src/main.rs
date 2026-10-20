//! sekiro-extract: pulls combat data out of the user's own Sekiro install.
//!
//!   sekiro-extract unpack <sekiro_dir> <out_dir> <regex>
//!       Extracts every dictionary path matching <regex>. DCX files are
//!       decompressed; BND4 binders are also expanded into "<name>.d/".
//!
//!   sekiro-extract params <gameparam.parambnd.d> <out_dir>
//!       Decodes every .param with the Paramdex defs in defs/paramdef into JSON.
//!
//!   sekiro-extract tae <anibnd.d> <out_dir>
//!       Decodes every .tae with defs/TAE.Template.SDT.xml into JSON.
//!
//!   sekiro-extract model <x.flver> <x.tpf|-> <out.bin> <texture_dir>
//!       Skinned mesh (bind-pose nodes, LOD0 triangles, weights) + albedo DDS textures.
//!
//!   sekiro-extract anims <x.anibnd.d> <out.bin> [suffix]
//!       One anibnd's skeleton and clips as an anim bin (keys: file stems minus suffix).
//!
//!   sekiro-extract export <extracted_dir>
//!       Writes <extracted_dir>/combat_data.json for the game (needs unpack + params first).
//!
//! Output is for local reference only: never commit or publish it.

mod anims;
mod bhd;
mod bin;
mod cloth;
mod cubemap;
mod cloth_export;
mod container;
mod export;
mod flver;
mod fmg;
mod fev;
mod fsb;
mod gparam;
mod hkx;
mod map;
mod msb;
mod mtd;
mod param;
mod sky;
mod spline;
mod tae;

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

/// The extractor's own files (keys/, defs/, export_states.txt): next to the exe in a release,
/// else the crate folder.
fn tool_dir() -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from));
    match exe_dir {
        Some(d) if d.join("keys").is_dir() => d,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    }
}

fn keys_dir() -> String {
    tool_dir().join("keys").to_string_lossy().into_owned()
}

fn defs_dir() -> String {
    tool_dir().join("defs").to_string_lossy().into_owned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("unpack") if args.len() == 5 => unpack(Path::new(&args[2]), Path::new(&args[3]), &args[4]),
        Some("params") if args.len() == 4 => dump_params(Path::new(&args[2]), Path::new(&args[3])),
        Some("tae") if args.len() == 4 => dump_tae(Path::new(&args[2]), Path::new(&args[3])),
        // Optional further args: extra texture binder folders (NpcParam normalChangeTexChrId, e.g.
        // c5430 -> chr/c5408.texbnd.d; materials may name a sibling pack, c5400 hair -> c5408).
        // One object's FLVER as JSON for the game to draw on its own (src/idol.rs): its meshes'
        // positions / normals / UVs / indices in FromSoftware space, the albedo and normal texture
        // stems (extracted/tex), and its dummies.
        Some("objmesh") if args.len() == 4 => export_objmesh(Path::new(&args[2]), Path::new(&args[3])),
        Some("model") if args.len() >= 6 => {
            let extra: Vec<&Path> = args[6..].iter().map(Path::new).collect();
            export_model(Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4]), Path::new(&args[5]), &extra)
        }
        // DDS (first mip) -> PNG; "alpha" writes the alpha channel as grey.
        Some("tpf") if args.len() >= 3 => {
            // List a TPF's textures (name, size, DDS header facts); with an out dir, write the DDS files.
            for (name, dds) in flver::tpf(&std::fs::read(&args[2]).unwrap()) {
                let u32_ = |o: usize| u32::from_le_bytes(dds[o..o + 4].try_into().unwrap());
                let dx10 = dds.len() >= 148 && &dds[84..88] == b"DX10";
                println!(
                    "{name}: {} bytes, {}x{}, mips {}, {} {}",
                    dds.len(),
                    u32_(16),
                    u32_(12),
                    u32_(28),
                    if dx10 { format!("dxgi {}", u32_(128)) } else { String::from_utf8_lossy(&dds[84..88]).into_owned() },
                    if dx10 { format!("dim {} misc {:#x} arraysize {}", u32_(132), u32_(136), u32_(140)) } else { String::new() }
                );
                if let Some(out) = args.get(3) {
                    std::fs::create_dir_all(out).unwrap();
                    std::fs::write(Path::new(out).join(format!("{name}.dds")), &dds).unwrap();
                }
            }
        }
        Some("gparam") if args.len() == 3 => {
            // A draw-param file as JSON (the map export's "drawparam"; the per-fight variants
            // `m11_01_0<T><SS>.gparam` the events select with Set Area Gparam Sub ID, kb/map.md).
            println!("{}", gparam::to_json(&gparam::read(&std::fs::read(&args[2]).unwrap())));
        }
        Some("sky") if args.len() == 3 => {
            // The sky library's domes (m89_00_00_99 m900000..070): sky/sky.json + textures.
            sky::export(Path::new(&args[2]));
        }
        Some("mtd") if args.len() == 3 => {
            // An MTD's shader, params and texture slots (map material layers: kb/map.md).
            let Some(m) = mtd::read(&std::fs::read(&args[2]).unwrap()) else {
                println!("not an MTD");
                return;
            };
            println!("shader {}", m.shader);
            for (k, v) in &m.ints {
                println!("  int {k} = {v:?}");
            }
            for (k, v) in &m.floats {
                println!("  float {k} = {v:?}");
            }
            for t in &m.textures {
                println!("  slot {} uv {} path {:?}", t.kind, t.uv_number, t.path);
            }
        }
        Some("vblend") if args.len() == 3 => {
            // Per mesh: range and mean of the six UByte4Norm "UV" members (layer blend weights)
            // and the second UV set, to see which bytes a multi-layer material uses.
            let f = flver::read(&std::fs::read(&args[2]).unwrap());
            for m in &f.meshes {
                let n = m.vertices.len().max(1) as f32;
                let mut lo = [[255u8; 4]; 6];
                let mut hi = [[0u8; 4]; 6];
                let mut sum = [[0f32; 4]; 6];
                let (mut u2lo, mut u2hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                for v in &m.vertices {
                    for k in 0..6 {
                        for c in 0..4 {
                            lo[k][c] = lo[k][c].min(v.blend[k][c]);
                            hi[k][c] = hi[k][c].max(v.blend[k][c]);
                            sum[k][c] += v.blend[k][c] as f32;
                        }
                    }
                    u2lo = [u2lo[0].min(v.uv2[0]), u2lo[1].min(v.uv2[1])];
                    u2hi = [u2hi[0].max(v.uv2[0]), u2hi[1].max(v.uv2[1])];
                }
                println!("mesh {} [{}] verts {} uv2 {:?}..{:?}", m.material, m.mtd, m.vertices.len(), u2lo, u2hi);
                for k in 0..6 {
                    if hi[k] != [0; 4] {
                        println!("  byte{k}: min {:?} max {:?} mean {:?}", lo[k], hi[k], sum[k].map(|x| (x / n) as u32));
                    }
                }
            }
        }
        Some("flverinfo") if args.len() == 3 => {
            let f = flver::read(&std::fs::read(&args[2]).unwrap());
            for m in &f.meshes {
                println!("mesh {} [{}] verts {} tris {} layouts {:?}", m.material, m.mtd, m.vertices.len(), m.indices.len() / 3, m.layouts);
                println!("  slots {:?}", m.textures);
                let uv: Vec<[f32; 2]> = m.vertices.iter().step_by((m.vertices.len() / 6).max(1)).map(|v| v.uv).collect();
                println!("  uv {:?}", uv);
            }
            // Dummy polys (model space unless parented): `dummy <id> <x> <y> <z> parent <bone>`.
            for d in &f.dummies {
                println!("dummy {} {} {} {} parent {}", d.id, d.pos[0], d.pos[1], d.pos[2], d.parent);
            }
            for (i, n) in f.nodes.iter().enumerate() {
                println!("node {i} {} parent {} t {:?} r {:?} s {:?} flags {}", n.name, n.parent, n.t, n.r, n.s, n.flags);
            }
        }
        // The Env_Box regions (GI probe boxes) within <metres> of a map point, their distance
        // and whether it is inside (`envboxes <msb> x y z [metres]`).
        Some("envboxes") if args.len() >= 6 => {
            let m = msb::read(&std::fs::read(&args[2]).unwrap());
            let p: [f32; 3] = std::array::from_fn(|i| args[3 + i].parse().unwrap());
            let within: f32 = args.get(6).map_or(40.0, |v| v.parse().unwrap());
            for o in m.regions.iter().filter(|r| r.kind == 17 && r.shape == 5) {
                let l = msb::rotate([0.0, -o.rot[1], 0.0], [p[0] - o.pos[0], p[1] - o.pos[1], p[2] - o.pos[2]]);
                let out = [(l[0].abs() - o.size[0] * 0.5).max(0.0), (-l[1]).max(l[1] - o.size[2]).max(0.0), (l[2].abs() - o.size[1] * 0.5).max(0.0)];
                let dist = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt();
                if dist < within {
                    let e = o.env.unwrap_or_default();
                    let f = |w: u32| f32::from_bits(w);
                    println!("{} {:?} rot {:?} dist {dist:.1} local {:?}", o.name, o.size, o.rot, l);
                    println!("    env T00 {} T04 {} T08 {} T09 {} T0A {} words {:08x?} T24 {} T28 {} T2C {}", f(e[0]), f(e[1]), e[2] & 0xff, (e[2] >> 8) & 0xff, e[2] >> 16, &e[3..9], e[9] as i32, f(e[10]), f(e[11]));
                }
            }
        }
        Some("dds2png") if args.len() >= 4 => dds2png(Path::new(&args[2]), Path::new(&args[3]), args.get(4).is_some_and(|a| a == "alpha")),
        // Whole maps' grapple points and ledges (`edges extracted m11_00_00_00 m11_01_00_00`).
        Some("edges") if args.len() >= 4 => map::export_edges(Path::new(&args[2]), &args[3..]),
        // Map pieces around an MSB part (e.g. `map extracted m11_01_00_00 c1020_0004 60`).
        Some("map") if args.len() == 6 => map::export(Path::new(&args[2]), &args[3], &args[4], args[5].parse().expect("radius")),
        // Split binder (.tpfbhd / .hkxbhd + its .*bdt) -> <out>/<file name>.
        Some("bxf") if args.len() == 4 => {
            let bhd = std::fs::read(&args[2]).expect("read bhd");
            let bdt = std::fs::read(args[2].replace("bhd", "bdt")).expect("read bdt");
            for f in container::read_bxf4(&bhd, &bdt) {
                let name = f.name.rsplit(['\\', '/']).next().unwrap_or("").trim_end_matches(".dcx").to_string();
                write(&Path::new(&args[3]).join(&name), &f.data);
            }
        }
        // Members of named types (parents included) from a tagfile / compendium.
        Some("hktypes") if args.len() >= 4 => {
            let t = hkx::read_types(&std::fs::read(&args[2]).unwrap());
            for name in &args[3..] {
                let Some(mut ty) = t.index_of(name) else { println!("{name}: none"); continue };
                while ty != 0 {
                    println!("{} (parent {}): {:?}", t.names[ty], t.parents.get(&ty).map(|p| t.names[*p].as_str()).unwrap_or(""), t.members.get(&ty));
                    ty = *t.parents.get(&ty).unwrap_or(&0);
                }
            }
        }
        Some("hkxdump") if args.len() == 3 => hkx::dump(&std::fs::read(&args[2]).unwrap(), None),
        // Map collision files keep their types in the binder's .compendium.
        Some("hkxdump") if args.len() == 4 => hkx::dump(&std::fs::read(&args[2]).unwrap(), Some(&hkx::read_types(&std::fs::read(&args[3]).unwrap()))),
        // A map collision part's mesh in its own space: counts, bounds and hit materials.
        Some("hitmesh") if args.len() == 4 => {
            let m = hkx::collision_mesh(&std::fs::read(&args[2]).unwrap(), &hkx::read_types(&std::fs::read(&args[3]).unwrap()));
            let (lo, hi) = m.vertices.iter().fold(([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]), |(lo, hi), v| (std::array::from_fn(|i| lo[i].min(v[i])), std::array::from_fn(|i| hi[i].max(v[i]))));
            let mats: std::collections::BTreeSet<u32> = m.materials.iter().copied().collect();
            println!("{} vertices, {} triangles, bounds {lo:?}..{hi:?}, materials {mats:?}", m.vertices.len(), m.indices.len() / 3);
        }
        // Dummy poly frames (id, attach bone, model-space position / forward / upward) as JSON:
        // the sidecar model_<chr>.dummies.json (throw absorb directions).
        // One anibnd's skeleton + clips as an anim bin; keys = file stems minus [suffix].
        Some("anims") if args.len() >= 4 => anims::export(Path::new(&args[2]), Path::new(&args[3]), args.get(4).map(String::as_str).unwrap_or("")),
        Some("dummies") if args.len() == 4 => {
            let f = flver::read(&std::fs::read(&args[2]).expect("read flver"));
            let v: Vec<serde_json::Value> = f
                .dummies
                .iter()
                .map(|d| serde_json::json!({"id": d.id, "attach": d.attach, "pos": d.pos, "fwd": d.fwd, "up": d.up}))
                .collect();
            write(Path::new(&args[3]), serde_json::to_string(&v).unwrap().as_bytes());
        }
        Some("blenders") if args.len() == 3 => {
            for b in hkx::blenders(&std::fs::read(&args[2]).unwrap()) {
                println!("{b}");
            }
        }
        Some("objects") if args.len() == 5 => {
            let t = hkx::read_types(&std::fs::read(&args[4]).unwrap());
            for o in hkx::objects_with(&std::fs::read(&args[2]).unwrap(), Some(&t), &args[3]) {
                println!("{o}");
            }
        }
        Some("objects") if args.len() == 4 => {
            for b in hkx::objects(&std::fs::read(&args[2]).unwrap(), &args[3]) {
                println!("{b}");
            }
        }
        Some("cmsgs") if args.len() == 3 => {
            let mut v: Vec<_> = hkx::cmsg_map(&std::fs::read(&args[2]).unwrap()).into_iter().collect();
            v.sort();
            let ends: HashMap<String, (i32, i32)> = hkx::cmsg_ends(&std::fs::read(&args[2]).unwrap()).into_iter().map(|(n, k, e)| (n.trim_end_matches("_CMSG").to_string(), (k, e))).collect();
            for (n, (id, off)) in v {
                let (k, e) = ends.get(&n).copied().unwrap_or((-1, -1));
                println!("{n} animId={id} offsetType={off} animeEndEventType={k} endEvent={e}");
            }
        }
        Some("twistmods") if args.len() == 3 => {
            for b in hkx::twist_mods(&std::fs::read(&args[2]).unwrap()) {
                println!("{b}");
            }
        }
        Some("sms") if args.len() == 3 => {
            for b in hkx::state_machines(&std::fs::read(&args[2]).unwrap()) {
                println!("{b}");
            }
        }
        Some("layers") if args.len() == 3 => {
            for b in hkx::layers(&std::fs::read(&args[2]).unwrap()) {
                println!("{b}");
            }
        }
        Some("selectors") if args.len() == 3 => {
            for b in hkx::selectors(&std::fs::read(&args[2]).unwrap()) {
                println!("{b}");
            }
        }
        Some("behavior") if args.len() == 4 => {
            let graph = hkx::behavior_graph(&std::fs::read(&args[2]).unwrap());
            std::fs::write(&args[3], serde_json::to_string(&graph).unwrap()).unwrap();
        }
        // Every clip's length (seconds) in a character's anibnds (<chr>.anibnd.d, <chr>_*.anibnd.d):
        // what the behaviour graph needs to end the clips the combat export leaves out.
        Some("clip-lengths") if args.len() == 5 => {
            let mut out = std::collections::BTreeMap::new();
            for e in std::fs::read_dir(&args[2]).unwrap().flatten() {
                let dir = e.path();
                let name = dir.file_name().unwrap().to_string_lossy().to_string();
                if !(name == format!("{}.anibnd.d", args[3]) || name.starts_with(&format!("{}_", args[3]))) || !name.ends_with(".anibnd.d") {
                    continue;
                }
                let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|f| f.path()).collect();
                let types = files.iter().find(|f| f.extension().is_some_and(|x| x == "compendium")).map(|f| hkx::read_types(&std::fs::read(f).unwrap())).unwrap_or_default();
                for f in files.iter().filter(|f| f.extension().is_some_and(|x| x == "hkx")) {
                    let key = f.file_stem().unwrap().to_string_lossy().to_string();
                    if let Some(d) = hkx::clip_duration(&std::fs::read(f).unwrap(), &types) {
                        out.entry(key).or_insert(d);
                    }
                }
            }
            println!("{} clip lengths", out.len());
            std::fs::write(&args[4], serde_json::to_string(&out).unwrap()).unwrap();
        }
        Some("transitions") if args.len() == 3 => {
            for (ty, n, dur, flags, end_mode, curve) in hkx::transition_effects(&std::fs::read(&args[2]).unwrap()) {
                println!("{ty} {n} duration={dur} flags={flags} endMode={end_mode} blendCurve={curve}");
            }
        }
        Some("clips") if args.len() == 3 => {
            // Behavior clip generators that are not plain (speed 1, no crop/start/enforced duration).
            let all = hkx::clip_generators(&std::fs::read(&args[2]).unwrap());
            println!("{} clip generators", all.len());
            for (n, sp, cs, ce, st, ed, mode) in all {
                if std::env::var("ALL_CLIPS").is_ok() || sp != 1.0 || cs != 0.0 || ce != 0.0 || st != 0.0 || ed != 0.0 || mode != 0 {
                    println!("{n} speed={sp} crop={cs}/{ce} start={st} enforced={ed} mode={mode}");
                }
            }
        }
        Some("cloth") if args.len() == 3 => cloth::survey(&std::fs::read(&args[2]).unwrap()),
        Some("cloth-bs") if args.len() == 3 => cloth::check_bone_space(&std::fs::read(&args[2]).unwrap()),
        Some("cloth-check") if args.len() == 3 => cloth::check(&std::fs::read(&args[2]).unwrap()),
        Some("members") if args.len() == 4 => cloth::type_members(&std::fs::read(&args[2]).unwrap(), &args[3]),
        Some("ragdoll") if args.len() == 3 => {
            for c in hkx::ragdoll_capsules(&std::fs::read(&args[2]).unwrap()) {
                println!("{:<20} a {:?} b {:?} r {:.3}", c.name, c.a, c.b, c.radius);
            }
        }
        Some("sounds") if args.len() == 4 => fsb::export_all(Path::new(&args[2]), Path::new(&args[3])),
        Some("fev-list") if args.len() == 5 => match fev::list_events(Path::new(&args[2]), Path::new(&args[3]), &args[4]) {
            Ok(evs) => {
                println!("{} events", evs.len());
                for e in evs {
                    println!("{e}");
                }
            }
            Err(e) => eprintln!("{e}"),
        },
        Some("sounds-fmod") if args.len() == 4 => fsb::decode_with_fmod(Path::new(&args[2]), Path::new(&args[3])).unwrap_or_else(|e| eprintln!("{e}")),
        // `hud <extracted>`: only the HUD sprites (export_hud), without the full export.
        Some("hud-all") if args.len() == 3 => export::export_hud_all(Path::new(&args[2])),
        Some("hud") if args.len() == 3 => export::export_hud(Path::new(&args[2]), &export::hud_icons(&Path::new(&args[2]).join("json/params"))),
        Some("npcs") if args.len() >= 3 => export::export_npcs(Path::new(&args[2]), Path::new(&defs_dir()), &args[3..]),
        Some("export") if args.len() == 3 => export::export(
            Path::new(&args[2]),
            &tool_dir().join("export_states.txt"),
            Path::new(&defs_dir()),
        ),
        _ => eprintln!(
            "usage: sekiro-extract unpack <sekiro_dir> <out_dir> <regex> | params <dir> <out> | tae <dir> <out>
               map <extracted> <map id> <centre part> <radius m> | bxf <x.bhd> <out dir> | flverinfo <flver>
               hkxdump <hkx> [compendium] | objects <hkx> <type> [compendium] | hktypes ..."
        ),
    }
}

fn unpack(game: &Path, out: &Path, pattern: &str) {
    unsafe { std::env::set_var("SEKIRO_DIR", game) };
    let re = regex::Regex::new(pattern).expect("bad regex");
    let dict = std::fs::read_to_string(format!("{}/SekiroDictionary.txt", keys_dir())).unwrap();
    let wanted: HashMap<u32, &str> = dict
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/') && re.is_match(l))
        .map(|l| (bhd::path_hash(l), l))
        .collect();
    println!("{} dictionary paths match", wanted.len());
    let only = std::env::var("SEKIRO_UNPACK_ONLY").ok();
    let mut found = 0;
    for n in 1..=5 {
        let pem = std::fs::read_to_string(format!("{}/Data{n}.pem", keys_dir())).unwrap();
        let entries = bhd::read_bhd(&game.join(format!("Data{n}.bhd")), &pem);
        let mut bdt = File::open(game.join(format!("Data{n}.bdt"))).unwrap();
        for e in &entries {
            let Some(path) = wanted.get(&e.hash) else { continue };
            found += 1;
            let mut data = bhd::read_entry(&mut bdt, e);
            let mut rel = path.trim_start_matches('/').to_string();
            if container::is_dcx(&data) {
                data = container::decompress_dcx(&data);
                rel = rel.trim_end_matches(".dcx").to_string();
            }
            let dest = out.join(&rel);
            // SEKIRO_UNPACK_ONLY=<regex>: of a binder, write only the inner files whose names
            // match (the maps' .grass tables without their gigabytes of pieces).
            if only.is_none() {
                write(&dest, &data);
            }
            println!("Data{n}: {rel} ({} bytes)", data.len());
            if data.starts_with(b"BND4") {
                expand_bnd(&dest, &data);
            }
        }
    }
    println!("extracted {found} of {}", wanted.len());
}

fn expand_bnd(dest: &Path, data: &[u8]) {
    let dir = PathBuf::from(format!("{}.d", dest.display()));
    let files = container::read_bnd4(data);
    let base = |n: &str| n.rsplit(['\\', '/']).next().unwrap_or("").to_string();
    let mut count: HashMap<String, usize> = HashMap::new();
    for f in &files {
        *count.entry(base(&f.name).to_lowercase()).or_default() += 1;
    }
    for f in files {
        // Binder names are full original paths like "N:\FDP\data\INTERROOT_win64\chr\c0000\tae\a000.tae".
        let name = base(&f.name);
        let name = if name.is_empty() { format!("{}.bin", f.id) } else { name };
        // Same file name in several folders (behbnd: Export\, Characters\ and Behaviors\c1020.hkx):
        // each is also written as "<folder>_<name>"; the plain name keeps the last one, as before.
        if count.get(&name.to_lowercase()).is_some_and(|&c| c > 1) {
            let folder = f.name.rsplit(['\\', '/']).nth(1).unwrap_or("");
            write(&dir.join(format!("{folder}_{name}")), &f.data);
        }
        let p = dir.join(&name);
        let only = std::env::var("SEKIRO_UNPACK_ONLY").ok().and_then(|r| regex::Regex::new(&r).ok());
        if only.as_ref().is_none_or(|re| re.is_match(&name)) {
            write(&p, &f.data);
        }
        if f.data.starts_with(b"BND4") {
            expand_bnd(&p, &f.data);
        }
    }
}

fn write(p: &Path, d: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, d).unwrap();
}

fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    v.sort();
    v
}

fn dump_params(dir: &Path, out: &Path) {
    let defs: HashMap<String, param::ParamDef> = files_with_ext(&Path::new(&defs_dir()).join("paramdef"), "xml")
        .iter()
        .map(|p| {
            let d = param::ParamDef::load(p);
            (d.param_type.clone(), d)
        })
        .collect();
    for p in files_with_ext(dir, "param") {
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        let names = load_names(&Path::new(&defs_dir()).join("names").join(format!("{stem}.txt")));
        let data = std::fs::read(&p).unwrap();
        let (ty, json) = param::read_param(&data, &defs, &names);
        if json.is_null() {
            println!("{stem}: no paramdef for {ty}");
            continue;
        }
        let rows = json["rows"].as_array().map_or(0, Vec::len);
        write(&out.join(format!("{stem}.json")), serde_json::to_string_pretty(&json).unwrap().as_bytes());
        println!("{stem}: {rows} rows ({ty})");
    }
}

/// Paramdex Names/*.txt: "<id> <name>" per line.
fn load_names(p: &Path) -> HashMap<i32, String> {
    let Ok(text) = std::fs::read_to_string(p) else { return HashMap::new() };
    text.lines()
        .filter_map(|l| {
            let (id, name) = l.trim().split_once(' ')?;
            Some((id.parse().ok()?, name.trim().to_string()))
        })
        .collect()
}

fn dump_tae(dir: &Path, out: &Path) {
    let tmpl = tae::load_template(&Path::new(&defs_dir()).join("TAE.Template.SDT.xml"));
    for p in files_with_ext(dir, "tae") {
        let data = std::fs::read(&p).unwrap();
        let json = tae::read_tae(&data, &tmpl);
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        write(&out.join(format!("{stem}.json")), serde_json::to_string_pretty(&json).unwrap().as_bytes());
        println!("{stem}: {} anims", json["anims"].as_array().map_or(0, Vec::len));
    }
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// model_<chr>.bin, little endian:
///   "SHMD" u32 version=3
///   u32 nodes; per node: u16 len, name, i16 parent, f32 t[3] q[4] s[3]   (FLVER space)
///   u32 meshes; per mesh: u16 len, material, u16 len, albedo texture name,
///     (v3) u16 len, normal-map texture name, (v4) u16 len, metallic-mask texture name,
///     (v5) u8 alpha test,
///     (v6) u8 DetailBlend variant (255: neither; FRESNEL = the coat shader); unless 255: u8
///       layer bits (`DETAIL_LAYERS`), 8 x (u16 len, name): layer A albedo / normal, B, C, D
///       (the coat: layers 3 / 1, 4 / 5), then f32[40] (`detail_params` / `fresnel_params`),
///       SHELL only: then u32 n, f32[n] (each vertex's colour alpha),
///     u32 vertices; per vertex f32 pos[3] normal[3] uv[2], u16 bones[4], f32 weights[4]
///     u32 indices; u32 each
///   (v2) u32 dummies; per dummy: i16 id, i16 parent, i16 attach, f32 pos[3]
/// Character_AMSN[DetailBlend] (Wolf's tops / scarf, many characters' cloth; docs/kb/visuals.md
/// "DetailBlend"): the layers each Gbuf variant (by GXFT_SATStaticSwitch) has. Bits: 1 B, 2 C,
/// 4 D, 8 / 16 / 32 B / C / D blended after the base modulation ("replace"), 64 back faces.
/// gap: variant 19 applies D before C, variant 21 also blends slot 12's normal alone; both drawn
/// in the common order.
const DETAIL_LAYERS: [u8; 25] = [0, 1, 9, 3, 19, 27, 6, 38, 54, 7, 39, 55, 63, 2, 18, 64, 66, 82, 70, 86, 118, 4, 5, 13, 36];

/// DetailBlend constants (40 floats, map in sekiro_material.wgsl `detail`): colours (rgb x
/// Intensity; color_0 A, color_1 B, color_5 C, color_2 D), the layers' gloss adds (float_0 / 1 /
/// 15 / 13) and metal (float_2 / 8 / 16 / 5), float_10 (normal strength), the groups' UV tiling
/// (base, A, B, C, D: the layer albedo's own MTD texture scale, or when that is (0, 0) its
/// group's `group_N_CommonUV-UVParam` (group_1 base, 2 A, 3 B, 4 C, 7 D): Wolf's scarf has
/// blend01 18 x 6 in one MTD and (0, 0) + group_2 [18, 6] in its cloth twin; read as
/// FC_TextureTilingScale, gap: inferred). A value the MTD leaves out: 0 (colours / tiling 1).
fn detail_params(d: &mtd::Mtd) -> Vec<f32> {
    let get = |n: &str| d.floats.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    let p = "Character_AMSN__DetailBlend__snp_0_";
    let f = |n: &str| get(&format!("{p}{n}")).and_then(|v| v.first().copied()).unwrap_or(0.0);
    let col = |n: &str| -> [f32; 4] {
        let c = get(&format!("{p}{n}")).filter(|v| v.len() >= 3).map_or([1.0; 3], |v| [v[0], v[1], v[2]]);
        let k = get(&format!("{p}{n}-Intensity")).and_then(|v| v.first().copied()).unwrap_or(1.0);
        [c[0] * k, c[1] * k, c[2] * k, 0.0]
    };
    let group = |g: u32| get(&format!("group_{g}_CommonUV-UVParam")).filter(|v| v.len() >= 2).map_or([1.0, 1.0], |v| [v[0], v[1]]);
    let uv = |g: u32| {
        let slot = match g {
            1 => 7,
            2 => 1,
            3 => 2,
            4 => 10,
            _ => 4,
        };
        tex_scale(d, slot, "AlbedoMap").unwrap_or_else(|| group(g))
    };
    let mut out = Vec::with_capacity(40);
    for c in ["color_0", "color_1", "color_5", "color_2"] {
        out.extend(col(c));
    }
    out.extend(["float_0", "float_1", "float_15", "float_13"].map(f));
    out.extend(["float_2", "float_8", "float_16", "float_5"].map(f));
    out.extend([f("float_10"), 0.0, 0.0, 0.0]);
    for g in [1, 2, 3, 4, 7] {
        out.extend(uv(g));
    }
    out.extend([0.0, 0.0]);
    out
}

/// A texture slot's own MTD tiling scale, None when it is (0, 0) (the group's then).
fn tex_scale(d: &mtd::Mtd, slot: u32, kind: &str) -> Option<[f32; 2]> {
    let want = format!("Texture2D_{slot}_{kind}");
    d.textures.iter().find(|t| t.kind.ends_with(&want) || t.kind.contains(&format!("{want}_"))).map(|t| t.scale).filter(|s| s[0] != 0.0 || s[1] != 0.0)
}

/// The variant byte model v6 gives Character_AN_Blend_AMSN_[Fresnel] (Wolf's coat; no static
/// switch of its own).
const FRESNEL: u8 = 100;

/// Character_AN_Blend_AMSN_[Fresnel] constants (40 floats, sekiro_material.wgsl `fresnel_layers`;
/// docs/kb/visuals.md "Coat"): color_1 (blend layer 3 / 1 tint), color_3 (layer 4 / 5 tint),
/// color_2 (rim colour), color_0 (rim target), color_4 (back faces), all rgb x Intensity; float_0
/// (metal), float_1 / float_11 (the two layers' normal strength), float_3 (layer 3 gloss
/// scale), float_5 (rim target mix), float_8 (rim exponent), float_9 (rim bias); the tiling of
/// slots 3, 1, 4, 5 and the base (MTD texture scale, 1 when (0, 0)).
fn fresnel_params(d: &mtd::Mtd) -> Vec<f32> {
    let get = |n: &str| d.floats.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    let p = "Character_AN_Blend_AMSN__Fresnel__snp_0_";
    let f = |n: &str| get(&format!("{p}{n}")).and_then(|v| v.first().copied()).unwrap_or(0.0);
    let col = |n: &str| -> [f32; 4] {
        let c = get(&format!("{p}{n}")).filter(|v| v.len() >= 3).map_or([1.0; 3], |v| [v[0], v[1], v[2]]);
        let k = get(&format!("{p}{n}-Intensity")).and_then(|v| v.first().copied()).unwrap_or(1.0);
        [c[0] * k, c[1] * k, c[2] * k, 0.0]
    };
    let t = |slot: u32, kind: &str| tex_scale(d, slot, kind).unwrap_or([1.0, 1.0]);
    let mut out = Vec::with_capacity(40);
    for c in ["color_1", "color_3", "color_2", "color_0", "color_4"] {
        out.extend(col(c));
    }
    out.extend(["float_0", "float_1", "float_11", "float_3"].map(f));
    out.extend([f("float_5"), f("float_8"), f("float_9"), 0.0]);
    for (slot, kind) in [(3, "AlbedoMap"), (1, "NormalMap"), (4, "AlbedoMap"), (5, "NormalMap"), (2, "AlbedoMap")] {
        out.extend(t(slot, kind));
    }
    out.extend([0.0, 0.0]);
    out
}

/// The variant byte model v6 gives Character_Ghost_scroll_test2 (the Demon of Hatred's burning
/// arm, c7020_arm_test.mtd; also c5070_em.mtd).
const GHOST: u8 = 101;

/// Character_Ghost_scroll_test2 constants (40 floats, sekiro_material.wgsl `ghost_scroll`;
/// decoded from its Fwd pixel shader, shader/shaderbdle.shaderbdlebnd
/// Character_Ghost_scroll_test2_Fwd.ppo, cbMtdParam cb3[24..39]): color_0..color_5 as
/// rgb^2.2 x Intensity (GXMaterialParam_HDRColor FUN_1403dc4b0, docs/kb/sky_shader.md), then
/// float_0..float_9, float_10 (cbMatDynParam SAT_CAParam, not in the MTD: 0, gap: set at run
/// time by the exe, not traced), color_0's alpha, and slot 0's tiling.
fn ghost_params(d: &mtd::Mtd) -> Vec<f32> {
    let get = |n: &str| d.floats.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    let p = "Character_Ghost_scroll_test2_snp_0_";
    let f = |n: &str| get(&format!("{p}{n}")).and_then(|v| v.first().copied()).unwrap_or(0.0);
    let col = |n: &str| -> [f32; 4] {
        let c = get(&format!("{p}{n}")).filter(|v| v.len() >= 4).map_or([1.0; 4], |v| [v[0], v[1], v[2], v[3]]);
        let k = get(&format!("{p}{n}-Intensity")).and_then(|v| v.first().copied()).unwrap_or(1.0);
        [c[0].powf(2.2) * k, c[1].powf(2.2) * k, c[2].powf(2.2) * k, c[3]]
    };
    let mut out = Vec::with_capacity(40);
    for c in ["color_0", "color_1", "color_2", "color_3", "color_4", "color_5"] {
        out.extend(col(c));
    }
    out.extend(["float_0", "float_1", "float_2", "float_3", "float_4", "float_5", "float_6", "float_7", "float_8", "float_9"].map(f));
    out.extend([0.0, col("color_0")[3]]);
    out.extend(tex_scale(d, 0, "AlbedoMap").unwrap_or([1.0, 1.0]));
    out.extend([0.0, 0.0]);
    out
}

/// The variant byte model v6 gives C[Shell] (fur shells: the Guardian Ape's c5100_fur_shell,
/// FC_A_0000_shell_fur). After its params the model carries u32 n and one f32 per vertex: the
/// vertex colour alpha (the shell layer's opacity).
const SHELL: u8 = 102;

/// C[Shell] constants (40 floats, sekiro_material.wgsl `shell_layers`), decoded from
/// C[Shell]_0_Gbuf_[A].ppo (shader/shaderbdle.shaderbdlebnd; cbMtdParam: color_0 at cb3[24],
/// float_3 / 4 / 5 / 6 at cb3[29..32]): [0] color_0 (rgb x Intensity), [1].xyzw float_3, float_4,
/// float_5, float_6, [2].x g_AlphaRef[0] / 255 (FC_AlphaRef.x, inferred), [7] tiling slot 6
/// (noise).xy, slot 7 (colour).zw, [8].xy slot 8 (normal). A texture's own MTD scale, else its
/// group: group_1 for the noise, group_2 for the colour and normal (inferred: FC_TextureTilingScale
/// by t-register, as DetailBlend).
fn shell_params(d: &mtd::Mtd) -> Vec<f32> {
    let get = |n: &str| d.floats.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    let p = "C_Shell__snp_0_";
    let f = |n: &str| get(&format!("{p}{n}")).and_then(|v| v.first().copied()).unwrap_or(0.0);
    let c = get(&format!("{p}color_0")).filter(|v| v.len() >= 3).map_or([1.0; 3], |v| [v[0], v[1], v[2]]);
    let k = get(&format!("{p}color_0-Intensity")).and_then(|v| v.first().copied()).unwrap_or(1.0);
    let group = |g: u32| get(&format!("group_{g}_CommonUV-UVParam")).filter(|v| v.len() >= 2).map_or([1.0, 1.0], |v| [v[0], v[1]]);
    let alpha_ref = d.ints.iter().find(|(n, _)| n == "g_AlphaRef").and_then(|(_, v)| v.first().copied()).unwrap_or(0) as f32 / 255.0;
    let mut out = vec![0.0; 40];
    out[0..4].copy_from_slice(&[c[0] * k, c[1] * k, c[2] * k, 0.0]);
    out[4..8].copy_from_slice(&["float_3", "float_4", "float_5", "float_6"].map(f));
    out[8] = alpha_ref;
    out[28..30].copy_from_slice(&tex_scale(d, 6, "AlbedoMap").unwrap_or_else(|| group(1)));
    out[30..32].copy_from_slice(&tex_scale(d, 7, "AlbedoMap").unwrap_or_else(|| group(2)));
    out[32..34].copy_from_slice(&tex_scale(d, 8, "NormalMap").unwrap_or_else(|| group(2)));
    out
}

/// One mesh's DetailBlend data (`export_model`): variant, layer bits, layer texture names, params.
struct Detail {
    variant: u8,
    bits: u8,
    names: [String; 8],
    params: Vec<f32>,
}

fn export_objmesh(flver_path: &Path, out: &Path) {
    let f = flver::read(&std::fs::read(flver_path).expect("read flver"));
    let slot = |m: &flver::Mesh, kind: &str| m.textures.iter().find(|(s, t)| s.contains(kind) && s.ends_with("_0") && !t.is_empty()).or(m.textures.iter().find(|(s, t)| s.contains(kind) && !t.is_empty())).map(|(_, t)| t.clone()).unwrap_or_default();
    let meshes: Vec<serde_json::Value> = f
        .meshes
        .iter()
        .map(|m| {
            let mut pos = Vec::with_capacity(m.vertices.len() * 3);
            let mut nrm = Vec::with_capacity(m.vertices.len() * 3);
            let mut uv = Vec::with_capacity(m.vertices.len() * 2);
            for v in &m.vertices {
                pos.extend_from_slice(&v.pos);
                nrm.extend_from_slice(&v.normal);
                uv.extend_from_slice(&v.uv);
            }
            serde_json::json!({ "mtd": m.mtd, "albedo": slot(m, "AlbedoMap"), "normal": slot(m, "NormalMap"), "cull": m.cull, "alpha_test": m.alpha_test, "pos": pos, "normal_vec": nrm, "uv": uv, "indices": m.indices })
        })
        .collect();
    let dummies: Vec<serde_json::Value> = f.dummies.iter().map(|d| serde_json::json!({ "id": d.id, "pos": d.pos })).collect();
    std::fs::write(out, serde_json::to_string(&serde_json::json!({ "meshes": meshes, "dummies": dummies })).unwrap()).expect("write");
    println!("{} meshes -> {}", f.meshes.len(), out.display());
}

fn export_model(flver_path: &Path, tpf_path: &Path, out: &Path, tex_dir: &Path, extra_tex: &[&Path]) {
    let mut f = flver::read(&std::fs::read(flver_path).expect("read flver"));
    let mut textures = if tpf_path.to_string_lossy() != "-" { flver::tpf(&std::fs::read(tpf_path).expect("read tpf")) } else { Vec::new() };
    // Shared character textures (hair, bandages, fabric detail blends): parts/common_body.tpf.
    let common = tex_dir.parent().unwrap_or(Path::new(".")).join("parts/common_body.tpf");
    if tpf_path.to_string_lossy() != "-" {
        if let Ok(d) = std::fs::read(&common) {
            textures.extend(flver::tpf(&d));
        }
        // A character family's shared texture pack: chr/cXXX9.texbnd (c1010's skin is
        // c1019_body_merge_a, c1020's head c1029_head02_a).
        let stem = flver_path.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
        if stem.len() == 5 && stem.starts_with('c') {
            let family = format!("{}9", &stem[..4]);
            let dir = tex_dir.parent().unwrap_or(Path::new(".")).join(format!("chr/{family}.texbnd.d"));
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                if e.path().extension().is_some_and(|x| x == "tpf") {
                    if let Ok(d) = std::fs::read(e.path()) {
                        textures.extend(flver::tpf(&d));
                    }
                }
            }
        }
    }
    for e in extra_tex.iter().filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten() {
        if e.path().extension().is_some_and(|x| x == "tpf") {
            if let Ok(d) = std::fs::read(e.path()) {
                textures.extend(flver::tpf(&d));
            }
        }
    }
    let albedos: Vec<String> = textures.iter().map(|(n, _)| n.clone()).filter(|n| n.ends_with("_a")).collect();
    let normals: Vec<String> = textures.iter().map(|(n, _)| n.clone()).filter(|n| n.ends_with("_n")).collect();
    // Each material's MTD names its textures (allmaterialbnd next to the tex dir); the first slot of
    // each kind whose texture is in this TPF wins, else the name-based guesses.
    let mtd_dir = tex_dir.parent().unwrap_or(Path::new(".")).join("mtd/allmaterialbnd.mtdbnd.d");
    let mtd_files: std::collections::HashMap<String, std::path::PathBuf> = std::fs::read_dir(&mtd_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.path().file_stem().unwrap_or_default().to_string_lossy().to_lowercase(), e.path()))
        .collect();
    let in_tpf: std::collections::HashSet<&str> = textures.iter().map(|(n, _)| n.as_str()).collect();
    let mut details: Vec<Option<Detail>> = Vec::with_capacity(f.meshes.len());
    for m in f.meshes.iter_mut() {
        let mtd_data = mtd_files.get(&m.mtd.to_lowercase()).and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
        let slots = flver::mtd_textures(&mtd_data);
        m.alpha_test = flver::alpha_tested(&mtd_data, &flver::mtd_shader(&mtd_data));
        // Primary albedo = the layer-0 slot ("..._AlbedoMap_0"; faces also list damage / skin-tone
        // blend layers), then the normal / metallic maps of the same texture set (x_a -> x_n / x_m).
        let available = |kind: &str| -> Vec<(String, String)> { slots.iter().filter(|(s, t)| s.contains(kind) && in_tpf.contains(t.as_str())).cloned().collect() };
        let albedo_slots = available("AlbedoMap");
        if let Some((_, a)) = albedo_slots.iter().find(|(s, _)| s.ends_with("AlbedoMap_0")).or(albedo_slots.first()) {
            m.albedo = a.clone();
        }
        if m.albedo.is_empty() {
            m.albedo = flver::guess_albedo(&m.mtd, &albedos);
        }
        let base = m.albedo.strip_suffix("_a").unwrap_or(&m.albedo).to_string();
        let same_set = |kind: &str, suffix: &str| {
            let list = available(kind);
            list.iter().find(|(_, t)| *t == format!("{base}{suffix}")).or(list.first()).map(|(_, t)| t.clone())
        };
        m.normal_map = same_set("NormalMap", "_n").unwrap_or_else(|| flver::guess_normal(&m.albedo, &normals));
        m.metallic = same_set("MetallicMap", "_m").unwrap_or_default();
        // DetailBlend (and its `_cloth` twin, same pixel shaders): base albedo slot 7, normal 0,
        // metal 3 by number; the fabric layers.
        let detail = flver::mtd_shader(&mtd_data).to_lowercase().starts_with("character_amsn_[detailblend]").then(|| mtd::read(&mtd_data)).flatten().map(|d| {
            let slot = |n: u32, kind: &str| available(kind).into_iter().find(|(s, _)| s.ends_with(&format!("Texture2D_{n}_{kind}"))).map(|(_, t)| t).unwrap_or_default();
            if !slot(7, "AlbedoMap").is_empty() {
                m.albedo = slot(7, "AlbedoMap");
            }
            if !slot(0, "NormalMap").is_empty() {
                m.normal_map = slot(0, "NormalMap");
            }
            m.metallic = slot(3, "MetallicMap");
            let variant = d.int("GXFT_SATStaticSwitch").unwrap_or(0).clamp(0, 24) as u8;
            let names = [slot(1, "AlbedoMap"), slot(6, "NormalMap"), slot(2, "AlbedoMap"), slot(5, "NormalMap"), slot(10, "AlbedoMap"), slot(12, "NormalMap"), slot(4, "AlbedoMap"), slot(9, "NormalMap")];
            Detail { variant, bits: DETAIL_LAYERS[variant as usize], names, params: detail_params(&d) }
        });
        // The coat shader (Character_AN_Blend_AMSN_[Fresnel] and its `_cloth` twin): base albedo
        // slot 2, normal 7 (alpha = the layer mask), metal 0; layers 3 / 1 and 4 / 5.
        let detail = detail.or_else(|| {
            let d = flver::mtd_shader(&mtd_data).to_lowercase().starts_with("character_an_blend_amsn_[fresnel]").then(|| mtd::read(&mtd_data)).flatten()?;
            let slot = |n: u32, kind: &str| available(kind).into_iter().find(|(s, _)| s.contains(&format!("Texture2D_{n}_{kind}"))).map(|(_, t)| t).unwrap_or_default();
            if !slot(2, "AlbedoMap").is_empty() {
                m.albedo = slot(2, "AlbedoMap");
            }
            if !slot(7, "NormalMap").is_empty() {
                m.normal_map = slot(7, "NormalMap");
            }
            m.metallic = slot(0, "ReflectanceMap");
            let names = [slot(3, "AlbedoMap"), slot(1, "NormalMap"), slot(4, "AlbedoMap"), slot(5, "NormalMap"), String::new(), String::new(), String::new(), String::new()];
            Some(Detail { variant: FRESNEL, bits: 0, names, params: fresnel_params(&d) })
        });
        // The scrolling ghost-fire shader (and its `_cloth` twin): albedo slot 0.
        let detail = detail.or_else(|| {
            let d = flver::mtd_shader(&mtd_data).to_lowercase().starts_with("character_ghost_scroll_test2").then(|| mtd::read(&mtd_data)).flatten()?;
            Some(Detail { variant: GHOST, bits: 0, names: Default::default(), params: ghost_params(&d) })
        });
        // C[Shell]: colour slot 7, normal 8, metal 10; the noise (slot 6) as layer A.
        let detail = detail.or_else(|| {
            let d = flver::mtd_shader(&mtd_data).to_lowercase().starts_with("c[shell]").then(|| mtd::read(&mtd_data)).flatten()?;
            let slot = |n: u32, kind: &str| available(kind).into_iter().find(|(s, _)| s.contains(&format!("Texture2D_{n}_{kind}"))).map(|(_, t)| t).unwrap_or_default();
            if !slot(7, "AlbedoMap").is_empty() {
                m.albedo = slot(7, "AlbedoMap");
            }
            m.normal_map = slot(8, "NormalMap");
            m.metallic = slot(10, "MetallicMap");
            let names = [slot(6, "AlbedoMap"), String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), String::new()];
            Some(Detail { variant: SHELL, bits: 0, names, params: shell_params(&d) })
        });
        details.push(detail);
        m.textures = slots;
    }
    let mut b: Vec<u8> = Vec::new();
    let put_str = |b: &mut Vec<u8>, s: &str| {
        b.extend_from_slice(&(s.len() as u16).to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    };
    let put_f = |b: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
    b.extend_from_slice(b"SHMD");
    b.extend_from_slice(&6u32.to_le_bytes());
    b.extend_from_slice(&(f.nodes.len() as u32).to_le_bytes());
    for n in &f.nodes {
        put_str(&mut b, &n.name);
        b.extend_from_slice(&n.parent.to_le_bytes());
        let axis = |i: usize, a: f32| {
            let mut q = [0.0, 0.0, 0.0, (a / 2.0).cos()];
            q[i] = (a / 2.0).sin();
            q
        };
        // Row-vector S*Rx*Rz*Ry*T == column-vector Ry*Rz*Rx.
        let q = quat_mul(axis(1, n.r[1]), quat_mul(axis(2, n.r[2]), axis(0, n.r[0])));
        put_f(&mut b, &n.t);
        put_f(&mut b, &q);
        put_f(&mut b, &n.s);
    }
    b.extend_from_slice(&(f.meshes.len() as u32).to_le_bytes());
    for (m, detail) in f.meshes.iter().zip(&details) {
        put_str(&mut b, &m.material);
        put_str(&mut b, &m.albedo);
        put_str(&mut b, &m.normal_map);
        put_str(&mut b, &m.metallic);
        b.push(m.alpha_test as u8);
        match detail {
            Some(d) => {
                b.push(d.variant);
                b.push(d.bits);
                d.names.iter().for_each(|n| put_str(&mut b, n));
                put_f(&mut b, &d.params);
                if d.variant == SHELL {
                    b.extend_from_slice(&(m.vertices.len() as u32).to_le_bytes());
                    put_f(&mut b, &m.vertices.iter().map(|v| v.color[3]).collect::<Vec<_>>());
                }
            }
            None => b.push(255),
        }
        b.extend_from_slice(&(m.vertices.len() as u32).to_le_bytes());
        for v in &m.vertices {
            put_f(&mut b, &v.pos);
            put_f(&mut b, &v.normal);
            put_f(&mut b, &v.uv);
            v.bones.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
            put_f(&mut b, &v.weights);
        }
        b.extend_from_slice(&(m.indices.len() as u32).to_le_bytes());
        m.indices.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
    }
    // v2: dummy polys (hitbox / effect anchors).
    b.extend_from_slice(&(f.dummies.len() as u32).to_le_bytes());
    for dm in &f.dummies {
        b.extend_from_slice(&dm.id.to_le_bytes());
        b.extend_from_slice(&dm.parent.to_le_bytes());
        b.extend_from_slice(&dm.attach.to_le_bytes());
        put_f(&mut b, &dm.pos);
    }
    write(out, &b);
    // Havok cloth next to the FLVER (c1020.flver -> c1020_c.hkx): exported as <out>.cloth.json.
    let stem = flver_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    if let Ok(d) = std::fs::read(flver_path.with_file_name(format!("{stem}_c.hkx"))) {
        let json = cloth_export::export(&f, &d);
        write(&out.with_extension("cloth.json"), serde_json::to_string(&json).unwrap().as_bytes());
    }
    let tris: usize = f.meshes.iter().map(|m| m.indices.len() / 3).sum();
    println!("{}: {} nodes, {} meshes, {} triangles", out.display(), f.nodes.len(), f.meshes.len(), tris);
    for m in &f.meshes {
        println!("  {} [{}] -> {} + {} + {}{} ({} verts)", m.material, m.mtd, m.albedo, m.normal_map, m.metallic, if m.alpha_test { " (alpha test)" } else { "" }, m.vertices.len());
    }
    for (m, d) in f.meshes.iter().zip(&details) {
        if let Some(d) = d {
            let what = if d.variant == SHELL { "fur shell".to_string() } else if d.variant == GHOST { "ghost scroll".to_string() } else if d.variant == FRESNEL { "Fresnel coat".to_string() } else { format!("DetailBlend variant {} layers {:#x}", d.variant, d.bits) };
            println!("  {} {what}: {:?}", m.material, d.names);
        }
    }
    let wanted: std::collections::HashSet<&str> = f.meshes.iter().flat_map(|m| [m.albedo.as_str(), m.normal_map.as_str(), m.metallic.as_str()]).chain(details.iter().flatten().flat_map(|d| d.names.iter().map(String::as_str))).collect();
    for (name, dds) in &textures {
        if wanted.contains(name.as_str()) {
            write(&tex_dir.join(format!("{name}.dds")), dds);
        }
    }
}


/// DDS (first mip, BC1-BC7 or DX10 header) -> width, height, BGRA texels (texture2ddecoder's
/// little-endian packing).
pub fn decode_dds(d: &[u8]) -> (usize, usize, Vec<u32>) {
    let u32_at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let (h, w) = (u32_at(12) as usize, u32_at(16) as usize);
    let four = &d[84..88];
    let (fmt, data_off) = if four == b"DX10" { (u32_at(128), 148) } else { (0, 128) };
    let src_data = &d[data_off..];
    let mut px = vec![0u32; w * h];
    let r = match (four, fmt) {
        (b"DXT1", _) | (_, 71 | 72) => texture2ddecoder::decode_bc1a(src_data, w, h, &mut px),
        (b"DXT3", _) | (_, 74 | 75) => texture2ddecoder::decode_bc2(src_data, w, h, &mut px),
        (b"DXT5", _) | (_, 77 | 78) => texture2ddecoder::decode_bc3(src_data, w, h, &mut px),
        (b"ATI1" | b"BC4U", _) | (_, 80 | 81) => texture2ddecoder::decode_bc4(src_data, w, h, &mut px),
        (b"ATI2" | b"BC5U", _) | (_, 83 | 84) => texture2ddecoder::decode_bc5(src_data, w, h, &mut px),
        (_, 98 | 99) => texture2ddecoder::decode_bc7(src_data, w, h, &mut px),
        _ => panic!("unsupported dds format {four:?} / {fmt}"),
    };
    r.expect("decode");
    (w, h, px)
}

/// DDS (first mip, BC1-BC7 or DX10 header) to an RGBA PNG; `alpha_only` writes the alpha as grey.
fn dds2png(src: &Path, dst: &Path, alpha_only: bool) {
    let d = std::fs::read(src).expect("read dds");
    let (w, h, px) = decode_dds(&d);
    // Normal-map check: share of texels whose RG (x2-1) reach the unit circle.
    if std::env::var("DDS_NORMAL_STATS").is_ok() {
        let (mut out, mut sum) = (0usize, 0.0f64);
        for &p in &px {
            let (g, r) = ((p >> 8 & 0xff) as f64 / 255.0 * 2.0 - 1.0, (p >> 16 & 0xff) as f64 / 255.0 * 2.0 - 1.0);
            let l = (r * r + g * g).sqrt();
            sum += l;
            if l >= 0.99 {
                out += 1;
            }
        }
        let mut hist = [0usize; 5];
        for &p in &px {
            hist[((p & 0xff) as usize * 5 / 256).min(4)] += 1;
        }
        let pct: Vec<String> = hist.iter().map(|&c| format!("{:.0}", 100.0 * c as f64 / px.len() as f64)).collect();
        println!("{}: mean |xy| {:.3}, |xy|>=0.99 {:.2} %, B quintiles % {}", src.display(), sum / px.len() as f64, 100.0 * out as f64 / px.len() as f64, pct.join("/"));
    }
    // texture2ddecoder packs BGRA little-endian.
    let mut rgba = Vec::with_capacity(w * h * 4);
    for p in px {
        let (b, g, r, a) = ((p & 0xff) as u8, (p >> 8 & 0xff) as u8, (p >> 16 & 0xff) as u8, (p >> 24) as u8);
        if alpha_only {
            rgba.extend_from_slice(&[a, a, a, 255]);
        } else {
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    write_png(dst, w, h, &rgba);
}

pub fn write_png(dst: &Path, w: usize, h: usize, rgba: &[u8]) {
    let f = std::fs::File::create(dst).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("png header").write_image_data(rgba).expect("png data");
}
