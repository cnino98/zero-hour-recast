//! Native partition dirty, cell-member and contact ordering for live objects.

use super::Skirmish;
use crate::collision_geometry::{Body, coverage, intersects};

mod potential;
mod query;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub(in crate::skirmish) enum Identity {
    Actor(usize),
    Map(usize),
}

enum Dirty {
    None,
    Collision,
    Cells,
}

struct Record {
    position: [f32; 3],
    angle: f32,
    cells: Vec<usize>,
    dirty: Dirty,
}

struct Contact {
    pair: [usize; 2],
    next_hash: Option<usize>,
}

struct ContactList {
    heads: Vec<Option<usize>>,
    contacts: Vec<Contact>,
}

impl ContactList {
    fn new() -> Self {
        Self {
            heads: vec![None; 5381],
            contacts: Vec::new(),
        }
    }

    fn add(&mut self, id: usize, other: usize) {
        if id == other {
            return;
        }
        // PartitionContactList::hash2ints is symmetric and uses 32-bit IDs.
        // Hash membership never determines the separate callback-list order.
        let hash = id.min(other).wrapping_shl(16).wrapping_add(id.max(other)) & 0xffff_ffff;
        let socket = hash % self.heads.len();
        let mut current = self.heads[socket];
        while let Some(index) = current {
            let contact = &self.contacts[index];
            if contact.pair == [id, other] || contact.pair == [other, id] {
                return;
            }
            current = contact.next_hash;
        }
        self.contacts.push(Contact {
            pair: [id, other],
            next_hash: self.heads[socket],
        });
        self.heads[socket] = Some(self.contacts.len() - 1);
    }
}

pub(super) struct Partition {
    cell_size: f32,
    dimensions: [u16; 2],
    cells: Vec<Vec<Identity>>,
    map_records: Vec<Option<Record>>,
    records: Vec<Option<Record>>,
    // Appending is native prepend; popping visits the current linked-list head.
    dirty: Vec<Identity>,
}

impl Partition {
    pub(super) fn new(cell_size: f32, world_size: [f32; 2]) -> Self {
        let cell_size = cell_size.max(1.0);
        let inverse = 1.0 / cell_size;
        let dimensions = world_size.map(|extent| ceil_dimension(extent.max(1.0) * inverse));
        Self {
            cell_size,
            dimensions,
            cells: (0..usize::from(dimensions[0]) * usize::from(dimensions[1]))
                .map(|_| Vec::new())
                .collect(),
            records: Vec::new(),
            map_records: Vec::new(),
            dirty: Vec::new(),
        }
    }

    pub(in crate::skirmish) fn registered(&self, id: usize) -> bool {
        self.records.get(id).is_some_and(Option::is_some)
    }

    pub(super) fn clear(&mut self) {
        self.records.clear();
        self.map_records.clear();
        self.dirty.clear();
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub(in crate::skirmish) fn observe(&mut self, id: usize, position: [f32; 3], angle: f32) {
        self.records
            .resize_with(self.records.len().max(id + 1), || None);
        let changed = if let Some(record) = &mut self.records[id] {
            let changed = position
                .iter()
                .zip(record.position)
                .any(|(current, previous)| (*current - previous).abs() > 0.01)
                || (angle - record.angle).abs() > 0.01;
            // Object::reactToTransformChange compares consecutive transforms,
            // including changes too small to invalidate partition coverage.
            record.position = position;
            record.angle = angle;
            changed
        } else {
            self.records[id] = Some(Record {
                position,
                angle,
                cells: Vec::new(),
                dirty: Dirty::None,
            });
            true
        };
        if changed {
            self.make_dirty(id, true);
        }
    }

    pub(super) fn remove(&mut self, id: usize) {
        let Some(record) = self.records.get_mut(id).and_then(Option::take) else {
            return;
        };
        for cell in record.cells {
            self.cells[cell].retain(|&member| member != Identity::Actor(id));
        }
        self.dirty.retain(|&member| member != Identity::Actor(id));
    }

    fn make_dirty(&mut self, id: usize, cells: bool) {
        let Some(record) = self.records.get_mut(id).and_then(Option::as_mut) else {
            return;
        };
        if matches!(record.dirty, Dirty::None) {
            self.dirty.push(Identity::Actor(id));
            record.dirty = Dirty::Collision;
        }
        if cells {
            record.dirty = Dirty::Cells;
        }
    }

    fn take_dirty(&mut self) -> Option<(Identity, bool)> {
        let id = self.dirty.pop()?;
        let record = match id {
            Identity::Actor(index) => &mut self.records[index],
            Identity::Map(index) => &mut self.map_records[index],
        }
        .as_mut()
        .expect("registered dirty object");
        let update_cells = matches!(record.dirty, Dirty::Cells);
        record.dirty = Dirty::None;
        Some((id, update_cells))
    }

    fn replace_cells(&mut self, id: usize, cells: Vec<[u16; 2]>) {
        let record = self.records[id]
            .as_mut()
            .expect("registered partition object");
        for cell in record.cells.drain(..) {
            self.cells[cell].retain(|&member| member != Identity::Actor(id));
        }
        for [x, y] in cells {
            let cell = usize::from(y) * usize::from(self.dimensions[0]) + usize::from(x);
            self.cells[cell].insert(0, Identity::Actor(id));
            record.cells.push(cell);
        }
    }

    fn observe_map(&mut self, id: usize, position: [f32; 3], angle: f32) {
        self.map_records
            .resize_with(self.map_records.len().max(id + 1), || None);
        if self.map_records[id].is_none() {
            self.map_records[id] = Some(Record {
                position,
                angle,
                cells: Vec::new(),
                dirty: Dirty::Cells,
            });
            self.dirty.push(Identity::Map(id));
        }
    }

    fn replace_map_cells(&mut self, id: usize, cells: Vec<[u16; 2]>) {
        let record = self.map_records[id]
            .as_mut()
            .expect("registered map object");
        for cell in record.cells.drain(..) {
            self.cells[cell].retain(|&member| member != Identity::Map(id));
        }
        for [x, y] in cells {
            let cell = usize::from(y) * usize::from(self.dimensions[0]) + usize::from(x);
            self.cells[cell].insert(0, Identity::Map(id));
            record.cells.push(cell);
        }
    }

    pub(super) fn projectile_candidates(&self, body: &Body<'_>) -> Vec<usize> {
        let cells = coverage(
            body,
            body.geometry.is_small,
            self.cell_size,
            self.dimensions,
        );
        let mut candidates = Vec::new();
        for [x, y] in cells {
            let cell = usize::from(y) * usize::from(self.dimensions[0]) + usize::from(x);
            for &identity in &self.cells[cell] {
                let Identity::Actor(id) = identity else {
                    continue;
                };
                if !candidates.contains(&id) {
                    candidates.push(id);
                }
            }
        }
        // Native contact records prepend each first-seen pair.
        candidates.reverse();
        candidates
    }

    fn add_candidates(&self, id: usize, contacts: &mut ContactList) {
        let record = self.records[id]
            .as_ref()
            .expect("registered partition object");
        for &cell in &record.cells {
            for &identity in &self.cells[cell] {
                let Identity::Actor(other) = identity else {
                    continue;
                };
                // Native unordered-pair dedup preserves the first pair's
                // callback orientation and prepends each newly found pair.
                contacts.add(id, other);
            }
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn observe_map_partition(&mut self, index: usize) {
        let object = &self.layout.world_objects[index];
        self.partition
            .observe_map(index, object.position, object.angle);
    }

    pub(super) fn rebuild_object_partition_geometry(&mut self, id: usize) {
        if self.partition.records.get(id).is_some_and(Option::is_some) {
            self.partition.remove(id);
            self.partition
                .observe(id, self.object_position(id), self.entities[id].facing);
        }
    }

    pub(in crate::skirmish) fn register_object_partition(&mut self, id: usize) {
        self.partition
            .observe(id, self.object_position(id), self.entities[id].facing);
    }

    pub(in crate::skirmish) fn note_drawable_transform(&mut self, id: usize, force: bool) {
        let transform = self.object_transform(id);
        let drawable = &mut self.entities[id].drawable;
        let bits = |(position, basis): ([f32; 3], [[f32; 3]; 3])| {
            (
                position.map(f32::to_bits),
                basis.map(|row| row.map(f32::to_bits)),
            )
        };
        if force || drawable.notified_transform.map(bits) != Some(bits(transform)) {
            drawable.transform_revision = drawable.transform_revision.wrapping_add(1);
            drawable.notified_transform = Some(transform);
        }
    }

    pub(super) fn observe_ground_partition(&mut self, id: usize) {
        self.note_drawable_transform(id, false);
        let position = self.object_position(id);
        let actor = &self.entities[id];
        let changed = position
            .iter()
            .zip(actor.drawable.transform_position)
            .any(|(new, old)| (*new - old).abs() > 0.01)
            || (actor.facing - actor.drawable.transform_angle).abs() > 0.01;
        self.entities[id].drawable.transform_angle = self.entities[id].facing;
        if changed {
            self.containment_transform_changed(id);
        }
        self.notify_terrain_movement(id);
        self.observe_map_status(id);
        self.observe_guard_triggers(id);
        self.observe_visibility(id);
        let actor = &self.entities[id];
        // Native partition membership lasts until explicit unregistration;
        // health and NO_COLLISIONS are separate query/contact predicates.
        if !actor.exists() {
            self.partition.remove(id);
        } else if self.partition.records.get(id).is_some_and(Option::is_some) {
            self.partition
                .observe(id, self.object_position(id), actor.facing);
        }
    }

    fn observe_map_status(&mut self, id: usize) {
        let position = self.object_position(id);
        let actor = &mut self.entities[id];
        let changed = position
            .iter()
            .zip(actor.drawable.transform_position)
            .any(|(new, old)| (*new - old).abs() > 0.01);
        actor.drawable.transform_position = position;
        if changed {
            let extent = self.playable_extent;
            actor.set_off_map(
                !(extent[0][0] < position[0]
                    && position[0] < extent[1][0]
                    && extent[0][1] < position[1]
                    && position[1] < extent[1][1]),
            );
        }
    }

    pub(super) fn update_ground_contacts(&mut self) {
        // PartitionContactList is local to PartitionManager::update. Only the
        // re-dirtied objects and their cell membership survive into next frame.
        let mut contacts = ContactList::new();
        while let Some((identity, update_cells)) = self.partition.take_dirty() {
            let Identity::Actor(id) = identity else {
                let Identity::Map(index) = identity else {
                    unreachable!()
                };
                if update_cells {
                    let object = &self.layout.world_objects[index];
                    let cells = coverage(
                        &Body {
                            geometry: &object.geometry,
                            position: object.position,
                            angle: object.angle,
                        },
                        object.geometry.is_small,
                        self.partition.cell_size,
                        self.partition.dimensions,
                    );
                    self.partition.replace_map_cells(index, cells);
                }
                continue;
            };
            if update_cells {
                let actor = &self.entities[id];
                let body = Body {
                    geometry: self.body_geometry(id),
                    position: self.object_position(id),
                    angle: actor.facing,
                };
                let cells = coverage(
                    &body,
                    body.geometry.is_small,
                    self.partition.cell_size,
                    self.partition.dimensions,
                );
                self.partition.replace_cells(id, cells);
            }
            if !self.catalog[self.entities[id].type_index].immobile() {
                self.partition.add_candidates(id, &mut contacts);
            }
        }
        for contact in contacts.contacts.into_iter().rev() {
            let [id, other] = contact.pair;
            if !self.ground_contact_intersects(id, other) {
                continue;
            }
            let ignored = self.entities[id].ai.ignored_producer == Some(other);
            self.object_contact(id, other, ignored);
            if !self.update_objects[self.entities[id].update_owner].destroy_requested
                && !self.update_objects[self.entities[other].update_owner].destroy_requested
            {
                let ignored = self.entities[other].ai.ignored_producer == Some(id);
                self.object_contact(other, id, ignored);
            }
            // Even a no-op live AI callback redirties surviving mobile objects.
            self.redirty_contact(id);
            self.redirty_contact(other);
        }
    }

    fn ground_contact_intersects(&self, id: usize, other: usize) -> bool {
        let actor = &self.entities[id];
        let target = &self.entities[other];
        let unit = &self.catalog[actor.type_index];
        let target_type = &self.catalog[target.type_index];
        if !self.collision_present(id)
            || !self.collision_present(other)
            || (unit.immobile() && target_type.immobile())
            || self.body_no_collisions(id)
            || self.body_no_collisions(other)
        {
            return false;
        }
        intersects(
            &Body {
                geometry: self.body_geometry(id),
                position: self.object_position(id),
                angle: actor.facing,
            },
            &Body {
                geometry: self.body_geometry(other),
                position: self.object_position(other),
                angle: target.facing,
            },
        )
    }

    fn redirty_contact(&mut self, id: usize) {
        if self.collision_present(id) && !self.catalog[self.entities[id].type_index].immobile() {
            self.partition.make_dirty(id, false);
        }
    }

    fn collision_present(&self, id: usize) -> bool {
        self.entities[id].exists()
            && !self.update_objects[self.entities[id].update_owner].destroy_requested
            && self.partition.records.get(id).is_some_and(Option::is_some)
    }
}

fn ceil_dimension(value: f32) -> u16 {
    assert!(value.is_finite() && value > 0.0 && value <= f32::from(u16::MAX));
    u16::try_from(crate::native_real::ceil(value)).expect("native partition dimension")
}
