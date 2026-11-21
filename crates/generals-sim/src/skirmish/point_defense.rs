//! Native `PointDefenseLaserUpdate`, `DAMAGE_STATUS` and the targeting-paint flag.
use super::{DamageType, Entity, Skirmish, UnitType, WeaponRules};

#[cfg(test)]
pub(super) mod tests;

pub struct Rules {
    pub weapon: WeaponRules,
    pub primary: Vec<String>,
    pub secondary: Vec<String>,
    pub scan_frames: u32,
    pub scan_range: f32,
    pub predict_velocity: f32,
}

pub struct Shot {
    pub weapon_type: usize,
    pub frame: u64,
    pub source: usize,
    pub source_position: [f32; 3],
    pub weapon_index: usize,
    pub target: Option<usize>,
    pub target_position: [f32; 3],
    pub veterancy: u8,
    pub lifetime: Option<u32>,
}

pub(super) struct State {
    target: Option<usize>,
    in_range: bool,
    scan: i32,
    reload: i32,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            target: None,
            in_range: false,
            scan: 0,
            reload: 0,
        }
    }
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    for rules in &unit.behavior.point_defense {
        if !rules.scan_range.is_finite()
            || rules.scan_range <= rules.weapon.range
            || !rules.predict_velocity.is_finite()
            || rules.primary.is_empty()
            || !matches!(rules.weapon.damage_type, DamageType::Laser)
            || !rules.weapon.laser
            || rules.weapon.anti_mask & 0x02 == 0
        {
            return Err("Invalid or unsupported point-defense module");
        }
    }
    Ok(())
}

impl Entity {
    #[must_use]
    pub const fn painted(&self) -> bool {
        self.helpers.status_damage.painted
    }

    pub(super) fn update_painted_target(
        &mut self,
        unit: &UnitType,
        painted: bool,
        frame: u64,
        extra: u32,
        random: &mut [u32; 6],
    ) {
        if self.firing.painted_target != painted {
            self.firing.painted_target = painted;
            self.weapon_bonus_changed(unit, frame, extra, random);
        }
    }
}

impl Skirmish {
    pub(super) fn fire_laser(
        &mut self,
        source: usize,
        binding: super::WeaponBinding,
        target: &super::damage::WeaponTarget,
        bonus: f32,
    ) -> bool {
        let unit = &self.catalog[binding.type_index];
        if !unit
            .weapon_template(binding.weapon_index)
            .expect("fired weapon")
            .laser
        {
            return false;
        }
        let weapon = unit
            .weapon_template(binding.weapon_index)
            .expect("fired laser weapon");
        let (primary, secondary) = weapon.area.as_ref().map_or((0.0, 0.0), |area| {
            (area.primary_radius, area.secondary_radius)
        });
        let radius_bonus = self.weapon_factor(source, weapon, super::containment::bonuses::RADIUS);
        let hits = target.scatter_radius <= primary * radius_bonus
            || target.scatter_radius <= secondary * radius_bonus;
        let victim = hits.then_some(target.object).flatten();
        let mut position = victim.map_or(target.projectile_position, |id| self.object_position(id));
        if victim.is_some_and(|id| self.victim_anti_mask(id) & 0x02 != 0) {
            position[2] += 10.0;
        }
        let at_source = weapon.area.as_ref().is_some_and(|area| area.at_source);
        let lifetime = self.laser_lifetime(binding);
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::Laser(
                self.presentation.laser_shots.len(),
            ));
        self.presentation
            .laser_shots
            .push(self.laser_shot(source, binding, victim, position, lifetime));
        self.deal_bound_weapon_damage(
            source,
            binding,
            if at_source { None } else { victim },
            victim.map_or(target.projectile_position, |id| self.object_position(id)),
            bonus,
        );
        true
    }

    fn laser_shot(
        &self,
        source: usize,
        binding: super::WeaponBinding,
        target: Option<usize>,
        position: [f32; 3],
        lifetime: Option<u32>,
    ) -> Shot {
        Shot {
            weapon_type: binding.type_index,
            frame: self.tick,
            source,
            source_position: self.object_position(source),
            weapon_index: binding.weapon_index,
            target,
            target_position: position,
            veterancy: self.entities[source].veterancy,
            lifetime,
        }
    }

    fn laser_lifetime(&mut self, binding: super::WeaponBinding) -> Option<u32> {
        let range = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("fired laser weapon")
            .laser_lifetime?;
        // LifetimeUpdate and DeletionUpdate sample on object construction.
        // RETAIL_COMPATIBLE_CRC=0 consumes no draw for equal bounds.
        let [low, high] = range.map(|frame| i32::from_ne_bytes(frame.to_ne_bytes()));
        let delay = if low >= high {
            range[1]
        } else {
            range[0].wrapping_add(self.random_between(range[1].wrapping_sub(range[0])))
        };
        Some(delay.max(1))
    }

    pub(in crate::skirmish) fn construct_point_defense(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.point_defense.len(), ordinal);
        self.entities[id].behaviors.point_defense.push(State::new());
    }

    pub(in crate::skirmish) fn update_point_defense(&mut self, id: usize, module: usize) -> i32 {
        if self.entities[id].is_effectively_dead() {
            return 0x3fff_ffff;
        }
        let frames = self.catalog[self.entities[id].type_index]
            .behavior
            .point_defense[module]
            .scan_frames;
        let state = &mut self.entities[id].behaviors.point_defense[module];
        if state.scan > 0 {
            state.scan -= 1;
            self.fire_point_defense(id, module);
        } else {
            state.scan = i32::from_ne_bytes(frames.to_ne_bytes());
            self.scan_point_defense(id, module);
            if self.entities[id].behaviors.point_defense[module]
                .target
                .is_some()
            {
                self.fire_point_defense(id, module);
            }
        }
        1
    }

    fn target_position(&self, target: usize) -> Option<[f32; 3]> {
        self.entities
            .get(target)
            .filter(|actor| actor.exists())
            .map(|_| self.object_position(target))
    }

    fn point_target_alive(&self, target: usize) -> bool {
        self.entities
            .get(target)
            .is_some_and(|actor| actor.exists() && !actor.is_effectively_dead())
    }

    fn rescan_point_defense(&mut self, id: usize, module: usize) -> bool {
        let delay = self.random_between(3);
        let state = &mut self.entities[id].behaviors.point_defense[module];
        state.scan = i32::try_from(delay).expect("native scan delay");
        state.target = None;
        if delay == 0 {
            self.scan_point_defense(id, module);
            let frames = self.catalog[self.entities[id].type_index]
                .behavior
                .point_defense[module]
                .scan_frames;
            self.entities[id].behaviors.point_defense[module].scan =
                i32::from_ne_bytes(frames.to_ne_bytes());
        }
        delay == 0
    }

    fn fire_point_defense(&mut self, id: usize, module: usize) {
        let mut target = self.entities[id].behaviors.point_defense[module].target;
        if let Some(position) = target.and_then(|target| self.target_position(target)) {
            let range = self.catalog[self.entities[id].type_index]
                .behavior
                .point_defense[module]
                .weapon
                .range;
            let distance = super::movement::distance_squared(
                self.entities[id].position,
                [position[0], position[1]],
            )
            .sqrt();
            if distance < range {
                self.entities[id].behaviors.point_defense[module].in_range = true;
            } else if self.entities[id].behaviors.point_defense[module].in_range {
                if self.rescan_point_defense(id, module) {
                    target = None;
                }
            } else {
                self.entities[id].behaviors.point_defense[module].in_range = false;
            }
        } else {
            target = None;
        }
        let state = &mut self.entities[id].behaviors.point_defense[module];
        if state.reload > 0 {
            state.reload -= 1;
            return;
        }
        let Some(target) = target.filter(|_| state.in_range) else {
            return;
        };
        if self.point_target_alive(target) {
            self.shoot_point_defense(id, module, target);
            let weapon = &self.catalog[self.entities[id].type_index]
                .behavior
                .point_defense[module]
                .weapon;
            self.entities[id].behaviors.point_defense[module].reload =
                i32::try_from(weapon.sample_shot_delay(&mut self.random, 1.0))
                    .expect("native point-defense shot delay");
        }
        if !self.point_target_alive(target) {
            self.rescan_point_defense(id, module);
        }
    }

    fn shoot_point_defense(&mut self, source: usize, module: usize, target: usize) {
        let unit = &self.catalog[self.entities[source].type_index];
        let weapon_index = 7 + unit.weapon_set_rules.templates.len() + module;
        let binding = super::WeaponBinding {
            type_index: self.entities[source].type_index,
            weapon_index,
            weapon_slot: 2,
        };
        self.fire_temporary_weapon_target(source, binding, target);
    }

    fn scan_point_defense(&mut self, id: usize, module: usize) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.point_defense[module];
        let mut best = [None; 4];
        let mut distances = [f32::MAX; 4];
        for target in self
            .partition
            .fastest_range_objects(actor.position, rules.scan_range)
        {
            let victim = &self.entities[target];
            let kinds = &self.catalog[victim.type_index].kind_of;
            let category = if kinds.iter().any(|kind| rules.primary.contains(kind)) {
                0
            } else if kinds.iter().any(|kind| rules.secondary.contains(kind)) {
                1
            } else {
                continue;
            };
            if (!victim.is_airborne_target() && rules.weapon.anti_mask & 0x02 == 0)
                || self.object_relationship(id, target) != 0
                || (victim.stealthed() && !victim.detected() && !victim.has_native_disguise())
            {
                continue;
            }
            let squared = super::movement::distance_squared(actor.position, victim.position);
            if squared >= rules.scan_range * rules.scan_range {
                continue;
            }
            let distance = squared.sqrt();
            let index = category + if distance <= rules.weapon.range { 0 } else { 2 };
            // The prediction branch computes a future point but repeats the
            // current center-distance query, so it cannot change the candidate.
            if best[index].is_none() || distance < distances[index] {
                best[index] = Some(target);
                distances[index] = distance;
            }
        }
        let found = best
            .into_iter()
            .enumerate()
            .find(|(_, target)| target.is_some());
        let state = &mut self.entities[id].behaviors.point_defense[module];
        state.target = found.and_then(|(_, target)| target);
        state.in_range = found.is_some_and(|(index, _)| index < 2);
    }
}
