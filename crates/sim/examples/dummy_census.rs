//! `dummy_census <dir>...`: counts FLVER dummy reference ids over every `.flver` found under the
//! given directories, with one example file per id. Run from the repository root.
use std::collections::BTreeMap;
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "flver") {
            out.push(p);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for d in std::env::args().skip(1) {
        walk(Path::new(&d), &mut files);
    }
    let mut counts: BTreeMap<i16, (usize, usize, String)> = BTreeMap::new();
    for f in &files {
        let Ok(model) = std::fs::read(f)
            .map_err(|_| ())
            .and_then(|b| sekiro_formats::flver::parse(&b).map_err(|_| ()))
        else {
            continue;
        };
        let mut seen = std::collections::HashSet::new();
        for d in &model.dummies {
            let e = counts.entry(d.reference_id).or_insert((
                0,
                0,
                f.file_name().unwrap().to_string_lossy().into_owned(),
            ));
            e.0 += 1;
            if seen.insert(d.reference_id) {
                e.1 += 1;
            }
        }
    }
    println!("{} files", files.len());
    for (id, (n, nf, example)) in counts {
        println!("{id:6} {n:6} dummies in {nf:4} files, e.g. {example}");
    }
}
