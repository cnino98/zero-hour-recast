//! `StructureToppleUpdate.cpp` 134–457: authoritative rotation and crushing.
use super::{Module, Skirmish, VisualAction, physical_ocl, startup};
use crate::skirmish::DamageType;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_6, PI};

mod geometry;

enum Stage {
    Standing,
    Waiting,
    Falling,
    Done,
}

pub(in crate::skirmish) struct State {
    stage: Stage,
    topple_frame: u64,
    angle: f32,
    velocity: f32,
    integrity: f32,
    direction: [f32; 2],
    burst_position: [f32; 3],
    basis: [[f32; 3]; 3],
    live_height: f32,
    last_crushed: f32,
}

impl State {
    pub(super) const fn set_basis(&mut self, basis: [[f32; 3]; 3]) {
        self.basis = basis;
    }

    pub const fn new(live_height: f32) -> Self {
        Self {
            stage: Stage::Standing,
            topple_frame: 0,
            angle: 0.001,
            velocity: 0.0,
            integrity: 0.0,
            direction: [0.0; 2],
            burst_position: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            live_height,
            last_crushed: 0.0,
        }
    }
    pub const fn basis(&self) -> Option<[[f32; 3]; 3]> {
        if matches!(self.stage, Stage::Standing) {
            None
        } else {
            Some(self.basis)
        }
    }
    pub const fn sleeping(&self) -> bool {
        matches!(self.stage, Stage::Standing | Stage::Done)
    }
}

impl Skirmish {
    pub(super) fn begin_topple(&mut self, id: usize, state: &mut State) {
        let rules = self.topple_rules(id);
        let delay = rules.delay;
        let burst_delay = rules.burst_delay;
        let start_effect = rules.start_effect.clone();
        state.topple_frame = self.tick + u64::from(super::random_delay(&mut self.random, delay));
        let actor = &self.entities[id];
        let angle = if let Some(attacker) = actor
            .damage_state
            .current_source
            .and_then(|index| self.entities.get(index))
            .filter(|actor| actor.exists())
        {
            let offset = [
                actor.position[0] - attacker.position[0],
                actor.position[1] - attacker.position[1],
            ];
            offset[1].atan2(offset[0]) + startup::random_real(&mut self.random, -PI / 8.0, PI / 8.0)
        } else {
            startup::random_angle(&mut self.random)
        };
        let geometry = &self.catalog[actor.type_index].geometry;
        let diameter_sum = geometry.major + geometry.minor;
        let radius = (diameter_sum / 2.0) * 0.90;
        let direction = self.adjust_topple_direction(id, [angle.cos(), angle.sin(), 0.0]);
        state.direction = [direction[0], direction[1]];
        state.basis = actor.object_basis();
        let offset = [radius * angle.cos(), radius * angle.sin()];
        let at = [actor.position[0] + offset[0], actor.position[1] + offset[1]];
        state.burst_position = [at[0], at[1], self.ground_height(at)];
        if self.topple_effects_enabled(id) {
            self.structure_position_fx(id, start_effect, self.object_position(id));
        }
        self.structure_phase(id, 0, self.object_position(id));
        self.structure_visual(id, VisualAction::ToppleBegin { burst_delay }, None);
        state.stage = Stage::Waiting;
    }

    pub(super) fn update_topple(&mut self, id: usize, state: &mut State) {
        if matches!(state.stage, Stage::Standing | Stage::Done) {
            return;
        }
        if matches!(state.stage, Stage::Waiting) {
            self.topple_burst_check(id, state);
            if self.tick >= state.topple_frame {
                state.stage = Stage::Falling;
                state.integrity = self.topple_rules(id).integrity;
            }
        }
        if matches!(state.stage, Stage::Falling) {
            self.advance_topple(id, state);
        }
    }

    fn advance_topple(&mut self, id: usize, state: &mut State) {
        let acceleration = 0.02 * (state.angle.sin() * (1.0 - state.integrity));
        state.velocity += acceleration;
        if state.integrity > 0.0 {
            state.integrity *= self.topple_rules(id).decay;
            if state.integrity < 0.0 {
                state.integrity = 0.0;
            }
        }
        let effects: Vec<_> = self
            .topple_rules(id)
            .angles
            .iter()
            .filter(|(angle, _)| *angle > state.angle && *angle <= state.angle + state.velocity)
            .map(|(_, effect)| effect.clone())
            .collect();
        if self.topple_effects_enabled(id) {
            for effect in effects {
                self.object_fx_at(id, effect, self.object_position(id), state.basis);
            }
        }
        state.angle += state.velocity;
        self.topple_crush(id, state, FRAC_PI_2 - state.angle);
        let done = state.angle >= FRAC_PI_2;
        if done {
            state.velocity -= state.angle - FRAC_PI_2;
            state.angle = FRAC_PI_2;
            self.topple_crush(id, state, 0.0);
            self.structure_phase(id, 3, self.object_position(id));
            if self.topple_effects_enabled(id) {
                let effect = self.topple_rules(id).done_effect.clone();
                self.object_fx_at(id, effect, self.object_position(id), state.basis);
            }
        }
        self.topple_burst_check(id, state);
        geometry::pre_rotate(&mut state.basis, 0, -state.velocity * state.direction[1]);
        geometry::pre_rotate(&mut state.basis, 1, state.velocity * state.direction[0]);
        self.set_object_transform(id, self.object_position(id), state.basis);
        if done {
            // Native resets pitch/roll, then rotates the upright POST_COLLAPSE
            // model from its current yaw to the fall direction in the same tick.
            self.set_post_collapse(id);
            self.update_body_particles(id);
            self.stop_first_bone_fx(id);
            let angle = self.entities[id].facing;
            self.set_object_orientation(id, angle);
            state.basis = self.entities[id].object_basis();
            let direction = state.direction[1].atan2(state.direction[0]);
            geometry::pre_rotate(&mut state.basis, 2, direction - angle);
            self.set_object_transform(id, self.object_position(id), state.basis);
            state.stage = Stage::Done;
            self.structure_visual(id, VisualAction::Clear, None);
        }
    }

    fn topple_rules(&self, id: usize) -> &super::ToppleRules {
        let rules = self.catalog[self.entities[id].type_index]
            .structure_death
            .as_ref()
            .expect("topple rules");
        let Module::Topple(rules) = &rules.module else {
            unreachable!("topple module")
        };
        rules
    }

    fn topple_effects_enabled(&self, id: usize) -> bool {
        !self.topple_rules(id).suppress_water_fx
            || !matches!(
                self.entities[id].damage_state.damage_type,
                DamageType::Water
            )
    }

    fn topple_burst_check(&mut self, id: usize, state: &State) {
        let rules = self.topple_rules(id);
        let burst_delay = rules.burst_delay;
        let effect = self.topple_effects_enabled(id).then(|| {
            self.snapshot_object_fx(
                id,
                rules.delay_effect.clone(),
                state.burst_position,
                physical_ocl::transform::yaw(0.0),
            )
        });
        self.structure_visual(id, VisualAction::ToppleCheck { burst_delay }, effect);
    }

    fn topple_crush(&mut self, id: usize, state: &mut State, theta: f32) {
        if theta > FRAC_PI_6 {
            return;
        }
        let direction = state.direction[1].atan2(state.direction[0]);
        let angle = self.entities[id].facing - direction;
        let geometry = &self.catalog[self.entities[id].type_index].geometry;
        let minor = geometry.minor * angle.cos();
        let major = geometry.major * angle.sin();
        let major_squared = major * major;
        let minor_squared = minor * minor;
        let width = (major_squared + minor_squared + 0.0).sqrt() / 2.0;
        let maximum = state.live_height * (1.0 - theta.sin());
        let mut distance = state.last_crushed;
        while distance.partial_cmp(&maximum) == Some(std::cmp::Ordering::Less) {
            self.topple_damage_line(id, direction, distance, width);
            distance += 25.0;
        }
        self.topple_damage_line(id, direction, maximum, width);
        state.last_crushed = distance;
    }

    fn topple_damage_line(&mut self, id: usize, direction: f32, distance: f32, width: f32) {
        let unit = &self.catalog[self.entities[id].type_index];
        let index = unit.slow_weapon_start() + self.topple_rules(id).crushing_weapon;
        let effect = self.topple_rules(id).crushing_effect.clone();
        let at = self.entities[id].position;
        let offset = [distance * direction.cos(), distance * direction.sin()];
        let base = [at[0] + offset[0], at[1] + offset[1]];
        let mut lateral = -width;
        while lateral.partial_cmp(&width) == Some(std::cmp::Ordering::Less) {
            self.topple_damage_point(
                id,
                index,
                &effect,
                geometry::crushing_point(base, direction, lateral),
            );
            lateral += 25.0;
        }
        self.topple_damage_point(
            id,
            index,
            &effect,
            geometry::crushing_point(base, direction, width),
        );
        self.structure_phase(id, 3, [base[0], base[1], self.ground_height(base)]);
    }

    fn topple_damage_point(&mut self, id: usize, index: usize, effect: &str, at: [f32; 2]) {
        let position = [at[0], at[1], self.ground_height(at)];
        self.fire_temporary_weapon_at(id, index, position);
        if self.topple_effects_enabled(id) {
            self.structure_position_fx(id, effect.into(), position);
        }
    }
}

#[cfg(test)]
mod tests;
