//! PartitionManager.cpp3795–4060, default `FindPositionOptions`.
use crate::collision_geometry::{Body, Geometry, Shape};
use crate::skirmish::{Skirmish, startup};
use crate::terrain::CellType;
use num_traits::ToPrimitive;
use std::f32::consts::TAU;

impl Skirmish {
    pub(in crate::skirmish) fn find_default_position_around(
        &mut self,
        center: [f32; 3],
        maximum: f32,
        start_angle: Option<f32>,
    ) -> Option<[f32; 3]> {
        if self.cell_at([center[0], center[1]]).is_none() {
            return Some(center);
        }
        let start = start_angle.unwrap_or_else(|| startup::random_angle(&mut self.random));
        let mut distance = 0.0;
        loop {
            if !matches!(
                distance.partial_cmp(&maximum),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ) {
                break;
            }
            let spacing = if distance == 0.0 {
                TAU
            } else {
                (5.0 / (distance + 1.0)) * (TAU / 6.0)
            };
            let samples = crate::native_real::ceil((TAU / spacing) / 2.0);
            for index in 0..samples {
                let offset = spacing * index.to_f32().expect("native search sample");
                for angle in [start + offset, start - offset]
                    .into_iter()
                    .take(if index == 0 { 1 } else { 2 })
                {
                    let dx = distance * angle.cos();
                    let dy = distance * angle.sin();
                    let point = [center[0] + dx, center[1] + dy];
                    let position = [point[0], point[1], self.ground_height(point)];
                    if (position[2] - center[2]).abs() <= 1e10
                        && self.default_position_clear(position, angle)
                    {
                        return Some(position);
                    }
                }
            }
            distance += 5.0;
        }
        None
    }

    pub(in crate::skirmish) fn default_position_clear(
        &self,
        position: [f32; 3],
        angle: f32,
    ) -> bool {
        let point = [position[0], position[1]];
        let Some(cell) = self.cell_at(point) else {
            return false;
        };
        if self
            .movement_terrain
            .cell(cell)
            .is_none_or(|cell| matches!(cell.kind, CellType::Cliff | CellType::Impassable))
        {
            return false;
        }
        if self
            .movement_terrain
            .parachute_water_height(point, position[2])
            .is_some()
        {
            return false;
        }
        let geometry = Geometry {
            shape: Shape::Sphere,
            is_small: true,
            major: 5.0,
            minor: 5.0,
            height: 5.0,
        };
        let probe = Body {
            geometry: &geometry,
            position,
            angle,
        };
        self.native_potential_collisions(&probe, true).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{DeathType, tests::game};
    use crate::terrain::WaterPolygon;

    #[test]
    fn default_search_uses_live_geometry_and_registered_dead_bodies() {
        let mut game = game();
        let center = [355.0, 255.0, 0.0];
        let obstacle = game.spawn(5, 0, [375.0, 255.0], true);
        game.set_dynamic_geometry(obstacle, [10.0, 2.0, 2.0]);
        game.update_ground_contacts();
        assert!(game.default_position_clear(center, 0.0));
        game.set_dynamic_geometry(obstacle, [10.0, 25.0, 25.0]);
        game.update_ground_contacts();
        assert!(!game.default_position_clear(center, 0.0));
        game.kill_object(obstacle, DeathType::Normal);
        game.update_ground_contacts();
        assert!(!game.default_position_clear(center, 0.0));
        game.set_object_position(obstacle, [375.0, 255.0, 6.0]);
        game.update_ground_contacts();
        assert!(game.default_position_clear(center, 0.0));
    }

    #[test]
    fn default_search_uses_polygon_water_and_preserves_off_map_xyz_without_rng() {
        let mut game = game();
        let center = [355.0, 255.0, 0.0];
        assert!(game.default_position_clear(center, 0.0));
        game.movement_terrain.set_water_polygons(vec![
            WaterPolygon::new(vec![
                [350, 250, 2],
                [360, 250, 2],
                [360, 260, 2],
                [350, 260, 2],
            ])
            .unwrap(),
        ]);
        assert!(!game.default_position_clear(center, 0.0));
        let random = game.random;
        let outside = [-10.0_f32, -10.0, 75.0];
        assert_eq!(
            game.find_default_position_around(outside, 100.0, None)
                .unwrap()
                .map(f32::to_bits),
            outside.map(f32::to_bits)
        );
        assert_eq!(game.random, random);
    }
}
