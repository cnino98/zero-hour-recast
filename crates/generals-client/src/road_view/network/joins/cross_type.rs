//! Native cross-type open-end atlas joins and texture stacking repair.
use super::super::super::{JOIN, Kind};
use super::super::curves::{closest_point, cross, intersection, rotate};
use super::super::{Network, Vec2};
use super::section;
use std::error::Error;

impl Network<'_> {
    fn cross_type_join(
        &self,
        location: Vec2,
        mut vector: Vec2,
        road: usize,
    ) -> (Option<usize>, Vec2) {
        for segment in &self.segments {
            if self.roads[segment.road].id == self.roads[road].id || segment.kind != Kind::Segment {
                continue;
            }
            let positions = segment.points.map(|point| point.position);
            let lower = positions[0].min(positions[1]) - Vec2::splat(segment.scale / 2.0);
            let upper = positions[0].max(positions[1]) + Vec2::splat(segment.scale / 2.0);
            if location.x < lower.x
                || location.y < lower.y
                || location.x > upper.x
                || location.y > upper.y
            {
                continue;
            }
            let closest = closest_point(positions, location);
            if closest.distance(location) < segment.scale * 0.55 {
                let direction = positions[1] - positions[0];
                let angle = if cross(direction, vector) > 0.0 {
                    std::f32::consts::PI / 2.0
                } else {
                    -std::f32::consts::PI / 2.0
                };
                vector = rotate(direction, angle);
                return (
                    (self.roads[segment.road].id != 0).then_some(segment.road),
                    vector,
                );
            }
        }
        (None, vector)
    }

    fn adjust_stacking(&mut self, top: usize, bottom: usize) {
        if self.roads[top].stacking > self.roads[bottom].stacking {
            return;
        }
        let stacking = self.roads[bottom].stacking;
        for road in &mut *self.roads {
            if road.stacking > stacking {
                road.stacking += 1;
            }
        }
        self.roads[top].stacking = stacking + 1;
    }

    pub(in super::super) fn insert_cross_type_joins(&mut self) -> Result<(), Box<dyn Error>> {
        let original = self.segments.len();
        for index in 0..original {
            let segment = &self.segments[index];
            let point = if segment.points[1].count == 0 && segment.points[1].flags & JOIN != 0 {
                1
            } else if segment.points[0].count == 0 && segment.points[0].flags & JOIN != 0 {
                0
            } else {
                continue;
            };
            let location = segment.points[point].position;
            let outside = segment.points[1 - point].position;
            let direction = super::super::normalized(outside - location);
            let (other, mut join) = self.cross_type_join(location, direction, segment.road);
            if other.is_none() {
                join *= 100.0;
            }
            let normal = Vec2::new(-direction.y, direction.x);
            let join_normal = Vec2::new(-join.y, join.x);
            let offset = normal * segment.scale * segment.width / 2.0;
            // The source starts from point one's stored edges even for point
            // two; retain that fallback if the infinite lines are parallel.
            let mut top = segment.points[0].top;
            let mut bottom = segment.points[0].bottom;
            let join_line = [location, location + join_normal];
            if let Some(position) = intersection(join_line, [location + offset, outside + offset]) {
                if point == 0 {
                    self.segments[index].points[0].top = position;
                } else {
                    self.segments[index].points[1].bottom = position;
                }
                top = position;
            }
            if let Some(position) = intersection(join_line, [location - offset, outside - offset]) {
                if point == 0 {
                    self.segments[index].points[0].bottom = position;
                } else {
                    self.segments[index].points[1].top = position;
                }
                bottom = position;
            }
            let difference = bottom - top;
            let scale =
                difference.length() / (self.segments[index].scale * self.segments[index].width);
            if let Some(other) = other {
                self.adjust_stacking(self.segments[index].road, other);
            }
            let width = self.segments[index].scale * scale;
            self.append_section(section(
                &self.segments[index],
                Kind::AlphaJoin,
                [location, location + join],
                width,
                0,
            ))?;
        }
        Ok(())
    }
}
