//! `param_rows <Table> [id...]`: prints rows (all non-padding fields). Run from the repo root.
use sekiro_formats::param::ParamSet;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let set = ParamSet::load(
        "cache/raw/param/gameparam/gameparam.parambnd.d".as_ref(),
        "cache/refs/paramdex/Defs".as_ref(),
    )
    .unwrap();
    let table = set.table(&args[1]).unwrap();
    let ids: Vec<i32> = args[2..].iter().filter_map(|s| s.parse().ok()).collect();
    for row in table.rows() {
        if !ids.is_empty() && !ids.contains(&row.id()) {
            continue;
        }
        let mut s = format!("{} {:?}:", row.id(), row.name().unwrap_or(""));
        for f in &table.def().fields {
            if f.name.starts_with("pad")
                || f.name.starts_with("reserve")
                || f.name.starts_with("dummy")
            {
                continue;
            }
            s += &format!(" {}={:?}", f.name, row.value(f));
        }
        println!("{s}");
    }
}
