//! SH AIUpdate.h405 / AIUpdate.cpp1193: this flag is distinct from current `AI_DEAD` state.
use crate::skirmish::{Entity, Skirmish};

impl Entity {
    pub(in crate::skirmish) const fn is_ai_dead(&self) -> bool {
        self.ai.ai_dead
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn mark_ai_dead(&mut self, id: usize) {
        self.entities[id].ai.ai_dead = true;
        self.entities[id].set_effectively_dead(true);
        self.wake_ai_now(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::ai_goals::MainState;

    #[test]
    fn marking_ai_dead_does_not_enter_dead_state() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        assert!(!game.entities[actor].is_ai_dead());
        game.mark_ai_dead(actor);
        assert!(game.entities[actor].is_ai_dead());
        assert!(game.entities[actor].is_effectively_dead());
        assert!(!matches!(
            game.entities[actor].ai.command.main_state,
            MainState::Dead
        ));
    }
}
