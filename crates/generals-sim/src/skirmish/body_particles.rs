//! `ActiveBody::updateBodyParticleSystems`: source-time model/bone and Object pose.
use super::{Skirmish, object_fx};
use std::collections::BTreeMap;

pub struct Event {
    pub pose: object_fx::Event,
    pub aflame: bool,
    pub draws: Vec<BTreeMap<String, [f32; 3]>>,
}

impl Event {
    /// `Drawable` appends each draw's contiguous numbered series, capped at 16.
    #[must_use]
    pub fn positions(&self, prefix: &str) -> Vec<[f32; 3]> {
        let prefix = prefix.to_ascii_lowercase();
        let mut positions = Vec::new();
        for draw in &self.draws {
            for index in 1..=99 {
                let Some(position) = draw.get(&format!("{prefix}{index:02}")) else {
                    break;
                };
                positions.push(*position);
                if positions.len() == 16 {
                    return positions;
                }
            }
        }
        positions
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn body_particle_state_changed(&mut self, id: usize, previous: usize) {
        if self.body_damage_state(id) != previous && !self.entities[id].under_construction() {
            self.update_body_particles(id);
        }
    }

    pub(in crate::skirmish) fn update_body_particles(&mut self, id: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if !unit.body_kind.is_active() {
            return;
        }
        let conditions = self.model_flags(actor, unit.body_damage_thresholds);
        let draws = if actor.has_drawable() {
            unit.pristine_draws
                .iter()
                .filter_map(|draw| draw.select(&conditions))
                .map(|state| {
                    state
                        .transforms
                        .iter()
                        .map(|(name, matrix)| (name.clone(), matrix.map(|row| row[3])))
                        .collect()
                })
                .collect()
        } else {
            Vec::new()
        };
        let (position, basis) = self.object_transform(id);
        let event = Event {
            pose: self.snapshot_object_fx(id, String::new(), position, basis),
            aflame: actor.aflame(),
            draws,
        };
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::BodyParticles(
                self.presentation.body_particles.len(),
            ));
        self.presentation.body_particles.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multi_draw_numbered_series_restarts_after_gaps_and_caps_total_at_sixteen() {
        let mut first = BTreeMap::new();
        first.insert("smokes01".into(), [1.0; 3]);
        first.insert("smokes03".into(), [999.0; 3]);
        let second = (1_u16..=20)
            .map(|index| (format!("smokes{index:02}"), [f32::from(index); 3]))
            .collect();
        let mut game = crate::skirmish::tests::game();
        let id = game.spawn(0, 0, [300.0, 300.0], true);
        game.update_body_particles(id);
        let mut event = game.presentation.body_particles.pop().unwrap();
        event.draws = vec![first, second];
        let positions = event.positions("SmokeS");
        assert_eq!(positions.len(), 16);
        assert_eq!(
            positions[0].map(f32::to_bits),
            [1.0_f32; 3].map(f32::to_bits)
        );
        assert_eq!(
            positions[1].map(f32::to_bits),
            [1.0_f32; 3].map(f32::to_bits)
        );
        assert_eq!(
            positions[15].map(f32::to_bits),
            [15.0_f32; 3].map(f32::to_bits)
        );
    }
}
