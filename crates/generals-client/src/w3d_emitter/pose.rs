//! Native `Build_Quaternion`, `Cached_Slerp` and quaternion vector rotation.
pub(super) struct Pose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}

impl Pose {
    pub fn new(position: [f32; 3], basis: [[f32; 3]; 3]) -> Self {
        // object_transform exposes the native row-major Matrix3D basis.
        let matrix = basis;
        let diagonal = matrix[0][0] + matrix[1][1];
        let trace = diagonal + matrix[2][2];
        let mut rotation = [0.0; 4];
        if trace > 0.0 {
            let root = (trace + 1.0).sqrt();
            rotation[3] = root * 0.5;
            let scale = 0.5 / root;
            rotation[0] = (matrix[2][1] - matrix[1][2]) * scale;
            rotation[1] = (matrix[0][2] - matrix[2][0]) * scale;
            rotation[2] = (matrix[1][0] - matrix[0][1]) * scale;
        } else {
            let mut index = usize::from(matrix[1][1] > matrix[0][0]);
            if matrix[2][2] > matrix[index][index] {
                index = 2;
            }
            let next = (index + 1) % 3;
            let last = (next + 1) % 3;
            let other = matrix[next][next] + matrix[last][last];
            let root = ((matrix[index][index] - other) + 1.0).sqrt();
            rotation[index] = root * 0.5;
            let scale = if root == 0.0 { root } else { 0.5 / root };
            rotation[3] = (matrix[last][next] - matrix[next][last]) * scale;
            rotation[next] = (matrix[next][index] + matrix[index][next]) * scale;
            rotation[last] = (matrix[last][index] + matrix[index][last]) * scale;
        }
        Self { position, rotation }
    }

    pub fn interpolate(&self, current: &Self, alpha: f32) -> Self {
        let position = std::array::from_fn(|axis| {
            let distance = current.position[axis] - self.position[axis];
            let movement = distance * alpha;
            self.position[axis] + movement
        });
        Self {
            position,
            rotation: generals_assets::w3d::animation::cached_slerp(
                self.rotation,
                current.rotation,
                alpha,
            ),
        }
    }

    pub fn rotate(&self, vector: [f32; 3]) -> [f32; 3] {
        let [qx, qy, qz, qw] = self.rotation;
        let [vx, vy, vz] = vector;
        let yz = qy * vz;
        let zy = vy * qz;
        let wx = qw * vx;
        let x = wx + (yz - zy);
        let xz = qx * vz;
        let zx = vx * qz;
        let wy = qw * vy;
        let y = wy - (xz - zx);
        let xy = qx * vy;
        let yx = vx * qy;
        let wz = qw * vz;
        let z = wz + (xy - yx);
        let xx = qx * vx;
        let yy = qy * vy;
        let zz = qz * vz;
        let w = -(xx + yy + zz);
        let first = [w * -qx, w * -qy, w * -qz];
        let second = [qw * x, qw * y, qw * z];
        let cross_left = [y * -qz, x * -qz, x * -qy];
        let cross_right = [-qy * z, -qx * z, -qx * y];
        let cross = std::array::from_fn::<_, 3, _>(|axis| cross_left[axis] - cross_right[axis]);
        [
            first[0] + second[0] + cross[0],
            first[1] + second[1] - cross[1],
            first[2] + second[2] + cross[2],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::Pose;

    #[test]
    fn object_row_major_pitch_and_heading_keep_full_native_rotation() {
        for (basis, input, expected) in [
            (
                [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ),
            (
                [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
                [0.0, 0.0, 1.0],
                [0.0, -1.0, 0.0],
            ),
        ] {
            let rotated = Pose::new([0.0; 3], basis).rotate(input);
            for axis in 0..3 {
                assert!((rotated[axis] - expected[axis]).abs() < 4.0 * f32::EPSILON);
            }
        }
    }
}
