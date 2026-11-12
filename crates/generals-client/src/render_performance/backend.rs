//! Opt-in CPU wall spans around Bevy's existing renderer stages.
use super::{Samples, Timings, record};
use bevy::{
    app::SubApp,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        mesh::{RenderMesh, allocator::allocate_and_free_meshes},
        pipelined_rendering::RenderExtractApp,
        render_asset::prepare_assets,
    },
};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        // Profiling is requested before DefaultPlugins creates these sub-apps.
        // finish runs after their creation and before pipelined rendering moves
        // RenderApp to its thread during plugin cleanup.
        let samples = Arc::clone(&app.world().resource::<Timings>().samples);
        let allocations = app
            .world()
            .resource::<crate::model_upload::diagnostics::Counters>()
            .clone();
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            wrap_extract(render, Arc::clone(&samples), "bevy-extract");
            install_stages(render, Arc::clone(&samples));
            crate::model_upload::diagnostics::install(render, allocations, Arc::clone(&samples));
        }
        if let Some(extract) = app.get_sub_app_mut(RenderExtractApp) {
            wrap_extract(extract, samples, "bevy-render-wait-and-extract");
        }
        eprintln!(
            "live-backend spans=cpu_wall wait_and_extract=inclusive main_render_threads=overlap gpu_timestamps=unmeasured"
        );
    }
}

fn wrap_extract(app: &mut SubApp, samples: Arc<Mutex<Samples>>, name: &'static str) {
    let Some(mut extract) = app.take_extract() else {
        return;
    };
    app.set_extract(move |main, render| {
        let start = Instant::now();
        extract(main, render);
        record(&samples, name, start.elapsed());
    });
}

#[derive(Resource)]
struct Stages {
    samples: Arc<Mutex<Samples>>,
    previous: Option<Instant>,
    assets_start: Option<Instant>,
    prepare_start: Option<Instant>,
    mesh_upload_start: Option<Instant>,
}

impl Stages {
    fn finish(&mut self, name: &'static str) {
        let now = Instant::now();
        if let Some(previous) = self.previous.replace(now) {
            record(&self.samples, name, now.duration_since(previous));
        }
    }
}

fn install_stages(render: &mut SubApp, samples: Arc<Mutex<Samples>>) {
    use RenderSystems as Stage;
    render.insert_resource(Stages {
        samples,
        previous: None,
        assets_start: None,
        prepare_start: None,
        mesh_upload_start: None,
    });
    // These boundaries follow the existing Render::base_schedule chains. They
    // preserve parallelism inside each stage and do not replace engine systems.
    render.add_systems(
        Render,
        (
            start.before(Stage::ExtractCommands),
            commands
                .after(Stage::ExtractCommands)
                .before(Stage::PrepareAssets),
            asset_preparation
                .after(Stage::PrepareAssets)
                .before(Stage::PrepareMeshes),
            assets
                .after(Stage::PrepareMeshes)
                .before(Stage::CreateViews),
            views.after(Stage::PrepareViews).before(Stage::Queue),
            queue.after(Stage::PhaseSort).before(Stage::Prepare),
            resources
                .after(Stage::PrepareResources)
                .before(Stage::PrepareResourcesBatchPhases),
            batching
                .after(Stage::PrepareResourcesBatchPhases)
                .before(Stage::PrepareResourcesWritePhaseBuffers),
            phase_buffers
                .after(Stage::PrepareResourcesCollectPhaseBuffers)
                .before(Stage::PrepareResourcesFlush),
            flush
                .after(Stage::PrepareResourcesFlush)
                .before(Stage::PrepareBindGroups),
            prepare.after(Stage::Prepare).before(Stage::Render),
            draw.after(Stage::Render).before(Stage::Cleanup),
            cleanup.after(Stage::PostCleanup),
        ),
    );
    // Only the mesh allocator depends on this start marker; other asset
    // preparation remains parallel. This interval includes scheduling delay,
    // allocator repacking and RenderQueue writes, not GPU execution time.
    render.add_systems(
        Render,
        (
            mesh_upload_start
                .before(crate::model_upload::Stage::Retain)
                .before(allocate_and_free_meshes),
            mesh_upload_end
                .after(allocate_and_free_meshes)
                .after(crate::model_upload::Stage::Upload)
                .after(mesh_upload_start)
                .before(prepare_assets::<RenderMesh>),
        )
            .in_set(Stage::PrepareAssets),
    );
}

fn start(mut stages: ResMut<Stages>) {
    let now = Instant::now();
    stages.previous = Some(now);
    stages.assets_start = Some(now);
}

fn commands(mut stages: ResMut<Stages>) {
    stages.finish("bevy-extract-commands");
}

fn asset_preparation(mut stages: ResMut<Stages>) {
    stages.finish("bevy-prepare-assets");
}

fn assets(mut stages: ResMut<Stages>) {
    stages.finish("bevy-prepare-meshes");
    if let Some(start) = stages.assets_start.take() {
        record(
            &stages.samples,
            "bevy-extract-commands-assets-meshes",
            start.elapsed(),
        );
    }
}

fn views(mut stages: ResMut<Stages>) {
    // prepare_windows acquires the next surface texture in PrepareViews. This
    // interval may include GPU/display backpressure, not only CPU computation.
    stages.finish("bevy-views-and-surface-acquire");
}

fn queue(mut stages: ResMut<Stages>) {
    stages.finish("bevy-queue-and-sort");
    stages.prepare_start = stages.previous;
}

fn resources(mut stages: ResMut<Stages>) {
    stages.finish("bevy-prepare-resources");
}

fn batching(mut stages: ResMut<Stages>) {
    stages.finish("bevy-batch-phases");
}

fn phase_buffers(mut stages: ResMut<Stages>) {
    stages.finish("bevy-write-and-collect-phase-buffers");
}

fn flush(mut stages: ResMut<Stages>) {
    stages.finish("bevy-flush-resources");
}

fn prepare(mut stages: ResMut<Stages>) {
    stages.finish("bevy-prepare-bind-groups");
    if let Some(start) = stages.prepare_start.take() {
        record(
            &stages.samples,
            "bevy-prepare-gpu-resources",
            start.elapsed(),
        );
    }
}

fn mesh_upload_start(mut stages: ResMut<Stages>) {
    stages.mesh_upload_start = Some(Instant::now());
}

fn mesh_upload_end(mut stages: ResMut<Stages>) {
    if let Some(start) = stages.mesh_upload_start.take() {
        record(
            &stages.samples,
            "bevy-mesh-allocator-and-upload",
            start.elapsed(),
        );
    }
}

fn draw(mut stages: ResMut<Stages>) {
    // render_system also submits readbacks and presents after RenderGraph ends.
    stages.finish("bevy-render-submit-present");
}

fn cleanup(mut stages: ResMut<Stages>) {
    stages.finish("bevy-cleanup");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource)]
    struct Probe(Vec<&'static str>);

    #[test]
    fn profiling_preserves_original_extraction_and_render_stage_order() {
        let samples = Arc::new(Mutex::new(Samples::new()));
        let mut app = SubApp::new();
        app.insert_resource(Probe(Vec::new()));
        let mut main = World::new();
        main.insert_resource(Probe(Vec::new()));
        app.set_extract(|main, render| {
            main.resource_mut::<Probe>().0.push("main-extract");
            render.resource_mut::<Probe>().0.push("render-extract");
        });
        wrap_extract(&mut app, Arc::clone(&samples), "bevy-extract");
        app.extract(&mut main);
        assert_eq!(main.resource::<Probe>().0, ["main-extract"]);
        app.add_schedule(Render::base_schedule());
        install_stages(&mut app, Arc::clone(&samples));
        add_stage_probes(&mut app);
        app.world_mut().run_schedule(Render);
        assert_eq!(
            app.world().resource::<Probe>().0,
            [
                "render-extract",
                "commands",
                "assets",
                "meshes",
                "create-views",
                "specialize",
                "views",
                "queue",
                "sort",
                "resources",
                "batch",
                "write-phase-buffers",
                "collect-phase-buffers",
                "flush",
                "bind-groups",
                "render",
                "cleanup",
                "post-cleanup",
            ]
        );
        let samples = samples.lock().unwrap();
        assert_eq!(samples.len(), 16);
        assert!(samples.values().all(|(count, _)| *count == 1));
        drop(samples);
    }

    fn add_stage_probes(app: &mut SubApp) {
        for (stage, label) in [
            (RenderSystems::ExtractCommands, "commands"),
            (RenderSystems::PrepareAssets, "assets"),
            (RenderSystems::PrepareMeshes, "meshes"),
            (RenderSystems::CreateViews, "create-views"),
            (RenderSystems::Specialize, "specialize"),
            (RenderSystems::PrepareViews, "views"),
            (RenderSystems::Queue, "queue"),
            (RenderSystems::PhaseSort, "sort"),
            (RenderSystems::PrepareResources, "resources"),
            (RenderSystems::PrepareResourcesBatchPhases, "batch"),
            (
                RenderSystems::PrepareResourcesWritePhaseBuffers,
                "write-phase-buffers",
            ),
            (
                RenderSystems::PrepareResourcesCollectPhaseBuffers,
                "collect-phase-buffers",
            ),
            (RenderSystems::PrepareResourcesFlush, "flush"),
            (RenderSystems::PrepareBindGroups, "bind-groups"),
            (RenderSystems::Render, "render"),
            (RenderSystems::Cleanup, "cleanup"),
            (RenderSystems::PostCleanup, "post-cleanup"),
        ] {
            app.add_systems(
                Render,
                (move |mut probe: ResMut<Probe>| {
                    probe.0.push(label);
                })
                .in_set(stage),
            );
        }
    }
}
