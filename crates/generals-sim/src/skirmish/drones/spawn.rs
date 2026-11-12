use super::{Rules, Skirmish};
use crate::skirmish::UnitType;

pub(in crate::skirmish) fn validate(
    unit: &UnitType,
    catalog: &[UnitType],
) -> Result<(), &'static str> {
    for rules in &unit.behavior.slaves {
        validate_rules(rules)?;
    }
    for upgrade in &unit.behavior.drone_upgrades {
        if upgrade.offset.iter().any(|value| !value.is_finite())
            || upgrade.trigger.is_empty()
            || catalog
                .iter()
                .find(|child| child.name == upgrade.object)
                .is_none_or(|child| child.behavior.slaves.is_empty())
        {
            return Err("Invalid original drone upgrade binding");
        }
    }
    Ok(())
}

fn validate_rules(rules: &Rules) -> Result<(), &'static str> {
    if [
        rules.guard_range,
        rules.guard_wander,
        rules.attack_range,
        rules.attack_wander,
        rules.scout_range,
        rules.scout_wander,
        rules.spotting_range,
        rules.panic_speed,
    ]
    .into_iter()
    .any(|value| !value.is_finite())
    {
        return Err("Invalid drone ranges or upgrade release");
    }
    if let Some(repair) = &rules.repair
        && ([
            repair.range,
            repair.altitude[0],
            repair.altitude[1],
            repair.per_second,
        ]
        .into_iter()
        .any(|value| !value.is_finite()))
    {
        return Err("Invalid drone repair rules");
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn drone_research_available(&self, id: usize, name: &str) -> bool {
        let actor = &self.entities[id];
        self.catalog[actor.type_index]
            .behavior
            .drone_upgrades
            .iter()
            .find(|upgrade| upgrade.trigger == name)
            .is_none_or(|upgrade| {
                actor.queue.is_empty()
                    && upgrade
                        .conflicts
                        .iter()
                        .all(|name| !self.object_has_upgrade(id, name))
            })
    }

    pub(in crate::skirmish) fn first_slaved_interface(
        &self,
        id: usize,
    ) -> Option<super::SlavedInterface> {
        for module in &self.catalog[self.entities[id].type_index].behavior_modules {
            match module.family.as_str() {
                "SlavedUpdate" => return Some(super::SlavedInterface::Ordinary(0)),
                "MobMemberSlavedUpdate" => return Some(super::SlavedInterface::Mob(0)),
                _ => {}
            }
        }
        None
    }

    pub(in crate::skirmish) fn slaved_parent(&self, id: usize) -> Option<usize> {
        match self.first_slaved_interface(id)? {
            super::SlavedInterface::Ordinary(ordinal) => {
                self.entities[id].behaviors.slaves[ordinal].parent
            }
            super::SlavedInterface::Mob(ordinal) => {
                self.entities[id].behaviors.mobs[ordinal].parent
            }
        }
    }

    pub(in crate::skirmish) fn slave_is_self_tasking(&self, id: usize) -> bool {
        match self.first_slaved_interface(id) {
            Some(super::SlavedInterface::Mob(ordinal)) => {
                self.entities[id].behaviors.mobs[ordinal].self_tasking
            }
            _ => false,
        }
    }

    /// Source creation/spawn producers call only the first Slaved interface.
    pub(in crate::skirmish) fn enslave_spawn(&mut self, child: usize, parent: usize) {
        match self.first_slaved_interface(child) {
            Some(super::SlavedInterface::Ordinary(ordinal)) => {
                self.start_slaved_effects(child, ordinal, parent);
            }
            Some(super::SlavedInterface::Mob(ordinal)) => {
                self.enslave_mob_member(child, ordinal, parent);
            }
            None => {}
        }
    }

    /// `SlowDeath` and Spawn call the first native Slaved virtual interface.
    pub(in crate::skirmish) fn slow_death_release_slave(&mut self, id: usize) {
        match self.first_slaved_interface(id) {
            Some(super::SlavedInterface::Ordinary(ordinal)) => {
                self.stop_slaved_effects(id, ordinal);
            }
            Some(super::SlavedInterface::Mob(ordinal)) => self.release_mob_member(id, ordinal),
            None => {}
        }
    }

    fn start_slaved_effects(&mut self, child: usize, ordinal: usize, parent: usize) {
        self.entities[child].behaviors.slaves[ordinal].parent = Some(parent);
        let range = self.catalog[self.entities[child].type_index]
            .behavior
            .slaves[ordinal]
            .guard_range;
        let offset = self.drone_random_offset(range);
        self.entities[child].behaviors.slaves[ordinal].offset = [offset[0], offset[1], 0.0];
        self.entities[child].maintenance.instance_flags |=
            super::super::maintenance::INSTANCE_UNSELECTABLE;
        if self.entities[parent].stealthed() {
            self.grant_stealth(child);
        }
    }

    pub(in crate::skirmish) fn stop_slaved_effects(&mut self, child: usize, ordinal: usize) {
        let state = &mut self.entities[child].behaviors.slaves[ordinal];
        state.parent = None;
        state.offset = [0.0; 3];
        self.entities[child].maintenance.instance_flags &=
            !super::super::maintenance::INSTANCE_UNSELECTABLE;
        self.clear_disabled_held(child);
    }
}
