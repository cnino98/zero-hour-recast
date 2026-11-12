use super::{Module, Skirmish, VisualAction, startup};

enum Stage {
    Standing,
    Waiting,
    Falling,
    Done,
}

pub(in crate::skirmish) struct State {
    stage: Stage,
    collapse_frame: u64,
    burst_frame: u64,
    velocity: f32,
    height: f32,
    template_height: f32,
}

impl State {
    pub const fn new(template_height: f32) -> Self {
        Self {
            stage: Stage::Standing,
            collapse_frame: 0,
            burst_frame: 0,
            velocity: 0.0,
            height: 0.0,
            template_height,
        }
    }
    pub const fn sleeping(&self) -> bool {
        matches!(self.stage, Stage::Standing | Stage::Done)
    }
}

impl Skirmish {
    pub(super) fn begin_collapse(&mut self, id: usize, state: &mut State) {
        let rules = self.catalog[self.entities[id].type_index]
            .structure_death
            .as_ref()
            .expect("collapse rules");
        let Module::Collapse(rules) = &rules.module else {
            unreachable!("collapse module")
        };
        state.collapse_frame =
            self.tick + u64::from(super::random_delay(&mut self.random, rules.delay));
        self.structure_phase(id, 0, self.object_position(id));
        state.stage = Stage::Waiting;
        state.height = 0.0;
    }

    pub(super) fn update_collapse(&mut self, id: usize, state: &mut State) {
        if matches!(state.stage, Stage::Standing | Stage::Done) {
            return;
        }
        let rules = self.catalog[self.entities[id].type_index]
            .structure_death
            .as_ref()
            .expect("collapse rules");
        let Module::Collapse(rules) = &rules.module else {
            unreachable!("collapse module")
        };
        let shudder = rules.shudder;
        let burst = rules.burst_delay;
        let frequency = rules.big_burst_frequency;
        let acceleration = rules.gravity * (1.0 - rules.damping);
        if matches!(state.stage, Stage::Waiting) {
            self.structure_visual(
                id,
                VisualAction::CollapseShudder {
                    height: 0.0,
                    amplitude: shudder,
                },
                None,
            );
            if self.tick >= state.collapse_frame {
                state.stage = Stage::Falling;
                self.structure_phase(id, 2, self.object_position(id));
                state.burst_frame =
                    self.tick + u64::from(super::random_delay(&mut self.random, burst));
            }
        }
        // Native uses two independent if statements: transition frames also
        // move the body and consume a second pair of client shudder draws.
        if matches!(state.stage, Stage::Falling) {
            state.height -= state.velocity;
            state.velocity -= acceleration;
            self.structure_visual(
                id,
                VisualAction::CollapseShudder {
                    height: state.height,
                    amplitude: shudder,
                },
                None,
            );
            if self.tick >= state.burst_frame {
                let big = if frequency <= 1 {
                    frequency == 1
                } else {
                    startup::random_between(
                        &mut self.random,
                        u32::try_from(frequency - 1).expect("positive burst frequency"),
                    ) == 0
                };
                self.structure_phase(id, if big { 2 } else { 1 }, self.object_position(id));
                state.burst_frame += u64::from(super::random_delay(&mut self.random, burst));
            }
            if state.height + state.template_height <= 0.0 {
                state.stage = Stage::Done;
                self.structure_phase(id, 3, self.object_position(id));
                self.stop_first_bone_fx(id);
                self.set_post_collapse(id);
                self.set_object_orientation(id, self.entities[id].facing);
                self.update_body_particles(id);
                self.structure_visual(id, VisualAction::Clear, None);
            }
        }
    }
}
