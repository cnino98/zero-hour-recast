//! The old unconditional paint/copy is the byte oracle; actual apply owns event gating.
use super::{MeshState, Parameters, Region, Resources, SourceMaterial, TerrainState, illuminate};
use bevy::{
    asset::{AssetPlugin, RenderAssetUsages},
    ecs::system::SystemState,
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use num_traits::ToPrimitive;
use std::collections::BTreeSet;

fn mesh(x: f32) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [x, 0.0, 0.0],
            [x + 1.0, 0.0, 0.0],
            [x + 1.0, 0.0, -1.0],
            [x, 0.0, -1.0],
        ],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[1.0, 0.75, 0.5, 127.0 / 255.0]; 4],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

fn cached(mesh: &Mesh, parameters: &Parameters) -> MeshState {
    let Some(VertexAttributeValues::Float32x4(original)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("colors");
    };
    MeshState {
        revision: (0, 0),
        original: original.clone(),
        baseline: Vec::new(),
        parameters: parameters.clone(),
        positions: [Vec4::ZERO; 20],
        colors: [Vec4::ZERO; 20],
        regions: [None; 20],
        output: Vec::new(),
    }
}

fn copy_output(mesh: &mut Mesh, state: &MeshState) {
    let Some(VertexAttributeValues::Float32x4(output)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("colors");
    };
    output.copy_from_slice(&state.output);
}

fn modified(app: &mut App) -> BTreeSet<AssetId<Mesh>> {
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<Mesh>>>()
        .drain()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .collect()
}

fn apply(
    app: &mut App,
    state: &mut TerrainState,
    positions: &[Vec4; 20],
    colors: &[Vec4; 20],
    regions: &[Option<Region>; 20],
) {
    let mut system = SystemState::<(
        Resources,
        Query<(&Mesh3d, &MeshMaterial3d<SourceMaterial>), With<crate::terrain::changes::Surface>>,
    )>::new(app.world_mut());
    let (mut resources, surfaces) = system.get_mut(app.world_mut()).unwrap();
    super::apply(&mut resources, &surfaces, state, positions, colors, regions);
}

fn assert_packed_meshes(app: &App, handles: &[Handle<Mesh>; 2], expected: &[Mesh; 2]) {
    let meshes = app.world().resource::<Assets<Mesh>>();
    for (handle, expected) in handles.iter().zip(expected) {
        let actual = meshes.get(handle).unwrap();
        assert_eq!(
            actual.create_packed_vertex_buffer_data(),
            expected.create_packed_vertex_buffer_data()
        );
        assert_eq!(actual.indices(), expected.indices());
    }
}

fn fixture() -> App {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let map = generals_assets::map::MapData::parse(
        &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
    )
    .unwrap();
    let terrain = crate::terrain::Terrain::new(map.heightmap, map.tiles, false, false).unwrap();
    let particles = crate::particles::load(&assets, &battle.simulation.catalog).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_resource::<Assets<SourceMaterial>>()
        .init_resource::<crate::light_environment::Frame>()
        .insert_resource(terrain)
        .insert_resource(particles)
        .insert_resource(battle);
    app
}

#[ignore = "requires original game installation"]
#[test]
fn actual_apply_emits_only_changed_assets_and_matches_old_packed_vertices() {
    let mut app = fixture();
    let material = SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false);
    let parameters = material.parameters.clone();
    let material = app
        .world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .add(material);
    let mut expected = [mesh(0.0), mesh(100.0)];
    let mut oracle = expected.each_ref().map(|mesh| cached(mesh, &parameters));
    let handles = expected.each_ref().map(|mesh| {
        app.world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(mesh.clone())
    });
    for (index, handle) in handles.iter().enumerate() {
        app.world_mut().spawn((
            crate::terrain::changes::Surface(index),
            Mesh3d(handle.clone()),
            MeshMaterial3d(material.clone()),
        ));
    }
    let _ = modified(&mut app);
    let mut state = TerrainState::default();
    let mut positions = [Vec4::ZERO; 20];
    let mut colors = [Vec4::ZERO; 20];
    let mut regions = [None; 20];
    regions[0] = Some(Region {
        current: [-2, -2, 3, 3],
        previous: [-2, -2, 3, 3],
    });
    positions[0] = Vec4::new(0.0, 0.0, 5.0, 21.0);
    // Initial baseline, pulse, fade, movement (old area clears), disable, and a
    // moving light outside both batches. Unchanged batches must not upload.
    for (step, intensity) in [0.0, 0.25, 0.125, 0.125, 0.0, 0.5].into_iter().enumerate() {
        colors[0] = Vec4::splat(intensity);
        if step == 3 {
            positions[0].x = 1000.0;
            regions[0].as_mut().unwrap().current = [98, -2, 103, 3];
        }
        if step == 4 {
            positions[0].w = 0.0;
            regions[0].as_mut().unwrap().previous = [98, -2, 103, 3];
        }
        if step == 5 {
            positions[0] = Vec4::new(2000.0, 0.0, 5.0, 21.0);
            regions[0] = Some(Region {
                current: [198, -2, 203, 3],
                previous: [198, -2, 203, 3],
            });
        }
        let revision = (
            0,
            app.world()
                .resource::<crate::playtest::Match>()
                .simulation
                .terrain_height_revision(),
        );
        let mut expected_modified = BTreeSet::new();
        for (index, (mesh, cached)) in expected.iter_mut().zip(&mut oracle).enumerate() {
            let before = mesh.create_packed_vertex_buffer_data();
            cached.prepare(mesh, revision, parameters.clone());
            old_paint(cached, mesh, &positions, &colors, &regions);
            copy_output(mesh, cached);
            if before != mesh.create_packed_vertex_buffer_data() {
                expected_modified.insert(handles[index].id());
            }
        }
        apply(&mut app, &mut state, &positions, &colors, &regions);
        assert_eq!(modified(&mut app), expected_modified, "step {step}");
        assert_packed_meshes(&app, &handles, &expected);
        if step == 1 || step == 2 {
            assert_eq!(expected_modified, BTreeSet::from([handles[0].id()]));
        }
        if step == 3 {
            assert_eq!(expected_modified.len(), 2);
        }
        if step == 5 {
            assert!(expected_modified.is_empty());
        }
    }
    // No input change remains event-free through the actual apply system.
    apply(&mut app, &mut state, &positions, &colors, &regions);
    assert!(modified(&mut app).is_empty());
}

#[test]
fn actual_mesh_bits_and_rebuilt_height_baseline_are_not_hidden_by_cache() {
    let parameters =
        SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false).parameters;
    let mut mesh = mesh(0.0);
    let mut state = cached(&mesh, &parameters);
    let positions = [Vec4::ZERO; 20];
    let colors = [Vec4::ZERO; 20];
    let regions = [None; 20];
    assert!(state.prepare(&mesh, (0, 0), parameters.clone()));
    assert!(state.paint(&mesh, &positions, &colors, &regions));
    copy_output(&mut mesh, &state);
    assert!(!state.paint(&mesh, &positions, &colors, &regions));
    let Some(VertexAttributeValues::Float32x4(actual)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("colors");
    };
    actual[0][0] = -0.0;
    assert!(
        state.paint(&mesh, &positions, &colors, &regions),
        "signed zero is a real bit change even with no intersecting light"
    );
    copy_output(&mut mesh, &state);
    // A height rebuild replaces authored colors/normals before revision advances.
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.5, 0.25, 1.0, 0.25]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.8, 0.6]; 4]);
    let mut changed = parameters;
    changed.scene_ambient = Vec4::splat(0.5);
    let mut expected = mesh.clone();
    let mut oracle = cached(&mesh, &changed);
    assert!(state.prepare(&mesh, (0, 1), changed.clone()));
    assert!(oracle.prepare(&expected, (0, 1), changed));
    old_paint(&mut oracle, &expected, &positions, &colors, &regions);
    copy_output(&mut expected, &oracle);
    assert!(state.paint(&mesh, &positions, &colors, &regions));
    copy_output(&mut mesh, &state);
    assert_eq!(
        mesh.create_packed_vertex_buffer_data(),
        expected.create_packed_vertex_buffer_data()
    );
    assert_eq!(state.revision, (0, 1));
    assert!(!state.paint(&mesh, &positions, &colors, &regions));
}

// Verbatim pre-change paint body (only self renamed to state). This intentionally
// retains unconditional writes independently of the new dirty-result comparison.
fn old_paint(
    state: &mut MeshState,
    mesh: &Mesh,
    positions: &[Vec4; 20],
    colors: &[Vec4; 20],
    regions: &[Option<Region>; 20],
) {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("native terrain positions");
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        panic!("native terrain normals");
    };
    for (quad_index, quad) in points.chunks_exact(4).enumerate() {
        let x = quad
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let y = quad
            .iter()
            .map(|point| -point[2])
            .fold(f32::INFINITY, f32::min);
        let x = x.to_i32().expect("native quad grid X");
        let y = y.to_i32().expect("native quad grid Y");
        if !regions
            .iter()
            .flatten()
            .any(|region| region.intersects(x, y))
        {
            continue;
        }
        for (corner, point) in quad.iter().enumerate() {
            let index = quad_index * 4 + corner;
            let point = Vec3::new(point[0] * 10.0, -point[2] * 10.0, point[1] * 10.0);
            let normal = normals[index];
            let normal = Vec3::new(normal[0], -normal[2], normal[1]);
            state.output[index] =
                illuminate(state.baseline[index], point, normal, positions, colors)
                    .map(|channel| f32::from(channel) / 255.0);
        }
    }
}
