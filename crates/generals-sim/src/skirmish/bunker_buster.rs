//! SH `BunkerBusterBehavior`, CRC=0 and `DO_SEISMIC_SIMULATIONS` unset.
use super::{DamageType, DeathType, Skirmish, WeaponBinding, WeaponRules};

pub struct Rules {
    pub upgrade: Option<String>,
    pub detonation_fx: String,
    pub crash_fx: String,
    pub crash_frames: u32,
    pub occupant_damage: Option<(DamageType, DeathType)>,
    pub shockwave: Option<WeaponRules>,
}

pub(in crate::skirmish) struct State {
    victim: Option<usize>,
    upgrade: Option<String>,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_bunker_buster(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.bunker_busters.len(), ordinal);
        self.entities[id].behaviors.bunker_busters.push(State {
            victim: None,
            upgrade: None,
        });
    }

    pub(in crate::skirmish) fn bunker_buster_on_created(&mut self, id: usize, ordinal: usize) {
        self.entities[id].behaviors.bunker_busters[ordinal].upgrade = self.catalog
            [self.entities[id].type_index]
            .behavior
            .bunker_busters[ordinal]
            .upgrade
            .clone();
    }

    pub(in crate::skirmish) fn update_bunker_buster(&mut self, id: usize, ordinal: usize) -> i32 {
        // Native getCurrentVictim performs findObjectByID, with no body-health test.
        if self.entities[id].ai.registration.is_some() {
            if self.entities[id].behaviors.bunker_busters[ordinal]
                .victim
                .is_none()
            {
                self.entities[id].behaviors.bunker_busters[ordinal].victim = self.entities[id]
                    .ai
                    .current_victim
                    .filter(|&victim| self.entities[victim].exists());
            }
            let rules = &self.catalog[self.entities[id].type_index]
                .behavior
                .bunker_busters[ordinal];
            if self.update_frame() % rules.crash_frames == 1
                && self.entities[id]
                    .projectile_flight
                    .as_ref()
                    .is_some_and(super::projectiles::Flight::killing_self)
            {
                let effect = rules.crash_fx.clone();
                self.object_fx(id, effect);
            }
        }
        1
    }

    pub(in crate::skirmish) fn bunker_buster_on_die(&mut self, id: usize, ordinal: usize) {
        if !self.entities[id]
            .projectile_flight
            .as_ref()
            .is_some_and(super::projectiles::Flight::killing_self)
        {
            return;
        }
        let actor = &self.entities[id];
        let state = &actor.behaviors.bunker_busters[ordinal];
        if state
            .upgrade
            .as_ref()
            .is_some_and(|name| !self.has_upgrade(actor.owner, name))
        {
            return;
        }
        let type_index = actor.type_index;
        let rules = &self.catalog[type_index].behavior.bunker_busters[ordinal];
        let target = state
            .victim
            .filter(|&victim| self.entities[victim].exists());
        let occupant = rules.occupant_damage;
        let effect = rules.detonation_fx.clone();
        let shockwave = rules.shockwave.is_some();
        let weapon_index = self.catalog[type_index].bunker_weapon_start() + ordinal;
        if let Some(target) = target
            && self.bustable_container(target)
        {
            self.bust_occupants(target, id, occupant);
        }
        let source = target.unwrap_or(id);
        self.object_fx(source, effect);
        if shockwave {
            self.fire_temporary_weapon_binding_at(
                source,
                WeaponBinding {
                    type_index,
                    weapon_index,
                    weapon_slot: 0,
                },
                self.object_position(source),
            );
        }
    }

    fn bustable_container(&self, target: usize) -> bool {
        self.catalog[self.entities[target].type_index]
            .contain
            .as_ref()
            .is_some_and(|rules| rules.tunnel.is_some() || rules.garrison.is_some())
    }

    fn bust_occupants(
        &mut self,
        target: usize,
        source: usize,
        occupant: Option<(DamageType, DeathType)>,
    ) {
        if occupant.is_none() {
            self.kill_all_contained(target);
            return;
        }
        // harmAndForceExit and OpenContain kill reread the mutating list.
        while let Some(&id) = self.passengers(target).first() {
            self.remove_passenger(id, target);
            if let Some((damage_type, death)) = occupant {
                self.apply_weapon_hit(id, source, 100.0, damage_type, death);
            } else {
                self.kill_object(id, DeathType::Normal);
            }
        }
    }
}

#[cfg(test)]
pub(in crate::skirmish) mod tests;
