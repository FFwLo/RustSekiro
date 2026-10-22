//! Change Form (the Sculptor's Idol, after the user's capture): Wolf's four looks, the memories
//! Original Memory: Wolf, Another's Memory: Shura / Ashina / Tengu. Each is a set of
//! EquipParamProtector rows (x00000 face, x01000 body, x02000 arms, x03000 legs; the leg row
//! carries the memory's name, text and icon, and names its picture menu_costume_<id>), drawn
//! with the parts of their equipModelId (category 3 fc_m_<id>, 2 bd_m, 1 am_m, 6 lg_m).
//!
//! Every form's parts are attached to Wolf (model.rs; the others' from extracted/form) and only
//! the chosen one (config player.form) is shown: a hidden cloth is not simulated (cloth.rs), so
//! the unseen forms cost little, and no dummy or weapon of Wolf's is rebuilt by a change.

use bevy::prelude::*;

/// One form: its leg row (name, text, picture), the leg row's iconIdM and its parts (face,
/// body, arms, legs).
pub(crate) struct Form {
    pub(crate) protector: i64,
    pub(crate) icon: i64,
    pub(crate) parts: [&'static str; 4],
}

pub(crate) const FORMS: [Form; 4] = [
    Form { protector: 103000, icon: 750, parts: ["fc_m_0200", "bd_m_9040", "am_m_9000", "lg_m_9000"] },
    Form { protector: 303000, icon: 756, parts: ["fc_m_0240", "bd_m_9110", "am_m_9110", "lg_m_9110"] },
    Form { protector: 403000, icon: 754, parts: ["fc_m_0220", "bd_m_9120", "am_m_9120", "lg_m_9120"] },
    Form { protector: 503000, icon: 752, parts: ["fc_m_0230", "bd_m_9130", "am_m_9130", "lg_m_9130"] },
];

/// The form a player model file belongs to (by its part name), None for the body dummies and
/// the weapons, which every form shares.
pub(crate) fn form_of(file_name: &str) -> Option<usize> {
    FORMS.iter().position(|f| f.parts.iter().any(|p| file_name.contains(&format!("_{p}"))))
}

/// The chosen form, clamped to the table.
pub(crate) fn current(config: &crate::config::GameConfig) -> usize {
    (config.player.form as usize).min(FORMS.len() - 1)
}

/// A mesh of one form's parts, and the visibility it was made with.
#[derive(Component)]
pub(crate) struct FormMesh {
    pub(crate) form: usize,
    pub(crate) own: Visibility,
}

/// Hides the other forms' meshes, every frame (the free cloth meshes' owner hiding, model.rs
/// follow_owner, restores what they had when Wolf was hidden at the start, and the draw masks
/// keep them hidden themselves), and shows the chosen form's when the choice changes or they
/// are new (leaving the draw masks to their groups).
pub(crate) fn show_form(config: Res<crate::config::GameConfig>, mut last: Local<Option<usize>>, mut meshes: Query<(Ref<FormMesh>, &mut Visibility, Has<crate::model::MeshGroup>)>) {
    let form = current(&config);
    let changed = *last != Some(form);
    *last = Some(form);
    for (m, mut vis, grouped) in &mut meshes {
        let want = if m.form != form {
            Visibility::Hidden
        } else if grouped || !(changed || m.is_added()) {
            continue;
        } else {
            m.own
        };
        if *vis != want {
            *vis = want;
        }
    }
}

/// The forms' texts (EquipParamProtector names and descriptions).
#[derive(Resource, Default)]
pub(crate) struct FormTexts {
    pub(crate) name: std::collections::HashMap<u32, String>,
    pub(crate) info: std::collections::HashMap<u32, String>,
}

impl FormTexts {
    pub(crate) fn load() -> Self {
        let dir = crate::paths::extracted().join("msg/engus/item.msgbnd.d");
        let read = |f: &str| std::fs::read(dir.join(f)).ok().map(|d| crate::data::read_fmg(&d)).unwrap_or_default();
        FormTexts { name: read("防具名.fmg"), info: read("防具説明.fmg") }
    }
}

/// The form's picture (tools/costume_art.py).
pub(crate) fn picture(f: &Form) -> Option<String> {
    let p = format!("menu_costume/{}.png", f.protector);
    crate::paths::extracted().join(&p).exists().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table against the params: each form's four rows name these parts.
    #[test]
    fn the_forms_are_the_protector_sets() {
        let path = crate::paths::extracted().join("json/params/EquipParamProtector.json");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("EquipParamProtector")).unwrap();
        let rows: std::collections::HashMap<i64, serde_json::Value> = v["rows"].as_array().unwrap().iter().map(|r| (r["id"].as_i64().unwrap(), r.clone())).collect();
        for f in &FORMS {
            let base = f.protector - 3000;
            let part = |off: i64, prefix: &str| {
                let r = &rows[&(base + off)];
                format!("{prefix}_m_{:04}", r["equipModelId"].as_i64().unwrap())
            };
            assert_eq!([part(0, "fc"), part(1000, "bd"), part(2000, "am"), part(3000, "lg")], f.parts.map(str::to_string), "form {}", f.protector);
            assert_eq!(rows[&f.protector]["iconIdM"].as_i64(), Some(f.icon), "form {} icon", f.protector);
        }
        assert_eq!(form_of("model_c0000_bd_m_9120.bin"), Some(2));
        assert_eq!(form_of("model_c0000_wp_a_0300.bin"), None);
        assert_eq!(form_of("model_c0000.bin"), None);
    }
}
