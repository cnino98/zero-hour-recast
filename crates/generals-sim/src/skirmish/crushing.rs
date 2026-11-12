//! `SquishCollide::onCollide`, `SuperHackers` 2bfaea60; ordinary ground contacts.
use super::{DamageType, DeathType, Order, Skirmish};
use crate::collision_geometry::{Body, Geometry, Shape, intersects};

pub(in crate::skirmish) fn validate(unit: &super::UnitType) -> Result<(), String> {
    for module in &unit.behavior_modules {
        if module.interfaces & 0x10 != 0
            && !matches!(
                module.family.as_str(),
                "FireWeaponCollide"
                    | "ToppleUpdate"
                    | "PhysicsBehavior"
                    | "ParachuteContain"
                    | "OpenContain"
                    | "HealContain"
                    | "TransportContain"
                    | "GarrisonContain"
                    | "TunnelContain"
                    | "CaveContain"
                    | "HelixContain"
                    | "OverlordContain"
                    | "InternetHackContain"
                    | "SquishCollide"
                    | "ConvertToCarBombCrateCollide"
                    | "ConvertToHijackedVehicleCrateCollide"
                    | "MoneyCrateCollide"
                    | "VeterancyCrateCollide"
                    | "SalvageCrateCollide"
                    | "MinefieldBehavior"
                    | "SabotagePowerPlantCrateCollide"
                    | "SabotageSuperweaponCrateCollide"
                    | "SabotageCommandCenterCrateCollide"
                    | "SabotageSupplyCenterCrateCollide"
                    | "SabotageMilitaryFactoryCrateCollide"
                    | "SabotageFakeBuildingCrateCollide"
                    | "SabotageInternetCenterCrateCollide"
            )
        {
            return Err(format!(
                "{}: unbound COLLIDE {} {} at {}:{} ({})",
                unit.name,
                module.family,
                module.tag,
                module.source.path,
                module.source.line,
                module.source.origin
            ));
        }
    }
    let veterancy = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "VeterancyCrateCollide")
        .count();
    if veterancy != unit.behavior.veterancy_crates.len() {
        return Err(format!(
            "{}: VeterancyCrateCollide data count differs from registrations",
            unit.name
        ));
    }
    let collisions = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "FireWeaponCollide")
        .count();
    if collisions != unit.behavior.collision_weapons.len() {
        return Err(format!(
            "{}: FireWeaponCollide data count differs from registrations",
            unit.name
        ));
    }
    Ok(())
}

impl Skirmish {
    pub(super) fn object_contact(&mut self, id: usize, other: usize, _ignored: bool) {
        self.object_collide(id, Some(other));
    }

    /// Object.cpp:2438 visits COLLIDE interfaces in authored behavior order.
    pub(in crate::skirmish) fn object_collide(&mut self, id: usize, other: Option<usize>) {
        let kind = self.entities[id].type_index;
        let mut sabotage = 0;
        let mut mine = 0;
        let mut weapon = 0;
        let mut veterancy = 0;
        for index in 0..self.catalog[kind].behavior_modules.len() {
            let module = &self.catalog[kind].behavior_modules[index];
            if module.interfaces & 0x10 == 0 {
                continue;
            }
            // A preceding handler may set this, even while destruction remains deferred.
            if self.body_collision_status(id) {
                break;
            }
            match module.family.as_str() {
                "ToppleUpdate" => {
                    if let Some(other) = other {
                        self.topple_contact(id, other);
                    }
                }
                "FireWeaponCollide" => {
                    if let Some(other) = other {
                        self.fire_weapon_contact(id, weapon, other);
                    }
                    weapon += 1;
                }
                "PhysicsBehavior" => self.physics_contact(id, other),
                "ParachuteContain" => {
                    if other.is_none() {
                        self.parachute_ground_contact(id);
                    }
                }
                "OpenContain"
                | "HealContain"
                | "TransportContain"
                | "GarrisonContain"
                | "TunnelContain"
                | "CaveContain"
                | "HelixContain"
                | "OverlordContain"
                | "InternetHackContain" => {
                    if let Some(other) = other {
                        self.container_contact(other, id);
                    }
                }
                "SquishCollide" => {
                    if let Some(other) = other {
                        self.squish_contact(id, other);
                    }
                }
                "ConvertToCarBombCrateCollide" => {
                    if let Some(other) = other {
                        self.car_bomb_contact(id, other);
                    }
                }
                "ConvertToHijackedVehicleCrateCollide" => {
                    if let Some(other) = other {
                        self.hijacker_contact(id, other);
                    }
                }
                "VeterancyCrateCollide" => {
                    if let Some(other) = other {
                        self.veterancy_crate_contact(id, veterancy, other);
                    }
                    veterancy += 1;
                }
                "MoneyCrateCollide" => {
                    if let Some(other) = other
                        && self.entities[id].salvage.money_active
                    {
                        self.money_crate_contact(id, other);
                    }
                }
                "SalvageCrateCollide" => {
                    if let Some(other) = other
                        && self.entities[id].crate_present()
                    {
                        self.salvage_contact(id, other);
                    }
                }
                "MinefieldBehavior" => {
                    if let Some(other) = other {
                        self.land_mine_contact(id, mine, other);
                    }
                    mine += 1;
                }
                "SabotagePowerPlantCrateCollide"
                | "SabotageSuperweaponCrateCollide"
                | "SabotageCommandCenterCrateCollide"
                | "SabotageSupplyCenterCrateCollide"
                | "SabotageMilitaryFactoryCrateCollide"
                | "SabotageFakeBuildingCrateCollide"
                | "SabotageInternetCenterCrateCollide" => {
                    if let Some(other) = other {
                        self.sabotage_module_contact(id, other, sabotage);
                    }
                    sabotage += 1;
                }
                family => unreachable!("unbound authored COLLIDE interface: {family}"),
            }
        }
    }

    fn physics_contact(&mut self, id: usize, other: Option<usize>) {
        // PhysicsUpdate.cpp:1159 caches Object's first ProjectileUpdateInterface.
        if self.projectile_contact(id, other) {
            return;
        }
        if let Some(other) = other {
            self.physical_contact(id, other);
        } else if let Some(carrier) = self.entities[id].contained_by() {
            self.object_collide(carrier, None);
        }
    }

    fn squish_contact(&mut self, id: usize, other: usize) {
        let actor = &self.entities[id];
        let victim = &self.catalog[actor.type_index];
        let crusher = &self.entities[other];
        let unit = &self.catalog[crusher.type_index];
        if victim.crush.squish_module.is_none()
            || !actor.alive()
            || unit.crush.crusher_level == 0
            || unit.crush.physics_module.is_none()
            || self.allied(crusher.owner, actor.owner)
        {
            return;
        }
        let goal = actor.ai.command.ability_goal.or(match actor.ai.order {
            Order::Attack(target) | Order::Enter(target) => Some(target),
            Order::AttackMove(_) => actor.ai.ai_target,
            _ => None,
        });
        if victim.crush.hijacker && goal == Some(other) {
            return;
        }
        let position = self.object_position(id);
        let crusher_position = self.object_position(other);
        let victim_geometry = self.body_geometry(id);
        let crusher_geometry = self.body_geometry(other);
        let geometry = Geometry {
            shape: match victim_geometry.shape {
                Shape::Sphere => Shape::Sphere,
                Shape::Cylinder => Shape::Cylinder,
                Shape::Box => Shape::Box,
            },
            is_small: victim_geometry.is_small,
            major: 1.0,
            minor: 1.0,
            height: victim_geometry.height,
        };
        // geomCollidesWithGeom has a stricter preliminary Z gate than the
        // partition's normal collidesWith: sphere undersides are not included.
        let crusher_height = if matches!(crusher_geometry.shape, Shape::Sphere) {
            crusher_geometry.major
        } else {
            crusher_geometry.height
        };
        let victim_height = if matches!(geometry.shape, Shape::Sphere) {
            geometry.major
        } else {
            geometry.height
        };
        if crusher_position[2] + crusher_height < position[2]
            || crusher_position[2] > position[2] + victim_height
        {
            return;
        }
        if !intersects(
            &Body {
                geometry: crusher_geometry,
                position: crusher_position,
                angle: crusher.facing,
            },
            &Body {
                geometry: &geometry,
                position,
                angle: actor.facing,
            },
        ) {
            return;
        }
        let x = (position[0] - crusher_position[0]) * crusher.ai.motion.velocity[0];
        let y = (position[1] - crusher_position[1]) * crusher.ai.motion.velocity[1];
        if x + y > 0.0 {
            // Damage.h HUGE_DAMAGE_AMOUNT passes through ActiveBody armor and
            // the ordinary damage/death callbacks; armor immunity still works.
            self.apply_weapon_hit(id, other, 999_999.0, DamageType::Crush, DeathType::Crushed);
        }
    }
}

#[cfg(test)]
mod tests;
