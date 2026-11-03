//! Authored BTL local lights, shared by map capture and play.
use super::*;

#[derive(Clone)]
struct LocalLight {
    block: String,
    position: Vec3,
    color: Color,
    intensity: f32,
    range: f32,
    spot: Option<(Quat, f32)>,
    shadows: bool,
}

#[derive(Resource, Default)]
pub(super) struct LocalLights {
    lights: Vec<LocalLight>,
    active: std::collections::BTreeMap<usize, Entity>,
}

impl LocalLights {
    pub(super) fn install(app: &mut App, dir: &Path, args: &[String]) -> Result<()> {
        let mut lights = Self::load(dir, args)?;
        if !crate::lighting::legacy_map_lights()
            && let Some(world) = app.world().get_resource::<world::MapWorld>()
        {
            for id in world.blocks.keys().filter(|id| *id != &world.primary) {
                let neighbour = dir.parent().context("map parent")?.join(id);
                lights.lights.extend(Self::load(&neighbour, args)?.lights);
            }
        }
        app.insert_resource(lights);
        Ok(())
    }

    pub(super) fn load(dir: &Path, args: &[String]) -> Result<Self> {
        let mut result = Self::default();
        if args.iter().any(|a| a == "--no-local-lights") {
            return Ok(result);
        }
        let scale = args
            .iter()
            .position(|a| a == "--local-light-scale")
            .map(|i| {
                args.get(i + 1)
                    .context("--local-light-scale needs a multiplier")?
                    .parse::<f32>()
                    .context("invalid local light scale")
            })
            .transpose()?
            .unwrap_or(1.);
        anyhow::ensure!(
            scale.is_finite() && scale >= 0.,
            "local light scale must be finite and nonnegative"
        );
        let path = dir.join("lights.json");
        if !path.exists() {
            error!(
                "missing BTL light bank: {} (re-extract map)",
                path.display()
            );
            return Ok(result);
        }
        let value: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let layout: Value = serde_json::from_slice(&std::fs::read(dir.join("layout.json"))?)?;
        let mut map_rotation = Quat::IDENTITY;
        let mut map_translation = Vec3::ZERO;
        if !crate::lighting::legacy_map_lights()
            && let Some(offset) = layout["map_offsets"].as_array().and_then(|a| a.first())
        {
            map_translation = Vec3::from_array(serde_json::from_value(offset["position"].clone())?);
            let yaw = offset["rotation_y"]
                .as_f64()
                .context("MapOffset rotation")? as f32;
            map_rotation = Quat::from_rotation_y(yaw.to_radians());
        }
        anyhow::ensure!(value["version"] == 1, "unsupported lights.json version");
        let banks = value["banks"].as_array().context("BTL banks array")?;
        let bank = args
            .iter()
            .position(|a| a == "--light-bank")
            .map(|i| {
                args.get(i + 1)
                    .context("--light-bank needs a bank number")?
                    .parse::<u32>()
                    .context("invalid BTL bank")
            })
            .transpose()?;
        let requested = bank;
        let bank = requested.unwrap_or(0);
        let selected = banks.iter().find(|b| {
            b["source"]
                .as_str()
                .is_some_and(|s| s.ends_with(&format!("_{bank:04}.btl")))
        });
        let Some(selected) = selected else {
            error!("missing BTL bank {bank:04} in {}", dir.display());
            anyhow::ensure!(requested.is_none(), "BTL bank {bank:04} is absent");
            return Ok(result);
        };
        for row in selected["lights"].as_array().context("BTL lights array")? {
            let raw: Vec<u8> = serde_json::from_value(row["raw"].clone())?;
            let native = sekiro_formats::btl::parse_record(
                &raw,
                row["name"].as_str().unwrap_or_default().into(),
            )?;
            anyhow::ensure!(
                native.kind <= 1,
                "unsupported BTL light type {}",
                native.kind
            );
            let vector = |key: &str| -> Result<[f32; 3]> {
                let a = row[key].as_array().context("BTL vector")?;
                anyhow::ensure!(a.len() == 3, "BTL vector length");
                let values = [0, 1, 2].map(|i| a[i].as_f64().map(|v| v as f32));
                let [Some(x), Some(y), Some(z)] = values else {
                    anyhow::bail!("BTL vector values")
                };
                anyhow::ensure!([x, y, z].iter().all(|v| v.is_finite()), "BTL finite vector");
                Ok([x, y, z])
            };
            let color = vector("color")?;
            let intensity = row["intensity"].as_f64().context("BTL intensity")? as f32;
            let range = row["range"].as_f64().context("BTL range")? as f32;
            anyhow::ensure!(
                intensity.is_finite() && intensity >= 0. && range.is_finite() && range >= 0.,
                "BTL range/intensity"
            );
            result.lights.push(LocalLight {
                block: dir
                    .file_name()
                    .context("map ID")?
                    .to_string_lossy()
                    .into_owned(),
                position: {
                    // MSB parts are already placed; only ancillary BTL coordinates use MapOffset.
                    let p = map_rotation * Vec3::from_array(native.position) + map_translation;
                    Vec3::new(-p.x, p.y, p.z)
                },
                // 3892e0 squares the local-light diffuse RGB before multiplying power.
                // Keep the old upload only for the matched runtime control.
                color: {
                    let rgb = Vec3::from_array(color) / 255.;
                    let rgb = if crate::lighting::legacy_map_lights() {
                        rgb
                    } else {
                        rgb * rgb
                    };
                    Color::linear_rgb(rgb.x, rgb.y, rgb.z)
                },
                intensity: intensity * scale,
                range,
                spot: if native.kind == 1 && !crate::lighting::legacy_map_lights() {
                    let [x, y, z] = native.rotation;
                    let rotation = Quat::from_rotation_y(-map_rotation.to_euler(EulerRot::YXZ).0)
                        * Quat::from_euler(EulerRot::YXZ, -y, x, -z)
                        * Quat::from_rotation_y(std::f32::consts::PI);
                    // BTL angles are degrees; Bevy takes a half angle and emits along -Z.
                    Some((rotation, native.cone_angle.to_radians() * 0.5))
                } else {
                    None
                },
                shadows: native.cast_shadows && !crate::lighting::legacy_map_lights(),
            });
        }
        info!(
            "BTL bank {}: {} lights, scale {scale}; authored range, clustered GPU culling",
            selected["source"],
            result.lights.len()
        );
        Ok(result)
    }
}

pub(super) fn update_local_lights(
    mut commands: Commands,
    time: Res<Time>,
    mut lights: ResMut<LocalLights>,
    focus: Query<&GlobalTransform, With<MapFocus>>,
    world: Option<Res<world::MapWorld>>,
    mut last: Local<f32>,
) {
    let _timer = crate::perf_log::SystemTimer::start("update_local_lights");
    let legacy = crate::lighting::legacy_map_lights();
    if legacy && time.elapsed_secs() - *last < 0.5 {
        return;
    }
    *last = time.elapsed_secs();
    let Some(focus) = focus.iter().next() else {
        return;
    };
    let position = focus.translation();
    let mut candidates: Vec<_> = lights
        .lights
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if !legacy
                && world.as_ref().is_some_and(|world| {
                    world
                        .blocks
                        .get(&l.block)
                        .is_some_and(|b| b.state != world::RequestState::Ready)
                })
            {
                return None;
            }
            let distance = l.position.distance_squared(position);
            (l.intensity > 0. && l.range > 0. && (!legacy || distance <= (60. + l.range).powi(2)))
                .then_some((i, distance))
        })
        .collect();
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let wanted: std::collections::BTreeSet<_> = candidates
        .into_iter()
        .take(if legacy { 64 } else { usize::MAX })
        .map(|(i, _)| i)
        .collect();
    let old_count = lights.active.len();
    let removed: Vec<_> = lights
        .active
        .keys()
        .copied()
        .filter(|i| !wanted.contains(i))
        .collect();
    for i in removed {
        commands.entity(lights.active.remove(&i).unwrap()).despawn();
    }
    for i in wanted {
        if lights.active.contains_key(&i) {
            continue;
        }
        let light = &lights.lights[i];
        let mut entity = commands.spawn(Transform::from_translation(light.position));
        if let Some((rotation, cone)) = light.spot {
            entity.insert((
                SpotLight {
                    color: light.color,
                    intensity: light.intensity * crate::lighting::LOCAL_LUMENS_PER_UNIT,
                    range: light.range,
                    radius: 0.,
                    inner_angle: 0.,
                    outer_angle: cone,
                    shadow_maps_enabled: light.shadows,
                    ..default()
                },
                Transform::from_translation(light.position).with_rotation(rotation),
            ));
        } else {
            entity.insert(PointLight {
                color: light.color,
                intensity: light.intensity * crate::lighting::LOCAL_LUMENS_PER_UNIT,
                range: light.range,
                radius: 0.,
                shadow_maps_enabled: light.shadows,
                ..default()
            });
        }
        lights.active.insert(i, entity.id());
    }
    if lights.active.len() != old_count {
        info!("BTL active lights: {}", lights.active.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn installed_banks_keep_spot_types_and_apply_only_ancillary_offsets() {
        let root = sekiro_sim::cache::root().join("maps");
        if !root.join("m11_00_00_00/lights.json").exists() {
            return;
        }
        let outskirts = LocalLights::load(&root.join("m11_00_00_00"), &[]).unwrap();
        let castle = LocalLights::load(&root.join("m11_01_00_00"), &[]).unwrap();
        assert_eq!(outskirts.lights.len(), 16);
        assert_eq!(castle.lights.len(), 34);
        assert_eq!(castle.lights.iter().filter(|l| l.spot.is_some()).count(), 2);
        assert!(
            outskirts
                .lights
                .iter()
                .chain(&castle.lights)
                .all(|l| !l.shadows)
        );
        let before = Vec3::new(139.609, -144.677, 1.99318);
        assert!(outskirts.lights[0].position.distance(before) > 90.);
        assert!((outskirts.lights[0].position.y - (-141.677)).abs() < 0.001);
        assert!(
            castle.lights[0]
                .position
                .distance(Vec3::new(344.742, -31.3559, 258.093))
                < 0.001
        );
        assert!(
            LocalLights::load(
                &root.join("m11_01_00_00"),
                &["--light-bank".into(), "9999".into()]
            )
            .is_err()
        );
    }
    #[test]
    fn clustered_lights_keep_every_authored_contributor() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(LocalLights {
                lights: (0..100)
                    .map(|i| LocalLight {
                        block: String::new(),
                        position: Vec3::X * i as f32 * 0.1,
                        color: Color::WHITE,
                        intensity: 1.,
                        range: 4.,
                        spot: None,
                        shadows: false,
                    })
                    .collect(),
                active: default(),
            })
            .add_systems(Update, update_local_lights);
        let focus = app
            .world_mut()
            .spawn((MapFocus, GlobalTransform::default()))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(1));
        app.update();
        let lights = app.world().resource::<LocalLights>();
        assert_eq!(lights.active.len(), 100);
        assert_eq!(
            lights.active.keys().copied().collect::<Vec<_>>(),
            (0..100).collect::<Vec<_>>()
        );
        for entity in lights.active.values() {
            assert!(
                !app.world()
                    .get::<PointLight>(*entity)
                    .unwrap()
                    .shadow_maps_enabled
            );
        }
        *app.world_mut().get_mut::<GlobalTransform>(focus).unwrap() =
            GlobalTransform::from_translation(Vec3::X * 1000.);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<LocalLights>().active.len(), 100);
        assert_eq!(
            app.world_mut()
                .query::<&PointLight>()
                .iter(app.world())
                .count(),
            100
        );
    }
}
