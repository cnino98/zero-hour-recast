//! SH `InstantDeathBehavior::onDie`, CRC=0: every applicable instance runs its effects.
use super::{Skirmish, WeaponRules, physical_ocl};

pub struct Rules {
    pub death_mask: u32,
    pub veterancy_mask: u8,
    pub exempt_status: u64,
    pub required_status: u64,
    pub effects: Vec<Option<String>>,
    pub objects: Vec<Option<physical_ocl::List>>,
    pub weapons: Vec<Option<WeaponRules>>,
}

impl Skirmish {
    pub(in crate::skirmish) fn instant_die(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.instant_death[ordinal];
        if !self.death_conditions_match(
            id,
            rules.death_mask,
            rules.veterancy_mask,
            rules.exempt_status,
            rules.required_status,
        ) {
            return;
        }
        let type_index = actor.type_index;
        let count = rules.effects.len();
        if let Some(choice) = self.instant_choice(count)
            && let Some(effect) =
                self.catalog[type_index].behavior.instant_death[ordinal].effects[choice].clone()
        {
            self.object_fx(id, effect);
        }
        let count = self.catalog[type_index].behavior.instant_death[ordinal]
            .objects
            .len();
        if let Some(choice) = self.instant_choice(count)
            && let Some(list) =
                self.catalog[type_index].behavior.instant_death[ordinal].objects[choice].clone()
        {
            self.create_physical_ocl(id, &list)
                .expect("admitted instant-death OCL");
        }
        // The fixed source omits both firing and its random choice during construction.
        if !self.entities[id].under_construction() {
            let count = self.catalog[type_index].behavior.instant_death[ordinal]
                .weapons
                .len();
            if let Some(choice) = self.instant_choice(count)
                && self.catalog[type_index].behavior.instant_death[ordinal].weapons[choice]
                    .is_some()
            {
                let offset: usize = self.catalog[type_index].behavior.instant_death[..ordinal]
                    .iter()
                    .map(|rules| rules.weapons.len())
                    .sum();
                let index = self.catalog[type_index].instant_weapon_start() + offset + choice;
                self.fire_temporary_weapon(id, index);
            }
        }
        // CRC=0 moved this guard after effects so repeated InstantDeath modules run.
        if !matches!(
            self.catalog[type_index].ai.update,
            super::UnitAiUpdate::Absent
        ) {
            if self.entities[id].is_ai_dead() {
                return;
            }
            self.mark_ai_dead(id);
        }
        self.destroy_maintenance_object(id);
    }

    fn instant_choice(&mut self, count: usize) -> Option<usize> {
        let maximum = count.checked_sub(1)?;
        Some(
            usize::try_from(self.random_between(
                u32::try_from(maximum).expect("native instant-death choice count"),
            ))
            .expect("native choice index"),
        )
    }

    pub(in crate::skirmish) fn death_conditions_match(
        &self,
        id: usize,
        death_mask: u32,
        veterancy_mask: u8,
        exempt_status: u64,
        required_status: u64,
    ) -> bool {
        self.death_conditions_match_type(
            id,
            self.entities[id].damage_state.death_type,
            death_mask,
            veterancy_mask,
            exempt_status,
            required_status,
        )
    }

    pub(in crate::skirmish) fn death_conditions_match_type(
        &self,
        id: usize,
        death: super::DeathType,
        death_mask: u32,
        veterancy_mask: u8,
        exempt_status: u64,
        required_status: u64,
    ) -> bool {
        let actor = &self.entities[id];
        let status = self.object_status_bits(id);
        death_mask & (1 << death as u32) != 0
            && veterancy_mask & (1 << actor.veterancy) != 0
            && exempt_status & status == 0
            && required_status & status == required_status
    }

    pub(in crate::skirmish) fn object_status_bits(&self, id: usize) -> u64 {
        let actor = &self.entities[id];
        actor.object_status
            | (u64::from(self.update_objects[actor.update_owner].destroy_requested) << 1)
            | (u64::from(actor.under_construction()) << 3)
            | (u64::from(self.body_collision_status(id)) << 5)
            | (u64::from(actor.hijacked()) << 10)
            | (u64::from(actor.aflame()) << 11)
            | (u64::from(actor.flammable.burned) << 12)
            | (u64::from(actor.stealthed()) << 16)
            | (u64::from(actor.detected()) << 17)
            | (u64::from(actor.selling()) << 19)
            | (u64::from(actor.reconstructing()) << 21)
            | (u64::from(actor.masked()) << 22)
            | (u64::from(actor.has_native_disguise()) << 43)
    }
}

#[cfg(test)]
mod tests;
