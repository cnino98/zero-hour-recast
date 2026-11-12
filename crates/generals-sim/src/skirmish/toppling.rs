//! SH `ToppleUpdate.cpp`: ordinary trees, streetlights and fences, not structure collapse.
use super::{DeathType, Entity, ModelConditions, Skirmish, module_updates::FOREVER};
use num_traits::ToPrimitive;
use std::f32::consts::{FRAC_PI_2, PI};

mod runtime;
#[cfg(test)]
mod tests;

pub struct Rules {
    pub topple_fx: String,
    pub bounce_fx: String,
    pub stump: Option<String>,
    pub initial_velocity: f32,
    pub initial_acceleration: f32,
    pub bounce_velocity: f32,
    pub kill_on_start: bool,
    pub left_or_right: bool,
    pub finish: FinishRules,
}

pub struct FinishRules {
    pub kill: bool,
    pub kill_stump: bool,
    pub reorient: bool,
}

pub(super) struct State {
    delta: f32,
    turns: i32,
    velocity: f32,
    acceleration: f32,
    accumulation: f32,
    direction: [f32; 3],
    phase: Phase,
    options: u32,
    stump: Option<usize>,
}

enum Phase {
    Upright,
    Falling,
    Down,
}

const LIMIT: f32 = PI / 2.0 - PI / 64.0;

impl Skirmish {
    pub(super) fn construct_toppling(&mut self, id: usize, ordinal: usize) {
        let states = &mut self.entities[id].behaviors.toppling;
        assert_eq!(states.len(), ordinal);
        states.push(State {
            delta: 0.0,
            turns: 0,
            velocity: 0.0,
            acceleration: 0.0,
            accumulation: 0.0,
            direction: [0.0; 3],
            phase: Phase::Upright,
            options: 0,
            stump: None,
        });
    }

    pub(in crate::skirmish) fn topple_contact(&mut self, id: usize, other: usize) {
        if self.catalog[self.entities[other].type_index]
            .crush
            .crusher_level
            <= 1
        {
            return;
        }
        let our = self.object_position(id);
        let their = self.object_position(other);
        let velocity = if self.object_physics_mass(other).is_some() {
            self.entities[other].physics_velocity()
        } else {
            [0.0; 3]
        };
        let squares = velocity.map(|component| component * component);
        let speed = ((squares[0] + squares[1]) + squares[2]).sqrt();
        self.topple_object(id, [our[0] - their[0], our[1] - their[1], 0.0], speed, 0);
    }

    /// `Object::topple` uses its first `ToppleUpdate`, even when later modules are upright.
    pub(in crate::skirmish) fn topple_object(
        &mut self,
        id: usize,
        direction: [f32; 3],
        speed: f32,
        options: u32,
    ) {
        if self.entities[id]
            .behaviors
            .toppling
            .first()
            .is_none_or(|state| !matches!(state.phase, Phase::Upright))
            || self.entities[id].is_effectively_dead()
        {
            return;
        }
        self.wake_family_update(id, "ToppleUpdate", 0, 1);
        let rules = &self.catalog[self.entities[id].type_index].behavior.toppling[0];
        if rules.kill_on_start {
            self.wake_family_update(id, "ToppleUpdate", 0, FOREVER);
            self.kill_object(id, DeathType::Normal);
            return;
        }
        let velocity = speed * rules.initial_velocity;
        let acceleration = speed * rules.initial_acceleration;
        let left_or_right = rules.left_or_right;
        let direction = self.adjust_topple_direction(id, normalize(direction));
        let state = &mut self.entities[id].behaviors.toppling[0];
        state.direction = direction;
        state.velocity = velocity;
        state.acceleration = acceleration;
        state.phase = Phase::Falling;
        state.options = options;
        self.align_toppling(id, left_or_right);
        self.entities[id]
            .drawable
            .model_conditions
            .set_flag(ModelConditions::TOPPLED, true);
        let effect = self.catalog[self.entities[id].type_index].behavior.toppling[0]
            .topple_fx
            .clone();
        self.object_fx(id, effect);
        self.create_topple_stump(id);
    }

    fn align_toppling(&mut self, id: usize, left_or_right: bool) {
        let current = super::locomotor::normalize_angle(self.entities[id].facing);
        let state = &mut self.entities[id].behaviors.toppling[0];
        let mut angle =
            super::locomotor::normalize_angle(state.direction[1].atan2(state.direction[0]));
        if left_or_right {
            angle = closest(current + FRAC_PI_2, current - FRAC_PI_2, angle);
            state.direction[0] = angle.cos();
            state.direction[1] = angle.sin();
        }
        let desired = closest(angle + FRAC_PI_2, angle - FRAC_PI_2, current);
        // Native float-to-Int invalid conversion yields the indefinite integer;
        // the following source clamp makes zero-speed and overflow use one turn.
        let turns = crate::native_real::floor(LIMIT / (state.velocity * 2.0)).max(1);
        state.turns = turns;
        state.delta = (desired - current) / turns.to_f32().expect("native Int angle count");
        if left_or_right {
            self.unregister_object_obstacle(id);
        }
    }

    fn create_topple_stump(&mut self, id: usize) {
        let Some(name) = &self.catalog[self.entities[id].type_index].behavior.toppling[0].stump
        else {
            return;
        };
        let kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == *name)
            .expect("resolved Topple stump template");
        let stump = self.new_object(kind, self.neutral_owner(), true);
        self.set_object_position(stump, self.object_position(id));
        self.set_object_orientation(stump, self.entities[id].facing);
        self.entities[id].behaviors.toppling[0].stump = Some(stump);
        if self.entities[id].drawable.present
            && self.entities[id]
                .drawable
                .model_conditions
                .has_flag(ModelConditions::BURNED)
            && self.entities[stump].drawable.present
        {
            self.entities[stump]
                .drawable
                .model_conditions
                .set_flag(ModelConditions::BURNED, true);
        }
    }
}

impl Entity {
    /// Native `stopSway` latches when `ToppleUpdate` starts falling, retaining its last matrix.
    #[must_use]
    pub fn topple_stopped_sway(&self) -> bool {
        self.behaviors
            .toppling
            .iter()
            .any(|state| !matches!(state.phase, Phase::Upright))
    }
}

fn closest(first: f32, second: f32, desired: f32) -> f32 {
    let first = super::locomotor::normalize_angle(first);
    let second = super::locomotor::normalize_angle(second);
    if super::locomotor::normalize_angle(desired - first).abs()
        < super::locomotor::normalize_angle(desired - second).abs()
    {
        first
    } else {
        second
    }
}

pub(super) fn normalize(value: [f32; 3]) -> [f32; 3] {
    let squares = value.map(|component| component * component);
    let length = ((squares[0] + squares[1]) + squares[2]).sqrt();
    if length == 0.0 {
        value
    } else {
        value.map(|component| component / length)
    }
}
