//! Native Tee/Y/H/four-way junctions and cross-type alpha joins.
use super::super::LAST;
use super::curves::{cross, rotate};
use super::{Kind, Network, Point, Segment, Vec2, normalized};
use std::error::Error;

mod cross_type;
mod offsets;
use offsets::Frame;

impl Network<'_> {
    pub(super) fn insert_tee_intersections(&mut self) -> Result<(), Box<dyn Error>> {
        let original = self.segments.len();
        for index in 0..original {
            if self.segments[index].kind != Kind::Segment {
                continue;
            }
            for count in [2, 3] {
                for point in 0..2 {
                    if self.segments[index].points[point].count != count {
                        continue;
                    }
                    let location = self.segments[index].points[point].position;
                    if count == 2 {
                        self.insert_three_way(location, index)?;
                    } else {
                        self.insert_four_way(location, index)?;
                    }
                }
            }
        }
        Ok(())
    }

    // The source scans all subsequent types, marking every matching endpoint;
    // the last matching arm replaces the final slot when there are extra arms.
    fn junction_arms(&mut self, location: Vec2, index: usize) -> Vec<(usize, usize)> {
        let center = usize::from(self.segments[index].points[0].position != location);
        let mut arms = vec![(index, center)];
        for next in index + 1..self.segments.len() {
            for point in 0..2 {
                if self.segments[next].points[point].position == location {
                    self.segments[next].points[point].count = -2;
                    arms.push((next, point));
                }
            }
        }
        arms
    }

    fn insert_three_way(&mut self, location: Vec2, index: usize) -> Result<(), Box<dyn Error>> {
        let arms = self.junction_arms(location, index);
        if arms.len() < 3 {
            return Ok(());
        }
        let selected = [arms[0], arms[1], *arms.last().expect("three junction arms")];
        let directions = selected.map(|(segment, center)| {
            normalized(self.segments[segment].points[1 - center].position - location)
        });
        let mut centers = selected.map(|(segment, center)| self.segments[segment].points[center]);
        let scale = self.segments[index].scale;
        let width = self.segments[index].width;
        let (kind, side) = if let Some(order) = y_arms(directions) {
            let up = normalized(directions[order[2]]) * (0.5 * scale);
            let side = rotate(up, -std::f32::consts::PI / 2.0);
            let mut points = order.map(|arm| centers[arm]);
            offsets::y(
                &mut points,
                &Frame {
                    location,
                    up,
                    side,
                    width,
                },
            );
            for (point, arm) in points.into_iter().zip(order) {
                centers[arm] = point;
            }
            (Kind::ThreeWayY, side)
        } else {
            let dots = [
                directions[0].dot(directions[1]),
                directions[0].dot(directions[2]),
                directions[2].dot(directions[1]),
            ];
            let pair = if dots[0] < dots[1] {
                if dots[0] < dots[2] { 0 } else { 2 }
            } else if dots[1] < dots[2] {
                1
            } else {
                2
            };
            let order = [[0, 1, 2], [0, 2, 1], [2, 1, 0]][pair];
            let [first, second, arm] = order;
            let axis = normalized(directions[second] - directions[first]);
            let decider = directions[arm];
            let mirror = cross(axis, decider) < 0.0;
            let angle = if mirror {
                -std::f32::consts::PI / 2.0
            } else {
                std::f32::consts::PI / 2.0
            };
            let up = normalized(axis) * (0.5 * scale);
            let side = rotate(up, angle);
            let mut points = order.map(|arm| centers[arm]);
            let frame = Frame {
                location,
                up,
                side,
                width,
            };
            let kind = if axis.dot(decider).abs() > 0.5 {
                let flip = cross(side, decider) > 0.0;
                offsets::h(&mut points, &frame, flip, mirror);
                if flip {
                    Kind::ThreeWayHFlip
                } else {
                    Kind::ThreeWayH
                }
            } else {
                offsets::tee(&mut points, &frame);
                Kind::Tee
            };
            for (point, arm) in points.into_iter().zip(order) {
                centers[arm] = point;
            }
            (kind, side)
        };
        for ((segment, center), mut point) in selected.into_iter().zip(centers) {
            point.flags |= LAST;
            point.count = 0;
            self.segments[segment].points[center] = point;
        }
        self.append_section(section(
            &self.segments[index],
            kind,
            [location, location + side],
            width,
            -3,
        ))
    }

    fn insert_four_way(&mut self, location: Vec2, index: usize) -> Result<(), Box<dyn Error>> {
        let arms = self.junction_arms(location, index);
        if arms.len() < 4 {
            return Ok(());
        }
        let selected = [
            arms[0],
            arms[1],
            arms[2],
            *arms.last().expect("four junction arms"),
        ];
        let outside =
            selected.map(|(segment, center)| self.segments[segment].points[1 - center].position);
        let vectors = outside.map(|point| normalized(point - location));
        let pairs = [
            [0, 1, 2, 3],
            [0, 2, 1, 3],
            [0, 3, 2, 1],
            [1, 2, 0, 3],
            [1, 3, 0, 2],
            [2, 3, 0, 1],
        ];
        let mut chosen = 0;
        let mut score = vectors[0].dot(vectors[1]);
        for (index, pair) in pairs.iter().enumerate().skip(1) {
            let next = vectors[pair[0]].dot(vectors[pair[1]]);
            if next < score {
                chosen = index;
                score = next;
            }
        }
        let order = pairs[chosen];
        let mut up =
            normalized(vectors[order[1]] - vectors[order[0]]) * (0.5 * self.segments[index].scale);
        let mut centers = order.map(|arm| {
            let (segment, center) = selected[arm];
            self.segments[segment].points[center]
        });
        offsets::four_way(
            &mut centers,
            outside[order[2]],
            &Frame {
                location,
                up,
                side: Vec2::ZERO,
                width: self.segments[index].width,
            },
        );
        for (mut point, arm) in centers.into_iter().zip(order) {
            point.flags |= LAST;
            point.count = 0;
            let (segment, center) = selected[arm];
            self.segments[segment].points[center] = point;
        }
        if up.x < 0.0 {
            up = -up;
        }
        self.append_section(section(
            &self.segments[index],
            Kind::FourWay,
            [location, location + up],
            1.03,
            -4,
        ))
    }

    pub(super) fn append_section(&mut self, segment: Segment) -> Result<(), Box<dyn Error>> {
        if self.segments.len() >= self.limit {
            return Err("Original road segment capacity exceeded by generated topology".into());
        }
        self.segments.push(segment);
        Ok(())
    }
}

pub(super) fn section(
    original: &Segment,
    kind: Kind,
    positions: [Vec2; 2],
    width: f32,
    count: i32,
) -> Segment {
    let mut points = positions.map(|position| Point {
        position,
        top: Vec2::ZERO,
        bottom: Vec2::ZERO,
        count: 0,
        flags: LAST,
    });
    points[0].count = count;
    Segment {
        points,
        radius: 0.0,
        kind,
        scale: original.scale,
        width,
        road: original.road,
    }
}

fn y_arms(vectors: [Vec2; 3]) -> Option<[usize; 3]> {
    let [first, second, third] = vectors;
    let dots = [first.dot(second), first.dot(third), third.dot(second)];
    if dots.into_iter().any(|dot| dot < -0.866) {
        return None;
    }
    let mut candidates = [false; 3];
    let mut scores = [2.0; 3];
    let orders = [[2, 0, 1], [1, 0, 2], [0, 2, 1]];
    for (candidate, [stem, left, right]) in orders.into_iter().enumerate() {
        let signs = [
            cross_sign(vectors[stem], vectors[left]),
            cross_sign(vectors[stem], vectors[right]),
        ];
        let normal = Vec2::new(-vectors[stem].y, vectors[stem].x);
        if signs[0] != signs[1]
            && signs[0] + signs[1] == 0
            && cross_sign(normal, vectors[left]) == 1
            && cross_sign(normal, vectors[right]) == 1
        {
            candidates[candidate] = true;
            let pair = match candidate {
                0 => [dots[1], dots[2]],
                1 => [dots[0], dots[2]],
                _ => [dots[0], dots[1]],
            };
            scores[candidate] = (pair[0] + 0.707).abs() + (pair[1] + 0.707).abs();
        }
    }
    let chosen = if scores[0] < scores[1] {
        if scores[0] < scores[2] { 0 } else { 2 }
    } else if scores[1] < scores[2] {
        1
    } else {
        2
    };
    if !candidates[chosen] {
        return None;
    }
    let [stem, left, right] = orders[chosen];
    Some(if cross_sign(vectors[stem], vectors[left]) == -1 {
        [left, right, stem]
    } else {
        [right, left, stem]
    })
}

fn cross_sign(first: Vec2, second: Vec2) -> i32 {
    let value = cross(first, second);
    if value < 0.0 {
        -1
    } else {
        i32::from(value > 0.0)
    }
}

#[cfg(test)]
mod tests;
