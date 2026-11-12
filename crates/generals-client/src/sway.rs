//! Original `SwayClientUpdate`, using the shared client random stream and `Drawable` matrix.
use crate::{model_animation::Animations, playtest::Match, playtest_view::UnitVisual};
use bevy::prelude::*;
use generals_sim::skirmish::{map_scripts::Breeze, physical_ocl::transform::pre_rotate};
use std::{collections::BTreeMap, f32::consts::PI};

#[derive(Resource, Default)]
pub struct Sway {
    generation: Option<u32>,
    states: BTreeMap<usize, Vec<State>>,
}

struct State {
    delta: f32,
    value: f32,
    angle: f32,
    limit: f32,
    lean: f32,
    swaying: bool,
    version: i16,
}

impl State {
    const fn new() -> Self {
        Self {
            delta: 0.0,
            value: 0.0,
            angle: 0.0,
            limit: 0.0,
            lean: 0.0,
            swaying: true,
            version: -1,
        }
    }

    fn update(
        &mut self,
        breeze: &Breeze,
        visible: bool,
        time: f32,
        matrix: &mut [[f32; 4]; 3],
        random: &mut impl FnMut(f32, f32) -> f32,
    ) -> bool {
        if !self.swaying {
            return false;
        }
        if breeze.version != self.version {
            if breeze.randomness == 0.0 {
                self.value = 0.0;
            }
            let delta = breeze.randomness * 0.5;
            self.limit = breeze.intensity * random(1.0 - delta, 1.0 + delta);
            self.delta = (2.0 * PI / f32::from(breeze.period)) * random(1.0 - delta, 1.0 + delta);
            self.lean = breeze.lean * random(1.0 - delta, 1.0 + delta);
            self.version = breeze.version;
        } else if !visible {
            return false;
        }
        let advance = self.delta * time;
        self.value += advance;
        let full_turn = 2.0 * PI;
        if self.value > full_turn {
            self.value -= full_turn;
        }
        let oscillation = self.value.cos() * self.limit;
        let target = oscillation + self.lean;
        let delta = target - self.angle;
        let mut basis = matrix.map(|row| [row[0], row[1], row[2]]);
        pre_rotate(&mut basis, 0, -delta * breeze.direction[0]);
        pre_rotate(&mut basis, 1, delta * breeze.direction[1]);
        for (row, values) in matrix.iter_mut().zip(basis) {
            row[..3].copy_from_slice(&values);
        }
        self.angle = target;
        true
    }
}

pub fn update(
    mut battle: ResMut<Match>,
    mut sway: ResMut<Sway>,
    mut animations: ResMut<Animations>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Virtual>>,
    roots: Query<(&UnitVisual, &Children)>,
    visible: Query<&ViewVisibility, With<Mesh3d>>,
) {
    let fixed = fixed.into_inner();
    let time = time.into_inner();
    if !battle.started || battle.paused || time.is_paused() {
        return;
    }
    let generation = battle.generation;
    if sway.generation != Some(generation) {
        sway.states.clear();
        sway.generation = Some(generation);
    }
    let breeze = battle.simulation.breeze();
    let ratio = (time.delta_secs() / fixed.timestep().as_secs_f32()).min(1.0);
    let visible: BTreeMap<_, _> = roots
        .iter()
        .filter(|(visual, _)| visual.generation == generation)
        .map(|(visual, children)| {
            (
                visual.index,
                children
                    .iter()
                    .any(|child| visible.get(child).is_ok_and(|visibility| visibility.get())),
            )
        })
        .collect();
    // GameClient prepends new Drawables and visits that list from its head.
    for id in (0..battle.simulation.entities.len()).rev() {
        let actor = &battle.simulation.entities[id];
        if !actor.exists() || !actor.has_drawable() {
            sway.states.remove(&id);
            continue;
        }
        let count = battle.simulation.catalog[actor.type_index].sway_updates;
        if count == 0 {
            continue;
        }
        let states = sway
            .states
            .entry(id)
            .or_insert_with(|| (0..count).map(|_| State::new()).collect());
        let mut matrix = actor.drawable_instance_matrix().unwrap_or([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ]);
        for state in states {
            if actor.topple_stopped_sway() {
                state.swaying = false;
            }
            if !state.swaying {
                continue;
            }
            let updated = state.update(
                &breeze,
                visible.get(&id).copied().unwrap_or(false),
                ratio,
                &mut matrix,
                &mut |low, high| animations.effect_real(generation, low, high),
            );
            if updated && actor.burned() {
                state.swaying = false;
            }
        }
        battle.simulation.entities[id].set_client_instance_matrix(matrix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breeze(version: i16) -> Breeze {
        Breeze {
            direction: [0.0, 1.0],
            intensity: 0.1,
            lean: 0.05,
            randomness: 0.2,
            period: 150,
            version,
        }
    }

    #[test]
    fn invisible_initial_and_changed_breeze_updates_draw_three_randoms_then_visible_only() {
        let mut state = State::new();
        let mut matrix = [
            [1.0, 0.0, 0.0, 9.0],
            [0.0, 1.0, 0.0, 8.0],
            [0.0, 0.0, 1.0, 7.0],
        ];
        let mut draws = 0;
        state.update(&breeze(0), false, 0.5, &mut matrix, &mut |_, _| {
            draws += 1;
            1.0
        });
        assert_eq!(draws, 3);
        assert!(matrix[0][2] > 0.1);
        assert_eq!(
            matrix.map(|row| row[3].to_bits()),
            [9.0_f32, 8.0, 7.0].map(f32::to_bits)
        );
        let retained = matrix;
        state.update(&breeze(0), false, 1.0, &mut matrix, &mut |_, _| {
            draws += 1;
            1.0
        });
        assert_eq!(
            matrix.map(|row| row.map(f32::to_bits)),
            retained.map(|row| row.map(f32::to_bits))
        );
        assert_eq!(draws, 3);
        state.update(&breeze(1), false, 1.0, &mut matrix, &mut |_, _| {
            draws += 1;
            1.0
        });
        assert_eq!(draws, 6);
        state.swaying = false;
        let retained = matrix;
        state.update(&breeze(2), true, 1.0, &mut matrix, &mut |_, _| {
            draws += 1;
            1.0
        });
        assert_eq!(draws, 6);
        assert_eq!(
            matrix.map(|row| row.map(f32::to_bits)),
            retained.map(|row| row.map(f32::to_bits))
        );
    }
}
