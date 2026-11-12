//! SH `ActiveBody`696–807 recruits idle allies; `AIGuardRetaliate` owns pursuit/return.
use super::{Order, Skirmish, ai::GuardPhase, movement::distance_squared};

#[cfg(test)]
mod tests;

impl Skirmish {
    /// Applies the local client's logical player retaliation preference.
    pub fn set_retaliation(&mut self, player: usize, enabled: bool) {
        if let Some(value) = self.retaliation_enabled.get_mut(player) {
            *value = enabled;
        }
    }

    pub(super) fn retaliation_building_target(&self, id: usize, target: usize) -> bool {
        let unit = &self.catalog[self.entities[target].type_index];
        !unit.kind.is_building()
            || self.computer_player(self.entities[id].owner)
            || unit.kind_of.iter().any(|kind| kind == "FS_BASE_DEFENSE")
            || (unit.contain.is_some()
                && (unit.weapon.damage > 0.0
                    || self.passengers(target).iter().any(|&rider| {
                        self.passenger_can_fire(rider)
                            && self.entities[rider]
                                .active_weapon(&self.catalog[self.entities[rider].type_index])
                                .damage
                                > 0.0
                    })))
    }

    pub(super) fn retaliate_after_damage(&mut self, victim: usize, aggressor: Option<usize>) {
        let Some(aggressor) = aggressor.filter(|&id| id < self.entities.len()) else {
            return;
        };
        let Some(victim_actor) = self.entities.get(victim) else {
            return;
        };
        let owner = victim_actor.owner;
        let victim_type = &self.catalog[victim_actor.type_index];
        let attacker = &self.entities[aggressor];
        let attacker_type = &self.catalog[attacker.type_index];
        if !self
            .retaliation_enabled
            .get(owner)
            .copied()
            .unwrap_or(false)
            || self.computer_player(owner)
            || victim_type.kind_of.iter().any(|kind| kind == "DRONE")
            || !attacker.alive()
            || attacker.masked()
            || !self.enemies(owner, attacker.owner)
            || self.height_above_ground(aggressor)
                > attacker
                    .active_locomotor(attacker_type)
                    .airborne_targeting_height
            || self.range_distance(victim, aggressor) > self.ai_rules.max_retaliation_distance
        {
            return;
        }
        let position = victim_actor.position;
        let radius = self.ai_rules.retaliation_friends_radius + self.object_radius(victim);
        for id in self.partition.fastest_range_objects(position, radius) {
            let actor = &self.entities[id];
            let unit = &self.catalog[actor.type_index];
            if !actor.complete()
                || actor.masked()
                || actor.unmanned()
                || actor.paralyzed()
                || actor.hacked()
                || !actor.ai_idle()
                || actor.laser_target().is_some()
                || actor.object_status & (1 << 24) != 0
                || (actor.stealthed() && !actor.detected())
                || unit
                    .kind_of
                    .iter()
                    .any(|kind| matches!(kind.as_str(), "DRONE" | "IMMOBILE" | "CANNOT_RETALIATE"))
                || unit.speed <= 0.0
                || !self.allied(owner, actor.owner)
                || distance_squared(position, actor.position) > radius * radius
                || !self.weapon_target_reachable(id, aggressor)
                || !self.maintenance_target_visible(actor.owner, aggressor)
            {
                continue;
            }
            let position = self.object_position(id);
            self.native_retaliate_from(
                id,
                aggressor,
                position,
                0x7fff_ffff,
                crate::skirmish::ai_goals::COMMAND_FROM_AI,
            );
        }
    }

    pub(in crate::skirmish) fn native_retaliate_from(
        &mut self,
        id: usize,
        victim: usize,
        position: [f32; 3],
        shots: u32,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Retaliate {
                    victim,
                    position,
                    shots,
                },
            },
        ) || !self.native_ai_command_allowed(id)
            || !self.entities.get(victim).is_some_and(super::Entity::exists)
        {
            return;
        }
        self.clear_ai_goal(id);
        self.set_ai_goal_object(id, victim);
        let position = self.clipped_ai_goal_position(id, position, source);
        self.set_ai_goal_position(id, position);
        self.set_last_command_source(id, source);
        self.set_order(id, Order::Guard([position[0], position[1]]));
        self.entities[id].ai.guard.retaliating = true;
        self.entities[id].ai.guard.center = position;
        self.entities[id].ai.guard.home_height = position[2];
        self.entities[id].ai.ai_target = Some(victim);
        self.enter_guard_phase(id, GuardPhase::Aggressor);
        let slot = self.entities[id].weapon_slot();
        self.entities[id].weapon_set.weapons[slot].max_shots = Some(shots);
    }
}
