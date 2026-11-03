use sekiro_formats::{
    map_layers,
    mtd::{Mtd, Param, ParamValue},
};

#[test]
fn families_and_variants_select_distinct_sampler_roles() {
    for (shader, kind) in [
        ("N:\\shader\\M[Multiple].spx", 1),
        ("m[multipleground].spx", 2),
        ("M[Multiple]_Snow.spx", 0),
    ] {
        assert_eq!(map_layers::family(shader), kind);
    }
    assert_eq!(map_layers::slots(1, 1), &[8, 0, 3, 6, 7]);
    assert_eq!(map_layers::slots(2, 24), &[1, 12, 3, 4, 5, 6, 8, 9]);
    assert!(map_layers::slots(1, 34).is_empty());
    for (value, expected) in [(0, Some(0)), (33, Some(33)), (34, None), (-1, None)] {
        let d = Mtd {
            shader: String::new(),
            description: String::new(),
            textures: vec![],
            params: vec![Param {
                name: "GXFT_SATStaticSwitch".into(),
                kind: "int".into(),
                value: ParamValue::Ints(vec![value]),
            }],
        };
        assert_eq!(map_layers::variant(&d), expected);
    }
}

#[test]
fn authored_mask_selectors_intensity_and_snow_direction_survive_packing() {
    for (kind, prefix, selector, snow) in [
        (1, "M_Multiple", "float3_5", "float3_7"),
        (2, "M_MultipleGround", "float3_3", "float3_5"),
    ] {
        let mut d = Mtd {
            shader: String::new(),
            description: String::new(),
            textures: vec![],
            params: vec![],
        };
        for (name, v) in [
            ("float3_0", vec![0., 1., 0.]),
            (selector, vec![0., 0., 1.]),
            (snow, vec![-0.3, 0.8, -0.3]),
            ("color_1", vec![0.2, 0.4, 0.6]),
            ("color_1-Intensity", vec![2.]),
            ("float_11", vec![0.25]),
            ("float_12", vec![0.75]),
        ] {
            d.params.push(Param {
                name: format!("{prefix}__snp_0_{name}"),
                kind: "float".into(),
                value: ParamValue::Floats(v),
            });
        }
        let p = map_layers::game_params(&d, kind);
        if kind == 2 {
            // Ground flow amplitude reads float_11 (cbMtdParam byte 448), not float_12.
            assert_eq!(p[6][3], 0.25);
        }
        assert_eq!(&p[9][..3], &[0., 1., 0.]);
        assert_eq!(&p[13][..3], &[0., 0., 1.]);
        assert_eq!(&p[14][..3], &[0.3, 0.8, -0.3]);
        assert_eq!(p[if kind == 1 { 0 } else { 4 }], [0.4, 0.8, 1.2, 0.]);
        // Selecting G and B produces different heights from the same RGB mask.
        let mask = [0.1, 0.7, 0.3];
        let select = |s: [f32; 4]| (0..3).map(|i| mask[i] * s[i]).sum::<f32>();
        assert_eq!(select(p[9]), 0.7);
        assert_eq!(select(p[13]), 0.3);
    }
}
