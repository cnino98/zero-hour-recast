//! Original `MSG_DO_WEAPON_AT_LOCATION` / attack-position path.
use crate::skirmish::{Order, Skirmish, damage::WeaponTarget, weapons::Lock};

impl Skirmish {
    #[must_use]
    pub fn location_weapon_ready(&self, id: usize, slot: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            actor.complete()
                && !actor.hacked()
                && !actor.paralyzed()
                && actor.contained_by().is_none()
                && slot < 3
                && self.catalog[actor.type_index].location_weapons & (1 << slot) != 0
                && self.allocated_weapon(id, slot).is_some()
        })
    }

    /// Selects the original temporary weapon lock and attacks the clicked ground position.
    /// # Errors
    /// Rejects disabled actors, absent location weapons and positions outside the map.
    pub fn fire_weapon_at(
        &mut self,
        id: usize,
        slot: usize,
        point: [f32; 2],
    ) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.location_weapon_ready(id, slot) || self.cell_at(point).is_none() {
            return Err("Choose an active location weapon and a point on the map");
        }
        self.dozer_player_command(id);
        self.entities[id].weapon_set.current = slot;
        self.entities[id].weapon_set.lock = Lock::Temporary;
        self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
        let position = [point[0], point[1], self.ground_height(point)];
        self.native_attack_goal_from(
            id,
            crate::skirmish::combat::attack::Goal::Position(position),
            i32::MAX.unsigned_abs(),
            crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
            false,
        );
        self.entities[id].weapon_set.weapons[slot].max_shots = None;
        self.finish_dozer_player_command(id);
        Ok(())
    }

    pub(in crate::skirmish) fn update_fire_position(&mut self, id: usize) {
        let position = self.entities[id].ai.command.goal_position;
        if !self.update_native_attack(id, super::super::combat::attack::Goal::Position(position)) {
            self.set_order(id, Order::Idle);
        }
    }

    pub(in crate::skirmish) fn fire_position_weapon(&mut self, id: usize, position: [f32; 3]) {
        let actor = &self.entities[id];
        let index = actor.weapon_index(actor.weapon_slot());
        let target = WeaponTarget {
            object: None,
            position,
            projectile_position: position,
            scatter_radius: 0.0,
        };
        if let Some(reloaded) = self.fire_current_weapon_payload(id, target) {
            self.notify_firing_tracker_shot(id, index, None);
            if reloaded && matches!(self.entities[id].weapon_set.lock, Lock::Temporary) {
                self.entities[id].weapon_set.lock = Lock::None;
            }
            self.entities[id].helpers.defection.undetected = false;
        }
    }
}
