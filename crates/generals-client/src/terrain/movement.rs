//! Original ground-cell classification and polygon-water corner sampling.
//! `SuperHackers` 2bfaea60: `AIPathfind.cpp::classifyMapCell`,
//! `TerrainLogic.cpp::getWaterHandle` and `PolygonTrigger.cpp::pointInTrigger`.

use super::Terrain;
use generals_assets::{
    HeightMap,
    map::{MapObject, PolygonTrigger},
};
use generals_sim::{
    Cell,
    terrain::{TerrainMap, WaterPolygon, polygon_contains as inside},
};
use std::error::Error;

impl Terrain {
    pub fn configure_build_restrictions(
        &self,
        assets: &generals_assets::virtual_assets::VirtualAssets,
        terrain: &mut TerrainMap,
    ) -> Result<(), Box<dyn Error>> {
        let source = generals_assets::content::scan(assets.read("data/ini/terrain.ini")?)?;
        let mut restricted = Vec::new();
        for class in &self.tiles.texture_classes {
            let definition = source
                .declarations
                .iter()
                .find(|item| item.kind == "Terrain" && item.name == class.name)
                .ok_or("Map terrain class missing original definition")?;
            let value = crate::skirmish_data::field(&definition.lines, 0, "RestrictConstruction")
                .map(crate::skirmish_data::scalar)
                .transpose()?;
            restricted.push(match value {
                None | Some("No") => false,
                Some("Yes") => true,
                _ => return Err("Malformed terrain RestrictConstruction".into()),
            });
        }
        let mut cells = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let index = usize::from(y + self.border) * usize::from(self.raw_width)
                    + usize::from(x + self.border);
                let tile = self.tiles.base_tiles[index] >> 2;
                let class = self
                    .tiles
                    .texture_classes
                    .iter()
                    .position(|class| {
                        tile >= class.first_tile && tile - class.first_tile < class.tile_count
                    })
                    .ok_or("Map tile has no original terrain class")?;
                if restricted[class] {
                    cells.push(Cell { x, y });
                }
            }
        }
        terrain.set_build_restrictions(cells);
        Ok(())
    }

    pub fn classify_movement(
        &self,
        polygons: &[PolygonTrigger],
        objects: &[MapObject],
    ) -> Result<TerrainMap, Box<dyn Error>> {
        if self.path_extent != [i32::from(self.width), i32::from(self.height)] {
            return Err(
                "Movement over a map with a larger inactive boundary is not ported.".into(),
            );
        }
        if objects
            .iter()
            .any(|object| object.waypoint_name() == Some("WaveGuide1"))
        {
            return Err("Grid-water movement classification is not ported.".into());
        }
        let corners_width = usize::from(self.width) + 1;
        let mut underwater = Vec::with_capacity(corners_width * (usize::from(self.height) + 1));
        for y in 0..=self.height {
            for x in 0..=self.width {
                let point = [i32::from(x) * 10, i32::from(y) * 10];
                let ground =
                    self.raw_height(x + self.border, y + self.border) * HeightMap::HEIGHT_SCALE;
                underwater.push(water_height(polygons, point)?.is_some_and(|water| ground < water));
            }
        }
        let mut cliffs = Vec::new();
        let mut water = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = Cell { x, y };
                if self
                    .tiles
                    .is_cliff(u32::from(x + self.border), u32::from(y + self.border))
                {
                    cliffs.push(cell);
                }
                let index = usize::from(y) * corners_width + usize::from(x);
                if [
                    index,
                    index + corners_width,
                    index + corners_width + 1,
                    index + 1,
                ]
                .into_iter()
                .any(|corner| underwater[corner])
                {
                    water.push(cell);
                }
            }
        }
        let mut terrain = TerrainMap::classify(self.width, self.height, cliffs, water)?;
        terrain.set_ground_heights(|position| {
            self.sample(bevy::prelude::Vec2::from_array(position) * 0.1) * 10.0
        });
        let water_polygons = polygons
            .iter()
            .filter(|polygon| polygon.water)
            .map(|polygon| WaterPolygon::new(polygon.points.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        terrain.set_water_polygons(water_polygons);
        Ok(terrain)
    }
}

pub(super) fn water_height(
    polygons: &[PolygonTrigger],
    point: [i32; 2],
) -> Result<Option<f32>, &'static str> {
    let mut water = None;
    let mut highest = 0.0;
    for polygon in polygons.iter().filter(|polygon| polygon.water) {
        if inside(&polygon.points, point)? {
            let height = source_real(polygon.points[0][2]);
            if height >= highest {
                highest = height;
                water = Some(height);
            }
        }
    }
    Ok(water)
}

fn source_real(value: i32) -> f32 {
    let upper = i16::try_from(value >> 16).expect("signed upper word");
    let lower = u16::try_from(value & 0xffff).expect("unsigned lower word");
    let high = f32::from(upper) * 65_536.0;
    high + f32::from(lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polygon(height: i32) -> PolygonTrigger {
        PolygonTrigger {
            name: String::new(),
            layer: String::new(),
            id: 0,
            water: true,
            river: false,
            river_start: 0,
            points: vec![
                [0, 0, height],
                [20, 0, height],
                [20, 20, height],
                [0, 20, height],
            ],
        }
    }

    #[test]
    fn native_polygon_boundary_and_highest_water_rules() {
        let water = polygon(10);
        assert!(!inside(&water.points, [10, 0]).unwrap());
        assert!(inside(&water.points, [10, 20]).unwrap());
        assert!(!inside(&water.points, [0, 10]).unwrap());
        assert!(inside(&water.points, [20, 10]).unwrap());
        let sloped = [[0, 0, 0], [9, 4, 0], [0, 4, 0]];
        assert!(inside(&sloped, [4, 2]).unwrap());
        assert!(!inside(&sloped, [5, 2]).unwrap());
        assert!(water_height(&[polygon(-5)], [10, 10]).unwrap().is_none());
        assert_eq!(
            water_height(&[polygon(10), polygon(20), polygon(15)], [10, 10])
                .unwrap()
                .map(f32::to_bits),
            Some(20.0_f32.to_bits())
        );
    }

    #[test]
    fn original_signed_integer_intersection_overflow_is_rejected() {
        let points = [[0, 0, 0], [100_000, 100_000, 0], [0, 100_000, 0]];
        assert!(inside(&points, [1, 50_000]).is_err());
    }
}
