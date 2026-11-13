//! `W3DView::getAxisAlignedViewRegion` using this frame's camera, not visibility lag.
use bevy::prelude::*;

pub fn region(
    transform: &Transform,
    projection: &Projection,
    camera: &Camera,
    terrain: [[f32; 2]; 2],
) -> [[f32; 2]; 2] {
    let Projection::Perspective(projection) = projection else {
        return overscan(terrain);
    };
    let aspect = camera
        .viewport
        .as_ref()
        .map_or(projection.aspect_ratio, |viewport| {
            let size = viewport.physical_size.as_vec2();
            size.x / size.y
        });
    let extent = (projection.fov * 0.5).tan();
    let mut bounds = [[f32::INFINITY; 2], [f32::NEG_INFINITY; 2]];
    for [x, y] in [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]] {
        let ray = transform.rotation * Vec3::new(x * extent * aspect, y * extent, -1.0);
        let ray = ray.normalize();
        let distance = -transform.translation.y / ray.y;
        if !distance.is_finite() || !(0.0..=projection.far * 2.0).contains(&distance) {
            // The current terrain renderer draws its complete stored mesh.
            // Native low-pitch fallback takes that terrain draw region.
            return overscan(terrain);
        }
        let point = transform.translation + ray * distance;
        let point = [point.x * 10.0, -point.z * 10.0];
        for (axis, coordinate) in point.into_iter().enumerate() {
            bounds[0][axis] = bounds[0][axis].min(coordinate);
            bounds[1][axis] = bounds[1][axis].max(coordinate);
        }
    }
    overscan(bounds)
}

fn overscan(mut region: [[f32; 2]; 2]) -> [[f32; 2]; 2] {
    // The ordinary user camera has the source's zero script guard-band bias.
    region[0][0] -= 75.0;
    region[0][1] -= 135.0;
    region[1][0] += 75.0;
    region[1][1] += 75.0;
    region
}

pub fn contains(region: [[f32; 2]; 2], position: [f32; 2]) -> bool {
    (0..2).all(|axis| (region[0][axis]..=region[1][axis]).contains(&position[axis]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_overscan_is_asymmetric_and_position_membership_includes_boundaries() {
        let region = overscan([[100.0, 200.0], [300.0, 400.0]]);
        assert_eq!(
            region.map(|point| point.map(f32::to_bits)),
            [[25.0, 65.0], [375.0, 475.0_f32]].map(|point| point.map(f32::to_bits))
        );
        assert!(contains(region, [25.0, 65.0]));
        assert!(contains(region, [375.0, 475.0]));
        assert!(!contains(region, [24.999, 100.0]));
        assert!(!contains(region, [100.0, 475.001]));
    }

    #[test]
    fn creation_region_uses_current_camera_and_low_pitch_uses_the_drawn_terrain() {
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.0,
            far: 1000.0,
            ..default()
        });
        let camera = Camera::default();
        let terrain = [[-100.0; 2], [2000.0; 2]];
        let mut transform =
            Transform::from_xyz(0.0, 100.0, 0.0).looking_to(Vec3::NEG_Y, Vec3::NEG_Z);
        let initial = region(&transform, &projection, &camera, terrain);
        assert!(contains(initial, [0.0; 2]));
        transform.translation.x = 500.0;
        let moved = region(&transform, &projection, &camera, terrain);
        assert!(!contains(moved, [0.0; 2]));
        assert!(contains(moved, [5000.0, 0.0]));
        transform.rotation = Quat::IDENTITY;
        assert_eq!(
            region(&transform, &projection, &camera, terrain).map(|point| point.map(f32::to_bits)),
            overscan(terrain).map(|point| point.map(f32::to_bits))
        );
    }
}
