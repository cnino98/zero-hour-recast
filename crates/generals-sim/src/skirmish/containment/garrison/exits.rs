use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish::containment) fn exit_garrison(&mut self, id: usize, carrier: usize) {
        let stationed = self.entities[id].stationed();
        self.remove_passenger(id, carrier);
        let host = &self.entities[carrier];
        let center = host.position;
        let angle = host.facing;
        let radius = self.catalog[host.type_index].geometry.major;
        let surfaces = self.catalog[self.entities[id].type_index]
            .locomotor
            .legal_surfaces;
        let valid = |position| self.movement_terrain.valid_movement(position, surfaces);
        let mut start = center;
        if !valid(start) {
            let x = radius * angle.cos();
            let y = radius * angle.sin();
            start = [start[0] - x, start[1] - y];
            if !valid(start) {
                let twice_x = 2.0 * x;
                let twice_y = 2.0 * y;
                start = [start[0] + twice_x, start[1] + twice_y];
                if !valid(start) {
                    start = center;
                }
            }
        }
        if !stationed {
            self.entities[id].position = start;
        }
        self.set_object_orientation(id, angle);
        self.update_position_claim(id);
        let end = self.adjusted_destination(id, start).unwrap_or(start);
        self.native_follow_path_from(
            id,
            vec![[end[0], end[1], self.ground_height(end)]],
            Some(carrier),
            crate::skirmish::ai_goals::COMMAND_FROM_AI,
            false,
        );
        self.reserve_goal(id, end);
        self.observe_ground_partition(id);
    }
}
