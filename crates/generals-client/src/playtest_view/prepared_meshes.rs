//! Upload a prepared original model only when an actual drawable uses it.
use bevy::{asset::RenderAssetUsages, prelude::*};

pub fn upload_used(
    instances: Query<&Mesh3d, (Changed<Mesh3d>, Without<crate::particles::ParticleVisual>)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for instance in &instances {
        if meshes
            .get(&instance.0)
            .is_some_and(|mesh| !mesh.asset_usage.contains(RenderAssetUsages::RENDER_WORLD))
        {
            meshes
                .get_mut(&instance.0)
                .expect("existing drawable mesh")
                .asset_usage
                .insert(RenderAssetUsages::RENDER_WORLD);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::{AssetEventSystems, AssetPlugin},
        render::render_resource::PrimitiveTopology,
    };

    #[test]
    fn unused_model_states_stay_cpu_only_and_first_drawable_uploads_exact_mesh() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .add_systems(PostUpdate, upload_used.before(AssetEventSystems));
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[1.0, -0.0, 3.0]; 3]);
        let bytes = mesh.create_packed_vertex_buffer_data();
        let handles: Vec<_> = (0..4_000)
            .map(|_| {
                app.world_mut()
                    .resource_mut::<Assets<Mesh>>()
                    .add(mesh.clone())
            })
            .collect();
        app.update();
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .iter()
                .all(|(_, mesh)| { mesh.asset_usage == RenderAssetUsages::MAIN_WORLD })
        );
        let actor = app.world_mut().spawn(Mesh3d(handles[9].clone())).id();
        app.update();
        let modified: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .drain()
            .filter_map(|event| match event {
                AssetEvent::Modified { id } => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(modified, [handles[9].id()]);
        let assets = app.world().resource::<Assets<Mesh>>();
        assert_eq!(
            assets.get(&handles[9]).unwrap().asset_usage,
            RenderAssetUsages::default()
        );
        assert_eq!(
            assets
                .get(&handles[9])
                .unwrap()
                .create_packed_vertex_buffer_data(),
            bytes
        );
        assert_eq!(
            assets.get(&handles[10]).unwrap().asset_usage,
            RenderAssetUsages::MAIN_WORLD
        );
        app.update();
        assert!(
            !app.world_mut()
                .resource_mut::<Messages<AssetEvent<Mesh>>>()
                .drain()
                .any(|event| matches!(event, AssetEvent::Modified { .. }))
        );
        // A model swap on an existing drawable must upload the replacement too.
        app.world_mut()
            .entity_mut(actor)
            .insert(Mesh3d(handles[10].clone()));
        app.update();
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&handles[10])
                .unwrap()
                .asset_usage
                .contains(RenderAssetUsages::RENDER_WORLD)
        );
    }
}
