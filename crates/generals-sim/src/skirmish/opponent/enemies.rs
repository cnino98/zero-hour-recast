//! `AISkirmishPlayer::getAiEnemy/acquireEnemy` (150-frame source cadence).
use super::OpponentRules;
use crate::skirmish::{Skirmish, movement::distance_squared};

impl Skirmish {
    pub(super) fn ai_enemy(&mut self, owner: usize) -> Option<usize> {
        let opponent = self.opponents.get(owner)?.as_ref()?;
        if self.tick < opponent.state.next_enemy_check {
            return opponent.state.enemy;
        }
        let current = opponent.state.enemy;
        let center = opponent.state.base_center;
        let rules = std::sync::Arc::clone(&opponent.rules);
        // Player::getCurrentEnemy recursively calls other AI getAiEnemy. Advance
        // the deadline before acquisition, so cycles observe cached candidates.
        self.opponents[owner].as_mut()?.state.next_enemy_check = self.tick + 150;
        if current
            .is_some_and(|enemy| self.player_active(enemy) && !in_bad_shape(self, &rules, enemy))
        {
            return current;
        }
        let mut best = None;
        let mut best_distance = 1_000_000.0_f32.powi(2);
        for candidate in 0..self.slots.len() {
            if !self.enemies(owner, candidate)
                || !self.player_active(candidate)
                || !self
                    .entities
                    .iter()
                    .any(|entity| entity.owner == candidate && entity.alive())
            {
                continue;
            }
            let mut distance = if in_bad_shape(self, &rules, candidate) {
                1_000_000.0_f32.powi(2) * 0.5
            } else {
                distance_squared(center, structure_center(self, candidate))
            };
            for other in 0..self.slots.len() {
                // The source excludes candidate index i, not querying AI index.
                if other == candidate {
                    continue;
                }
                let enemy = self.ai_enemy(other);
                if enemy == Some(candidate) {
                    distance += 250_000.0;
                }
                if enemy == Some(owner) {
                    distance = (distance - 625.0).max(0.0);
                }
            }
            if distance < best_distance {
                best_distance = distance;
                best = Some(candidate);
            }
        }
        if let Some(enemy) = best {
            let state = &mut self.opponents[owner].as_mut()?.state;
            state.enemy = Some(enemy);
            state.enemy_start = self.start_positions[enemy].unwrap_or(0);
        }
        self.opponents[owner].as_ref()?.state.enemy
    }
}

fn in_bad_shape(game: &Skirmish, rules: &OpponentRules, owner: usize) -> bool {
    let mut units = false;
    let mut facility = false;
    for entity in &game.entities {
        if entity.owner != owner || !entity.alive() {
            continue;
        }
        let unit = &game.catalog[entity.type_index];
        units |= !unit.kind.is_building();
        facility |= rules.build_facilities.contains(&unit.name);
    }
    !units || !facility
}

pub(super) fn structure_center(game: &Skirmish, owner: usize) -> [f32; 2] {
    // AIPlayer::getPlayerStructureBounds unites structure origin points. Its
    // original no-structure branch leaves the zero bounds unchanged.
    let mut bounds: Option<([f32; 2], [f32; 2])> = None;
    for entity in &game.entities {
        if entity.owner != owner
            || !entity.alive()
            || !game.catalog[entity.type_index].kind.is_building()
        {
            continue;
        }
        if let Some((low, high)) = &mut bounds {
            for axis in 0..2 {
                low[axis] = low[axis].min(entity.position[axis]);
                high[axis] = high[axis].max(entity.position[axis]);
            }
        } else {
            bounds = Some((entity.position, entity.position));
        }
    }
    bounds.map_or([0.0; 2], |(low, high)| {
        let half_width = (high[0] - low[0]) * 0.5;
        let half_height = (high[1] - low[1]) * 0.5;
        [low[0] + half_width, low[1] + half_height]
    })
}
