//! Native `MobMemberSlavedUpdate`, SH 2bfaea60.
use crate::skirmish::{
    Entity, Locomotor, Skirmish, UnitType, combat::attack::Goal, locomotor::set, startup,
};
use num_traits::ToPrimitive;

pub struct MobRules {
    pub catch_up: i32,
    pub no_catch_up: i32,
    pub squirrelliness: f32,
    pub crisis_calls: u32,
    pub wander: Locomotor,
    pub wander_speed: f32,
    pub panic: Locomotor,
    pub panic_speed: f32,
}

pub(in crate::skirmish) struct MobState {
    pub(in crate::skirmish) parent: Option<usize>,
    wait: i32,
    primary_victim: Option<usize>,
    pub(in crate::skirmish) self_tasking: bool,
    squirrelliness: f32,
    crisis: u32,
}

impl Entity {
    #[must_use]
    pub fn spawn_master(&self) -> Option<usize> {
        self.behaviors.mobs.first().and_then(|state| state.parent)
    }

    pub(in crate::skirmish) fn mob_speed(&self, unit: &UnitType) -> Option<f32> {
        let rules = unit.behavior.mobs.first()?;
        match self.ai.locomotor_set {
            set::WANDER => Some(rules.wander_speed),
            set::PANIC => Some(rules.panic_speed),
            _ => None,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_mob_member(&mut self, id: usize, ordinal: usize) {
        let wait =
            i32::try_from(startup::random_between(&mut self.random, 20)).expect("native wait");
        // Personal tint is commented out in the source; these constructor draws remain.
        for _ in 0..3 {
            startup::random_real(&mut self.random, 0.2, 0.4);
        }
        assert_eq!(self.entities[id].behaviors.mobs.len(), ordinal);
        self.entities[id].behaviors.mobs.push(MobState {
            parent: None,
            wait,
            primary_victim: None,
            self_tasking: false,
            squirrelliness: 0.0,
            crisis: 0,
        });
    }

    pub(in crate::skirmish) fn created_mob_member(&mut self, id: usize, ordinal: usize) {
        self.entities[id].behaviors.mobs[ordinal].squirrelliness =
            self.catalog[self.entities[id].type_index].behavior.mobs[ordinal]
                .squirrelliness
                .clamp(0.0, 1.0);
    }

    pub(in crate::skirmish) fn enslave_mob_member(
        &mut self,
        id: usize,
        ordinal: usize,
        parent: usize,
    ) {
        self.entities[id].behaviors.mobs[ordinal].parent = Some(parent);
    }

    pub(in crate::skirmish) fn release_mob_member(&mut self, id: usize, ordinal: usize) {
        self.entities[id].behaviors.mobs[ordinal].parent = None;
        self.entities[id].maintenance.instance_flags &=
            !crate::skirmish::maintenance::INSTANCE_UNSELECTABLE;
        self.clear_disabled_held(id);
    }

    pub(in crate::skirmish) fn update_mob_member(&mut self, id: usize, ordinal: usize) -> i32 {
        let Some(parent) = self.entities[id].behaviors.mobs[ordinal]
            .parent
            .filter(|&id| self.entities[id].exists())
        else {
            self.release_mob_member(id, ordinal);
            self.kill_object(id, crate::skirmish::DeathType::Normal);
            return 1;
        };
        if self.entities[id].ai.registration.is_none()
            || self.entities[parent].ai.registration.is_none()
            || !self.entities[id].has_drawable()
            || !self.entities[parent].has_drawable()
        {
            return 1;
        }
        if self.entities[id].weapon_set_upgraded() {
            // Clear only Drawable's A flags, leaving ObjectSMHelper's last-condition cache intact.
            self.entities[id].weapon_set.status.condition[0] = None;
        }
        let state = &mut self.entities[id].behaviors.mobs[ordinal];
        state.wait = state.wait.wrapping_add(1);
        if state.wait < 16 {
            return 1;
        }
        state.wait = 0;
        let actor = &self.entities[id];
        if self.catalog[actor.type_index]
            .locomotor_in_set(actor.ai.locomotor_set, actor.ai.surface_locomotor)
            .is_none_or(|loco| loco.wander_length.is_none())
        {
            return 1;
        }
        let victim = self.entities[id]
            .ai
            .current_victim
            .filter(|&id| self.entities[id].exists());
        if let Some(victim) = self.entities[parent]
            .ai
            .current_victim
            .filter(|&id| self.entities[id].exists())
        {
            self.entities[id].behaviors.mobs[ordinal].primary_victim = Some(victim);
        }
        let primary = self.entities[id].behaviors.mobs[ordinal]
            .primary_victim
            .filter(|&id| self.entities[id].exists());
        // Both source queries occur before deciding whether a catch-up move is needed.
        let parent_distance = self.mob_path_distance(parent);
        let distance = self.mob_path_distance(id);
        let delta = distance3_squared(self.object_position(id), self.object_position(parent));
        let rules = &self.catalog[self.entities[id].type_index].behavior.mobs[ordinal];
        let catch_up = rules.catch_up;
        let crisis_calls = rules.crisis_calls;
        if delta > int_square(catch_up) {
            if self.entities[parent].ai.motion.is_moving() {
                self.choose_mob_locomotor(
                    id,
                    if parent_distance > distance {
                        set::WANDER
                    } else {
                        set::PANIC
                    },
                );
                let goal = self.entities[parent].ai_goal_position();
                if distance3_squared(goal, [0.0; 3]).sqrt() < 1.0 {
                    self.construction_move(id, self.object_position(parent));
                } else if distance3_squared(self.entities[id].ai_goal_position(), goal).sqrt()
                    > 5.0 * crate::skirmish::CELL_SIZE
                {
                    self.construction_move(id, goal);
                }
            } else {
                self.choose_mob_locomotor(id, set::PANIC);
                self.construction_move(id, self.object_position(parent));
            }
            if delta > int_square(catch_up.wrapping_mul(3)) {
                let state = &mut self.entities[id].behaviors.mobs[ordinal];
                state.crisis = state.crisis.wrapping_add(1);
                if state.crisis > crisis_calls {
                    self.kill_object(id, crate::skirmish::DeathType::Normal);
                    return 1;
                } else if state.crisis > crisis_calls / 3 {
                    self.construction_move(id, self.object_position(parent));
                }
            }
        } else if self.entities[id].ai.motion.is_moving() {
            self.entities[id].behaviors.mobs[ordinal].crisis = 0;
            match self.random_between(10) {
                1 => self.choose_mob_locomotor(id, set::WANDER),
                2 => self.choose_mob_locomotor(id, set::PANIC),
                3 => self.choose_mob_locomotor(id, set::NORMAL),
                _ => {}
            }
        } else {
            self.entities[id].behaviors.mobs[ordinal].crisis = 0;
            self.mob_idle(id, ordinal, parent, victim, primary);
        }
        1
    }

    fn choose_mob_locomotor(&mut self, id: usize, role: u8) {
        self.choose_locomotor_set(id, role);
    }

    fn mob_idle(
        &mut self,
        id: usize,
        ordinal: usize,
        parent: usize,
        mut victim: Option<usize>,
        primary: Option<usize>,
    ) {
        if self.entities[parent].behaviors.spawns.is_empty() {
            return;
        }
        if self.entities[parent].ai_idle() {
            self.ai_idle_from_ai(id);
            self.entities[id].behaviors.mobs[ordinal].primary_victim = None;
            return;
        }
        let ratio = self.entities[id].behaviors.mobs[ordinal].squirrelliness;
        if self.may_spawn_self_task(parent, ratio)
            && !self.entities[id].last_command_from_ai()
            && let Some(target) = self
                .mood_target(id, false, false)
                .filter(|target| Some(*target) != victim)
        {
            victim = Some(target);
            if self.native_ai_command_allowed(id) {
                self.ai_attack_goal(id, Goal::Object(target), 999);
            }
            self.entities[id].behaviors.mobs[ordinal].self_tasking = true;
        }
        if victim.is_none() {
            if let Some(target) = primary
                && self.native_ai_command_allowed(id)
            {
                self.ai_attack_goal(id, Goal::Object(target), 999);
            }
            self.entities[id].behaviors.mobs[ordinal].self_tasking = false;
        }
    }

    fn may_spawn_self_task(&self, id: usize, ratio: f32) -> bool {
        let Some(state) = self.entities[id].behaviors.spawns.first() else {
            return false;
        };
        if state.spawn_count == 0
            || ratio == 0.0
            || self.entities[id].ai.registration.is_none()
            || !self.entities[id].last_command_from_ai()
        {
            return false;
        }
        let count = state.spawn_count.to_f32().expect("native spawn count");
        let self_taskers = state.self_taskers.to_f32().expect("native self task count");
        self_taskers / count < ratio
    }

    fn mob_path_distance(&mut self, id: usize) -> f32 {
        if !self.entities[id].ai.motion.follows_path() {
            return 0.0;
        }
        let Some(mut path) = self.entities[id].ai.ground_path.take() else {
            return 0.0;
        };
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let position = self.object_position(id);
        let layer = actor.layer;
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let ignored = self.ignored_navigation(id);
        let grid = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let sample = path.sample_layer(position, layer, |layer, from, to| {
            crate::skirmish::ground_movement::line_passable_on(
                grid,
                |node| self.fixed_unit_blocks_on(id, node),
                radius,
                centered,
                from,
                to,
                layer,
            )
        });
        let distance = path.distance_to_goal(position, &sample);
        self.entities[id].ai.ground_path = Some(path);
        distance
    }

    pub(super) fn aggregate_spawn_health(&mut self, id: usize, ordinal: usize) {
        use num_traits::ToPrimitive;
        let children = self.entities[id].behaviors.spawns[ordinal].children.clone();
        let initial_level = self.entities[id].veterancy;
        let mut health = 0.0;
        let mut max_health = 0.0;
        let mut center = [0.0; 3];
        let mut count = 0_i32;
        let mut selected = false;
        let mut missing = false;
        let mut self_taskers = 0;
        for &child in &children {
            if !self.entities[child].exists() {
                continue;
            }
            self_taskers += i32::from(self.slave_is_self_tasking(child));
            let level = self.entities[child].veterancy;
            if level > initial_level {
                self.set_veterancy(id, level);
            } else if level < initial_level {
                self.set_veterancy(child, initial_level);
            }
            let position = self.object_position(child);
            for axis in 0..3 {
                center[axis] += position[axis];
            }
            health += self.entities[child].health;
            max_health += self.entities[child].max_health;
            if self.entities[child].drawable.selected != 0 {
                selected = true;
            } else {
                missing = true;
            }
            count += 1;
        }
        self.entities[id].behaviors.spawns[ordinal].self_taskers = self_taskers;
        if selected && (self.entities[id].drawable.selected == 0 || missing) {
            let mut added = Vec::new();
            if missing {
                for &child in &children {
                    if self.entities[child].exists() && self.entities[child].drawable.selected == 0
                    {
                        self.entities[child].drawable.selected = 1;
                        added.push(child);
                    }
                }
            }
            if self.entities[id].drawable.selected == 0 {
                self.entities[id].drawable.selected = 1;
                added.push(id);
            }
            self.spawn_selection_events.push(added);
        }
        let scale = 1.0 / count.to_f32().expect("native spawn count");
        let position = self.object_position(id);
        for axis in 0..3 {
            let average = center[axis] * scale;
            center[axis] = average - position[axis];
        }
        self.entities[id].health_box_offset = center;
        let percent = if count != 0 {
            let average = max_health / count.to_f32().expect("native spawn count");
            let optimal = average
                * self.catalog[self.entities[id].type_index].behavior.spawns[ordinal]
                    .count
                    .to_f32()
                    .expect("native spawn count");
            (100.0 * (health / optimal))
                .to_i32()
                .expect("native aggregate health percent")
        } else {
            0
        };
        let fraction = percent.to_f32().expect("native initial health percent") / 100.0;
        let requested = fraction * self.entities[id].damage_state.initial_health;
        self.change_body_health(id, requested - self.entities[id].health);
        self.entities[id].maintenance.masked = true;
    }
}

fn int_square(value: i32) -> f32 {
    value
        .wrapping_mul(value)
        .to_f32()
        .expect("native radius square")
}
fn distance3_squared(first: [f32; 3], second: [f32; 3]) -> f32 {
    let delta = std::array::from_fn::<_, 3, _>(|axis| first[axis] - second[axis]);
    let squares = delta.map(|value| value * value);
    (squares[0] + squares[1]) + squares[2]
}
#[cfg(test)]
mod tests;
