//! Native airfield taxi path construction; carrier decks are separate source modules.
use super::{super::parking, Phase};
use std::f32::consts::{FRAC_PI_2, PI};

pub(super) fn points(info: &parking::Info, phase: Phase) -> Vec<[f32; 3]> {
    if phase == Phase::TaxiFromHangar {
        return vec![info.parking.position];
    }
    let parked = info.parking.position;
    let orient = (info.prep[1] - parked[1]).atan2(info.prep[0] - parked[0]);
    let middle = if super::super::locomotor::normalize_angle(orient - info.parking.orientation())
        .abs()
        > PI / 128.0
    {
        intersection(
            parked,
            info.parking.orientation(),
            info.prep,
            info.parking.orientation() + FRAC_PI_2,
        )
    } else {
        None
    };
    let mut points = Vec::new();
    if phase == Phase::TaxiFromLanding {
        points.push(info.prep);
        points.extend(middle);
        points.push(parked);
    } else {
        points.extend(middle);
        points.push(info.prep);
        points.push(info.start);
    }
    points
}

fn intersection(a: [f32; 3], angle_a: f32, c: [f32; 3], angle_c: f32) -> Option<[f32; 3]> {
    let b = [a[0] + angle_a.cos(), a[1] + angle_a.sin()];
    let d = [c[0] + angle_c.cos(), c[1] + angle_c.sin()];
    let left = (b[0] - a[0]) * (d[1] - c[1]);
    let right = (b[1] - a[1]) * (d[0] - c[0]);
    let denominator = left - right;
    if denominator == 0.0 {
        return None;
    }
    let left = (a[1] - c[1]) * (d[0] - c[0]);
    let right = (a[0] - c[0]) * (d[1] - c[1]);
    let ratio = (left - right) / denominator;
    let dx = ratio * (b[0] - a[0]);
    let dy = ratio * (b[1] - a[1]);
    Some([a[0] + dx, a[1] + dy, (a[2] + c[2]) * 0.5])
}
