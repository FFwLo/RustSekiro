//! `hkb-dump <file.hkx> [type] [n] [member] [k]`: type histogram, describe the first n objects of
//! a type, or the first k elements of an array (or pointee) member of those objects.
use sekiro_formats::hkx::tagfile::{Kind, TagFile};
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read");
    let file = TagFile::parse(&data, None).expect("parse");
    let Some(ty) = args.get(2) else {
        let mut hist: BTreeMap<&str, usize> = BTreeMap::new();
        for o in file.objects() {
            *hist.entry(o.type_name()).or_default() += 1;
        }
        for (k, v) in hist {
            println!("{v:6} {k}");
        }
        return;
    };
    let n: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let k: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(2);
    for o in file.objects_of(ty).into_iter().take(n) {
        match args.get(4) {
            None => println!("{}", o.describe(0)),
            Some(member) => {
                let m = o.get(member).expect("member");
                match m.kind() {
                    Kind::Pointer => {
                        if let Some(p) = m.deref().unwrap() {
                            println!("{}", p.describe(0));
                        }
                    }
                    _ => {
                        for e in m.elements().expect("elements").into_iter().take(k) {
                            println!("{}", e.describe(0));
                        }
                    }
                }
            }
        }
    }
}
