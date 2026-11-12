//! `W3DRoadBuffer` curve insertion, miter joins and native `WWMath` line arithmetic.
use super::super::{ANGLED, Kind, LAST};
use super::joins::section;
use super::{Network, Vec2, normalized};
use num_traits::ToPrimitive;
use std::error::Error;

pub(super) fn cross(first: Vec2, second: Vec2) -> f32 {
    let first_product = first.x * second.y;
    let second_product = first.y * second.x;
    first_product - second_product
}

pub(super) fn rotate(point: Vec2, angle: f32) -> Vec2 {
    let sine = f64::from(angle)
        .sin()
        .to_f32()
        .expect("WWMath sine is float representable");
    let cosine = f64::from(angle)
        .cos()
        .to_f32()
        .expect("WWMath cosine is float representable");
    let xx = point.x * cosine;
    let xy = point.y * -sine;
    let yx = point.x * sine;
    let yy = point.y * cosine;
    Vec2::new(xx + xy, yx + yy)
}

fn rotate_about(point: Vec2, center: Vec2, angle: f32) -> Vec2 {
    let offset = point - center;
    let mut rotated = rotate(offset, angle);
    rotated.y -= offset.y;
    rotated.x -= offset.x;
    point + rotated
}

pub(super) fn intersection(first: [Vec2; 2], second: [Vec2; 2]) -> Option<Vec2> {
    let direction1 = normalized(first[1] - first[0]).extend(0.0);
    let direction2 = normalized(second[1] - second[0]).extend(0.0);
    let cross1 = direction1.cross(direction2);
    let cross2 = (second[0] - first[0]).extend(0.0).cross(direction2);
    let numerator = cross2.dot(cross1);
    let denominator = cross1.dot(cross1);
    if denominator.partial_cmp(&0.0) == Some(std::cmp::Ordering::Equal) {
        return None;
    }
    Some(first[0] + direction1.truncate() * (numerator / denominator))
}

pub(super) fn closest_point(line: [Vec2; 2], position: Vec2) -> Vec2 {
    let delta = line[1] - line[0];
    let direction = normalized(delta);
    let along = direction.dot(position - line[0]);
    if along <= 0.0 {
        line[0]
    } else if along >= delta.length() {
        line[1]
    } else {
        line[0] + direction * along
    }
}

impl Network<'_> {
    pub(super) fn insert_curve_segments(&mut self) -> Result<(), Box<dyn Error>> {
        let original = self.segments.len();
        let mut start = None;
        for index in 0..original {
            if index + 1 < original
                && self.segments[index].points[0].position
                    == self.segments[index + 1].points[1].position
            {
                if self.segments[index + 1].points[1].count == 1
                    && self.segments[index].points[0].count == 1
                {
                    self.insert_curve_at(index, index + 1)?;
                    if start.is_none() {
                        start = Some(index);
                    }
                }
            } else if let Some(first) = start {
                if self.segments[index].points[0].position
                    == self.segments[first].points[1].position
                    && self.segments[first].points[1].count == 1
                    && self.segments[index].points[0].count == 1
                {
                    self.insert_curve_at(index, first)?;
                }
                start = None;
            }
        }
        Ok(())
    }

    fn miter(&mut self, first: usize, second: usize) {
        for top in [true, false] {
            let first_line = self.segments[first]
                .points
                .map(|point| if top { point.top } else { point.bottom });
            let second_line = self.segments[second]
                .points
                .map(|point| if top { point.top } else { point.bottom });
            if let Some(point) = intersection(first_line, second_line) {
                if top {
                    self.segments[second].points[1].top = point;
                    self.segments[first].points[0].top = point;
                } else {
                    self.segments[second].points[1].bottom = point;
                    self.segments[first].points[0].bottom = point;
                }
            }
        }
    }

    fn insert_curve_at(&mut self, first: usize, second: usize) -> Result<(), Box<dyn Error>> {
        if self.roads[self.segments[first].road].id != self.roads[self.segments[second].road].id {
            return Ok(());
        }
        let original = self.segments[first].points[0].position;
        let first_line = self.segments[first].points.map(|point| point.position);
        let second_line = self.segments[second].points.map(|point| point.position);
        let direction1 = normalized(first_line[1] - first_line[0]);
        let direction2 = normalized(second_line[1] - second_line[0]);
        let sine = direction1.dot(direction2);
        let refs = if cross(direction1, direction2) > 0.0 {
            [(first, 0), (first, 1), (second, 0), (second, 1)]
        } else {
            [(second, 1), (second, 0), (first, 1), (first, 0)]
        };
        let mut points = refs.map(|(segment, point)| self.segments[segment].points[point].position);
        let tangents = [
            normalized(points[1] - points[0]),
            normalized(points[3] - points[2]),
        ];
        let angle = f64::from(sine)
            .acos()
            .to_f32()
            .expect("WWMath acos is float representable");
        let count = angle / (std::f32::consts::PI / 6.0);
        if count < 0.9 || self.segments[first].points[0].flags & ANGLED != 0 {
            self.miter(first, second);
            return Ok(());
        }
        let radius = self.segments[first].radius * self.segments[first].scale;
        let offsets =
            tangents.map(|direction| rotate(direction * radius, -std::f32::consts::PI / 2.0));
        let Some(center) = intersection(
            [points[0] + offsets[0], points[1] + offsets[0]],
            [points[2] + offsets[1], points[3] + offsets[1]],
        ) else {
            return Ok(());
        };
        self.segments[second].points[1].flags |= LAST;
        let contact2 =
            intersection([center, center - offsets[1]], [points[2], points[3]]).unwrap_or(center);
        let contact1 = intersection([center, center - offsets[0]], [points[0], points[1]])
            .ok_or("Degenerate native road curve tangent")?;
        if tangents[1].dot(contact2 - points[2]) < 0.5
            || tangents[0].dot(points[1] - contact1) < 0.5
        {
            for slot in [refs[0], refs[3]] {
                self.segments[slot.0].points[slot.1].position = original;
            }
            self.miter(first, second);
            return Ok(());
        }
        points[3] = contact2;
        self.segments[refs[3].0].points[refs[3].1].position = contact2;
        // The other tangent is changed only after emitting the forward sweep.
        self.append_curve_sweep(first, points, contact1, count)?;
        self.segments[refs[0].0].points[refs[0].1].position = contact1;
        self.segments[first].points[0].flags |= LAST;
        for (segment, point) in [(first, 0), (second, 1)] {
            let direction = self.segments[segment].points[1].position
                - self.segments[segment].points[0].position;
            let normal = normalized(Vec2::new(-direction.y, direction.x))
                * (self.segments[segment].scale * self.segments[segment].width / 2.0);
            let endpoint = &mut self.segments[segment].points[point];
            endpoint.top = endpoint.position + normal;
            endpoint.bottom = endpoint.position - normal;
        }
        Ok(())
    }

    fn append_curve_sweep(
        &mut self,
        index: usize,
        points: [Vec2; 4],
        other_tangent: Vec2,
        count: f32,
    ) -> Result<(), Box<dyn Error>> {
        let angle = -std::f32::consts::PI / 6.0;
        let mut second = points[3];
        let mut direction = points[2] - second;
        let normal = normalized(Vec2::new(-direction.y, direction.x));
        let center = normal * (self.segments[index].radius * self.segments[index].scale) + second;
        second = rotate_about(second, center, angle);
        direction = rotate(direction, angle);
        self.append_curve(index, [second, second + direction])?;
        if count > 2.0 {
            for ordinal in 2_u16.. {
                if f32::from(ordinal).partial_cmp(&count) != Some(std::cmp::Ordering::Less) {
                    break;
                }
                direction = rotate(direction, angle);
                second = rotate_about(second, center, angle);
                self.append_curve(index, [second, second + direction])?;
            }
        }
        if count > 1.0 {
            let first = other_tangent + other_tangent - points[1];
            direction = first - other_tangent;
            self.append_curve(index, [other_tangent, other_tangent + direction])?;
        }
        Ok(())
    }

    fn append_curve(&mut self, index: usize, points: [Vec2; 2]) -> Result<(), Box<dyn Error>> {
        let mut curve = section(
            &self.segments[index],
            Kind::Curve,
            points,
            self.segments[index].width,
            0,
        );
        curve.radius = self.segments[index].radius;
        self.append_section(curve)
    }
}
