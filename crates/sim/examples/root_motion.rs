//! `root_motion <anim>...`: duration and root-motion totals (source space) of cached clips.
fn main() {
    for a in std::env::args().skip(1) {
        let Ok(b) = std::fs::read(format!("cache/anim/c0000/{a}.bin")) else {
            println!("{a}: not cached");
            continue;
        };
        let c = sekiro_formats::anim::AnimClip::from_bytes(&b).unwrap();
        match &c.root_motion {
            Some(rm) => {
                let mid = rm.sample(c.duration * 0.5);
                println!(
                    "{a}: {:.3}s total {:?} half {:?}",
                    c.duration,
                    rm.total(),
                    mid
                );
            }
            None => println!("{a}: {:.3}s no root motion", c.duration),
        }
    }
}
