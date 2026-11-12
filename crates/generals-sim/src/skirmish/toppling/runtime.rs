//! Native falling integration, bounce and completion damage ordering.
use super::{DeathType, FOREVER, LIMIT, Phase, Skirmish};
use crate::skirmish::physical_ocl::transform::{pre_rotate, vector};

impl Skirmish {
    pub(in crate::skirmish) fn update_toppling(&mut self, id: usize, ordinal: usize) -> i32 {
        if !matches!(
            self.entities[id].behaviors.toppling[ordinal].phase,
            Phase::Falling
        ) {
            return i32::try_from(FOREVER).expect("native forever");
        }
        let state = &self.entities[id].behaviors.toppling[ordinal];
        if state.turns != 0 {
            let angle = state.delta;
            let (position, mut basis) = self.object_transform(id);
            pre_rotate(&mut basis, 2, angle);
            self.set_object_transform(id, position, basis);
            self.entities[id].behaviors.toppling[ordinal].turns -= 1;
        }
        let state = &self.entities[id].behaviors.toppling[ordinal];
        let velocity = if state.accumulation + state.velocity > LIMIT {
            LIMIT - state.accumulation
        } else {
            state.velocity
        };
        let direction = state.direction;
        let (position, mut basis) = self.object_transform(id);
        pre_rotate(&mut basis, 0, -velocity * direction[1]);
        pre_rotate(&mut basis, 1, velocity * direction[0]);
        self.set_object_transform(id, position, basis);
        let state = &mut self.entities[id].behaviors.toppling[ordinal];
        state.accumulation += velocity;
        if state.accumulation >= LIMIT && state.velocity > 0.0 {
            self.topple_bounce(id, ordinal);
        } else {
            state.velocity += state.acceleration;
        }
        if self.entities[id].drawable.present {
            self.entities[id].drawable.status &= !2;
        }
        1
    }

    fn topple_bounce(&mut self, id: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[id].type_index].behavior.toppling[ordinal];
        let state = &mut self.entities[id].behaviors.toppling[ordinal];
        state.velocity *= -rules.bounce_velocity;
        if state.options & 1 != 0 || state.velocity.abs() < 0.01 {
            state.velocity = 0.0;
            state.phase = Phase::Down;
            let kill = rules.finish.kill;
            let reorient = rules.finish.reorient;
            let stump = rules.finish.kill_stump.then_some(state.stump).flatten();
            if kill {
                self.apply_native_damage(id, None, 999_999.0, DeathType::Toppled);
                if reorient {
                    let height = self.body_geometry_bounds(id).max_height;
                    let (position, basis) = self.object_transform(id);
                    let offset = vector(basis, [0.0, 0.0, height]);
                    self.set_object_position(
                        id,
                        std::array::from_fn(|axis| position[axis] + offset[axis]),
                    );
                    self.set_object_orientation(id, self.entities[id].facing);
                }
            }
            if let Some(stump) = stump.filter(|&id| self.entities[id].exists()) {
                self.apply_native_damage(stump, None, 999_999.0, DeathType::Toppled);
            }
        } else if state.velocity.abs() >= 0.03 && state.options & 2 == 0 {
            let effect = rules.bounce_fx.clone();
            self.object_fx(id, effect);
        }
    }
}
