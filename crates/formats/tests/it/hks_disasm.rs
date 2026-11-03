//! Parses and disassembles every installed HKS script. Skips when `cache/` has not been extracted.

use sekiro_formats::hks::{self, Op};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn script_dir() -> PathBuf {
    sekiro_formats::cache::root().join("raw/action/script")
}

#[test]
fn disassemble_all_installed_scripts() {
    let dir = script_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipping: {} not extracted", dir.display());
        return;
    };
    let mut files = 0;
    let mut functions = 0;
    let mut instructions = 0;
    let mut ops: BTreeMap<Op, usize> = BTreeMap::new();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("hks") {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        assert!(hks::is_hks(&data), "{} is not HKS", path.display());
        let file = hks::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let listing = hks::disassemble(&file.main);
        assert!(!listing.contains("<bad opcode"), "{}", path.display());
        file.main.walk(&mut |p| {
            functions += 1;
            instructions += p.code.len();
            for ins in &p.code {
                *ops.entry(ins.op().unwrap()).or_default() += 1;
            }
            // DATA words are operands, never executed: a closure's upvalue bindings follow it,
            // a cached global load is followed by one cache slot and a table read by two.
            let mut expected_data = vec![false; p.code.len()];
            for (pc, ins) in p.code.iter().enumerate() {
                let op = ins.op().unwrap();
                let trailing = match op {
                    Op::Closure => p.protos[ins.bx() as usize].num_upvalues as usize,
                    Op::Data => 0,
                    _ => hks::trailing_data_words(op),
                };
                for slot in &mut expected_data[pc + 1..pc + 1 + trailing] {
                    *slot = true;
                }
            }
            for (pc, ins) in p.code.iter().enumerate() {
                assert_eq!(
                    ins.op() == Some(Op::Data),
                    expected_data[pc],
                    "{} {} pc {pc}",
                    path.display(),
                    p.name()
                );
            }
        });
        files += 1;
    }
    assert!(files > 0, "no .hks files in {}", dir.display());
    eprintln!("{files} files, {functions} functions, {instructions} instructions");
    for (op, n) in &ops {
        eprintln!("{:<16} {n}", op.name());
    }
}
