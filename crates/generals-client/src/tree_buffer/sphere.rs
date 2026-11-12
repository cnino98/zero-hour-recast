//! `WWMath` `SphereClass(Vector3*, Int)`: double radii, binary32 centers.
use num_traits::ToPrimitive;

pub(super) fn from_points(points: &[[f32; 3]]) -> ([f32; 3], f32) {
    let mut low = [points[0]; 3];
    let mut high = low;
    for &point in &points[1..] {
        for axis in 0..3 {
            if point[axis] < low[axis][axis] {
                low[axis] = point;
            }
            if point[axis] > high[axis][axis] {
                high[axis] = point;
            }
        }
    }
    let square = |a: [f32; 3], b: [f32; 3]| {
        let delta = std::array::from_fn::<_, 3, _>(|axis| f64::from(a[axis] - b[axis]));
        let x = delta[0] * delta[0];
        let y = delta[1] * delta[1];
        let z = delta[2] * delta[2];
        (x + y) + z
    };
    let mut axis = 0;
    let mut span = square(low[0], high[0]);
    for next in 1..3 {
        let value = square(low[next], high[next]);
        if value > span {
            span = value;
            axis = next;
        }
    }
    let mut center = std::array::from_fn(|i| {
        let diameter_center = low[axis][i] + high[axis][i];
        diameter_center / 2.0
    });
    let mut squared = square(high[axis], center);
    let mut radius = squared.sqrt();
    for &point in points {
        let test = square(point, center);
        if test > squared {
            let distance = test.sqrt();
            let expanded_diameter = radius + distance;
            radius = expanded_diameter / 2.0;
            squared = radius * radius;
            let remainder = distance - radius;
            center = std::array::from_fn(|i| {
                let old_center = radius * f64::from(center[i]);
                let extension = remainder * f64::from(point[i]);
                ((old_center + extension) / distance)
                    .to_f32()
                    .expect("native sphere")
            });
        }
    }
    (center, radius.to_f32().expect("native sphere"))
}
