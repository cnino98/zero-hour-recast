//! Native Hacker command adapters and actual `DISABLED_HACKED` expiry.
use super::special_ability::PowerType;
use super::{Entity, Skirmish};

pub enum Effect {
    Disabled { target: usize, lifetime: u32 },
    DisabledEdge { target: usize },
    Reenabled { target: usize },
}

pub struct Event {
    pub object: usize,
    pub frame: u64,
    pub effect: Effect,
}

impl Entity {
    #[must_use]
    pub const fn hacked(&self) -> bool {
        self.disabled_status & (1 << 1) != 0
    }
}

impl Skirmish {
    fn disable_building_power(&self, id: usize) -> Option<&str> {
        self.entities.get(id)?;
        let ordinal = self.first_special_ability(id, PowerType::SpecialHackerDisableBuilding)?;
        self.catalog[self.entities[id].type_index]
            .behavior
            .special_abilities[ordinal]
            .power
            .as_ref()
            .map(|power| power.name.as_str())
    }

    pub fn disable_building_ready(&mut self, id: usize) -> bool {
        let eligible = self.entities.get(id).is_some_and(|actor| {
            actor.complete() && !self.object_disabled(id) && actor.contained_by().is_none()
        });
        if !eligible {
            return false;
        }
        let Some(power) = self.disable_building_power(id).map(str::to_owned) else {
            return false;
        };
        self.special_power_ready(id, &power)
    }

    /// Explicit `SPECIAL_POWER` targeting differs from the ordinary context action.
    #[must_use]
    pub fn can_disable_building(&mut self, id: usize, target: usize) -> bool {
        self.disable_target(id, target, false)
    }

    #[must_use]
    pub fn can_context_disable_building(&mut self, id: usize, target: usize) -> bool {
        self.disable_target(id, target, true)
    }

    fn disable_target(&mut self, id: usize, target: usize, context: bool) -> bool {
        let ready = self.disable_building_ready(id);
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        let unit = &self.catalog[victim.type_index];
        let kind = |name| unit.kind_of.iter().any(|value| value == name);
        ready
            && victim.alive()
            && !victim.masked()
            && self.enemies(actor.owner, victim.owner)
            && self.maintenance_target_visible(actor.owner, target)
            && kind("STRUCTURE")
            && !kind("REBUILD_HOLE")
            && (kind("CAPTURABLE")
                || (context && kind("FS_TECHNOLOGY") && !kind("IMMUNE_TO_CAPTURE")))
            && (!context || !victim.under_construction())
    }

    /// # Errors
    /// Rejects an unavailable ability or a target outside its source admission rules.
    pub fn disable_building(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_disable_building(id, target) {
            return Err("Cannot disable this building");
        }
        self.begin_disable_building(id, target)
    }

    /// # Errors
    /// Rejects unfinished buildings and other invalid native context targets.
    pub fn context_disable_building(
        &mut self,
        id: usize,
        target: usize,
    ) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_context_disable_building(id, target) {
            return Err("Cannot disable this building");
        }
        self.begin_disable_building(id, target)
    }

    fn begin_disable_building(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        let Some(power) = self.disable_building_power(id).map(str::to_owned) else {
            return Err("Hacker disable ability is unavailable");
        };
        if self.special_power_at_object(id, &power, target) {
            Ok(())
        } else {
            Err("Hacker disable ability rejected the command")
        }
    }

    pub(super) fn expire_hacked(&mut self) {
        let now = self.update_frame();
        for target in 0..self.entities.len() {
            if self.entities[target].hacked()
                && u64::from(now) >= self.entities[target].hacked_until
            {
                self.clear_hacked_disable(target);
            }
        }
    }
}
