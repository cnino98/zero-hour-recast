use super::*;
use bevy::camera::primitives::MeshAabb;
use generals_assets::w3d::MeshPose;

fn triangle() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.5, 0.75]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.1, 0.2, 0.3, 0.4]; 3])
}

fn float3(mesh: &Mesh, attribute: bevy::mesh::MeshVertexAttribute) -> &[[f32; 3]] {
    let Some(VertexAttributeValues::Float32x3(values)) = mesh.attribute(attribute) else {
        panic!("missing float3 attribute");
    };
    values
}

#[test]
fn deformation_reuses_storage_preserves_uv_color_and_separates_instances() {
    let mut mesh = triangle();
    let unchanged = mesh.clone();
    let pointer = float3(&mesh, Mesh::ATTRIBUTE_POSITION).as_ptr();
    let binding = PartBinding {
        pass: 0,
        mesh: 0,
        vertices: vec![2, 0, 1],
    };
    let poses = [MeshPose {
        bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
        positions: vec![[10.0, 20.0, 30.0], [40.0, 50.0, 60.0], [70.0, 80.0, 90.0]],
        normals: Some(vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        visible: false,
    }];
    assert!(!deform(&mut mesh, &binding, &poses).unwrap());
    assert_eq!(float3(&mesh, Mesh::ATTRIBUTE_POSITION).as_ptr(), pointer);
    assert_eq!(
        float3(&mesh, Mesh::ATTRIBUTE_POSITION)[0].map(f32::to_bits),
        [7.0_f32, 9.0, -8.0].map(f32::to_bits)
    );
    assert_eq!(
        float3(&unchanged, Mesh::ATTRIBUTE_POSITION)[0].map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    assert_eq!(
        mesh.attribute(Mesh::ATTRIBUTE_UV_0),
        unchanged.attribute(Mesh::ATTRIBUTE_UV_0)
    );
    assert_eq!(
        mesh.attribute(Mesh::ATTRIBUTE_COLOR),
        unchanged.attribute(Mesh::ATTRIBUTE_COLOR)
    );
    assert_ne!(
        mesh.compute_aabb().unwrap().center,
        unchanged.compute_aabb().unwrap().center
    );
}

#[test]
fn normal_less_pose_matches_bevy_flat_normals_including_degenerate_faces() {
    for points in [
        vec![[0.0; 3], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]],
        vec![[0.0; 3]; 3],
    ] {
        let mut mesh = triangle();
        let binding = PartBinding {
            pass: 0,
            mesh: 0,
            vertices: vec![0, 1, 2],
        };
        let poses = [MeshPose {
            bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
            positions: points,
            normals: None,
            visible: true,
        }];
        assert!(deform(&mut mesh, &binding, &poses).unwrap());
        assert!(deformation_matches(&mesh, &binding, &poses).unwrap());
        let actual = float3(&mesh, Mesh::ATTRIBUTE_NORMAL)
            .iter()
            .map(|normal| normal.map(f32::to_bits))
            .collect::<Vec<_>>();
        mesh.compute_flat_normals();
        let expected = float3(&mesh, Mesh::ATTRIBUTE_NORMAL)
            .iter()
            .map(|normal| normal.map(f32::to_bits))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}

#[test]
fn invalid_source_binding_does_not_partially_deform_live_mesh() {
    let mut mesh = triangle();
    let original = mesh.clone();
    let binding = PartBinding {
        pass: 0,
        mesh: 0,
        vertices: vec![0, 1, 2],
    };
    let poses = [MeshPose {
        bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
        positions: vec![[20.0; 3]; 3],
        normals: Some(vec![[1.0; 3]; 2]),
        visible: true,
    }];
    assert!(deform(&mut mesh, &binding, &poses).is_err());
    assert!(deformation_matches(&mesh, &binding, &poses).is_err());
    assert_eq!(
        mesh.attribute(Mesh::ATTRIBUTE_POSITION),
        original.attribute(Mesh::ATTRIBUTE_POSITION)
    );
}

#[test]
fn exact_comparison_separates_visibility_normal_changes_and_signed_zero() {
    let mut mesh = triangle();
    let binding = PartBinding {
        pass: 0,
        mesh: 0,
        vertices: vec![0, 1, 2],
    };
    let mut poses = [MeshPose {
        bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
        positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
        normals: Some(vec![[0.0, 0.0, 1.0]; 3]),
        visible: true,
    }];
    deform(&mut mesh, &binding, &poses).unwrap();
    poses[0].visible = false;
    assert!(deformation_matches(&mesh, &binding, &poses).unwrap());
    poses[0].normals.as_mut().unwrap()[1] = [1.0, 0.0, 0.0];
    assert!(!deformation_matches(&mesh, &binding, &poses).unwrap());
    deform(&mut mesh, &binding, &poses).unwrap();
    assert!(deformation_matches(&mesh, &binding, &poses).unwrap());
    let previous = mesh.create_packed_vertex_buffer_data();
    poses[0].positions[0][0] = -0.0;
    assert!(!deformation_matches(&mesh, &binding, &poses).unwrap());
    deform(&mut mesh, &binding, &poses).unwrap();
    assert_ne!(mesh.create_packed_vertex_buffer_data(), previous);
    assert!(deformation_matches(&mesh, &binding, &poses).unwrap());
}
