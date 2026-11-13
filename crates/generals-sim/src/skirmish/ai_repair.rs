//! `AIPlayer::repairStructure`, `updateBridgeRepair` and `findDozer`.
use super::{Skirmish, ai_goals::COMMAND_FROM_AI, movement::distance_squared};

pub(super) struct State {
    pub(super) queue: Vec<usize>,
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
        // The native constructor does not initialize its bridge timer.
        Self {
            queue: Vec::new(),
            timer: 0,
            worker: None,
            phase: Phase::Idle,
            origin: [0.0; 3],
        }
    }

    pub(super) fn enqueue(&mut self, game: &Skirmish, target: usize) {
        if self.queue.len() < 2
            && !self.queue.contains(&target)
            && game.entities[target].exists()
            && game.body_damage_state(target) != 0
        {
            self.queue.push(target);
        }
    }

    /// Returns whether this player must request a paid dozer work order.
    pub(super) fn update(
        &mut self,
        game: &mut Skirmish,
        owner: usize,
        center: Option<[f32; 3]>,
    ) -> bool {
        if self.queue.is_empty() {
            return false;
        }
        self.timer -= 1;
        if self.timer > 0 {
            return false;
        }
        self.timer = 30;
        while self
            .queue
            .first()
            .is_some_and(|&id| !game.entities[id].exists())
        {
            self.queue.remove(0);
        }
        let Some(&target) = self.queue.first() else {
            return false;
        };
        let Some(worker) = self.worker else {
            return self.acquire_worker(game, owner, target);
        };
        if !game.entities[worker].exists() {
            self.worker = None;
            self.timer = 0;
            return false;
        }
        if matches!(self.phase, Phase::Repairing) {
            if game.entities[worker].has_pending_dozer_task() {
                return false;
            }
            if game.body_damage_state(target) == 0 {
                self.queue.remove(0);
                self.phase = Phase::Idle;
                if self.queue.is_empty() {
                    let goal =
                        game.adjust_to_possible_destination(worker, center.unwrap_or(self.origin));
                    game.native_move_from(worker, goal, COMMAND_FROM_AI);
                    return false;
                }
            }
        }
        self.issue(game, worker, target);
        false
    }

    fn acquire_worker(&mut self, game: &mut Skirmish, owner: usize, target: usize) -> bool {
        if matches!(self.phase, Phase::Queued) {
            return false;
        }
        self.phase = Phase::Idle;
        let (worker, _) = select_dozer(game, owner, game.entities[target].position, None);
        if let Some(worker) = worker {
            self.worker = Some(worker);
            self.origin = game.object_position(worker);
            self.issue(game, worker, target);
            false
        } else {
            self.phase = Phase::Queued;
            true
        }
    }

    fn issue(&mut self, game: &mut Skirmish, worker: usize, target: usize) {
        if !game.entities[worker].is_effectively_dead() && game.entities[worker].ai.dozer.is_some()
        {
            let _ = game.dozer_repair_command(worker, target, true);
        }
        self.phase = Phase::Repairing;
    }

    pub(super) const fn produced(&mut self, unit: usize) -> bool {
        if matches!(self.phase, Phase::Queued) {
            self.worker = Some(unit);
            self.phase = Phase::Idle;
            true
        } else {
            false
        }
    }
}

pub(super) fn select_dozer(
    game: &Skirmish,
    owner: usize,
    position: [f32; 2],
    reserved: Option<usize>,
) -> (Option<usize>, bool) {
    let mut selected = None;
    let mut closest = None;
    let mut nearest = 0.0;
    let mut exists = false;
    for id in (0..game.entities.len()).rev() {
        let actor = &game.entities[id];
        if actor.owner != owner
            || !actor.exists()
            || !game.attack_kind(id, "DOZER")
            || actor.ai.dozer.is_none()
            || reserved == Some(id)
        {
            continue;
        }
        let pending = actor.has_pending_dozer_task();
        if !pending
            && game.worker_interface(id)
            && (actor.ai.supply.ferrying() || actor.ai.supply.force_wanting)
        {
            continue;
        }
        exists = true;
        if actor.build_task_target().is_some() {
            continue;
        }
        let idle = !pending;
        if idle || selected.is_none() {
            selected = Some(id);
        }
        if idle {
            let distance = distance_squared(position, actor.position);
            if closest.is_none() || distance < nearest {
                closest = Some(id);
                nearest = distance;
            }
        }
    }
    (closest.or(selected), !exists)
}
