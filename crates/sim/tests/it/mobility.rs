use sekiro_sim::InputFrame;
use sekiro_sim::body::{FlatGround, Ground};
use sekiro_sim::duel::{Duel, RESPAWN_DELAY};
use sekiro_sim::tae::TaeDb;
use std::path::PathBuf;

const DT: f32 = 1.0 / 60.0;

fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}

fn duel() -> Option<Duel> {
    let cache = cache();
    if !cache.join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        return None;
    }
    let mut duel = Duel::load(&cache, 4.0).unwrap();
    duel.enemy_active = false;
    Some(duel)
}

#[test]
fn velocity_params_read_fall_type() {
    if !cache()
        .join("params/ChrPhysicsVelocityChangeParam.json")
        .exists()
    {
        return;
    }
    let db = TaeDb::load_from_cache(&cache(), "c0000");
    assert_eq!(db.velocity_changes[&3810].fall_type, 1);
    for id in 1100..=1102 {
        assert_eq!(db.velocity_changes[&id].fall_type, 2);
    }
}

#[test]
fn fall_env_units_are_centimetres() {
    let Some(mut d) = duel() else {
        return;
    };
    d.player.body.last_fall_height = 16.8;
    d.player.body.grounded = false;
    d.player.body.position[1] = 20.0;
    d.player.body.velocity[1] = -21.0;
    d.step(&InputFrame::default(), DT);
    // c0000_transition.lua multiplies env 224 by 0.01 and compares 2000 raw.
    assert!((d.player.behavior.env.fall_height - 1680.0).abs() < 0.01);
    assert_eq!(d.player.behavior.env.vertical_velocity, -2100.0);
}

#[test]
fn a_sixteen_point_eight_metre_fall_reaches_the_short_landing_clip() {
    let Some(mut d) = duel() else {
        return;
    };
    d.player.body.position[1] = 16.8;
    d.player.body.fall_top = 16.8;
    d.player.body.grounded = false;
    let mut landed = false;
    for _ in 0..180 {
        let r = d.step(&InputFrame::default(), DT);
        if r.player.animation.as_deref() == Some("a000_200020") {
            assert!((d.player.behavior.env.fall_height - 1680.0).abs() < 0.1);
            assert!(!d.rules.fighters[0].dead());
            landed = true;
            break;
        }
    }
    assert!(landed);
}

struct MaterialGround(u32);
impl Ground for MaterialGround {
    fn height(&self, _: f32, _: f32) -> Option<f32> {
        Some(0.0)
    }
    fn material(&self, _: f32, _: f32, _: f32) -> Option<u32> {
        Some(self.0)
    }
}

#[test]
fn lethal_landing_routes_falling_death_and_respawns() {
    let Some(mut d) = duel() else {
        return;
    };
    d.player.ground = Box::new(MaterialGround(1));
    let spawn = d.player.body.position;
    d.step(&InputFrame::default(), DT);
    d.player.body.grounded = false;
    d.player.body.position[1] = 0.01;
    d.player.body.fall_top = 40.0;
    d.player.body.velocity[1] = -10.0;
    d.step(&InputFrame::default(), DT);
    assert!(d.rules.fighters[0].dead());
    assert_eq!(d.player.pending_damage.unwrap().damage_type, 6);
    let r = d.step(&InputFrame::default(), DT);
    assert!(
        r.player
            .behavior
            .state_path
            .iter()
            .any(|s| s.contains("FallDeathStart")),
        "{:?}",
        r.player.behavior.state_path
    );
    let mut respawned = false;
    for _ in 0..((RESPAWN_DELAY / DT) as usize + 2) {
        respawned |= d.step(&InputFrame::default(), DT).respawned;
    }
    assert!(respawned);
    assert!(!d.rules.fighters[0].dead());
    assert_eq!(d.player.body.position, spawn);
}

#[test]
fn landing_uses_material_threshold_and_disable_flag() {
    // A material id is its special setting times 100 plus the HitMtrlParam row the thresholds
    // are read by: the `l` binder's deep water is 722 (#189).
    for (material, height, dies) in [
        (1, 39.99, false),
        (22, 60.0, false),
        (22, 80.0, true),
        (40, 100.0, false),
        (101, 39.99, false),
        (101, 40.0, true),
        (722, 60.0, false),
        (722, 80.0, true),
        (440, 100.0, false),
    ] {
        let Some(mut d) = duel() else {
            return;
        };
        d.player.ground = Box::new(MaterialGround(material));
        d.player.body.grounded = false;
        d.player.body.position[1] = 0.01;
        d.player.body.fall_top = height;
        d.player.body.velocity[1] = -10.0;
        d.step(&InputFrame::default(), DT);
        assert_eq!(
            d.rules.fighters[0].dead(),
            dies,
            "material {material}, height {height}"
        );
    }
}

#[test]
fn a_fall_into_the_void_returns_wolf_to_where_he_last_stood() {
    struct Void;
    impl Ground for Void {
        fn height(&self, _: f32, _: f32) -> Option<f32> {
            None
        }
    }
    let Some(mut d) = duel() else {
        return;
    };
    d.step(&InputFrame::default(), DT);
    let safe = d.player.body.position;
    d.player.ground = Box::new(Void);
    d.fall_respawn_depth = Some(40.0);
    d.player.body.position[1] = -41.0;
    d.player.body.grounded = false;
    d.step(&InputFrame::default(), DT);
    assert!(!d.rules.fighters[0].dead());
    assert!(d.player.pending_damage.is_none());
    assert_eq!(d.player.body.position, safe);
    assert!(d.player.body.grounded);
    d.player.ground = Box::new(FlatGround::default());
}
