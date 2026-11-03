//! Material families, static-switch samplers and constants for layered map surfaces.
use crate::mtd;

pub fn family(shader: &str) -> u8 {
    match shader
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(shader)
        .to_ascii_lowercase()
        .as_str()
    {
        "m[multiple].spx" => 1,
        "m[multipleground].spx" => 2,
        "m[landscape]_mountain.spx" => 3,
        _ => 0,
    }
}

pub fn variant(d: &mtd::Mtd) -> Option<usize> {
    match d.param("GXFT_SATStaticSwitch") {
        Some(mtd::ParamValue::Ints(v)) => v
            .first()
            .and_then(|&n| usize::try_from(n).ok())
            .filter(|&n| n < 34),
        _ => None,
    }
}

pub fn slots(kind: u8, variant: usize) -> &'static [u8] {
    match kind {
        1 => MULTIPLE_SLOTS.get(variant),
        2 => GROUND_SLOTS.get(variant),
        _ => None,
    }
    .copied()
    .unwrap_or(&[])
}

fn mirror(v: [f32; 3]) -> [f32; 3] {
    [-v[0], v[1], v[2]]
}

const MULTIPLE_SLOTS: [&[u8]; 34] = [
    &[8, 0, 3],
    &[8, 0, 3, 6, 7],
    &[8, 0, 16, 3, 6, 7],
    &[8, 0, 3],
    &[8, 0, 3],
    &[8, 0, 12, 3, 9],
    &[8, 0, 12, 3, 6, 7, 9],
    &[8, 0, 12, 16, 3, 6, 7, 9],
    &[8, 0, 3],
    &[8, 0, 3],
    &[8, 0, 10, 12, 3, 9],
    &[8, 0, 10, 12, 3, 6, 7, 9],
    &[8, 0, 10, 12, 16, 3, 6, 7, 9],
    &[8, 0, 3],
    &[8, 0, 3],
    &[8, 0, 3, 4, 5, 9],
    &[8, 0, 3, 4, 5, 6, 7, 9],
    &[8, 0, 16, 3, 4, 5, 6, 7, 9],
    &[8, 0, 17, 3, 4, 5, 6, 7, 9],
    &[8, 0, 3],
    &[8, 0, 12, 3, 4, 5, 9],
    &[8, 0, 12, 3, 4, 5, 6, 7, 9],
    &[8, 0, 12, 16, 3, 4, 5, 6, 7, 9],
    &[8, 0, 3],
    &[8, 0, 3],
    &[8, 0, 10, 12, 3, 4, 5, 9],
    &[8, 0, 10, 12, 3, 4, 5, 6, 7, 9],
    &[8, 0, 10, 12, 16, 3, 4, 5, 6, 7, 9],
    &[8, 0, 3],
    &[8, 0, 3],
    &[8, 0, 3],
    &[8, 0, 3],
    &[8, 0, 3, 4, 5, 9],
    &[8, 0, 3, 4, 5, 9],
];
const GROUND_SLOTS: [&[u8]; 34] = [
    &[12, 3, 8],
    &[0, 12, 3, 8],
    &[0, 12, 2, 3, 8],
    &[11, 12, 3, 7, 8, 9],
    &[0, 11, 12, 3, 7, 8, 9],
    &[3, 8],
    &[11, 12, 3, 7, 8, 9],
    &[0, 11, 12, 3, 7, 8, 9],
    &[3, 8],
    &[3, 8],
    &[3, 8],
    &[3, 8],
    &[12, 3, 4, 5, 8, 9],
    &[0, 12, 3, 4, 5, 8, 9],
    &[0, 12, 2, 3, 4, 5, 8, 9],
    &[11, 12, 3, 4, 5, 7, 8, 9],
    &[0, 11, 12, 3, 4, 5, 7, 8, 9],
    &[3, 8],
    &[11, 12, 3, 4, 5, 7, 8, 9],
    &[0, 11, 12, 3, 4, 5, 7, 8, 9],
    &[3, 8],
    &[3, 8],
    &[3, 8],
    &[3, 8],
    &[1, 12, 3, 4, 5, 6, 8, 9],
    &[0, 1, 12, 3, 4, 5, 6, 8, 9],
    &[0, 1, 12, 2, 3, 4, 5, 6, 8, 9],
    &[1, 11, 12, 3, 4, 5, 6, 7, 8, 9],
    &[3, 8],
    &[3, 8],
    &[1, 11, 12, 3, 4, 5, 6, 7, 8, 9],
    &[3, 8],
    &[3, 8],
    &[3, 8],
];

pub fn game_params(d: &mtd::Mtd, kind: u8) -> [[f32; 4]; 16] {
    let prefix = if kind == 3 {
        "M_LandScape__Mountain_snp_0_"
    } else if kind == 2 {
        "M_MultipleGround__snp_0_"
    } else {
        "M_Multiple__snp_0_"
    };
    let get = |n: &str| match d.param(&format!("{prefix}{n}")) {
        Some(mtd::ParamValue::Floats(v)) => Some(v.clone()),
        _ => None,
    };
    let f = |n: &str| get(n).and_then(|v| v.first().copied()).unwrap_or(0.0);
    let v3 = |n: &str| {
        get(n)
            .filter(|v| v.len() >= 3)
            .map_or([0.0; 3], |v| [v[0], v[1], v[2]])
    };
    let v2 = |n: &str| {
        get(n)
            .filter(|v| v.len() >= 2)
            .map_or([0.0; 2], |v| [v[0], v[1]])
    };
    let col = |n: &str| -> [f32; 4] {
        let c = get(n)
            .filter(|v| v.len() >= 3)
            .map_or([1.0; 3], |v| [v[0], v[1], v[2]]);
        let k = get(&format!("{n}-Intensity"))
            .and_then(|v| v.first().copied())
            .unwrap_or(1.0);
        [c[0] * k, c[1] * k, c[2] * k, 0.0]
    };
    let w = |a: [f32; 3], x: f32| [a[0], a[1], a[2], x];
    let one = [1.0, 1.0, 1.0, 0.0];
    let p: [[f32; 4]; 16] = if kind == 3 {
        // Native Mountain Gbuf/Fwd: two albedos and three mask lookups.
        let mut p = [[0.; 4]; 16];
        p[0] = col("color_0");
        p[1] = col("color_1");
        p[2] = [f("float_0"), f("float_2"), f("float_3"), f("float_6")];
        for (i, name) in ["float2_0", "float2_1", "float2_2"].iter().enumerate() {
            let v = v2(name);
            p[3 + i] = [v[0], v[1], 0., 0.];
        }
        for (i, name) in ["float3_1", "float3_2", "float3_3"].iter().enumerate() {
            p[6 + i] = w(v3(name), 0.);
        }
        p
    } else if kind == 2 {
        let snow = mirror(v3("float3_5"));
        let wet = v2("float2_1");
        [
            one,
            one,
            one,
            one,
            col("color_1"),
            col("color_0"),
            [f("float_4"), f("float_5"), f("float_23"), f("float_11")],
            [f("float_10"), f("float_6"), f("float_7"), f("float_22")],
            [f("float_2"), f("float_3"), 0.0, 0.0],
            w(v3("float3_0"), f("float_31")),
            w(v3("float3_2"), f("float_8")),
            [0.0; 4],
            [0.0; 4],
            w(v3("float3_3"), 0.0),
            w(snow, 0.0),
            [wet[0], wet[1], 0.0, 0.0],
        ]
    } else {
        let snow = mirror(v3("float3_7"));
        let (l, wet) = (v2("float2_0"), v2("float2_1"));
        [
            col("color_1"),
            col("color_7"),
            col("color_2"),
            col("color_5"),
            col("color_6"),
            col("color_8"),
            [f("float_4"), f("float_5"), f("float_1"), f("float_0")],
            [f("float_29"), f("float_26"), f("float_30"), f("float_32")],
            [f("float_2"), f("float_19"), l[0], l[1]],
            w(v3("float3_0"), 0.0),
            w(v3("float3_2"), 0.0),
            w(v3("float3_1"), 0.0),
            w(v3("float3_3"), 0.0),
            w(v3("float3_5"), 0.0),
            w(snow, 0.0),
            [wet[0], wet[1], 0.0, 0.0],
        ]
    };
    p
}
