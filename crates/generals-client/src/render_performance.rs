//! Repeatable installed-content CPU workloads; no gameplay population or LOD changes.
use crate::{
    model_animation, particles, playtest::MAP_NAME, skirmish_data, terrain::Terrain, world_data,
};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use std::{
    collections::BTreeMap,
    error::Error,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

mod backend;
mod mesh_changes;

pub type Samples = BTreeMap<&'static str, (u32, Duration)>;

#[derive(Resource)]
pub struct Timings {
    started: Option<Instant>,
    samples: Arc<Mutex<Samples>>,
    frames: u32,
    previous: Option<Instant>,
    intervals: Vec<Duration>,
    changed: [usize; 3],
    mesh_changes: mesh_changes::Changes,
    peak: SceneCounts,
}

pub struct Span<'a> {
    timings: &'a Timings,
    name: &'static str,
    start: Instant,
}

const WARMUP_FRAMES: u32 = 120;
const SAMPLE_FRAMES: u32 = 300;

impl Timings {
    fn new() -> Self {
        Self {
            started: None,
            samples: Arc::new(Mutex::new(BTreeMap::new())),
            frames: 0,
            previous: None,
            intervals: Vec::with_capacity(300),
            changed: [0; 3],
            mesh_changes: mesh_changes::Changes::new(),
            peak: SceneCounts::empty(),
        }
    }

    fn readiness_timed_out(&mut self, now: Instant) -> bool {
        let started = *self.started.get_or_insert(now);
        now.duration_since(started) >= Duration::from_mins(1)
    }

    fn record_frame(&mut self, now: Instant) {
        self.frames += 1;
        if let Some(previous) = self.previous.replace(now)
            && self.frames > WARMUP_FRAMES
            && self.frames <= WARMUP_FRAMES + SAMPLE_FRAMES
        {
            self.intervals.push(now.duration_since(previous));
        }
    }

    fn discard_frame_window(&mut self) {
        self.frames = 0;
        self.previous = None;
        self.intervals.clear();
        self.changed = [0; 3];
        self.mesh_changes.reset_counts();
        self.peak = SceneCounts::empty();
    }

    pub fn span(&self, name: &'static str) -> Span<'_> {
        Span {
            timings: self,
            name,
            start: Instant::now(),
        }
    }
}

impl Drop for Span<'_> {
    fn drop(&mut self) {
        record(&self.timings.samples, self.name, self.start.elapsed());
    }
}

pub fn record(samples: &Mutex<Samples>, name: &'static str, elapsed: Duration) {
    if let Ok(mut samples) = samples.lock() {
        let (count, total) = samples.entry(name).or_default();
        *count += 1;
        *total += elapsed;
    }
}

/// Installs only on an explicitly requested live profiling run.
pub fn install(app: &mut App) {
    app.insert_resource(Timings::new())
        .insert_resource(crate::model_upload::diagnostics::Counters::new())
        .add_plugins(backend::Plugin)
        .add_systems(Last, report);
    eprintln!(
        "live-profile-v1 warmup_frames={WARMUP_FRAMES} sampled_frames={SAMPLE_FRAMES} frame_clock=wall_interval"
    );
}

struct SceneCounts {
    entities: usize,
    mesh_entities: usize,
    visible_meshes: usize,
    meshes: usize,
    materials: usize,
    images: usize,
    objects: usize,
    alive_objects: usize,
}

impl SceneCounts {
    const fn empty() -> Self {
        Self {
            entities: 0,
            mesh_entities: 0,
            visible_meshes: 0,
            meshes: 0,
            materials: 0,
            images: 0,
            objects: 0,
            alive_objects: 0,
        }
    }

    fn include(&mut self, other: &Self) {
        self.entities = self.entities.max(other.entities);
        self.mesh_entities = self.mesh_entities.max(other.mesh_entities);
        self.visible_meshes = self.visible_meshes.max(other.visible_meshes);
        self.meshes = self.meshes.max(other.meshes);
        self.materials = self.materials.max(other.materials);
        self.images = self.images.max(other.images);
        self.objects = self.objects.max(other.objects);
        self.alive_objects = self.alive_objects.max(other.alive_objects);
    }

    fn print(&self, kind: &str) {
        eprintln!(
            "live-scene kind={kind} entities={} mesh_entities={} visible_meshes={} mesh_assets={} source_material_assets={} image_assets={} sim_objects={} alive_objects={}",
            self.entities,
            self.mesh_entities,
            self.visible_meshes,
            self.meshes,
            self.materials,
            self.images,
            self.objects,
            self.alive_objects
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct LiveScene<'w, 's> {
    battle: Res<'w, crate::playtest::Match>,
    camera: Res<'w, crate::playtest::CameraRig>,
    window: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<crate::source_material::SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
    entities: Query<'w, 's, Entity>,
    visuals: Query<'w, 's, Option<&'static ViewVisibility>, With<Mesh3d>>,
    mesh_sources: Query<'w, 's, mesh_changes::Source<'static>, Changed<Mesh3d>>,
    projectile_parents: Query<'w, 's, (), With<crate::projectile_render::ProjectileVisual>>,
    mesh_events: MessageReader<'w, 's, AssetEvent<Mesh>>,
    material_events: MessageReader<'w, 's, AssetEvent<crate::source_material::SourceMaterial>>,
    image_events: MessageReader<'w, 's, AssetEvent<Image>>,
}

impl LiveScene<'_, '_> {
    fn print_battle(&self, stage: &str) {
        let game = &self.battle.simulation;
        if let Ok(window) = self.window.single() {
            eprintln!(
                "live-view stage={stage} tick={} pixels={}x{} camera_world={:?} projectiles={}",
                game.tick,
                window.physical_width(),
                window.physical_height(),
                self.camera.focus * 10.0,
                game.projectiles.len(),
            );
        }
        let mut counts = BTreeMap::new();
        for entity in &game.entities {
            if entity.alive() && entity.owner < game.slots.len() {
                let name = game.catalog[entity.type_index].name.as_str();
                *counts.entry((entity.owner, name)).or_insert(0_u32) += 1;
            }
        }
        for ((owner, template), count) in counts {
            eprintln!("live-roster stage={stage} owner={owner} template={template} alive={count}");
        }
    }

    fn counts(&self) -> SceneCounts {
        SceneCounts {
            entities: self.entities.iter().len(),
            mesh_entities: self.visuals.iter().len(),
            visible_meshes: self
                .visuals
                .iter()
                .filter(|visibility| visibility.is_some_and(|value| value.get()))
                .count(),
            meshes: self.meshes.len(),
            materials: self.materials.len(),
            images: self.images.len(),
            objects: self.battle.simulation.entities.len(),
            alive_objects: self
                .battle
                .simulation
                .entities
                .iter()
                .filter(|unit| unit.alive())
                .count(),
        }
    }

    fn changes(&mut self, categories: &mut mesh_changes::Changes, sample: bool) -> [usize; 3] {
        categories.begin_frame();
        for source in &self.mesh_sources {
            let projectile_parent = source
                .1
                .is_some_and(|parent| self.projectile_parents.contains(parent.parent()));
            categories.observe(source, projectile_parent);
        }
        [
            self.mesh_events
                .read()
                .filter(|event| categories.event(event, sample, &self.meshes))
                .count(),
            self.material_events
                .read()
                .filter(|event| matches!(event, AssetEvent::Modified { .. }))
                .count(),
            self.image_events
                .read()
                .filter(|event| matches!(event, AssetEvent::Modified { .. }))
                .count(),
        ]
    }
}

fn report(
    mut timings: ResMut<Timings>,
    allocations: Res<crate::model_upload::diagnostics::Counters>,
    mut scene: LiveScene,
    mut capture: ResMut<crate::playtest_view::Capture>,
    surface: Res<crate::playtest_view::CaptureSurface>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    let allocations = allocations.into_inner();
    // Headless paid-scenario preparation happens after install, before app.run.
    // The pipeline/drawable deadline starts when the event loop can observe it.
    let readiness_timed_out = timings.readiness_timed_out(Instant::now());
    let surface = surface.into_inner();
    // Keep source ownership during loading too: Changed<Mesh3d> is relative to
    // this system's previous invocation even if its readiness guard returned.
    let sample = (WARMUP_FRAMES..WARMUP_FRAMES + SAMPLE_FRAMES).contains(&timings.frames)
        && surface.available();
    let changed = scene.changes(&mut timings.mesh_changes, sample);
    if timings.frames == 0 && (!capture.pipelines_ready() || !surface.available()) {
        if readiness_timed_out {
            eprintln!("Live profile timed out waiting for compiled pipelines and a drawable");
            exit.write(bevy::app::AppExit::error());
        }
        return;
    }
    if timings.frames > 0 && timings.frames < WARMUP_FRAMES + SAMPLE_FRAMES && !surface.available()
    {
        eprintln!(
            "Live profile lost its drawable; discarding incomplete samples and restarting warmup"
        );
        allocations.reset(false);
        timings.discard_frame_window();
        return;
    }
    if timings.frames == 0 {
        scene.print_battle("ready");
    }
    timings.record_frame(Instant::now());
    if timings.frames > WARMUP_FRAMES + SAMPLE_FRAMES {
        if capture.ready_for_scenario() {
            capture.finish_scenario();
        }
        // GPU compilation/readback may finish after the fixed sampling window.
        // Keep its existing failure deadline without adding timing samples.
        if !capture.is_pending() {
            exit.write(bevy::app::AppExit::Success);
        }
        return;
    }
    if timings.frames == WARMUP_FRAMES {
        if let Ok(mut samples) = timings.samples.lock() {
            samples.clear();
        }
        allocations.reset(true);
        eprintln!("live-profile warmup_complete={WARMUP_FRAMES}");
    }
    if timings.frames <= WARMUP_FRAMES {
        return;
    }
    let counts = scene.counts();
    timings.peak.include(&counts);
    for (total, count) in timings.changed.iter_mut().zip(changed) {
        *total += count;
    }
    if timings.frames != WARMUP_FRAMES + SAMPLE_FRAMES {
        return;
    }
    allocations.finish();
    scene.print_battle("final");
    eprintln!(
        "live-frames count={} {}",
        timings.intervals.len(),
        frame_summary(&mut timings.intervals)
    );
    counts.print("final");
    timings.peak.print("peak");
    eprintln!(
        "live-assets modified_meshes={} modified_source_materials={} modified_images={}",
        timings.changed[0], timings.changed[1], timings.changed[2]
    );
    timings.mesh_changes.print();
    if let Ok(samples) = timings.samples.lock() {
        for (name, (count, total)) in &*samples {
            eprintln!(
                "render-system name={name} samples={count} total_ms={:.3} mean_ms={:.3}",
                total.as_secs_f64() * 1000.0,
                total.as_secs_f64() * 1000.0 / f64::from(*count)
            );
        }
    }
    // Emit the larger owner report after the timing summary, outside its lock.
    allocations.print_owners();
    if !capture.is_pending() {
        exit.write(bevy::app::AppExit::Success);
    }
}

/// Original local setup used in the reported four-player match; no injected army.
pub fn start_skirmish(app: &mut App) -> Result<(), Box<dyn Error>> {
    use generals_sim::skirmish::{Faction, OpponentDifficulty, PlayerSlot};
    let mut battle = app.world_mut().resource_mut::<crate::playtest::Match>();
    if battle.simulation.slots.len() != 4 {
        return Err("startup profile requires the original four-player map".into());
    }
    let slots = [Faction::Usa, Faction::China, Faction::Gla, Faction::Gla]
        .into_iter()
        .enumerate()
        .map(|(index, faction)| PlayerSlot {
            faction: Some(faction),
            occupied: true,
            difficulty: (index != 0).then_some(OpponentDifficulty::Hard),
            team: Some(u8::from(index >= 2)),
            start: Some(index),
            color: Some(index),
        })
        .collect();
    battle.start_slots(slots);
    if !battle.started {
        return Err("original four-player startup failed".into());
    }
    eprintln!(
        "live-skirmish map=FallenEmpire players=4 local=USA ally=ChinaHard opponents=GLAHard,GLAHard"
    );
    // The profile releases this capture only after recording its last interval.
    app.world_mut()
        .resource_mut::<crate::playtest_view::Capture>()
        .wait_for_scenario();
    Ok(())
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = MapData::parse(&assets.read(MAP_NAME)?.bytes)?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
    let settings = world_data::camera_settings(&assets, root)?;
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    let (layout, world_objects) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )?;
    let mut animations = model_animation::load(&assets, &mut data.models, &data.types)?;
    animations.load_world(&assets, &mut data.models, &world_objects)?;
    let mut particles = particles::load(&assets, &data.types)?;
    animations.retain_models(std::mem::take(&mut data.models), layout.random_seed);
    let terrain = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(terrain, layout)?;
    println!(
        "render-workload-v1 profile={} counts=250,1000,4000 display_frames=20 logic_frames=10",
        if cfg!(debug_assertions) {
            "dev-opt1"
        } else {
            "release"
        }
    );
    for count in [250, 1000, 4000] {
        for moving in [false, true] {
            animations.benchmark(&mut game.entities[0], &game.catalog, count, moving)?;
        }
        for mixed in [false, true] {
            particles.benchmark(count, mixed)?;
        }
    }
    animations.verify_pose_reuse(&mut game.entities[0])?;
    animations.verify_world_pose_reuse()?;
    Ok(())
}

pub struct Digest(u64);

/// # Panics
/// Workloads must supply at least one timed display frame.
pub fn frame_summary(samples: &mut [Duration]) -> String {
    samples.sort_unstable();
    let millis = |index: usize| samples[index].as_secs_f64() * 1000.0;
    format!(
        "p50_ms={:.3} p95_ms={:.3} max_ms={:.3} over_60hz={} over_30hz={} over_100ms={}",
        millis(samples.len() / 2),
        millis(samples.len() * 95 / 100),
        millis(samples.len() - 1),
        samples.len() - samples.partition_point(|sample| sample.as_nanos() * 60 <= 1_000_000_000),
        samples.len() - samples.partition_point(|sample| sample.as_nanos() * 30 <= 1_000_000_000),
        samples.len() - samples.partition_point(|sample| *sample <= Duration::from_millis(100)),
    )
}

impl Digest {
    pub const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub fn word(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub fn mesh(&mut self, mesh: &Mesh, visible: bool) {
        // Vertex order remains meaningful; mesh boundaries do not. This also
        // compares contiguous native particle batches with separate systems.
        for vertex in 0..mesh.count_vertices() {
            self.word(u32::from(visible));
            for attribute in [
                Mesh::ATTRIBUTE_POSITION,
                Mesh::ATTRIBUTE_NORMAL,
                Mesh::ATTRIBUTE_UV_0,
                Mesh::ATTRIBUTE_COLOR,
            ] {
                match mesh.attribute(attribute) {
                    Some(VertexAttributeValues::Float32x3(values)) => {
                        for value in values[vertex] {
                            self.word(value.to_bits());
                        }
                    }
                    Some(VertexAttributeValues::Float32x2(values)) => {
                        for value in values[vertex] {
                            self.word(value.to_bits());
                        }
                    }
                    Some(VertexAttributeValues::Float32x4(values)) => {
                        for value in values[vertex] {
                            self.word(value.to_bits());
                        }
                    }
                    _ => self.word(0),
                }
            }
        }
    }

    pub const fn value(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{SAMPLE_FRAMES, Timings, WARMUP_FRAMES};
    use std::time::{Duration, Instant};

    #[test]
    fn display_budget_counts_keep_exact_frame_boundaries() {
        let mut frames = [
            100_000_001,
            16_666_666,
            33_333_334,
            100_000_000,
            16_666_667,
            33_333_333,
        ]
        .map(Duration::from_nanos);
        assert!(
            super::frame_summary(&mut frames).ends_with("over_60hz=5 over_30hz=3 over_100ms=1")
        );
    }

    #[test]
    fn readiness_deadline_excludes_headless_setup_but_still_limits_the_event_loop() {
        let mut timings = Timings::new();
        // A paid defense setup can exceed the entire readiness allowance before
        // the first renderer update is possible.
        let first_report = Instant::now() + Duration::from_secs(95);
        assert!(!timings.readiness_timed_out(first_report));
        assert!(!timings.readiness_timed_out(first_report + Duration::from_millis(59_999)));
        assert!(timings.readiness_timed_out(first_report + Duration::from_mins(1)));
        // Losing a covered drawable may discard measurements, but must not
        // repeatedly restart the deadline and hide a genuine rendering failure.
        timings.discard_frame_window();
        assert!(timings.readiness_timed_out(first_report + Duration::from_secs(61)));
    }

    #[test]
    fn live_report_initializes_with_explicit_disjoint_component_access() {
        use bevy::{ecs::system::System, prelude::*};

        // Initialize the actual production system. This detects B0001 resource
        // and component conflicts without graphics resources or a native window.
        let mut report = IntoSystem::into_system(super::report);
        let mut world = World::new();
        report.initialize(&mut world);
    }

    #[test]
    fn live_profile_excludes_warmup_and_records_exactly_three_hundred_frames() {
        let mut timings = Timings::new();
        let mut now = Instant::now();
        for _ in 0..WARMUP_FRAMES {
            now += Duration::from_millis(100);
            timings.record_frame(now);
        }
        assert!(timings.intervals.is_empty());
        for _ in 0..SAMPLE_FRAMES {
            now += Duration::from_millis(16);
            timings.record_frame(now);
        }
        assert_eq!(timings.intervals.len(), 300);
        assert!(
            timings
                .intervals
                .iter()
                .all(|&duration| duration == Duration::from_millis(16))
        );
        timings.record_frame(now + Duration::from_secs(1));
        assert_eq!(timings.intervals.len(), 300);
    }

    #[test]
    fn interrupted_visible_window_cannot_mix_old_samples_or_an_occluded_time_gap() {
        let mut timings = Timings::new();
        let mut now = Instant::now();
        for _ in 0..WARMUP_FRAMES + 80 {
            now += Duration::from_millis(25);
            timings.record_frame(now);
        }
        timings.discard_frame_window();
        now += Duration::from_secs(10);
        for _ in 0..WARMUP_FRAMES + SAMPLE_FRAMES {
            now += Duration::from_millis(16);
            timings.record_frame(now);
        }
        assert_eq!(timings.intervals.len(), 300);
        assert!(
            timings
                .intervals
                .iter()
                .all(|&duration| duration == Duration::from_millis(16))
        );
    }
}
