//! `sorb_dummies <file.flver> <reference id>...`: throw (sorb) dummies as frames in the
//! character's local space: where a character snapped to each would stand and face.
//! For example `cache/raw/chr/c5060.chrbnd.d/c5060.flver 230 235`.
use sekiro_sim::deathblow::DummyFrame;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let model = sekiro_formats::flver::parse(&std::fs::read(&args[1]).expect("flver")).unwrap();
    for id in args[2..].iter().filter_map(|s| s.parse::<i16>().ok()) {
        match DummyFrame::from_flver(&model, id) {
            Some(f) => println!(
                "{id}: position [{:.3}, {:.3}, {:.3}] yaw {:.1} deg",
                f.position[0],
                f.position[1],
                f.position[2],
                f.yaw.to_degrees()
            ),
            None => println!("{id}: missing"),
        }
    }
}
