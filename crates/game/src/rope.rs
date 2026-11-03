//! Grapple equipment on TAE 715 dummy 144; FwLo identified its 4 m -Y bind chain.
use super::*;

#[derive(Component)]
pub(super) struct Rope;

#[derive(Resource)]
pub(super) struct RopeLibrary(AnimLibrary);
#[derive(Resource)]
pub(super) struct RopeCache(pub PathBuf);

pub(super) fn spawn(commands: &mut Commands, assets: &AssetServer, cache: &Path, prefix: &str) {
    if !std::env::args().any(|a| a == "skinned-rope") {
        return;
    }
    let Ok(mut lib) = AnimLibrary::load(cache, "wp_a_9500") else {
        return;
    };
    let Ok(clip) = lib.clip("a999_000000") else {
        return;
    };
    let mut animator = Animator::new(clip, false);
    animator.speed = 0.0;
    animator.apply_root_motion = false;
    commands.spawn((
        Rope,
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("{prefix}wp_a_9500.glb"))),
        ),
        Rig::new(Arc::new(lib.skeleton.clone())),
        animator,
        Transform::IDENTITY,
        Visibility::Hidden,
    ));
    commands.insert_resource(RopeLibrary(lib));
}

pub(super) fn pose(
    sim: NonSend<Sim>,
    fixed: Res<Time<Fixed>>,
    mut lib: Option<ResMut<RopeLibrary>>,
    mut ropes: Query<(&mut Visibility, &mut Animator), With<Rope>>,
) {
    let wire = &sim.duel.player.wire;
    let endpoints = wire.rope_dummy.zip(wire.tip).and_then(|(dummy, tip)| {
        sim.duel
            .dummy_position(WOLF, dummy)
            .map(|p| (Vec3::from_array(p), Vec3::from_array(tip.to_array())))
    });
    for (mut vis, mut anim) in &mut ropes {
        *vis = Visibility::Hidden;
        let Some((origin, tip)) = endpoints else {
            continue;
        };
        let delta = tip - origin;
        if delta.length_squared() < 0.01 {
            continue;
        }
        *vis = Visibility::Inherited;
        let Some(lib) = lib.as_mut() else { continue };
        let report = &sim.last.player;
        if let Some(name) = &report.animation
            && anim.clip_name() != name
            && let Ok(clip) = lib.0.clip(name)
        {
            anim.play_named(name, clip, false, 0.0);
        }
        anim.time = drawn_clip_time(report, fixed.overstep_fraction());
    }
}

pub(super) fn draw(
    sim: NonSend<Sim>,
    mut ropes: Query<(&Rig, &mut Transform, &mut Visibility), With<Rope>>,
    mut gizmos: Gizmos,
) {
    let wire = &sim.duel.player.wire;
    let Some((dummy, tip)) = wire.rope_dummy.zip(wire.tip) else {
        return;
    };
    let Some(origin) = sim.duel.dummy_position(WOLF, dummy) else {
        return;
    };
    let origin = Vec3::from_array(origin);
    let tip = Vec3::from_array(tip.to_array());
    let delta = tip - origin;
    if delta.length_squared() < 0.01 {
        return;
    }
    // The native model's attachment/chain deformation is not traced yet.
    // Keep its export and opt-in preview, but use the endpoint-correct line in play.
    if !std::env::args().any(|a| a == "skinned-rope") {
        for (_, _, mut vis) in &mut ropes {
            *vis = Visibility::Hidden;
        }
        gizmos.line(origin, tip, Color::srgb(0.75, 0.70, 0.55));
        return;
    }
    let mut placed = false;
    for (rig, mut tf, mut vis) in &mut ropes {
        let first = rig.skeleton.bones.iter().position(|b| b.name == "Born00");
        let last = rig.skeleton.bones.iter().position(|b| b.name == "BornEnd");
        let endpoints = first.zip(last).and_then(|(a, b)| {
            Some((
                rig.model.get(a)?.w_axis.truncate(),
                rig.model.get(b)?.w_axis.truncate(),
            ))
        });
        let Some((start, end)) = endpoints else {
            continue;
        };
        // Chain animation bends and rotates the local tip; anchor its sampled endpoints.
        let local = end - start;
        if local.length_squared() < 0.01 {
            *vis = Visibility::Hidden;
            continue;
        }
        tf.rotation = Quat::from_rotation_arc(local.normalize(), delta.normalize());
        tf.scale = Vec3::splat(delta.length() / 4.0);
        tf.translation = origin - tf.rotation * (start * tf.scale);
        placed = true;
    }
    if !placed {
        gizmos.line(origin, tip, Color::srgb(0.65, 0.6, 0.45));
    }
}
