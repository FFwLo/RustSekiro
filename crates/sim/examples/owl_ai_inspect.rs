//! Read-only inspection of the installed Lua function constants for Father and his bird.
use sekiro_sim::lua_ai::lua50::{Proto, load_chunk};

fn inspect(proto: &Proto, path: &str) {
    println!("{path} line {}: {:?}", proto.line, proto.consts);
    for (i, child) in proto.protos.iter().enumerate() {
        inspect(child, &format!("{path}/{i}"));
    }
}

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read Lua");
        inspect(&load_chunk(&bytes).expect("load Lua"), &path);
    }
}
