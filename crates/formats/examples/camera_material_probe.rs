//! Read-only inspection of native map distance-fade materials.
use sekiro_formats::mtd;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let material = mtd::parse(&std::fs::read(&path)?)?;
        println!(
            "{path}: shader={} description={}",
            material.shader, material.description
        );
        for param in material
            .params
            .iter()
            .filter(|param| param.name.contains("Fade"))
        {
            println!("  {} = {:?}", param.name, param.value);
        }
    }
    Ok(())
}
