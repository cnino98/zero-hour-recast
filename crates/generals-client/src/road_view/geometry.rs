//! `W3DRoadBuffer::loadFloat4PtSection`, SH 2bfaea60.
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use num_traits::ToPrimitive;
use std::error::Error;

use crate::terrain::Terrain;

pub(super) struct Section {
    pub origin: Vec2,
    pub normal: Vec2,
    pub direction: Vec2,
    /// Native bottom-left, bottom-right, top-left, top-right order.
    pub corners: [Vec2; 4],
    pub offset: Vec2,
    pub scale: Vec2,
}

struct Column {
    points: [Vec3; 2],
    index: f32,
    vertices: [u32; 2],
}

pub(super) fn normalized(vector: Vec2) -> Vec2 {
    let squared = vector.length_squared();
    if squared == 0.0 {
        vector
    } else {
        vector * (1.0 / squared.sqrt())
    }
}

pub(super) fn mesh(section: &Section, terrain: &Terrain) -> Result<Mesh, Box<dyn Error>> {
    let columns = (section.direction.length() / 10.0)
        .to_u32()
        .ok_or("invalid road length")?
        .saturating_add(1)
        .max(2);
    let rows = (2.0 * section.normal.length() / 10.0)
        .to_u32()
        .ok_or("invalid road width")?
        .saturating_add(1)
        .clamp(2, 100);
    if columns > 100_000 {
        return Err("road sampling exceeds supported map extent".into());
    }
    let direction = normalized(section.direction);
    let normal = normalized(section.normal);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uv = Vec::new();
    let mut indices = Vec::new();
    let mut previous: Option<Column> = None;
    let mut current: Option<Column> = None;
    for index in 0..=columns {
        let mut next = if index < columns {
            Some(sample_column(section, terrain, index, columns, rows))
        } else {
            None
        };
        if index < 2 {
            current = next.take();
        } else if let (Some(before), Some(now), Some(after)) = (&previous, &current, &next) {
            let can_delete = (0..2).all(|side| {
                // Preserve the original weighting order, including its unequal
                // spans after an earlier column was removed.
                let lower_height = before.points[side].z * (now.index - before.index);
                let upper_height = after.points[side].z * (after.index - now.index);
                let height = (lower_height + upper_height) / (after.index - before.index);
                let tolerance = 0.625 * 1.1;
                height >= now.points[side].z && height < now.points[side].z + tolerance
            });
            if can_delete {
                current = None;
            }
        }
        if index != 1
            && let Some(mut column) = current.take()
        {
            if positions.len() + 2 > 500 || indices.len() + 6 > 2000 {
                return Err("original road segment vertex/index capacity exceeded".into());
            }
            for (side, point) in column.points.iter().enumerate() {
                column.vertices[side] = u32::try_from(positions.len())?;
                let delta = point.truncate() - section.origin;
                uv.push([
                    section.offset.x + direction.dot(delta) / (section.scale.x * 4.0),
                    section.offset.y - normal.dot(delta) / (section.scale.y * 4.0),
                ]);
                positions.push([point.x * 0.1, (point.z + 0.625 / 8.0) * 0.1, -point.y * 0.1]);
                normals.push(terrain.road_vertex_normal(point.truncate()).to_array());
            }
            if let Some(before) = &previous {
                indices.extend([
                    before.vertices[1],
                    before.vertices[0],
                    column.vertices[0],
                    before.vertices[1],
                    column.vertices[0],
                    column.vertices[1],
                ]);
            }
            previous = Some(column);
        }
        if index >= 2 {
            current = next;
        }
    }
    Ok(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(indices)))
}

fn sample_column(
    section: &Section,
    terrain: &Terrain,
    index: u32,
    columns: u32,
    rows: u32,
) -> Column {
    let [bottom_left, bottom_right, top_left, top_right] = section.corners;
    let lower = bottom_right - bottom_left;
    let left = top_left - bottom_left;
    let right = top_right - bottom_right;
    let upper = top_right - top_left + (left - right);

    let fraction = index.to_f32().expect("bounded road ordinal")
        / (columns - 1).to_f32().expect("bounded road count");
    let mut endpoints = [Vec3::ZERO; 2];
    let mut maximum = f32::NEG_INFINITY;
    for row in 0..rows {
        let across =
            row.to_f32().expect("bounded row") / (rows - 1).to_f32().expect("bounded rows");
        let point = bottom_left
            + lower * (1.0 - across) * fraction
            + upper * across * fraction
            + left * (1.0 - fraction) * across
            + right * fraction * across;
        maximum = maximum.max(terrain.road_cell_height(point));
        if row == 0 {
            endpoints[0] = point.extend(0.0);
        }
        if row == rows - 1 {
            endpoints[1] = point.extend(0.0);
        }
    }
    for point in &mut endpoints {
        point.z = maximum;
    }
    Column {
        points: endpoints,
        index: index.to_f32().expect("bounded ordinal"),
        vertices: [0; 2],
    }
}

#[cfg(test)]
mod tests;
