//! PartitionManager.cpp3795–4060, default `FindPositionOptions`.
use crate::collision_geometry::{Body, Geometry, Shape};
use crate::grid::LayerCell;
use crate::skirmish::{Skirmish, UnitAiUpdate, partition::Identity, startup};
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
        self.find_ground_position_around(center, maximum, start_angle, |game, position, angle| {
            (position[2] - center[2]).abs() <= 1e10 && game.default_position_clear(position, angle)
        })
    }

    fn find_ground_position_around(
        &mut self,
        center: [f32; 3],
        maximum: f32,
        start_angle: Option<f32>,
        accept: impl Fn(&Self, [f32; 3], f32) -> bool,
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
                    if accept(self, position, angle) {
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
        self.ground_position_clear(position, angle, [None; 2])
    }

    fn ground_position_clear(
        &self,
        position: [f32; 3],
        angle: f32,
        ignored: [Option<Identity>; 2],
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
        self.native_potential_collisions(&probe, true)
            .into_iter()
            .all(|object| ignored.contains(&Some(object)))
    }

    // DockUpdate::computeApproachPosition and DozerAIUpdate::findGoodBuildOrRepairPosition.
    pub(super) fn find_dock_approach_position(
        &mut self,
        id: usize,
        target: usize,
        working: [f32; 3],
        ground_max_z: Option<f32>,
    ) -> [f32; 3] {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let airborne = actor.is_using_airborne_locomotor(unit);
        let max_z = if airborne { None } else { ground_max_z }.unwrap_or(1e10);
        let ignored = [
            Some(Identity::Actor(id)),
            airborne.then_some(Identity::Actor(target)),
        ];
        let source = self.dock_query_node(self.object_position(id));
        let surfaces = actor.locomotor_surfaces(unit);
        let has_ai = !matches!(unit.ai.update, UnitAiUpdate::Absent);
        self.find_ground_position_around(working, 100.0, None, |game, position, angle| {
            (position[2] - working[2]).abs() <= max_z
                && game.ground_position_clear(position, angle, ignored)
                && (!has_ai
                    || game.navigation_for(id).quick_path_exists_layers(
                        surfaces,
                        source,
                        game.dock_query_node(position),
                    ))
        })
        .unwrap_or(working)
    }

    fn dock_query_node(&self, position: [f32; 3]) -> LayerCell {
        let point = [position[0], position[1]];
        LayerCell {
            cell: self.clipped_path_cell(point),
            layer: self
                .movement_terrain
                .get_layer_for_destination(position, self.ground_height(point)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{DeathType, Kind, tests::game};
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
    fn dock_approach_ignores_high_overlaps_but_keeps_registered_corpses() {
        let mut game = game();
        let dock = game.supplies[0].object;
        let kind = game.type_for(0, Kind::Builder).unwrap();
        let mover = game.spawn(kind, 0, [305.0, 255.0], true);
        let obstacle = game.spawn(5, 0, [355.0, 255.0], true);
        let center = [355.0_f32, 255.0, 0.0];
        game.set_object_position(obstacle, [355.0, 255.0, 60.0]);
        game.update_ground_contacts();
        assert_eq!(
            game.find_native_dock_position(mover, dock, center)
                .map(f32::to_bits),
            center.map(f32::to_bits)
        );

        game.set_object_position(obstacle, center);
        game.kill_object(obstacle, DeathType::Normal);
        game.update_ground_contacts();
        assert_ne!(
            game.find_native_dock_position(mover, dock, center)
                .map(f32::to_bits),
            center.map(f32::to_bits)
        );
    }

    #[test]
    fn dock_approach_uses_terrain_zones_without_a_mobile_goal_claim() {
        let mut game = game();
        let dock = game.supplies[0].object;
        let kind = game.type_for(0, Kind::Builder).unwrap();
        let mover = game.spawn(kind, 0, [305.0, 255.0], true);
        let other = game.spawn(kind, 0, [305.0, 325.0], true);
        let center = [355.0_f32, 255.0, 0.0];
        let goal = [center[0], center[1]];
        game.reserve_goal_on(other, goal, 1);
        assert!(game.movement_path(mover, [305.0, 255.0], goal).is_none());
        assert_eq!(
            game.find_native_dock_position(mover, dock, center)
                .map(f32::to_bits),
            center.map(f32::to_bits)
        );
    }

    #[test]
    fn dock_approach_air_locomotor_ignores_only_its_actual_target() {
        let mut game = game();
        let mover_type = game.type_for(0, Kind::Builder).unwrap();
        game.catalog[mover_type].locomotor.legal_surfaces = 8;
        let mover = game.spawn(mover_type, 0, [305.0, 255.0], true);
        let center = [355.0_f32, 255.0, 0.0];
        let dock = game.spawn(0, 0, [center[0], center[1]], true);
        game.update_ground_contacts();
        assert_eq!(
            game.find_native_dock_position(mover, dock, center)
                .map(f32::to_bits),
            center.map(f32::to_bits)
        );

        game.spawn(5, 0, [center[0], center[1]], true);
        game.update_ground_contacts();
        assert_ne!(
            game.find_native_dock_position(mover, dock, center)
                .map(f32::to_bits),
            center.map(f32::to_bits)
        );
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
