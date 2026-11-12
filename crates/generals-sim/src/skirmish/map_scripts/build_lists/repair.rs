//! Native two-entry `AIPlayer::repairStructure` queue and reserved repair dozer.
use super::{State as BuildState, runtime::placement, work};
use crate::skirmish::Skirmish;

pub(super) struct State {
    queue: Vec<usize>,
    timer: i32,
    pub(super) worker: Option<usize>,
    phase: Phase,
    origin: [f32; 3],
}

enum Phase {
    Idle,
    Queued,
    Repairing,
}

impl State {
    pub(super) const fn new() -> Self {
        // AIPlayer leaves m_bridgeTimer uninitialized; Rust chooses zero.
        // MAPS.md records this defined adaptation, not initial dispatch parity.
        Self {
            queue: Vec::new(),
            timer: 0,
            worker: None,
            phase: Phase::Idle,
            origin: [0.0; 3],
        }
    }
}

pub(in crate::skirmish::map_scripts) fn enqueue(
    game: &Skirmish,
    state: &mut super::super::State,
    owner: usize,
    target: usize,
) {
    if pristine(game, target) || !game.entities[target].exists() {
        return;
    }
    let Some(build) = state
        .npc_builds
        .iter_mut()
        .find(|state| state.owner == owner)
    else {
        return;
    };
    let repair = &mut build.repair;
    if repair.queue.len() >= 2 || repair.queue.contains(&target) {
        return;
    }
    repair.queue.push(target);
}

pub(super) fn update(game: &mut Skirmish, state: &mut BuildState) {
    if state.repair.queue.is_empty() {
        return;
    }
    state.repair.timer -= 1;
    if state.repair.timer > 0 {
        return;
    }
    state.repair.timer = 30;
    while state
        .repair
        .queue
        .first()
        .is_some_and(|&id| !game.entities[id].exists())
    {
        state.repair.queue.remove(0);
    }
    let Some(&target) = state.repair.queue.first() else {
        return;
    };
    let Some(worker) = state.repair.worker else {
        acquire_worker(game, state, target);
        return;
    };
    if !game.entities[worker].exists() {
        state.repair.worker = None;
        state.repair.timer = 0;
        return;
    }
    if matches!(state.repair.phase, Phase::Repairing) {
        if game.entities[worker].has_pending_dozer_task() {
            return;
        }
        if pristine(game, target) {
            state.repair.queue.remove(0);
            state.repair.phase = Phase::Idle;
            if state.repair.queue.is_empty() {
                let requested = state
                    .center
                    .map_or(state.repair.origin, |point| [point[0], point[1], 0.0]);
                let adjusted = game
                    .adjusted_destination_world(worker, requested)
                    .unwrap_or(requested);
                let _ = game.begin_move_world(worker, adjusted);
                return;
            }
        }
    }
    let _ = game.begin_repair(worker, target, true);
    state.repair.phase = Phase::Repairing;
}

fn acquire_worker(game: &mut Skirmish, state: &mut BuildState, target: usize) {
    if matches!(state.repair.phase, Phase::Queued) {
        return;
    }
    state.repair.phase = Phase::Idle;
    let (worker, need_dozer) =
        placement::select_dozer(game, state.owner, game.entities[target].position, None);
    if let Some(worker) = worker {
        state.repair.worker = Some(worker);
        state.repair.origin = game.object_position(worker);
        let _ = game.begin_repair(worker, target, true);
        state.repair.phase = Phase::Repairing;
        return;
    }
    if need_dozer {
        work::queue_dozer(game, state);
    }
    // Source queues again even if findDozer saw only building dozers.
    work::queue_dozer(game, state);
    state.repair.phase = Phase::Queued;
}

pub(super) const fn produced(state: &mut BuildState, unit: usize) -> bool {
    if matches!(state.repair.phase, Phase::Queued) {
        state.repair.worker = Some(unit);
        state.repair.phase = Phase::Idle;
        return true;
    }
    false
}

fn pristine(game: &Skirmish, id: usize) -> bool {
    let actor = &game.entities[id];
    actor.health / actor.max_health > game.ai_rules.unit_damaged_threshold
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
        game.entities[first].health = 400.0;
        game.entities[second].health = 400.0;
        game.entities[third].health = 400.0;
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
