use super::*;
use bevy::{asset::AssetPlugin, ecs::system::RunSystemOnce};

#[test]
fn visibility_only_pose_updates_without_mesh_modified_event() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>();
    let binding = Arc::new(model_render::PartBinding {
        pass: 0,
        mesh: 0,
        vertices: vec![0, 1, 2],
    });
    let mut pose = vec![MeshPose {
        bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
        positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
        normals: None,
        visible: true,
    }];
    let mut mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; 3]);
    model_render::deform(&mut mesh, &binding, &pose).unwrap();
    let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    let parent = app.world_mut().spawn_empty().id();
    let child = app
        .world_mut()
        .spawn((
            AnimatedPart { model: 0, binding },
            Mesh3d(handle),
            ChildOf(parent),
            Visibility::Inherited,
            Transform::IDENTITY,
        ))
        .id();
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<Mesh>>>()
        .clear();
    pose[0].visible = false;
    let poses = BTreeMap::from([(
        (parent, 0),
        Frame {
            supply: None,
            meshes: Some(pose),
            height_offset: -20.0,
        },
    )]);
    app.world_mut()
        .run_system_once(
            move |mut meshes: ResMut<Assets<Mesh>>, mut parts: Query<AnimatedMesh>| {
                apply_poses(&mut meshes, &mut parts, &poses);
            },
        )
        .unwrap();
    app.update();
    assert!(
        !app.world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .drain()
            .any(|event| matches!(event, AssetEvent::Modified { .. }))
    );
    assert_eq!(
        *app.world().get::<Visibility>(child).unwrap(),
        Visibility::Hidden
    );
    assert_eq!(
        app.world()
            .get::<Transform>(child)
            .unwrap()
            .translation
            .y
            .to_bits(),
        (-2.0_f32).to_bits()
    );
}

#[test]
fn repeated_pose_does_not_mark_mesh_modified_but_changed_pose_does() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>();
    let mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; 3]);
    let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    let parent = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((
        AnimatedPart {
            model: 0,
            binding: Arc::new(model_render::PartBinding {
                pass: 0,
                mesh: 0,
                vertices: vec![0, 1, 2],
            }),
        },
        Mesh3d(handle.clone()),
        ChildOf(parent),
        Visibility::Inherited,
        Transform::IDENTITY,
    ));
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<Mesh>>>()
        .clear();
    let unchanged = BTreeMap::from([(
        (parent, 0),
        Frame {
            supply: None,
            meshes: None,
            height_offset: -20.0,
        },
    )]);
    app.world_mut()
        .run_system_once(
            move |mut meshes: ResMut<Assets<Mesh>>, mut parts: Query<AnimatedMesh>| {
                apply_poses(&mut meshes, &mut parts, &unchanged);
            },
        )
        .unwrap();
    app.update();
    assert!(
        !app.world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .drain()
            .any(|event| matches!(event, AssetEvent::Modified { .. }))
    );
    let changed = BTreeMap::from([(
        (parent, 0),
        Frame {
            supply: None,
            meshes: Some(vec![MeshPose {
                bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
                positions: vec![[5.0; 3]; 3],
                normals: None,
                visible: true,
            }]),
            height_offset: -10.0,
        },
    )]);
    app.world_mut()
        .run_system_once(
            move |mut meshes: ResMut<Assets<Mesh>>, mut parts: Query<AnimatedMesh>| {
                apply_poses(&mut meshes, &mut parts, &changed);
            },
        )
        .unwrap();
    app.update();
    assert!(
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .drain()
            .any(|event| matches!(event, AssetEvent::Modified { id } if id == handle.id()))
    );
    let mut parts = app
        .world_mut()
        .query::<(&AnimatedPart, &Visibility, &Transform)>();
    let (_, visibility, transform) = parts.single(app.world()).unwrap();
    assert_eq!(*visibility, Visibility::Inherited);
    assert_eq!(transform.translation.y.to_bits(), (-1.0_f32).to_bits());
}
