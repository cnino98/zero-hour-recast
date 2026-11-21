//! Native `CreateObjectDie`: ordinary ordered OCL nuggets at the authored DIE slot.
use super::{
    DamageType, DeathType, Skirmish, UnitType, death_callbacks::Conditions, object_creation,
    physical_ocl,
};

pub struct Rules {
    pub conditions: Conditions,
    pub actions: Vec<Action>,
    pub transfer_previous_health: bool,
}

/// OCL dispatch follows its actual declaration sequence across nugget classes.
#[derive(Clone)]
pub enum Action {
    Objects(object_creation::List),
    Physical(physical_ocl::List),
}

pub(in crate::skirmish) fn validate(
    unit: &UnitType,
    catalog: &[UnitType],
) -> Result<(), &'static str> {
    for action in unit
        .behavior
        .create_object_die
        .iter()
        .flat_map(|rules| &rules.actions)
    {
        if let Action::Objects(list) = action {
            for object in &list.objects {
                for name in object
                    .names
                    .iter()
                    .chain(std::iter::once(&object.container))
                    .filter(|name| !name.is_empty())
                {
                    if !catalog.iter().any(|target| &target.name == name) {
                        return Err(
                            "CreateObjectDie requires the original admitted OCL object/container",
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn create_object_die_on_die(
        &mut self,
        id: usize,
        ordinal: usize,
        _source: Option<usize>,
    ) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .create_object_die[ordinal];
        if !self.die_conditions_match(id, &rules.conditions) {
            return;
        }
        let transfer = rules.transfer_previous_health;
        let actions = rules.actions.clone();
        let mut first = None;
        for action in actions {
            let created = match action {
                Action::Objects(list) => self.create_generic_objects_from_source(id, &list),
                Action::Physical(list) => self
                    .create_physical_ocl(id, &list)
                    .expect("admitted native OCL debris")
                    .first()
                    .copied(),
            };
            first = first.or(created);
        }
        if transfer && let Some(replacement) = first {
            self.transfer_death_creation_health(id, replacement);
        }
    }

    fn transfer_death_creation_health(&mut self, old: usize, new: usize) {
        let actor = &self.entities[old];
        let subdual = actor.subdual_damage();
        let damage = actor.max_health - actor.damage_state.previous_health;
        let attacker = actor.damage_state.retained_source;
        if subdual > 0.0 {
            let unit = &self.catalog[self.entities[new].type_index];
            let amount =
                self.entities[new].armor_damage(unit, DamageType::SubdualUnresistable, subdual);
            self.apply_subdual_damage(new, amount);
        }
        if damage > 0.0 {
            self.attempt_health_damage(
                new,
                attacker,
                damage,
                DamageType::Unresistable,
                DeathType::Normal,
            );
        }
        for id in (0..self.entities.len()).rev() {
            if self.entities[id].exists() && self.entities[id].ai.registration.is_some() {
                self.transfer_native_attack(id, old, new);
            }
        }
    }
}

#[cfg(test)]
mod tests;
