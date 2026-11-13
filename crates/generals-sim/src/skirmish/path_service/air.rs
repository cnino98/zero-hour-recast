//! AIUpdate.cpp1616–1664,1782–1970; Weapon.cpp2093–2220; AIPathfind.cpp5549–5628.
use super::{Kind, Request};
use crate::skirmish::{
    CELL_SIZE, Skirmish,
    path_following::{Node, Path},
};

impl Skirmish {
    pub(super) fn compute_quick_air_path(&mut self, id: usize, destination: [f32; 3]) {
        let retained = self.entities[id]
            .ai
            .ground_path
            .as_ref()
            .and_then(Path::last_position)
            .is_some_and(|end| {
                let delta = std::array::from_fn::<_, 3, _>(|axis| destination[axis] - end[axis]);
                let squared = delta.map(|component| component * component);
                squared[0] + squared[1] + squared[2] < 0.25
            });
        self.entities[id].ai.path_request.cancel();
        if retained {
            return;
        }
        let path = if self.attack_kind(id, "AIRCRAFT") && !self.attack_kind(id, "PROJECTILE") {
            self.native_aircraft_path(id, destination, None)
        } else {
            let mut start = self.object_position(id);
            start[2] = destination[2];
            Path::new(vec![
                Node {
                    position: start,
                    layer: self.entities[id].layer,
                    previous_layer: 1,
                    can_optimize: false,
                },
                Node {
                    position: destination,
                    layer: 1,
                    previous_layer: self.entities[id].layer,
                    can_optimize: false,
                },
            ])
        };
        self.install_air_path(id, path, destination);
    }

    fn install_air_path(&mut self, id: usize, path: Path, destination: [f32; 3]) {
        let actor = &mut self.entities[id];
        actor.ai.path.clear();
        actor.ai.ground_path = Some(path);
        actor.ai.motion.clear_goal();
        actor.ai.path_endpoint = Some([destination[0], destination[1]]);
        actor.ai.path_request.last_goal = Some([destination[0], destination[1]]);
        actor.ai.path_request.last_endpoint = actor
            .ai
            .ground_path
            .as_ref()
            .and_then(Path::last_position)
            .map(|point| [point[0], point[1]]);
        actor.ai.path_request.failed = false;
        actor.ai.blocking.path_frame = self.tick;
        actor.ai.blocking.frames = 0;
        actor.ai.blocking.stuck = false;
    }

    pub(super) fn compute_air_attack_path(&mut self, id: usize, request: &Request) -> Option<bool> {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let Kind::Attack(target) = request.kind else {
            return None;
        };
        if actor.locomotor_surfaces(unit) & 8 == 0 {
            return None;
        }
        if self.tick >= 2
            && actor.ai.blocking.path_frame >= self.tick - 2
            && actor.ai.ground_path.is_some()
            && actor.ai.blocking.stuck
        {
            let actor = &mut self.entities[id];
            actor.ai.blocking.ignore_until = self.tick + 60;
            actor.ai.blocking.frames = 0;
            actor.ai.blocking.blocked = false;
            actor.ai.blocking.stuck = false;
            return Some(true);
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if actor.active_weapon(unit).contact() && !self.attack_ground_movement(id) {
            return Some(self.compute_air_contact_path(id, target, request));
        }
        if self.height_above_ground(id) <= 0.0
            || self.entities[id]
                .active_weapon(&self.catalog[self.entities[id].type_index])
                .contact()
        {
            return None;
        }
        let victim = self
            .entities
            .get(target)
            .filter(|target| target.exists())
            .map(|_| target);
        let mut position = victim.map_or(request.goal, |target| self.object_position(target));
        let layer = victim.map_or(1, |target| self.entities[target].layer);
        position[2] = self.movement_layer_height(layer, [position[0], position[1]]);
        let destination = self.air_weapon_approach_target(id, victim, position);
        if self.entities[id]
            .ai
            .ground_path
            .as_ref()
            .is_some_and(|path| {
                path.is_direct()
                    && path.last_position().is_some_and(|end| {
                        let x = end[0] - destination[0];
                        let y = end[1] - destination[1];
                        let squared = [x, y].map(|component| component * component);
                        squared[0] + squared[1] < 0.25
                    })
            })
        {
            self.reserve_goal_on(id, [destination[0], destination[1]], 1);
            return Some(true);
        }
        let mut start = self.object_position(id);
        start[2] = destination[2];
        let path = Path::new(vec![
            Node {
                position: start,
                layer: 1,
                previous_layer: 1,
                can_optimize: false,
            },
            Node {
                position: destination,
                layer: 1,
                previous_layer: 1,
                can_optimize: false,
            },
        ]);
        self.install_air_path(id, path, destination);
        self.reserve_goal_on(id, [destination[0], destination[1]], 1);
        Some(true)
    }

    fn compute_air_contact_path(&mut self, id: usize, target: usize, request: &Request) -> bool {
        self.entities[id].ai.motion.set_no_slow_down(true);
        self.compute_quick_air_path(id, request.goal);
        let endpoint = self.entities[id]
            .ai
            .ground_path
            .as_ref()
            .and_then(Path::last_position)
            .expect("quick path");
        let x = request.goal[0] - endpoint[0];
        let y = request.goal[1] - endpoint[1];
        // The quick path's final point is replaced when it is within three cells.
        let squared = [x, y].map(|component| component * component);
        if squared[0] + squared[1] < 900.0 {
            let layer = self.movement_terrain.get_layer_for_destination(
                request.goal,
                self.ground_height([request.goal[0], request.goal[1]]),
            );
            self.entities[id]
                .ai
                .ground_path
                .as_mut()
                .expect("quick path")
                .update_last_position(request.goal, layer);
        }
        let endpoint = self.entities[id]
            .ai
            .ground_path
            .as_ref()
            .and_then(Path::last_position)
            .expect("quick path");
        let x = self.entities[id].position[0] - endpoint[0];
        let y = self.entities[id].position[1] - endpoint[1];
        let squared = [x, y].map(|component| component * component);
        if squared[0] + squared[1] < 100.0 {
            self.entities[id].ai.ground_path = None;
            let goal = self
                .entities
                .get(target)
                .filter(|target| target.exists())
                .map_or(request.goal, |_| self.object_position(target));
            if self
                .entities
                .get(target)
                .is_some_and(crate::skirmish::Entity::exists)
            {
                self.entities[id].ai.ignored_producer = Some(target);
            }
            self.compute_quick_air_path(id, goal);
            return false;
        }
        self.reserve_goal_on(id, [endpoint[0], endpoint[1]], 1);
        true
    }

    fn air_weapon_approach_target(
        &self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
    ) -> [f32; 3] {
        let source = self.object_position(id);
        let target_position = target.map_or(position, |target| self.object_position(target));
        let x = source[0] - target_position[0];
        let y = source[1] - target_position[1];
        let squared = [x, y].map(|component| component * component);
        let length = (squared[0] + squared[1]).sqrt();
        let radii =
            self.object_radius(id) + target.map_or(0.0, |target| self.object_radius(target));
        let shrink = if radii > 0.0 {
            if length <= radii {
                0.0
            } else {
                (length - radii) / length
            }
        } else {
            1.0
        };
        let boundary = [x * shrink, y * shrink];
        let boundary_squared = boundary.map(|component| component * component);
        let separation = (boundary_squared[0] + boundary_squared[1]).sqrt();
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let weapon = actor.active_weapon(unit);
        let maximum = actor.weapon_range(unit, self.passenger_bonus_flags(id));
        let mut direction = if length == 0.0 {
            [0.0; 2]
        } else {
            [x / length, y / length]
        };
        if weapon.min_range > CELL_SIZE && separation < weapon.min_range {
            let angle = (-direction[1]).atan2(-direction[0]);
            let mut relative = actor.facing - angle;
            if relative > std::f32::consts::TAU {
                relative -= std::f32::consts::TAU;
            }
            if relative < -std::f32::consts::TAU {
                relative += std::f32::consts::TAU;
            }
            if self.height_above_ground(id) > 0.0 && relative.abs() < std::f32::consts::FRAC_PI_2 {
                direction[0] = -direction[0];
                direction[1] = -direction[1];
            }
            let halfway = (maximum + weapon.min_range) * 0.5;
            let range = halfway + radii;
            let displacement = direction.map(|component| component * range);
            let extent = [
                f32::from(self.grid.width()) * CELL_SIZE,
                f32::from(self.grid.height()) * CELL_SIZE,
            ];
            return [
                (target_position[0] + displacement[0]).clamp(CELL_SIZE, extent[0] - CELL_SIZE),
                (target_position[1] + displacement[1]).clamp(CELL_SIZE, extent[1] - CELL_SIZE),
                target_position[2],
            ];
        }
        if separation < 0.001 {
            return source;
        }
        direction = [boundary[0] / separation, boundary[1] / separation];
        let range = maximum * 0.9;
        let displacement = direction.map(|component| component * range);
        let mut destination = [
            target_position[0] + displacement[0],
            target_position[1] + displacement[1],
            target_position[2],
        ];
        if actor.active_locomotor(unit).flight.is_some() {
            self.adjust_air_attack_destination(id, target, position, &mut destination);
        }
        destination
    }

    fn adjust_air_attack_destination(
        &self,
        id: usize,
        target: Option<usize>,
        target_position: [f32; 3],
        destination: &mut [f32; 3],
    ) {
        let actor = &self.entities[id];
        let (_, centered) = crate::skirmish::startup::radius_and_center(
            &self.catalog[actor.type_index].footprint(actor.position, 0.0),
        );
        let offset = if centered { 0.0 } else { CELL_SIZE * 0.5 };
        let Some(cell) = self.cell_at([destination[0] + offset, destination[1] + offset]) else {
            return;
        };
        let check = |x: i32, y: i32| {
            let cell = crate::grid::Cell {
                x: u16::try_from(x).ok()?,
                y: u16::try_from(y).ok()?,
            };
            if !self.aircraft_destination_unclaimed(id, cell) {
                return None;
            }
            let point = self.path_position(id, cell);
            let point = [point[0], point[1], self.ground_height(point)];
            self.weapon_goal_in_range(id, point, target, target_position)
                .then_some(point)
        };
        let mut x = i32::from(cell.x);
        let mut y = i32::from(cell.y);
        if let Some(point) = check(x, y) {
            *destination = point;
            return;
        }
        let mut limit = 400;
        let mut delta = 1;
        while limit > 0 {
            for [dx, dy] in [[1, 0], [0, 1], [-1, 0], [0, -1]] {
                for _ in 0..delta {
                    x += dx;
                    y += dy;
                    limit -= 1;
                    if let Some(point) = check(x, y) {
                        *destination = point;
                        return;
                    }
                }
                if dy != 0 {
                    delta += 1;
                }
            }
        }
    }

    pub(in crate::skirmish) fn weapon_goal_in_range(
        &self,
        id: usize,
        goal: [f32; 3],
        target: Option<usize>,
        position: [f32; 3],
    ) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let weapon = actor.active_weapon(unit);
        let range_inset = CELL_SIZE * 0.25;
        let maximum = actor.weapon_range(unit, self.passenger_bonus_flags(id)) - range_inset;
        let maximum_squared = maximum * maximum;
        let point_distance = |point: [f32; 3], radius: f32| {
            crate::skirmish::combat::circle_distance_squared(
                [goal[0], goal[1]],
                [point[0], point[1]],
                radius,
            )
        };
        let radius = self.object_radius(id);
        let squared = match target {
            Some(target) if self.attack_kind(target, "BRIDGE") => {
                let points = self
                    .bridge_attack_points(target)
                    .expect("existing bridge target");
                let first = point_distance(points[0], radius);
                if first > maximum_squared {
                    point_distance(points[1], radius)
                } else {
                    first
                }
            }
            Some(target) => point_distance(
                self.object_position(target),
                radius + self.object_radius(target),
            ),
            None => point_distance(position, radius),
        };
        let minimum = weapon.min_range + range_inset;
        squared >= minimum * minimum && squared <= maximum_squared
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{
        Kind, Order,
        ai_goals::COMMAND_FROM_PLAYER,
        flight::{Appearance, tests::rules},
        tests::game,
    };

    #[test]
    fn airborne_attack_retains_and_follows_its_approach_path_before_entering_aim() {
        let mut game = game();
        let kind = game.type_for(0, Kind::Tank).unwrap();
        game.catalog[kind].locomotor.flight = Some(rules(Appearance::Hover));
        game.catalog[kind].locomotor.legal_surfaces = 8;
        game.catalog[kind].weapon.range = 100.0;
        let actor = game.spawn(kind, 0, [200.0, 200.0], true);
        let target_kind = game.type_for(1, Kind::Infantry).unwrap();
        let target = game.spawn(target_kind, 1, [500.0, 200.0], true);
        game.set_flight_position(actor, [200.0, 200.0, 100.0]);
        game.set_last_command_source(actor, COMMAND_FROM_PLAYER);
        game.set_order(actor, Order::Attack(target));
        assert!(game.entities[actor].ai.path_request.pending.is_some());
        game.process_path_service();
        let path = game.entities[actor].ai.ground_path.as_ref().unwrap();
        assert!(path.is_direct());
        let endpoint = path.last_position().unwrap();
        assert!(endpoint[0] > 350.0 && endpoint[0] < 500.0);
        assert!(game.weapon_goal_in_range(
            actor,
            endpoint,
            Some(target),
            game.object_position(target)
        ));
        assert!(game.entities[actor].ai.path_request.attack_path);
        game.tick += 1;
        assert!(game.update_attack(actor, target));
        assert!(
            game.entities[actor]
                .drawable
                .model_conditions
                .has_flag(crate::skirmish::ModelConditions::MOVING)
        );
        assert!(game.entities[actor].ai.ground_path.is_some());
        assert!(game.presentation.shots.is_empty());
    }

    #[test]
    fn quick_air_path_reuses_xyz_endpoint_and_keeps_off_map_position() {
        let mut game = game();
        let kind = game.type_for(0, Kind::Tank).unwrap();
        game.catalog[kind].locomotor.flight = Some(rules(Appearance::Hover));
        game.catalog[kind].locomotor.legal_surfaces = 8;
        let actor = game.spawn(kind, 0, [200.0, 200.0], true);
        let destination = [-100.0, 200.0, 70.0];
        game.compute_quick_air_path(actor, destination);
        assert_eq!(
            game.entities[actor]
                .ai
                .ground_path
                .as_ref()
                .unwrap()
                .last_position(),
            Some(destination)
        );
        let frame = game.entities[actor].ai.blocking.path_frame;
        game.tick += 1;
        game.compute_quick_air_path(actor, [-100.0, 200.0, 70.4]);
        assert_eq!(game.entities[actor].ai.blocking.path_frame, frame);
        assert_eq!(
            game.entities[actor]
                .ai
                .ground_path
                .as_ref()
                .unwrap()
                .last_position(),
            Some(destination)
        );
        game.compute_quick_air_path(actor, [-100.0, 200.0, 70.6]);
        assert_eq!(game.entities[actor].ai.blocking.path_frame, game.tick);
        assert_eq!(
            game.entities[actor]
                .ai
                .ground_path
                .as_ref()
                .unwrap()
                .last_position(),
            Some([-100.0, 200.0, 70.6])
        );
    }
}
