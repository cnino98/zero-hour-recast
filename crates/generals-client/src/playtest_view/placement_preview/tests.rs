use super::*;
use crate::playtest_view::PreparedPart;
use bevy::{
    asset::{AssetApp, AssetPlugin, RenderAssetUsages},
    render::render_resource::PrimitiveTopology,
};

#[derive(Resource)]
struct Request {
    model: Option<(u32, usize)>,
    pose: Option<(Transform, bool)>,
}

fn drive(
    mut commands: Commands,
    mut state: ResMut<Previews>,
    request: Res<Request>,
    geometry: Res<Geometry>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    mut previews: PreviewQuery,
) {
    let geometry = geometry.into_inner();
    let request = request.into_inner();
    state.reconcile(
        request.model,
        request.pose,
        &mut commands,
        geometry,
        &mut materials,
    );
    state.display(request.pose, &mut previews);
}

fn fixture() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        bevy::mesh::MeshPlugin,
        bevy::camera::visibility::VisibilityPlugin,
    ))
    .init_asset::<Image>()
    .init_asset::<SourceMaterial>()
    .insert_resource(Previews::new(0.45))
    .insert_resource(Request {
        model: None,
        pose: None,
    })
    .add_systems(Update, drive);
    let mut source =
        SourceMaterial::new([0.25; 3], [0.5; 3], [0.1; 3], 0.75, AlphaMode::Opaque, true);
    source.shroud = Some(
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default()),
    );
    source.parameters.shroud_extent = Vec4::new(25.0, 35.0, 0.0, 0.0);
    let material = app
        .world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .add(source);
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0.0_f32; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        ),
    );
    let models = (0..2)
        .map(|_| {
            vec![PreparedPart {
                light_bounds: None,
                submission: None,
                order: crate::model_render::RenderOrder::ordinary(),
                binding: None,
                pickable: false,
                mesh: mesh.clone(),
                materials: vec![material.clone(), material.clone()],
            }]
        })
        .collect();
    app.insert_resource(Geometry {
        models,
        colors: vec![[0; 3]; 2],
        generation: u32::MAX,
        spawned: 0,
    });
    app
}

fn roots(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<Placement>>()
        .iter(app.world())
        .collect()
}

fn request(app: &mut App, model: Option<(u32, usize)>, pose: Option<(Transform, bool)>) {
    let mut request = app.world_mut().resource_mut::<Request>();
    request.model = model;
    request.pose = pose;
}

#[test]
fn idle_has_no_previews_and_cancel_type_change_and_restart_release_only_temporary_assets() {
    let mut app = fixture();
    app.update();
    assert!(roots(&mut app).is_empty());
    assert_eq!(app.world().resource::<Assets<SourceMaterial>>().len(), 1);
    for (generation, model) in [(0, 0), (0, 1), (1, 1)] {
        let previous = roots(&mut app);
        request(&mut app, Some((generation, model)), None);
        app.update();
        let current = roots(&mut app);
        assert_eq!(current.len(), 2);
        assert!(previous.iter().all(|root| !current.contains(root)));
        assert_eq!(app.world().resource::<Assets<SourceMaterial>>().len(), 3);
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Mesh3d>>()
                .iter(app.world())
                .count(),
            2
        );
    }
    let retired: Vec<_> = app
        .world()
        .resource::<Previews>()
        .active
        .as_ref()
        .unwrap()
        .materials
        .iter()
        .map(Handle::id)
        .collect();
    request(&mut app, None, None);
    app.update();
    assert!(roots(&mut app).is_empty());
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<Mesh3d>>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(app.world().resource::<Assets<SourceMaterial>>().len(), 1);
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
    // Handle retirement happens in the next PreUpdate. Render asset cleanup
    // consumes Unused, so removal alone is insufficient proof of GPU lifetime.
    app.update();
    let unused: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<AssetEvent<SourceMaterial>>>()
        .drain()
        .filter_map(|event| {
            if let AssetEvent::Unused { id } = event {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    assert!(retired.iter().all(|id| unused.contains(id)));
}

#[test]
fn same_building_reuses_pair_and_switches_exact_pose_legality_and_original_materials() {
    let mut app = fixture();
    let pose = Transform::from_xyz(10.0, 2.0, -30.0).with_rotation(Quat::from_rotation_y(0.7));
    request(&mut app, Some((0, 0)), Some((pose, true)));
    app.update();
    assert_initial_pose(&mut app, pose);
    let original_roots = roots(&mut app);
    let original_materials: Vec<_> = app
        .world()
        .resource::<Assets<SourceMaterial>>()
        .ids()
        .collect();
    for allowed in [true, false, false, true] {
        request(&mut app, Some((0, 0)), Some((pose, allowed)));
        app.update();
        assert_eq!(roots(&mut app), original_roots);
        assert_eq!(
            app.world()
                .resource::<Assets<SourceMaterial>>()
                .ids()
                .collect::<Vec<_>>(),
            original_materials
        );
        let mut query = app
            .world_mut()
            .query::<(&Placement, &Transform, &Visibility, &Children)>();
        for (preview, transform, visibility, children) in query.iter(app.world()) {
            assert_eq!(*transform, pose);
            assert_eq!(
                matches!(visibility, Visibility::Visible),
                preview.legal == allowed
            );
            let material = app
                .world()
                .get::<MeshMaterial3d<SourceMaterial>>(children[0])
                .unwrap();
            let material = app
                .world()
                .resource::<Assets<SourceMaterial>>()
                .get(&material.0)
                .unwrap();
            assert!(matches!(material.alpha_mode, AlphaMode::Blend));
            assert!(!material.depth_write);
            assert_eq!(material.parameters.opacity.to_bits(), 0.45_f32.to_bits());
            assert_eq!(material.parameters.diffuse.w.to_bits(), 0.75_f32.to_bits());
            let tint = if preview.legal { 0.0_f32 } else { 1.0 };
            assert_eq!(
                material.parameters.scene_ambient.x.to_bits(),
                tint.to_bits()
            );
            assert!(
                material
                    .parameters
                    .light_color
                    .iter()
                    .all(|light| light.x.to_bits() == tint.to_bits())
            );
        }
    }
    // A paused/temporarily unavailable projected pose only hides the active
    // drawable. It must not allocate again or reset its last world transform.
    request(&mut app, Some((0, 0)), None);
    app.update();
    assert_eq!(roots(&mut app), original_roots);
    let mut query = app
        .world_mut()
        .query::<(&Placement, &Transform, &Visibility)>();
    for (_, transform, visibility) in query.iter(app.world()) {
        assert_eq!(*transform, pose);
        assert_eq!(*visibility, Visibility::Hidden);
    }
}

fn assert_initial_pose(app: &mut App, pose: Transform) {
    let geometry = app.world().resource::<Geometry>();
    let source = app
        .world()
        .resource::<Assets<SourceMaterial>>()
        .get(&geometry.models[0][0].materials[1])
        .unwrap();
    let shroud = source.shroud.clone();
    let extent = source.parameters.shroud_extent;
    let mut query = app.world_mut().query::<(
        &Placement,
        &GlobalTransform,
        &InheritedVisibility,
        &Children,
    )>();
    let mut count = 0;
    for (preview, transform, visible, children) in query.iter(app.world()) {
        count += 1;
        assert_eq!(visible.get(), preview.legal);
        assert_eq!(
            transform.translation().to_array().map(f32::to_bits),
            pose.translation.to_array().map(f32::to_bits)
        );
        let child = children[0];
        assert_eq!(
            app.world().get::<InheritedVisibility>(child).unwrap().get(),
            preview.legal
        );
        assert_eq!(
            app.world()
                .get::<GlobalTransform>(child)
                .unwrap()
                .translation()
                .to_array()
                .map(f32::to_bits),
            pose.translation.to_array().map(f32::to_bits)
        );
        let material = app
            .world()
            .get::<MeshMaterial3d<SourceMaterial>>(child)
            .unwrap();
        let material = app
            .world()
            .resource::<Assets<SourceMaterial>>()
            .get(&material.0)
            .unwrap();
        assert_eq!(material.shroud, shroud);
        assert_eq!(material.parameters.shroud_extent, extent);
    }
    assert_eq!(count, 2);
}
