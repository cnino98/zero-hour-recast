//! Native `CreateObjectDie`: ordinary ordered OCL nuggets at the authored DIE slot.
use super::{Skirmish, UnitType, death_callbacks::Conditions, object_creation, physical_ocl};

pub struct Rules {
    pub conditions: Conditions,
    pub actions: Vec<Action>,
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
        source: Option<usize>,
    ) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .create_object_die[ordinal];
        if !self.die_conditions_match(id, &rules.conditions) {
            return;
        }
        let actions = rules.actions.clone();
        for action in actions {
            match action {
                Action::Objects(list) => self.create_generic_objects(id, source, &list),
                Action::Physical(list) => {
                    self.create_physical_ocl(id, &list)
                        .expect("admitted native OCL debris");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
