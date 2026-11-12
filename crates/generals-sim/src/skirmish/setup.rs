//! `GameLogic::populateRandomStartPosition` and player-slot relationships.
#[cfg(test)]
mod height_tests;
mod identity;
mod resolution;
pub use resolution::{ResolvedSlots, resolve_player_slots};
#[cfg(test)]
mod map_properties_tests;
#[cfg(test)]
mod selectable_tests;
#[cfg(test)]
mod tests;

use super::{Faction, OpponentDifficulty, Skirmish};
use num_traits::ToPrimitive;

// A copied menu selection is retained for restart, independently of live players.
#[derive(Clone)]
pub struct PlayerSlot {
    pub faction: Option<Faction>,
    pub occupied: bool,
    pub difficulty: Option<OpponentDifficulty>,
    pub team: Option<u8>,
    pub start: Option<usize>,
    pub color: Option<usize>,
}

pub struct NeutralObject {
    pub initial_health: Option<i32>,
    pub attitude: Option<i32>,
    pub recruitable: Option<bool>,
    pub original_team: String,
    pub script_name: Option<String>,
    pub name: String,
    pub position: [f32; 3],
    pub angle: f32,
    pub script_targetable: bool,
    pub indestructible: bool,
    pub selectable: Option<bool>,
}

impl Skirmish {
    /// Selects the base seed for the next match; restart retains this choice.
    pub const fn set_random_seed(&mut self, seed: u32) {
        self.layout.random_seed = seed;
    }

    #[must_use]
    pub const fn random_seed(&self) -> u32 {
        self.layout.random_seed
    }

    /// Resets every occupied slot on the current original map.
    /// # Errors
    /// Rejects conflicting starts, invalid slots, or unavailable starting units.
    pub fn reset_slots(&mut self, slots: Vec<PlayerSlot>) -> Result<(), &'static str> {
        let color_count = self
            .progression
            .as_ref()
            .map_or(8, |rules| rules.multiplayer_colors.len());
        let starts: Vec<_> = self
            .layout
            .starts
            .iter()
            .map(|start| start.position)
            .collect();
        let resolved = resolve_player_slots(&slots, self.layout.random_seed, color_count, &starts)?;
        self.slots = slots;
        self.occupied = self
            .slots
            .iter()
            .map(|slot| slot.occupied)
            .chain([false])
            .collect();
        self.defeated = vec![false; self.occupied.len()];
        self.random = resolved.random;
        self.player_colors = resolved.colors;
        self.start_positions = resolved.start_positions;
        self.reset_players(resolved.factions)
    }

    pub(super) fn legacy_slots(&self, faction: Faction, enemy: Option<Faction>) -> Vec<PlayerSlot> {
        (0..self.layout.starts.len())
            .map(|owner| PlayerSlot {
                faction: Some(if owner == 0 {
                    faction
                } else {
                    enemy.unwrap_or(faction)
                }),
                occupied: owner == 0 || (owner == 1 && enemy.is_some()),
                difficulty: if owner == 1 {
                    self.opponents
                        .get(owner)
                        .and_then(|opponent| opponent.as_ref())
                        .map(super::opponent::Opponent::difficulty)
                } else {
                    None
                },
                team: None,
                start: Some(owner),
                color: Some(owner),
            })
            .collect()
    }

    #[must_use]
    pub const fn neutral_owner(&self) -> usize {
        self.layout.starts.len()
    }

    #[must_use]
    pub fn player_active(&self, owner: usize) -> bool {
        self.occupied.get(owner).copied().unwrap_or(false) && !self.defeated[owner]
    }

    #[must_use]
    pub fn allied(&self, first: usize, second: usize) -> bool {
        self.map_side_relationship(first, second).map_or_else(
            || allied(&self.slots, first, second),
            |relation| relation == 2,
        )
    }

    #[must_use]
    pub fn enemies(&self, first: usize, second: usize) -> bool {
        if let Some(relation) = self.map_side_relationship(first, second) {
            return relation == 0;
        }
        self.occupied.get(first).copied().unwrap_or(false)
            && self.occupied.get(second).copied().unwrap_or(false)
            && !self.allied(first, second)
    }

    #[must_use]
    pub fn victorious(&self, owner: usize) -> bool {
        self.outcome
            .is_some_and(|winner| self.allied(owner, winner))
    }

    #[must_use]
    pub fn player_start(&self, owner: usize) -> Option<&super::PlayerStart> {
        self.layout
            .starts
            .get(self.start_positions.get(owner).copied().flatten()?)
    }

    #[must_use]
    pub fn named_map_object(&self, name: &str) -> Option<usize> {
        self.map_objects.get(name).copied()
    }

    // Object::updateObjValuesFromMapProperties: initial health, then AI properties.
    fn apply_map_initial_controls(
        &mut self,
        id: usize,
        initial_health: Option<i32>,
        attitude: Option<i32>,
        recruitable: Option<bool>,
    ) {
        let kind = self.entities[id].type_index;
        if let Some(percent) = initial_health
            && self.catalog[kind].body_kind.is_active()
        {
            let factor = percent.to_f32().expect("native signed health percentage") / 100.0;
            let desired = factor * self.entities[id].damage_state.initial_health;
            self.change_body_health(id, desired - self.entities[id].health);
        }
        if self.entities[id].ai.registration.is_some() {
            if let Some(value) = attitude {
                self.entities[id].ai.mood.attitude = value;
            }
            if let Some(value) = recruitable {
                self.entities[id].ai.recruitable = value;
            }
        }
    }

    pub(in crate::skirmish) fn spawn_neutral_object(
        &mut self,
        index: usize,
        bridge: bool,
    ) -> Result<usize, &'static str> {
        let record = &self.layout.neutral_objects[index];
        let type_index = self
            .catalog
            .iter()
            .position(|unit| unit.name == record.name)
            .ok_or("Map neutral object is missing its original template")?;
        let [x, y, z] = record.position;
        if !z.is_finite() {
            return Err("Nonfinite neutral map object height is unsupported");
        }
        let angle = record.angle;
        let script_name = record.script_name.clone();
        let civilian_owner = record.original_team == "teamPlyrCivilian";
        let script_targetable = record.script_targetable;
        let indestructible = record.indestructible;
        let selectable = record.selectable;
        let initial_health = record.initial_health;
        let attitude = record.attitude;
        let recruitable = record.recruitable;
        let (owner, original_team) = if bridge {
            (self.neutral_owner(), "team".into())
        } else {
            self.placed_map_team(&record.original_team)
        };
        let team = self.team_membership.find_team(owner, &original_team);
        let id = self.new_object_on_team(type_index, owner, team);
        self.set_object_orientation(id, super::locomotor::normalize_angle(angle));
        let ground = self.ground_height([x, y]);
        let height = z + ground;
        self.set_object_position(id, [x, y, height]);
        if bridge && self.attack_kind(id, "WALK_ON_TOP_OF_WALL") {
            self.movement_terrain.add_wall_piece(id);
        }
        self.register_map_member(id, original_team);
        self.apply_map_initial_controls(id, initial_health, attitude, recruitable);
        if let Some(value) = selectable
            && value != self.entities[id].selectable(&self.catalog[type_index])
        {
            self.set_object_selectable(id, value);
        }
        self.set_map_body_controls(id, script_targetable, indestructible);
        if !bridge {
            let layer = self
                .movement_terrain
                .get_layer_for_destination([x, y, height], ground);
            self.entities[id].layer = layer;
            let floor = self
                .movement_terrain
                .destination_layer_height(layer, [x, y], ground);
            self.entities[id].placed_height_offset = Some(height - floor);
            self.update_position_claim(id);
        }
        self.native_build_complete(id);
        self.adopt_neutral_obstacle(index, id);
        if civilian_owner && !bridge {
            self.civilian_objects.insert(id);
        }
        if let Some(name) = script_name {
            self.map_objects.insert(name, id);
        }
        self.observe_ground_partition(id);
        Ok(id)
    }

    pub(super) fn spawn_neutral_objects(&mut self) -> Result<(), &'static str> {
        for placement in 0..self.layout.world_object_order.len() {
            match self.layout.world_object_order[placement] {
                super::MapWorldPlacement::Object(index) => self.observe_map_partition(index),
                super::MapWorldPlacement::Tree(index) => {
                    self.record_terrain_movement(super::terrain_movement::Event::TreeAdded(index));
                }
                super::MapWorldPlacement::Neutral(index) => {
                    if self.registered_neutral_object(index).is_none() {
                        self.spawn_neutral_object(index, false)?;
                    }
                }
            }
        }
        if self.layout.world_object_order.is_empty() {
            for index in 0..self.layout.neutral_objects.len() {
                if self.registered_neutral_object(index).is_none() {
                    self.spawn_neutral_object(index, false)?;
                }
            }
        }
        self.bind_map_bridges();
        self.initialize_map_build_lists()?;
        self.rebuild_navigation();
        Ok(())
    }
}

pub(super) fn allied(slots: &[PlayerSlot], first: usize, second: usize) -> bool {
    first == second
        || slots
            .get(first)
            .zip(slots.get(second))
            .is_some_and(|(a, b)| a.occupied && b.occupied && a.team.is_some() && a.team == b.team)
}

impl super::Entity {
    /// Authored Z for immutable map structures, before adding terrain height.
    #[must_use]
    pub const fn placed_height_offset(&self) -> Option<f32> {
        self.placed_height_offset
    }
}
