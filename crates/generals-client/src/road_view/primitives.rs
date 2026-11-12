//! Native road atlas regions and corner shapes; `W3DRoadBuffer` load/preload methods.
use super::{
    Kind, Segment,
    geometry::{Section, normalized},
};
use bevy::prelude::*;

pub(super) fn section(road: &Segment) -> Section {
    let origin = road.points[0].position;
    let raw = road.points[1].position - origin;
    let (mut direction, mut normal) = if raw.x.abs() < 0.25
        && raw.y.abs() < 0.25
        && !matches!(
            road.kind,
            Kind::Segment | Kind::Curve | Kind::Tee | Kind::FourWay
        ) {
        (Vec2::X, Vec2::Y)
    } else {
        (normalized(raw), normalized(Vec2::new(-raw.y, raw.x)))
    };
    let scale = road.scale;
    let width = road.width;
    let mut result = Section {
        origin,
        direction: raw,
        normal: normal * (width * scale / 2.0),
        corners: [
            road.points[0].bottom,
            road.points[1].bottom,
            road.points[0].top,
            road.points[1].top,
        ],
        offset: Vec2::new(0.0, 85.0 / 512.0),
        scale: Vec2::splat(scale),
    };
    match road.kind {
        Kind::Segment => {}
        Kind::Curve => curve(&mut result, road, direction, normal),
        Kind::Tee | Kind::FourWay => {
            let half_height = scale * 1.03 / 2.0;
            let left = width * scale / 2.0;
            let right_vector = direction * half_height;
            let left_vector = normalized(right_vector) * left;
            normal *= half_height.abs();
            direction = right_vector + left_vector;
            let bottom = origin - left_vector - normal;
            result.corners = [
                bottom,
                bottom + direction,
                bottom + 2.0 * normal,
                bottom + direction + 2.0 * normal,
            ];
            result.direction = direction;
            result.normal = normal;
            result.offset = Vec2::new(
                425.0 / 512.0,
                if road.kind == Kind::FourWay {
                    425.0
                } else {
                    255.0
                } / 512.0,
            );
        }
        Kind::ThreeWayY => {
            direction *= scale;
            normal *= scale;
            direction *= 1.59;
            let top = origin + normal * 0.29 - direction * 0.5;
            let bottom = top - normal * 1.08;
            result.corners = [bottom, bottom + direction, top, top + direction];
            result.direction = direction;
            result.normal = normal;
            result.offset = Vec2::new(255.0 / 512.0, 226.0 / 512.0);
        }
        Kind::ThreeWayH | Kind::ThreeWayHFlip => {
            direction *= scale;
            normal *= scale;
            normal *= 1.35;
            let flip = road.kind == Kind::ThreeWayHFlip;
            let bottom = origin - normal * if flip { 0.20 } else { 0.8 } - direction * width / 2.0;
            let width_vector = direction * width / 2.0 + direction * 1.2;
            result.corners = [
                bottom,
                bottom + width_vector,
                bottom + normal,
                bottom + width_vector + normal,
            ];
            result.direction = direction;
            result.normal = if flip { -normal } else { normal };
            result.offset = Vec2::new(202.0 / 512.0, 364.0 / 512.0);
        }
        Kind::AlphaJoin => {
            direction *= scale * 48.0 / 128.0;
            normal *= width * (1.0 + 8.0 / 128.0);
            let top = origin + normal * 0.5 - direction * 0.65;
            let bottom = top - normal;
            result.corners = [bottom, bottom + direction, top, top + direction];
            result.direction = direction;
            result.normal = normal;
            result.scale.y = width;
            result.offset = Vec2::new(106.0 / 512.0, 425.0 / 512.0);
        }
    }
    result
}

fn curve(result: &mut Section, road: &Segment, mut direction: Vec2, mut normal: Vec2) {
    let origin = result.origin;
    let scale = road.scale;
    let width = road.width;
    direction *= scale;
    normal *= (width * scale / 2.0).abs();
    let tight = road.radius.to_bits() == 0.5_f32.to_bits();
    let length = if tight { 0.5 } else { 1.0 };
    let lower = origin - normal;
    let mut corners = [
        lower,
        lower + direction * length,
        lower + 2.0 * normal,
        lower + direction * length + 2.0 * normal,
    ];
    corners[1] += direction * 0.1;
    corners[1] += normal * if tight { 0.2 } else { 0.4 };
    corners[0] -= normal * if tight { 0.1 } else { 0.2 };
    corners[0] -= direction * 0.02;
    corners[2] -= direction * 0.02;
    corners[3] -= direction * 0.4;
    corners[3] += normal * if tight { 0.2 } else { 0.4 };
    result.corners = corners;
    result.direction = direction;
    result.normal = normal;
    result.offset = Vec2::new(4.0 / 512.0, if tight { 425.0 } else { 255.0 } / 512.0);
}
