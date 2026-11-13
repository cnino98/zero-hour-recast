use super::{Skirmish, projectiles};
use num_traits::ToPrimitive;

pub(super) enum Source {
    Object(usize),
    Projectile { launcher: usize, object: usize },
}

/// `WeaponTemplate` keeps its victim position separate from projectile scatter.
pub(super) struct WeaponTarget {
    pub object: Option<usize>,
    pub position: [f32; 3],
    pub projectile_position: [f32; 3],
    pub scatter_radius: f32,
}

/// `WeaponTemplate` primary/secondary rings; affects bits: allies, enemies, neutrals, self.
pub struct AreaDamage {
    pub at_source: bool,
    pub primary_radius: f32,
    pub secondary_radius: f32,
    pub secondary_damage: f32,
    pub affects: u8,
}

pub(super) fn validate(weapon: &super::WeaponRules) -> Result<(), &'static str> {
    if weapon.area.as_ref().is_some_and(|area| {
        [
            area.primary_radius,
            area.secondary_radius,
            area.secondary_damage,
        ]
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
            || (area.primary_radius.max(area.secondary_radius) <= 0.0 && area.affects & 32 == 0)
            || (area.secondary_radius != 0.0 && area.secondary_radius < area.primary_radius)
            || area.affects > 127
    }) {
        return Err("Invalid weapon damage rings");
    }
    Ok(())
}

impl Skirmish {
    pub(super) fn weapon_target(&self, target: usize, pattern: Option<[f32; 3]>) -> WeaponTarget {
        let unit = &self.catalog[self.entities[target].type_index];
        let offset = self.entities[target].aircraft_targeting_offset(unit, self.tick);
        let origin = self.object_position(target);
        // Weapon::fireWeapon resolves ScatterTargets before fireWeaponTemplate;
        // an explicit pattern position never receives a sneaky object offset.
        let position = pattern.unwrap_or_else(|| {
            offset.map_or(origin, |offset| {
                std::array::from_fn(|axis| origin[axis] + offset[axis])
            })
        });
        WeaponTarget {
            object: (pattern.is_none() && offset.is_none()).then_some(target),
            position,
            projectile_position: position,
            scatter_radius: 0.0,
        }
    }

    pub(super) fn scatter_weapon_destination(
        &mut self,
        source_type: usize,
        slot: usize,
        target: &mut WeaponTarget,
    ) {
        let weapon = self.catalog[source_type]
            .weapon_template(slot)
            .expect("fired weapon slot");
        let infantry = target.object.is_some_and(|id| {
            self.catalog[self.entities[id].type_index]
                .maintenance
                .healing_kind_flags
                & 1
                != 0
        });
        let scatter = weapon.scatter
            + if infantry {
                weapon.infantry_scatter
            } else {
                0.0
            };
        if scatter == 0.0 {
            return;
        }
        target.scatter_radius = super::startup::random_real(&mut self.random, 0.0, scatter);
        let angle = super::startup::random_angle(&mut self.random);
        let offset = [
            target.scatter_radius * angle.cos(),
            target.scatter_radius * angle.sin(),
        ];
        target.projectile_position[0] += offset[0];
        target.projectile_position[1] += offset[1];
        target.projectile_position[2] =
            self.ground_height([target.projectile_position[0], target.projectile_position[1]]);
    }

    #[cfg(test)]
    pub(super) fn weapon_destination(
        &mut self,
        source_type: usize,
        slot: usize,
        target: usize,
    ) -> ([f32; 3], Option<usize>) {
        let mut target = self.weapon_target(target, None);
        self.scatter_weapon_destination(source_type, slot, &mut target);
        let tracked = (target.scatter_radius == 0.0)
            .then_some(target.object)
            .flatten();
        let position = tracked.map_or(target.projectile_position, |id| self.target_center(id));
        (position, tracked)
    }

    pub(in crate::skirmish) fn apply_weapon_hit(
        &mut self,
        target: usize,
        source: usize,
        amount: f32,
        damage: super::DamageType,
        death: super::DeathType,
    ) {
        let amount = self.body_damage_input(target, damage, amount);
        self.record_slow_death_damage(target, 0.0, 0.0);
        let second_life = self.battle_bus_damage_raw(target, damage, amount);
        self.apply_active_weapon_hit(target, source, second_life.unwrap_or(amount), damage, death);
        if second_life.is_some() {
            self.start_battle_bus_second_life(target, death);
        }
    }

    fn apply_active_weapon_hit(
        &mut self,
        target: usize,
        source: usize,
        amount: f32,
        damage: super::DamageType,
        death: super::DeathType,
    ) {
        let unit = &self.catalog[self.entities[target].type_index];
        if matches!(unit.body_kind, crate::skirmish::BodyKind::Inactive) {
            self.apply_damage(target, Some(source), amount, damage, death);
            return;
        }
        if self.entities[target].indestructible() || self.entities[target].is_effectively_dead() {
            return;
        }
        if matches!(damage, super::DamageType::Healing) {
            self.apply_healing(target, Some(source), amount);
            return;
        }
        let fx_set = self.body_damage_fx_set(target);
        if matches!(
            damage,
            super::DamageType::SubdualBuilding
                | super::DamageType::SubdualUnresistable
                | super::DamageType::SubdualVehicle
                | super::DamageType::SubdualMissile
        ) {
            if unit.maintenance.subdual.is_none() {
                return;
            }
            let amount = self.entities[target].armor_damage(unit, damage, amount);
            self.apply_subdual_damage(target, amount);
            self.finish_handled_body_damage(target, source, amount, damage, death, fx_set);
            return;
        }
        if matches!(damage, super::DamageType::KillGarrisoned) {
            let count = amount.floor().to_u32().unwrap_or(0);
            let amount = self.entities[target].armor_damage(unit, damage, amount);
            self.clear_garrison(source, target, count, &[], &[]);
            self.finish_handled_body_damage(target, source, amount, damage, death, fx_set);
            return;
        }
        if matches!(damage, super::DamageType::Status) {
            let amount = self.entities[target].armor_damage(unit, damage, amount);
            self.do_painted_status_damage(target, (amount * 0.03).ceil());
            self.finish_handled_body_damage(target, source, amount, damage, death, fx_set);
            return;
        }
        if matches!(damage, super::DamageType::KillPilot) {
            let amount = self.entities[target].armor_damage(unit, damage, amount);
            self.kill_vehicle_pilot(target, source, amount);
            self.finish_handled_body_damage(target, source, amount, damage, death, fx_set);
            return;
        }
        let amount = self.entities[target].adjust_damage(unit, damage, amount);
        self.apply_damage(target, Some(source), amount, damage, death);
    }

    /// Raw health damage enters `UndeadBody` before the `ActiveBody` early returns.
    pub(in crate::skirmish) fn attempt_health_damage(
        &mut self,
        target: usize,
        source: Option<usize>,
        raw: f32,
        damage: super::DamageType,
        death: super::DeathType,
    ) {
        let raw = self.body_damage_input(target, damage, raw);
        self.record_slow_death_damage(target, 0.0, 0.0);
        let second_life = self.battle_bus_damage_raw(target, damage, raw);
        let unit = &self.catalog[self.entities[target].type_index];
        let amount = self.entities[target].adjust_damage(unit, damage, second_life.unwrap_or(raw));
        self.apply_damage(target, source, amount, damage, death);
        if second_life.is_some() {
            self.start_battle_bus_second_life(target, death);
        }
    }

    // alreadyHandled bypasses internalChangeHealth, retaining its previous-health
    // value for callbacks and clipped damage from the last actual health change.
    fn finish_handled_body_damage(
        &mut self,
        target: usize,
        source: usize,
        amount: f32,
        damage: super::DamageType,
        death: super::DeathType,
        fx_set: Option<usize>,
    ) {
        let actual = if amount > 0.0 {
            let old_state = self.body_damage_state(target);
            self.record_body_damage(target, Some(source), damage);
            self.body_health_loss_callbacks(target, Some(source), amount, damage, death);
            self.notify_containment_body_state(target, old_state);
            if self.body_damage_state(target) != old_state {
                self.structure_damage_transition(
                    target,
                    self.entities[target].damage_state.previous_health,
                );
            }
            self.body_damage_feedback(target, old_state);
            if self.entities[target].health <= 0.0
                && self.entities[target].damage_state.previous_health > 0.0
            {
                self.entities[target].damage_state.death_type = death;
                self.finish_death(target, Some(source));
            }
            amount
        } else {
            0.0
        };
        self.do_body_damage_fx(target, fx_set, actual, damage, Some(source));
        self.repulse_after_damage(target);
        self.retaliate_after_damage(target, Some(source));
    }

    pub(super) fn deal_bound_weapon_damage(
        &mut self,
        source: usize,
        binding: super::WeaponBinding,
        primary: Option<usize>,
        position: [f32; 3],
        bonus: f32,
    ) {
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("fired weapon slot");
        let radius = self.weapon_factor(source, weapon, super::containment::bonuses::RADIUS);
        self.deal_bound_weapon_damage_with_radius(
            &Source::Object(source),
            binding,
            primary,
            position,
            bonus,
            radius,
        );
    }

    fn deal_direct_weapon_damage(
        &mut self,
        source: usize,
        damage_source: usize,
        binding: super::WeaponBinding,
        primary: Option<usize>,
        bonus: f32,
    ) {
        let Some(target) = primary else { return };
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("fired weapon slot");
        let Some(target) = self.hive_damage_target(target, source, weapon.damage_type) else {
            return;
        };
        let amount = weapon.damage * bonus;
        let shock = weapon.shockwave;
        let damage_type = weapon.damage_type;
        let death_type = weapon.death_type;
        let vector = self.weapon_shock_vector(target, damage_source);
        self.apply_weapon_hit(target, source, amount, damage_type, death_type);
        self.apply_damage_shock(target, vector, shock[0], shock[1], shock[2]);
    }

    fn weapon_shock_vector(&self, target: usize, source: usize) -> [f32; 3] {
        let mut vector = if self.entities[source].exists() {
            let target = self.object_position(target);
            let source = self.object_position(source);
            std::array::from_fn(|axis| target[axis] - source[axis])
        } else {
            [0.0; 3]
        };
        if vector.iter().all(|value| value.abs() < 0.0001) {
            vector[2] = 1.0;
        }
        vector
    }

    pub(super) fn deal_bound_weapon_damage_with_radius(
        &mut self,
        origin: &Source,
        binding: super::WeaponBinding,
        primary: Option<usize>,
        position: [f32; 3],
        bonus: f32,
        radius_bonus: f32,
    ) {
        let (source, damage_source) = match *origin {
            Source::Object(source) => (source, source),
            Source::Projectile { launcher, object } => (launcher, object),
        };
        let primary = primary.filter(|&id| self.entities[id].exists());
        // WeaponTemplate::dealDamageInternal overrides pos for a designated victim.
        let position = primary.map_or(position, |id| self.object_position(id));
        let unit = &self.catalog[binding.type_index];
        let weapon = unit
            .weapon_template(binding.weapon_index)
            .expect("fired weapon slot");
        let Some(area) = &weapon.area else {
            self.deal_direct_weapon_damage(source, damage_source, binding, primary, bonus);
            return;
        };
        let primary_radius = area.primary_radius * radius_bonus;
        let secondary_radius = area.secondary_radius * radius_bonus;
        let radius = primary_radius.max(secondary_radius);
        if radius == 0.0 && area.affects & 32 != 0 {
            self.apply_weapon_hit(
                damage_source,
                source,
                999_999.0,
                weapon.damage_type,
                weapon.death_type,
            );
            return;
        }
        if radius == 0.0 {
            self.deal_direct_weapon_damage(source, damage_source, binding, primary, bonus);
            return;
        }
        let mut hits = Vec::new();
        for target in self
            .partition
            .fastest_range_objects([position[0], position[1]], radius)
        {
            let distance = (projectiles::distance(position, self.target_center(target))
                - self.body_geometry_bounds(target).sphere_radius)
                .max(0.0);
            // PartitionManager's FROM_BOUNDINGSPHERE_3D range test is strict.
            if distance * distance >= radius * radius {
                continue;
            }
            hits.push((target, distance * distance));
        }
        let shock = weapon.shockwave;
        let primary_damage = weapon.damage * bonus;
        let secondary_damage = area.secondary_damage * bonus;
        let affects = area.affects;
        let death_type = weapon.death_type;
        let damage_type = weapon.damage_type;
        for (target, squared) in hits {
            let mut kill_self = false;
            // The native iterator retains its objects/distances before damage, but
            // relationship and source-liveness checks occur after prior victims.
            if Some(target) != primary && self.entities[damage_source].exists() {
                kill_self = target == damage_source && affects & 32 != 0;
                if !kill_self {
                    if affects & 8 == 0
                        && (target == damage_source
                            || self.entities[damage_source].producer == Some(target))
                    {
                        continue;
                    }
                    let source_type = self.entities[damage_source].type_index;
                    let target_type = self.entities[target].type_index;
                    if affects & 16 != 0
                        && self.catalog[source_type].equivalent_to(&self.catalog[target_type])
                        && self.object_relationship(damage_source, target) == 2
                    {
                        continue;
                    }
                    if affects & 64 != 0 && self.height_above_ground(target) > -9.0 * self.gravity {
                        continue;
                    }
                    let relationship = match self.object_relationship(target, damage_source) {
                        2 => 1,
                        0 => 2,
                        _ => 4,
                    };
                    if affects & relationship == 0 {
                        continue;
                    }
                }
            }
            let damage = if kill_self {
                999_999.0
            } else if squared <= primary_radius * primary_radius {
                primary_damage
            } else {
                secondary_damage
            };
            let Some(target) = self.hive_damage_target(target, source, damage_type) else {
                continue;
            };
            let vector = self.weapon_shock_vector(target, damage_source);
            self.apply_weapon_hit(target, source, damage, damage_type, death_type);
            self.apply_damage_shock(target, vector, shock[0], shock[1], shock[2]);
        }
    }
}

#[cfg(test)]
mod tests;
