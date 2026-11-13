//! Ordered `LocomotorSet` selection at `AIUpdateInterface::doLocomotor`.
use super::{Locomotor, set};
#[cfg(test)]
mod tests;
use crate::skirmish::{Entity, Skirmish, UnitAiUpdate, UnitType};

pub struct LocomotorVariant {
    pub speed: f32,
    pub locomotor: Locomotor,
}

pub struct LocomotorSets {
    /// Additional normal-set locomotors after the primary `UnitType` locomotor.
    pub normal: Vec<LocomotorVariant>,
    pub wander: Vec<LocomotorVariant>,
    pub panic: Vec<LocomotorVariant>,
}

impl LocomotorSets {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            normal: Vec::new(),
            wander: Vec::new(),
            panic: Vec::new(),
        }
    }
}

impl UnitType {
    pub(in crate::skirmish) fn locomotor_in_set(
        &self,
        role: u8,
        index: usize,
    ) -> Option<&Locomotor> {
        if index == 0
            && !matches!(role, set::NORMAL | set::WANDER | set::TAXI)
            && let Some(loco) = self.aircraft.as_ref().and_then(|rules| {
                rules
                    .attacking
                    .as_ref()
                    .filter(|loco| loco.role == role)
                    .or_else(|| rules.returning.as_ref().filter(|loco| loco.role == role))
            })
        {
            return Some(&loco.locomotor);
        }
        match role {
            set::SLUGGISH if self.combat_bike.is_some() => self
                .combat_bike
                .as_ref()?
                .sluggish
                .get(index)
                .map(|variant| &variant.locomotor),
            set::NORMAL if index == 0 => Some(&self.locomotor),
            set::NORMAL => self
                .locomotor_sets
                .normal
                .get(index - 1)
                .map(|variant| &variant.locomotor),
            set::WANDER if !self.locomotor_sets.wander.is_empty() => self
                .locomotor_sets
                .wander
                .get(index)
                .map(|variant| &variant.locomotor),
            set::PANIC if !self.locomotor_sets.panic.is_empty() => self
                .locomotor_sets
                .panic
                .get(index)
                .map(|variant| &variant.locomotor),
            _ if index != 0 => None,
            set::UPGRADED => self
                .maintenance
                .movement_upgrade
                .as_ref()
                .map(|upgrade| &upgrade.locomotor),
            set::WANDER => self.behavior.mobs.first().map(|rules| &rules.wander),
            set::PANIC => self
                .behavior
                .mobs
                .first()
                .map(|rules| &rules.panic)
                .or_else(|| {
                    self.behavior
                        .slaves
                        .first()
                        .and_then(|rules| rules.panic.as_ref())
                }),
            set::TAXI => self
                .aircraft
                .as_ref()
                .map(|rules| &rules.taxi.locomotor)
                .or_else(|| self.chinook.as_ref().map(|rules| &rules.taxi)),
            set::FREEFALL => self.parachute.as_ref().map(|rules| &rules.freefall),
            _ => None,
        }
    }

    fn locomotor_for_surface(&self, role: u8, surfaces: u8) -> Option<usize> {
        (0..)
            .map_while(|index| self.locomotor_in_set(role, index))
            .position(|locomotor| locomotor.legal_surfaces & surfaces != 0)
    }

    pub(in crate::skirmish) fn locomotor_surfaces(&self) -> u8 {
        self.locomotor_set_surfaces(set::NORMAL)
    }

    pub(in crate::skirmish) fn locomotor_set_surfaces(&self, role: u8) -> u8 {
        (0..)
            .map_while(|index| self.locomotor_in_set(role, index))
            .fold(0, |surfaces, locomotor| surfaces | locomotor.legal_surfaces)
    }
}

impl Entity {
    pub(in crate::skirmish) fn locomotor_surfaces(&self, unit: &UnitType) -> u8 {
        unit.locomotor_set_surfaces(self.ai.locomotor_set)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn select_surface_locomotor(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if actor.ai.surface_motion.is_empty() {
            return;
        }
        let acceptable = self.movement_terrain.locomotor_surfaces(actor.position);
        let selected = unit.locomotor_for_surface(actor.ai.locomotor_set, acceptable);
        // With no matching cell surface, the original keeps its previous loco.
        let selected = selected.unwrap_or(actor.ai.surface_locomotor);
        if selected != actor.ai.surface_locomotor {
            actor
                .ai
                .motion
                .swap_surface_state(&mut actor.ai.surface_motion[actor.ai.surface_locomotor]);
            actor
                .ai
                .motion
                .swap_surface_state(&mut actor.ai.surface_motion[selected]);
            actor.ai.motion.clear_surface_precision();
            actor.ai.surface_locomotor = selected;
        }
    }

    pub(in crate::skirmish) fn choose_locomotor_set(&mut self, id: usize, requested: u8) {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let role = if actor.landed() {
            set::TAXI
        } else {
            actor.requested_aircraft_locomotor(unit, requested)
        };
        let role = if role == set::NORMAL && actor.upgraded_locomotor(unit).is_some() {
            set::UPGRADED
        } else {
            role
        };
        if matches!(unit.ai.update, UnitAiUpdate::Absent)
            || actor.ai.locomotor_set == role
            || unit.locomotor_in_set(role, 0).is_none()
        {
            return;
        }
        actor.ai.locomotor_set = role;
        self.initialize_locomotor_set(id);
    }

    /// `AIUpdate` creates only the selected role, in its authored template order.
    pub(in crate::skirmish) fn initialize_locomotor_set(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if matches!(unit.ai.update, UnitAiUpdate::Absent) {
            return;
        }
        actor.ai.surface_motion = (0..)
            .map_while(|index| unit.locomotor_in_set(actor.ai.locomotor_set, index))
            .filter(|locomotor| locomotor.wander_length.is_some())
            .map(|locomotor| {
                super::SavedGroundState::from_template(self.tick, locomotor, &mut self.random)
            })
            .collect();
        // A new set has no previous locomotor: unlike later terrain changes, it
        // falls back to the first GROUND template if the birth surface is invalid.
        let acceptable = self.movement_terrain.locomotor_surfaces(actor.position);
        actor.ai.surface_locomotor = unit
            .locomotor_for_surface(actor.ai.locomotor_set, acceptable)
            .or_else(|| unit.locomotor_for_surface(actor.ai.locomotor_set, 1))
            .unwrap_or(0);
        if let Some(state) = actor.ai.surface_motion.get_mut(actor.ai.surface_locomotor) {
            actor.ai.motion.swap_surface_state(state);
        }
        // AIUpdate::chooseGoodLocomotorFromCurrentSet clears flags after
        // every new selected locomotor, including pure flight roles.
        actor.ai.motion.clear_surface_precision();
        if let Some(flight) = &mut actor.flight {
            flight.replace_locomotor();
        }
    }

    pub(in crate::skirmish) fn sample_climb_terrain(&mut self, id: usize) {
        let actor = &self.entities[id];
        if !actor
            .active_locomotor(&self.catalog[actor.type_index])
            .ground
            .as_ref()
            .is_some_and(|rules| matches!(rules.appearance, super::GroundAppearance::Climber))
        {
            return;
        }
        let Some(goal) = actor.ai.motion.climb_goal() else {
            return;
        };
        let position = actor.position;
        let mut direction = [goal[0] - position[0], goal[1] - position[1]];
        // Coord3D::normalize rounds the squared components before summing.
        let x_squared = direction[0] * direction[0];
        let y_squared = direction[1] * direction[1];
        let length = (x_squared + y_squared).sqrt();
        if length != 0.0 {
            direction[0] /= length;
            direction[1] /= length;
        }
        let ahead = [position[0] + direction[0], position[1] + direction[1]];
        let samples = [
            self.object_position(id)[2],
            goal[2],
            self.ground_height(ahead),
        ];
        self.entities[id].ai.motion.climb_heights = samples;
    }
}
