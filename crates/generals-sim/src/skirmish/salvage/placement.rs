//! `CreateCrateDie::createCrate`'s two native ground-position searches.
use super::super::{Footprint, Skirmish, movement::footprints_overlap, startup};
use crate::collision_geometry::{Body, Geometry, Shape, intersects};
use std::f32::consts::TAU;
impl Skirmish {
    pub(super) fn crate_position(&mut self, victim: usize) -> Option<[f32; 2]> {
        self.find_crate_position(victim, 5.0, true)
            .or_else(|| self.find_crate_position(victim, 125.0, false))
    }
    fn find_crate_position(
        &mut self,
        victim: usize,
        maximum: f32,
        ignore_friends: bool,
    ) -> Option<[f32; 2]> {
        let center = self.entities[victim].position;
        if self.cell_at(center).is_none() {
            return Some(center);
        }
        let start = startup::random_angle(&mut self.random);
        let mut distance = 0.0;
        loop {
            if distance > maximum {
                break;
            }
            let spacing = if distance == 0.0 {
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
                for angle in [start + offset, start - offset]
                    .into_iter()
                    .take(if sample == 0 { 1 } else { 2 })
                {
                    let x = distance * angle.cos();
                    let y = distance * angle.sin();
                    let point = [center[0] + x, center[1] + y];
                    if self.crate_probe(point, angle, victim, ignore_friends) {
                        return Some(point);
                    }
                }
                sample += 1;
            }
            distance += 5.0;
        }
        None
    }
    fn crate_probe(
        &self,
        point: [f32; 2],
        angle: f32,
        victim: usize,
        ignore_friends: bool,
    ) -> bool {
        if self
            .cell_at(point)
            .is_none_or(|cell| self.grid.is_blocked(cell))
        {
            return false;
        }
        let footprint = Footprint {
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
            .any(|other| footprints_overlap(&footprint, other))
        {
            return false;
        }
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
                let actor = &self.entities[id];
                let unit = &self.catalog[actor.type_index];
                if ignore_friends
                    && !self.enemies(self.entities[victim].owner, actor.owner)
                    && unit
                        .kind_of
                        .iter()
                        .any(|name| name == "INFANTRY" || name == "VEHICLE")
                {
                    return false;
                }
                !actor.masked()
                    && (actor.alive() || actor.crate_present() || self.retained_rubble(id))
                    && intersects(
                        &probe,
                        &Body {
                            geometry: &unit.geometry,
                            position: self.object_position(id),
                            angle: actor.facing,
                        },
                    )
            })
    }
}
