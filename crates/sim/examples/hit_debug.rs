//! Prints the enemy's attack windows and hitbox positions during a duel (debugging aid).
use sekiro_sim::InputFrame;
use sekiro_sim::duel::Duel;
use sekiro_sim::tae::TaeKind;
fn main() {
    let mut duel = Duel::load(&sekiro_sim::cache::root(), 4.0).unwrap();
    let mut keys: Vec<_> = duel.fighters[0].attacks.keys().copied().collect();
    keys.sort();
    println!(
        "player variation {} judges {:?}",
        duel.fighters[0].variation, keys
    );
    let mut keys: Vec<_> = duel.fighters[1].attacks.keys().copied().collect();
    keys.sort();
    println!(
        "enemy variation {} judges {:?}",
        duel.fighters[1].variation, keys
    );
    for i in 0..500 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
        for (who, c, f) in [
            ("enemy", &duel.enemy, &duel.fighters[1]),
            ("wolf", &duel.player, &duel.fighters[0]),
        ] {
            for e in &c.tae_frame().attacks {
                if let TaeKind::Attack { judge, .. } = e.kind {
                    let a = f.attacks.get(&judge);
                    let model = c.bone_source_matrices();
                    let w = c.source_to_world();
                    let pos: Vec<_> = a
                        .map(|a| {
                            a.shapes
                                .iter()
                                .map(|s| {
                                    (
                                        f.rig.positions(s.dummy_a, &model, w),
                                        f.rig.positions(s.dummy_b, &model, w),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    println!(
                        "{i} {who} judge {judge} known={} bones={} shapes={:?} body={:?} other={:?}",
                        a.is_some(),
                        model.len(),
                        pos,
                        c.body.position,
                        duel.player.body.position
                    );
                }
            }
        }
    }
}
