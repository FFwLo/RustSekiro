//! Sekiro map layouts (`MSB `, the "MSBS" variant): the models a map uses and the parts that
//! place them (map pieces, objects, enemies, player start points, collision).
//!
//! Layout knowledge from SoulsFormatsNEXT `MSBS` (ModelParam, PartsParam), reimplemented. Only
//! placed-part visibility/loading data and MapOffset provenance are retained.
//! Region identity, shapes and native SFX settings are also retained for map effects.
//! Other event payloads and routes are skipped over.
//!
//! Part transforms are in FromSoftware's left-handed space: position in metres, rotation in
//! degrees about X, Y and Z, composed like FLVER nodes (`T * Ry * Rz * Rx * S` in
//! column-vector form, see [`Part::matrix`]).

use crate::reader::{Reader, Result, bail, utf16z};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    MapPiece,
    Object,
    Enemy,
    Player,
    Collision,
    Other(u32),
}

impl ModelKind {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::MapPiece,
            1 => Self::Object,
            2 => Self::Enemy,
            4 => Self::Player,
            5 => Self::Collision,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub kind: ModelKind,
    pub sib_path: String,
    pub instance_count: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    MapPiece,
    Object,
    Enemy,
    Player,
    Collision,
    DummyObject,
    DummyEnemy,
    ConnectCollision,
    Other(u32),
}

impl PartKind {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::MapPiece,
            1 => Self::Object,
            2 => Self::Enemy,
            4 => Self::Player,
            5 => Self::Collision,
            9 => Self::DummyObject,
            10 => Self::DummyEnemy,
            11 => Self::ConnectCollision,
            other => Self::Other(other),
        }
    }
}

/// Enemy and dummy-enemy settings.
#[derive(Debug, Clone, Default)]
pub struct EnemyData {
    pub think_param_id: i32,
    pub npc_param_id: i32,
    /// Compatibility alias for SoulsFormats UnkT10, formerly described as TalkID.
    pub talk_id: i32,
    pub unk_t10: i32,
    pub platoon_id: i16,
    pub chara_init_id: i32,
    /// Index into the full parts table of the collision the enemy stands on, or -1.
    pub collision_part: i32,
    pub backup_event_anim_id: i32,
    pub event_flag_id: i32,
    /// Index within the PatrolInfo event list, or -1.
    pub walk_route_index: i16,
    pub init_anim_id_type: i16,
    pub unk_t24: i32,
    pub event_flag_compare_state: i32,
    pub unk_t48: i32,
    pub unk_t4c: i32,
    pub unk_t50: i32,
    pub unk_t78: i32,
    pub unk_t84: f32,
}

/// Collision (hit) part settings.
#[derive(Debug, Clone, Default)]
pub struct CollisionData {
    pub hit_filter_id: u8,
    pub sound_space_type: u8,
    pub map_name_id: i16,
    pub disable_start: bool,
    pub play_region_id: i32,
    pub lock_cam_param_id: i16,
    pub reflect_plane_height: f32,
    pub unk_t17: u8,
    pub disable_bonfire_entity_id: i32,
    pub unk_t24: u8,
    pub unk_t25: u8,
    pub unk_t26: u8,
    pub map_visibility: u8,
    pub unk_t3c: i32,
    pub unk_t40: i32,
    pub unk_t44: f32,
    pub unk_t48: f32,
    pub unk_t4c: i32,
    pub unk_t50: f32,
    pub unk_t54: f32,
}

/// SoulsFormats DisplayDataStruct (formerly UnkStruct1).
/// MSBS has no separately identified BackreadGroups field; do not reinterpret
/// CollisionMask as that field without evidence of its retail semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayDataStruct {
    pub display_groups: [u32; 8],
    pub draw_groups: [u32; 8],
    pub collision_mask: [u32; 32],
    pub condition1: u8,
    pub condition2: u8,
}

/// SoulsFormats DisplayGroupStruct (formerly UnkStruct2).
/// Present on Collision and ConnectCollision, separately from DisplayDataStruct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayGroupStruct {
    pub condition: i32,
    pub disp_groups: [i32; 8],
    pub unk24: i16,
    pub unk26: i16,
}

/// ObjectBase data shared by Object and DummyObject.
#[derive(Debug, Clone)]
pub struct ObjectData {
    /// All three ObjPartIndex fields index the full parts table, or are -1.
    /// The first is believed to control loading; the latter two are described
    /// as collision references associated with grappling, but are NOT indices
    /// into the collision-only list (unlike ConnectCollision).
    pub obj_part_index1: i32,
    pub obj_part_index2: i32,
    pub obj_part_index3: i32,
    pub break_term: u8,
    pub net_sync_type: bool,
    pub unk_t0e: u8,
    pub set_main_obj_structure_booleans: bool,
    pub anim_id: i16,
    pub model_sfx_param_relative_id: i16,
    pub unk_t1a: i16,
}

/// Remaining common entity flags, with SoulsFormats names and unknowns intact.
#[derive(Debug, Clone, Default)]
pub struct EntityData {
    pub unk_e04: u8,
    pub unk_e05: u8,
    pub unk_e06: u8,
    pub lantern_id: u8,
    pub unk_e09: u8,
    pub is_point_light_shadow_src: bool,
    pub unk_e0b: u8,
    pub is_shadow_src: bool,
    pub is_static_shadow_src: u8,
    pub is_cascade3_shadow_src: u8,
    pub unk_e0f: u8,
    pub unk_e10: u8,
    pub is_shadow_dest: bool,
    pub draw_by_reflect_cam: bool,
    pub draw_only_reflect_cam: bool,
    pub enable_on_above_shadow: u8,
    pub disable_point_light_effect: bool,
    pub unk_e17: u8,
    pub unk_e18: i32,
    pub unk_e3c: i32,
    pub unk_e40: i32,
}

/// SoulsFormats SceneGparamConfig on collision parts.
#[derive(Debug, Clone)]
pub struct SceneGparamConfig {
    pub event_ids: [i8; 4],
    pub transition_time: f32,
}

/// SoulsFormats GrassConfigStruct (formerly UnkStruct7), on map pieces.
#[derive(Debug, Clone)]
pub struct GrassConfigStruct {
    pub unk00: i32,
    pub unk04: i32,
    pub grass_type_param_id: i32,
    pub unk0c: i32,
    pub unk10: i32,
    pub unk14: i32,
}

#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub kind: PartKind,
    /// Type-local ID stored in the part header, not an entity or table index.
    pub id: i32,
    /// Index into [`Msb::models`], or -1.
    pub model_index: i32,
    pub sib_path: String,
    pub position: [f32; 3],
    /// Euler angles in degrees.
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub entity_id: i32,
    pub entity_group_ids: [i32; 8],
    /// Compatibility view: DisplayGroups[8], DrawGroups[8], CollisionMask[32].
    /// Empty when DisplayDataStruct is absent; the first words are NOT DrawGroups.
    pub masks: Vec<u32>,
    /// Compatibility bit-pattern view of DisplayGroupStruct.DispGroups.
    /// Present on Collision and ConnectCollision, empty otherwise.
    pub disp_groups: Vec<u32>,
    pub display_data: Option<DisplayDataStruct>,
    pub display_group: Option<DisplayGroupStruct>,
    pub entity_data: EntityData,
    pub lod_param_id: u8,
    pub is_shadow_only: bool,
    pub enemy: Option<EnemyData>,
    pub collision: Option<CollisionData>,
    pub object: Option<ObjectData>,
    /// For connect collisions: (index within the collision-only list, target map ID bytes).
    /// An index of -1 explicitly means no attached collision.
    /// Resolve with Msb::collision_by_index, never Msb::parts[index] or the part name.
    /// Map bytes of 0xFF remain wildcards; no candidate-selection policy is applied here.
    pub connect: Option<(i32, [u8; 4])>,
    /// Draw-param selection: light set, fog, light scattering and environment map ids.
    pub gparam: Option<[i32; 4]>,
    pub scene_gparam: Option<SceneGparamConfig>,
    pub grass_config: Option<GrassConfigStruct>,
}

impl Part {
    /// Local-to-map transform in FromSoftware space (column-vector convention).
    pub fn matrix(&self) -> glam::Mat4 {
        let [rx, ry, rz] = self.rotation.map(f32::to_radians);
        let rotation = glam::Quat::from_rotation_y(ry)
            * glam::Quat::from_rotation_z(rz)
            * glam::Quat::from_rotation_x(rx);
        glam::Mat4::from_scale_rotation_translation(
            self.scale.into(),
            rotation,
            self.position.into(),
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct Msb {
    pub models: Vec<Model>,
    pub parts: Vec<Part>,
    pub env_regions: Vec<EnvRegion>,
    pub map_offsets: Vec<MapOffset>,
    pub regions: Vec<MapRegion>,
    pub patrols: Vec<PatrolInfo>,
    pub treasures: Vec<Treasure>,
}

/// MSBS type-4 Treasure event: an item lot picked up at a part (SoulsFormats `MSBS.Event.Treasure`).
#[derive(Debug, Clone)]
pub struct Treasure {
    pub name: String,
    /// Index into the full parts table, or -1.
    pub part_index: i32,
    /// ItemLotParam row, or -1.
    pub item_lot: i32,
    /// ActionButtonParam row, -1 for the default pick-up button.
    pub action_button: i32,
    pub in_chest: bool,
    pub start_disabled: bool,
}

/// MSBS type-14 PatrolInfo, in event-list order (EnemyData::walk_route_index).
#[derive(Debug, Clone)]
pub struct PatrolInfo {
    pub kind: i32,
    pub regions: [i16; 32],
    pub arrivals: [(i16, i32, i32); 5],
}

/// MSBS region identity and effect placement, in native map space.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MapRegion {
    pub name: String,
    pub kind: u32,
    pub entity_id: i32,
    pub activation_part: i32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub shape: u32,
    pub dimensions: Vec<f32>,
    pub children: Vec<i32>,
    pub effect: Option<i32>,
    pub start_disabled: bool,
}

fn read_map_region(data: &[u8], start: usize) -> Result<MapRegion> {
    let mut r = Reader::at(data, start);
    let name = utf16z(data, start + r.offset64()?)?;
    let kind = r.u32()?;
    r.skip(4);
    let shape = r.u32()?;
    let position = r.vec3()?;
    let rotation = r.vec3()?;
    let shape_offset = Reader::at(data, start + 0x48).offset64()?;
    let base_offset = Reader::at(data, start + 0x50).offset64()?;
    let type_offset = Reader::at(data, start + 0x58).offset64()?;
    let mut base = Reader::at(data, start + base_offset);
    let activation_part = base.i32()?;
    let entity_id = base.i32()?;
    let mut dimensions = Vec::new();
    let mut children = Vec::new();
    if shape_offset != 0 {
        let mut s = Reader::at(data, start + shape_offset);
        if shape == 6 {
            for _ in 0..8 {
                children.push(s.i32()?);
                s.skip(4);
            }
        } else {
            for _ in 0..match shape {
                1 | 2 => 1,
                3 | 4 => 2,
                5 => 3,
                _ => 0,
            } {
                dimensions.push(s.f32()?);
            }
        }
    }
    let effect = if matches!(kind, 5 | 6) {
        Some(Reader::at(data, start + type_offset).i32()?)
    } else {
        None
    };
    let start_disabled = kind == 5 && Reader::at(data, start + type_offset + 20).i32()? != 0;
    Ok(MapRegion {
        name,
        kind,
        entity_id,
        activation_part,
        position,
        rotation,
        shape,
        dimensions,
        children,
        effect,
        start_disabled,
    })
}

/// MapOffset event provenance for ancillary resources.
/// Its position/rotation are ALREADY accounted for in placed MSB coordinates.
/// Parsing this event does not apply another transform to parts.
#[derive(Debug, Clone)]
pub struct MapOffset {
    pub name: String,
    pub event_id: i32,
    /// Type-local event ID stored in the event header.
    pub id: i32,
    /// Byte offset of this event in the source MSB, for audit diagnostics.
    pub source_offset: usize,
    /// Base-event references into the full parts and regions tables, or -1.
    pub part_index: i32,
    pub region_index: i32,
    pub entity_id: i32,
    pub position: [f32; 3],
    pub rotation_y: f32,
}

/// Environment probe placement, in the same space and rotation convention as parts.
#[derive(Debug, Clone)]
pub struct EnvRegion {
    pub name: String,
    pub probe: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Full box dimensions (width, height, depth); position is the bottom centre.
    /// None for an environment point.
    pub size: Option<[f32; 3]>,
}

impl Msb {
    /// Resolve a full-table reference (ObjectBase, EnemyBase or event), or -1.
    pub fn part_by_index(&self, index: i32) -> Option<&Part> {
        usize::try_from(index).ok().and_then(|i| self.parts.get(i))
    }

    /// Resolve ConnectCollision.CollisionIndex within Parts.Collisions.
    pub fn collision_by_index(&self, index: i32) -> Option<&Part> {
        let index = usize::try_from(index).ok()?;
        self.parts
            .iter()
            .filter(|p| p.kind == PartKind::Collision)
            .nth(index)
    }

    pub fn model(&self, part: &Part) -> Option<&Model> {
        usize::try_from(part.model_index)
            .ok()
            .and_then(|i| self.models.get(i))
    }
}

pub fn is_msb(data: &[u8]) -> bool {
    data.starts_with(b"MSB ")
}

pub fn parse(data: &[u8]) -> Result<Msb> {
    let mut r = Reader::new(data);
    r.magic(b"MSB ")?;
    if r.i32()? != 1 || r.i32()? != 0x10 {
        return bail("unexpected MSB header");
    }
    let mut msb = Msb::default();
    let mut pos = 0x10;
    let mut guard = 0;
    while pos != 0 {
        guard += 1;
        if guard > 32 {
            return bail("too many MSB params");
        }
        let mut p = Reader::at(data, pos);
        let _version = p.i32()?;
        let offset_count = p.i32()? as usize;
        let name_offset = p.offset64()?;
        let mut entries = Vec::with_capacity(offset_count.saturating_sub(1));
        for _ in 1..offset_count {
            entries.push(p.offset64()?);
        }
        let next = p.offset64()?;
        let name = utf16z(data, name_offset)?;
        match name.as_str() {
            "MODEL_PARAM_ST" => {
                for &e in &entries {
                    msb.models.push(read_model(data, e)?);
                }
            }
            "PARTS_PARAM_ST" => {
                for &e in &entries {
                    msb.parts.push(read_part(data, e)?);
                }
            }
            "POINT_PARAM_ST" => {
                for &e in &entries {
                    msb.regions.push(read_map_region(data, e)?);
                    if let Some(region) = read_env_region(data, e)? {
                        msb.env_regions.push(region);
                    }
                }
            }
            "EVENT_PARAM_ST" => {
                for &e in &entries {
                    if let Some(route) = read_patrol(data, e)? {
                        msb.patrols.push(route);
                    }
                    if let Some(offset) = read_map_offset(data, e)? {
                        msb.map_offsets.push(offset);
                    }
                    if let Some(treasure) = read_treasure(data, e)? {
                        msb.treasures.push(treasure);
                    }
                }
            }
            _ => {}
        }
        pos = next;
    }
    Ok(msb)
}

fn read_patrol(data: &[u8], start: usize) -> Result<Option<PatrolInfo>> {
    let mut header = Reader::at(data, start + 12);
    if header.u32()? != 14 {
        return Ok(None);
    }
    let offset = Reader::at(data, start + 32).offset64()?;
    let mut r = Reader::at(data, start + offset);
    let kind = r.i32()?;
    r.skip(12);
    let mut regions = [-1; 32];
    for region in &mut regions {
        *region = r.i16()?;
    }
    let mut arrivals = [(-1, -1, 0); 5];
    for arrival in &mut arrivals {
        let region = r.i16()?;
        r.skip(2);
        *arrival = (region, r.i32()?, r.i32()?);
    }
    Ok(Some(PatrolInfo {
        kind,
        regions,
        arrivals,
    }))
}

fn read_map_offset(data: &[u8], start: usize) -> Result<Option<MapOffset>> {
    let mut r = Reader::at(data, start);
    let name_offset = r.offset64()?;
    let event_id = r.i32()?;
    if r.u32()? != 9 {
        return Ok(None);
    }
    let id = r.i32()?;
    r.skip(4);
    let base_offset = r.offset64()?;
    let type_offset = r.offset64()?;
    let mut base = Reader::at(data, start + base_offset);
    let mut t = Reader::at(data, start + type_offset);
    Ok(Some(MapOffset {
        name: utf16z(data, start + name_offset)?,
        event_id,
        id,
        source_offset: start,
        part_index: base.i32()?,
        region_index: base.i32()?,
        entity_id: base.i32()?,
        position: t.vec3()?,
        rotation_y: t.f32()?,
    }))
}

fn read_treasure(data: &[u8], start: usize) -> Result<Option<Treasure>> {
    let mut r = Reader::at(data, start);
    let name_offset = r.offset64()?;
    r.skip(4);
    if r.u32()? != 4 {
        return Ok(None);
    }
    let type_offset = Reader::at(data, start + 0x20).offset64()?;
    // Type data: 8 unused bytes, part index, 4 unused, item lot, 0x24 bytes of 0xFF, then
    // ActionButtonID, PickupAnimID (unused here), InChest and StartDisabled.
    let mut t = Reader::at(data, start + type_offset + 8);
    let part_index = t.i32()?;
    t.skip(4);
    let item_lot = t.i32()?;
    t.skip(0x24);
    let action_button = t.i32()?;
    t.skip(4);
    let flags = t.bytes(2)?;
    Ok(Some(Treasure {
        name: utf16z(data, start + name_offset)?,
        part_index,
        item_lot,
        action_button,
        in_chest: flags[0] != 0,
        start_disabled: flags[1] != 0,
    }))
}

fn read_words<const N: usize>(r: &mut Reader<'_>) -> Result<[u32; N]> {
    let mut words = [0; N];
    for word in &mut words {
        *word = r.u32()?;
    }
    Ok(words)
}

fn read_env_region(data: &[u8], start: usize) -> Result<Option<EnvRegion>> {
    let mut r = Reader::at(data, start);
    let name_offset = r.offset64()?;
    let kind = r.u32()?;
    if kind != 2 && kind != 17 {
        return Ok(None);
    }
    r.skip(4);
    let shape = r.u32()?;
    let position = r.vec3()?;
    let rotation = r.vec3()?;
    let name = utf16z(data, start + name_offset)?;
    let prefix = if kind == 17 { "Env_Box" } else { "Env_Point" };
    let Some(suffix) = name.strip_prefix(prefix) else {
        return Ok(None);
    };
    let digits: String = suffix.chars().take_while(char::is_ascii_digit).collect();
    let probe = digits
        .parse()
        .map_err(|_| crate::reader::Error::Format("invalid environment probe name".into()))?;
    let size = if kind == 17 {
        if shape != 5 {
            return bail(format!("environment box {name} has shape {shape}"));
        }
        let offset = Reader::at(data, start + 0x48).offset64()?;
        let [width, depth, height] = Reader::at(data, start + offset).vec3()?;
        Some([width, height, depth])
    } else {
        None
    };
    Ok(Some(EnvRegion {
        name,
        probe,
        position,
        rotation,
        size,
    }))
}

fn read_model(data: &[u8], start: usize) -> Result<Model> {
    let mut r = Reader::at(data, start);
    let name_offset = r.offset64()?;
    let kind = ModelKind::from_u32(r.u32()?);
    let _id = r.i32()?;
    let sib_offset = r.offset64()?;
    let instance_count = r.i32()?;
    Ok(Model {
        name: utf16z(data, start + name_offset)?,
        kind,
        sib_path: utf16z(data, start + sib_offset)?,
        instance_count,
    })
}

fn read_part(data: &[u8], start: usize) -> Result<Part> {
    let mut r = Reader::at(data, start);
    let name_offset = r.offset64()?;
    let kind = PartKind::from_u32(r.u32()?);
    let id = r.i32()?;
    let model_index = r.i32()?;
    r.skip(4);
    let sib_offset = r.offset64()?;
    let position = r.vec3()?;
    let rotation = r.vec3()?;
    let scale = r.vec3()?;
    r.skip(12);
    let unk1 = r.i64()?;
    let unk2 = r.i64()?;
    let entity = r.offset64()?;
    let type_data = r.offset64()?;
    let gparam = r.i64()?;
    let scene_gparam = r.i64()?;
    let grass_config = r.i64()?;

    let mut part = Part {
        name: utf16z(data, start + name_offset)?,
        kind,
        id,
        model_index,
        sib_path: utf16z(data, start + sib_offset)?,
        position,
        rotation,
        scale,
        entity_id: -1,
        entity_group_ids: [-1; 8],
        masks: Vec::new(),
        disp_groups: Vec::new(),
        display_data: None,
        display_group: None,
        entity_data: EntityData::default(),
        lod_param_id: 0,
        is_shadow_only: false,
        enemy: None,
        collision: None,
        object: None,
        connect: None,
        gparam: None,
        scene_gparam: None,
        grass_config: None,
    };
    if gparam > 0 {
        let mut g = Reader::at(data, start + gparam as usize);
        part.gparam = Some([g.i32()?, g.i32()?, g.i32()?, g.i32()?]);
    }
    if scene_gparam > 0 {
        let mut g = Reader::at(data, start + scene_gparam as usize + 0x3c);
        part.scene_gparam = Some(SceneGparamConfig {
            event_ids: [g.i8()?, g.i8()?, g.i8()?, g.i8()?],
            transition_time: g.f32()?,
        });
    }
    if grass_config > 0 {
        let mut g = Reader::at(data, start + grass_config as usize);
        part.grass_config = Some(GrassConfigStruct {
            unk00: g.i32()?,
            unk04: g.i32()?,
            grass_type_param_id: g.i32()?,
            unk0c: g.i32()?,
            unk10: g.i32()?,
            unk14: g.i32()?,
        });
    }

    if unk1 > 0 {
        let mut u = Reader::at(data, start + unk1 as usize);
        let display = DisplayDataStruct {
            display_groups: read_words(&mut u)?,
            draw_groups: read_words(&mut u)?,
            collision_mask: read_words(&mut u)?,
            condition1: u.u8()?,
            condition2: u.u8()?,
        };
        part.masks.extend(display.display_groups);
        part.masks.extend(display.draw_groups);
        part.masks.extend(display.collision_mask);
        part.display_data = Some(display);
    }
    if unk2 > 0 {
        let mut u = Reader::at(data, start + unk2 as usize);
        let condition = u.i32()?;
        let disp_groups = read_words::<8>(&mut u)?.map(|word| word as i32);
        part.disp_groups.extend(disp_groups.map(|word| word as u32));
        part.display_group = Some(DisplayGroupStruct {
            condition,
            disp_groups,
            unk24: u.i16()?,
            unk26: u.i16()?,
        });
    }

    let mut e = Reader::at(data, start + entity);
    part.entity_id = e.i32()?;
    let flags = &mut part.entity_data;
    flags.unk_e04 = e.u8()?;
    flags.unk_e05 = e.u8()?;
    flags.unk_e06 = e.u8()?;
    flags.lantern_id = e.u8()?;
    part.lod_param_id = e.u8()?;
    flags.unk_e09 = e.u8()?;
    flags.is_point_light_shadow_src = e.u8()? != 0;
    flags.unk_e0b = e.u8()?;
    flags.is_shadow_src = e.u8()? != 0;
    flags.is_static_shadow_src = e.u8()?;
    flags.is_cascade3_shadow_src = e.u8()?;
    flags.unk_e0f = e.u8()?;
    flags.unk_e10 = e.u8()?;
    flags.is_shadow_dest = e.u8()? != 0;
    part.is_shadow_only = e.u8()? != 0; // E12
    flags.draw_by_reflect_cam = e.u8()? != 0;
    flags.draw_only_reflect_cam = e.u8()? != 0;
    flags.enable_on_above_shadow = e.u8()?;
    flags.disable_point_light_effect = e.u8()? != 0;
    flags.unk_e17 = e.u8()?;
    flags.unk_e18 = e.i32()?;
    for g in &mut part.entity_group_ids {
        *g = e.i32()?;
    }
    flags.unk_e3c = e.i32()?;
    flags.unk_e40 = e.i32()?;

    let mut t = Reader::at(data, start + type_data);
    match kind {
        PartKind::Object | PartKind::DummyObject => {
            t.skip(8);
            let obj_part_index1 = t.i32()?;
            let break_term = t.u8()?;
            let net_sync_type = t.u8()? != 0;
            let unk_t0e = t.u8()?;
            let set_main_obj_structure_booleans = t.u8()? != 0;
            let anim_id = t.i16()?;
            t.skip(6);
            let model_sfx_param_relative_id = t.i16()?;
            let unk_t1a = t.i16()?;
            t.skip(4);
            part.object = Some(ObjectData {
                obj_part_index1,
                obj_part_index2: t.i32()?,
                obj_part_index3: t.i32()?,
                break_term,
                net_sync_type,
                unk_t0e,
                set_main_obj_structure_booleans,
                anim_id,
                model_sfx_param_relative_id,
                unk_t1a,
            });
        }
        PartKind::Enemy | PartKind::DummyEnemy => {
            t.skip(8);
            let think_param_id = t.i32()?;
            let npc_param_id = t.i32()?;
            let talk_id = t.i32()?;
            t.skip(2);
            let platoon_id = t.i16()?;
            let chara_init_id = t.i32()?;
            let collision_part = t.i32()?;
            let walk_route_index = t.i16()?;
            let init_anim_id_type = t.i16()?;
            let unk_t24 = t.i32()?;
            t.skip(16);
            let backup_event_anim_id = t.i32()?;
            t.skip(4);
            let event_flag_id = t.i32()?;
            let event_flag_compare_state = t.i32()?;
            let unk_t48 = t.i32()?;
            let unk_t4c = t.i32()?;
            let unk_t50 = t.i32()?;
            t.skip(36);
            let unk_t78 = t.i32()?;
            t.skip(8);
            let unk_t84 = t.f32()?;
            part.enemy = Some(EnemyData {
                think_param_id,
                npc_param_id,
                talk_id,
                unk_t10: talk_id,
                platoon_id,
                chara_init_id,
                collision_part,
                backup_event_anim_id,
                event_flag_id,
                walk_route_index,
                init_anim_id_type,
                unk_t24,
                event_flag_compare_state,
                unk_t48,
                unk_t4c,
                unk_t50,
                unk_t78,
                unk_t84,
            });
        }
        PartKind::Collision => {
            let hit_filter_id = t.u8()?;
            let sound_space_type = t.u8()?;
            t.skip(2);
            let reflect_plane_height = t.f32()?;
            t.skip(12);
            let map_name_id = t.i16()?;
            let disable_start = t.u8()? != 0;
            let unk_t17 = t.u8()?;
            let disable_bonfire_entity_id = t.i32()?;
            t.skip(8);
            let unk_t24 = t.u8()?;
            let unk_t25 = t.u8()?;
            let unk_t26 = t.u8()?;
            let map_visibility = t.u8()?;
            let play_region_id = t.i32()?;
            let lock_cam_param_id = t.i16()?;
            t.skip(14);
            part.collision = Some(CollisionData {
                hit_filter_id,
                sound_space_type,
                map_name_id,
                disable_start,
                play_region_id,
                lock_cam_param_id,
                reflect_plane_height,
                unk_t17,
                disable_bonfire_entity_id,
                unk_t24,
                unk_t25,
                unk_t26,
                map_visibility,
                unk_t3c: t.i32()?,
                unk_t40: t.i32()?,
                unk_t44: t.f32()?,
                unk_t48: t.f32()?,
                unk_t4c: t.i32()?,
                unk_t50: t.f32()?,
                unk_t54: t.f32()?,
            });
        }
        PartKind::ConnectCollision => {
            let index = t.i32()?;
            let map: [u8; 4] = t.bytes(4)?.try_into().unwrap();
            part.connect = Some((index, map));
        }
        _ => {}
    }
    Ok(part)
}

#[cfg(test)]
mod env_tests {
    #[test]
    fn castle_probe_regions() {
        let path = crate::cache::root().join("raw/map/mapstudio/m11_01_00_00.msb");
        if !path.exists() {
            return;
        }
        let msb = super::parse(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            msb.env_regions.iter().filter(|r| r.size.is_some()).count(),
            107
        );
        assert_eq!(
            msb.env_regions.iter().filter(|r| r.size.is_none()).count(),
            107
        );
        assert!(msb.env_regions.iter().all(|r| {
            r.size
                .is_none_or(|s| s.iter().all(|x| x.is_finite() && *x > 0.))
        }));
    }

    #[test]
    fn ashina_outskirts_treasures() {
        let path = crate::cache::root().join("raw/map/mapstudio/m11_00_00_00.msb");
        if !path.exists() {
            return;
        }
        let msb = super::parse(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(msb.treasures.len(), 73);
        assert_eq!(
            msb.treasures.iter().filter(|t| t.start_disabled).count(),
            16
        );
        let pellet = msb
            .treasures
            .iter()
            .find(|t| t.name == "Item_宝_02")
            .unwrap();
        assert_eq!((pellet.item_lot, pellet.action_button), (1100020, -1));
        let part = msb.part_by_index(pellet.part_index).unwrap();
        assert!((part.position[0] - 146.2).abs() < 0.01);
    }
}
