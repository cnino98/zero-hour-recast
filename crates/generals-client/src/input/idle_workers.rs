//! `InGameUI`'s insertion-ordered idle dozer list for represented ground workers.
use super::Mouse;
use generals_sim::skirmish::Skirmish;

impl Mouse {
    /// Observe every logic frame, so rendering rate cannot reorder idle entries.
    pub fn refresh_idle_workers(&mut self, game: &Skirmish, generation: u32) {
        self.reset_generation(generation);
        // DozerPrimaryIdleState::update registers a living idle dozer once;
        // onExit removes it. A later idle entry is appended, not ID-sorted.
        let idle: Vec<_> = game
            .idle_workers
            .iter()
            .filter_map(|&(id, owner)| (owner == 0).then_some(id))
            .collect();
        update_list(&mut self.idle_workers, &idle);
    }

    pub fn next_idle_worker(&self, selected: &[usize]) -> Option<usize> {
        next_worker(&self.idle_workers, selected)
    }
}

fn update_list(workers: &mut Vec<usize>, idle: &[usize]) {
    workers.retain(|id| idle.contains(id));
    for &id in idle {
        if !workers.contains(&id) {
            workers.push(id);
        }
    }
}

fn next_worker(workers: &[usize], selected: &[usize]) -> Option<usize> {
    // InGameUI::selectNextIdleWorker: only a single selected list member advances.
    let first = *workers.first()?;
    if let [selected] = selected
        && let Some(index) = workers.iter().position(|id| id == selected)
    {
        return Some(workers.get(index + 1).copied().unwrap_or(first));
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::{next_worker, update_list};

    #[test]
    fn idle_worker_rotation_wraps_and_busy_reentry_moves_to_the_end() {
        let mut workers = Vec::new();
        update_list(&mut workers, &[2, 4, 7]);
        assert_eq!(next_worker(&workers, &[]), Some(2));
        assert_eq!(next_worker(&workers, &[2]), Some(4));
        assert_eq!(next_worker(&workers, &[7]), Some(2));
        assert_eq!(next_worker(&workers, &[9]), Some(2));
        assert_eq!(next_worker(&workers, &[2, 4]), Some(2));
        update_list(&mut workers, &[2, 7]);
        assert_eq!(next_worker(&workers, &[2]), Some(7));
        update_list(&mut workers, &[2, 4, 7]);
        assert_eq!(workers, [2, 7, 4]);
        assert_eq!(next_worker(&workers, &[7]), Some(4));
        update_list(&mut workers, &[]);
        assert!(next_worker(&workers, &[4]).is_none());
    }
}
