//! Original partition looker counts, discrete reveal circles and object fog memory.

use std::collections::VecDeque;

use super::Skirmish;
use crate::collision_geometry::{Body, coverage};

mod circle;
mod clear_region;
pub use clear_region::ClearRegion;
#[cfg(test)]
mod tests;

pub enum Status {
    Clear,
    Partial,
    Fogged,
    Shrouded,
}

pub enum RadarPriority {
    None,
    Unit,
    LocalUnit,
    Structure,
}

pub struct Sight {
    pub clearing_range: f32,
    pub reveal_to_all_range: f32,
    pub reveal_to_all: bool,
    pub immobile: bool,
    pub mine: bool,
    pub radar_priority: RadarPriority,
}

impl Sight {
    #[must_use]
    pub const fn new(clearing_range: f32) -> Self {
        Self {
            clearing_range,
            reveal_to_all_range: 0.0,
            reveal_to_all: false,
            immobile: false,
            mine: false,
            radar_priority: RadarPriority::None,
        }
    }
}

pub struct VisibilityRules {
    pub cell_size: f32,
    pub unlook_persist_frames: u64,
    pub use_shroud: bool,
}

struct Look {
    center: [i32; 2],
    radius: i32,
    owners: Vec<usize>,
}

struct Pending {
    look: Look,
    deadline: u64,
}

struct Record {
    center: [i32; 2],
    range: f32,
    reveal_range: f32,
    owner: usize,
    spy_mask: u16,
    look: Option<Look>,
    reveal: Option<Look>,
    seen: Vec<bool>,
    statuses: Vec<Status>,
}

pub(super) struct Visibility {
    rules: VisibilityRules,
    dimensions: [u16; 2],
    cells: Vec<Vec<i32>>,
    permanent: Vec<bool>,
    records: Vec<Option<Record>>,
    pending: VecDeque<Pending>,
}

impl Visibility {
    pub(super) fn for_map(layout: &super::MapLayout, extent: [f32; 2], players: usize) -> Self {
        Self::new(
            VisibilityRules {
                cell_size: layout.partition_cell_size,
                unlook_persist_frames: 30,
                use_shroud: true,
            },
            extent,
            players,
        )
    }

    pub(super) fn new(rules: VisibilityRules, world_size: [f32; 2], players: usize) -> Self {
        let inverse = 1.0 / rules.cell_size;
        let dimensions = world_size.map(|size| dimension((size * inverse).ceil()));
        let initial = i32::from(rules.use_shroud);
        Self {
            rules,
            dimensions,
            cells: vec![
                vec![initial; usize::from(dimensions[0]) * usize::from(dimensions[1])];
                players
            ],
            permanent: vec![false; players],
            records: Vec::new(),
            pending: VecDeque::new(),
        }
    }

    pub(super) fn reset(&mut self, players: usize) {
        self.records.clear();
        self.pending.clear();
        self.permanent = vec![false; players];
        self.cells.resize_with(players, || {
            vec![1; usize::from(self.dimensions[0]) * usize::from(self.dimensions[1])]
        });
        for cells in &mut self.cells {
            cells.fill(i32::from(self.rules.use_shroud));
        }
    }

    fn center(&self, point: [f32; 2]) -> [i32; 2] {
        cell_center(self.rules.cell_size, point)
    }

    fn look(&mut self, center: [i32; 2], range: f32, owners: Vec<usize>) -> Option<Look> {
        if range <= 0.0 || owners.is_empty() {
            return None;
        }
        let radius = integer((range * (1.0 / self.rules.cell_size)).ceil()).max(1);
        let look = Look {
            center,
            radius,
            owners,
        };
        self.change(&look, true);
        Some(look)
    }

    fn change(&mut self, look: &Look, add: bool) {
        let indices = circle::cells(look.center, look.radius, self.dimensions);
        for &owner in &look.owners {
            for &index in &indices {
                let count = &mut self.cells[owner][index];
                if add {
                    *count = (*count - 1).min(-1);
                } else if *count == -1 {
                    *count = 0;
                } else {
                    assert!(*count < 0, "unbalanced native shroud looker");
                    *count += 1;
                }
            }
        }
    }

    fn unlook(&mut self, look: Option<Look>, tick: u64) {
        if let Some(look) = look {
            self.pending.push_back(Pending {
                look,
                deadline: tick + self.rules.unlook_persist_frames,
            });
        }
    }

    fn expire(&mut self, tick: u64) {
        while self
            .pending
            .front()
            .is_some_and(|item| item.deadline < tick)
        {
            let pending = self.pending.pop_front().expect("pending looker");
            self.change(&pending.look, false);
        }
    }

    fn cell(&self, owner: usize, point: [f32; 2]) -> Status {
        let [x, y] = self.center(point);
        let Ok(x) = u16::try_from(x) else {
            return Status::Shrouded;
        };
        let Ok(y) = u16::try_from(y) else {
            return Status::Shrouded;
        };
        if x >= self.dimensions[0] || y >= self.dimensions[1] {
            return Status::Shrouded;
        }
        let index = usize::from(y) * usize::from(self.dimensions[0]) + usize::from(x);
        match self.cells[owner][index] {
            1 => Status::Shrouded,
            0 => Status::Fogged,
            _ => Status::Clear,
        }
    }
}

impl Skirmish {
    pub(super) fn reveal_defeated_player(&mut self, owner: usize) {
        // VictoryConditions::update calls the permanent reveal only after frame 1.
        if self.tick > 1 && !self.visibility.permanent[owner] {
            self.visibility.permanent[owner] = true;
            for cell in &mut self.visibility.cells[owner] {
                *cell = (*cell - 1).min(-1);
            }
        }
    }

    /// Installs source partition size and delayed-unlook timing.
    ///
    /// # Errors
    /// Rejects a nonfinite or nonpositive partition cell size.
    pub fn configure_visibility(&mut self, rules: VisibilityRules) -> Result<(), &'static str> {
        if !rules.cell_size.is_finite() || rules.cell_size < 1.0 {
            return Err("invalid visibility partition size");
        }
        self.visibility = Visibility::new(
            rules,
            [
                f32::from(self.grid.width()) * 10.0,
                f32::from(self.grid.height()) * 10.0,
            ],
            self.players.len(),
        );
        for id in 0..self.entities.len() {
            self.observe_visibility(id);
        }
        self.update_visibility();
        Ok(())
    }

    pub(super) fn observe_visibility(&mut self, id: usize) {
        self.visibility
            .records
            .resize_with(self.entities.len(), || None);
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let center = self.visibility.center(actor.position);
        // Object::look permits garrison occupants to reveal from their current
        // fire point; other enclosed transports suppress both kinds of looking.
        let can_look = actor.exists()
            && !self.update_objects[actor.update_owner].destroy_requested
            && !actor.is_effectively_dead()
            && actor
                .contained_by()
                .is_none_or(|host| self.is_garrison(host));
        let range = if !can_look {
            0.0
        } else if actor.under_construction() {
            super::movement::bounding_radius(&unit.footprint(actor.position, actor.facing))
        } else {
            actor.shroud_range(unit)
        };
        let concealed = actor.stealthed() && !actor.detected() && !actor.has_native_disguise();
        let reveal_range = if can_look && !actor.under_construction() && !concealed {
            unit.sight.reveal_to_all_range
        } else {
            0.0
        };
        let spy_mask = actor.vision_spy_mask();
        let prior = self.visibility.records[id].take();
        if let Some(record) = prior.as_ref()
            && record.center == center
            && record.range.to_bits() == range.to_bits()
            && record.reveal_range.to_bits() == reveal_range.to_bits()
            && record.owner == actor.owner
            && record.spy_mask == spy_mask
        {
            self.visibility.records[id] = prior;
            return;
        }
        let owners: Vec<_> = (0..self.players.len())
            .filter(|&owner| {
                unit.sight.reveal_to_all
                    || self.allied(actor.owner, owner)
                    || spy_mask & (1 << owner) != 0
            })
            .collect();
        let other_owners: Vec<_> = (0..self.players.len())
            .filter(|&owner| !self.allied(actor.owner, owner))
            .collect();
        let mut record = prior.unwrap_or_else(|| Record {
            center,
            range,
            reveal_range,
            owner: actor.owner,
            spy_mask,
            look: None,
            reveal: None,
            seen: vec![false; self.players.len()],
            statuses: (0..self.players.len()).map(|_| Status::Shrouded).collect(),
        });
        self.visibility.unlook(record.look.take(), self.tick);
        self.visibility.unlook(record.reveal.take(), self.tick);
        record.center = center;
        record.range = range;
        record.reveal_range = reveal_range;
        record.owner = actor.owner;
        record.spy_mask = spy_mask;
        record.look = self.visibility.look(center, range, owners);
        record.reveal = self.visibility.look(center, reveal_range, other_owners);
        self.visibility.records[id] = Some(record);
    }

    pub(super) fn update_visibility(&mut self) {
        self.visibility.expire(self.tick);
        let neutral = self.neutral_owner();
        for (id, actor) in self.entities.iter().enumerate() {
            let Some(record) = self.visibility.records.get_mut(id).and_then(Option::as_mut) else {
                continue;
            };
            let unit = &self.catalog[actor.type_index];
            let body = Body {
                geometry: &unit.geometry,
                position: [actor.position[0], actor.position[1], 0.0],
                angle: actor.facing,
            };
            let cells = coverage(
                &body,
                unit.geometry.is_small,
                self.visibility.rules.cell_size,
                self.visibility.dimensions,
            );
            for owner in 0..self.players.len() {
                let values = cells.iter().map(|[x, y]| {
                    self.visibility.cells[owner][usize::from(*y)
                        * usize::from(self.visibility.dimensions[0])
                        + usize::from(*x)]
                });
                let shrouded = values.clone().filter(|&value| value == 1).count();
                let fogged = values.filter(|&value| value == 0).count();
                record.statuses[owner] = if cells.is_empty() || shrouded == cells.len() {
                    record.seen[owner] = false;
                    Status::Shrouded
                } else if shrouded + fogged == cells.len() {
                    if unit.sight.immobile
                        && (actor.owner == neutral || record.seen[owner])
                        && !unit.sight.mine
                    {
                        Status::Fogged
                    } else {
                        Status::Shrouded
                    }
                } else {
                    record.seen[owner] = true;
                    if shrouded + fogged == 0 {
                        Status::Clear
                    } else {
                        Status::Partial
                    }
                };
            }
        }
    }

    #[must_use]
    pub fn visibility_status(&self, owner: usize, id: usize) -> &Status {
        self.visibility
            .records
            .get(id)
            .and_then(Option::as_ref)
            .and_then(|record| record.statuses.get(owner))
            .unwrap_or(&Status::Shrouded)
    }

    /// Native human automatic acquisition requires every covered cell clear.
    #[must_use]
    pub fn visible_to(&self, owner: usize, id: usize) -> bool {
        matches!(self.visibility_status(owner, id), Status::Clear)
    }

    /// Partition coverage status before the immobile-object ghost policy.
    #[must_use]
    pub fn visibility_footprint(&self, owner: usize, body: &Body<'_>) -> Status {
        let cells = coverage(
            body,
            body.geometry.is_small,
            self.visibility.rules.cell_size,
            self.visibility.dimensions,
        );
        let values = cells.iter().map(|[x, y]| {
            self.visibility.cells[owner]
                [usize::from(*y) * usize::from(self.visibility.dimensions[0]) + usize::from(*x)]
        });
        let shrouded = values.clone().filter(|&value| value == 1).count();
        let fogged = values.filter(|&value| value == 0).count();
        if cells.is_empty() || shrouded == cells.len() {
            Status::Shrouded
        } else if shrouded + fogged == cells.len() {
            Status::Fogged
        } else if shrouded + fogged == 0 {
            Status::Clear
        } else {
            Status::Partial
        }
    }

    #[must_use]
    pub fn visibility_cell(&self, owner: usize, point: [f32; 2]) -> Status {
        self.visibility.cell(owner, point)
    }

    #[must_use]
    pub const fn visibility_dimensions(&self) -> [u16; 2] {
        self.visibility.dimensions
    }

    #[must_use]
    pub const fn visibility_cell_size(&self) -> f32 {
        self.visibility.rules.cell_size
    }

    /// `RadarUpgrade` contributes immediately when its creation upgrade executes.
    #[must_use]
    pub fn has_radar(&self, owner: usize) -> bool {
        // The local elimination callback forces radar on independently of providers.
        if owner == 0 && self.defeated[owner] {
            return true;
        }
        self.players
            .get(owner)
            .is_some_and(|player| player.radar.available())
    }
}

fn cell_center(cell_size: f32, point: [f32; 2]) -> [i32; 2] {
    let inverse = 1.0 / cell_size;
    point.map(|value| integer((value * inverse).floor()))
}

fn integer(value: f32) -> i32 {
    // Source map cell coordinates fit signed 16 bits. Binary conversion avoids
    // changing floor/ceil behavior through a saturating, lossy float cast.
    assert!(value.is_finite() && (f32::from(i16::MIN)..=f32::from(i16::MAX)).contains(&value));
    let (mut low, mut high) = (i32::from(i16::MIN), i32::from(i16::MAX));
    while low < high {
        let middle = low + (high - low) / 2;
        let real = f32::from(i16::try_from(middle).expect("bounded cell conversion"));
        if real < value {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

fn dimension(value: f32) -> u16 {
    u16::try_from(integer(value)).expect("bounded map partition")
}

impl Skirmish {
    pub(in crate::skirmish) fn set_shroud_clearing_range(&mut self, id: usize, range: f32) {
        let actor = &self.entities[id];
        if actor
            .shroud_range(&self.catalog[actor.type_index])
            .partial_cmp(&range)
            != Some(std::cmp::Ordering::Equal)
        {
            self.entities[id].shroud_range_override = Some(range);
            if self
                .object_position(id)
                .iter()
                .any(|coordinate| *coordinate != 0.0)
            {
                self.observe_visibility(id);
            }
        }
    }
}
