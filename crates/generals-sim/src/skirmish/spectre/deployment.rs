use super::{CreationEdge, Skirmish, length, normalized};

impl Skirmish {
    pub(in crate::skirmish) fn deploy_spectre(
        &mut self,
        id: usize,
        ordinal: usize,
        power: &str,
        target: [f32; 3],
        script: bool,
    ) -> bool {
        let rules = &self.catalog[self.entities[id].type_index].spectre_deployments[ordinal];
        if rules.power != power {
            return false;
        }
        let Some(kind) = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.gunship)
        else {
            panic!("validated Spectre gunship template");
        };
        let edge_position = match rules.edge {
            CreationEdge::NearSource | CreationEdge::FarSource => self.object_position(id),
            CreationEdge::NearTarget | CreationEdge::FarTarget => target,
        };
        let nearest = matches!(
            rules.edge,
            CreationEdge::NearSource | CreationEdge::NearTarget
        );
        if script {
            self.set_special_power_ready_frame(id, power, self.update_frame());
        }
        self.entities[id].behaviors.spectre_deployments[ordinal].target = target;
        self.entities[id].behaviors.spectre_deployments[ordinal].gunship = None;
        // Native newObject completes constructors/CREATE at the original zero pose.
        let owner = self.entities[id].owner;
        let ship = self.new_object(kind, owner, true);
        self.entities[ship].producer = Some(id);
        let mut creation = edge_point(self.playable_extent, edge_position, nearest);
        creation[2] = self.ground_height([creation[0], creation[1]]);
        let delta = std::array::from_fn(|axis| target[axis] - creation[axis]);
        let distance = length(delta);
        let delta = normalized(delta);
        // ModuleData radius is allocator-zeroed and has no parser field. It is
        // independent of SpectreGunshipUpdate's authored orbital radius.
        let offset = delta.map(|component| component * distance);
        creation[0] = target[0] - offset[0];
        creation[1] = target[1] - offset[1];
        creation[2] = self.entities[ship]
            .active_locomotor(&self.catalog[kind])
            .flight
            .as_ref()
            .expect("Spectre WINGS locomotor")
            .preferred_height;
        self.set_object_position(ship, creation);
        self.set_object_orientation(
            ship,
            (target[1] - creation[1]).atan2(target[0] - creation[0]),
        );
        self.entities[id].behaviors.spectre_deployments[ordinal].gunship = Some(ship);
        // Native deployment marks the ship before its own power intent, which
        // marks it a second time. Root special-power binding retains this order.
        self.mark_special_power_triggered(ship, power, Some(target));
        self.special_power_at_location(ship, power, target, script);
        drop(self.select_object(self.entities[id].owner, &[], ship, true));
        self.spectre_selections.push(ship);
        self.mark_special_power_triggered(id, power, Some(target));
        true
    }
}

fn edge_point(extent: [[f32; 2]; 2], point: [f32; 3], nearest: bool) -> [f32; 3] {
    let mut result = point;
    if nearest {
        let distances = [
            (point[1] - extent[0][1]).abs(),
            (point[0] - extent[1][0]).abs(),
            (point[1] - extent[1][1]).abs(),
            (point[0] - extent[0][0]).abs(),
        ];
        let mut best = 0;
        for index in 1..4 {
            if distances[index] < distances[best] {
                best = index;
            }
        }
        match best {
            0 => result[1] = extent[0][1],
            1 => result[0] = extent[1][0],
            2 => result[1] = extent[1][1],
            _ => result[0] = extent[0][0],
        }
    } else {
        // Source compares to half the extent width, without adding its origin.
        for axis in 0..2 {
            result[axis] = if point[axis] < (extent[1][axis] - extent[0][axis]) / 2.0 {
                extent[1][axis]
            } else {
                extent[0][axis]
            };
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_edge_ties_and_nonzero_origin_farthest_comparison() {
        let extent = [[100.0, 200.0], [500.0, 600.0]];
        assert_eq!(
            edge_point(extent, [300.0, 400.0, 2.0], true).map(f32::to_bits),
            [300.0_f32, 200.0, 2.0].map(f32::to_bits)
        );
        assert_eq!(
            edge_point(extent, [250.0, 250.0, 2.0], false).map(f32::to_bits),
            [100.0_f32, 200.0, 2.0].map(f32::to_bits)
        );
    }
}
