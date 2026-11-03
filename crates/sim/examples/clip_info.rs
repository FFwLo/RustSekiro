//! `clip_info <chr> <anim>...`: resolved length, alias and TAE presence of animations.
use sekiro_sim::ClipDurations;
fn main() {
    let mut args = std::env::args().skip(1);
    let chr = args.next().unwrap();
    let mut lib = sekiro_sim::clips::ClipLibrary::new(format!("cache/anim/{chr}"));
    for a in args {
        println!(
            "{a}: alias {} duration {:?} decoded {}",
            lib.alias(&a),
            lib.duration(&a),
            lib.clip(&a).is_some()
        );
    }
}
