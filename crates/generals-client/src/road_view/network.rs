//! `W3DRoadBuffer` ordered map insertion, endpoint counts and link repair.
mod curves;
mod joins;
use super::{ANGLED, JOIN, Kind, LAST, MULTIPLE, Point, Road, Segment, geometry::normalized};
use bevy::prelude::*;
use generals_assets::map::MapObject;
use std::error::Error;

struct Network<'a> {
    segments: Vec<Segment>,
    roads: &'a mut [Road],
    limit: usize,
}

pub(super) fn build(
    records: &[MapObject],
    roads: &mut [Road],
    limit: usize,
) -> Result<Vec<Segment>, Box<dyn Error>> {
    let mut network = Network {
        segments: Vec::new(),
        roads,
        limit,
    };
    network.add_map_objects(records)?;
    network.update_counts_and_flags();
    network.insert_tee_intersections()?;
    network.insert_curve_segments()?;
    network.insert_cross_type_joins()?;
    Ok(network.segments)
}

impl Network<'_> {
    fn move_segment(&mut self, from: usize, to: usize) {
        if from >= self.segments.len() || to >= self.segments.len() || from == to {
            return;
        }
        let segment = self.segments.remove(from);
        self.segments.insert(to, segment);
    }

    fn check_link_before(&mut self, index: usize) {
        if self.segments[index].points[1].count != 1 {
            return;
        }
        let mut location = self.segments[index].points[1].position;
        let mut end = index + 1;
        while end < self.segments.len() - 1 {
            if self.segments[end].points[0].position != self.segments[end + 1].points[1].position {
                break;
            }
            end += 1;
        }
        let mut check = end + 1;
        while check < self.segments.len() {
            if self.segments[check].points[0].position == location {
                self.move_segment(check, index);
                location = self.segments[index].points[1].position;
                if self.segments[index].points[1].count != 1 {
                    return;
                }
                end += 1;
            } else if self.segments[check].points[1].position == location {
                self.segments[check].points.swap(0, 1);
                self.move_segment(check, index);
                location = self.segments[index].points[1].position;
                if self.segments[index].points[1].count != 1 {
                    return;
                }
                end += 1;
            } else {
                check += 1;
            }
            if check <= end {
                check = end + 1;
            }
        }
    }

    fn check_link_after(&mut self, mut index: usize) {
        if self.segments[index].points[0].count != 1 || index >= self.segments.len() - 1 {
            return;
        }
        let mut location = self.segments[index].points[0].position;
        let mut check = index + 1;
        while check < self.segments.len() && index < self.segments.len() - 1 {
            if self.segments[check].points[1].position == location {
                index += 1;
                self.move_segment(check, index);
                location = self.segments[index].points[0].position;
                if self.segments[index].points[0].count != 1 {
                    return;
                }
            } else if self.segments[check].points[0].position == location {
                self.segments[check].points.swap(0, 1);
                index += 1;
                self.move_segment(check, index);
                location = self.segments[index].points[0].position;
                if self.segments[index].points[0].count != 1 {
                    return;
                }
            } else {
                check += 1;
            }
        }
    }

    fn add_map_object(&mut self, mut current: Segment, update_counts: bool) {
        let [from, to] = current.points.map(|point| point.position);
        let direction = to - from;
        let normal = normalized(Vec2::new(-direction.y, direction.x))
            * (current.scale * current.width / 2.0);
        for point in &mut current.points {
            point.top = point.position + normal;
            point.bottom = point.position - normal;
        }
        if update_counts {
            self.update_counts(&mut current);
        }
        if self.segments.len() >= self.limit {
            return;
        }
        let mut flip = false;
        let mut before = false;
        let mut after = false;
        let mut index = self.segments.len();
        for (candidate, segment) in self.segments.iter().enumerate() {
            let [first, second] = segment.points.map(|point| point.position);
            if (first == from && second == to) || (first == to && second == from) {
                return;
            }
            if current.points[0].count == 1 {
                if first == from {
                    flip = true;
                    after = true;
                }
                if second == from {
                    flip = false;
                    before = true;
                }
            }
            if current.points[1].count == 1 {
                if first == to {
                    flip = false;
                    after = true;
                }
                if second == to {
                    flip = true;
                    before = true;
                }
            }
            if before || after {
                index = candidate;
                break;
            }
        }
        if after {
            index += 1;
        }
        if flip {
            current.points.swap(0, 1);
        }
        self.segments.insert(index, current);
        if before {
            self.check_link_before(index);
        } else if after {
            self.check_link_after(index);
        }
    }

    fn add_map_objects(&mut self, records: &[MapObject]) -> Result<(), Box<dyn Error>> {
        let mut index = 0;
        while index < records.len() {
            if self.segments.len() >= self.limit {
                break;
            }
            let first = &records[index];
            index += 1;
            if first.flags & 2 == 0 {
                continue;
            }
            let Some(second) = records.get(index) else {
                break;
            };
            if second.flags & 4 == 0 {
                continue;
            }
            index += 1;
            let mut positions =
                [first, second].map(|record| Vec2::new(record.position[0], record.position[1]));
            if positions[0] == positions[1] {
                positions[1].x += 0.25;
            }
            let (road, scale, width) = if let Some(road) = self
                .roads
                .iter()
                .position(|road| road.name == first.template)
            {
                (road, self.roads[road].scale, self.roads[road].width)
            } else {
                let road = self
                    .roads
                    .iter()
                    .position(|road| road.id == 1)
                    .ok_or("Unknown map road has no native ID 1 texture")?;
                (road, 8.0, 1.0)
            };
            if scale <= 0.0 || width <= 0.0 {
                return Err("Map road has nonpositive native width".into());
            }
            let points = [first, second].map(|record| Point {
                position: Vec2::new(record.position[0], record.position[1]),
                top: Vec2::ZERO,
                bottom: Vec2::ZERO,
                count: 0,
                flags: LAST
                    | (u8::from(record.flags & 8 != 0) * ANGLED)
                    | (u8::from(record.flags & 128 != 0) * JOIN),
            });
            let mut current = Segment {
                points,
                radius: if first.flags & 64 != 0 { 0.5 } else { 1.5 },
                kind: Kind::Segment,
                scale,
                width,
                road,
            };
            for (point, position) in current.points.iter_mut().zip(positions) {
                point.position = position;
            }
            self.add_map_object(current, true);
        }
        let original = std::mem::take(&mut self.segments);
        for segment in original {
            self.add_map_object(segment, false);
        }
        Ok(())
    }

    fn update_counts(&mut self, current: &mut Segment) {
        for point in &mut current.points {
            point.flags = (point.flags | LAST) & !MULTIPLE;
            point.count = 0;
        }
        for segment in &mut self.segments {
            if self.roads[segment.road].id != self.roads[current.road].id {
                continue;
            }
            for point in &mut segment.points {
                for added in &mut current.points {
                    if point.position == added.position {
                        point.count += 1;
                        added.count += 1;
                    }
                }
                point.flags = (point.flags & !MULTIPLE) | (u8::from(point.count > 1) * MULTIPLE);
            }
        }
        for point in &mut current.points {
            point.flags = (point.flags & !MULTIPLE) | (u8::from(point.count > 1) * MULTIPLE);
        }
    }

    fn update_counts_and_flags(&mut self) {
        for point in self
            .segments
            .iter_mut()
            .flat_map(|segment| &mut segment.points)
        {
            point.flags |= LAST;
            point.count = 0;
        }
        for last in (1..self.segments.len()).rev() {
            let (before, after) = self.segments.split_at_mut(last);
            let current = &mut after[0];
            for segment in before {
                if self.roads[segment.road].id != self.roads[current.road].id {
                    continue;
                }
                for point in &mut segment.points {
                    for added in &mut current.points {
                        if point.position == added.position {
                            point.flags &= !LAST;
                            point.count += 1;
                            added.count += 1;
                        }
                    }
                }
            }
        }
    }
}
