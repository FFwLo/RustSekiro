//! `bxf_list <file.xxxbhd> [dump_dir]`: lists (and optionally extracts) a BXF4 pair, using the
//! game's Oodle DLL (read only, from `SEKIRO_DIR` or the default Steam folder) for KRAK DCX.
//! Run from the repository root; extract only under `re/`, which is not tracked.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game = std::env::var("SEKIRO_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro".into());
    let oodle = sekiro_formats::dcx::Oodle::load(game.as_ref()).ok();
    let bhd = std::fs::read(&args[1]).unwrap();
    let bdt = std::fs::read(args[1].replace("bhd", "bdt")).unwrap();
    let files = sekiro_formats::bxf4::parse(&bhd, &bdt).unwrap();
    for f in &files {
        let data = f.contents(oodle).unwrap();
        let magic = String::from_utf8_lossy(&data[..data.len().min(8)]).into_owned();
        println!("{} {} bytes, magic {magic:?}", f.file_name(), data.len());
        if let Some(dir) = args.get(2) {
            std::fs::create_dir_all(dir).unwrap();
            let name = f
                .file_name()
                .rsplit(['\\', '/'])
                .next()
                .unwrap()
                .to_string();
            std::fs::write(std::path::Path::new(dir).join(name), &data).unwrap();
        }
    }
}
