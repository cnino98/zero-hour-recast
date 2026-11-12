//! `HiveStructureBody::attemptDamage` containment branch, SH 2bfaea60.
use super::{DamageType, Skirmish, movement::distance_squared};

pub struct Rules {
    pub propagate: u64,
    pub swallow: u64,
}

impl Skirmish {
    pub(super) fn hive_damage_target(
        &self,
        target: usize,
        source: usize,
        damage: DamageType,
    ) -> Option<usize> {
        let Some(shooter) = self.entities.get(source) else {
            return Some(target);
        };
        self.hive_damage_target_from(target, shooter.position, damage)
    }

    pub(super) fn hive_damage_target_from(
        &self,
        target: usize,
        source: [f32; 2],
        damage: DamageType,
    ) -> Option<usize> {
        let unit = &self.catalog[self.entities[target].type_index];
        let Some(rules) = &unit.hive_body else {
            return Some(target);
        };
        if rules.propagate & (1 << damage as usize) == 0 {
            return Some(target);
        }
        let mut closest = None;
        let mut distance = f32::MAX;
        let children = self.spawned_children(target);
        let victims = if self.catalog[self.entities[target].type_index]
            .behavior
            .spawns
            .is_empty()
        {
            self.passengers(target)
        } else {
            children
        };
        for &id in victims {
            let candidate = distance_squared(self.entities[id].position, source);
            if closest.is_none() || candidate < distance {
                closest = Some(id);
                distance = candidate;
            }
        }
        closest.or_else(|| (rules.swallow & (1 << damage as usize) == 0).then_some(target))
    }
}

#[cfg(test)]
mod tests;
