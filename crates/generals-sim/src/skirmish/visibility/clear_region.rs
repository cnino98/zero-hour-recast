//! Callback-time `FXList::doFXPos` point-cell visibility, SH 2bfaea60.
use super::{Skirmish, Visibility, cell_center};

/// Owned clear-player masks for a clipped rectangle of partition cells.
/// Later look/unlook and permanent reveals cannot change a captured FX decision.
pub struct ClearRegion {
    cell_size: f32,
    minimum: [u16; 2],
    dimensions: [u16; 2],
    cells: Vec<u16>,
}

impl ClearRegion {
    fn new(visibility: &Visibility, minimum: [f32; 2], maximum: [f32; 2]) -> Self {
        assert!(visibility.cells.len() <= 16, "native player mask capacity");
        let first = visibility.center(minimum);
        let last = visibility.center(maximum);
        let minimum = std::array::from_fn::<_, 2, _>(|axis| {
            first[axis].clamp(0, i32::from(visibility.dimensions[axis]))
        });
        let maximum = std::array::from_fn::<_, 2, _>(|axis| {
            last[axis].clamp(-1, i32::from(visibility.dimensions[axis]) - 1)
        });
        let dimensions = std::array::from_fn::<_, 2, _>(|axis| {
            u16::try_from((maximum[axis] - minimum[axis] + 1).max(0)).expect("clipped region cells")
        });
        let mut cells = Vec::with_capacity(usize::from(dimensions[0]) * usize::from(dimensions[1]));
        if !dimensions.contains(&0) {
            for y in minimum[1]..=maximum[1] {
                for x in minimum[0]..=maximum[0] {
                    let index = usize::try_from(y).expect("clipped cell Y")
                        * usize::from(visibility.dimensions[0])
                        + usize::try_from(x).expect("clipped cell X");
                    let mut mask = 0;
                    for (owner, player) in visibility.cells.iter().enumerate() {
                        // PartitionCell::getShroudStatusForPlayer: 1 shrouded,
                        // 0 fogged, every other counter value exactly CLEAR.
                        if !matches!(player[index], 0 | 1) {
                            mask |= 1 << owner;
                        }
                    }
                    cells.push(mask);
                }
            }
        }
        Self {
            cell_size: visibility.rules.cell_size,
            minimum: minimum.map(|value| u16::try_from(value).expect("clipped region origin")),
            dimensions,
            cells,
        }
    }

    /// Tests the one original point cell, not prop four-cell partial visibility.
    /// Points outside the captured cells or native map cells are not clear.
    ///
    /// # Panics
    /// Coordinates must fit the live partition's finite signed-cell conversion range.
    #[must_use]
    pub fn is_clear(&self, viewer: usize, native_xy: [f32; 2]) -> bool {
        let Some(bit) = u32::try_from(viewer)
            .ok()
            .and_then(|viewer| 1_u16.checked_shl(viewer))
        else {
            return false;
        };
        let point = cell_center(self.cell_size, native_xy);
        let offset =
            std::array::from_fn::<_, 2, _>(|axis| point[axis] - i32::from(self.minimum[axis]));
        if (0..2).any(|axis| offset[axis] < 0 || offset[axis] >= i32::from(self.dimensions[axis])) {
            return false;
        }
        let index = usize::try_from(offset[1]).expect("bounded region Y")
            * usize::from(self.dimensions[0])
            + usize::try_from(offset[0]).expect("bounded region X");
        self.cells[index] & bit != 0
    }
}

impl Skirmish {
    /// Captures the current clear-player masks for the rectangular point-cell bounds.
    /// This preserves source FX visibility at a terrain movement callback, before
    /// later partition movement, look/unlook or defeat-reveal callbacks.
    ///
    /// # Panics
    /// Coordinates must fit the live partition's finite signed-cell conversion range.
    /// The native player-mask capacity is sixteen players.
    #[must_use]
    pub fn terrain_visibility_region(&self, minimum: [f32; 2], maximum: [f32; 2]) -> ClearRegion {
        ClearRegion::new(&self.visibility, minimum, maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Status, VisibilityRules};
    use super::*;

    fn map(cell_size: f32, players: usize) -> Visibility {
        Visibility::new(
            VisibilityRules {
                cell_size,
                unlook_persist_frames: 30,
                use_shroud: true,
            },
            [400.0; 2],
            players,
        )
    }

    #[test]
    fn compact_region_keeps_independent_clear_masks_and_uses_exactly_one_point_cell() {
        let mut map = map(40.0, 16);
        let _look = map.look([5, 5], 40.0, vec![0, 7, 15]);
        let region = ClearRegion::new(&map, [181.0; 2], [219.0; 2]);
        assert_eq!(region.dimensions, [2; 2]);
        assert_eq!(region.cells.len(), 4); // Four u16 values, not sixteen full maps.
        for viewer in 0..16 {
            for point in [[181.0; 2], [200.0; 2], [219.0; 2], [200.0, 181.0]] {
                assert_eq!(
                    region.is_clear(viewer, point),
                    matches!(map.cell(viewer, point), Status::Clear)
                );
            }
        }
        assert!(region.is_clear(15, [200.0; 2]));
        assert!(!region.is_clear(1, [200.0; 2]));
        assert!(!region.is_clear(16, [200.0; 2]));
        assert!(!region.is_clear(usize::MAX, [200.0; 2]));
        // The native radius-one circle has a clear cell immediately below, but
        // that cell is outside this bounded callback snapshot.
        assert!(matches!(map.cell(0, [200.0, 240.0]), Status::Clear));
        assert!(!region.is_clear(0, [200.0, 240.0]));
        assert!(!region.is_clear(0, [199.99, 200.0]));
    }

    #[test]
    fn snapshots_survive_new_looks_and_expired_unlooks_without_using_fog_memory_as_clear() {
        let mut map = map(40.0, 2);
        let look = map.look([5, 5], 40.0, vec![0]);
        let before = ClearRegion::new(&map, [190.0; 2], [210.0; 2]);
        map.unlook(look, 4);
        let _new_look = map.look([5, 5], 40.0, vec![1]);
        map.expire(35);
        let after = ClearRegion::new(&map, [190.0; 2], [210.0; 2]);
        assert!(matches!(map.cell(0, [200.0; 2]), Status::Fogged));
        assert!(before.is_clear(0, [200.0; 2]));
        assert!(!before.is_clear(1, [200.0; 2]));
        assert!(!after.is_clear(0, [200.0; 2]));
        assert!(after.is_clear(1, [200.0; 2]));
    }

    #[test]
    fn shared_binary32_reciprocal_conversion_preserves_cells_at_authored_boundaries() {
        let mut map = map(41.0, 1);
        let _look = map.look([0, 0], 41.0, vec![0]);
        // binary32 41 * (1 / 41) is .99999994, not the division result 1.0.
        // Snapshot bounds and queries must share the live partition convention.
        let edge = [41.0, 0.0];
        let next = [f32::from_bits(41.0_f32.to_bits() + 1), 0.0];
        let region = ClearRegion::new(&map, edge, edge);
        assert_eq!(region.minimum, [0; 2]);
        assert_eq!(region.dimensions, [1; 2]);
        assert!(region.is_clear(0, edge));
        assert!(matches!(map.cell(0, edge), Status::Clear));
        assert!(!region.is_clear(0, next));
        assert!(matches!(map.cell(0, next), Status::Shrouded));
    }

    #[test]
    fn negative_and_outer_map_points_do_not_alias_edge_cells_and_disjoint_bounds_are_empty() {
        let mut map = map(40.0, 1);
        map.cells[0].fill(-1);
        let region = ClearRegion::new(&map, [-80.0; 2], [480.0; 2]);
        assert_eq!(region.minimum, [0; 2]);
        assert_eq!(region.dimensions, [10; 2]);
        for point in [[0.0; 2], [399.99; 2]] {
            assert!(region.is_clear(0, point));
        }
        for point in [[-0.01, 0.0], [0.0, -40.0], [400.0, 0.0], [0.0, 400.0]] {
            assert!(!region.is_clear(0, point));
            assert!(!matches!(map.cell(0, point), Status::Clear));
        }
        for (minimum, maximum) in [
            ([-80.0; 2], [-1.0; 2]),
            ([400.0; 2], [480.0; 2]),
            ([240.0; 2], [120.0; 2]),
        ] {
            let empty = ClearRegion::new(&map, minimum, maximum);
            assert!(empty.cells.is_empty());
            assert!(!empty.is_clear(0, [0.0; 2]));
        }
    }

    #[test]
    fn public_callback_snapshot_is_unchanged_by_permanent_defeat_reveal() {
        let mut game = super::super::super::tests::game();
        game.entities.clear();
        game.partition.clear();
        game.configure_visibility(VisibilityRules {
            cell_size: 40.0,
            unlook_persist_frames: 30,
            use_shroud: true,
        })
        .unwrap();
        let point = [200.0; 2];
        let before = game.terrain_visibility_region([190.0; 2], [210.0; 2]);
        assert!(!before.is_clear(0, point));
        game.tick = 2;
        game.reveal_defeated_player(0);
        assert!(matches!(game.visibility_cell(0, point), Status::Clear));
        let after = game.terrain_visibility_region([190.0; 2], [210.0; 2]);
        assert!(!before.is_clear(0, point));
        assert!(after.is_clear(0, point));
    }
}
