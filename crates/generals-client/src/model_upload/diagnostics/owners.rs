//! Main-world ownership and byte totals for ordinary allocator uploads.
use super::State;
use crate::{source_material::SourceMaterial, terrain::changes::Surface};
use bevy::{
    prelude::*,
    render::{Extract, MainWorld},
};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

pub(super) type Snapshots = BTreeMap<AssetId<Mesh>, Arc<Snapshot>>;
pub(super) type Counts = BTreeMap<AssetId<Mesh>, Payload>;

pub(super) struct Snapshot {
    references: u64,
    examples: Vec<Owner>,
}

struct Owner {
    entity: Entity,
    components: Arc<str>,
    surface_index: Option<usize>,
    source_alpha_mode: Option<AlphaMode>,
    source_draw_alpha_mode: Option<AlphaMode>,
}

pub(super) struct Payload {
    updates: u64,
    vertex_bytes: u64,
    max_vertex_bytes: u64,
    indexed_updates: u64,
    unowned_updates: u64,
    ownership: Option<Arc<Snapshot>>,
}

type OutsideOwner = (
    With<Mesh3d>,
    Without<crate::model_animation::AnimatedPart>,
    Without<crate::sorted_view::SortedBatch>,
);

pub(super) fn extract(
    main: Res<MainWorld>,
    mut entities: Extract<Query<EntityRef, OutsideOwner>>,
    mut state: ResMut<State>,
    mut signatures: Local<BTreeMap<usize, Arc<str>>>,
) {
    let main = main.into_inner();
    state.owners.clear();
    if !state.counters.collecting() {
        return;
    }
    let started = Instant::now();
    let mut snapshots: BTreeMap<AssetId<Mesh>, Snapshot> = BTreeMap::new();
    for entity in entities.iter_mut() {
        let id = entity.get::<Mesh3d>().expect("queried mesh owner").id();
        let snapshot = snapshots.entry(id).or_insert_with(|| Snapshot {
            references: 0,
            examples: Vec::new(),
        });
        snapshot.references += 1;
        let archetype = entity.archetype();
        let components = signatures.entry(archetype.id().index()).or_insert_with(|| {
            let mut names: Vec<_> = archetype
                .components()
                .iter()
                .map(|&component| {
                    main.components()
                        .get_info(component)
                        .expect("main-world component metadata")
                        .name()
                        .to_string()
                })
                .collect();
            names.sort_unstable();
            Arc::from(names.join(","))
        });
        let surface_index = entity.get::<Surface>().map(|surface| surface.0);
        let material = entity
            .get::<MeshMaterial3d<SourceMaterial>>()
            .and_then(|handle| {
                main.get_resource::<Assets<SourceMaterial>>()
                    .and_then(|materials| materials.get(&handle.0))
            });
        let source_alpha_mode = material.map(|material| material.alpha_mode);
        let source_draw_alpha_mode = material.map(Material::alpha_mode);
        if snapshot.examples.len() < 3
            && !snapshot.examples.iter().any(|owner| {
                Arc::ptr_eq(&owner.components, components)
                    && owner.surface_index == surface_index
                    && owner.source_alpha_mode == source_alpha_mode
                    && owner.source_draw_alpha_mode == source_draw_alpha_mode
            })
        {
            snapshot.examples.push(Owner {
                entity: entity.id(),
                components: Arc::clone(components),
                surface_index,
                source_alpha_mode,
                source_draw_alpha_mode,
            });
        }
    }
    state.owners.extend(
        snapshots
            .into_iter()
            .map(|(id, snapshot)| (id, Arc::new(snapshot))),
    );
    state.record("mesh-outside-owner-extract", started);
}

pub(super) fn count(
    counts: &mut Counts,
    id: AssetId<Mesh>,
    vertex_bytes: u64,
    indexed: bool,
    ownership: Option<Arc<Snapshot>>,
) {
    let total = counts.entry(id).or_insert_with(|| Payload {
        updates: 0,
        vertex_bytes: 0,
        max_vertex_bytes: 0,
        indexed_updates: 0,
        unowned_updates: 0,
        ownership: None,
    });
    total.updates += 1;
    total.vertex_bytes += vertex_bytes;
    total.max_vertex_bytes = total.max_vertex_bytes.max(vertex_bytes);
    total.indexed_updates += u64::from(indexed);
    total.unowned_updates += u64::from(ownership.is_none());
    if ownership.is_some() {
        total.ownership = ownership;
    }
}

pub(super) fn merge(totals: &mut Counts, frame: Counts) {
    for (id, payload) in frame {
        let Some(total) = totals.get_mut(&id) else {
            totals.insert(id, payload);
            continue;
        };
        total.updates += payload.updates;
        total.vertex_bytes += payload.vertex_bytes;
        total.max_vertex_bytes = total.max_vertex_bytes.max(payload.max_vertex_bytes);
        total.indexed_updates += payload.indexed_updates;
        total.unowned_updates += payload.unowned_updates;
        if payload.ownership.is_some() {
            total.ownership = payload.ownership;
        }
    }
}

pub(super) fn print(counts: &Counts) {
    let mut ranked: Vec<_> = counts.iter().collect();
    ranked.sort_unstable_by(|(left_id, left), (right_id, right)| {
        right
            .vertex_bytes
            .cmp(&left.vertex_bytes)
            .then_with(|| left_id.cmp(right_id))
    });
    eprintln!(
        "live-mesh-upload-outside distinct_meshes={} printed={} ranking=total_vertex_bytes owner_snapshot=latest_observed_in_sample_window owner_examples=up_to_3_distinct_component_surface_material_signatures",
        counts.len(),
        ranked.len().min(20)
    );
    for (index, (id, payload)) in ranked.into_iter().take(20).enumerate() {
        let rank = index + 1;
        let references = payload
            .ownership
            .as_ref()
            .map_or(0, |owner| owner.references);
        eprintln!(
            "live-mesh-upload-top rank={rank} mesh_id={id:?} updates={} vertex_bytes={} max_vertex_bytes={} indexed_updates={} no_owner_updates={} owner_references={references}",
            payload.updates,
            payload.vertex_bytes,
            payload.max_vertex_bytes,
            payload.indexed_updates,
            payload.unowned_updates,
        );
        if let Some(ownership) = &payload.ownership {
            for owner in &ownership.examples {
                eprintln!(
                    "live-mesh-upload-owner mesh_id={id:?} entity={:?} surface_index={:?} source_alpha_mode={:?} source_draw_alpha_mode={:?} components={}",
                    owner.entity,
                    owner.surface_index,
                    owner.source_alpha_mode,
                    owner.source_draw_alpha_mode,
                    owner.components
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_upload::diagnostics::Counters;
    use std::sync::Mutex;

    #[test]
    fn main_world_owner_signatures_survive_later_unowned_uploads() {
        let counters = Counters::new();
        counters.reset(true);
        let mut world = World::new();
        world.init_resource::<MainWorld>();
        world.insert_resource(State {
            counters,
            kinds: BTreeMap::new(),
            owners: BTreeMap::new(),
            owners_enabled: true,
            samples: Arc::new(Mutex::new(BTreeMap::new())),
        });
        let handle = Handle::<Mesh>::default();
        let (surface, road) = {
            let mut main = world.resource_mut::<MainWorld>();
            let mut materials = Assets::<SourceMaterial>::default();
            let material = materials.add(SourceMaterial::new(
                [0.0; 3],
                [1.0; 3],
                [0.0; 3],
                1.0,
                AlphaMode::Opaque,
                false,
            ));
            main.insert_resource(materials);
            let surface = main
                .spawn((Mesh3d(handle.clone()), Surface(7), MeshMaterial3d(material)))
                .id();
            let road = main
                .spawn((Mesh3d(handle.clone()), crate::terrain::changes::Road))
                .id();
            (surface, road)
        };
        let mut schedule = Schedule::new(bevy::render::ExtractSchedule);
        schedule.add_systems(extract);
        world.add_schedule(schedule);
        world.run_schedule(bevy::render::ExtractSchedule);
        let ownership = Arc::clone(&world.resource::<State>().owners[&handle.id()]);
        assert_eq!(ownership.references, 2);
        assert_eq!(ownership.examples.len(), 2);
        let surface_owner = ownership
            .examples
            .iter()
            .find(|owner| owner.entity == surface)
            .unwrap();
        assert_eq!(surface_owner.surface_index, Some(7));
        assert_eq!(surface_owner.source_alpha_mode, Some(AlphaMode::Opaque));
        assert_eq!(
            surface_owner.source_draw_alpha_mode,
            Some(AlphaMode::Opaque)
        );
        let road_owner = ownership
            .examples
            .iter()
            .find(|owner| owner.entity == road)
            .unwrap();
        assert_eq!(road_owner.surface_index, None);
        assert_eq!(road_owner.source_alpha_mode, None);
        assert!(
            surface_owner
                .components
                .contains("terrain::changes::Surface")
        );
        assert!(road_owner.components.contains("terrain::changes::Road"));
        let mut totals = Counts::new();
        count(&mut totals, handle.id(), 144, true, Some(ownership));
        {
            let mut main = world.resource_mut::<MainWorld>();
            main.despawn(surface);
            main.despawn(road);
        }
        world.run_schedule(bevy::render::ExtractSchedule);
        assert!(world.resource::<State>().owners.is_empty());
        let mut later = Counts::new();
        count(&mut later, handle.id(), 288, false, None);
        merge(&mut totals, later);
        let total = &totals[&handle.id()];
        assert_eq!(
            (total.updates, total.vertex_bytes, total.max_vertex_bytes),
            (2, 432, 288)
        );
        assert_eq!((total.indexed_updates, total.unowned_updates), (1, 1));
        assert_eq!(total.ownership.as_ref().unwrap().references, 2);
    }
}
