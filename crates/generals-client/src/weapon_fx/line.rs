//! Shared native `Line3DClass` closed prism, used by `TracerDraw` and `RopeDraw`.
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
pub(super) fn vertices(axes: [[f32; 3]; 3], length: f32, width: f32) -> Vec<[f32; 3]> {
    let half = width * 0.5;
    [
        [0.0, -half, -half],
        [0.0, half, -half],
        [0.0, -half, half],
        [0.0, half, half],
        [length, -half, -half],
        [length, half, -half],
        [length, -half, half],
        [length, half, half],
    ]
    .map(|point| {
        let world = std::array::from_fn::<_, 3, _>(|axis| {
            let x = axes[0][axis] * point[0];
            let y = axes[1][axis] * point[1];
            let z = axes[2][axis] * point[2];
            x + y + z
        });
        [world[0] * 0.1, world[2] * 0.1, -world[1] * 0.1]
    })
    .to_vec()
}

pub(super) fn mesh(vertices: Vec<[f32; 3]>) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    // PRELIT_DIFFUSE has no normal lighting. The shared source shader needs
    // a finite normal/UV input; emissive supplies the original vertex RGB.
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 8]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 8]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; 8]);
    mesh.insert_indices(Indices::U32(vec![
        3, 5, 1, 7, 5, 3, 1, 5, 0, 5, 4, 0, 4, 2, 0, 4, 6, 2, 7, 3, 2, 6, 7, 2, 7, 6, 5, 5, 6, 4,
        2, 3, 1, 2, 1, 0,
    ]));
    mesh
}
pub(super) fn axes(direction: [f32; 3]) -> [[f32; 3]; 3] {
    // Matrix3D::buildTransformMatrix: identity, Rotate_Z(yaw), Rotate_Y(-pitch).
    let squared = [direction[0] * direction[0], direction[1] * direction[1]];
    let cosine_pitch = (squared[0] + squared[1]).sqrt();
    let (sine_yaw, cosine_yaw) = if cosine_pitch == 0.0 {
        (0.0, 1.0)
    } else {
        (direction[1] / cosine_pitch, direction[0] / cosine_pitch)
    };
    [
        [
            cosine_yaw * cosine_pitch,
            sine_yaw * cosine_pitch,
            direction[2],
        ],
        [-sine_yaw, cosine_yaw, 0.0],
        [
            -cosine_yaw * direction[2],
            -sine_yaw * direction[2],
            cosine_pitch,
        ],
    ]
}
