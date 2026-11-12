//! SH `DieMux`, `FXListDie` and `DestroyDie`, invoked in authored behavior order.
use super::Skirmish;

#[cfg(test)]
mod tests;

pub struct Conditions {
    pub death_mask: u32,
    pub veterancy_mask: u8,
    pub exempt_status: u64,
    pub required_status: u64,
}

pub struct UpgradeRules {
    pub conditions: Conditions,
    pub upgrade: Option<String>,
}

pub struct CrushRules {
    pub conditions: Conditions,
    /// Native `TOTAL_CRUSH`, `BACK_END_CRUSH`, `FRONT_END_CRUSH` order.
    pub sounds: [String; 3],
    pub percentages: [i32; 3],
}

pub struct FxRules {
    pub conditions: Conditions,
    pub effect: String,
    pub orient_to_object: bool,
    pub starts_active: bool,
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
    pub removals: Vec<String>,
    pub upgrade_fx: String,
}

pub(in crate::skirmish) fn validate(unit: &super::UnitType) -> Result<(), String> {
    for (family, expected) in [
        ("FXListDie", unit.behavior.death_fx.len()),
        ("DestroyDie", unit.behavior.destroy_die.len()),
        ("CrushDie", unit.behavior.crush_die.len()),
        ("UpgradeDie", unit.behavior.upgrade_die.len()),
        (
            "FireWeaponWhenDeadBehavior",
            unit.behavior.death_weapons.len(),
        ),
        ("InstantDeathBehavior", unit.behavior.instant_death.len()),
        ("BunkerBusterBehavior", unit.behavior.bunker_busters.len()),
        ("NeutronBlastBehavior", unit.behavior.neutron_blast.len()),
        ("EjectPilotDie", unit.behavior.eject_pilots.len()),
        ("CreateObjectDie", unit.behavior.create_object_die.len()),
        ("RebuildHoleExposeDie", unit.maintenance.expose_hole.len()),
    ] {
        let actual = unit
            .behavior_modules
            .iter()
            .filter(|module| module.family == family)
            .count();
        if actual != expected {
            return Err(format!(
                "{}: ordered {family} data count differs from authored modules",
                unit.name
            ));
        }
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_death_fx(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.death_fx.len(), ordinal);
        self.entities[id].behaviors.death_fx.push(false);
        if self.catalog[self.entities[id].type_index].behavior.death_fx[ordinal].starts_active {
            self.activate_death_fx(id, ordinal);
        }
    }

    fn activate_death_fx(&mut self, id: usize, ordinal: usize) {
        self.give_self_upgrade_instance(id, "FXListDie", ordinal);
    }

    pub(in crate::skirmish) fn apply_death_fx_upgrade(&mut self, id: usize, ordinal: usize) {
        self.entities[id].behaviors.death_fx[ordinal] = true;
    }

    pub(in crate::skirmish) fn death_fx_on_die(
        &mut self,
        id: usize,
        ordinal: usize,
        source: Option<usize>,
    ) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.death_fx[ordinal];
        if !actor.behaviors.death_fx[ordinal]
            || !self.die_conditions_match(id, &rules.conditions)
            || rules.conflicts.iter().any(|name| {
                self.object_has_upgrade(id, name) || self.has_upgrade(actor.owner, name)
            })
            || rules.effect.is_empty()
        {
            return;
        }
        let effect = rules.effect.clone();
        let orient = rules.orient_to_object;
        let position = self.object_position(id);
        let basis = if orient {
            actor
                .projectile_transform()
                .map(|(_, axes)| {
                    std::array::from_fn(|row| std::array::from_fn(|column| axes[column][row]))
                })
                .or_else(|| actor.physical_basis())
                .unwrap_or_else(|| super::physical_ocl::transform::yaw(actor.facing))
        } else {
            super::physical_ocl::transform::yaw(0.0)
        };
        let mut event = self.snapshot_object_fx(id, effect, position, basis);
        event.oriented_object = orient;
        event.secondary_position = if orient {
            source
                .filter(|&other| self.entities.get(other).is_some_and(super::Entity::exists))
                .map(|other| self.object_position(other))
        } else {
            None
        };
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::ObjectFx(
                self.presentation.object_fx_events.len(),
            ));
        self.presentation.object_fx_events.push(event);
    }

    // UpgradeDie.cpp68: find the original producer, then remove its completed bit.
    pub(in crate::skirmish) fn upgrade_on_die(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.upgrade_die[ordinal];
        if !self.die_conditions_match(id, &rules.conditions) {
            return;
        }
        let Some(producer) = actor
            .producer
            .filter(|&producer| self.entities[producer].exists())
        else {
            return;
        };
        let Some(name) = &rules.upgrade else {
            return;
        };
        if self.object_has_upgrade(producer, name) {
            let name = name.clone();
            self.remove_native_object_upgrade(producer, &name);
        }
    }

    pub(in crate::skirmish) fn destroy_on_die(&mut self, id: usize, ordinal: usize) {
        let conditions = &self.catalog[self.entities[id].type_index]
            .behavior
            .destroy_die[ordinal];
        if self.die_conditions_match(id, conditions) {
            self.destroy_maintenance_object(id);
        }
    }

    pub(in crate::skirmish) fn crush_on_die(
        &mut self,
        id: usize,
        ordinal: usize,
        source: Option<usize>,
    ) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.crush_die[ordinal];
        if !self.die_conditions_match(id, &rules.conditions)
            || !matches!(actor.damage_state.damage_type, super::DamageType::Crush)
        {
            return;
        }
        let mask = source
            .filter(|&source| self.entities.get(source).is_some_and(super::Entity::exists))
            .map_or(Some(3), |source| self.crush_death_location(source, id));
        let Some(mask) = mask else { return };
        let index = usize::from(3 - mask);
        if !rules.sounds[index].is_empty() {
            let sound = rules.sounds[index].clone();
            let percent = rules.percentages[index];
            let draw = i32::try_from(super::startup::random_between(&mut self.random, 99))
                .expect("native sound probability draw");
            if draw < percent {
                self.object_sound(id, sound, super::object_sounds::Action::Play);
            }
        }
        let actor = &mut self.entities[id];
        actor.damage_state.front_crushed = mask & 1 != 0;
        actor.damage_state.back_crushed = mask & 2 != 0;
    }

    pub(in crate::skirmish) fn die_conditions_match(
        &self,
        id: usize,
        conditions: &Conditions,
    ) -> bool {
        self.death_conditions_match(
            id,
            conditions.death_mask,
            conditions.veterancy_mask,
            conditions.exempt_status,
            conditions.required_status,
        )
    }
}
