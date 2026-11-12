//! Native `SpawnPointProductionExitUpdate`, SH 2bfaea60.
use crate::skirmish::{Skirmish, physical_ocl::transform};

pub(super) enum Exit {
    Points(usize),
    Production,
    Contain,
}
struct WorldPoint {
    position: [f32; 2],
    angle: f32,
}
pub(in crate::skirmish) struct ExitState {
    points: Option<Vec<WorldPoint>>,
    occupants: [Option<usize>; 10],
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_spawn_exit(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.spawn_exits.len(), ordinal);
        self.entities[id].behaviors.spawn_exits.push(ExitState {
            points: None,
            occupants: [None; 10],
        });
    }

    pub(in crate::skirmish::spawn) fn spawn_exit_interface(&self, id: usize) -> Option<Exit> {
        for module in &self.catalog[self.entities[id].type_index].behavior_modules {
            match module.family.as_str() {
                "SpawnPointProductionExitUpdate" => return Some(Exit::Points(0)),
                "DefaultProductionExitUpdate"
                | "SupplyCenterProductionExitUpdate"
                | "QueueProductionExitUpdate" => return Some(Exit::Production),
                "ParkingPlaceBehavior" | "FlightDeckBehavior" => return None,
                _ => {}
            }
        }
        self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .map(|_| Exit::Contain)
    }

    pub(in crate::skirmish::spawn) fn reserve_spawn_exit(&mut self, id: usize) -> Option<Exit> {
        let exit = self.spawn_exit_interface(id)?;
        let available = match exit {
            Exit::Points(ordinal) => self.reserve_spawn_point(id, ordinal),
            Exit::Production => self.catalog[self.entities[id].type_index]
                .behavior_modules
                .iter()
                .find(|module| {
                    matches!(
                        module.family.as_str(),
                        "DefaultProductionExitUpdate"
                            | "SupplyCenterProductionExitUpdate"
                            | "QueueProductionExitUpdate"
                    )
                })
                .is_some_and(|module| {
                    module.family != "QueueProductionExitUpdate"
                        || self.entities[id].queue_exit.free()
                }),
            Exit::Contain => true,
        };
        available.then_some(exit)
    }

    pub(in crate::skirmish::spawn) fn exit_spawn_via_door(
        &mut self,
        parent: usize,
        child: usize,
        exit: &Exit,
    ) {
        match exit {
            Exit::Points(ordinal) => self.exit_spawn_point(parent, *ordinal, child),
            Exit::Production => self.exit_spawn_from_producer(parent, child),
            Exit::Contain => self.exit_spawned_from_container(child, parent),
        }
    }

    fn reserve_spawn_point(&mut self, id: usize, ordinal: usize) -> bool {
        if self.entities[id].behaviors.spawn_exits[ordinal]
            .points
            .is_none()
        {
            if !self.entities[id].has_drawable() {
                return false;
            }
            let (position, basis) = self.object_transform(id);
            let points = self.catalog[self.entities[id].type_index]
                .behavior
                .spawn_exits[ordinal]
                .iter()
                .map(|point| {
                    let offset = transform::vector(basis, point.position);
                    let forward = transform::vector(
                        basis,
                        [point.basis[0][0], point.basis[1][0], point.basis[2][0]],
                    );
                    WorldPoint {
                        position: [position[0] + offset[0], position[1] + offset[1]],
                        angle: forward[1].atan2(forward[0]),
                    }
                })
                .collect();
            self.entities[id].behaviors.spawn_exits[ordinal].points = Some(points);
        }
        let count = self.entities[id].behaviors.spawn_exits[ordinal]
            .points
            .as_ref()
            .expect("initialized spawn bones")
            .len();
        for index in 0..count {
            if self.entities[id].behaviors.spawn_exits[ordinal].occupants[index]
                .is_some_and(|rider| !self.entities[rider].exists())
            {
                self.entities[id].behaviors.spawn_exits[ordinal].occupants[index] = None;
            }
        }
        self.entities[id].behaviors.spawn_exits[ordinal].occupants[..count]
            .iter()
            .any(Option::is_none)
    }

    pub(in crate::skirmish::spawn) fn exit_spawn_point(
        &mut self,
        parent: usize,
        ordinal: usize,
        child: usize,
    ) {
        if self.entities[parent].behaviors.spawn_exits[ordinal]
            .points
            .is_none()
        {
            self.reserve_spawn_point(parent, ordinal);
        }
        let state = &mut self.entities[parent].behaviors.spawn_exits[ordinal];
        let Some(points) = state.points.as_ref() else {
            return;
        };
        let Some(slot) = state.occupants[..points.len()]
            .iter()
            .position(Option::is_none)
        else {
            return;
        };
        let point = &points[slot];
        let position = point.position;
        let angle = point.angle;
        state.occupants[slot] = Some(child);
        let layer = self.entities[parent].layer;
        let height = self.movement_layer_height(layer, position);
        self.set_object_position(child, [position[0], position[1], height]);
        self.set_object_orientation(child, angle);
        self.entities[child].layer = layer;
        self.restore_object_obstacle(child);
        self.set_disabled_held(child);
    }
}
