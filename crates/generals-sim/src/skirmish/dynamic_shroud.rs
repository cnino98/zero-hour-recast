//! SH 2bfaea60 `DynamicShroudClearingRangeUpdate` and its grid decals.
use super::{Skirmish, progression::integer_real, radius_decal};
use num_traits::ToPrimitive;
use std::f32::consts::TAU;

pub struct Rules {
    pub final_vision: f32,
    pub change_interval: u32,
    pub grow_interval: u32,
    pub shrink_delay: u32,
    pub shrink_time: u32,
    pub grow_delay: u32,
    pub grow_time: u32,
    pub grid_decal: Option<radius_decal::Template>,
}

#[derive(PartialEq, Eq)]
enum Phase {
    NotStarted,
    Growing,
    Sustaining,
    Shrinking,
    Done,
    Sleeping,
}

pub(super) struct State {
    phase: Phase,
    remaining: u32,
    done_frame: u32,
    interval: u32,
    native_range: f32,
    current_range: f32,
    grid: Option<(f32, f32)>,
}

const GRID_DECAL_COUNT: usize = 30;

impl Skirmish {
    pub(super) fn construct_dynamic_shroud(&mut self, id: usize, ordinal: usize) {
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = &unit.behavior.dynamic_shroud[ordinal];
        let remaining = rules.shrink_delay.wrapping_add(rules.shrink_time);
        let native_range = actor.shroud_range(unit);
        actor.behaviors.dynamic_shroud.push(State {
            phase: Phase::NotStarted,
            remaining,
            done_frame: now.wrapping_add(remaining),
            interval: 0,
            native_range,
            current_range: 0.0,
            grid: None,
        });
    }

    pub(super) fn update_dynamic_shroud(&mut self, id: usize, ordinal: usize) -> i32 {
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.dynamic_shroud[ordinal];
        let state = &mut actor.behaviors.dynamic_shroud[ordinal];
        if state.phase == Phase::Sleeping {
            return 1;
        }
        let total = rules.shrink_delay.wrapping_add(rules.shrink_time);
        let grow_start = total.wrapping_sub(rules.grow_delay);
        let sustain_start = grow_start.wrapping_sub(rules.grow_time);
        if state.remaining == 0 || now > state.done_frame {
            state.phase = Phase::Done;
        } else if state.remaining <= rules.shrink_time {
            state.phase = Phase::Shrinking;
        } else if state.remaining <= sustain_start {
            state.phase = Phase::Sustaining;
        } else if state.remaining <= grow_start {
            state.phase = Phase::Growing;
        }
        if rules.grid_decal.is_some() && matches!(state.phase, Phase::NotStarted | Phase::Growing) {
            let elapsed = integer_real(total.max(1) - state.remaining);
            state.grid = Some((
                state.current_range + elapsed * 2.0,
                1.0 - state.current_range / state.native_range,
            ));
        }
        match state.phase {
            Phase::NotStarted | Phase::Sleeping => {}
            Phase::Growing => {
                state.current_range += state.native_range / integer_real(rules.grow_time.max(1));
                if state.current_range >= state.native_range {
                    state.phase = Phase::Sustaining;
                }
            }
            Phase::Sustaining => {
                state.current_range = state.native_range;
                state.grid = None;
            }
            Phase::Shrinking => {
                state.current_range -= (state.native_range - rules.final_vision)
                    / integer_real(rules.shrink_time.max(1));
            }
            Phase::Done => {
                state.grid = None;
                state.current_range = rules.final_vision;
            }
        }
        state.remaining = state.remaining.saturating_sub(1);
        if state.interval > 0 {
            state.interval -= 1;
        } else {
            state.interval = if state.phase == Phase::Growing {
                rules.grow_interval
            } else {
                rules.change_interval
            };
            if state.phase == Phase::Done {
                state.phase = Phase::Sleeping;
            }
            let range = state.current_range;
            self.set_shroud_clearing_range(id, range);
        }
        1
    }

    /// Current packed radius-grid placement. Its 23-unit snap truncates signed XY.
    ///
    /// # Panics
    /// Panics if a slot falls outside the native signed coordinate domain.
    #[must_use]
    pub fn dynamic_shroud_grid_slots(
        &self,
        id: usize,
        ordinal: usize,
    ) -> Option<([[f32; 3]; GRID_DECAL_COUNT], f32)> {
        let actor = self.entities.get(id)?;
        if !actor.exists() || actor.is_effectively_dead() {
            return None;
        }
        let (radius, opacity) = actor.behaviors.dynamic_shroud.get(ordinal)?.grid?;
        let [x, y] = actor.position;
        let mut angle = 0.0_f32;
        let positions = std::array::from_fn(|_| {
            let offset = [angle.sin() * radius, angle.cos() * radius];
            let snap = |point: f32| {
                point
                    - (point.to_i32().expect("native grid decal coordinate") % 23)
                        .to_f32()
                        .expect("grid decal snap remainder")
            };
            angle += TAU / 30.0;
            [snap(x + offset[0]), snap(y + offset[1]), 0.0]
        });
        Some((positions, opacity))
    }
}

#[cfg(test)]
mod tests;
