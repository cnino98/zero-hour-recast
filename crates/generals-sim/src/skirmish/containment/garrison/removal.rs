use crate::collision_geometry::{Body, Geometry, Shape, intersects};
use crate::skirmish::{Footprint, Skirmish, movement::footprints_overlap, startup};
use std::f32::consts::TAU;

impl Skirmish {
    /// Forced removal (sale, capture or death) uses native `removeAllContained`,
    /// not the independently ordered `exitObjectViaDoor` walk-out state.
    pub(in crate::skirmish) fn remove_garrison_contents(&mut self, carrier: usize) {
        self.remove_garrison_contents_exposed(carrier, false);
    }

    pub(in crate::skirmish) fn remove_garrison_contents_exposed(
        &mut self,
        carrier: usize,
        expose: bool,
    ) {
        if !self.is_garrison(carrier) {
            return;
        }
        if !self.passengers(carrier).is_empty() {
            self.validate_garrison_rally(carrier);
        }
        for id in self.passengers(carrier).to_vec() {
            self.remove_passenger_exposed(id, carrier, expose);
        }
        self.recalculate_garrison_owner(carrier);
    }

    fn validate_garrison_rally(&mut self, carrier: usize) {
        let rally = self.entities[carrier]
            .containment
            .garrison
            .as_ref()
            .and_then(|state| state.rally);
        let rally = rally.and_then(|point| self.find_garrison_rally(carrier, point, 0.0, 0.0));
        let rally = rally.or_else(|| {
            let radius = self.object_radius(carrier);
            self.find_garrison_rally(
                carrier,
                self.entities[carrier].position,
                radius,
                radius * 1.8,
            )
        });
        if let Some(state) = &mut self.entities[carrier].containment.garrison {
            state.rally = rally;
        }
    }

    fn find_garrison_rally(
        &mut self,
        carrier: usize,
        center: [f32; 2],
        minimum: f32,
        maximum: f32,
    ) -> Option<[f32; 2]> {
        if self.cell_at(center).is_none() {
            return Some(center);
        }
        let start_angle = startup::random_angle(&mut self.random);
        let mut distance = minimum;
        loop {
            if distance > maximum {
                break;
            }
            let spacing = if distance.to_bits() == minimum.to_bits() {
                TAU
            } else {
                (5.0 / (distance + 1.0)) * (TAU / 6.0)
            };
            let samples = ((TAU / spacing) / 2.0).ceil();
            let mut sample = 0_u16;
            loop {
                if f32::from(sample) >= samples {
                    break;
                }
                let offset = spacing * f32::from(sample);
                for angle in [start_angle + offset, start_angle - offset]
                    .into_iter()
                    .take(if sample == 0 { 1 } else { 2 })
                {
                    let x = distance * angle.cos();
                    let y = distance * angle.sin();
                    let point = [center[0] + x, center[1] + y];
                    if self.garrison_rally_clear(carrier, point, angle) {
                        return Some(point);
                    }
                }
                sample += 1;
            }
            distance += 5.0;
        }
        None
    }

    fn garrison_rally_clear(&self, carrier: usize, point: [f32; 2], angle: f32) -> bool {
        if self
            .cell_at(point)
            .is_none_or(|cell| self.grid.is_blocked(cell))
            || !self.movement_terrain.valid_movement(point, 1)
        {
            return false;
        }
        let probe = Footprint {
            position: point,
            angle,
            major: 5.0,
            minor: 5.0,
            shape: crate::collision_geometry::Shape::Sphere,
        };
        if self
            .layout
            .obstacles
            .iter()
            .any(|other| footprints_overlap(&probe, other))
        {
            return false;
        }
        let owner = self.entities[carrier].owner;
        let geometry = Geometry {
            is_small: true,
            shape: Shape::Sphere,
            major: 5.0,
            minor: 5.0,
            height: 5.0,
        };
        let probe = Body {
            geometry: &geometry,
            position: [point[0], point[1], self.ground_height(point)],
            angle,
        };
        !self
            .partition
            .projectile_candidates(&probe)
            .into_iter()
            .any(|id| {
                let other = &self.entities[id];
                if id == carrier || !other.alive() || other.masked() {
                    return false;
                }
                let unit = &self.catalog[other.type_index];
                if !self.enemies(owner, other.owner)
                    && unit
                        .kind_of
                        .iter()
                        .any(|kind| matches!(kind.as_str(), "INFANTRY" | "VEHICLE"))
                {
                    return false;
                }
                intersects(
                    &probe,
                    &Body {
                        geometry: &unit.geometry,
                        position: self.object_position(id),
                        angle: other.facing,
                    },
                )
            })
    }
}
