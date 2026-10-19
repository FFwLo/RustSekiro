//! Clash effects: the sparks of a deflect or guard and the blood of a hit. combat.rs spawns a
//! `Clash` request at the contact point with the game's effect ids for it (`hit_sfx_turned`:
//! HitEffectSfx* params); fxr.rs plays those. A row whose effect is not in the game's archives
//! (206000-207004, 211000-211004, 234xxx, 243001-247002, 254000-254004: no f<id>.fxr in any of the
//! eight sfxbnd archives the game ships) shows nothing, as in the game; the hand-made sparks,
//! flash and droplets that stood in for them are gone.

use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Perfect deflect: the big burst.
    Deflect,
    /// Blocked (guard) or an enemy's deflect against Wolf.
    Guard,
    /// Flesh hit.
    Hit,
}

/// A request from combat: one clash at `pos`, its effects facing `normal` (toward the attacker).
#[derive(Component)]
pub struct Clash {
    pub normal: Vec3,
    /// The game's effects for it (FFX ids, `hit_sfx_turned`).
    pub sfx: Vec<i64>,
    /// Each effect's world rotation from HitEffectSfxAngleParam (`hit_sfx_turned`); None (or
    /// missing) keeps the look-at toward the attacker.
    pub turns: Vec<Option<Quat>>,
}

impl Clash {
    pub fn bundle(pos: Vec3, normal: Vec3, fx: Vec<(i64, Option<Quat>)>) -> impl Bundle {
        let (sfx, turns) = fx.into_iter().unzip();
        (Clash { normal: normal.normalize_or(Vec3::Y), sfx, turns }, Transform::from_translation(pos))
    }
}

/// HitEffectSfxConcept(JustGuard)Param column groups in paramdef order, picked by the attack's
/// atkMaterial_forSfx (the same order as the sound tables' groups, sound::HIT_SE_GROUPS).
const SFX_GROUPS: [&str; 15] =
    ["Iron", "Fire", "Wood", "Body", "Eclipse", "Energy", "None", "Dmy1", "Dmy2", "Dmy3", "Maggot", "Wax", "FireFlame", "EclipseGas", "EnergyStrong"];

/// The game's effects for a clash. For each material row (deflect / guard: the attack's
/// defSfxMaterial1/2; hit: the defender's materialSfx1/2, Wolf's protector defenseMaterialSfx1/2)
/// the concept table (deflect: HitEffectSfxConceptJustGuardParam, e.g. 100 "Weapon iron" ->
/// atkIron_1 520 "Jasuga sparks"; guard / hit: HitEffectSfxConceptParam) gives up to two concepts
/// (atk<group>_1 / _2), and HitEffectSfxParam[concept] at the attack's type (Slash / Blow / Thrust)
/// and size (atkPow_forSfx: S M L LL LLL) the FFX id: the General's sword deflected -> 252001,
/// guarded -> 201002, on his armour -> 201001 + 229001 ("meat under armour").
/// gap: the exe's SFX path is not traced; a deflect falls back to HitEffectSfxConceptParam when the
/// JustGuard table has nothing, as the traced sound path does (sound::guard_sounds).
#[cfg(test)]
pub fn hit_sfx(combat: &crate::data::Combat, atk: &crate::data::Attack, kind: Kind, def_materials: [i64; 2]) -> Vec<i64> {
    hit_sfx_turned(combat, atk, kind, def_materials).into_iter().map(|(id, _)| id).collect()
}

/// `hit_sfx` with each effect's turn: the spark spawn (hit handler FUN_140888960 -> FXHGSfxCtrl
/// FUN_140929dd0 -> FUN_14092b6c0, 14092.c:7843-8042) looks the HitEffectSfxParam row up in
/// HitEffectSfxAngleParam. Where that row exists, Euler(X, Y, Z) of the attack's atkDir_forSfx
/// (1 Upper, 2 Bottom, 3 Left, 4 Right; else 0, 0, 0; degrees, FUN_1406cd620) **replaces** the
/// look-at rotation as an absolute world rotation; only the position is kept. The event path
/// FUN_140b63fe0 (character-relative) is the hit-sound dispatcher, not this. atkDir<Dir> swaps
/// the row (210 -> 211, which does not exist: look-at). FUN_1406cd620 -> FUN_1401b9820
/// (1401b.c:5245-5407) is DirectXMath XMMatrixRotationRollPitchYaw(X, Y, Z): row-vector
/// Rz Rx Ry (Z first), i.e. Qy Qx Qz, which is fxr::game_rot (its Y / Z negated for the X mirror).
pub fn hit_sfx_turned(combat: &crate::data::Combat, atk: &crate::data::Attack, kind: Kind, def_materials: [i64; 2]) -> Vec<(i64, Option<Quat>)> {
    let turn = |row: i64| -> Option<Quat> {
        let dir = ["", "Upper", "Bottom", "Left", "Right"].get(atk.atk_dir_sfx as usize).copied().unwrap_or("");
        let angles = combat.params.get("HitEffectSfxAngleParam")?;
        let mut r = angles.get(row.to_string())?;
        if !dir.is_empty() {
            let swap = r[format!("atkDir{dir}")].as_i64().unwrap_or(-1);
            if swap >= 0 {
                r = angles.get(swap.to_string())?;
            }
        }
        let a = |ax: &str| if dir.is_empty() { 0.0 } else { r[format!("{ax}AxisAngle_AttackDir{dir}")].as_f64().unwrap_or(0.0) as f32 };
        Some(crate::fxr::game_rot([a("X"), a("Y"), a("Z"), 0.0]))
    };
    let Some(group) = usize::try_from(atk.atk_material_sfx).ok().and_then(|g| SFX_GROUPS.get(g)) else { return Vec::new() };
    let col = format!(
        "{}_{}",
        match atk.atk_type {
            1 => "Blow",
            2 => "Thrust",
            _ => "Slash",
        },
        ["S", "M", "L", "LL", "LLL"][atk.atk_pow_sfx.clamp(0, 4) as usize]
    );
    let look = |table: &str, mats: [i64; 2]| -> Vec<(i64, Option<Quat>)> {
        let (Some(t), Some(sfx)) = (combat.params.get(table), combat.params.get("HitEffectSfxParam")) else { return Vec::new() };
        let mut out = Vec::new();
        for m in mats.into_iter().filter(|m| *m >= 0) {
            for n in 1..=2 {
                let concept = t.get(m.to_string()).and_then(|r| r[format!("atk{group}_{n}")].as_i64()).unwrap_or(0);
                if let Some(id) = (concept > 0).then(|| sfx.get(concept.to_string()).and_then(|r| r[&col].as_i64())).flatten().filter(|id| *id > 0) {
                    if !out.iter().any(|(o, _)| *o == id) {
                        out.push((id, turn(concept)));
                    }
                }
            }
        }
        out
    };
    let guard = guard_materials(combat, [atk.def_sfx_material1, atk.def_sfx_material2], false);
    match kind {
        Kind::Deflect => {
            let jg = look("HitEffectSfxConceptJustGuardParam", guard);
            if jg.is_empty() { look("HitEffectSfxConceptParam", guard) } else { jg }
        }
        Kind::Guard => look("HitEffectSfxConceptParam", guard),
        Kind::Hit => look("HitEffectSfxConceptParam", def_materials),
    }
}

/// An attack's two guard materials (defSfxMaterial1/2, or with `se` defSeMaterial1/2) as the exe
/// resolves them: 255 = the attacker's weapon's (FUN_140ba8a50 SFX / FUN_140ba8840 SE: AtkParam
/// +0x188 / +0x18c (SE +0x82 / +0x18a) == 0xff -> the attacker's EquipParamWeapon
/// defSfxMaterial1/2 +0x106 / +0x16c (SE defSeMaterial1/2 +0xe6 / +0x16e), 100 without a weapon
/// row; a chr without weapons (vfunc +0x1a8 false) takes its own default instead). Only Wolf's
/// AtkParam_Pc rows hold 255 (281 rows; no AtkParam_Npc row does), and every Wolf weapon row
/// (5000, 70000-79000) holds 101 / 139, so his sword row stands for "the weapon".
/// gap: which weapon the exe picks for a tool attack (vfunc +0x348); the values are the same.
pub fn guard_materials(combat: &crate::data::Combat, mats: [i64; 2], se: bool) -> [i64; 2] {
    let weapon = combat.param("EquipParamWeapon", 5000);
    let names = if se { ["defSeMaterial1", "defSeMaterial2"] } else { ["defSfxMaterial1", "defSfxMaterial2"] };
    std::array::from_fn(|i| if mats[i] == 255 { weapon[names[i]].as_i64().unwrap_or(100) } else { mats[i] })
}

/// The defender's two hit-effect materials: the enemy's NpcParam materialSfx1/2, Wolf's body
/// protector defenseMaterialSfx1/2 (146 / 106).
pub fn defender_sfx_materials(combat: &crate::data::Combat, def: &crate::actor::Actor) -> [i64; 2] {
    let (r, a, b) = match def.side {
        crate::actor::Side::Enemy => (combat.npc(def), "materialSfx1", "materialSfx2"),
        crate::actor::Side::Player => (combat.param("EquipParamProtector", 100000), "defenseMaterialSfx1", "defenseMaterialSfx2"),
    };
    [r[a].as_i64().unwrap_or(-1), r[b].as_i64().unwrap_or(-1)]
}

/// A small xorshift for the stains' random size and roll (gore.rs).
#[derive(Resource)]
pub(crate) struct VfxRng(u32);

impl VfxRng {
    pub fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 % 100_000) as f32 / 100_000.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VfxRng(0x1234_5679)).add_systems(Update, spawn_clashes);
    }
}

/// Each clash's game effects, +Z toward the attacker, or HitEffectSfxAngleParam's world turn
/// (`hit_sfx_turned`); ids the effect library does not hold are left out.
fn spawn_clashes(mut commands: Commands, q: Query<(Entity, &Clash, &Transform)>, mut lib: Option<ResMut<crate::fxr::FxrLib>>) {
    for (e, c, tf) in &q {
        commands.entity(e).despawn();
        let pos = tf.translation;
        let look = Transform::from_translation(pos).looking_to(-c.normal, Vec3::Y);
        for (i, id) in c.sfx.iter().enumerate() {
            if !lib.as_mut().is_some_and(|l| l.has(*id)) {
                continue;
            }
            let at = c.turns.get(i).copied().flatten().map_or(look, |q| Transform::from_translation(pos).with_rotation(q));
            commands.spawn(crate::fxr::FxEffect::new(*id, None, false, at));
        }
    }
}
