//! Validate the authored billboard shader stage interfaces and bindings.
use bevy::shader::ShaderDefVal;

#[test]
fn billboard_shader_composes_and_validates() {
    let (mut cache, mut assets) = super::shader_support::cache();
    for (name, source) in [
        ("fxr", include_str!("../../src/fxr.wesl")),
        ("fxr_multi", include_str!("../../src/fxr_multi.wesl")),
    ] {
        let id = super::shader_support::register(&mut cache, &mut assets, name, source);
        if name == "fxr" || name == "fxr_multi" {
            for (index, (additive, prepass)) in
                [(false, false), (true, false), (false, true), (true, true)]
                    .into_iter()
                    .enumerate()
            {
                let mut definitions = vec![
                    ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3),
                    "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
                    "VERTEX_POSITIONS".into(),
                    "VERTEX_NORMALS".into(),
                    "VERTEX_UVS_A".into(),
                    "VERTEX_TANGENTS".into(),
                    "VERTEX_COLORS".into(),
                ];
                if additive {
                    definitions.push("BLEND_PREMULTIPLIED_ALPHA".into());
                }
                if prepass {
                    // Single-sample captures read Bevy's depth prepass for the soft fade.
                    definitions.push("DEPTH_PREPASS".into());
                }
                cache.get(index, id, &definitions).unwrap();
            }
        }
    }
}
