//! `tpf_list <file.tpf> [prefix]`: lists the textures of a TPF with their DDS format and size.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: tpf_list <file.tpf> [prefix]")?;
    let prefix = args.next().unwrap_or_default();
    let data = std::fs::read(path)?;
    for t in sekiro_formats::tpf::parse(&data)? {
        if !t.name.starts_with(&prefix) {
            continue;
        }
        match t.header() {
            Ok(h) => println!(
                "{} {:?} {}x{} mips {}",
                t.name, h.format, h.width, h.height, h.mip_count
            ),
            Err(e) => println!("{} unreadable: {e}", t.name),
        }
    }
    Ok(())
}
