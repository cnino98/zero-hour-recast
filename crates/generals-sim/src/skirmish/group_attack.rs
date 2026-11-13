//! Native selected-group attack dispatch, SH Core `GameLogicDispatch` / MD `AIGroup`.
use super::{
    CanAttackResult, Skirmish, ai_goals::COMMAND_FROM_PLAYER, combat::attack::Goal, weapons::Lock,
};

const NO_MAX_SHOTS_LIMIT: u32 = i32::MAX.unsigned_abs();

impl Skirmish {
    /// Native normal player object query with its explicit command source.
    pub fn player_attack_result(&mut self, id: usize, victim: usize) -> CanAttackResult {
        if self.entities.get(id).is_none_or(|object| !object.exists())
            || self
                .entities
                .get(victim)
                .is_none_or(|object| !object.exists())
            || !self.object_can_attack(id)
        {
            return CanAttackResult::NotPossible;
        }
        self.player_weapon_attack_result(id, Some(victim), self.object_position(victim), false)
    }

    /// `CommandXlat::canObjectForceAttack`; the caller chooses the first Drawable.
    pub fn force_attack_result(
        &mut self,
        id: usize,
        victim: Option<usize>,
        position: [f32; 3],
    ) -> CanAttackResult {
        if self.entities.get(id).is_none_or(|object| !object.exists())
            || !self.object_can_attack(id)
        {
            return CanAttackResult::NotPossible;
        }
        if let Some(victim) = victim {
            if !self.entities.get(victim).is_some_and(super::Entity::exists) {
                return CanAttackResult::NotPossible;
            }
            let mut result = self.player_weapon_attack_result(id, Some(victim), position, true);
            if self.attack_kind(id, "SPAWNS_ARE_THE_WEAPONS") {
                if !matches!(
                    result,
                    CanAttackResult::Possible | CanAttackResult::PossibleAfterMoving
                ) {
                    if let Some(slave) = self.closest_attack_member(
                        self.spawned_children(id),
                        self.object_position(victim),
                    ) {
                        result =
                            self.player_weapon_attack_result(slave, Some(victim), position, true);
                    }
                } else if let Some(rider) = self.closest_attack_member(
                    &self.entities[id].containment.passengers,
                    self.object_position(victim),
                ) {
                    // Native assigns this result even if the rider says NOT_POSSIBLE.
                    result = self.player_weapon_attack_result(rider, Some(victim), position, true);
                }
            }
            result
        } else {
            let mut tested = id;
            if self.attack_kind(id, "IMMOBILE") || self.attack_kind(id, "SPAWNS_ARE_THE_WEAPONS") {
                if !self.entities[id].behaviors.spawns.is_empty() {
                    if let Some(slave) =
                        self.closest_attack_member(self.spawned_children(id), position)
                    {
                        tested = slave;
                    }
                } else if !matches!(
                    self.player_weapon_attack_result(id, None, position, false),
                    CanAttackResult::Possible
                ) && let Some(rider) =
                    self.closest_attack_member(&self.entities[id].containment.passengers, position)
                {
                    tested = rider;
                }
            }
            self.player_weapon_attack_result(tested, None, position, false)
        }
    }

    fn closest_attack_member(&self, members: &[usize], position: [f32; 3]) -> Option<usize> {
        let mut closest = None;
        let mut best = 0.0;
        for &id in members {
            if !self.entities[id].exists() {
                continue;
            }
            let distance = super::movement::distance_squared(
                self.entities[id].position,
                [position[0], position[1]],
            );
            if closest.is_none() || best > distance {
                closest = Some(id);
                best = distance;
            }
        }
        closest
    }

    /// Core `onDoForceAttackGround`: group-idle-dependent lock lifetime.
    /// # Errors
    /// Rejects a missing group or a nonfinite picked world position.
    pub fn group_force_attack_position(
        &mut self,
        members: &[usize],
        position: [f32; 3],
    ) -> Result<(), &'static str> {
        if members.is_empty()
            || members
                .iter()
                .any(|&id| self.entities.get(id).is_none_or(|actor| !actor.exists()))
        {
            return Err("Missing selected group");
        }
        if position.iter().any(|value| !value.is_finite()) {
            return Err("Invalid attack position");
        }
        let idle = members.iter().all(|&id| {
            self.entities[id].ai.registration.is_none()
                || self.entities[id].ai_idle()
                || self.entities[id].is_effectively_dead()
        });
        for &id in members {
            if idle {
                self.release_group_temporary_lock(id);
            } else if self.allocated_weapon(id, 0).is_some()
                && !matches!(self.entities[id].weapon_set.lock, Lock::Permanent)
            {
                self.entities[id].weapon_set.current = 0;
                self.entities[id].weapon_set.lock = Lock::Temporary;
            }
        }
        for &id in members {
            if self.container_fire_allowed(id, None) {
                for rider in self.passengers(id).to_vec() {
                    if matches!(
                        self.player_weapon_attack_result(rider, None, position, false),
                        CanAttackResult::Possible | CanAttackResult::PossibleAfterMoving
                    ) {
                        self.player_group_attack_member(rider, Goal::Position(position), false);
                    }
                }
            }
            if self.catalog[self.entities[id].type_index]
                .behavior
                .spawns
                .first()
                .is_some_and(|rules| !rules.has(super::spawn::Rules::FREE_WILL))
            {
                for child in self.spawned_children(id).to_vec() {
                    if self.entities[child].exists() {
                        self.player_group_attack_member(child, Goal::Position(position), false);
                    }
                }
            }
            self.player_group_attack_member(id, Goal::Position(position), false);
        }
        if !idle {
            for &id in members {
                self.release_group_temporary_lock(id);
            }
        }
        Ok(())
    }

    /// `AIGroup::groupAttackObjectPrivate`: prepend, stable near-to-far, then
    /// contained weapons, forced slaves, and the member's own AI command.
    /// # Errors
    /// Rejects a missing group or a destroyed target.
    pub fn group_attack_objects(
        &mut self,
        members: &[usize],
        victim: usize,
        forced: bool,
    ) -> Result<(), &'static str> {
        if members.is_empty()
            || self
                .entities
                .get(victim)
                .is_none_or(|actor| !actor.exists())
            || members
                .iter()
                .any(|&id| self.entities.get(id).is_none_or(|actor| !actor.exists()))
        {
            return Err("Missing selected group or target");
        }
        for &id in members {
            self.release_group_temporary_lock(id);
        }
        let position = self.object_position(victim);
        let mut ordered: Vec<_> = members
            .iter()
            .rev()
            .map(|&id| {
                (
                    id,
                    super::movement::distance_squared(
                        self.entities[id].position,
                        [position[0], position[1]],
                    ),
                )
            })
            .collect();
        ordered.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (id, _) in ordered {
            if self.container_fire_allowed(id, None) {
                for rider in self.passengers(id).to_vec() {
                    if matches!(
                        self.player_weapon_attack_result(rider, Some(victim), position, forced),
                        CanAttackResult::Possible | CanAttackResult::PossibleAfterMoving
                    ) {
                        self.player_group_attack_member(rider, Goal::Object(victim), forced);
                    }
                }
            }
            self.group_spawn_attack(id, victim, COMMAND_FROM_PLAYER);
            if id != victim {
                self.player_group_attack_member(id, Goal::Object(victim), forced);
            }
        }
        Ok(())
    }

    fn player_group_attack_member(&mut self, id: usize, goal: Goal, forced: bool) {
        if self.entities[id].ai.registration.is_none() {
            return;
        }
        // Dozer clears its construction indicator even when its command gate refuses.
        if self.entities[id].ai.dozer.is_some() {
            self.entities[id].drawable.model_conditions.constructing = false;
        }
        if !self.native_ai_command_allowed(id) {
            return;
        }
        self.dozer_player_command(id);
        self.native_attack_goal_from(id, goal, NO_MAX_SHOTS_LIMIT, COMMAND_FROM_PLAYER, forced);
        self.finish_dozer_player_command(id);
    }

    pub(super) fn release_group_temporary_lock(&mut self, id: usize) {
        if matches!(self.entities[id].weapon_set.lock, Lock::Temporary) {
            self.entities[id].weapon_set.lock = Lock::None;
        }
    }
}

#[cfg(test)]
mod tests;
