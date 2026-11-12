//! Opt-in attribution of actual asset events, retained through visual retirement.
use bevy::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

pub(super) type Source<'a> = (
    &'a Mesh3d,
    Option<&'a ChildOf>,
    Option<&'a crate::model_animation::AnimatedPart>,
    Option<&'a crate::particles::ParticleVisual>,
    Option<&'a crate::laser_beam::StreamVisual>,
    Option<&'a crate::projectile_stream::ProjectileStreamVisual>,
    Option<&'a crate::laser_beam::PulseVisual>,
    Option<&'a crate::weapon_fx::TracerVisual>,
);

const NAMES: [&str; 8] = [
    "animated-model",
    "particle",
    "laser-stream",
    "projectile-stream",
    "laser-or-tracer",
    "projectile-model",
    "other-or-unreferenced",
    "shared-categories",
];

pub(super) struct Changes {
    categories: BTreeMap<AssetId<Mesh>, usize>,
    counts: [usize; 8],
    payloads: Option<ModelPayloads>,
}

type ModelPart = (usize, usize, usize, Vec<usize>);

struct ModelPayloads {
    parts: BTreeMap<AssetId<Mesh>, ModelPart>,
    frame: BTreeMap<(ModelPart, u64), Vec<Vec<u8>>>,
    seen: BTreeSet<AssetId<Mesh>>,
    meshes: usize,
    bytes: usize,
    duplicate_meshes: usize,
    duplicate_bytes: usize,
}

impl ModelPayloads {
    const fn new() -> Self {
        Self {
            parts: BTreeMap::new(),
            frame: BTreeMap::new(),
            seen: BTreeSet::new(),
            meshes: 0,
            bytes: 0,
            duplicate_meshes: 0,
            duplicate_bytes: 0,
        }
    }

    fn record(&mut self, id: AssetId<Mesh>, meshes: &Assets<Mesh>) {
        if !self.seen.insert(id) {
            return;
        }
        let Some(part) = self.parts.get(&id) else {
            return;
        };
        let Some(mesh) = meshes.get(id) else {
            return;
        };
        let bytes = mesh.create_packed_vertex_buffer_data();
        self.meshes += 1;
        self.bytes += bytes.len();
        let mut hash = DefaultHasher::new();
        bytes.hash(&mut hash);
        let candidates = self.frame.entry((part.clone(), hash.finish())).or_default();
        // Hash collisions never establish equivalence. Source-part identity
        // fixes topology and attribute layout; compare every uploaded byte.
        if candidates.contains(&bytes) {
            self.duplicate_meshes += 1;
            self.duplicate_bytes += bytes.len();
        } else {
            candidates.push(bytes);
        }
    }
}

impl Changes {
    pub(super) fn new() -> Self {
        Self {
            categories: BTreeMap::new(),
            counts: [0; 8],
            // Diagnostic packing adds work; do not enable this for timing
            // comparisons. It measures reuse opportunity, not elapsed speed.
            payloads: std::env::var_os("GENERALS_PROFILE_MODEL_PAYLOADS")
                .map(|_| ModelPayloads::new()),
        }
    }

    pub(super) fn reset_counts(&mut self) {
        self.counts = [0; 8];
        if let Some(payloads) = &mut self.payloads {
            payloads.frame.clear();
            payloads.seen.clear();
            payloads.meshes = 0;
            payloads.bytes = 0;
            payloads.duplicate_meshes = 0;
            payloads.duplicate_bytes = 0;
        }
    }

    pub(super) fn begin_frame(&mut self) {
        if let Some(payloads) = &mut self.payloads {
            payloads.frame.clear();
            payloads.seen.clear();
        }
    }

    pub(super) fn observe(&mut self, source: Source<'_>, projectile_parent: bool) {
        let mesh = source.0;
        if let Some(payloads) = &mut self.payloads
            && let Some(part) = source.2
        {
            payloads.parts.insert(
                mesh.id(),
                (
                    part.model,
                    part.binding.mesh,
                    part.binding.pass,
                    part.binding.vertices.clone(),
                ),
            );
        }
        let category = category(source, projectile_parent);
        let known = self.categories.entry(mesh.id()).or_insert(category);
        // The friendly heat overlay shares its source mesh but has no source
        // marker. Its unclassified reference must not replace that ownership.
        if *known == 6 {
            *known = category;
        } else if category != 6 && *known != category {
            *known = 7;
        }
    }

    pub(super) fn event(
        &mut self,
        event: &AssetEvent<Mesh>,
        sample: bool,
        meshes: &Assets<Mesh>,
    ) -> bool {
        match *event {
            AssetEvent::Modified { id } => {
                if sample {
                    self.counts[self.categories.get(&id).copied().unwrap_or(6)] += 1;
                    if let Some(payloads) = &mut self.payloads {
                        payloads.record(id, meshes);
                    }
                }
                true
            }
            AssetEvent::Removed { id } => {
                self.categories.remove(&id);
                if let Some(payloads) = &mut self.payloads {
                    payloads.parts.remove(&id);
                }
                false
            }
            _ => false,
        }
    }

    pub(super) fn print(&self) {
        for (name, count) in NAMES.into_iter().zip(self.counts) {
            eprintln!("live-mesh-modified category={name} events={count}");
        }
        if let Some(payloads) = &self.payloads {
            eprintln!(
                "live-model-payloads diagnostic_only=true meshes={} bytes={} identical_same_frame={} duplicate_bytes={}",
                payloads.meshes,
                payloads.bytes,
                payloads.duplicate_meshes,
                payloads.duplicate_bytes
            );
        }
    }
}

const fn category(source: Source<'_>, projectile_parent: bool) -> usize {
    let (_, _, model, particle, laser_stream, projectile_stream, pulse, tracer) = source;
    if projectile_parent {
        5
    } else if model.is_some() {
        0
    } else if particle.is_some() {
        1
    } else if laser_stream.is_some() {
        2
    } else if projectile_stream.is_some() {
        3
    } else if pulse.is_some() || tracer.is_some() {
        4
    } else {
        6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_payloads_require_distinct_handles_exact_bytes_and_same_source_part() {
        let mut meshes = Assets::<Mesh>::default();
        let source = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 3]);
        let first = meshes.add(source.clone());
        let same = meshes.add(source.clone());
        let other_part = meshes.add(source.clone());
        let signed_zero = meshes.add(
            source.with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-0.0_f32, 0.0, 0.0]; 3]),
        );
        let mut payloads = ModelPayloads::new();
        for handle in [&first, &same, &signed_zero] {
            payloads.parts.insert(handle.id(), (0, 1, 0, vec![0, 1, 2]));
        }
        payloads
            .parts
            .insert(other_part.id(), (0, 2, 0, vec![0, 1, 2]));
        for handle in [&first, &first, &same, &signed_zero, &other_part] {
            payloads.record(handle.id(), &meshes);
        }
        assert_eq!(payloads.meshes, 4);
        assert_eq!(payloads.duplicate_meshes, 1);
        assert_eq!(payloads.bytes, 4 * 36);
        assert_eq!(payloads.duplicate_bytes, 36);
        payloads.frame.clear();
        payloads.seen.clear();
        payloads.record(first.id(), &meshes);
        assert_eq!(payloads.meshes, 5);
        assert_eq!(payloads.duplicate_meshes, 1);
    }

    #[test]
    fn retired_particle_events_keep_ownership_and_shared_handles_count_once() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let particle = meshes.add(Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        ));
        let visual = world
            .spawn((
                Mesh3d(particle.clone()),
                crate::particles::ParticleVisual {
                    system: 1,
                    generation: 1,
                    bounded: false,
                },
            ))
            .id();
        let overlay = world.spawn(Mesh3d(particle.clone())).id();
        let mut changes = Changes::new();
        let mut query = world.query::<Source>();
        changes.observe(query.get(&world, visual).unwrap(), false);
        changes.observe(query.get(&world, overlay).unwrap(), false);
        let modified = AssetEvent::Modified { id: particle.id() };
        assert!(changes.event(&modified, false, &meshes));
        assert_eq!(changes.counts, [0; 8]);
        world.despawn(visual);
        world.despawn(overlay);
        assert!(changes.event(&modified, true, &meshes));
        assert_eq!(changes.counts[1], 1);
        assert_eq!(changes.counts.iter().sum::<usize>(), 1);
        changes.reset_counts();
        assert!(changes.categories.contains_key(&particle.id()));
        assert!(!changes.event(&AssetEvent::Removed { id: particle.id() }, true, &meshes));
        assert!(!changes.categories.contains_key(&particle.id()));
    }
}
