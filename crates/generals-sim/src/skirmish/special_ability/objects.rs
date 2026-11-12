//! Native special-object list ownership; destruction is deferred, not a body kill.
use super::{PowerType, Skirmish};
use crate::skirmish::{laser_update, weapons::Lock};

impl Skirmish {
    pub(super) fn validate_ability_objects(&mut self, id: usize, ordinal: usize) {
        let objects =
            std::mem::take(&mut self.entities[id].behaviors.special_abilities[ordinal].objects);
        let mut retained = Vec::with_capacity(objects.len());
        for object in objects {
            if self.entities[object].exists() {
                retained.push(object);
            } else {
                let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
                state.object_entries = state.object_entries.wrapping_sub(1);
            }
        }
        self.entities[id].behaviors.special_abilities[ordinal].objects = retained;
    }

    pub(super) fn kill_ability_objects(&mut self, id: usize, ordinal: usize) {
        let objects =
            std::mem::take(&mut self.entities[id].behaviors.special_abilities[ordinal].objects);
        for object in objects {
            if self.entities[object].exists() {
                self.destroy_maintenance_object(object);
            }
        }
        self.entities[id].behaviors.special_abilities[ordinal].object_entries = 0;
        if self.ability_kind(id, ordinal) == PowerType::SpecialMissileDefenderLaserGuidedMissiles
            && self.allocated_weapon(id, 0).is_some()
            && !matches!(self.entities[id].weapon_set.lock, Lock::Permanent)
        {
            self.entities[id].weapon_set.current = 0;
            self.entities[id].weapon_set.lock = Lock::Temporary;
        }
    }

    pub(super) fn create_ability_object(&mut self, id: usize, ordinal: usize) -> Option<usize> {
        let rules = self.ability_rules(id, ordinal);
        if self.entities[id].behaviors.special_abilities[ordinal].object_entries
            == rules.objects.maximum
        {
            if rules.objects.persistent {
                return None;
            }
            self.kill_ability_objects(id, ordinal);
        }
        let kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == self.ability_rules(id, ordinal).objects.name)?;
        let owner = self.entities[id].owner;
        let team = self.team_membership.team_of(id);
        let object = self.new_object_on_team(kind, owner, team);
        let position = self.object_position(id);
        let angle = self.entities[id].facing;
        let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
        state.objects.push(object);
        state.object_entries = state.object_entries.wrapping_add(1);
        self.set_object_position(object, position);
        self.set_object_orientation(object, angle);
        self.entities[object].set_experience_sink(Some(id));
        if let Some(rules) = self.catalog[self.entities[object].type_index].physics_motion_rules() {
            let rate = rules.center_of_mass_offset;
            self.entities[object].set_physics_pitch_rate(rate);
            self.entities[object].set_physics_airborne_friction(false);
        }
        Some(object)
    }

    pub(super) fn initialize_ability_laser(
        &mut self,
        id: usize,
        ordinal: usize,
        object: usize,
        target: Option<usize>,
    ) -> bool {
        let bone = self.ability_rules(id, ordinal).objects.bone.clone();
        let position = self.logical_bone_position(id, &bone).map_or_else(
            || self.object_position(id),
            |local| {
                let (position, basis) = self.object_transform(id);
                let offset = crate::skirmish::physical_ocl::transform::vector(basis, local);
                std::array::from_fn(|axis| offset[axis] + position[axis])
            },
        );
        let end = target.map_or(position, |target| self.target_center(target));
        if self.init_native_laser(
            object,
            laser_update::Initialization {
                parent: Some(id),
                target,
                start: Some(position),
                end: Some(end),
                bone,
                size_delta: 0,
            },
        ) {
            true
        } else {
            self.kill_ability_objects(id, ordinal);
            false
        }
    }
}
