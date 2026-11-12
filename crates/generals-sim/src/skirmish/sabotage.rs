//! Sabotage crate consumers and explicit `AI_ENTER` goals, SH 2bfaea60.
use super::{DeathType, Order, Skirmish};

#[cfg(test)]
mod tests;

pub enum Effect {
    PowerPlant { frames: u32 },
    Superweapon,
    CommandCenter,
    SupplyCenter { amount: u32 },
    MilitaryFactory { frames: u32 },
    FakeBuilding,
    InternetCenter { frames: u32 },
}

pub struct Rules {
    pub effect: Effect,
    pub required: Vec<String>,
    pub forbidden: Vec<String>,
}

pub struct Event {
    pub frame: u64,
    pub source: usize,
    pub target: usize,
    pub module: usize,
    pub cash: u32,
}

impl Effect {
    fn accepts(&self, kinds: &[String]) -> bool {
        let has = |kind| kinds.iter().any(|value| value == kind);
        match self {
            Self::PowerPlant { .. } => has("FS_POWER"),
            Self::Superweapon => has("FS_SUPERWEAPON") || has("FS_STRATEGY_CENTER"),
            Self::CommandCenter => has("COMMANDCENTER"),
            Self::SupplyCenter { .. } => has("FS_SUPPLY_CENTER"),
            Self::MilitaryFactory { .. } => {
                !has("AIRCRAFT_CARRIER")
                    && (has("FS_BARRACKS") || has("FS_WARFACTORY") || has("FS_AIRFIELD"))
            }
            Self::FakeBuilding => has("FS_FAKE"),
            Self::InternetCenter { .. } => has("FS_INTERNET_CENTER"),
        }
    }
}

impl Skirmish {
    #[must_use]
    pub fn can_sabotage(&self, id: usize, target: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            actor.complete()
                && !actor.masked()
                && !actor.hacked()
                && !actor.paralyzed()
                && actor.contained_by().is_none()
                && self.maintenance_target_visible(actor.owner, target)
                && self.sabotage_accepts(id, target)
        })
    }

    /// Orders an explicit enter goal; incidental contact cannot sabotage.
    /// # Errors
    /// Rejects hidden, allied, unfinished and unsupported sabotage targets.
    pub fn sabotage(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_sabotage(id, target) {
            return Err("This unit cannot sabotage that building");
        }
        self.begin_container_enter(id, target);
        Ok(())
    }

    pub(super) fn sabotage_accepts(&self, id: usize, target: usize) -> bool {
        self.sabotage_module(id, target).is_some()
    }

    fn sabotage_module(&self, id: usize, target: usize) -> Option<usize> {
        let actor = self.entities.get(id)?;
        let victim = self.entities.get(target)?;
        let unit = &self.catalog[victim.type_index];
        if self.update_objects[actor.update_owner].destroy_requested
            || !actor.alive()
            || actor.masked()
            || !victim.complete()
            || victim.masked()
            || victim.selling()
            || victim.owner == self.neutral_owner()
            || !self.enemies(actor.owner, victim.owner)
            || !unit.kind.is_building()
        {
            return None;
        }
        self.catalog[actor.type_index]
            .behavior
            .sabotage
            .iter()
            .position(|rules| {
                rules.effect.accepts(&unit.kind_of)
                    && rules
                        .required
                        .iter()
                        .all(|kind| unit.kind_of.contains(kind))
                    && !rules
                        .forbidden
                        .iter()
                        .any(|kind| unit.kind_of.contains(kind))
            })
    }

    pub(super) fn sabotage_module_contact(&mut self, id: usize, target: usize, ordinal: usize) {
        if self.sabotage_module(id, target) == Some(ordinal) {
            self.sabotage_contact(id, target);
        }
    }

    pub(super) fn sabotage_contact(&mut self, id: usize, target: usize) -> bool {
        if !matches!(self.entities[id].ai.order, Order::Enter(goal) if goal == target) {
            return false;
        }
        let Some(module) = self.sabotage_module(id, target) else {
            return false;
        };
        let actor = &self.entities[id];
        let owner = actor.owner;
        let victim = self.entities[target].owner;
        let mut cash = 0;
        match self.catalog[actor.type_index].behavior.sabotage[module].effect {
            Effect::PowerPlant { frames } => {
                self.players[victim].energy.sabotaged_until = self.tick + u64::from(frames);
                self.players[victim].energy.brownout = true;
                // SabotagePowerPlantCrateCollide sends literal TRUE even for a zero/repeated outage.
                self.sabotage_power(victim);
            }
            Effect::Superweapon | Effect::CommandCenter => {
                if let Some(progression) = &self.progression {
                    let powers = &self.catalog[self.entities[target].type_index]
                        .progression
                        .powers;
                    for (index, power) in progression.powers.iter().enumerate() {
                        if powers.contains(&power.name) {
                            let timer = &mut self.players[victim].power_ready_at[index];
                            *timer = Some(if timer.is_some() {
                                self.tick + u64::from(power.reload_frames)
                            } else {
                                // Player::resetOrStartSpecialPowerReadyFrame
                                // adds an absent shared timer ready immediately.
                                self.tick
                            });
                        }
                    }
                }
            }
            Effect::SupplyCenter { amount } => {
                cash = amount.min(self.players[victim].cash);
                self.players[victim].cash -= cash;
                self.players[owner].cash = self.players[owner].cash.wrapping_add(cash);
                self.players[owner].statistics.money_earned += cash;
            }
            Effect::MilitaryFactory { frames } => {
                self.set_hacked_until(target, self.update_frame().wrapping_add(frames));
            }
            Effect::FakeBuilding => {
                self.apply_typed_hit(
                    target,
                    id,
                    self.entities[target].max_health,
                    DeathType::Detonated,
                );
            }
            Effect::InternetCenter { frames } => {
                let deadline = self.tick + u64::from(frames);
                self.sabotage_spy_vision(victim, deadline);
                self.set_hacked_until(target, self.update_frame().wrapping_add(frames));
                for passenger in self.passengers(target).to_vec() {
                    self.set_hacked_until(passenger, self.update_frame().wrapping_add(frames));
                }
            }
        }
        self.presentation.sabotage_events.push(Event {
            frame: self.tick,
            source: id,
            target,
            module,
            cash,
        });
        // CrateCollide destroys its owner without OnDie, kill credit or salvage.
        self.destroy_maintenance_object(id);
        true
    }
}
