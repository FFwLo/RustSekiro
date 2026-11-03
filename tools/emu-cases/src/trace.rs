//! `trace`: run one game function on the snapshot and list every function it enters (starts from
//! DSekiro's functions.json), with entry counts and sizes, to find what the game runs that Sv1 has
//! not ported.

use std::path::Path;

use serde_json::json;

use crate::Emu;

/// Runs `va` with integer `args` and float `xmm` args, up to 500 M instructions, and writes
/// `trace_<name>.json`: {function, stopped, imports, entered: [{va, count, size}], edges: [[return
/// address, function]]}.
pub fn trace(emu: &mut Emu, va: u64, args: &[u64], xmm: &[f32], out: &Path, name: &str) {
    assert!(!emu.sizes.is_empty(), "trace needs functions.json (DSEKIRO_FUNCTIONS)");
    emu.st.starts = emu.sizes.keys().copied().collect();
    emu.st.entered.clear();
    emu.st.edges.clear();
    emu.limit = 500_000_000;
    let t = std::time::Instant::now();
    let r = emu.call(va, args, xmm);
    let entered = std::mem::take(&mut emu.st.entered);
    let edges: Vec<_> = std::mem::take(&mut emu.st.edges).into_iter().map(|(r, f)| [format!("{r:#x}"), format!("{f:#x}")]).collect();
    let mut imports = r.calls.clone();
    imports.sort();
    imports.dedup();
    let list: Vec<_> = entered.iter().map(|(&a, &n)| json!({"va": format!("{a:#x}"), "count": n, "size": emu.sizes.get(&a).copied().unwrap_or(0)})).collect();
    println!(
        "trace {name} ({va:#x}): {} functions entered, {} entries, {} imports, stopped {:?}, {:.1} s",
        entered.len(),
        entered.values().sum::<u64>(),
        imports.len(),
        r.stopped.map(|(a, e)| format!("{a:#x} error {e}")),
        t.elapsed().as_secs_f32()
    );
    let v = json!({"function": format!("{va:#x}"), "stopped": r.stopped.map(|(a, e)| format!("{a:#x} error {e}")), "imports": imports, "entered": list, "edges": edges});
    std::fs::write(out.join(format!("trace_{name}.json")), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}
