use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use generals_assets::{HeightMap, map::TerrainTiles};
use std::{collections::BTreeMap, error::Error};

// WorldHeightMap.h: VERTEX_BUFFER_TILE_LENGTH, in full-resolution quads.
const VERTEX_BUFFER_TILE_LENGTH: u16 = 32;

pub mod changes;
mod decal;
mod movement;
mod radar;
mod road;
mod scorch;
mod water;

/// Source terrain class, rendered in opaque/primary/extra blend pass order.
pub struct TerrainBatch {
    pub texture_class: String,
    pub pixel_extent: u32,
    pub layer: u8,
    pub mesh: Mesh,
}

/// Native XY coordinates divided by ten; Bevy basis is [X, Z, -Y].
#[derive(Resource)]
pub struct Terrain {
    pub width: u16,
    pub height: u16,
    pub border: u16,
    raw_width: u16,
    raw_height: u16,
    path_extent: [i32; 2],
    heights: Vec<u16>,
    tiles: TerrainTiles,
    adjust_cliff_textures: bool,
    use_three_way_blends: bool,
}

impl Terrain {
    pub(crate) fn tree_bounds(&self) -> [f32; 2] {
        [self.raw_width, self.raw_height].map(|extent| f32::from(extent - 2 * self.border) * 10.0)
    }

    pub(crate) fn draw_region(&self) -> [[f32; 2]; 2] {
        let border = f32::from(self.border);
        [
            [-border * 10.0; 2],
            [self.raw_width, self.raw_height].map(|size| (f32::from(size - 1) - border) * 10.0),
        ]
    }

    pub fn height_map(&self) -> generals_sim::skirmish::TerrainHeights {
        generals_sim::skirmish::TerrainHeights {
            width: self.raw_width,
            height: self.raw_height,
            origin: [-f32::from(self.border) * HeightMap::CELL_SIZE; 2],
            samples: self
                .heights
                .iter()
                .map(|height| f32::from(*height) * HeightMap::HEIGHT_SCALE)
                .collect(),
        }
    }

    pub fn configure_projectiles(
        &self,
        game: &mut generals_sim::skirmish::Skirmish,
        partition_cell_size: f32,
    ) -> Result<(), &'static str> {
        let height_map = game
            .terrain_height_map()
            .unwrap_or_else(|| self.height_map());
        game.set_projectile_terrain(
            height_map.width,
            height_map.height,
            height_map.origin,
            height_map.samples,
            partition_cell_size,
        )
    }

    pub fn new(
        map: HeightMap,
        tiles: TerrainTiles,
        adjust_cliff_textures: bool,
        use_three_way_blends: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let raw_width = u16::try_from(map.width)?;
        let raw_height = u16::try_from(map.height)?;
        let border = u16::try_from(map.border)?;
        if !(2..=2048).contains(&raw_width) || !(2..=2048).contains(&raw_height) {
            return Err("Supported map dimensions are 2 to 2048 samples per side.".into());
        }
        let [width, height] = *map
            .boundaries
            .first()
            .ok_or("Map has no active boundary.")?;
        let width = u16::try_from(width)?;
        let height = u16::try_from(height)?;
        if width == 0
            || height == 0
            || u32::from(width) + map.border >= map.width
            || u32::from(height) + map.border >= map.height
        {
            return Err("Playable map boundary lies outside the terrain samples.".into());
        }
        if map.heights.len() != usize::from(raw_width) * usize::from(raw_height)
            || map.heights.iter().any(|&height| height > 255)
            || tiles.width != map.width
            || tiles.height != map.height
        {
            return Err("Terrain arrays do not match the source heightmap.".into());
        }
        if tiles
            .blends
            .iter()
            .flatten()
            .any(|blend| blend.custom_edge >= 0)
        {
            return Err(
                "This terrain renderer does not yet support custom blend-edge textures.".into(),
            );
        }
        Ok(Self {
            width,
            height,
            border,
            raw_width,
            raw_height,
            path_extent: map.boundaries.iter().fold([0, 0], |extent, boundary| {
                [extent[0].max(boundary[0]), extent[1].max(boundary[1])]
            }),
            heights: map.heights,
            tiles,
            adjust_cliff_textures,
            use_three_way_blends,
        })
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.raw_width) + usize::from(x)
    }

    fn raw_height(&self, x: u16, y: u16) -> f32 {
        f32::from(self.heights[self.index(x, y)])
    }

    /// `BaseHeightMap::getHeightMapHeight` samples the fixed 0-to-2 diagonal,
    /// regardless of the renderer's texture blend flips. Source revision:
    /// 2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed.
    pub fn sample(&self, point: Vec2) -> f32 {
        let integral = point.floor();
        let fraction = point - integral;
        let raw_sample = (integral + Vec2::splat(f32::from(self.border))).clamp(
            Vec2::ZERO,
            Vec2::new(
                f32::from(self.raw_width - 1),
                f32::from(self.raw_height - 1),
            ),
        );
        let integral = raw_sample.as_uvec2();
        let x = u16::try_from(integral.x).unwrap_or(self.raw_width - 1);
        let y = u16::try_from(integral.y).unwrap_or(self.raw_height - 1);
        // Original getClipHeight returns a clamped raw sample near outer edges.
        if x < 1
            || y < 1
            || x > self.raw_width.saturating_sub(3)
            || y > self.raw_height.saturating_sub(3)
        {
            return self.raw_height(x, y) * 0.0625;
        }
        let near_left = self.raw_height(x, y);
        let far_right = self.raw_height(x + 1, y + 1);
        let height = if fraction.y > fraction.x {
            let far_left = self.raw_height(x, y + 1);
            let vertical = (1.0 - fraction.y) * (near_left - far_left);
            let horizontal = fraction.x * (far_right - far_left);
            far_left + vertical + horizontal
        } else {
            let near_right = self.raw_height(x + 1, y);
            let vertical = fraction.y * (far_right - near_right);
            let horizontal = (1.0 - fraction.x) * (near_left - near_right);
            near_right + vertical + horizontal
        };
        height * 0.0625
    }

    /// Original smoothed `getHeightMapHeight` normal and heading-zero
    /// `TerrainLogic::makeAlignToNormalMatrix`, in the renderer's basis.
    pub fn aligned_transform(&self, point: Vec2) -> Transform {
        let normal = self.ground_normal(point);
        let forward = Vec3::new(1.0, -normal.x / normal.y, 0.0).normalize();
        Transform::from_translation(Vec3::new(point.x, self.sample(point), -point.y)).with_rotation(
            Quat::from_mat3(&Mat3::from_cols(
                forward,
                normal,
                forward.cross(normal).normalize(),
            )),
        )
    }

    fn ground_normal(&self, point: Vec2) -> Vec3 {
        let integral = point.floor();
        let fraction = point - integral;
        let raw = integral + Vec2::splat(f32::from(self.border));
        if raw.min_element() < 1.0
            || raw.x > f32::from(self.raw_width - 3)
            || raw.y > f32::from(self.raw_height - 3)
        {
            return Vec3::Y;
        }
        let cell = raw.as_uvec2();
        let x = u16::try_from(cell.x).expect("bounded terrain coordinate");
        let y = u16::try_from(cell.y).expect("bounded terrain coordinate");
        let [d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d11] = [
            (x, y),
            (x + 1, y),
            (x + 1, y + 1),
            (x, y + 1),
            (x, y - 1),
            (x + 1, y - 1),
            (x + 2, y),
            (x + 2, y + 1),
            (x + 1, y + 2),
            (x, y + 2),
            (x - 1, y),
        ]
        .map(|(x, y)| self.raw_height(x, y));
        // Preserve source sample ordering (including its repeated d6-d0 term).
        let blend = |near: f32, far: f32, fraction: f32| {
            let near = near * (1.0 - fraction);
            let far = fraction * far;
            near + far
        };
        let interpolate = |values: [f32; 4]| {
            let left = blend(values[0], values[3], fraction.x);
            let right = blend(values[1], values[2], fraction.x);
            blend(left, right, fraction.y)
        };
        let dx = interpolate([d1 - d11, d6 - d0, d7 - d3, d6 - d0]);
        let dy = interpolate([d3 - d4, d2 - d5, d8 - d1, d9 - d0]);
        Vec3::new(-dx, 32.0, dy).normalize()
    }

    fn corners(&self, x: u16, y: u16) -> [[f32; 3]; 4] {
        [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)].map(|(x, y)| {
            [
                f32::from(x) - f32::from(self.border),
                self.raw_height(x, y) * 0.0625,
                f32::from(self.border) - f32::from(y),
            ]
        })
    }

    fn normal(&self, x: u16, y: u16) -> [f32; 3] {
        let dx = (self.raw_height((x + 1).min(self.raw_width - 1), y)
            - self.raw_height(x.saturating_sub(1), y))
            * 0.0625;
        let dy = (self.raw_height(x, (y + 1).min(self.raw_height - 1))
            - self.raw_height(x, y.saturating_sub(1)))
            * 0.0625;
        Vec3::new(-dx, 2.0, dy).normalize().to_array()
    }

    pub fn batches(&self) -> Vec<TerrainBatch> {
        let count = self.tiles.texture_classes.len();
        let mut batches: Vec<_> = (0..count * 3).map(|_| Batch::default()).collect();
        let mut opaque = BTreeMap::new();
        // The source draws the stored surrounding border as well as playable cells.
        for y in 0..self.raw_height - 1 {
            for x in 0..self.raw_width - 1 {
                self.append_cell(&mut batches, &mut opaque, x, y);
            }
        }
        let opaque = opaque
            .into_iter()
            .map(|((class, _, _), batch)| TerrainBatch {
                texture_class: self.tiles.texture_classes[class].name.clone(),
                pixel_extent: u32::from(self.tiles.texture_classes[class].width) * 64,
                layer: 0,
                mesh: batch.mesh(),
            });
        // Blended meshes retain their original grouping and AABB-based draw order.
        let blended = batches
            .into_iter()
            .enumerate()
            .filter_map(|(index, batch)| {
                if batch.positions.is_empty() {
                    return None;
                }
                let texture_class = self.tiles.texture_classes[index % count].name.clone();
                let layer = u8::try_from(index / count).expect("three terrain layers");
                Some(TerrainBatch {
                    texture_class,
                    pixel_extent: u32::from(self.tiles.texture_classes[index % count].width) * 64,
                    layer,
                    mesh: batch.mesh(),
                })
            });
        opaque.chain(blended).collect()
    }

    fn append_cell(
        &self,
        batches: &mut [Batch],
        opaque: &mut BTreeMap<(usize, u16, u16), Batch>,
        x: u16,
        y: u16,
    ) {
        let index = self.index(x, y);
        let primary = self.tiles.blends[usize::from(self.tiles.blend_tiles[index])].as_ref();
        let primary_tile = primary.map_or(self.tiles.base_tiles[index], |blend| blend.tile);
        let primary_uv = self
            .tiles
            .texture_uv(
                index,
                primary_tile,
                &self.heights,
                self.adjust_cliff_textures,
            )
            .expect("validated terrain cell");
        let positions = self.corners(x, y);
        let normals =
            [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)].map(|(x, y)| self.normal(x, y));
        let cliff_flip =
            (positions[0][1] - positions[2][1]).abs() > (positions[1][1] - positions[3][1]).abs();
        let flip = self.primary_flip(x, y, primary_uv.stretched);
        let base_uv = self
            .tiles
            .texture_uv(
                index,
                self.tiles.base_tiles[index],
                &self.heights,
                self.adjust_cliff_textures,
            )
            .expect("validated terrain cell");
        opaque
            .entry((
                base_uv.texture_class,
                y / VERTEX_BUFFER_TILE_LENGTH,
                x / VERTEX_BUFFER_TILE_LENGTH,
            ))
            .or_default()
            .append(positions, normals, base_uv.corners, [255; 4], flip);
        if let Some(primary) = primary {
            let (alpha, _) = primary.alpha_and_flip();
            batches[self.tiles.texture_classes.len() + primary_uv.texture_class].append(
                positions,
                normals,
                primary_uv.corners,
                alpha,
                flip,
            );
        }
        if self.use_three_way_blends
            && let Some(extra) =
                self.tiles.blends[usize::from(self.tiles.extra_blend_tiles[index])].as_ref()
        {
            let uv = self
                .tiles
                .texture_uv(index, extra.tile, &self.heights, self.adjust_cliff_textures)
                .expect("validated terrain cell");
            let (alpha, extra_flip) = extra.alpha_and_flip();
            batches[2 * self.tiles.texture_classes.len() + uv.texture_class].append(
                positions,
                normals,
                uv.corners,
                alpha,
                extra_flip || uv.stretched && cliff_flip,
            );
        }
    }

    fn primary_flip(&self, x: u16, y: u16, stretched: bool) -> bool {
        let index = self.index(x, y);
        let primary = self.tiles.blends[usize::from(self.tiles.blend_tiles[index])].as_ref();
        if stretched {
            let positions = self.corners(x, y);
            return (positions[0][1] - positions[2][1]).abs()
                > (positions[1][1] - positions[3][1]).abs();
        }
        if !self.use_three_way_blends
            && primary.is_some_and(|blend| blend.directions[0] || blend.directions[1])
        {
            // Source clears FLIPPED_MASK before evaluating independent directions.
            let blend = primary.expect("axis blend");
            (blend.directions[2] && blend.inverted & 1 == 0)
                || (blend.directions[3] && blend.inverted & 1 != 0)
        } else {
            primary.is_some_and(|blend| blend.alpha_and_flip().1)
        }
    }

    pub fn ray_point(&self, ray: Ray3d) -> Option<Vec2> {
        let mut previous = ray.origin;
        for step in 1..=4096_u16 {
            let point = ray.get_point(f32::from(step) * 0.25);
            let ground = Vec2::new(point.x, -point.z);
            if ground.x < 0.0
                || ground.y < 0.0
                || ground.x >= f32::from(self.width)
                || ground.y >= f32::from(self.height)
            {
                previous = point;
                continue;
            }
            if point.y <= self.sample(ground) {
                let mut above = previous;
                let mut below = point;
                for _ in 0..10 {
                    let middle = (above + below) * 0.5;
                    if middle.y > self.sample(Vec2::new(middle.x, -middle.z)) {
                        above = middle;
                    } else {
                        below = middle;
                    }
                }
                return Some(Vec2::new(below.x, -below.z));
            }
            previous = point;
        }
        None
    }
}

#[derive(Default)]
struct Batch {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Batch {
    fn append(
        &mut self,
        positions: [[f32; 3]; 4],
        normals: [[f32; 3]; 4],
        uv: [[f32; 2]; 4],
        alpha: [u8; 4],
        flip: bool,
    ) {
        let offset = u32::try_from(self.positions.len()).expect("bounded terrain mesh");
        self.positions.extend(positions);
        self.normals.extend(normals);
        self.uv.extend(uv);
        self.colors
            .extend(alpha.map(|alpha| [1.0, 1.0, 1.0, f32::from(alpha) / 255.0]));
        let indices = if flip {
            [1, 3, 0, 1, 2, 3]
        } else {
            [0, 2, 3, 0, 1, 2]
        };
        self.indices.extend(indices.map(|index| index + offset));
    }

    fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::map::TextureClass;
    use generals_sim::{Cell, terrain::CellType};

    pub(super) fn terrain() -> Result<Terrain, Box<dyn Error>> {
        terrain_with_boundaries(vec![[5, 4]])
    }

    fn terrain_with_boundaries(boundaries: Vec<[i32; 2]>) -> Result<Terrain, Box<dyn Error>> {
        let map = HeightMap {
            width: 8,
            height: 8,
            border: 2,
            // Saved boundary deliberately differs from raw width minus borders.
            boundaries,
            heights: vec![0; 64],
        };
        let tiles = TerrainTiles {
            width: 8,
            height: 8,
            base_tiles: vec![0; 64],
            blend_tiles: vec![0; 64],
            extra_blend_tiles: vec![0; 64],
            cliff_tiles: vec![0; 64],
            cliff_flags: vec![0; 8],
            texture_classes: vec![TextureClass {
                name: "SourceSand".to_owned(),
                first_tile: 0,
                tile_count: 1,
                width: 1,
            }],
            edge_classes: Vec::new(),
            blends: vec![None],
            cliffs: vec![None],
        };
        Terrain::new(map, tiles, true, true)
    }

    #[test]
    fn signed_inactive_boundaries_do_not_enlarge_native_path_extent() -> Result<(), Box<dyn Error>>
    {
        // Original Final Crusade's inactive boundary is (-6, 2). Native
        // W3DTerrainLogic::getMaximumPathfindExtent compares signed values.
        let terrain = terrain_with_boundaries(vec![[5, 4], [-6, 2], [1, 1]])?;
        assert_eq!(terrain.path_extent, [5, 4]);
        assert!(terrain.classify_movement(&[], &[]).is_ok());
        let larger = terrain_with_boundaries(vec![[5, 4], [6, 4]])?;
        assert_eq!(
            larger
                .classify_movement(&[], &[])
                .err()
                .unwrap()
                .to_string(),
            "Movement over a map with a larger inactive boundary is not ported."
        );
        Ok(())
    }

    #[test]
    fn native_origin_border_and_saved_boundary_control_picking() -> Result<(), Box<dyn Error>> {
        let mut terrain = terrain()?;
        assert_eq!((terrain.width, terrain.height), (5, 4));
        terrain.heights[18] = 16; // raw (2,2) is native world origin.
        assert!((terrain.sample(Vec2::ZERO) - 1.0).abs() < f32::EPSILON);
        let ray = Ray3d::new(Vec3::new(4.75, 10.0, -1.75), Dir3::NEG_Y);
        assert_eq!(terrain.ray_point(ray), Some(Vec2::new(4.75, 1.75)));
        let outside = Ray3d::new(Vec3::new(5.25, 10.0, -1.75), Dir3::NEG_Y);
        assert_eq!(terrain.ray_point(outside), None);
        terrain.tiles.cliff_flags[3] = 1 << 6; // native (4,1) -> raw (6,3)
        let classified = terrain.classify_movement(&[], &[])?;
        assert!(matches!(
            classified.cell(Cell { x: 4, y: 1 }).unwrap().kind,
            CellType::Cliff
        ));
        assert!(matches!(
            classified.cell(Cell { x: 3, y: 1 }).unwrap().kind,
            CellType::Cliff
        ));
        assert!(matches!(
            classified.cell(Cell { x: 2, y: 1 }).unwrap().kind,
            CellType::Clear
        ));
        Ok(())
    }

    #[test]
    fn a_single_submerged_corner_classifies_all_four_touching_cells() -> Result<(), Box<dyn Error>>
    {
        use generals_assets::map::PolygonTrigger;
        let mut terrain = terrain()?;
        terrain.heights[27] = 16; // native (10,10) has source height 10.
        let mut water = PolygonTrigger {
            name: String::new(),
            layer: String::new(),
            id: 0,
            water: true,
            river: false,
            river_start: 0,
            points: vec![[0, 0, 10], [10, 0, 10], [10, 10, 10], [0, 10, 10]],
        };
        let dry = terrain.classify_movement(std::slice::from_ref(&water), &[])?;
        assert_eq!(dry.blocked_ground_cells().count(), 0);
        water.points[0][2] = 11; // Source uses only the first vertex's Z.
        let wet = terrain.classify_movement(&[water], &[])?;
        assert_eq!(wet.blocked_ground_cells().count(), 4);
        assert!(matches!(
            wet.cell(Cell { x: 1, y: 1 }).unwrap().kind,
            CellType::Water
        ));
        assert!(matches!(
            wet.cell(Cell { x: 2, y: 1 }).unwrap().kind,
            CellType::Clear
        ));
        Ok(())
    }

    #[test]
    fn source_fixed_ground_diagonal_and_rotated_mesh_face_up() -> Result<(), Box<dyn Error>> {
        let mut terrain = terrain()?;
        terrain.heights[27] = 16; // raw far-right corner of source origin cell.
        assert!((terrain.sample(Vec2::new(0.5, 0.5)) - 0.5).abs() < f32::EPSILON);
        assert!((terrain.sample(Vec2::new(0.75, 0.25)) - 0.25).abs() < f32::EPSILON);
        assert!((terrain.sample(Vec2::new(0.25, 0.75)) - 0.25).abs() < f32::EPSILON);
        let batches = terrain.batches();
        assert_eq!(batches.len(), 1);
        let positions = batches[0]
            .mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("positions");
        let bevy::mesh::VertexAttributeValues::Float32x3(positions) = positions else {
            panic!("position format");
        };
        assert_eq!(
            positions[0].map(f32::to_bits),
            [-2.0_f32, 0.0, 2.0].map(f32::to_bits)
        );
        let a = Vec3::from_array(positions[0]);
        let b = Vec3::from_array(positions[2]);
        let c = Vec3::from_array(positions[3]);
        assert!((b - a).cross(c - a).y > 0.0);
        Ok(())
    }

    #[test]
    fn move_hint_alignment_keeps_heading_and_follows_source_ground_normal()
    -> Result<(), Box<dyn Error>> {
        let mut terrain = terrain()?;
        for y in 0..8_u16 {
            for x in 0..8_u16 {
                terrain.heights[usize::from(y) * 8 + usize::from(x)] = x * 8;
            }
        }
        let point = Vec2::new(1.25, 1.5);
        let transform = terrain.aligned_transform(point);
        let normal = Vec3::new(-0.5, 1.0, 0.0).normalize();
        assert!((transform.rotation * Vec3::Y).distance(normal) < 0.0001);
        let forward = transform.rotation * Vec3::X;
        assert!(forward.z.abs() < 0.0001);
        assert!(forward.y > 0.0);
        assert!((transform.translation.y - terrain.sample(point)).abs() < f32::EPSILON);
        Ok(())
    }
}

#[cfg(test)]
mod batching_tests;
