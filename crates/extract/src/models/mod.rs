//! `sekiro-extract models`: FLVER + TPF + MTD to skinned, textured GLB files in cache/models.
//!
//! Coordinate convention (see docs/FORMATS.md): FromSoftware data is left-handed with +Y up.
//! Every position, direction and matrix is mirrored across X (`x -> -x`, `M -> S M S` with
//! `S = diag(-1, 1, 1)`), which gives right-handed +Y-up data for glTF and Bevy. Index order is
//! kept: FromSoftware front faces are clockwise in their left-handed space, which the mirror
//! turns into the counter-clockwise front faces glTF expects (verified on single-sided meshes).

mod cloth;
pub(crate) mod glb;
mod info;
pub(crate) mod textures;
mod tool;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use glam::{Mat4, Vec3};
use sekiro_formats::{flver, mtd, param, paramdef, tae};
use serde_json::{Value, json};

pub use info::info;

use glb::Glb;
use textures::{TextureStore, Usage};

pub fn export(cache: &Path, ids: &[String], verbose: bool) -> Result<()> {
    let raw = cache.join("raw");
    let out = cache.join("models");
    std::fs::create_dir_all(&out)?;
    let ids: Vec<String> = if ids.is_empty() {
        crate::roster_ids(cache, false)?
    } else {
        ids.to_vec()
    };
    let params = Params::open(cache)?;
    for selection in &ids {
        let (id, row) = selection
            .split_once(':')
            .map_or((selection.as_str(), None), |(id, row)| (id, Some(row)));
        let row = row.map(str::parse::<i32>).transpose()?;
        eprintln!("{id}:");
        let model = if id == "c0000" {
            wolf(&raw, &params)?
        } else {
            npc(&raw, &params, id, row)?
        };
        for note in &model.notes {
            eprintln!("  {note}");
        }
        let bytes = build(&raw, &model, verbose)?;
        let path = out.join(format!("{id}.glb"));
        std::fs::write(&path, &bytes)?;
        eprintln!("  wrote {} ({} KiB)", path.display(), bytes.len() / 1024);
        cloth::export(&raw, &model, &out, id)?;
        if id == "c0000" {
            tool::export(cache, verbose)?;
        }
    }
    Ok(())
}

/// A model to export: one skeleton plus the parts skinned to it.
struct Model {
    skeleton: flver::Flver,
    parts: Vec<Part>,
    /// Human-readable record of the choices made (variant masks, equipment).
    notes: Vec<String>,
    /// Display-mask groups on by default and in combat (enemies; `None` exports only what
    /// shows).
    draw_masks: Option<([bool; 32], [bool; 32])>,
}

struct Part {
    label: String,
    flver: flver::Flver,
    tpfs: Vec<PathBuf>,
    /// Per-mesh visibility.
    visible: Vec<bool>,
    /// Skeleton node that this part's parentless bones hang from (weapons).
    attach: Option<String>,
}

/// The `#NN#` display-mask group a material belongs to, if any.
fn mask_group(material_name: &str) -> Option<usize> {
    let rest = material_name.strip_prefix('#')?;
    let end = rest.find('#')?;
    rest[..end].parse().ok()
}

fn read_flver(path: &Path) -> Result<flver::Flver> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    flver::parse(&data).with_context(|| format!("parsing {}", path.display()))
}

/// Finds a file in `dir` by case-insensitive name.
fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.find_map(|e| {
        let e = e.ok()?;
        e.file_name()
            .to_str()?
            .eq_ignore_ascii_case(name)
            .then(|| e.path())
    })
}

struct Params {
    root: PathBuf,
    defs: PathBuf,
}

impl Params {
    fn open(cache: &Path) -> Result<Self> {
        Ok(Self {
            root: cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            defs: cache.join("refs/paramdex/Defs"),
        })
    }

    /// Loads one PARAM table with its Paramdex def.
    fn load(&self, name: &str) -> Result<param::Table> {
        let path = self.root.join(format!("{name}.param"));
        let raw = param::RawParam::parse(
            std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
        )?;
        let def = paramdef::ParamDef::load(&self.defs.join(format!("{name}.xml")))?;
        Ok(param::Table::new(name, &raw, Arc::new(def), None)?)
    }
}

/// Prefer the most frequently placed row, with the lowest row breaking ties.
fn npc_row(cache: &Path, id: &str) -> Result<i32> {
    let mut counts = std::collections::BTreeMap::<i32, usize>::new();
    if let Ok(maps) = std::fs::read_dir(cache.join("maps")) {
        for map in maps {
            let path = map?.path().join("layout.json");
            if !path.exists() {
                continue;
            }
            let layout: Value = serde_json::from_slice(&std::fs::read(path)?)?;
            for enemy in layout["enemies"].as_array().into_iter().flatten() {
                if enemy["model"].as_str() == Some(id)
                    && let Some(row) = enemy["npc_param"].as_i64()
                {
                    *counts.entry(row as i32).or_default() += 1;
                }
            }
        }
    }
    if let Some((&row, _)) = counts
        .iter()
        .max_by_key(|&(row, count)| (count, std::cmp::Reverse(row)))
    {
        return Ok(row);
    }
    Ok(id.trim_start_matches('c').parse::<i32>()? * 10000)
}

/// The draw-mask change made by the enemy's weapon-draw animation (`a000_001040`, which
/// switches the soldier from sheathed to katana in hand), from the unpacked TAE.
/// Models can be exported before animations, so never depend on `anim/<id>/tae.json` here.
/// Values per group: 0 off, 1 on, 255 unchanged.
fn combat_draw_mask_event(cache: &Path, id: &str) -> Option<Vec<u8>> {
    const DRAW_WEAPON_ANIM: i64 = 1040;
    let bytes = std::fs::read(cache.join(format!("raw/chr/{id}.anibnd.d/{id}.tae"))).ok()?;
    let tae = tae::parse(&bytes).ok()?;
    for anim in &tae.animations {
        if anim.id != DRAW_WEAPON_ANIM {
            continue;
        }
        for event in &anim.events {
            // ChangeChrDrawMask (233): one byte per display group, then padding.
            if event.event_type == 233 {
                return Some(event.params.get(..32)?.to_vec());
            }
        }
    }
    None
}

/// An enemy/NPC character: its chrbnd FLVER and textures from its texbnd.
///
/// Every variant mesh is exported. Which `#NN#` groups show is game state: the scene extras
/// carry `drawMask` (NpcParam `modelDispMask0..31`, the out-of-combat look) and
/// `combatDrawMask` (that plus the weapon-draw TAE event), and `sekiro-game` applies them.
fn npc(raw: &Path, params: &Params, id: &str, selected_row: Option<i32>) -> Result<Model> {
    let chrbnd = raw.join(format!("chr/{id}.chrbnd.d"));
    let flver = read_flver(&chrbnd.join(format!("{id}.flver")))?;
    let mut notes = Vec::new();
    let row_id = match selected_row {
        Some(row) => row,
        None => npc_row(raw.parent().unwrap_or(raw), id)?,
    };
    let npc_param = params.load("NpcParam")?;
    let mut mask = [false; 32];
    match npc_param.find(row_id) {
        Some(row) => {
            for (i, m) in mask.iter_mut().enumerate() {
                *m = row.int(&format!("modelDispMask{i}"))? != 0;
            }
        }
        None => notes.push(format!(
            "NpcParam {row_id} missing; no display-mask group is on"
        )),
    }
    let on = |m: &[bool; 32]| (0..32).filter(|&i| m[i]).collect::<Vec<_>>();
    notes.push(format!(
        "NpcParam {row_id} draw mask groups on: {:?}",
        on(&mask)
    ));
    let cache = raw.parent().unwrap_or(raw);
    let mut combat = mask;
    match combat_draw_mask_event(cache, id) {
        Some(event) => {
            for (m, v) in combat.iter_mut().zip(event) {
                match v {
                    0 => *m = false,
                    1 => *m = true,
                    _ => {}
                }
            }
            notes.push(format!("combat draw mask groups on: {:?}", on(&combat)));
        }
        None => notes.push(format!(
            "no weapon-draw TAE event in cache/raw/chr/{id}.anibnd.d/{id}.tae; combat mask = default"
        )),
    }
    let visible = vec![true; flver.meshes.len()];
    let mut tpfs = vec![raw.join(format!("chr/{id}.texbnd.d/{id}.tpf"))];
    tpfs.push(chrbnd.join(format!("{id}.tpf")));
    let tex_chr = npc_param
        .find(row_id)
        .and_then(|r| r.int("normalChangeTexChrId").ok())
        .filter(|id| *id > 0);
    let mut texture_ids = vec![format!("{}8", &id[..4]), format!("{}9", &id[..4])];
    if let Some(tex) = tex_chr {
        texture_ids.push(format!("c{tex:04}"));
    }
    for tex in texture_ids {
        if let Ok(files) = std::fs::read_dir(raw.join(format!("chr/{tex}.texbnd.d"))) {
            tpfs.extend(
                files
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tpf"))),
            );
        }
    }
    Ok(Model {
        skeleton: flver.clone(),
        parts: vec![Part {
            label: id.to_string(),
            flver,
            tpfs,
            visible,
            attach: None,
        }],
        notes,
        draw_masks: Some((mask, combat)),
    })
}

/// The ordinary Wolf: c0000's skeleton, the protector parts from the new-game CharaInitParam
/// row, the face part, and the starting katana with its scabbard.
fn wolf(raw: &Path, params: &Params) -> Result<Model> {
    const CHARA_INIT_ROW: i32 = 10010; // "Castle": ordinary gear, katana, prosthetic
    let skeleton = read_flver(&raw.join("chr/c0000.chrbnd.d/c0000.flver"))?;
    let mut notes = Vec::new();
    let init = params.load("CharaInitParam")?;
    let prot = params.load("EquipParamProtector")?;
    let wep = params.load("EquipParamWeapon")?;
    let absorp = params.load("WepAbsorpPosParam")?;
    let row = init
        .find(CHARA_INIT_ROW)
        .context("CharaInitParam new-game row missing")?;

    let mut part_files: Vec<(String, Option<String>)> = Vec::new();
    // Display-mask groups hidden by the equipped protectors.
    let mut hidden = [false; 96];
    for slot in ["equip_Helm", "equip_Armer", "equip_Gaunt", "equip_Leg"] {
        let id = row.int(slot)? as i32;
        let Some(p) = prot.find(id) else {
            notes.push(format!("{slot} = {id}: no EquipParamProtector row"));
            continue;
        };
        let model = p.int("equipModelId")?;
        let prefix = match (
            p.int("headEquip")?,
            p.int("bodyEquip")?,
            p.int("armEquip")?,
            p.int("legEquip")?,
        ) {
            (_, 1, _, _) => "BD",
            (_, _, 1, _) => "AM",
            (_, _, _, 1) => "LG",
            _ => "HD",
        };
        for (g, h) in hidden.iter_mut().enumerate() {
            if p.int(&format!("invisibleFlag{g:02}")).unwrap_or(0) != 0 {
                *h = true;
            }
        }
        let mut name = format!("{prefix}_M_{model:04}");
        // Sekiro head protectors name face models (FC_M_*), which carry the head and hair.
        if prefix == "HD"
            && !raw
                .join(format!("parts/{}.partsbnd.d", name.to_ascii_lowercase()))
                .exists()
        {
            name = format!("FC_M_{model:04}");
        }
        notes.push(format!("{slot} = protector {id} -> model {name}"));
        part_files.push((name, None));
    }

    let wep_id = row.int("equip_Wep_Right")? as i32;
    match wep.find(wep_id) {
        Some(w) => {
            let model = w.int("equipModelId")?;
            let name = format!("WP_A_{model:04}");
            // The weapon's WepAbsorpPosParam row names the body dummies its models sit on:
            // `right_0` the blade in the hand, `right_1` the scabbard.
            let absorp_id = w.int("absorpParamId")? as i32;
            let place = absorp
                .find(absorp_id)
                .with_context(|| format!("no WepAbsorpPosParam row {absorp_id}"))?;
            let (held, scabbard) = (place.int("right_0")?, place.int("right_1")?);
            notes.push(format!(
                "equip_Wep_Right = weapon {wep_id} -> {name} at dummy {held}, scabbard {name}_1 at dummy {scabbard} (WepAbsorpPosParam {absorp_id})"
            ));
            part_files.push((name.clone(), Some(format!("dmy:{held}"))));
            part_files.push((format!("{name}_1"), Some(format!("dmy:{scabbard}"))));
        }
        None => notes.push(format!(
            "equip_Wep_Right = {wep_id}: no EquipParamWeapon row"
        )),
    }
    let hidden_groups: Vec<usize> = (0..96).filter(|&g| hidden[g]).collect();
    notes.push(format!(
        "display-mask groups hidden by protectors: {hidden_groups:?}"
    ));

    let mut parts = Vec::new();
    for (name, attach) in part_files {
        let base = name.split('_').take(3).collect::<Vec<_>>().join("_");
        let dir = raw.join(format!("parts/{}.partsbnd.d", base.to_ascii_lowercase()));
        let Some(path) = find_ci(&dir, &format!("{name}.flver")) else {
            notes.push(format!("{name}: no FLVER in {}, skipped", dir.display()));
            continue;
        };
        let flver = read_flver(&path)?;
        let visible = flver
            .meshes
            .iter()
            .map(|m| match mask_group(&flver.materials[m.material].name) {
                Some(g) => !hidden.get(g).copied().unwrap_or(false),
                None => true,
            })
            .collect();
        let tpfs = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tpf")))
            .collect();
        parts.push(Part {
            label: name,
            flver,
            tpfs,
            visible,
            attach,
        });
    }
    Ok(Model {
        skeleton,
        parts,
        notes,
        draw_masks: None,
    })
}

/// Mirror across X: converts FromSoftware's left-handed space to right-handed.
fn mirror_point(p: [f32; 3]) -> [f32; 3] {
    [-p[0], p[1], p[2]]
}

fn mirror_matrix(m: Mat4) -> Mat4 {
    let s = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    s * m * s
}

struct Node {
    name: String,
    local: Mat4,
    children: Vec<usize>,
    mesh: Option<usize>,
    skin: Option<usize>,
}

struct Scene {
    nodes: Vec<Node>,
    roots: Vec<usize>,
    by_name: HashMap<String, usize>,
}

impl Scene {
    fn add(&mut self, name: &str, local: Mat4, parent: Option<usize>) -> usize {
        let i = self.nodes.len();
        self.nodes.push(Node {
            name: name.to_string(),
            local,
            children: Vec::new(),
            mesh: None,
            skin: None,
        });
        match parent {
            Some(p) => self.nodes[p].children.push(i),
            None => self.roots.push(i),
        }
        i
    }
}

/// Maps a part's node indices onto scene nodes, adding nodes the skeleton lacks.
struct PartNodes<'a> {
    part: &'a Part,
    map: HashMap<usize, usize>,
}

impl PartNodes<'_> {
    fn node(&mut self, scene: &mut Scene, i: usize) -> usize {
        if let Some(&n) = self.map.get(&i) {
            return n;
        }
        let src = &self.part.flver.nodes[i];
        let n = match scene.by_name.get(&src.name) {
            Some(&n) => n,
            None => {
                let parent = if src.parent >= 0 {
                    Some(self.node(scene, src.parent as usize))
                } else {
                    self.part
                        .attach
                        .as_ref()
                        .and_then(|a| scene.by_name.get(a).copied())
                };
                scene.add(&src.name, mirror_matrix(src.local_matrix()), parent)
            }
        };
        self.map.insert(i, n);
        n
    }
}

/// A game texture path and the UV channel it is sampled with.
type TextureRef = (String, usize);

/// Picks the base colour and normal map paths (with UV channel) for a material.
///
/// Sekiro materials list many layers. The main albedo is the slot whose name ends in
/// `AlbedoMap_0` when there is one, otherwise the first albedo that is not an overlay (shared
/// `FC_A_0000_*` detail maps, damage, expression or skin-detail layers). The normal map is the
/// one named like the chosen albedo (`x_a` -> `x_n`), falling back to the same rules.
fn material_textures(
    material: &flver::Material,
    mtd: Option<&mtd::Mtd>,
) -> (Option<TextureRef>, Option<TextureRef>, Option<TextureRef>) {
    struct Slot {
        param: String,
        path: String,
        uv: usize,
    }
    let mut albedo = Vec::new();
    let mut normal = Vec::new();
    let mut metallic = Vec::new();
    for t in &material.textures {
        let from_mtd = mtd.and_then(|m| m.textures.iter().find(|mt| mt.kind == t.param));
        let path = if t.path.is_empty() {
            from_mtd.map(|m| m.path.clone()).unwrap_or_default()
        } else {
            t.path.clone()
        };
        if path.is_empty() {
            continue;
        }
        let uv = from_mtd
            .map(|m| (m.uv_number - 1).max(0) as usize)
            .unwrap_or(0);
        let param = t.param.to_ascii_lowercase();
        let slot = Slot { param, path, uv };
        if slot.param.contains("albedomap") || slot.param.contains("diffuse") {
            albedo.push(slot);
        } else if slot.param.contains("normalmap") || slot.param.contains("bumpmap") {
            normal.push(slot);
        } else if slot.param.contains("metallicmap") {
            metallic.push(slot);
        }
    }
    // Hair and fur cards (`Fur_NTC*`): AlbedoMap_0 is the shared strand texture (grey strands with
    // an alpha cutout), not an overlay; AlbedoMap_1 is a fur-coat colour map that is wrong on a
    // card (the Owl's beard and brows turned into grey flaps), so it is left out. A card with no
    // AlbedoMap_0 path takes the albedo that pairs with its normal map (`x_n` -> `x_a`).
    let fur = mtd.is_some_and(|m| m.shader.contains("Fur_"));
    if fur {
        albedo.retain(|s| s.param.ends_with("albedomap_0"));
        if albedo.is_empty()
            && let Some(n) = normal.first()
            && let Some(base) = textures::texture_stem(&n.path)
                .to_ascii_lowercase()
                .strip_suffix("_n")
        {
            albedo.push(Slot {
                param: "albedomap_0".into(),
                path: format!("{base}_a"),
                uv: n.uv,
            });
        }
    }
    let overlay = |s: &Slot| {
        let stem = textures::texture_stem(&s.path).to_ascii_lowercase();
        (!fur && stem.starts_with("fc_a_0000"))
            || ["damage", "expression", "_skin_"]
                .iter()
                .any(|w| stem.contains(w))
    };
    let main = |list: &[Slot], suffix: &str| -> Option<usize> {
        list.iter()
            .position(|s| s.param.ends_with(suffix) && !overlay(s))
            .or_else(|| list.iter().position(|s| !overlay(s)))
            .or(if list.is_empty() { None } else { Some(0) })
    };
    let a = main(&albedo, "albedomap_0");
    let n = a
        .and_then(|a| {
            let stem = textures::texture_stem(&albedo[a].path).to_ascii_lowercase();
            let want = format!("{}_n", stem.strip_suffix("_a")?);
            normal
                .iter()
                .position(|s| textures::texture_stem(&s.path).eq_ignore_ascii_case(&want))
        })
        .or_else(|| main(&normal, "normalmap_4"));
    // The metallic mask of the same texture set (`x_a` -> `x_m`), else the first one listed.
    let m = a
        .and_then(|a| {
            let stem = textures::texture_stem(&albedo[a].path).to_ascii_lowercase();
            let want = format!("{}_m", stem.strip_suffix("_a")?);
            metallic
                .iter()
                .position(|s| textures::texture_stem(&s.path).eq_ignore_ascii_case(&want))
        })
        .or(if metallic.is_empty() { None } else { Some(0) });
    let out = |list: &[Slot], i: Option<usize>| i.map(|i| (list[i].path.clone(), list[i].uv));
    (out(&albedo, a), out(&normal, n), out(&metallic, m))
}

fn build(raw: &Path, model: &Model, verbose: bool) -> Result<Vec<u8>> {
    let mut g = Glb::default();
    let mut store = TextureStore::new(raw);
    let mtd_dir = raw.join("mtd/allmaterialbnd.mtdbnd.d");
    let mtd_index: HashMap<String, PathBuf> = std::fs::read_dir(&mtd_dir)
        .with_context(|| format!("reading {}", mtd_dir.display()))?
        .filter_map(|e| {
            let e = e.ok()?;
            Some((e.file_name().to_str()?.to_ascii_lowercase(), e.path()))
        })
        .collect();

    // Skeleton nodes, in file order so node i of the skeleton FLVER is scene node i.
    let mut scene = Scene {
        nodes: Vec::new(),
        roots: Vec::new(),
        by_name: HashMap::new(),
    };
    for n in &model.skeleton.nodes {
        scene.add(&n.name, mirror_matrix(n.local_matrix()), None);
    }
    scene.roots.clear();
    for (i, n) in model.skeleton.nodes.iter().enumerate() {
        if n.parent >= 0 {
            scene.nodes[n.parent as usize].children.push(i);
        } else {
            scene.roots.push(i);
        }
        scene.by_name.entry(n.name.clone()).or_insert(i);
    }

    let mut texture_ids: HashMap<(String, Usage), Option<usize>> = HashMap::new();
    let mut stats = Vec::new();
    for part in &model.parts {
        for t in &part.tpfs {
            store.load_tpf(t)?;
        }
        // Parts hung on a dummy poly (`dmy:<id>`) get a node at the dummy's frame, parented to
        // the dummy's attach bone, so they follow that bone when animated. The part's model
        // space is the dummy's frame (`Dummy::frame`, see FORMATS.md).
        let dummy_attach = part.attach.as_deref().and_then(|a| a.strip_prefix("dmy:"));
        if let Some(id) = dummy_attach
            && !scene
                .by_name
                .contains_key(part.attach.as_deref().unwrap_or_default())
        {
            let id: i16 = id.parse()?;
            let d = model
                .skeleton
                .dummies
                .iter()
                .find(|d| d.reference_id == id)
                .with_context(|| format!("skeleton has no dummy {id}"))?;
            let skel_world = flver::world_matrices(&model.skeleton.nodes);
            let bone = usize::try_from(d.attach_bone).context("dummy without attach bone")?;
            let parent_world = usize::try_from(d.parent_bone)
                .ok()
                .map_or(Mat4::IDENTITY, |p| skel_world[p]);
            let frame = parent_world * d.frame();
            let local = mirror_matrix(skel_world[bone].inverse() * frame);
            let name = part.attach.clone().unwrap_or_default();
            let n = scene.add(&name, local, Some(bone));
            scene.by_name.insert(name, n);
        }
        let world = flver::world_matrices(&part.flver.nodes);
        let mut nodes = PartNodes {
            part,
            map: HashMap::new(),
        };
        let mut joint_slots: HashMap<usize, u16> = HashMap::new();
        let mut joints: Vec<usize> = Vec::new();
        let mut ibms: Vec<Mat4> = Vec::new();
        let mut primitives = Vec::new();
        let mut shown = 0;
        for (mi, mesh) in part.flver.meshes.iter().enumerate() {
            if !part.visible[mi] {
                continue;
            }
            let Some(lod0) = mesh.lod0() else { continue };
            let v = &mesh.vertices;
            let count = v.len();
            if count == 0 {
                continue;
            }
            shown += 1;
            let mut slot_of = |src: usize, scene: &mut Scene| -> u16 {
                if let Some(&s) = joint_slots.get(&src) {
                    return s;
                }
                let node = nodes.node(scene, src);
                joints.push(node);
                ibms.push(mirror_matrix(world[src]).inverse());
                let s = (joints.len() - 1) as u16;
                joint_slots.insert(src, s);
                s
            };
            let to_node = |b: u16| mesh.palette_node(b as usize);
            let default_bone = if mesh.node >= 0 {
                mesh.node as usize
            } else {
                0
            };
            let mut zero_weight = 0usize;
            let mut joint_data = Vec::with_capacity(count);
            let mut weight_data = Vec::with_capacity(count);
            for k in 0..count {
                let (idx, w) = match (v.bone_indices.get(k), v.bone_weights.get(k)) {
                    (Some(i), Some(w)) if mesh.dynamic => (*i, *w),
                    _ => ([0; 4], [0.0; 4]),
                };
                let total: f32 = w.iter().sum();
                if total <= 1e-6 {
                    zero_weight += 1;
                    let s = slot_of(default_bone, &mut scene);
                    joint_data.push([s, s, s, s]);
                    weight_data.push([1.0, 0.0, 0.0, 0.0]);
                    continue;
                }
                let mut j = [0u16; 4];
                let mut ww = [0f32; 4];
                let first = (0..4).find(|&c| w[c] > 0.0).unwrap_or(0);
                let fallback = slot_of(to_node(idx[first]), &mut scene);
                for c in 0..4 {
                    if w[c] > 0.0 {
                        j[c] = slot_of(to_node(idx[c]), &mut scene);
                        ww[c] = w[c] / total;
                    } else {
                        j[c] = fallback;
                    }
                }
                joint_data.push(j);
                weight_data.push(ww);
            }
            if verbose && zero_weight > 0 {
                eprintln!(
                    "    {} mesh {mi}: {zero_weight}/{count} vertices without weights",
                    part.label
                );
            }
            let positions: Vec<[f32; 3]> = v.positions.iter().map(|&p| mirror_point(p)).collect();
            let normals: Vec<[f32; 3]> = v
                .normals
                .iter()
                .map(|&n| {
                    let n = Vec3::from(mirror_point(n));
                    n.try_normalize().unwrap_or(Vec3::Y).into()
                })
                .collect();
            let indices = lod0.triangles(count);
            if indices.iter().any(|&i| i as usize >= count) {
                bail!("{}: mesh {mi} index out of range", part.label);
            }
            let mut attributes = json!({
                "POSITION": g.vec3(&positions, true),
                "JOINTS_0": g.joints(&joint_data),
                "WEIGHTS_0": g.vec4(&weight_data),
            });
            if normals.len() == count {
                attributes["NORMAL"] = json!(g.vec3(&normals, false));
            }
            for (c, uv) in v.uvs.iter().take(2).enumerate() {
                if uv.len() == count {
                    attributes[format!("TEXCOORD_{c}")] = json!(g.vec2(uv));
                }
            }

            let mtd_name = textures::texture_stem(&part.flver.materials[mesh.material].mtd)
                .to_ascii_lowercase();
            let mtd = mtd_index
                .get(&format!("{mtd_name}.mtd"))
                .map(|p| -> Result<mtd::Mtd> { Ok(mtd::parse(&std::fs::read(p)?)?) })
                .transpose()?;
            let mut ctx = MaterialContext {
                g: &mut g,
                store: &mut store,
                texture_ids: &mut texture_ids,
            };
            let mat = material_json(part, mi, lod0, mtd.as_ref(), &mtd_name, &mut ctx)?;
            if verbose {
                eprintln!(
                    "    {}.{}: {}",
                    part.label,
                    primitives.len(),
                    mat["name"].as_str().unwrap_or_default()
                );
            }
            g.materials.push(mat);
            let material = g.materials.len() - 1;
            primitives.push(json!({
                "attributes": attributes,
                "indices": g.indices(&indices),
                "material": material,
            }));
        }
        if verbose {
            // How far each part's bind pose is from the skeleton's rest pose, per shared bone.
            let skel_world = flver::world_matrices(&model.skeleton.nodes);
            for (&src, &slot) in &joint_slots {
                let name = &part.flver.nodes[src].name;
                let Some(&s) = scene.by_name.get(name) else {
                    continue;
                };
                if s >= skel_world.len() {
                    continue;
                }
                let diff = world[src] - skel_world[s];
                let d = diff
                    .to_cols_array()
                    .iter()
                    .fold(0.0f32, |m, v| m.max(v.abs()));
                if d > 0.002 {
                    eprintln!(
                        "    {}: bone {name} (joint {slot}) bind differs from skeleton (max matrix delta {d:.3})",
                        part.label
                    );
                }
            }
        }
        stats.push(format!(
            "{}: {shown}/{} meshes, {} joints",
            part.label,
            part.flver.meshes.len(),
            joints.len()
        ));
        if primitives.is_empty() {
            continue;
        }
        if verbose {
            let names: Vec<&str> = joints
                .iter()
                .map(|&n| scene.nodes[n].name.as_str())
                .collect();
            eprintln!("    {} joints: {}", part.label, names.join(" "));
        }
        let ibm = g.mat4s(&ibms);
        g.skins.push(json!({
            "name": part.label,
            "joints": joints,
            "inverseBindMatrices": ibm,
        }));
        g.meshes
            .push(json!({ "name": part.label, "primitives": primitives }));
        let n = scene.add(&part.label, Mat4::IDENTITY, None);
        scene.nodes[n].mesh = Some(g.meshes.len() - 1);
        scene.nodes[n].skin = Some(g.skins.len() - 1);
    }
    for s in &stats {
        eprintln!("  {s}");
    }

    for n in &scene.nodes {
        let (s, r, t) = n.local.to_scale_rotation_translation();
        let mut node = json!({
            "name": n.name,
            "translation": t.to_array(),
            "rotation": r.normalize().to_array(),
            "scale": s.to_array(),
        });
        if !n.children.is_empty() {
            node["children"] = json!(n.children);
        }
        if let Some(m) = n.mesh {
            node["mesh"] = json!(m);
        }
        if let Some(s) = n.skin {
            node["skin"] = json!(s);
        }
        g.nodes.push(node);
    }

    let dummies: Vec<Value> = model
        .skeleton
        .dummies
        .iter()
        .map(|d| {
            let name = |i: i16| {
                (i >= 0)
                    .then(|| model.skeleton.nodes.get(i as usize).map(|n| n.name.clone()))
                    .flatten()
            };
            json!({
                "id": d.reference_id,
                "attach": name(d.attach_bone),
                "parent": name(d.parent_bone),
                "position": mirror_point(d.position),
                "forward": mirror_point(d.forward),
                "upward": mirror_point(d.upward),
                "useUpward": d.use_upward,
            })
        })
        .collect();
    let extras = json!({
        "source": "sekiro-extract models",
        "convention": "FromSoftware x mirrored (x -> -x); see docs/FORMATS.md",
        "notes": model.notes,
        "drawMask": model.draw_masks.map(|m| m.0),
        "combatDrawMask": model.draw_masks.map(|m| m.1),
        "dummies": dummies,
    });
    Ok(g.finish(&scene.roots, extras))
}

struct MaterialContext<'a> {
    g: &'a mut Glb,
    store: &'a mut TextureStore,
    texture_ids: &'a mut HashMap<(String, Usage), Option<usize>>,
}

impl MaterialContext<'_> {
    /// Texture index of the packed metallic-roughness map for a normal map and metallic mask.
    fn metal_rough(
        &mut self,
        normal: Option<&str>,
        metallic: Option<&str>,
    ) -> Result<Option<usize>> {
        let stem = |p: Option<&str>| p.map(|p| textures::texture_stem(p).to_ascii_lowercase());
        let name = format!(
            "{}+{}_mr",
            stem(normal).unwrap_or_default(),
            stem(metallic).unwrap_or_default()
        );
        let key = (name.clone(), Usage::MetalRough);
        if let Some(&t) = self.texture_ids.get(&key) {
            return Ok(t);
        }
        let t = self
            .store
            .metal_rough_png(normal, metallic)?
            .map(|png| self.g.texture(&png, &name));
        self.texture_ids.insert(key, t);
        Ok(t)
    }

    fn texture(&mut self, path: &str, usage: Usage) -> Result<Option<usize>> {
        let key = (textures::texture_stem(path).to_ascii_lowercase(), usage);
        if let Some(&t) = self.texture_ids.get(&key) {
            return Ok(t);
        }
        let t = self
            .store
            .png(path, usage)?
            .map(|png| self.g.texture(&png, &key.0));
        self.texture_ids.insert(key, t);
        Ok(t)
    }
}

/// A glTF material for one FLVER mesh. Named `m<mesh> <material> | <owner node> | <mtd>` so
/// variants can be identified (and filtered in the viewer).
fn material_json(
    part: &Part,
    mesh_index: usize,
    lod0: &flver::FaceSet,
    mtd: Option<&mtd::Mtd>,
    mtd_name: &str,
    ctx: &mut MaterialContext,
) -> Result<Value> {
    let mesh = &part.flver.meshes[mesh_index];
    let fm = &part.flver.materials[mesh.material];
    let owner = usize::try_from(mesh.node)
        .ok()
        .and_then(|n| part.flver.nodes.get(n))
        .map(|n| n.name.as_str())
        .unwrap_or("-");
    let (albedo, normal, metallic) = material_textures(fm, mtd);
    let mut pbr = json!({ "metallicFactor": 0.0, "roughnessFactor": 0.75 });
    if let Some((path, uv)) = &albedo
        && let Some(t) = ctx.texture(path, Usage::Color)?
    {
        pbr["baseColorTexture"] = json!({ "index": t, "texCoord": uv });
    }
    let mut mat = json!({
        "name": format!("m{mesh_index} {} | {owner} | {mtd_name}", fm.name),
        "pbrMetallicRoughness": pbr,
        "doubleSided": !lod0.cull_backfaces,
    });
    if let Some((path, uv)) = &normal
        && let Some(t) = ctx.texture(path, Usage::Normal)?
    {
        mat["normalTexture"] = json!({ "index": t, "texCoord": uv });
    }
    // Shininess (normal map blue) and the metallic mask go in the glTF metallic-roughness slot.
    let (npath, nuv) = normal
        .as_ref()
        .map_or((None, 0), |(p, uv)| (Some(p.as_str()), *uv));
    let (mpath, muv) = metallic
        .as_ref()
        .map_or((None, 0), |(p, uv)| (Some(p.as_str()), *uv));
    if let Some(t) = ctx.metal_rough(npath, mpath)? {
        let pbr = &mut mat["pbrMetallicRoughness"];
        pbr["metallicFactor"] = json!(1.0);
        pbr["roughnessFactor"] = json!(1.0);
        pbr["metallicRoughnessTexture"] =
            json!({ "index": t, "texCoord": if npath.is_some() { nuv } else { muv } });
    }
    if let Some(m) = mtd {
        let int = |name: &str| match m.param(name) {
            Some(mtd::ParamValue::Ints(v)) => v.first().copied().unwrap_or(0),
            _ => 0,
        };
        let (alpha_ref, blend) = (int("g_AlphaRef"), int("g_BlendMode"));
        if blend == 2 || blend >= 4 {
            mat["alphaMode"] = json!("BLEND");
        } else if alpha_ref > 0 || blend == 1 || edge_cutout(mtd_name) {
            mat["alphaMode"] = json!("MASK");
            mat["alphaCutoff"] = json!((alpha_ref.max(1) as f32 / 255.0).max(0.3));
        }
    }
    Ok(mat)
}

/// MTD names with an `_e` (edge) part, e.g. `c1010_hat_e_decal`, cut their albedo alpha out even
/// when the material carries no alpha reference (the straw hat's weave and fraying are holes).
fn edge_cutout(mtd_name: &str) -> bool {
    let name = mtd_name.to_ascii_lowercase();
    name.contains("_e_") || name.ends_with("_e")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soldier_combat_mask_does_not_require_extracted_animations() {
        let cache = sekiro_formats::cache::root();
        let source = cache.join("raw/chr/c1010.anibnd.d/c1010.tae");
        if !source.is_file() {
            eprintln!("skipping: c1010 TAE is not unpacked");
            return;
        }
        // Only the unpacked TAE exists, as after `unpack` followed by `models`.
        let target = std::env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| "target".into());
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(target)
            .join(format!("tests/sword-mask-{}", std::process::id()));
        let raw = fixture.join("raw/chr/c1010.anibnd.d");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::copy(source, raw.join("c1010.tae")).unwrap();
        assert!(!fixture.join("anim/c1010/tae.json").exists());
        let event = combat_draw_mask_event(&fixture, "c1010").unwrap();
        assert_eq!(&event[..4], &[1, 0, 0, 1]);
        assert!(event[4..].iter().all(|&v| v == 255));
    }
}
