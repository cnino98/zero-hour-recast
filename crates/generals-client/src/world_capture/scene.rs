use crate::terrain::Terrain;
use bevy::prelude::*;
use generals_assets::map::{MapObject, PolygonTrigger};
use generals_sim::terrain::{TerrainMap, WaterPolygon, water_height};
use num_traits::ToPrimitive;
use std::error::Error;

#[derive(Resource)]
pub struct Scene {
    pub bridges: Vec<(u8, [[f32; 3]; 2])>,
    pub shores: Vec<[f32; 3]>,
    pub bridge_staging: Vec<(u8, [[f32; 3]; 2])>,
    pub roads: Vec<[f32; 2]>,
}

impl Scene {
    pub fn load(
        records: &[MapObject],
        polygons: &[PolygonTrigger],
        terrain: &Terrain,
        movement: &TerrainMap,
    ) -> Result<Self, Box<dyn Error>> {
        let water: Vec<_> = polygons
            .iter()
            .filter(|polygon| polygon.water || polygon.river)
            .map(|polygon| WaterPolygon::new(polygon.points.clone()))
            .collect::<Result<_, _>>()?;
        let mut shores = Vec::new();
        for y in (2..terrain.height.saturating_sub(2)).step_by(3) {
            for x in (2..terrain.width.saturating_sub(2)).step_by(3) {
                let native_x = f32::from(x) * 10.0;
                let native_y = f32::from(y) * 10.0;
                let point = [native_x + 5.0, native_y + 5.0];
                let ground = terrain.sample(Vec2::from_array(point) * 0.1) * 10.0;
                if water_height(&water, point)?.is_some_and(|height| height > ground) {
                    continue;
                }
                for delta in [[30.0, 0.0], [-30.0, 0.0], [0.0, 30.0], [0.0, -30.0]] {
                    let next = [point[0] + delta[0], point[1] + delta[1]];
                    let floor = terrain.sample(Vec2::from_array(next) * 0.1) * 10.0;
                    if water_height(&water, next)?.is_some_and(|height| height > floor + 1.0) {
                        shores.push([point[0], point[1], ground]);
                        break;
                    }
                }
            }
        }
        let bridges: Vec<_> = movement
            .bridge_layers()
            .filter_map(|layer| {
                let ends = movement.bridge_endpoint_cells(layer)?;
                Some((
                    layer,
                    ends.map(|cell| {
                        let native_x = f32::from(cell.x) * 10.0;
                        let native_y = f32::from(cell.y) * 10.0;
                        let point = [native_x + 5.0, native_y + 5.0];
                        [
                            point[0],
                            point[1],
                            terrain.sample(Vec2::from_array(point) * 0.1) * 10.0,
                        ]
                    }),
                ))
            })
            .collect();
        let bridge_staging = bridges
            .iter()
            .filter_map(|&(layer, ends)| {
                let direction =
                    Vec2::new(ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]).normalize_or_zero();
                if direction == Vec2::ZERO {
                    return None;
                }
                // Keep the complete arrival neighborhood off the original deck. The
                // native connection cells alone do not establish a walk-off target.
                (60_u16..=160).step_by(20).find_map(|distance| {
                    let points = [
                        Vec2::new(ends[0][0], ends[0][1]) - direction * f32::from(distance),
                        Vec2::new(ends[1][0], ends[1][1]) + direction * f32::from(distance),
                    ];
                    points
                        .iter()
                        .all(|&point| ground_neighborhood(movement, layer, point))
                        .then(|| {
                            (
                                layer,
                                points.map(|point| {
                                    [point.x, point.y, terrain.sample(point * 0.1) * 10.0]
                                }),
                            )
                        })
                })
            })
            .collect();
        let roads = records
            .iter()
            .filter(|object| object.flags & 2 != 0)
            .map(|object| [object.position[0], object.position[1]])
            .collect();
        Ok(Self {
            bridges,
            shores,
            bridge_staging,
            roads,
        })
    }
}

fn ground_neighborhood(movement: &TerrainMap, layer: u8, point: Vec2) -> bool {
    (-30_i16..=30).step_by(10).all(|dy| {
        (-30_i16..=30).step_by(10).all(|dx| {
            let point = point + Vec2::new(f32::from(dx), f32::from(dy));
            if point.x < 0.0 || point.y < 0.0 {
                return false;
            }
            let (Some(x), Some(y)) = (
                (point.x / 10.0).floor().to_u16(),
                (point.y / 10.0).floor().to_u16(),
            ) else {
                return false;
            };
            let cell = generals_sim::Cell { x, y };
            movement.cell(cell).is_some()
                && movement.bridge_cell(layer, cell).is_none()
                && !movement.bridge_contains(layer, point.to_array())
        })
    })
}
