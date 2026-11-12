//! Road sampling uses truncation and rounded static-light vertices, not terrain interpolation.
use super::Terrain;
use bevy::prelude::*;

impl Terrain {
    pub(crate) fn road_cell_height(&self, point: Vec2) -> f32 {
        let raw = ((point / 10.0).trunc() + Vec2::splat(f32::from(self.border)))
            .clamp(
                Vec2::ZERO,
                Vec2::new(
                    f32::from(self.raw_width - 2),
                    f32::from(self.raw_height - 2),
                ),
            )
            .as_uvec2();
        let x = u16::try_from(raw.x).expect("bounded terrain sample");
        let y = u16::try_from(raw.y).expect("bounded terrain sample");
        self.raw_height(x, y)
            .max(self.raw_height(x + 1, y))
            .max(self.raw_height(x, y + 1))
            .max(self.raw_height(x + 1, y + 1))
            * 0.625
    }

    pub(crate) fn road_vertex_normal(&self, point: Vec2) -> Vec3 {
        let raw = ((point / 10.0 + Vec2::splat(0.5)).trunc() + Vec2::splat(f32::from(self.border)))
            .clamp(
                Vec2::ZERO,
                Vec2::new(
                    f32::from(self.raw_width - 1),
                    f32::from(self.raw_height - 1),
                ),
            )
            .as_uvec2();
        let x = u16::try_from(raw.x).expect("bounded terrain light vertex");
        let y = u16::try_from(raw.y).expect("bounded terrain light vertex");
        let left_right = Vec3::new(
            20.0,
            0.0,
            (self.raw_height((x + 1).min(self.raw_width - 1), y)
                - self.raw_height(x.saturating_sub(1), y))
                * 0.625,
        );
        let near_far = Vec3::new(
            0.0,
            20.0,
            (self.raw_height(x, (y + 1).min(self.raw_height - 1))
                - self.raw_height(x, y.saturating_sub(1)))
                * 0.625,
        );
        let source = left_right.cross(near_far).normalize();
        Vec3::new(source.x, source.z, -source.y)
    }
}
