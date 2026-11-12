//! Native `FASTER_GCO` outward cell order, retaining each cell's COI order.
use super::{Identity, Partition};
use num_traits::ToPrimitive;

impl Partition {
    pub(in crate::skirmish) fn outward_world_range_rings(
        &self,
        position: [f32; 2],
        range: f32,
    ) -> Vec<(f64, Identity)> {
        let mut cells = self.range_cells(position, range);
        cells.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        let mut actors_seen = vec![false; self.records.len()];
        let mut maps_seen = vec![false; self.map_records.len()];
        let mut result = Vec::new();
        for (radius, cell) in cells {
            for &id in &self.cells[cell] {
                let seen = match id {
                    Identity::Actor(index) => &mut actors_seen[index],
                    Identity::Map(index) => &mut maps_seen[index],
                };
                if !*seen {
                    *seen = true;
                    result.push((radius, id));
                }
            }
        }
        result
    }

    pub(in crate::skirmish) fn outward_range_rings(
        &self,
        position: [f32; 2],
        range: f32,
    ) -> Vec<(f64, usize)> {
        let mut cells = self.range_cells(position, range);
        cells.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        let mut seen = vec![false; self.records.len()];
        let mut result = Vec::new();
        for (radius, cell) in cells {
            for &identity in &self.cells[cell] {
                let Identity::Actor(id) = identity else {
                    continue;
                };
                if !seen[id] {
                    seen[id] = true;
                    result.push((radius, id));
                }
            }
        }
        result
    }
    // Horde only inspects membership; keep its existing bounded Y/X order.
    pub(in crate::skirmish) fn range_objects(&self, position: [f32; 2], radius: f32) -> Vec<usize> {
        self.objects_in_cells(self.range_cells(position, radius))
    }

    // FASTER_GCO rings/Y/X/COIs, then SimpleObjectIterator prepend and ITER_FASTEST.
    pub(in crate::skirmish) fn fastest_range_objects(
        &self,
        position: [f32; 2],
        radius: f32,
    ) -> Vec<usize> {
        let mut cells = self.range_cells(position, radius);
        cells.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        let mut result = self.objects_in_cells(cells);
        result.reverse();
        result
    }

    fn range_cells(&self, position: [f32; 2], radius: f32) -> Vec<(f64, usize)> {
        let inverse = 1.0 / self.cell_size;
        let center = position.map(|value| {
            crate::native_real::floor(value * inverse)
                .to_f32()
                .expect("native partition coordinate")
        });
        let maximum = crate::native_real::ceil(radius * inverse)
            .to_f32()
            .expect("native partition radius");
        let edges = std::array::from_fn::<_, 2, _>(|axis| {
            let limit = f32::from(self.dimensions[axis] - 1);
            [
                (center[axis] - maximum - 1.0)
                    .clamp(0.0, limit)
                    .to_u16()
                    .expect("partition lower bound"),
                (center[axis] + maximum + 1.0)
                    .clamp(0.0, limit)
                    .to_u16()
                    .expect("partition upper bound"),
            ]
        });
        let mut cells = Vec::new();
        for y in edges[1][0]..=edges[1][1] {
            for x in edges[0][0]..=edges[0][1] {
                let offset = [f32::from(x) - center[0], f32::from(y) - center[1]];
                let distance = min_radius(offset, self.cell_size);
                if distance > f64::from(maximum) {
                    continue;
                }
                let index = usize::from(y) * usize::from(self.dimensions[0]) + usize::from(x);
                if !self.cells[index].is_empty() {
                    cells.push((distance, index));
                }
            }
        }
        cells
    }

    fn objects_in_cells(&self, cells: Vec<(f64, usize)>) -> Vec<usize> {
        let mut result = Vec::new();
        for (_, cell) in cells {
            for &identity in &self.cells[cell] {
                let Identity::Actor(id) = identity else {
                    continue;
                };
                if !result.contains(&id) {
                    result.push(id);
                }
            }
        }
        result
    }

    pub(in crate::skirmish) fn outward_objects(&self, position: [f32; 2]) -> Vec<usize> {
        self.ordered_objects(position, f64::INFINITY)
    }

    pub(in crate::skirmish) fn outward_range(&self, position: [f32; 2], range: f32) -> Vec<usize> {
        self.ordered_objects(
            position,
            f64::from(crate::native_real::ceil(range * (1.0 / self.cell_size))),
        )
    }

    pub(in crate::skirmish) fn outward_rings(&self, position: [f32; 2]) -> Vec<(f64, usize)> {
        let mut seen = vec![false; self.records.len()];
        let mut result = Vec::new();
        for (radius, cell) in self.ordered_cells(position, f64::INFINITY) {
            for &identity in &self.cells[cell] {
                let Identity::Actor(id) = identity else {
                    continue;
                };
                if !seen[id] {
                    seen[id] = true;
                    result.push((radius, id));
                }
            }
        }
        result
    }

    fn ordered_cells(&self, position: [f32; 2], maximum: f64) -> Vec<(f64, usize)> {
        let inverse = 1.0 / self.cell_size;
        let center = position.map(|value| {
            crate::native_real::floor(value * inverse)
                .to_f32()
                .expect("native partition coordinate")
        });
        let mut cells = Vec::new();
        for (index, members) in self.cells.iter().enumerate() {
            if members.is_empty() {
                continue;
            }
            let width = usize::from(self.dimensions[0]);
            let x = f32::from(u16::try_from(index % width).expect("partition column"));
            let y = f32::from(u16::try_from(index / width).expect("partition row"));
            let offset = [x - center[0], y - center[1]];
            if offset
                .iter()
                .any(|value| f64::from(value.abs()) > maximum + 1.0)
            {
                continue;
            }
            let radius = min_radius(offset, self.cell_size);
            if radius <= maximum {
                cells.push((radius, index));
            }
        }
        // calcRadiusVec inserts offsets in Y/X order within each radius. Empty
        // cells have no observable effect; retain that order among occupied cells.
        cells.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        cells
    }

    fn ordered_objects(&self, position: [f32; 2], maximum: f64) -> Vec<usize> {
        let cells = self.ordered_cells(position, maximum);
        let mut seen = vec![false; self.records.len()];
        let mut result = Vec::new();
        for (_, cell) in cells {
            for &identity in &self.cells[cell] {
                let Identity::Actor(id) = identity else {
                    continue;
                };
                if !seen[id] {
                    seen[id] = true;
                    result.push(id);
                }
            }
        }
        result
    }
}

impl crate::skirmish::Skirmish {
    pub(in crate::skirmish) fn closest_cleanup_hazard(
        &self,
        source: usize,
        position: [f32; 2],
        range: f32,
    ) -> Option<usize> {
        let mut closest = None;
        let mut distance = range * range;
        let mut last_ring = f64::INFINITY;
        for (ring, id) in self.partition.outward_range_rings(position, range) {
            if ring > last_ring {
                break;
            }
            let actor = &self.entities[id];
            let x = actor.position[0] - position[0];
            let y = actor.position[1] - position[1];
            let y_squared = y * y;
            let x_squared = x * x;
            let squared = x_squared + y_squared;
            if squared < distance
                && actor.is_off_map() == self.entities[source].is_off_map()
                && self.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "CLEANUP_HAZARD")
            {
                closest = Some(id);
                distance = squared;
                last_ring = ring;
            }
        }
        closest
    }

    pub(in crate::skirmish) fn closest_heal_pad(&self, source: usize, range: f32) -> Option<usize> {
        let position = self.entities[source].position;
        let mut closest = None;
        let mut closest_squared = range * range;
        for id in self.partition.fastest_range_objects(position, range) {
            let actor = &self.entities[id];
            let x = actor.position[0] - position[0];
            let y = actor.position[1] - position[1];
            let y_squared = y * y;
            let x_squared = x * x;
            let squared = x_squared + y_squared;
            if squared < closest_squared
                && self.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "HEAL_PAD")
            {
                closest = Some(id);
                closest_squared = squared;
            }
        }
        closest
    }

    pub(in crate::skirmish) fn firestorm_damage_targets(
        &self,
        source: usize,
        range: f32,
    ) -> Vec<usize> {
        let position = self.entities[source].position;
        self.partition
            .fastest_range_objects(position, range)
            .into_iter()
            .filter(|&id| {
                let actor = &self.entities[id];
                let x = actor.position[0] - position[0];
                let y = actor.position[1] - position[1];
                let y_squared = y * y;
                let x_squared = x * x;
                let center_squared = x_squared + y_squared;
                let geometry = self.body_geometry(id);
                let radius = if matches!(geometry.shape, crate::collision_geometry::Shape::Box) {
                    let squared = [
                        geometry.major * geometry.major,
                        geometry.minor * geometry.minor,
                    ];
                    (squared[0] + squared[1]).sqrt()
                } else {
                    geometry.major
                };
                let squared = if radius > 0.0 {
                    let distance = center_squared.sqrt() - radius;
                    if distance <= 0.0 {
                        0.0
                    } else {
                        distance * distance
                    }
                } else {
                    center_squared
                };
                squared < range * range
            })
            .collect()
    }

    pub(in crate::skirmish) fn closest_flammable(
        &self,
        source: usize,
        range: f32,
    ) -> Option<usize> {
        let position = self.object_position(source);
        let mut closest = None;
        let mut distance = range * range;
        let mut last_ring = f64::INFINITY;
        // FASTER_GCO finishes the first successful radius, then stops. It does
        // not globally sort objects by distance or break ties by object ID.
        for (ring, id) in self
            .partition
            .outward_range_rings([position[0], position[1]], range)
        {
            if ring > last_ring {
                break;
            }
            if id == source {
                continue;
            }
            let other = self.object_position(id);
            let x = other[0] - position[0];
            let y = other[1] - position[1];
            let z = other[2] - position[2];
            let components = [x * x, y * y, z * z];
            let xy = components[0] + components[1];
            let squared = xy + components[2];
            if squared < distance && self.entities[id].flammable_would_ignite() {
                closest = Some(id);
                distance = squared;
                last_ring = ring;
            }
        }
        closest
    }
}

fn min_radius(offset: [f32; 2], cell_size: f32) -> f64 {
    let half = cell_size * 0.5;
    let center = [[-half, -half], [half, -half], [-half, half], [half, half]];
    let point = offset.map(|value| value * cell_size);
    let other = center.map(|corner| [point[0] + corner[0], point[1] + corner[1]]);
    let mut minimum = 1.0e12_f64;
    for left in center {
        for right in other {
            let x = f64::from(left[0] - right[0]);
            let y = f64::from(left[1] - right[1]);
            let y_squared = y * y;
            let x_squared = x * x;
            minimum = minimum.min(x_squared + y_squared);
        }
    }
    // sqrtf receives the native double minimum converted to Real first.
    let minimum = minimum.to_f32().expect("finite bounded partition distance");
    let ratio = (f64::from(minimum.sqrt()) / f64::from(cell_size))
        .to_f32()
        .expect("native radius conversion");
    f64::from(crate::native_real::ceil(ratio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_fastest_range_keeps_ring_cell_coi_prepend_order_and_excludes_distant_records() {
        let mut partition = Partition::new(40.0, [400.0; 2]);
        for id in 0..5 {
            partition.observe(id, [200.0; 3], 0.0);
        }
        partition.replace_cells(0, vec![[5, 2]]);
        partition.replace_cells(1, vec![[8, 5]]);
        partition.replace_cells(2, vec![[5, 5], [5, 6]]);
        partition.replace_cells(3, vec![[5, 5]]);
        // Native ceil(160 / 40) is five, so offset [4,4] still belongs
        // to this broadphase query. [5,5] lies beyond its native radius.
        partition.replace_cells(4, vec![[10, 10]]);
        assert_eq!(
            partition.fastest_range_objects([200.0; 2], 160.0),
            [1, 0, 2, 3]
        );
        assert_eq!(partition.range_objects([200.0; 2], 160.0), [0, 3, 2, 1]);
    }

    #[test]
    fn radius_ties_keep_native_row_column_and_latest_coi_order_once_per_object() {
        let mut partition = Partition::new(40.0, [400.0; 2]);
        for id in 0..3 {
            partition.observe(id, [200.0; 3], 0.0);
        }
        partition.replace_cells(0, vec![[6, 4], [6, 5]]);
        partition.replace_cells(1, vec![[4, 4], [5, 4]]);
        partition.replace_cells(2, vec![[4, 4]]);
        assert_eq!(partition.outward_objects([200.0; 2]), [2, 1, 0]);
        partition.remove(2);
        assert_eq!(partition.outward_objects([200.0; 2]), [1, 0]);
        // Re-registering prepends the source COI without changing cell order.
        partition.observe(2, [200.0; 3], 0.0);
        partition.replace_cells(2, vec![[6, 4]]);
        assert_eq!(partition.outward_objects([200.0; 2]), [1, 2, 0]);
    }
}

#[cfg(test)]
mod world_identity_tests {
    use super::*;

    #[test]
    fn raw_and_actor_objects_share_the_same_coi_and_dirty_order() {
        let mut partition = Partition::new(40.0, [400.0; 2]);
        partition.observe(0, [200.0; 3], 0.0);
        partition.observe_map(0, [200.0; 3], 0.0);
        partition.observe(1, [200.0; 3], 0.0);
        while let Some((id, _)) = partition.take_dirty() {
            match id {
                Identity::Actor(id) => partition.replace_cells(id, vec![[5, 5]]),
                Identity::Map(id) => partition.replace_map_cells(id, vec![[5, 5]]),
            }
        }
        let order: Vec<_> = partition
            .outward_world_range_rings([200.0; 2], 100.0)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        assert_eq!(
            order,
            [Identity::Actor(0), Identity::Map(0), Identity::Actor(1)]
        );
        assert_eq!(partition.outward_range([200.0; 2], 100.0), [0, 1]);
    }
}
