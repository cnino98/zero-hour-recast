//! `OpenContain::redeployOccupants` / `putObjAtNextFirePoint`, SH 2bfaea60.
use super::super::{Skirmish, physical_ocl::transform};

impl Skirmish {
    pub(super) fn deploy_firepoint_passengers(&mut self, carrier: usize) {
        let count = if self.entities[carrier].has_drawable() {
            self.art(carrier).map_or(0, |art| art.fire_points.len())
        } else {
            0
        };
        // Native redeployment walks the actual literal list in reverse insertion order.
        let mut remaining = self.entities[carrier].containment.passengers.len();
        let mut next = 0;
        while remaining != 0 {
            remaining -= 1;
            let rider = self.entities[carrier].containment.passengers[remaining];
            if count == 0 {
                self.set_object_position(rider, self.object_position(carrier));
                continue;
            }
            let (position, basis) = self.firepoint_transform(carrier, next);
            if self.container_encloses(carrier, rider) {
                self.set_object_position(rider, position);
            } else {
                self.set_object_transform(rider, position, basis);
            }
            next += 1;
            if next >= count {
                next = 0;
            }
        }
    }

    fn firepoint_transform(&self, carrier: usize, index: usize) -> ([f32; 3], [[f32; 3]; 3]) {
        let unit = &self.catalog[self.entities[carrier].type_index];
        let point = &self
            .art(carrier)
            .expect("authored container art")
            .fire_points[index];
        let (position, local_basis) = if unit
            .contain
            .as_ref()
            .expect("OpenContain data")
            .passengers_in_turret
        {
            let thresholds = unit
                .contain
                .as_ref()
                .expect("OpenContain data")
                .damage_thresholds;
            let ratio = self.entities[carrier].health / self.entities[carrier].max_health;
            let state = if ratio <= thresholds[1] {
                2
            } else {
                usize::from(ratio <= thresholds[0])
            };
            let turret = unit
                .turret
                .as_ref()
                .expect("admitted MAIN turret firepoints");
            let pivot = turret.mounts[state].yaw;
            let yaw = transform::yaw(self.entities[carrier].ai.turret.angle);
            // Translate(pivot), pre-rotate the basis, Translate(-pivot), then
            // multiply the identity-basis bone offset, as Object.cpp6150.
            let adjustment = std::array::from_fn::<_, 3, _>(|row| {
                let x = yaw[row][0] * -pivot[0];
                let y = yaw[row][1] * -pivot[1];
                let z = yaw[row][2] * -pivot[2];
                pivot[row] + ((x + y) + z)
            });
            let position = std::array::from_fn(|row| {
                let x = yaw[row][0] * point.position[0];
                let y = yaw[row][1] * point.position[1];
                let z = yaw[row][2] * point.position[2];
                ((x + y) + z) + adjustment[row]
            });
            (position, yaw)
        } else {
            (point.position, point.basis)
        };
        let host = self.object_transform(carrier).1;
        let basis = std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                let x = host[row][0] * local_basis[0][column];
                let y = host[row][1] * local_basis[1][column];
                let z = host[row][2] * local_basis[2][column];
                (x + y) + z
            })
        });
        (self.container_point(carrier, position), basis)
    }
}

#[cfg(test)]
mod tests;
