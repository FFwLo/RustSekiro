//! `weapon_frame_probe`: compares a bone's bind frame in the c0000 FLVER with the Havok
//! skeleton's reference pose (both in FromSoftware space), to check weapon attachment.
use sekiro_formats::anim::Skeleton;
use sekiro_formats::flver;

fn main() {
    let cache = &sekiro_sim::cache::root();
    let skel = Skeleton::from_bytes(&std::fs::read(cache.join("anim/c0000/skeleton.bin")).unwrap())
        .unwrap();
    let model = skel.model_space(&skel.reference_pose());
    let f = flver::parse(&std::fs::read(cache.join("raw/chr/c0000.chrbnd.d/c0000.flver")).unwrap())
        .unwrap();
    for name in ["R_Hand", "R_Weapon", "L_Weapon", "Sheath", "Pelvis"] {
        let Some(h) = skel.bone_index(name) else {
            println!("{name}: not in Havok skeleton");
            continue;
        };
        let i = f.nodes.iter().position(|n| n.name == name).unwrap();
        let mut w = f.nodes[i].local_matrix();
        let mut p = f.nodes[i].parent;
        while p >= 0 {
            w = f.nodes[p as usize].local_matrix() * w;
            p = f.nodes[p as usize].parent;
        }
        let hm = model[h];
        println!("{name}:");
        println!(
            "  flver  x {:?} y {:?} z {:?} t {:?}",
            w.x_axis.truncate(),
            w.y_axis.truncate(),
            w.z_axis.truncate(),
            w.w_axis.truncate()
        );
        println!(
            "  havok  x {:?} y {:?} z {:?} t {:?}",
            hm.x_axis.truncate(),
            hm.y_axis.truncate(),
            hm.z_axis.truncate(),
            hm.w_axis.truncate()
        );
    }
}
