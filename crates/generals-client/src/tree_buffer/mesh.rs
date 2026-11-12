//! Native ordered optimized tree vertex/index rebuilding, with pre-shader lighting.
use super::{
    atlas::Atlas,
    state::{Buffer, Topple},
};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use generals_sim::skirmish::physical_ocl::transform::vector;
use num_traits::ToPrimitive;

pub struct Lighting {
    pub ambient: [f32; 3],
    pub colors: [[f32; 3]; 3],
    pub directions: [[f32; 3]; 3],
}

impl Buffer {
    pub(super) fn mesh(&self, atlas: &Atlas, lighting: &Lighting) -> Mesh {
        let mut positions = Vec::new();
        let mut packed = Vec::new();
        let mut colors = Vec::new();
        let mut uv = Vec::new();
        let mut indices = Vec::new();
        for tree in &self.trees {
            let Some(kind) = tree.kind else {
                continue;
            };
            if !tree.visible {
                continue;
            }
            let data = &self.types[kind];
            if positions.len() + data.vertices.len() + 2 >= 30_000
                || indices.len() + data.indices.len() + 6 >= 60_000
            {
                break;
            }
            let start = u32::try_from(positions.len()).expect("30000 vertices");
            let (scale, offset, v_offset) = atlas_coordinates(data, atlas, kind);
            for (index, vertex) in data.vertices.iter().enumerate() {
                let x = vertex[0] + data.offset[0];
                let y = vertex[1] + data.offset[1];
                let x_cosine = x * tree.scale * tree.cosine;
                let y_sine = y * tree.scale * tree.sine;
                let y_cosine = y * tree.scale * tree.cosine;
                let x_sine = x * tree.scale * tree.sine;
                let height = vertex[2] * tree.scale;
                let mut point = [
                    x_cosine - y_sine,
                    y_cosine + x_sine,
                    height + data.offset[2],
                ];
                if matches!(tree.topple, Topple::Upright) {
                    if tree.push > 0.0 {
                        let x_push = vertex[2] * tree.push * tree.push_cosine * data.distance;
                        let y_push = vertex[2] * tree.push * tree.push_sine * data.distance;
                        point[0] += x_push;
                        point[1] += y_push;
                    }
                } else {
                    point = vector(tree.basis, point);
                }
                for (point, translation) in point.iter_mut().zip(tree.position) {
                    *point += translation;
                }
                positions.push(point);
                let darkening = data.darkening * tree.push;
                packed.push([
                    tree.sway.to_f32().expect("ten sway types"),
                    1.0 - darkening,
                    tree.position[2],
                ]);
                let normal = data.normals[index];
                let x_cosine = normal[0] * tree.cosine;
                let y_sine = normal[1] * tree.sine;
                let y_cosine = normal[1] * tree.cosine;
                let x_sine = normal[0] * tree.sine;
                let normal = [x_cosine - y_sine, y_cosine + x_sine, normal[2]];
                colors.push(light(normal, data.emissive, data.colors[index], lighting));
                let u = data.uv[index][0].clamp(0.0, 1.0) * scale;
                let v = data.uv[index][1].clamp(0.0, 1.0) * scale;
                uv.push([u + offset[0], v + v_offset]);
            }
            indices.extend(data.indices.iter().map(|index| start + index));
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, packed)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(indices))
    }
}

fn atlas_coordinates(data: &super::data::Data, atlas: &Atlas, kind: usize) -> (f32, [f32; 2], f32) {
    let width = data.pixels.width.to_f32().expect("ten texture tiles") * 64.0;
    let mut scale = width / atlas.extent.to_f32().expect("2048 atlas");
    let offset = atlas.origins[kind].map(|value| {
        value.to_f32().expect("2048 atlas") / atlas.extent.to_f32().expect("2048 atlas")
    });
    let mut v_offset = offset[1];
    if data.pixels.half {
        scale *= 0.5;
        v_offset += 32.0 / atlas.extent.to_f32().expect("2048 atlas");
    }
    (scale, offset, v_offset)
}

fn light(normal: [f32; 3], emissive: [f32; 3], diffuse: [u8; 4], lighting: &Lighting) -> [f32; 4] {
    let mut color = std::array::from_fn::<_, 3, _>(|axis| lighting.ambient[axis] + emissive[axis]);
    for (direction, light) in lighting.directions.iter().zip(lighting.colors) {
        let x_squared = direction[0] * direction[0];
        let y_squared = direction[1] * direction[1];
        let z_squared = direction[2] * direction[2];
        let length = ((x_squared + y_squared) + z_squared).sqrt();
        let direction = direction.map(|value| value / length);
        let x = -direction[0] * normal[0];
        let y = direction[1] * normal[1];
        let z = direction[2] * normal[2];
        let shade = ((x - y) - z).clamp(0.0, 1.0);
        for axis in 0..3 {
            let contribution = shade * light[axis];
            color[axis] += contribution;
        }
    }
    let color = std::array::from_fn::<_, 3, _>(|axis| {
        let mut channel = color[axis].clamp(0.0, 1.0);
        if diffuse != [255; 4] {
            channel *= f32::from(diffuse[axis]) / 255.0;
        }
        (channel * 255.0).trunc() / 255.0
    });
    [color[0], color[1], color[2], 1.0]
}
