//! Opt-in allocation decisions for the existing live-profile sampling window.
use crate::render_performance::{Samples, record};
use bevy::{
    mesh::MeshVertexBufferLayouts,
    prelude::*,
    render::{Extract, ExtractSchedule, mesh::RenderMesh},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Instant,
};

mod owners;

type Counts = BTreeMap<(&'static str, &'static str), (u64, u64)>;

#[derive(Resource, Clone)]
pub struct Counters(Arc<Mutex<Window>>);

struct Window {
    generation: u64,
    active: bool,
    frames: u64,
    counts: Counts,
    outside: owners::Counts,
}

pub(super) struct Frame {
    generation: u64,
    counts: Counts,
    outside: owners::Counts,
}

#[derive(Resource)]
pub(super) struct State {
    counters: Counters,
    kinds: BTreeMap<AssetId<Mesh>, u8>,
    owners: owners::Snapshots,
    owners_enabled: bool,
    samples: Arc<Mutex<Samples>>,
}

impl Counters {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(Window {
            generation: 0,
            active: false,
            frames: 0,
            counts: BTreeMap::new(),
            outside: BTreeMap::new(),
        })))
    }

    pub(crate) fn reset(&self, active: bool) {
        let mut window = self.0.lock().expect("allocation diagnostic window");
        window.generation += 1;
        window.active = active;
        window.frames = 0;
        window.counts.clear();
        window.outside.clear();
    }

    fn collecting(&self) -> bool {
        self.0.lock().expect("allocation diagnostic window").active
    }

    fn begin(&self) -> Option<Frame> {
        let window = self.0.lock().expect("allocation diagnostic window");
        window.active.then(|| Frame {
            generation: window.generation,
            counts: BTreeMap::new(),
            outside: BTreeMap::new(),
        })
    }

    fn commit(&self, frame: Frame) {
        let mut window = self.0.lock().expect("allocation diagnostic window");
        // A retain pass crossing a reset/close belongs to neither complete window.
        if !window.active || window.generation != frame.generation {
            return;
        }
        window.frames += 1;
        for (key, (meshes, bytes)) in frame.counts {
            let total = window.counts.entry(key).or_default();
            total.0 += meshes;
            total.1 += bytes;
        }
        if !frame.outside.is_empty() {
            owners::merge(&mut window.outside, frame.outside);
        }
        drop(window);
    }

    pub(crate) fn finish(&self) {
        let mut window = self.0.lock().expect("allocation diagnostic window");
        window.active = false;
        eprintln!(
            "live-mesh-upload-window generation={} retain_passes={} scope=complete_retain_passes_between_warmup_and_summary bytes=packed_vertex_bytes gpu_time=unmeasured",
            window.generation, window.frames
        );
        for ((kind, outcome), (meshes, bytes)) in &window.counts {
            eprintln!(
                "live-mesh-upload kind={kind} outcome={outcome} meshes={meshes} vertex_bytes={bytes}"
            );
        }
    }

    pub(crate) fn print_owners(&self) {
        let outside = {
            let mut window = self.0.lock().expect("allocation diagnostic window");
            if window.outside.is_empty() {
                return;
            }
            std::mem::take(&mut window.outside)
        };
        owners::print(&outside);
    }
}

impl State {
    pub(super) fn begin(&self) -> Option<Frame> {
        self.counters.begin()
    }

    pub(super) fn count(
        &self,
        frame: &mut Option<Frame>,
        id: AssetId<Mesh>,
        mesh: &Mesh,
        outcome: &'static str,
    ) {
        let Some(frame) = frame else {
            return;
        };
        let kind = match self.kinds.get(&id).copied().unwrap_or(0) {
            1 => "animated-part",
            2 => "sorted-batch",
            3 => "shared-animated-sorted",
            4 => "terrain-surface",
            8 => "water-visual",
            _ => "other-or-unreferenced",
        };
        let count = frame.counts.entry((kind, outcome)).or_default();
        let bytes = u64::try_from(mesh.get_vertex_buffer_size()).expect("vertex buffer size");
        count.0 += 1;
        count.1 += bytes;
        if self.owners_enabled && outcome == "outside-sources" {
            owners::count(
                &mut frame.outside,
                id,
                bytes,
                mesh.indices().is_some(),
                self.owners.get(&id).cloned(),
            );
        }
    }

    pub(super) fn commit(&self, frame: Option<Frame>) {
        if let Some(frame) = frame {
            self.counters.commit(frame);
        }
    }

    pub(super) fn record(&self, name: &'static str, start: Instant) {
        record(&self.samples, name, start.elapsed());
    }
}

pub fn install(render: &mut bevy::app::SubApp, counters: Counters, samples: Arc<Mutex<Samples>>) {
    let owners_enabled =
        std::env::var("GENERALS_PROFILE_MESH_OWNERS").is_ok_and(|value| value == "1");
    render
        .insert_resource(State {
            counters,
            kinds: BTreeMap::new(),
            owners: BTreeMap::new(),
            owners_enabled,
            samples,
        })
        .add_systems(ExtractSchedule, extract_kinds);
    if owners_enabled {
        render.add_systems(ExtractSchedule, owners::extract);
        eprintln!(
            "live-mesh-owners enabled=true env=GENERALS_PROFILE_MESH_OWNERS diagnostic_overhead=additional"
        );
    }
}

fn extract_kinds(
    mut animated: Extract<Query<&Mesh3d, With<crate::model_animation::AnimatedPart>>>,
    mut sorted: Extract<Query<&Mesh3d, With<crate::sorted_view::SortedBatch>>>,
    mut surfaces: Extract<Query<&Mesh3d, With<crate::terrain::changes::Surface>>>,
    mut water: Extract<Query<&Mesh3d, With<crate::water_view::WaterVisual>>>,
    mut state: ResMut<State>,
) {
    state.kinds.clear();
    for mesh in animated.iter_mut() {
        *state.kinds.entry(mesh.id()).or_default() |= 1;
    }
    for mesh in sorted.iter_mut() {
        *state.kinds.entry(mesh.id()).or_default() |= 2;
    }
    for mesh in surfaces.iter_mut() {
        *state.kinds.entry(mesh.id()).or_default() |= 4;
    }
    for mesh in water.iter_mut() {
        *state.kinds.entry(mesh.id()).or_default() |= 8;
    }
}

pub(super) fn incompatible_reason(
    mesh: &Mesh,
    previous: &RenderMesh,
    layouts: &mut MeshVertexBufferLayouts,
) -> &'static str {
    // Same first-failure order as compatible(); evaluated only by the profiler.
    if !super::compatible_indices(mesh, previous) {
        "index-count-format-or-alignment"
    } else if mesh.has_morph_targets() || previous.has_morph_targets() {
        "morph-targets"
    } else if mesh.primitive_topology() != previous.primitive_topology() {
        "topology-changed"
    } else if mesh.count_vertices() != usize::try_from(previous.vertex_count).expect("vertex count")
    {
        "vertex-count-changed"
    } else if mesh.get_vertex_buffer_size() == 0 {
        "empty"
    } else if !mesh.get_vertex_buffer_size().is_multiple_of(4) {
        "unaligned"
    } else {
        assert_ne!(mesh.get_mesh_vertex_buffer_layout(layouts), previous.layout);
        "layout-changed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_upload::compatible;

    fn frame(counters: &Counters, count: u64, bytes: u64) -> Frame {
        let mut frame = counters.begin().expect("active sample window");
        frame
            .counts
            .insert(("sorted-batch", "retained"), (count, bytes));
        frame
    }

    #[test]
    fn excludes_startup_closed_and_interrupted_windows() {
        let counters = Counters::new();
        assert!(counters.begin().is_none());
        counters.reset(true);
        let stale = frame(&counters, 9, 432);
        counters.commit(frame(&counters, 2, 96));
        counters.reset(false);
        assert!(counters.begin().is_none());
        counters.reset(true);
        counters.commit(stale);
        counters.commit(frame(&counters, 3, 144));
        let late = frame(&counters, 7, 336);
        counters.finish();
        counters.commit(late);
        assert!(counters.begin().is_none());
        let window = counters.0.lock().unwrap();
        assert_eq!(window.frames, 1);
        assert_eq!(window.counts.len(), 1);
        assert_eq!(window.counts[&("sorted-batch", "retained")], (3, 144));
        drop(window);
    }

    #[test]
    fn counts_shared_ids_once_and_keeps_fallback_bytes_separate() {
        let counters = Counters::new();
        counters.reset(true);
        let mut meshes = Assets::<Mesh>::default();
        let mesh = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 3]);
        let id = meshes.add(mesh.clone()).id();
        let state = State {
            counters: counters.clone(),
            kinds: BTreeMap::from([(id, 3)]),
            owners: BTreeMap::new(),
            owners_enabled: false,
            samples: Arc::new(Mutex::new(BTreeMap::new())),
        };
        let mut frame = state.begin();
        state.count(&mut frame, id, &mesh, "retained");
        state.count(&mut frame, AssetId::invalid(), &mesh, "outside-sources");
        state.commit(frame);
        let window = counters.0.lock().unwrap();
        assert_eq!(window.frames, 1);
        assert_eq!(window.counts.len(), 2);
        assert!(window.outside.is_empty());
        assert_eq!(
            window.counts[&("shared-animated-sorted", "retained")],
            (1, 36)
        );
        assert_eq!(
            window.counts[&("other-or-unreferenced", "outside-sources")],
            (1, 36)
        );
        drop(window);
    }
    #[test]
    fn incompatible_reasons_follow_the_retention_first_failure_order() {
        use bevy::{
            asset::RenderAssetUsages,
            mesh::{BaseMeshPipelineKey, Indices},
            render::{mesh::RenderMeshBufferInfo, render_resource::PrimitiveTopology},
        };
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 3]);
        let mut layouts = MeshVertexBufferLayouts::default();
        let mut previous = RenderMesh {
            vertex_count: 3,
            aabb_center: Vec3::ZERO,
            buffer_info: RenderMeshBufferInfo::NonIndexed,
            key_bits: BaseMeshPipelineKey::from_primitive_topology_and_strip_index(
                PrimitiveTopology::TriangleList,
                None,
            ),
            layout: mesh.get_mesh_vertex_buffer_layout(&mut layouts),
        };
        assert!(compatible(&mesh, &previous, &mut layouts));
        previous.vertex_count = 6;
        assert!(!compatible(&mesh, &previous, &mut layouts));
        assert_eq!(
            incompatible_reason(&mesh, &previous, &mut layouts),
            "vertex-count-changed"
        );
        let indexed = mesh
            .clone()
            .with_inserted_indices(Indices::U32(vec![0, 1, 2]));
        assert_eq!(
            incompatible_reason(&indexed, &previous, &mut layouts),
            "index-count-format-or-alignment"
        );
        previous.vertex_count = 3;
        let changed_layout =
            mesh.with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32; 3]; 3]);
        assert!(!compatible(&changed_layout, &previous, &mut layouts));
        assert_eq!(
            incompatible_reason(&changed_layout, &previous, &mut layouts),
            "layout-changed"
        );
    }
}
