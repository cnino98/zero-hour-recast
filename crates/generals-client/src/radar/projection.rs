//! Integer-pixel transforms from `Radar::findDrawPositions` and `localPixelToRadar`.

use bevy::prelude::*;

pub fn fit(bounds: Rect, world: Vec2) -> Rect {
    let size = bounds.size();
    let ratios = world / size;
    let (low, high) = if ratios.x >= ratios.y {
        let radar = world / ratios.x;
        let low = Vec2::new(0.0, ((size.y - radar.y) / 2.0).trunc());
        (low, Vec2::new(radar.x.trunc(), size.y - low.y))
    } else {
        let radar = world / ratios.y;
        let low = Vec2::new(((size.x - radar.x) / 2.0).trunc(), 0.0);
        (low, Vec2::new(size.x - low.x, radar.y.trunc()))
    };
    Rect::from_corners(bounds.min + low, bounds.min + high)
}

pub(super) fn mouse_world(cursor: Vec2, bounds: Rect, world: Vec2) -> Option<Vec2> {
    let point = cursor.trunc() - bounds.min;
    let size = bounds.size();
    let fitted = fit(Rect::from_corners(Vec2::ZERO, size), world);
    if point.x < fitted.min.x
        || point.x > fitted.max.x
        || point.y < fitted.min.y
        || point.y > fitted.max.y
    {
        return None;
    }
    let scaled = fitted.size();
    let radar = if scaled.x >= scaled.y {
        let x = ((point.x - fitted.min.x) * 128.0 / scaled.x).trunc();
        let relative_y = ((point.y - fitted.min.y) / scaled.y * size.y).trunc();
        Vec2::new(x, ((size.y - relative_y) * 128.0 / size.y).trunc())
    } else {
        let relative_x = ((point.x - fitted.min.x) / scaled.x * size.x).trunc();
        Vec2::new(
            (relative_x * 128.0 / size.x).trunc(),
            ((size.y - point.y) * 128.0 / size.y).trunc(),
        )
    };
    Some(radar.clamp(Vec2::ZERO, Vec2::splat(127.0)) * world / 128.0)
}

pub(super) fn world_pixel(point: Vec2, bounds: Rect, world: Vec2) -> Vec2 {
    let radar = (point / (world / 128.0)).trunc();
    let relative = Vec2::new(radar.x, 127.0 - radar.y) * bounds.size() / 128.0;
    bounds.min + relative.trunc()
}

// Line2D.cpp clips Y then X for the first endpoint, then the second endpoint
// against the already clipped first. Its integer divisions truncate toward zero.
pub(super) fn clip(mut ends: [Vec2; 2], bounds: Rect) -> Option<[Vec2; 2]> {
    if (ends[0].x < bounds.min.x && ends[1].x < bounds.min.x)
        || (ends[0].x > bounds.max.x && ends[1].x > bounds.max.x)
        || (ends[0].y < bounds.min.y && ends[1].y < bounds.min.y)
        || (ends[0].y > bounds.max.y && ends[1].y > bounds.max.y)
    {
        return None;
    }
    for index in 0..2 {
        let mut point = ends[index];
        let other = ends[1 - index];
        let edge = point.y.clamp(bounds.min.y, bounds.max.y);
        if edge.to_bits() != point.y.to_bits() {
            let difference = other.y - point.y;
            if difference == 0.0 {
                return None;
            }
            point.x += ((other.x - point.x) * (edge - point.y) / difference).trunc();
            point.y = edge;
        }
        let edge = point.x.clamp(bounds.min.x, bounds.max.x);
        if edge.to_bits() != point.x.to_bits() {
            let difference = other.x - point.x;
            if difference == 0.0 {
                return None;
            }
            point.y += ((other.y - point.y) * (edge - point.x) / difference).trunc();
            point.x = edge;
        }
        ends[index] = point;
    }
    ends.iter()
        .all(|point| point.cmpge(bounds.min).all() && point.cmple(bounds.max).all())
        .then_some(ends)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tall_map_input_uses_native_cell_quantization_and_rejects_letterbox() {
        let bounds = Rect::new(0.0, 0.0, 200.0, 200.0);
        let world = Vec2::new(2000.0, 3000.0);
        assert_eq!(fit(bounds, world), Rect::new(33.0, 0.0, 167.0, 200.0));
        assert!(mouse_world(Vec2::new(32.0, 100.0), bounds, world).is_none());
        assert_eq!(
            mouse_world(Vec2::new(100.0, 100.0), bounds, world),
            Some(world / 2.0)
        );
        assert_eq!(
            mouse_world(Vec2::new(167.0, 0.0), bounds, world),
            Some(world * (127.0 / 128.0))
        );
    }

    #[test]
    fn clipped_camera_side_restarts_its_gradient_at_integer_endpoint() {
        let line = [Vec2::new(-7.0, -5.0), Vec2::new(11.0, 13.0)];
        assert_eq!(
            clip(line, Rect::new(0.0, 0.0, 10.0, 10.0)),
            Some([Vec2::new(0.0, 2.0), Vec2::new(8.0, 10.0)])
        );
    }
}
