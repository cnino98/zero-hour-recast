//! AIUpdate.cpp2887 native position commands retain AI temporary-state semantics.
use crate::skirmish::{
    Order, Skirmish,
    ai_dispatch::stored_command::{Command, Kind},
    ai_goals::COMMAND_FROM_AI,
};
impl Skirmish {
    pub(in crate::skirmish) fn native_move_from(
        &mut self,
        id: usize,
        position: [f32; 3],
        source: i32,
    ) {
        if source == COMMAND_FROM_AI {
            self.construction_move(id, position);
            return;
        }
        if self.native_subclass_command(
            id,
            Command {
                source,
                kind: Kind::Move(position),
            },
        ) || !self.native_ai_command_allowed(id)
            || self.attack_kind(id, "IMMOBILE")
            || self.object_disabled(id)
        {
            return;
        }
        let position = self.clipped_ai_goal_position(id, position, source);
        self.set_order_state_from(
            id,
            Order::Move([position[0], position[1]]),
            false,
            Some(position),
            Some(source),
        );
    }
}
