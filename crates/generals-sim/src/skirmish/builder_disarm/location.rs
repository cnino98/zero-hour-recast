//! `MSG_SET_MINE_CLEARING_DETAIL` and `AIUpdate::privateAttackPosition`.
use super::{DamageType, Order, Skirmish, Source};
use crate::skirmish::weapons::Lock;

impl Skirmish {
    fn mine_clearing_position(&mut self, id: usize, point: [f32; 2]) -> [f32; 2] {
        let extra = self.passenger_bonus_flags(id);
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let distance = (super::super::movement::distance_squared(actor.position, point).sqrt()
            - self.object_radius(id))
        .max(0.0);
        let weapon = actor.active_weapon(unit);
        if !weapon.contact()
            || (distance >= weapon.min_range && distance <= actor.weapon_range(unit, extra))
            || self.mine_clearing_path(id, point)
        {
            return point;
        }
        // Fixed-CRC privateAttackPosition uses the default five-unit probe,
        // minRadius=0/maxRadius=100, then the quick zone query (not a full A*).
        self.find_position_around(point, |game, candidate, angle| {
            game.clear_approach_geometry(id, candidate, angle, None, None)
                && game.mine_clearing_path(id, candidate)
        })
    }

    fn mine_clearing_path(&self, id: usize, point: [f32; 2]) -> bool {
        let (Some(start), Some(goal)) = (
            self.cell_at(self.entities[id].position),
            self.cell_at(point),
        ) else {
            return false;
        };
        let actor = &self.entities[id];
        self.navigation_for(id).quick_path_exists(
            actor.locomotor_surfaces(&self.catalog[actor.type_index]),
            start,
            goal,
        )
    }

    #[must_use]
    pub fn mine_clearing_ready(&mut self, id: usize, slot: usize) -> bool {
        self.mine_clearing_command_ready(id, slot)
    }

    pub(in crate::skirmish) fn mine_clearing_command_ready(
        &mut self,
        id: usize,
        slot: usize,
    ) -> bool {
        let Ok(actor) = self.ready_actor(id) else {
            return false;
        };
        if actor.hacked() || actor.paralyzed() || slot >= 3 {
            return false;
        }
        let unit = &self.catalog[actor.type_index];
        if !unit.weapon_set_rules.templates.iter().any(|weapon| {
            weapon.slot == slot
                && weapon.flags & 16 != 0
                && matches!(weapon.rules.damage_type, DamageType::Disarm)
        }) {
            return false;
        }
        if self.allocated_weapon(id, slot).is_none() {
            return !actor.weapon_set.mine_clearing;
        }
        self.weapon_button_ready(id, slot)
    }

    /// Installs the original conditional set before the targeting cursor opens.
    /// # Errors
    /// Rejects actors without the represented mine-clearing command or ready weapon.
    pub fn select_mine_clearing(&mut self, id: usize, slot: usize) -> Result<(), &'static str> {
        if !self.mine_clearing_command_ready(id, slot) {
            return Err("Mine-clearing weapon is unavailable");
        }
        self.set_mine_clearing(id, true);
        Ok(())
    }

    /// Targets the nearest legal object around the click, ignoring its stealth.
    /// With no object the native command performs one harmless ground shot.
    /// # Errors
    /// Rejects missing DISARM sets, disabled actors and points outside the map.
    pub fn clear_mines_at(
        &mut self,
        id: usize,
        slot: usize,
        point: [f32; 2],
    ) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        if actor.hacked()
            || actor.paralyzed()
            || slot >= 3
            || self.cell_at(point).is_none()
            || !actor.weapon_set.mine_clearing
        {
            return Err("Choose a mine-clearing weapon and a point on the map");
        }
        let range = self
            .allocated_weapon(id, slot)
            .filter(|weapon| matches!(weapon.damage_type, DamageType::Disarm))
            .ok_or("Choose an allocated mine-clearing weapon")?
            .continue_attack_range;
        self.entities[id].weapon_set.current = slot;
        self.entities[id].weapon_set.lock = Lock::Temporary;
        self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
        self.entities[id].ai.attack_context.ignoring_stealth = true;
        let target = self.closest_disarm_target(id, point, range, None, Source::Player);
        self.entities[id].ai.attack_context.ignoring_stealth = false;
        if let Some(target) = target {
            self.set_order(id, Order::Attack(target));
            self.entities[id].weapon_set.current = slot;
            self.entities[id].weapon_set.lock = Lock::Temporary;
        } else {
            let point = self.mine_clearing_position(id, point);
            let weapon = self.entities[id].weapon_index(slot);
            self.set_order(
                id,
                Order::FireAt {
                    position: point,
                    weapon,
                },
            );
            let actor = &mut self.entities[id];
            actor.weapon_set.current = slot;
            actor.weapon_set.lock = Lock::Temporary;
            actor.weapon_set.weapons[slot].max_shots = Some(1);
        }
        Ok(())
    }
}
