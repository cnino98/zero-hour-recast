//! Map-script entry to the shared native player repair queue.
use super::{State as BuildState, work};
use crate::skirmish::Skirmish;

pub(in crate::skirmish::map_scripts) fn enqueue(
    game: &Skirmish,
    state: &mut super::super::State,
    owner: usize,
    target: usize,
) {
    if let Some(build) = state
        .npc_builds
        .iter_mut()
        .find(|state| state.owner == owner)
    {
        build.repair.enqueue(game, target);
    }
}

pub(super) fn update(game: &mut Skirmish, state: &mut BuildState) {
    let center = state.center.map(|[x, y]| [x, y, 0.0]);
    if state.repair.update(game, state.owner, center) {
        work::queue_dozer(game, state);
    }
}

pub(super) const fn produced(state: &mut BuildState, unit: usize) -> bool {
    state.repair.produced(unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::Order;
    #[test]
    fn scripted_repair_caps_distinct_targets_and_retains_native_thirty_frame_handoff() {
        let mut game = super::super::tests::game(Vec::new());
        game.step();
        let first = game.map_objects["NPC_Barracks"];
        let owner = game.entities[first].owner;
        let kind = game.entities[first].type_index;
        let second = game.spawn(kind, owner, [620.0, 200.0], true);
        let third = game.spawn(kind, owner, [720.0, 200.0], true);
        for target in [first, second, third] {
            game.change_body_health(target, 400.0 - game.entities[target].health);
        }
        let worker = game.spawn(1, owner, [450.0, 200.0], true);
        game.catalog[1].maintenance.repair_per_second = 0.5;
        let mut scripts = std::mem::replace(
            &mut game.map_scripts,
            super::super::super::State::new(Vec::new()),
        );
        enqueue(&game, &mut scripts, owner, first);
        enqueue(&game, &mut scripts, owner, first);
        enqueue(&game, &mut scripts, owner, second);
        enqueue(&game, &mut scripts, owner, third);
        let state = &mut scripts.npc_builds[0];
        assert_eq!(state.repair.queue, [first, second]);
        update(&mut game, state);
        assert_eq!(state.repair.worker, Some(worker));
        assert_eq!(game.entities[worker].repair_task_target(), Some(first));
        game.update_dozer_tasks(worker);
        game.apply_healing(first, Some(worker), 600.0);
        game.update_dozer_tasks(worker);
        assert!(game.entities[worker].repair_task_target().is_none());
        for _ in 0..29 {
            update(&mut game, state);
        }
        assert_eq!(state.repair.queue, [first, second]);
        update(&mut game, state);
        assert_eq!(state.repair.queue, [second]);
        // Native updateBridgeRepair keeps bridgeObj pointing to the removed
        // first entry until the next thirty-frame scan; that full target rejects repair.
        assert!(matches!(game.entities[worker].ai.order, Order::Idle));
        for _ in 0..30 {
            update(&mut game, state);
        }
        assert_eq!(game.entities[worker].repair_task_target(), Some(second));
        game.update_dozer_tasks(worker);
        game.apply_healing(second, Some(worker), 600.0);
        game.update_dozer_tasks(worker);
        assert!(game.entities[worker].repair_task_target().is_none());
        for _ in 0..30 {
            update(&mut game, state);
        }
        assert!(state.repair.queue.is_empty());
        assert!(matches!(game.entities[worker].ai.order, Order::Move(_)));
        assert_eq!(state.repair.worker, Some(worker));
    }
}
