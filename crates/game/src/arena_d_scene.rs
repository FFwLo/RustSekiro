//! Supporting cast and sanctuary props, driven by the reviewed map events.

use std::path::Path;
use std::sync::Arc;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::prelude::*;
use sekiro_sim::arena::ArenaSpec;
use serde_json::Value;

use super::Sim;
use crate::character::{AnimLibrary, Animator, Rig};
use crate::draw_mask::DrawMask;

#[derive(Component)]
struct StagePart {
    entity: i32,
    groups: [i32; 8],
    initially_enabled: bool,
}

#[derive(Component)]
struct Cast {
    library: AnimLibrary,
    requested: Option<(i32, bool)>,
    requested_warp: Option<i32>,
    requested_bone_warp: Option<(i32, i16)>,
}

#[derive(Resource)]
struct Staging {
    cache: std::path::PathBuf,
    area: u8,
    block: u8,
    rows: Vec<(Value, String)>,
    cast: Vec<(Value, AnimLibrary, [bool; 32])>,
}

pub(crate) fn install(
    app: &mut App,
    cache: &Path,
    spec: Option<&ArenaSpec>,
    objects: bool,
) -> Result<()> {
    let Some(spec) = spec.filter(|s| {
        matches!(
            s.name,
            "mibu-bridge" | "fountainhead-bridge" | "fountainhead-sanctuary"
        ) || sekiro_sim::arenas_a::owns(s)
    }) else {
        return Ok(());
    };
    if matches!(
        spec.name,
        "mibu-bridge" | "fountainhead-bridge" | "fountainhead-sanctuary"
    ) {
        let character = spec.enemy_spec().chr;
        anyhow::ensure!(
            cache.join(format!("models/{character}.glb")).is_file(),
            "missing extracted arena character cache/models/{character}.glb"
        );
    }
    let json: Value = serde_json::from_slice(&std::fs::read(
        cache.join(format!("maps/{}/layout.json", spec.map)),
    )?)?;
    let roster = sekiro_sim::registry::Registry::load(cache).map_err(anyhow::Error::msg)?;
    let mut staging = Staging {
        cache: cache.to_path_buf(),
        area: spec.map[1..3].parse()?,
        block: spec.map[4..6].parse()?,
        rows: Vec::new(),
        cast: Vec::new(),
    };
    if sekiro_sim::arenas_a::owns(spec) && !objects {
        let entities =
            sekiro_sim::arenas_a::object_entities(cache, spec).map_err(anyhow::Error::msg)?;
        for row in json["objects"].as_array().into_iter().flatten() {
            if !row["entity_id"]
                .as_i64()
                .is_some_and(|id| entities.contains(&(id as i32)))
            {
                continue;
            }
            let model = row["model"]
                .as_str()
                .context("arena object without model")?;
            let glb = json["models"][model]["glb"]
                .as_str()
                .context("arena object without GLB")?;
            anyhow::ensure!(
                cache.join(format!("maps/{}/{glb}", spec.map)).is_file(),
                "missing extracted arena object maps/{}/{glb}",
                spec.map
            );
            staging
                .rows
                .push((row.clone(), format!("maps/{}/{glb}", spec.map)));
        }
    }
    // Only the sanctuary's authored set pieces are force-loaded; unrelated map objects
    // retain the normal viewer option. No render mesh is substituted for hit collision.
    if spec.name == "fountainhead-sanctuary" && !objects {
        for row in json["objects"].as_array().into_iter().flatten() {
            let model = row["model"].as_str().unwrap_or("");
            if !model.starts_with("o258") && !model.starts_with("o259") {
                continue;
            }
            let Some(glb) = json["models"][model]["glb"].as_str() else {
                // o259900 is an authored dummy-only FLVER, so extraction emits no mesh GLB.
                let path = cache.join(format!("raw/obj/{model}.objbnd.d/{model}.flver"));
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("missing extracted sanctuary object {model}"))?;
                let data = sekiro_formats::flver::parse(&bytes)?;
                anyhow::ensure!(
                    data.meshes.is_empty(),
                    "missing extracted sanctuary object {model}; run sekiro-extract map {}",
                    spec.map
                );
                continue;
            };
            anyhow::ensure!(
                cache.join(format!("maps/{}/{glb}", spec.map)).is_file(),
                "missing extracted sanctuary object maps/{}/{glb}",
                spec.map
            );
            staging
                .rows
                .push((row.clone(), format!("maps/{}/{glb}", spec.map)));
        }
    }
    let boss = sekiro_sim::arena::boss_events(&spec.enemy_spec())
        .unwrap()
        .boss_entity;
    for row in json["enemies"].as_array().into_iter().flatten() {
        let Some(entity) = row["entity_id"].as_i64().map(|e| e as i32) else {
            continue;
        };
        if !sekiro_sim::arena::depths_palace::simultaneous(boss, entity)
            && !sekiro_sim::arenas_a::simultaneous(boss, entity)
        {
            continue;
        }
        let (enemy, _) = roster
            .entity(spec.map, entity)
            .map_err(anyhow::Error::msg)?;
        let donor = roster.anim_chr(&enemy.chr, enemy.npc_param);
        let library = AnimLibrary::load(cache, &donor)?;
        let masks = roster
            .draw_mask(cache, &enemy)
            .with_context(|| format!("missing NpcParam draw mask for supporting actor {entity}"))?;
        anyhow::ensure!(
            cache.join(format!("models/{}.glb", enemy.chr)).is_file(),
            "missing extracted supporting character models/{}.glb",
            enemy.chr
        );
        staging.cast.push((row.clone(), library, masks));
    }
    app.insert_resource(staging)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            update
                .before(crate::character::animate)
                .before(crate::draw_mask::apply_draw_masks),
        );
    Ok(())
}

fn pose(row: &Value) -> Transform {
    let values = |key: &str, defaults: &[f32]| {
        defaults
            .iter()
            .enumerate()
            .map(|(i, d)| row[key][i].as_f64().map_or(*d, |v| v as f32))
            .collect::<Vec<_>>()
    };
    let p = values("translation", &[0.; 3]);
    let q = values("rotation", &[0., 0., 0., 1.]);
    let s = values("scale", &[1.; 3]);
    Transform {
        translation: Vec3::new(p[0], p[1], p[2]),
        rotation: Quat::from_xyzw(q[0], q[1], q[2], q[3]),
        scale: Vec3::new(s[0], s[1], s[2]),
    }
}

fn spawn(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut staging: ResMut<Staging>,
    identities: Res<crate::arena_b::Parts>,
) {
    for (row, glb) in staging.rows.drain(..) {
        let entity = row["entity_id"].as_i64().unwrap_or(-1) as i32;
        // Event 0 starts the Dragon cloud disabled; attached trees are disabled by their
        // events before appearing. The arena floor/set meshes remain visible.
        let on = entity != 2_501_800;
        commands.spawn((
            StagePart {
                entity,
                groups: row["name"]
                    .as_str()
                    .and_then(|n| identities.0.get(n))
                    .map_or([0; 8], |p| p.groups),
                initially_enabled: on,
            },
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(glb))),
            pose(&row),
            if on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ));
    }
    for (row, mut library, masks) in staging.cast.drain(..) {
        let entity = row["entity_id"].as_i64().unwrap_or(-1) as i32;
        let model = row["model"].as_str().unwrap_or("");
        let clip = ["a000_000000", "a000_400000", "a000_010000"]
            .iter()
            .find_map(|name| library.clip(name).ok());
        let Some(clip) = clip else {
            error!("supporting actor {entity}: no extracted standing clip");
            continue;
        };
        let mut animator = Animator::new(clip, true);
        animator.apply_root_motion = false;
        let on = !(2_500_851..=2_500_854).contains(&entity) && entity != 2_500_840;
        let on = sekiro_sim::arenas_a::initially_enabled(entity).unwrap_or(on);
        commands.spawn((
            StagePart {
                entity,
                groups: row["name"]
                    .as_str()
                    .and_then(|n| identities.0.get(n))
                    .map_or([0; 8], |p| p.groups),
                initially_enabled: on,
            },
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{model}.glb"))),
            ),
            pose(&row),
            Rig::new(Arc::new(library.skeleton.clone())),
            animator,
            DrawMask::with_groups(masks),
            Cast {
                library,
                requested: None,
                requested_warp: None,
                requested_bone_warp: None,
            },
            if on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ));
    }
}

type StageParts<'w, 's> = Query<
    'w,
    's,
    (
        &'static StagePart,
        &'static mut Visibility,
        &'static mut Transform,
        Option<(
            &'static mut Cast,
            &'static mut Animator,
            &'static mut DrawMask,
        )>,
    ),
>;

fn update(
    staging: Res<Staging>,
    sim: NonSend<Sim>,
    mut parts: StageParts<'_, '_>,
    mut sky: Option<ResMut<crate::map::sky::SkyOverride>>,
) {
    let Some(events) = sim.duel.events.as_ref() else {
        return;
    };
    let scene = &events.scripts.cast_scene;
    if let Some(mut sky) = sky.take() {
        sky.0 = scene
            .enabled
            .get(&8_907_090)
            .copied()
            .unwrap_or(false)
            .then_some("m900090")
            .or_else(|| sekiro_sim::arenas_a::sky_override(events.boss_entity));
    }
    for (part, mut visibility, mut transform, actor) in &mut parts {
        let on = scene
            .enabled
            .get(&part.entity)
            .or_else(|| part.groups.iter().find_map(|g| scene.enabled.get(g)))
            .copied()
            .unwrap_or(part.initially_enabled);
        let backread = scene
            .backread_enabled
            .get(&part.entity)
            .or_else(|| {
                part.groups
                    .iter()
                    .find_map(|g| scene.backread_enabled.get(g))
            })
            .copied()
            .unwrap_or(true);
        // A support model becomes the primary opponent at a sequential phase transition.
        let primary = part.entity == events.boss_entity;
        // The sanctuary keeps c5200 frozen during the opening, then enables his branch.
        // This visibility gate follows those event states; native draw-flag hosting remains open.
        let dragon_waiting =
            part.entity == 2_500_800 && scene.animation_enabled.get(&part.entity) == Some(&false);
        *visibility = if on && backread && !primary && !dragon_waiting {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(&(owner, dummy)) = scene.attachments.get(&part.entity)
            && let Some([point, forward, up]) = sim.duel.sanctuary_dummy(owner, dummy)
        {
            *transform =
                crate::fxr::frame_transform(point.to_array(), forward.to_array(), up.to_array());
        }
        let Some((mut cast, mut animator, mut mask)) = actor else {
            continue;
        };
        if let Some(bird) = sim
            .duel
            .father_bird
            .as_ref()
            .filter(|b| b.entity == part.entity)
        {
            let report = &bird.report.enemy;
            transform.translation = Vec3::from_array(bird.duel.enemy.body.position);
            transform.rotation = Quat::from_rotation_y(bird.duel.enemy.body.yaw);
            if let Some(name) = &report.animation {
                if (animator.clip_name() != name || report.clip_started)
                    && let Ok(clip) = cast.library.clip(name)
                {
                    animator.play_named(name, clip, false, report.blend_in);
                }
                animator.time = report.anim_time;
                animator.speed = 0.0;
                animator.apply_root_motion = false;
            }
            if report.fade >= 0.5 {
                *visibility = Visibility::Hidden;
            }
            continue;
        }
        if let Some(illusion) = sim
            .duel
            .butterfly_illusions
            .as_ref()
            .and_then(|e| e.actors.iter().find(|a| a.entity == part.entity))
        {
            let report = &illusion.report.enemy;
            transform.translation = Vec3::from_array(illusion.duel.enemy.body.position);
            transform.rotation = Quat::from_rotation_y(illusion.duel.enemy.body.yaw);
            if let Some(name) = &report.animation {
                if (animator.clip_name() != name || report.clip_started)
                    && let Ok(clip) = cast.library.clip(name)
                {
                    animator.play_named(name, clip, false, report.blend_in);
                }
                animator.time = report.anim_time;
                animator.speed = 0.0;
                animator.apply_root_motion = false;
            }
            if illusion.duel.rules.fighters[1].dead() || report.fade >= 0.5 {
                *visibility = Visibility::Hidden;
            }
            continue;
        }
        if let Some(dragon) = sim
            .duel
            .old_dragons
            .as_ref()
            .and_then(|e| e.actors.iter().find(|p| p.entity == part.entity))
        {
            let report = &dragon.report.enemy;
            transform.translation = Vec3::from_array(dragon.duel.enemy.body.position);
            transform.rotation = Quat::from_rotation_y(dragon.duel.enemy.body.yaw);
            if let Some(name) = &report.animation {
                if (animator.clip_name() != name || report.clip_started)
                    && let Ok(clip) = cast.library.clip(name)
                {
                    animator.play_named(name, clip, false, report.blend_in);
                }
                animator.time = report.anim_time;
                animator.speed = 0.0;
                animator.apply_root_motion = false;
            }
            if let Some(bits) = scene.masks.get(&part.entity) {
                let mut values = [2u8; 32];
                for (&bit, &shown) in bits {
                    if let Some(v) = values.get_mut(bit as usize) {
                        *v = u8::from(shown);
                    }
                }
                mask.apply_event(&values);
            }
            continue;
        }
        if let Some(phantom) = sim
            .duel
            .monk_phantoms
            .as_ref()
            .and_then(|e| e.actors.iter().find(|p| p.entity == part.entity))
        {
            let report = &phantom.report.enemy;
            transform.translation = Vec3::from_array(phantom.duel.enemy.body.position);
            transform.rotation = Quat::from_rotation_y(phantom.duel.enemy.body.yaw);
            if let Some(name) = &report.animation {
                if (animator.clip_name() != name || report.clip_started)
                    && let Ok(clip) = cast.library.clip(name)
                {
                    animator.play_named(name, clip, false, report.blend_in);
                }
                animator.time = report.anim_time;
                animator.speed = 0.0;
                animator.apply_root_motion = false;
            }
            if report.fade >= 0.5 {
                *visibility = Visibility::Hidden;
            }
            continue;
        }
        if let Some(monkey) = sim
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|e| e.monkeys.iter().find(|m| m.entity == part.entity))
        {
            if sim
                .duel
                .screen_monkeys
                .as_ref()
                .is_some_and(|encounter| encounter.camouflaged(part.entity))
            {
                *visibility = Visibility::Hidden;
            }
            let report = &monkey.report.enemy;
            transform.translation = Vec3::from_array(monkey.duel.enemy.body.position);
            transform.rotation = Quat::from_rotation_y(monkey.duel.enemy.body.yaw);
            if let Some(name) = &report.animation {
                if (animator.clip_name() != name || report.clip_started)
                    && let Ok(clip) = cast.library.clip(name)
                {
                    animator.play_named(name, clip, false, report.blend_in);
                }
                animator.time = report.anim_time;
                animator.speed = 0.0;
                animator.apply_root_motion = false;
            }
            continue;
        }
        let bone_warp = events
            .scripts
            .arena_c_state
            .bone_warps
            .get(&part.entity)
            .copied();
        if bone_warp != cast.requested_bone_warp
            && let Some((entity, dummy)) = bone_warp
            && entity == events.boss_entity
        {
            let model = sim.duel.enemy.bone_source_matrices();
            if let Some(&[position, forward, up]) = sim.duel.fighters[1]
                .rig
                .frames(dummy, &model, sim.duel.enemy.source_to_world())
                .first()
            {
                *transform = crate::fxr::frame_transform(
                    position.to_array(),
                    forward.to_array(),
                    up.to_array(),
                );
                cast.requested_bone_warp = bone_warp;
            }
        }
        if bone_warp.is_none() {
            cast.requested_bone_warp = None;
        }
        if let Some(&region) = scene.warps.get(&part.entity)
            && cast.requested_warp != Some(region)
            && let Some(spot) =
                sekiro_sim::events::warp_point(&staging.cache, staging.area, staging.block, region)
        {
            transform.translation = Vec3::from_array(spot.position);
            transform.rotation = Quat::from_rotation_y(spot.yaw);
            cast.requested_warp = Some(region);
        }
        animator.speed = if scene
            .animation_enabled
            .get(&part.entity)
            .copied()
            .unwrap_or(true)
        {
            1.0
        } else {
            0.0
        };
        if let Some(&request) = scene.animations.get(&part.entity)
            && cast.requested != Some(request)
        {
            let name = format!("a000_{:06}", request.0);
            match cast.library.clip(&name) {
                Ok(clip) => animator.play_named(&name, clip, request.1, 0.0),
                Err(error) => warn!("supporting actor {}: {error}", part.entity),
            }
            cast.requested = Some(request);
        }
        // 12505990 disables ordinary centipede animation updates; 12505965 still
        // explicitly requests its finishing playback without enabling that state.
        if part.entity == 2_500_852 && cast.requested == Some((20000, false)) {
            animator.speed = 1.0;
        }
        if let Some(bits) = scene.masks.get(&part.entity) {
            let mut values = [2u8; 32];
            for (&bit, &shown) in bits {
                if let Some(value) = values.get_mut(bit as usize) {
                    *value = u8::from(shown);
                }
            }
            mask.apply_event(&values);
        }
    }
}
