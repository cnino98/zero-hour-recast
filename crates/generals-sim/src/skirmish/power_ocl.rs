//! SH 2bfaea60 `OCLSpecialPower` and its ordered creation nuggets.
#[cfg(test)]
mod tests;

use super::{Skirmish, WeaponBinding, combat::attack::Goal, object_creation};

pub struct Rules {
    pub power: String,
    pub location: Location,
    pub adjust_position: bool,
    pub reference: String,
    pub default: List,
    pub upgrades: Vec<(String, List)>,
}

pub enum Location {
    NearSource,
    NearTarget,
    FarTarget,
    AtTarget,
    AboveTarget,
    Owner,
}

// Creation callbacks can append objects while executing this immutable list.
#[derive(Clone)]
pub struct List {
    pub name: String,
    pub nuggets: Vec<Nugget>,
}

#[derive(Clone)]
pub enum Nugget {
    Objects(object_creation::List),
    Delivery(String),
    FireWeapon(usize),
    Attack {
        shots: i32,
        slot: usize,
        decal: super::radius_decal::Template,
        radius: f32,
    },
}

impl Skirmish {
    pub(in crate::skirmish) fn execute_power_ocl(
        &mut self,
        id: usize,
        ordinal: usize,
        mut target: [f32; 3],
        angle: Option<f32>,
    ) {
        let kind = self.entities[id].type_index;
        if self.catalog[kind].behavior.ocl_powers[ordinal].adjust_position {
            target = self.adjust_ocl_position_to_passable(target, 500.0);
        }
        let power = self.catalog[kind].behavior.ocl_powers[ordinal]
            .power
            .clone();
        // The building receives the old ready frame before the base module
        // restarts its timer; its weapon must fire from the open launch state.
        self.initiate_nuclear_launcher(id, &power);
        if let Some(power_ordinal) = self.special_power_ordinal(id, &power) {
            self.record_special_power_used(id, power_ordinal);
            if !self.catalog[kind].special_powers[power_ordinal].update_starts_attack {
                self.mark_special_power_triggered(id, &power, Some(target));
            }
        }
        let rules = &self.catalog[kind].behavior.ocl_powers[ordinal];
        let owner = self.entities[id].owner;
        let list = rules
            .upgrades
            .iter()
            .find(|(science, _)| self.players[owner].has_science(science))
            .map_or(&rules.default, |(_, list)| list)
            .clone();
        let mut start = match rules.location {
            Location::NearSource => self.delivery_closest_edge(self.object_position(id)),
            Location::NearTarget => self.delivery_closest_edge(target),
            Location::FarTarget => self.power_farthest_edge(target),
            Location::AtTarget | Location::AboveTarget | Location::Owner => target,
        };
        if matches!(rules.location, Location::AboveTarget | Location::FarTarget) {
            start[2] += 300.0;
        }
        // USE_OWNER_OBJECT also selects the Real-angle overload: its final
        // false argument is lifetimeFrames=0, not the Bool createOwner overload.
        self.execute_power_list(id, &list, start, target, angle, true);
    }

    pub(in crate::skirmish) fn execute_power_ocl_without_target(
        &mut self,
        id: usize,
        ordinal: usize,
    ) {
        let kind = self.entities[id].type_index;
        let power = self.catalog[kind].behavior.ocl_powers[ordinal]
            .power
            .clone();
        let position = self.object_position(id);
        self.initiate_nuclear_launcher(id, &power);
        if let Some(power_ordinal) = self.special_power_ordinal(id, &power) {
            self.record_special_power_used(id, power_ordinal);
            if !self.catalog[kind].special_powers[power_ordinal].update_starts_attack {
                self.mark_special_power_triggered(id, &power, Some(position));
            }
        }
        let rules = &self.catalog[kind].behavior.ocl_powers[ordinal];
        let owner = self.entities[id].owner;
        let list = rules
            .upgrades
            .iter()
            .find(|(science, _)| self.players[owner].has_science(science))
            .map_or(&rules.default, |(_, list)| list)
            .clone();
        self.execute_power_list(id, &list, position, position, None, false);
    }

    fn execute_power_list(
        &mut self,
        id: usize,
        list: &List,
        start: [f32; 3],
        target: [f32; 3],
        angle: Option<f32>,
        create_owner: bool,
    ) {
        for nugget in &list.nuggets {
            match nugget {
                Nugget::Objects(list) => {
                    self.create_generic_objects_at(id, list, start, angle, 0);
                }
                Nugget::Delivery(name) => {
                    let index = self
                        .delivery_lists
                        .iter()
                        .position(|list| &list.name == name)
                        .expect("validated special power delivery list");
                    self.create_delivery_at(id, index, start, target, create_owner);
                }
                Nugget::FireWeapon(ordinal) => {
                    let kind = self.entities[id].type_index;
                    let binding = WeaponBinding {
                        type_index: kind,
                        weapon_index: self.catalog[kind].power_weapon_start() + ordinal,
                        weapon_slot: 0,
                    };
                    self.fire_temporary_weapon_binding_at(id, binding, target);
                }
                Nugget::Attack {
                    shots,
                    slot,
                    decal,
                    radius,
                } => {
                    if self.entities[id].ai.registration.is_some() {
                        if self.weapon_slot_present(id, *slot)
                            && !matches!(
                                self.entities[id].weapon_set.lock,
                                super::weapons::Lock::Permanent
                            )
                        {
                            self.entities[id].weapon_set.current = *slot;
                            self.entities[id].weapon_set.lock = super::weapons::Lock::Temporary;
                        }
                        self.ai_attack_goal(
                            id,
                            Goal::Position(target),
                            u32::from_ne_bytes(shots.to_ne_bytes()),
                        );
                    }
                    self.create_attack_radius_decal(id, decal.clone(), *radius, target);
                }
            }
        }
    }

    fn power_farthest_edge(&self, target: [f32; 3]) -> [f32; 3] {
        // TerrainLogic compares against half the width without adding the origin.
        let mut point = target;
        for axis in 0..2 {
            point[axis] = if target[axis]
                < (self.playable_extent[1][axis] - self.playable_extent[0][axis]) / 2.0
            {
                self.playable_extent[1][axis]
            } else {
                self.playable_extent[0][axis]
            };
        }
        point[2] = self.ground_height([point[0], point[1]]);
        point
    }
}
