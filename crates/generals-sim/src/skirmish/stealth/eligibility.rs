use super::{
    ATTACKING, FIRING_PRIMARY, FIRING_SECONDARY, FIRING_TERTIARY, MOVING, NO_BLACK_MARKET,
    RIDERS_ATTACKING, Skirmish, TAKING_DAMAGE, USING_ABILITY,
};

impl Skirmish {
    pub(super) fn allowed_to_stealth(&mut self, id: usize) -> bool {
        if !self.stealth_slaves_allowed(id) {
            return false;
        }
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .behavior
            .stealth
            .as_ref()
            .expect("stealth module");
        let owner = if rules.use_rider {
            self.passengers(id).first().copied().unwrap_or(id)
        } else {
            id
        };
        let owner_rules = self.catalog[self.entities[owner].type_index]
            .behavior
            .stealth
            .as_ref()
            .unwrap_or(rules);
        let flags = owner_rules.forbidden;
        if flags & ATTACKING != 0 && actor.attacking()
            || flags & USING_ABILITY != 0 && self.stealth_status(id, "USING_ABILITY")
        {
            return false;
        }
        if flags & NO_BLACK_MARKET != 0 && !self.stealth_black_market(id) {
            return false;
        }
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .behavior
            .stealth
            .as_ref()
            .expect("stealth module");
        if self.entities[owner]
            .stealth
            .as_ref()
            .is_none_or(|state| !state.has(super::CAN_STEALTH))
            || flags & TAKING_DAMAGE != 0
                && actor.damage_state.retained_frame != u32::MAX
                && actor.damage_state.retained_frame >= self.update_frame().wrapping_sub(1)
            || !rules
                .required_status
                .iter()
                .all(|status| self.stealth_status(id, status))
            || rules
                .forbidden_status
                .iter()
                .any(|status| self.stealth_status(id, status))
        {
            return false;
        }
        let firing = FIRING_PRIMARY | FIRING_SECONDARY | FIRING_TERTIARY;
        if flags & firing != 0 && actor.attacking() {
            if flags & firing == firing {
                return false;
            }
            for (slot, flag) in [FIRING_PRIMARY, FIRING_SECONDARY, FIRING_TERTIARY]
                .into_iter()
                .enumerate()
            {
                if flags & flag != 0
                    && self.allocated_weapon(id, slot).is_some()
                    && super::super::ai_dispatch::native_frame(
                        actor.weapon_set.weapons[slot].last_shot.unwrap_or(0),
                    ) >= self.update_frame().wrapping_sub(1)
                {
                    return false;
                }
            }
        }
        if actor.contained_by().is_some_and(|carrier| {
            self.catalog[self.entities[carrier].type_index]
                .contain
                .as_ref()
                .is_none_or(|contain| contain.garrison.is_none())
        }) {
            return false;
        }
        if flags & RIDERS_ATTACKING != 0
            && self.catalog[actor.type_index]
                .contain
                .as_ref()
                .is_some_and(|contain| contain.weapons.fire)
            && self
                .passengers(id)
                .iter()
                .any(|rider| self.entities[*rider].attacking())
        {
            return false;
        }
        // Physics stores units per logic frame, as does parseVelocityReal.
        let squares = actor.physics_velocity().map(|axis| axis * axis);
        let speed = ((squares[0] + squares[1]) + squares[2]).sqrt();
        flags & MOVING == 0 || speed <= rules.threshold_speed
    }

    fn stealth_black_market(&mut self, id: usize) -> bool {
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .behavior
            .stealth
            .as_ref()
            .expect("stealth module");
        if actor
            .stealth
            .as_ref()
            .expect("stealth module")
            .next_black_market
            < self.tick
        {
            let duration = rules.black_market_frames;
            let jitter = self.random_between(10);
            self.entities[id]
                .stealth
                .as_mut()
                .expect("stealth module")
                .next_black_market += u64::from(duration + jitter);
            if !self.entities.iter().any(|other| {
                other.owner == self.entities[id].owner
                    && other.complete()
                    && !other.selling()
                    && self.catalog[other.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "FS_BLACK_MARKET")
            }) {
                return false;
            }
        }
        true
    }

    fn stealth_slaves_allowed(&mut self, id: usize) -> bool {
        if !self.catalog[self.entities[id].type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "SPAWNS_ARE_THE_WEAPONS")
        {
            return true;
        }
        let children = self.spawned_children(id).to_vec();
        let allowed = children
            .iter()
            .copied()
            .all(|child| self.entities[child].stealth.is_some() && self.allowed_to_stealth(child));
        if !allowed {
            for child in children {
                self.mark_detected(child, 0);
            }
        }
        allowed
    }

    pub(super) fn stealth_status(&self, id: usize, status: &str) -> bool {
        let actor = &self.entities[id];
        match status {
            "CAN_STEALTH" => actor
                .stealth
                .as_ref()
                .is_some_and(|state| state.has(super::CAN_STEALTH)),
            "STEALTHED" => actor.stealthed(),
            "DETECTED" => actor.detected(),
            "IS_FIRING_WEAPON" => actor.attacking(),
            "USING_ABILITY" | "IS_USING_ABILITY" => actor.object_status & (1 << 24) != 0,
            "UNDER_CONSTRUCTION" => !actor.complete(),
            "SOLD" => actor.selling(),
            _ => false,
        }
    }
}
