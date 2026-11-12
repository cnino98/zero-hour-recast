//! `MapUtil` cache coordinates and `SkirmishGameOptionsMenu` preview placement.
use bevy::prelude::*;
use generals_assets::content;
use std::error::Error;

pub(super) const MARKER_SIZE: f32 = 15.0;

pub(super) fn positions(
    entry: &content::Declaration,
    field: &str,
) -> Result<Vec<Vec2>, Box<dyn Error>> {
    entry
        .lines
        .iter()
        .filter(|line| {
            line.tokens
                .first()
                .is_some_and(|name| name.eq_ignore_ascii_case(field))
        })
        .map(|line| {
            let axis = |name: &str| -> Result<f32, Box<dyn Error>> {
                let value = line
                    .tokens
                    .iter()
                    .skip(1)
                    .find_map(|token| token.strip_prefix(name))
                    .ok_or("map preview coordinate axis missing")?
                    .parse::<f32>()?;
                if !value.is_finite() {
                    return Err("map preview coordinate is nonfinite".into());
                }
                Ok(value)
            };
            Ok(Vec2::new(axis("X:")?, axis("Y:")?))
        })
        .collect()
}

pub(super) fn marker(bounds: Rect, extent: Vec2, position: Vec2, size: Vec2) -> Rect {
    let ratio = position / extent;
    // C++ assigns each complete floating expression to an Int (towards zero).
    let top = (Vec2::new(ratio.x, 1.0 - ratio.y) * bounds.size() - (size * 0.5).trunc()
        + bounds.min)
        .trunc();
    Rect::from_corners(top, top + size)
}

pub fn starts(window: Rect, extent: Vec2, positions: &[Vec2], size: Vec2) -> Vec<Rect> {
    let local =
        crate::radar::projection::fit(Rect::from_corners(Vec2::ZERO, window.size()), extent);
    let mut placed: Vec<Rect> = Vec::new();
    for &position in positions {
        let mut rect = marker(local, extent, position, size);
        // Preserve the pinned local/new versus screen/previous comparison.
        // Native does not translate the earlier screen positions back to local.
        for previous in &placed {
            let top = previous.min;
            if rect.min.x > top.x
                && rect.min.x < top.x + size.x
                && rect.min.y > top.y
                && rect.min.y < top.y + size.y
            {
                let right = top.x + size.x - rect.min.x;
                let bottom = top.y + size.y - rect.min.y;
                if right <= bottom {
                    rect.min.x = top.x + size.x + 1.0;
                }
                if bottom <= right {
                    rect.min.y = top.y + size.y + 1.0;
                }
            }
        }
        rect.min += window.min;
        rect.max = rect.min + size;
        placed.push(rect);
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;

    #[test]
    fn repeated_original_cache_coordinates_keep_authored_order_and_fractional_position() {
        let cache = content::scan(Asset { path: "maps/mapcache.ini".to_owned(), origin: "fixture".to_owned(),
            bytes: b"MapCache sample\n techPosition = X:7.25 Y:20 Z:0\n techPosition = X:11 Y:33.5 Z:0\n End\n".to_vec() }).unwrap();
        assert_eq!(
            positions(&cache.declarations[0], "techPosition").unwrap(),
            [Vec2::new(7.25, 20.0), Vec2::new(11.0, 33.5)]
        );
        assert!(
            positions(&cache.declarations[0], "supplyPosition")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            marker(
                Rect::new(0.0, 0.0, 100.0, 50.0),
                Vec2::splat(100.0),
                Vec2::new(7.25, 20.0),
                Vec2::splat(15.0)
            ),
            // Native's 15-pixel marker subtracts integer half-size 7: 40 - 7.
            Rect::new(0.0, 33.0, 15.0, 48.0)
        );
    }

    #[test]
    fn original_overlap_displacement_preserves_strict_edges_ties_and_coordinate_frames() {
        let extent = Vec2::splat(100.0);
        let size = Vec2::splat(10.0);
        let positions = [Vec2::new(30.0, 70.0), Vec2::new(35.0, 65.0)];
        let placed = starts(Rect::new(0.0, 0.0, 100.0, 100.0), extent, &positions, size);
        assert_eq!(placed[0].min, Vec2::splat(25.0));
        assert_eq!(placed[1].min, Vec2::splat(36.0));
        let shifted = starts(
            Rect::new(400.0, 100.0, 500.0, 200.0),
            extent,
            &positions,
            size,
        );
        assert_eq!(shifted[1].min, Vec2::new(430.0, 130.0));
        let touching = starts(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            extent,
            &[positions[0], positions[0]],
            size,
        );
        assert_eq!(touching[0], touching[1]);
    }
}

#[cfg(test)]
#[test]
fn odd_start_marker_dimensions_use_native_integer_half_size() {
    let rect = marker(
        Rect::from_corners(Vec2::ZERO, Vec2::new(200.0, 100.0)),
        Vec2::new(400.0, 200.0),
        Vec2::new(200.0, 100.0),
        Vec2::new(16.0, 17.0),
    );
    assert_eq!(rect.min, Vec2::new(92.0, 42.0));
    assert_eq!(rect.max, Vec2::new(108.0, 59.0));
}
