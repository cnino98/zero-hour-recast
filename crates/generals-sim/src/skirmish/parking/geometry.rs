use super::{Info, Pose, Rules, State};

impl crate::skirmish::Skirmish {
    // Object::getSingleLogicalBonePosition falls back to cached position and
    // raw Object orientation when the current Draw cannot supply the bone.
    pub(in crate::skirmish) fn parking_logical_pose(&self, id: usize, name: &str) -> Pose {
        let (position, basis) = self.object_transform(id);
        let Some(local) = self.logical_bone_matrix(id, name) else {
            return Pose {
                position: self.object_position(id),
                direction: basis.map(|row| row[0]),
            };
        };
        let offset =
            crate::skirmish::physical_ocl::transform::vector(basis, local.map(|row| row[3]));
        Pose {
            position: std::array::from_fn(|axis| offset[axis] + position[axis]),
            direction: crate::skirmish::physical_ocl::transform::vector(
                basis,
                local.map(|row| row[0]),
            ),
        }
    }
}

impl State {
    pub(in crate::skirmish) fn info(&self, id: usize, rules: &Rules, offset: f32) -> Option<Info> {
        let space = &self.spaces[self.space_index(id)?];
        let geometry = &space.geometry;
        let runway = &self.runways.get(geometry.runway)?.geometry;
        let parked = if rules.park_in_hangars {
            &geometry.hangar
        } else {
            &geometry.parking
        };
        let mut parking = parked.position;
        if offset != 0.0 {
            // Source reserveSpace uses the parking bone's orientation even
            // when calcPPInfo selected the internal hangar location.
            let x = offset * geometry.parking.orientation().cos();
            let y = offset * geometry.parking.orientation().sin();
            parking[0] += x;
            parking[1] += y;
        }
        let dx = runway.end[0] - runway.start[0];
        let dy = runway.end[1] - runway.start[1];
        let dz = runway.end[2] - runway.start[2];
        let xx = dx * dx;
        let yy = dy * dy;
        let zz = dz * dz;
        let ax = dx * 0.75;
        let ay = dy * 0.75;
        let approach = [
            runway.end[0] + ax,
            runway.end[1] + ay,
            (runway.end[2] + rules.approach_height) + rules.deck_height,
        ];
        let waiting = self
            .runways
            .iter()
            .any(|runway| runway.object == Some(id) && runway.was_in_line);
        Some(Info {
            parking: Pose {
                position: parking,
                direction: parked.direction,
            },
            prep: geometry.prep,
            start: if waiting { geometry.prep } else { runway.start },
            end: runway.end,
            approach,
            hangar: Pose {
                position: geometry.hangar.position,
                direction: geometry.hangar.direction,
            },
            takeoff_distance: ((xx + yy) + zz).sqrt(),
        })
    }
}
