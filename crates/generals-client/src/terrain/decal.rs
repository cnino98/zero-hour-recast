//! `W3DProjectedShadowManager::queueDecal`: original terrain cells and decal UVs.
use super::Terrain;
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

impl Terrain {
    /// Native XY inputs; output uses the existing [X, Z, -Y] / 10 render basis.
    pub fn radius_decal(&self, center: Vec2, radius: f32, color: [f32; 4]) -> Mesh {
        self.object_decal(center, Vec2::splat(radius * 2.0), Vec2::ZERO, 0.0, color)
    }

    /// Native object shadow dimensions, offsets and projected heading.
    pub fn object_decal(
        &self,
        center: Vec2,
        size: Vec2,
        offset: Vec2,
        angle: f32,
        color: [f32; 4],
    ) -> Mesh {
        let (sin, cos) = angle.sin_cos();
        let u = Vec2::new(cos, sin).normalize();
        let v = Vec2::new(u.y, -u.x);
        let shift = offset / size;
        let left = -size.x * (u * (0.5 + shift.x));
        let right = size.x * (u * (0.5 - shift.x));
        let top = -size.y * (v * (0.5 + shift.y));
        let bottom = size.y * (v * (0.5 - shift.y));
        let corners = [left + top, right + top, right + bottom, left + bottom];
        let minimum = corners
            .into_iter()
            .fold(Vec2::splat(f32::INFINITY), Vec2::min)
            + center;
        let maximum = corners
            .into_iter()
            .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max)
            + center;
        let [start_x, end_x] = bounds(minimum.x, maximum.x, self.border, self.raw_width);
        let [start_y, end_y] = bounds(minimum.y, maximum.y, self.border, self.raw_height);
        let width = u32::from(end_x - start_x + 1);
        let mut positions = Vec::new();
        let mut uv = Vec::new();
        let mut indices = Vec::new();
        let u = u / size.x;
        let v = v / size.y;
        for y in start_y..=end_y {
            for x in start_x..=end_x {
                let point = Vec2::new(
                    f32::from(x) - f32::from(self.border),
                    f32::from(y) - f32::from(self.border),
                ) * 10.0;
                // Original raw terrain height plus 0.01 * MAP_XY_FACTOR.
                // Every u8 height times 0.625 is exact, so the fused form retains
                // the native sum's bits here; UV products below must round first.
                let height = self.raw_height(x, y).mul_add(0.625, 0.1);
                positions.push([point.x * 0.1, height * 0.1, -point.y * 0.1]);
                let relative = point - center;
                let ux = u.x * relative.x;
                let uy = u.y * relative.y;
                let vx = v.x * relative.x;
                let vy = v.y * relative.y;
                uv.push([ux + uy + (shift.x + 0.5), vx + vy + (shift.y + 0.5)]);
            }
        }
        for y in start_y..end_y {
            for x in start_x..end_x {
                let index = self.index(x, y);
                let primary =
                    self.tiles.blends[usize::from(self.tiles.blend_tiles[index])].as_ref();
                let tile = primary.map_or(self.tiles.base_tiles[index], |blend| blend.tile);
                let stretched = self
                    .tiles
                    .texture_uv(index, tile, &self.heights, self.adjust_cliff_textures)
                    .expect("validated terrain cell")
                    .stretched;
                let first = u32::from(y - start_y) * width + u32::from(x - start_x);
                let cell = if self.primary_flip(x, y, stretched) {
                    [1, width, 0, 1, 1 + width, width]
                } else {
                    [0, 1 + width, width, 0, 1, 1 + width]
                };
                indices.extend(cell.map(|offset| first + offset));
            }
        }
        let count = positions.len();
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![color; count])
        .with_inserted_indices(Indices::U32(indices))
    }
}

fn bounds(minimum: f32, maximum: f32, border: u16, extent: u16) -> [u16; 2] {
    let edges = [
        generals_sim::native_real::floor(minimum * 0.1),
        generals_sim::native_real::ceil(maximum * 0.1),
    ]
    .map(|edge| (edge + i32::from(border)).clamp(0, i32::from(extent - 1)));
    let [mut start, mut end] =
        edges.map(|edge| u16::try_from(edge).expect("bounded terrain index"));
    let extra = (end - start + 1).saturating_sub(104);
    start += extra / 2;
    end -= extra - extra / 2;
    [start, end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn projected_radius_uses_native_uv_orientation_offset_and_terrain_diagonal() {
        let mut terrain = super::super::tests::terrain().unwrap();
        terrain.heights[27] = 16;
        let mesh = terrain.radius_decal(Vec2::new(5.0, 5.0), 5.0, [1.0; 4]);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("decal positions");
        };
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("decal UVs");
        };
        // Native fastceil(1.0) is 2: the exact upper terrain edge includes
        // one extra cell, whose clamped shadow UV lies outside the decal.
        assert_eq!(positions.len(), 9);
        assert_eq!(uv[0].map(f32::to_bits), [0.0_f32, 1.0].map(f32::to_bits));
        assert_eq!(uv[4].map(f32::to_bits), [1.0_f32, 0.0].map(f32::to_bits));
        assert_eq!(uv[8].map(f32::to_bits), [2.0_f32, -1.0].map(f32::to_bits));
        assert!((positions[4][1] - 1.01).abs() < 0.000_001);
        assert_eq!(
            mesh.indices().unwrap().iter().take(6).collect::<Vec<_>>(),
            [0, 4, 3, 0, 1, 4]
        );
        let a = Vec3::from_array(positions[0]);
        let b = Vec3::from_array(positions[4]);
        let c = Vec3::from_array(positions[3]);
        assert!((b - a).cross(c - a).y > 0.0);
    }

    #[test]
    fn projected_bounds_floor_negative_positions_clip_edges_and_cap_native_buffer() {
        assert_eq!(bounds(-10.01, 9.99, 2, 8), [0, 3]);
        assert_eq!(bounds(-10.0, 10.0, 2, 8), [0, 4]);
        assert_eq!(bounds(10.0, 110.0, 2, 8), [3, 7]);
        assert_eq!(bounds(0.0, 2_000.0, 2, 300), [51, 154]);
    }

    #[test]
    fn rectangular_object_decal_projects_rotated_axes_and_authored_shadow_offset() {
        let terrain = super::super::tests::terrain().unwrap();
        let mesh = terrain.object_decal(
            Vec2::new(20.0, 20.0),
            Vec2::new(40.0, 20.0),
            Vec2::new(10.0, -5.0),
            std::f32::consts::FRAC_PI_2,
            [1.0; 4],
        );
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("decal positions");
        };
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("decal UVs");
        };
        // Native U follows the projected object X axis; V turns it clockwise.
        // The object origin therefore samples .5 + offset / dimension.
        let origin = positions
            .iter()
            .position(|point| {
                (point[0] - 2.0).abs() < f32::EPSILON && (point[2] + 2.0).abs() < f32::EPSILON
            })
            .unwrap();
        assert!((uv[origin][0] - 0.75).abs() < f32::EPSILON);
        assert!((uv[origin][1] - 0.25).abs() < f32::EPSILON);
        let east = positions
            .iter()
            .position(|point| {
                (point[0] - 3.0).abs() < f32::EPSILON && (point[2] + 2.0).abs() < f32::EPSILON
            })
            .unwrap();
        assert!((uv[east][0] - 0.75).abs() < 0.000_001);
        assert!((uv[east][1] - 0.75).abs() < 0.000_001);
    }
}
