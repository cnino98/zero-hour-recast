//! Native `AIUpdate::privateExit`, SH 2bfaea60 AIUpdate.cpp3826.
use crate::skirmish::{Order, Skirmish, ai_goals::COMMAND_FROM_AI};

impl Skirmish {
    pub(in crate::skirmish) fn native_ai_exit(&mut self, id: usize, carrier: usize) {
        self.native_exit_from(id, carrier, COMMAND_FROM_AI);
    }

    pub(in crate::skirmish) fn native_exit_from(&mut self, id: usize, carrier: usize, source: i32) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Exit(carrier),
            },
        ) {
            return;
        }
        if self.entities[id].ai.registration.is_none()
            || !self.native_ai_command_allowed(id)
            || self.catalog[self.entities[carrier].type_index]
                .contain_family()
                .is_none()
            || !self.passengers(carrier).contains(&id)
            || self.entities[carrier].subdued()
        {
            return;
        }
        self.entities[id].containment.exit_at = Some(carrier);
        // Clear the previous state, install the goal and source, then notify AI_EXIT entry.
        self.set_order_state_from(id, Order::ExitContainer, false, None, Some(source));
    }
}
