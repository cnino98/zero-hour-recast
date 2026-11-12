//! Infantry/Black Lotus action adapters select their authored native power.
use super::{Entity, FIRING_A, PACKING, PowerType, RAISING_FLAG, Skirmish, UNPACKING};

const fn slot(kind: PowerType) -> Option<usize> {
    match kind {
        PowerType::SpecialInfantryCaptureBuilding | PowerType::SpecialBlacklotusCaptureBuilding => {
            Some(0)
        }
        PowerType::SpecialBlacklotusDisableVehicleHack => Some(1),
        PowerType::SpecialBlacklotusStealCashHack => Some(2),
        _ => None,
    }
}

impl Entity {
    #[must_use]
    pub fn capture_condition(&self) -> Option<&'static str> {
        if !self
            .behaviors
            .special_abilities
            .iter()
            .any(|state| state.active && slot(state.kind).is_some())
        {
            return None;
        }
        let flags = &self.drawable.model_conditions;
        [
            (UNPACKING, "UNPACKING"),
            (RAISING_FLAG, "RAISING_FLAG"),
            (PACKING, "PACKING"),
            (FIRING_A, "FIRING_A"),
        ]
        .into_iter()
        .find_map(|(flag, name)| flags.has_flag(flag).then_some(name))
    }

    #[must_use]
    pub fn finite_ability_index(&self) -> usize {
        self.behaviors
            .special_abilities
            .iter()
            .find(|state| state.active && slot(state.kind).is_some())
            .and_then(|state| slot(state.kind))
            .unwrap_or(0)
    }

    #[must_use]
    pub fn lotus_stream_target(&self) -> Option<usize> {
        self.behaviors
            .special_abilities
            .iter()
            .find(|state| {
                state.active
                    && matches!(slot(state.kind), Some(0..=2))
                    && state.kind != PowerType::SpecialInfantryCaptureBuilding
                    && state.preparing.frames > 0
            })
            .and_then(|state| state.target)
    }
}

impl Skirmish {
    fn finite_power(&self, id: usize, ability: usize) -> Option<&super::Power> {
        let actor = self.entities.get(id)?;
        self.catalog[actor.type_index]
            .behavior
            .special_abilities
            .iter()
            .filter_map(|rules| rules.power.as_ref())
            .find(|power| slot(power.kind) == Some(ability))
    }

    #[must_use]
    pub fn capture_ready(&mut self, id: usize) -> bool {
        self.finite_ability_ready(id, 0)
    }

    #[must_use]
    pub fn capture_progress(&mut self, id: usize) -> f32 {
        self.finite_ability_progress(id, 0)
    }

    #[must_use]
    pub fn capture_in_use(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            actor
                .behaviors
                .special_abilities
                .iter()
                .enumerate()
                .any(|(ordinal, state)| {
                    slot(state.kind).is_some() && actor.special_ability_in_use(ordinal)
                })
        })
    }

    #[must_use]
    pub fn finite_ability_ready(&mut self, id: usize, ability: usize) -> bool {
        if !self.entities.get(id).is_some_and(|actor| {
            actor.complete() && !self.object_disabled(id) && actor.contained_by().is_none()
        }) {
            return false;
        }
        let Some(name) = self
            .finite_power(id, ability)
            .map(|power| power.name.clone())
        else {
            return false;
        };
        self.special_power_ready(id, &name)
    }

    #[must_use]
    pub fn finite_ability_progress(&mut self, id: usize, ability: usize) -> f32 {
        let Some(name) = self
            .finite_power(id, ability)
            .map(|power| power.name.clone())
        else {
            return 0.0;
        };
        self.special_power_percent(id, &name)
    }

    #[must_use]
    pub fn can_capture(&mut self, id: usize, target: usize) -> bool {
        if !self.capture_ready(id) {
            return false;
        }
        if self.entities.get(target).is_none() {
            return false;
        }
        let Some((name, kind)) = self
            .finite_power(id, 0)
            .map(|power| (power.name.clone(), power.kind))
        else {
            return false;
        };
        self.can_special_power_at_object_from(
            id,
            &name,
            kind,
            target,
            crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
        )
    }

    /// # Errors
    /// Rejects unavailable powers and ineligible native capture targets.
    pub fn capture(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_capture(id, target) {
            return Err("Cannot capture this building");
        }
        self.begin_finite_ability(id, target, 0)
    }

    #[must_use]
    pub fn can_hack(&mut self, id: usize, target: usize, ability: usize) -> bool {
        if !matches!(ability, 1 | 2) || !self.finite_ability_ready(id, ability) {
            return false;
        }
        if self.entities.get(target).is_none() {
            return false;
        }
        let Some(name) = self
            .finite_power(id, ability)
            .map(|power| power.name.clone())
        else {
            return false;
        };
        let kind = if ability == 1 {
            PowerType::SpecialBlacklotusDisableVehicleHack
        } else {
            PowerType::SpecialBlacklotusStealCashHack
        };
        self.can_special_power_at_object_from(
            id,
            &name,
            kind,
            target,
            crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
        )
    }

    /// # Errors
    /// Rejects unavailable powers and targets excluded by native `ActionManager`.
    pub fn hack(&mut self, id: usize, target: usize, ability: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_hack(id, target, ability) {
            return Err("Cannot hack this target");
        }
        self.begin_finite_ability(id, target, ability)
    }

    fn begin_finite_ability(
        &mut self,
        id: usize,
        target: usize,
        ability: usize,
    ) -> Result<(), &'static str> {
        let Some(name) = self
            .finite_power(id, ability)
            .map(|power| power.name.clone())
        else {
            return Err("Finite special power is unavailable");
        };
        if self.special_power_at_object(id, &name, target) {
            Ok(())
        } else {
            Err("Finite special power rejected the command")
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(in crate::skirmish) mod fixture;
