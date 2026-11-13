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
        // An empty source buffer submits nothing. Keep the last allocation hidden;
        // uploading zero vertices would free its GPU slabs before an empty copy.
        if data.index_count > 0
            && let Some(mesh) = mesh.take()
        {
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
    material.parameters.shroud_mode = 1;
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
        RenderLayers::layer(crate::source_material::terrain::SCORCH),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;

    #[test]
    #[ignore = "requires original game installation"]
    fn populated_scorch_clear_and_resume_never_uploads_empty_geometry() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let map = generals_assets::map::MapData::parse(
            &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
        )
        .unwrap();
        let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
        let mut particles = Particles::draw_order_fixture(&[]);
        particles.scorches.add([100.0, 100.0, 0.0], 16.0, 1);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<SourceMaterial>();
        particles.scorches.texture = Some(
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::default()),
        );
        app.insert_resource(battle)
            .insert_resource(terrain)
            .insert_resource(particles)
            .add_systems(Update, update);
        app.update();
        let entity = app
            .world_mut()
            .query::<EntityRef>()
            .iter(app.world())
            .find(EntityRef::contains::<Visual>)
            .unwrap()
            .id();
        let handle = app.world().get::<Visual>(entity).unwrap().mesh.clone();
        let before = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&handle)
            .unwrap()
            .clone();
        assert!(before.count_vertices() > 0);
        app.world_mut().resource_mut::<Particles>().scorches.clear();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        let retained = app.world().resource::<Assets<Mesh>>().get(&handle).unwrap();
        assert_eq!(retained.indices(), before.indices());
        assert_eq!(
            retained.attribute(Mesh::ATTRIBUTE_POSITION),
            before.attribute(Mesh::ATTRIBUTE_POSITION),
        );
        app.world_mut()
            .resource_mut::<Particles>()
            .scorches
            .add([200.0, 200.0, 0.0], 24.0, 2);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );
        assert_eq!(app.world().get::<Visual>(entity).unwrap().mesh, handle);
        let resumed = app.world().resource::<Assets<Mesh>>().get(&handle).unwrap();
        assert!(resumed.count_vertices() > 0);
        assert_ne!(
            resumed.attribute(Mesh::ATTRIBUTE_POSITION),
            before.attribute(Mesh::ATTRIBUTE_POSITION),
        );
    }
}
