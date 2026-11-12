//! SH MinefieldBehavior.cpp: scooting, immunity, virtual mines and body callbacks.
use super::{Skirmish, unsigned_real};
use crate::skirmish::{DamageType, DeathType};

impl Skirmish {
    pub(in crate::skirmish) fn update_land_mine(&mut self, id: usize, ordinal: usize) -> i32 {
        if self.entities[id].mines.mines[ordinal].scoot_frames > 0 {
            self.move_scooting_mine(id, ordinal);
        }
        let now = self.update_frame();
        for index in 0..3 {
            let (immune, frame) = self.entities[id].mines.mines[ordinal].immunes[index];
            if let Some(immune) = immune
                && (!self.entities[immune].exists() || now > frame.wrapping_add(2))
            {
                self.entities[id].mines.mines[ordinal].immunes[index] = (None, 0);
            }
        }
        let state = &self.entities[id].mines.mines[ordinal];
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .minefields[ordinal];
        if now >= state.next_death_check && state.regenerates && rules.stop_regeneration {
            self.entities[id].mines.mines[ordinal].next_death_check =
                now.wrapping_add(rules.creator_check_frames);
            if self.entities[id].producer.is_some_and(|producer| {
                !self.entities[producer].exists() || self.entities[producer].is_effectively_dead()
            }) {
                let state = &mut self.entities[id].mines.mines[ordinal];
                state.regenerates = false;
                state.draining = true;
                self.stop_first_auto_heal(id);
            }
        }
        if self.entities[id].mines.mines[ordinal].draining {
            let rules = &self.catalog[self.entities[id].type_index]
                .behavior
                .minefields[ordinal];
            let amount = (self.entities[id].max_health * rules.drain_percent) / 30.0;
            self.apply_native_damage(id, Some(id), amount, DeathType::Normal);
        }
        i32::from_ne_bytes(self.mine_sleep(id, ordinal).to_ne_bytes())
    }

    fn move_scooting_mine(&mut self, id: usize, ordinal: usize) {
        let mut point = self.object_position(id);
        let state = &mut self.entities[id].mines.mines[ordinal];
        for (axis, value) in point.iter_mut().enumerate() {
            state.velocity[axis] += state.acceleration[axis];
            *value += state.velocity[axis];
        }
        let last = state.scoot_frames <= 1;
        let ground = self.ground_height([point[0], point[1]]);
        let layer = self.movement_terrain.get_highest_layer_for_destination(
            [point[0], point[1], 99999.0],
            ground,
            false,
        );
        self.entities[id].layer = layer;
        let mut ground =
            self.movement_terrain
                .destination_layer_height(layer, [point[0], point[1]], ground);
        if layer != 1 {
            ground += 1.0;
        }
        if point[2] < ground || last {
            point[2] = ground;
        }
        self.set_object_position(id, point);
        self.entities[id].mines.mines[ordinal].scoot_frames -= 1;
    }

    pub(in crate::skirmish) fn set_mine_scoot(
        &mut self,
        id: usize,
        start: [f32; 3],
        mut end: [f32; 3],
    ) {
        if self.entities[id].mines.mines.is_empty() {
            return;
        }
        let mut frames = self.catalog[self.entities[id].type_index]
            .behavior
            .minefields[0]
            .scoot_frames;
        end[2] = self.ground_height([end[0], end[1]]);
        if start[2] > end[2] {
            let time = (2.0 * (start[2] - end[2]) / self.gravity.abs()).sqrt();
            let falling = u32::from_ne_bytes(super::native_ceil(time).to_ne_bytes());
            frames = frames.max(falling);
        }
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let dz = end[2] - start[2];
        let squared = [dx * dx, dy * dy];
        let distance = (squared[0] + squared[1]).sqrt();
        if frames == 0 || distance <= 0.1 && dz.abs() <= 0.1 {
            self.set_object_position(id, end);
            self.entities[id].mines.mines[0].scoot_frames = 0;
            return;
        }
        let time = unsigned_real(frames);
        let speed = distance / time;
        let travel = speed * time;
        let acceleration = (2.0 * (distance - travel) / (time * time)).abs();
        let nx = if distance <= 0.1 { 0.0 } else { dx / distance };
        let ny = if distance <= 0.1 { 0.0 } else { dy / distance };
        let state = &mut self.entities[id].mines.mines[0];
        state.velocity[0] = nx * speed;
        state.velocity[1] = ny * speed;
        // Repeated setScootParms retains its current vertical velocity.
        state.acceleration = [-nx * acceleration, -ny * acceleration, self.gravity];
        self.set_object_position(id, start);
        self.entities[id].mines.mines[0].scoot_frames = frames;
        self.wake_family_update(id, "MinefieldBehavior", 0, self.mine_sleep(id, 0));
    }

    pub(in crate::skirmish) fn land_mine_contact(
        &mut self,
        id: usize,
        ordinal: usize,
        other: usize,
    ) {
        if self.entities[other].is_effectively_dead()
            || self.entities[id].mines.mines[ordinal].remaining == 0
        {
            return;
        }
        let now = self.update_frame();
        for immune in &mut self.entities[id].mines.mines[ordinal].immunes {
            if immune.0 == Some(other) {
                immune.1 = now;
                return;
            }
        }
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .minefields[ordinal];
        if !rules.workers_detonate
            && self.attack_kind(other, "INFANTRY")
            && self.attack_kind(other, "DOZER")
        {
            return;
        }
        let relation = self.object_relationship(id, other);
        if rules.detonated_by & (1 << relation) == 0
            || self.entities[id].mines.mines[ordinal].scoot_frames > 0
        {
            return;
        }
        let actor = &self.entities[other];
        if actor.ai.registration.is_some()
            && actor.weapon_set.mine_clearing
            && actor
                .ai
                .command
                .goal_object
                .is_some_and(|goal| self.entities[goal].exists())
        {
            if let Some(slot) = self.entities[id].mines.mines[ordinal]
                .immunes
                .iter_mut()
                .find(|slot| slot.0.is_none() || slot.0 == Some(other))
            {
                *slot = (Some(other), now);
                self.wake_family_update(
                    id,
                    "MinefieldBehavior",
                    ordinal,
                    self.mine_sleep(id, ordinal),
                );
            }
            return;
        }
        let point = self.object_position(other);
        let threshold = rules.repeat_move_threshold * rules.repeat_move_threshold;
        let detonators = &mut self.entities[id].mines.mines[ordinal].detonators;
        if let Some((_, prior)) = detonators.iter_mut().find(|(target, _)| *target == other) {
            let delta = std::array::from_fn::<_, 3, _>(|axis| point[axis] - prior[axis]);
            let squares = delta.map(|value| value * value);
            if squares[0] + squares[1] + squares[2] <= threshold {
                return;
            }
            *prior = point;
        } else {
            detonators.push((other, point));
        }
        let point =
            super::clip_to_footprint(self.body_geometry(id), self.object_position(id), point);
        self.detonate_land_mine(id, ordinal, point);
    }

    fn detonate_land_mine(&mut self, id: usize, ordinal: usize, point: [f32; 3]) {
        let unit = &self.catalog[self.entities[id].type_index];
        if unit.behavior.minefields[ordinal].weapon.is_some() {
            self.fire_temporary_weapon_at(id, unit.mine_weapon_start() + ordinal, point);
        }
        let state = &mut self.entities[id].mines.mines[ordinal];
        if state.remaining > 0 {
            state.remaining -= 1;
        }
        if !state.regenerates && state.remaining == 0 {
            self.destroy_maintenance_object(id);
        } else {
            let rules = &self.catalog[self.entities[id].type_index]
                .behavior
                .minefields[ordinal];
            let percent = unsigned_real(self.entities[id].mines.mines[ordinal].remaining)
                / unsigned_real(rules.virtual_mines);
            let mut desired = percent * self.entities[id].max_health;
            if desired < 0.1 {
                desired = 0.1;
            }
            let amount = self.entities[id].health - desired;
            if amount > 0.0 {
                self.damage_mine_without_detonation(id, ordinal, amount);
            }
        }
        self.refresh_mine_mask(id, ordinal);
        if let Some(list) = self.catalog[self.entities[id].type_index]
            .behavior
            .minefields[ordinal]
            .creation
            .clone()
        {
            self.create_generic_objects(id, Some(id), &list);
        }
    }

    pub(in crate::skirmish) fn mine_body_changed(
        &mut self,
        id: usize,
        ordinal: usize,
        source: Option<usize>,
        damage: DamageType,
    ) {
        if self.entities[id].mines.mines[ordinal].ignore_damage {
            return;
        }
        loop {
            let actor = &self.entities[id];
            let count = self.catalog[actor.type_index].behavior.minefields[ordinal].virtual_mines;
            let real = unsigned_real(count) * actor.health / actor.max_health;
            let expected = if matches!(damage, DamageType::Healing) {
                super::native_floor(real)
            } else {
                super::native_ceil(real)
            };
            // Native comparisons promote the signed result to UnsignedInt.
            let expected = u32::from_ne_bytes(expected.to_ne_bytes()).min(count);
            let state = &mut self.entities[id].mines.mines[ordinal];
            match state.remaining.cmp(&expected) {
                std::cmp::Ordering::Less => state.remaining = expected,
                std::cmp::Ordering::Greater => {
                    if state.draining
                        && source == Some(id)
                        && matches!(damage, DamageType::Unresistable)
                    {
                        state.remaining -= 1;
                    } else {
                        self.detonate_land_mine(id, ordinal, self.object_position(id));
                    }
                }
                std::cmp::Ordering::Equal => break,
            }
        }
        let state = &self.entities[id].mines.mines[ordinal];
        if state.remaining == 0 && state.regenerates && self.entities[id].health < 0.1 {
            self.change_body_health(id, 0.1 - self.entities[id].health);
        }
        self.refresh_mine_mask(id, ordinal);
    }

    pub(in crate::skirmish) fn disarm_native_land_mine(&mut self, id: usize) -> bool {
        let Some(state) = self.entities[id].mines.mines.first() else {
            return false;
        };
        if !state.regenerates {
            self.destroy_maintenance_object(id);
            return true;
        }
        self.damage_mine_without_detonation(id, 0, self.entities[id].health - 0.1);
        self.entities[id].mines.mines[0].remaining = 0;
        self.refresh_mine_mask(id, 0);
        true
    }

    fn damage_mine_without_detonation(&mut self, id: usize, ordinal: usize, amount: f32) {
        self.entities[id].mines.mines[ordinal].ignore_damage = true;
        self.apply_native_damage(id, Some(id), amount, DeathType::None);
        self.entities[id].mines.mines[ordinal].ignore_damage = false;
    }
}
