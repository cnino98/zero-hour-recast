//! Original map road network and W3D road atlas geometry, SH 2bfaea60.
mod data;
mod geometry;
mod network;
mod primitives;
#[cfg(test)]
mod tests;

use bevy::prelude::*;

const LAST: u8 = 1;
const MULTIPLE: u8 = 2;
const ANGLED: u8 = 4;
const JOIN: u8 = 8;

// Native road insertion/reordering copies both endpoints and whole segments.
#[derive(Clone, Copy)]
struct Point {
    position: Vec2,
    top: Vec2,
    bottom: Vec2,
    count: i32,
    flags: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Segment,
    Curve,
    Tee,
    FourWay,
    ThreeWayY,
    ThreeWayH,
    ThreeWayHFlip,
    AlphaJoin,
}

#[derive(Clone)]
struct Segment {
    points: [Point; 2],
    radius: f32,
    kind: Kind,
    scale: f32,
    width: f32,
    road: usize,
}

struct Road {
    name: String,
    texture: String,
    scale: f32,
    width: f32,
    id: u32,
    stacking: u32,
}

pub fn load(
    assets: &generals_assets::virtual_assets::VirtualAssets,
    records: &[generals_assets::map::MapObject],
    terrain: &crate::terrain::Terrain,
) -> Result<Vec<crate::model_render::Part>, Box<dyn std::error::Error>> {
    use bevy::{
        asset::RenderAssetUsages,
        mesh::Indices,
        render::render_resource::{CompareFunction, PrimitiveTopology},
    };
    use num_traits::ToPrimitive;
    let (mut roads, limits) = data::load(assets)?;
    let segments = network::build(records, &mut roads, limits.segments)?;
    let mut order: Vec<_> = (0..roads.len().min(limits.types)).collect();
    order.sort_by_key(|&index| roads[index].stacking);
    let mut parts = Vec::new();
    for index in order {
        let road = &roads[index];
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
        .with_inserted_indices(Indices::U32(Vec::new()));
        // Native render traversal differs from preload traversal: enum order
        // determines overlap within each road type.
        for kind in [
            Kind::Segment,
            Kind::Curve,
            Kind::Tee,
            Kind::FourWay,
            Kind::ThreeWayY,
            Kind::ThreeWayH,
            Kind::ThreeWayHFlip,
            Kind::AlphaJoin,
        ] {
            for segment in segments
                .iter()
                .filter(|segment| segment.road == index && segment.kind == kind)
            {
                let part = geometry::mesh(&primitives::section(segment), terrain)?;
                let vertices = mesh.count_vertices() + part.count_vertices();
                let indices = mesh.indices().expect("road indices").len()
                    + part.indices().expect("road indices").len();
                // Native buffers are populated from the visible terrain window.
                // Until the common terrain window is available, reject maps that
                // cannot fit rather than silently dropping distant road sections.
                if vertices >= limits.vertices || indices >= limits.indices {
                    return Err(format!(
                        "Road {} requires native terrain-window batching",
                        road.name
                    )
                    .into());
                }
                mesh.merge(&part)?;
            }
        }
        if mesh.count_vertices() == 0 {
            continue;
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; mesh.count_vertices()]);
        mesh.final_aabb = Some(bevy::math::bounding::Aabb3d::new(Vec3::ZERO, Vec3::ZERO));
        let mut material = crate::source_material::SourceMaterial::new(
            [1.0; 3],
            [1.0; 3],
            [0.0; 3],
            1.0,
            AlphaMode::Blend,
            false,
        );
        material.depth_write = false;
        // Native LESSEQUAL under Bevy's reversed depth range.
        material.depth_compare = Some(CompareFunction::GreaterEqual);
        material.depth_bias = parts.len().to_f32().ok_or("Too many road draw batches")?;
        material.parameters.vertex_diffuse = 2;
        parts.push(crate::model_render::Part {
            light_bounds: None,
            submission: None,
            order: crate::model_render::RenderOrder::ordinary(),
            mesh,
            material,
            texture: Some((road.texture.to_ascii_lowercase(), 0x8000)),
            texture_extent: None,
            house_color: false,
            pickable: false,
            binding: None,
        });
    }
    Ok(parts)
}
