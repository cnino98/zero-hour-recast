//! Native atlas stump offsets; preserve the previous edge orientation.
use super::super::super::{Point, Vec2};
use super::super::curves::{cross, rotate};

pub(super) struct Frame {
    pub location: Vec2,
    pub up: Vec2,
    pub side: Vec2,
    pub width: f32,
}

fn edges(point: &mut Point, normal: Vec2, positive: bool) {
    if positive {
        point.top = point.position + normal;
        point.bottom = point.position - normal;
    } else {
        point.top = point.position - normal;
        point.bottom = point.position + normal;
    }
}

pub(super) fn tee(points: &mut [Point; 3], frame: &Frame) {
    let Frame {
        location,
        up,
        side,
        width,
    } = *frame;
    points[0].position = location - up;
    points[1].position = location + up;
    points[2].position = location + side;
    let right = if cross(up, side) < 0.0 { -side } else { side } * width;
    for point in &mut points[..2] {
        let positive = cross(up, point.top - point.position) > 0.0;
        edges(point, right, positive);
    }
    let normal = up * width;
    let positive = cross(right, points[2].top - points[2].position) < 0.0;
    edges(&mut points[2], normal, positive);
}

pub(super) fn h(points: &mut [Point; 3], frame: &Frame, flip: bool, mirror: bool) {
    let Frame {
        location,
        up,
        side,
        width,
    } = *frame;
    if flip == mirror {
        points[0].position = location - up * 0.46;
        points[1].position = location + up * 2.05;
    } else {
        points[0].position = location - up * 2.05;
        points[1].position = location + up * 0.46;
    }
    let right = if cross(up, side) < 0.0 { -side } else { side } * width;
    for point in &mut points[..2] {
        let positive = cross(up, point.top - point.position) > 0.0;
        edges(point, right, positive);
    }
    let angle = if flip {
        std::f32::consts::PI / 4.0
    } else {
        -std::f32::consts::PI / 4.0
    };
    let arm = rotate(side, angle);
    let normal = Vec2::new(-arm.y, arm.x) * width;
    points[2].position += arm * 2.10;
    let positive = cross(arm, points[2].top - location) > 0.0;
    edges(&mut points[2], normal, positive);
}

pub(super) fn y(points: &mut [Point; 3], frame: &Frame) {
    let Frame {
        location,
        up,
        width,
        ..
    } = *frame;
    let delta = up * 0.55;
    points[2].position += delta;
    points[2].top += delta;
    points[2].bottom += delta;
    let mut arm = rotate(up, 3.0 * std::f32::consts::PI / 4.0);
    let mut normal = Vec2::new(-arm.y, arm.x) * width;
    points[1].position += arm * 1.1;
    let positive = cross(arm, points[1].top - location) > 0.0;
    edges(&mut points[1], normal, positive);
    arm = rotate(up, -3.0 * std::f32::consts::PI / 4.0);
    normal = Vec2::new(-arm.y, arm.x) * width;
    points[0].position += arm * 1.1;
    let positive = cross(arm, points[0].top - location) > 0.0;
    edges(&mut points[0], normal, positive);
}

pub(super) fn four_way(points: &mut [Point; 4], outside: Vec2, frame: &Frame) {
    let Frame {
        location,
        up,
        width,
        ..
    } = *frame;
    points[0].position = location - up;
    points[1].position = location + up;
    let angle = if cross(up, outside - location) < 0.0 {
        -std::f32::consts::PI / 2.0
    } else {
        std::f32::consts::PI / 2.0
    };
    let side = rotate(up, angle);
    points[2].position = location + side;
    points[3].position = location - side;
    let right = rotate(up, std::f32::consts::PI / 2.0) * width;
    for point in &mut points[..2] {
        let positive = cross(up, point.top - point.position) > 0.0;
        edges(point, right, positive);
    }
    let normal = up * width;
    for point in &mut points[2..] {
        let positive = cross(right, point.top - point.position) < 0.0;
        edges(point, normal, positive);
    }
}
