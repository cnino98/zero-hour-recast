//! `SortingRenderer::Sort` and `Flush_Sorting_Pool` (SH 2bfaea60).
use bevy::{
    asset::RenderAssetUsages, math::Affine3A, mesh::VertexAttributeValues, prelude::*,
    render::render_resource::PrimitiveTopology,
};

pub(super) struct Vertex {
    pub position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}

// Copy is needed by the source insertion sort's saved pivot and overlapping moves.
#[derive(Clone, Copy)]
pub(super) struct Triangle {
    pub vertices: [usize; 3],
    pub node: usize,
    pub depth: f32,
}

pub(super) fn append(
    mesh: &Mesh,
    world: Affine3A,
    view: Affine3A,
    node: usize,
    vertices: &mut Vec<Vertex>,
    triangles: &mut Vec<Triangle>,
) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("source sorted mesh requires positions")
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        panic!("source sorted mesh requires normals")
    };
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("source sorted mesh requires UV0")
    };
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(colors)) => Some(colors),
        None => None,
        _ => panic!("source sorted mesh requires floating diffuse colors"),
    };
    let normal_matrix = world.matrix3.inverse().transpose();
    let start = vertices.len();
    for (index, position) in positions.iter().enumerate() {
        vertices.push(Vertex {
            position: world.transform_point3(Vec3::from(*position)).to_array(),
            normal: (normal_matrix * Vec3::from(normals[index]))
                .normalize_or_zero()
                .to_array(),
            uv: uv[index],
            color: colors.map_or([1.0; 4], |colors| colors[index]),
        });
    }
    let transform = view * world;
    let append_triangle = |indices: [usize; 3], triangles: &mut Vec<Triangle>| {
        // Native computes one matrix product from the summed local coordinates,
        // then adds translation; it does not average three transformed positions.
        let [a, b, c] = indices.map(|index| positions[index]);
        let horizontal_depth = transform.matrix3.x_axis.z * (a[0] + b[0] + c[0]);
        let vertical_depth = transform.matrix3.y_axis.z * (a[1] + b[1] + c[1]);
        let z = transform.matrix3.z_axis.z * (a[2] + b[2] + c[2]);
        let z = (horizontal_depth + vertical_depth + z) / 3.0 + transform.translation.z;
        assert!(z.is_finite(), "source sorted triangle has nonfinite depth");
        triangles.push(Triangle {
            vertices: indices.map(|index| start + index),
            node,
            depth: z,
        });
    };
    if let Some(indices) = mesh.indices() {
        let mut indices = indices.iter();
        while let Some(a) = indices.next() {
            let b = indices.next().expect("source triangle indices");
            let c = indices.next().expect("source triangle indices");
            append_triangle([a, b, c], triangles);
        }
    } else {
        for first in (0..positions.len()).step_by(3) {
            append_triangle([first, first + 1, first + 2], triangles);
        }
    }
}

pub(super) fn sort(triangles: &mut [Triangle]) {
    let count = triangles.len();
    if count <= 16 {
        for current in 1..count {
            let value = triangles[current];
            let mut insert = current;
            while insert > 0 && triangles[insert - 1].depth > value.depth {
                triangles[insert] = triangles[insert - 1];
                insert -= 1;
            }
            triangles[insert] = value;
        }
        return;
    }
    triangles.swap(count / 2, 1);
    if triangles[1].depth > triangles[count - 1].depth {
        triangles.swap(1, count - 1);
    }
    if triangles[0].depth > triangles[count - 1].depth {
        triangles.swap(0, count - 1);
    }
    if triangles[1].depth > triangles[0].depth {
        triangles.swap(1, 0);
    }
    let mut left = 1;
    let mut right = count - 1;
    loop {
        left += 1;
        while left < count - 1 && triangles[left].depth < triangles[0].depth {
            left += 1;
        }
        right -= 1;
        while right > 1 && triangles[right].depth > triangles[0].depth {
            right -= 1;
        }
        if right < left {
            break;
        }
        triangles.swap(left, right);
    }
    triangles.swap(0, right);
    let (lower, upper) = triangles.split_at_mut(right);
    if lower.len() > upper.len() - 1 {
        sort(&mut upper[1..]);
        sort(lower);
    } else {
        sort(lower);
        sort(&mut upper[1..]);
    }
}

pub(super) fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    mesh.final_aabb = Some(bevy::math::bounding::Aabb3d::new(Vec3::ZERO, Vec3::ZERO));
    mesh
}

pub(super) fn fill(mesh: &mut Mesh, vertices: &[Vertex], triangles: &[Triangle]) {
    let Some(VertexAttributeValues::Float32x3(values)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        unreachable!()
    };
    values.clear();
    values.extend(
        triangles
            .iter()
            .flat_map(|triangle| triangle.vertices.map(|index| vertices[index].position)),
    );
    let Some(VertexAttributeValues::Float32x3(values)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    else {
        unreachable!()
    };
    values.clear();
    values.extend(
        triangles
            .iter()
            .flat_map(|triangle| triangle.vertices.map(|index| vertices[index].normal)),
    );
    let Some(VertexAttributeValues::Float32x2(values)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
    else {
        unreachable!()
    };
    values.clear();
    values.extend(
        triangles
            .iter()
            .flat_map(|triangle| triangle.vertices.map(|index| vertices[index].uv)),
    );
    let Some(VertexAttributeValues::Float32x4(values)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        unreachable!()
    };
    values.clear();
    values.extend(
        triangles
            .iter()
            .flat_map(|triangle| triangle.vertices.map(|index| vertices[index].color)),
    );
}

/// Preserve the dynamic pool's high-water allocation; only live regions draw.
pub(super) fn fill_pool(mesh: &mut Mesh, vertices: &[Vertex], triangles: &[Triangle]) {
    let high_water = mesh.count_vertices().max(triangles.len() * 3);
    fill(mesh, vertices, triangles);
    for (_, values) in mesh.attributes_mut() {
        match values {
            VertexAttributeValues::Float32x2(values) => values.resize(high_water, [0.0; 2]),
            VertexAttributeValues::Float32x3(values) => values.resize(high_water, [0.0; 3]),
            VertexAttributeValues::Float32x4(values) => values.resize(high_water, [0.0; 4]),
            _ => unreachable!("fixed sorted pool vertex layout"),
        }
    }
}
