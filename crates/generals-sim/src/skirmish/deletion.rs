//! DeletionUpdate.cpp: constructor RNG and deferred destruction without onDie.
use super::{Skirmish, module_updates::FOREVER};

impl Skirmish {
    pub(super) fn construct_deletion(&mut self, id: usize, registration: usize) {
        let ordinal = self.entities[id].behaviors.deletion_frames.len();
        let range = self.catalog[self.entities[id].type_index]
            .behavior
            .deletions[ordinal];
        let [low, high] = range.map(|value| i32::from_ne_bytes(value.to_ne_bytes()));
        let delay = if low >= high {
            range[1]
        } else {
            range[0].wrapping_add(self.random_between(range[1].wrapping_sub(range[0])))
        }
        .max(1);
        let deadline = self.update_frame().wrapping_add(delay);
        self.entities[id].behaviors.deletion_frames.push(deadline);
        self.wake_update(id, registration, delay);
    }

    pub(super) fn update_deletion(&mut self, id: usize) -> i32 {
        self.queue_update_destruction(id);
        i32::from_ne_bytes(FOREVER.to_ne_bytes())
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{
        Kind, startup,
        tests::{game, register_update},
    };

    #[test]
    fn deletion_preserves_health_and_effective_dead_until_deferred_removal() {
        let mut game = game();
        let kind = game.type_for(0, Kind::CommandCenter).unwrap();
        game.catalog[kind].behavior.deletions = vec![[0; 2], [30; 2]];
        register_update(&mut game.catalog[kind], "DeletionUpdate");
        register_update(&mut game.catalog[kind], "DeletionUpdate");
        // Three child Idle machines and primary Idle enter after the module constructors.
        let mut expected = game.random;
        for _ in 0..4 {
            startup::random_between(&mut expected, 60);
        }
        let id = game.spawn(kind, 0, [240.0, 240.0], true);
        let health = game.entities[id].health;
        assert_eq!(game.entities[id].behaviors.deletion_frames, [1, 30]);
        assert_eq!(game.random, expected);
        game.run_object_updates();
        game.process_update_destruction();
        assert!(game.entities[id].exists());
        game.tick = 1;
        game.run_object_updates();
        assert!(game.entities[id].exists());
        assert_eq!(game.entities[id].health.to_bits(), health.to_bits());
        assert!(!game.entities[id].is_effectively_dead());
        game.process_update_destruction();
        assert!(!game.entities[id].exists());
        assert_eq!(game.entities[id].health.to_bits(), health.to_bits());
        assert!(!game.entities[id].is_effectively_dead());
    }

    #[test]
    fn deletion_constructor_draws_in_authored_order_and_uses_signed_endpoint_check() {
        let mut game = game();
        let kind = game.type_for(0, Kind::CommandCenter).unwrap();
        game.catalog[kind].behavior.deletions = vec![[4, 8], [u32::MAX, 3]];
        register_update(&mut game.catalog[kind], "DeletionUpdate");
        register_update(&mut game.catalog[kind], "DeletionUpdate");
        let mut expected = game.random;
        let first = 4 + startup::random_between(&mut expected, 4);
        let second = u32::MAX
            .wrapping_add(startup::random_between(&mut expected, 4))
            .max(1);
        for _ in 0..4 {
            startup::random_between(&mut expected, 60);
        }
        let id = game.spawn(kind, 0, [240.0, 240.0], true);
        assert_eq!(game.entities[id].behaviors.deletion_frames, [first, second]);
        assert_eq!(game.random, expected);
    }
}
