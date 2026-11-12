//! Native terrain full-update, scorch invalidation and road-lighting consumers.
use super::Terrain;
use crate::playtest::Match;
use bevy::{
    camera::primitives::{Aabb, MeshAabb},
    mesh::VertexAttributeValues,
    prelude::*,
};
use num_traits::ToPrimitive;

// Batch order follows immutable opaque tile/class keys and blended layer/class indices.
#[derive(Component)]
pub struct Surface(pub usize);

#[derive(Component)]
pub struct Road;

pub fn update(
    battle: Res<Match>,
    mut terrain: ResMut<Terrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut surfaces: Query<(&Surface, &Mesh3d, Option<&mut Aabb>)>,
    roads: Query<&Mesh3d, With<Road>>,
    mut applied: Local<Option<(u32, u64)>>,
) {
    let battle = battle.into_inner();
    let revision = (
        battle.generation,
        battle.simulation.terrain_height_revision(),
    );
    if *applied == Some(revision) {
        return;
    }
    let Some(heights) = battle.simulation.terrain_height_map() else {
        return;
    };
    terrain.apply_height_map(&heights);
    let mut batches: Vec<_> = terrain
        .batches()
        .into_iter()
        .map(|batch| Some(batch.mesh))
        .collect();
    for (surface, handle, bounds) in &mut surfaces {
        let mesh = batches[surface.0].take().expect("unique terrain batch");
        if let Some(mut bounds) = bounds {
            *bounds = mesh.compute_aabb().expect("nonempty terrain batch");
        }
        *meshes.get_mut(&handle.0).expect("prepared terrain mesh") = mesh;
    }
    // W3DRoadBuffer::updateSegLighting updates shading at existing road vertices;
    // flattenTerrain does not rebuild the road's floating geometry.
    for handle in &roads {
        let mut mesh = meshes.get_mut(&handle.0).expect("prepared road mesh");
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("native road vertex positions");
        };
        let normals: Vec<_> = positions
            .iter()
            .map(|point| {
                terrain
                    .road_vertex_normal(Vec2::new(point[0] * 10.0, -point[2] * 10.0))
                    .to_array()
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    *applied = Some(revision);
}

impl Terrain {
    fn apply_height_map(&mut self, map: &generals_sim::skirmish::TerrainHeights) {
        assert_eq!([self.raw_width, self.raw_height], [map.width, map.height]);
        assert_eq!(self.heights.len(), map.samples.len());
        for (height, &sample) in self.heights.iter_mut().zip(&map.samples) {
            *height = (sample / 0.625).to_u16().expect("native raw terrain byte");
        }
    }
}

#[cfg(test)]
mod tests;
