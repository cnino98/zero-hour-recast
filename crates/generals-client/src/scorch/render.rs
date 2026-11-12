use super::Scorches;
use crate::{
    particles::Particles, playtest::Match, source_material::SourceMaterial, terrain::Terrain,
};
use bevy::{
    camera::{
        primitives::{Aabb, MeshAabb},
        visibility::RenderLayers,
    },
    ecs::system::SystemParam,
    prelude::*,
};

#[derive(Component)]
pub struct Visual {
    pub(super) mesh: Handle<Mesh>,
}

#[derive(SystemParam)]
pub struct Art<'w> {
    images: ResMut<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
}

pub fn update(
    mut commands: Commands,
    mut particles: ResMut<Particles>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut art: Art,
    mut visuals: Query<(&Visual, &mut Visibility, &mut Aabb)>,
) {
    let battle = battle.into_inner();
    let terrain_changed = terrain.is_changed();
    let terrain = terrain.into_inner();
    let data = &mut particles.scorches;
    if let Some(image) = data.image.take() {
        data.texture = Some(art.images.add(image));
    }
    let mut mesh = (data.marks.dirty || terrain_changed)
        .then(|| terrain.scorch_mesh(&data.marks.entries, data.color));
    data.marks.dirty = false;
    if let Some(mesh) = &mesh {
        data.index_count = mesh.indices().map_or(0, bevy::mesh::Indices::len);
    }
    let visible = battle.started && data.index_count > 0;
    if visuals.is_empty() {
        if let Some(mesh) = mesh.take()
            && data.index_count > 0
        {
            spawn(&mut commands, data, &mut art, mesh, visible);
        }
        return;
    }
    for (visual, mut visibility, mut bounds) in &mut visuals {
        visibility.set_if_neq(if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        if let Some(mesh) = mesh.take() {
            if let Some(new_bounds) = mesh.compute_aabb() {
                *bounds = new_bounds;
            }
            if let Some(mut current) = art.meshes.get_mut(&visual.mesh) {
                *current = mesh;
            }
        }
    }
}

fn spawn(commands: &mut Commands, data: &Scorches, art: &mut Art, mesh: Mesh, visible: bool) {
    let Some(texture) = &data.texture else {
        return;
    };
    let Some(bounds) = mesh.compute_aabb() else {
        return;
    };
    let mesh = art.meshes.add(mesh);
    let mut material =
        SourceMaterial::new([0.0; 3], [0.0; 3], [0.0; 3], 1.0, AlphaMode::Blend, false);
    material.parameters.vertex_diffuse = 4;
    material.texture = Some(texture.clone());
    commands.spawn((
        Visual { mesh: mesh.clone() },
        Mesh3d(mesh),
        MeshMaterial3d(art.materials.add(material)),
        Transform::IDENTITY,
        bounds,
        if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
        RenderLayers::layer(0),
    ));
}
