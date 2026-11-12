//! Native parking exit reservations and actual logical-bone placement.
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn spawn_parking_unit(
        &mut self,
        producer: usize,
        type_index: usize,
        free: bool,
        ordinal: usize,
    ) -> Option<usize> {
        let type_index = self.resolve_variation(type_index);
        let unit = &self.catalog[type_index];
        let helipad = unit
            .kind_of
            .iter()
            .any(|kind| kind == "PRODUCED_AT_HELIPAD");
        let actor = &self.entities[producer];
        let door = actor.production_queue_at(ordinal).front()?.parking_door;
        let owner = actor.owner;
        let rally = actor.rally_point;
        let spawned = self.new_object(type_index, owner, true);
        self.entities[spawned].producer = Some(producer);
        self.entities[producer]
            .parking
            .as_mut()?
            .exit(door, spawned)
            .ok()?;
        let (position, facing) = if helipad {
            let pose = self.parking_logical_pose(producer, "HeliPark01");
            (pose.position, pose.orientation())
        } else {
            let offset = self.catalog[type_index]
                .aircraft
                .as_ref()
                .map_or(0.0, |rules| rules.parking_offset);
            self.reserve_parking_space(producer, spawned)?;
            let actor = &self.entities[producer];
            let info = actor.parking.as_ref()?.info(
                spawned,
                self.catalog[actor.type_index].parking.as_ref()?,
                offset,
            )?;
            (info.hangar.position, info.hangar.orientation())
        };
        self.set_object_position(spawned, position);
        self.set_object_orientation(spawned, facing);
        self.restore_object_obstacle(spawned);
        if free {
            self.opponent_unit_produced(producer, spawned);
        }
        if self.entities[spawned].ai.registration.is_some() {
            if helipad {
                let goal = rally.unwrap_or(position);
                self.construction_move(spawned, goal);
            } else {
                self.native_follow_path_from(
                    spawned,
                    Vec::new(),
                    Some(producer),
                    crate::skirmish::ai_goals::COMMAND_FROM_AI,
                    true,
                );
            }
        }
        if !free {
            self.opponent_unit_produced(producer, spawned);
            self.native_build_complete(spawned);
        }
        self.record_built(spawned);
        Some(spawned)
    }
}
