//! `BezierSegment` and `BezFwdIterator`, `SuperHackers` 2bfaea60 (EA GPL-3.0+).

pub(super) fn length(points: [[f32; 3]; 4]) -> f32 {
    let chord = distance(points[0], points[3]);
    let polygon = distance(points[0], points[1])
        + distance(points[1], points[2])
        + distance(points[2], points[3]);
    if polygon - chord <= 1.0 {
        let total = chord + polygon;
        return total / 2.0;
    }
    let first = midpoint(points[0], points[1]);
    let middle = midpoint(points[1], points[2]);
    let last = midpoint(points[2], points[3]);
    let left = midpoint(first, middle);
    let right = midpoint(middle, last);
    let center = std::array::from_fn(|axis| {
        let a = points[0][axis] * 0.125;
        let b = points[1][axis] * 0.375;
        let c = points[2][axis] * 0.375;
        let d = points[3][axis] * 0.125;
        a + b + c + d
    });
    length([points[0], first, left, center]) + length([center, right, last, points[3]])
}

pub(super) fn sample(points: [[f32; 3]; 4], count: u16) -> Vec<[f32; 3]> {
    if count <= 1 {
        // BezFwdIterator leaves its zero point unchanged for a one-point curve.
        // DumbProjectileBehavior's fixed one-point branch uses the endpoint.
        return vec![[0.0; 3]; usize::from(count)];
    }
    let d = 1.0 / f32::from(count - 1);
    let d2 = d * d;
    let d3 = d * d2;
    let mut current = points[0];
    let mut first = [0.0; 3];
    let mut second = [0.0; 3];
    let mut third = [0.0; 3];
    for axis in 0..3 {
        let p1 = 3.0 * points[1][axis];
        let p2 = 3.0 * points[2][axis];
        let a = -points[0][axis] + p1 - p2 + points[3][axis];
        let three_start = 3.0 * points[0][axis];
        let six_first = 6.0 * points[1][axis];
        let b = three_start - six_first + p2;
        let c = -three_start + p1;
        let ad = a * d3;
        let bd = b * d2;
        let cd = c * d;
        first[axis] = ad + bd + cd;
        let six_ad = 6.0 * a * d3;
        let two_bd = 2.0 * b * d2;
        second[axis] = six_ad + two_bd;
        third[axis] = six_ad;
    }
    let mut result = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        result.push(current);
        for axis in 0..3 {
            current[axis] += first[axis];
            first[axis] += second[axis];
            second[axis] += third[axis];
        }
    }
    result
}

fn midpoint(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| {
        let delta = (b[axis] - a[axis]) * 0.5;
        delta + a[axis]
    })
}

pub(in crate::skirmish) fn distance(start: [f32; 3], end: [f32; 3]) -> f32 {
    let delta: [f32; 3] = std::array::from_fn(|axis| end[axis] - start[axis]);
    let squared = delta.map(|value| value * value);
    (squared[0] + squared[1] + squared[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{length, sample};

    #[test]
    fn source_basis_and_forward_differences_follow_cubic_arc() {
        // These dyadic points are exact in both the native D3DX basis multiply
        // and BezFwdIterator's quarter-step forward differences.
        let points = [[0.0; 3], [0.0, 0.0, 8.0], [8.0, 0.0, 8.0], [8.0, 0.0, 0.0]];
        let expected = [
            [0.0; 3],
            [1.25, 0.0, 4.5],
            [4.0, 0.0, 6.0],
            [6.75, 0.0, 4.5],
            [8.0, 0.0, 0.0],
        ];
        for (actual, expected) in sample(points, 5).into_iter().zip(expected) {
            assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
        }
    }

    #[test]
    fn elevated_shell_curve_retains_endpoints_and_longer_flight() {
        let points = [
            [0.0, 0.0, 5.0],
            [50.0, 0.0, 30.0],
            [90.0, 0.0, 30.0],
            [100.0, 0.0, 5.0],
        ];
        assert!(length(points) > 100.0);
        let path = sample(points, 9);
        assert_eq!(path[0].map(f32::to_bits), points[0].map(f32::to_bits));
        assert!(path[4][2] > 20.0);
        for (actual, expected) in path[8].into_iter().zip(points[3]) {
            assert!((actual - expected).abs() < 0.0001);
        }
    }
}
