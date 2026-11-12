//! Rendered original route evidence for native controls checks.
use super::{Display, SourceMaterial};
use bevy::{ecs::system::SystemParam, prelude::*};

#[derive(SystemParam)]
pub struct RouteInspection<'w, 's> {
    display: Res<'w, Display>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    children: Query<'w, 's, &'static Children>,
    parts: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static ViewVisibility,
        ),
    >,
}

impl RouteInspection<'_, '_> {
    pub fn revealed(&self) -> bool {
        let Some(material) = self.materials.get(&self.display.revealed_material) else {
            return false;
        };
        if material.parameters.emissive.to_array().map(f32::to_bits)
            != [0.95_f32, 0.5, 0.0, 0.0].map(f32::to_bits)
            || material.texture.is_none()
        {
            return false;
        }
        let line = self.display.lines.iter().any(|(entity, _)| {
            self.parts
                .get(*entity)
                .is_ok_and(|(mesh, material, visible)| {
                    visible.get()
                        && material.0 == self.display.revealed_material
                        && self
                            .meshes
                            .get(&mesh.0)
                            .is_some_and(|mesh| mesh.count_vertices() >= 4)
                })
        });
        let node = self.display.nodes.iter().any(|entity| {
            self.children.get(*entity).is_ok_and(|children| {
                children.iter().any(|child| {
                    self.parts
                        .get(child)
                        .is_ok_and(|(mesh, material, visible)| {
                            visible.get()
                                && self
                                    .display
                                    .node_parts
                                    .iter()
                                    .any(|(_, expected)| *expected == material.0)
                                && self
                                    .meshes
                                    .get(&mesh.0)
                                    .is_some_and(|mesh| mesh.count_vertices() >= 3)
                        })
                })
            })
        });
        line && node
    }
}
