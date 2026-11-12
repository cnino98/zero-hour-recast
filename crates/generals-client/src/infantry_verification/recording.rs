//! Diagnostic fire-time records retained across multiple fixed updates per draw.
use crate::playtest::Match;
use bevy::prelude::*;
use generals_sim::skirmish::Shot;

pub(super) struct RecordedShot {
    source: usize,
    born: u64,
    pub veterancy: u8,
    pub source_position: [f32; 3],
}

#[derive(Resource)]
pub struct FiringShots {
    recorded: Option<(u32, u64)>,
    shots: Vec<RecordedShot>,
}

impl FiringShots {
    pub const fn new() -> Self {
        Self {
            recorded: None,
            shots: Vec::new(),
        }
    }

    fn observe(&mut self, generation: u32, tick: u64, selected: &[usize], shots: &[Shot]) {
        if self.recorded == Some((generation, tick)) {
            return;
        }
        if self
            .recorded
            .is_none_or(|(previous, _)| previous != generation)
        {
            self.shots.clear();
        }
        self.recorded = Some((generation, tick));
        self.shots.extend(
            shots
                .iter()
                .filter(|shot| selected.contains(&shot.source))
                .map(|shot| RecordedShot {
                    source: shot.source,
                    born: tick,
                    veterancy: shot.veterancy,
                    source_position: shot.source_position,
                }),
        );
    }

    pub(super) fn find(&self, generation: u32, source: usize, born: u64) -> Option<&RecordedShot> {
        if self
            .recorded
            .is_none_or(|(previous, _)| previous != generation)
        {
            return None;
        }
        self.shots
            .iter()
            .find(|shot| shot.source == source && shot.born == born)
    }
}

pub fn record(battle: Res<Match>, mut shots: ResMut<FiringShots>) {
    let battle = battle.into_inner();
    shots.observe(
        battle.generation,
        battle.simulation.tick,
        &battle.selected,
        &battle.simulation.presentation.shots,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_after_catch_up_matches_the_tracer_birth_not_the_latest_logic_tick() {
        let mut records = FiringShots::new();
        let shot = Shot {
            weapon_type: 0,
            weapon_index: 0,
            weapon_slot: 0,
            barrel: 0,
            source: 8,
            target: Some(2),
            source_position: [100.0, 200.0, 5.0],
            source_facing: 0.5,
            target_position: [200.0, 200.0, 10.0],
            veterancy: 3,
            from: [100.0, 200.0],
            to: [200.0, 200.0],
            owner: 0,
        };
        records.observe(1, 100, &[8], std::slice::from_ref(&shot));
        records.observe(1, 100, &[8], std::slice::from_ref(&shot));
        for tick in 101..=107 {
            records.observe(1, tick, &[8], &[]);
        }
        let stored = records.find(1, 8, 100).unwrap();
        assert_eq!(stored.veterancy, 3);
        assert_eq!(
            stored.source_position.map(f32::to_bits),
            shot.source_position.map(f32::to_bits)
        );
        assert_eq!(records.shots.len(), 1);
        assert!(records.find(1, 8, 107).is_none());
        assert!(records.find(1, 9, 100).is_none());
        records.observe(2, 100, &[8], &[]);
        assert!(records.find(1, 8, 100).is_none());
        assert!(records.find(2, 8, 100).is_none());
    }
}
