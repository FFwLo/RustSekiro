//! `hks-dis <file.hks> [out.txt]`: writes a disassembly listing (for `re/`, never for git).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("usage: hks-dis <file.hks> [out.txt]");
        std::process::exit(2);
    };
    let data = std::fs::read(input).expect("read input");
    let file = sekiro_formats::hks::parse(&data).expect("parse");
    let listing = sekiro_formats::hks::disassemble(&file.main);
    match args.get(2) {
        Some(out) => std::fs::write(out, listing).expect("write output"),
        None => print!("{listing}"),
    }
}
