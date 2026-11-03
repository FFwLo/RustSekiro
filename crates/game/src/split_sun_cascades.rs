//! Give each sun cascade its own encoder, which Bevy finishes in parallel before submission.
use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
use bevy::ecs::schedule::ScheduleCleanupPolicy;
use bevy::ecs::system::{BoxedReadOnlySystem, Deferred, SystemBuffer, SystemMeta};
use bevy::ecs::world::DeferredWorld;
use bevy::pbr::{ViewLightEntities, per_view_shadow_pass};
use bevy::prelude::*;
use bevy::render::{RenderApp, renderer::CurrentView};

pub(crate) struct SplitSunCascadesPlugin;

impl Plugin for SplitSunCascadesPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        let world = render.world_mut();
        let mut schedule = world
            .resource_mut::<Schedules>()
            .remove(Core3d)
            .expect("3D schedule");
        let removed = schedule
            .remove_systems_in_set(
                per_view_shadow_pass::<false>,
                world,
                ScheduleCleanupPolicy::RemoveSystemsOnly,
            )
            .expect("replace sun shadow pass");
        assert_eq!(removed, 1, "sun shadow pass must be replaced exactly once");
        schedule.add_systems(split_sun_cascades
            .after(bevy::pbr::early_prepass_build_indirect_parameters)
            .before(bevy::core_pipeline::mip_generation::experimental::depth::early_downsample_depth)
            .before(per_view_shadow_pass::<true>)
            .before(Core3dSystems::MainPass));
        world.resource_mut::<Schedules>().insert(schedule);
    }
}

// A deferred buffer is system-local, just like Local, but runs in Bevy's ordered
// encoder-queue phase. Preprocessing is queued first without flushing unrelated systems.
struct SunPass {
    system: BoxedReadOnlySystem,
    lights: Vec<Entity>,
}

impl FromWorld for SunPass {
    fn from_world(world: &mut World) -> Self {
        let mut system = IntoSystem::into_system(per_view_shadow_pass::<false>);
        system.initialize(world);
        Self {
            system: Box::new(system),
            lights: vec![Entity::PLACEHOLDER],
        }
    }
}

impl SystemBuffer for SunPass {
    fn queue(&mut self, _: &SystemMeta, mut world: DeferredWorld) {
        let _timer = crate::perf_log::SystemTimer::start("shadow tasks");
        let view = world.resource::<CurrentView>().0;
        let Some(view_lights) = world.get_mut::<ViewLightEntities>(view) else {
            return;
        };
        let view_lights = view_lights.into_inner() as *mut ViewLightEntities;
        // SAFETY: DeferredWorld excludes other world access and structural changes, so
        // the component stays at this address. No reference through this pointer lives
        // across the native read-only system call. Its deferred buffer only appends to
        // PendingCommandBuffers, and cannot move or borrow ViewLightEntities.
        unsafe {
            std::mem::swap(&mut self.lights, &mut (*view_lights).lights);
            for &light in &self.lights {
                (&mut (*view_lights).lights)[0] = light;
                if let Err(error) = self.system.run_readonly((), &world) {
                    error!("sun shadows skipped: {error}");
                    break;
                }
                self.system.queue_deferred(world.reborrow());
            }
            std::mem::swap(&mut self.lights, &mut (*view_lights).lights);
        }
    }
}

fn split_sun_cascades(_pass: Deferred<SunPass>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Submitted(Vec<&'static str>);

    #[derive(Default)]
    struct Encoder(Vec<&'static str>);

    impl SystemBuffer for Encoder {
        fn queue(&mut self, _: &SystemMeta, mut world: DeferredWorld) {
            world.resource_mut::<Submitted>().0.append(&mut self.0);
        }
    }

    struct Cascades(BoxedReadOnlySystem);

    impl FromWorld for Cascades {
        fn from_world(world: &mut World) -> Self {
            let mut system = IntoSystem::into_system(|mut encoder: Deferred<Encoder>| {
                encoder.0.push("cascade");
            });
            system.initialize(world);
            Self(Box::new(system))
        }
    }

    impl SystemBuffer for Cascades {
        fn queue(&mut self, _: &SystemMeta, mut world: DeferredWorld) {
            for _ in 0..4 {
                self.0.run_readonly((), &world).unwrap();
                self.0.queue_deferred(world.reborrow());
            }
        }
    }

    #[test]
    fn cascades_submit_after_preprocessing_before_color_on_every_frame() {
        fn preprocess(mut encoder: Deferred<Encoder>) {
            encoder.0.push("preprocess");
        }
        fn split(_cascades: Deferred<Cascades>) {}
        fn color(mut encoder: Deferred<Encoder>) {
            encoder.0.push("color");
        }
        let mut world = World::new();
        world.init_resource::<Submitted>();
        let mut schedule = Core3d::base_schedule();
        schedule.add_systems((preprocess, split, color).chain());
        for _ in 0..3 {
            schedule.run(&mut world);
            assert_eq!(
                std::mem::take(&mut world.resource_mut::<Submitted>().0),
                [
                    "preprocess",
                    "cascade",
                    "cascade",
                    "cascade",
                    "cascade",
                    "color"
                ]
            );
        }
    }
}
