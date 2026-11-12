//! `SlavedUpdate` welding systems use pristine bones and a per-particle lifetime.
use super::{Particles, Pose};
use crate::model_animation::Animations;
use generals_sim::skirmish::Skirmish;

pub(super) struct Weld {
    template: String,
    position: [f32; 3],
    lifetime: f32,
}

impl Particles {
    pub fn slave_weld_event(
        &mut self,
        game: &Skirmish,
        generation: u32,
        index: usize,
        animations: &mut Animations,
    ) {
        self.begin(generation, game.tick);
        let event = &game.presentation.slave_welds[index];
        let repair = game.catalog[event.type_index].behavior.slaves[event.ordinal]
            .repair
            .as_ref()
            .expect("SlavedUpdate welding data");
        let weld = Weld {
            template: repair.welding_system.clone(),
            position: animations.drone_weld_position(
                event.type_index,
                &repair.welding_bone,
                &event.weld,
            ),
            lifetime: event.weld.lifetime,
        };
        self.drone_welds(
            std::slice::from_ref(&weld),
            &mut super::random(animations, generation),
        );
    }

    pub(super) fn drone_welds(&mut self, welds: &[Weld], random: &mut impl FnMut(f32, f32) -> f32) {
        for weld in welds {
            let Some(index) = self
                .templates
                .iter()
                .position(|template| template.name == weld.template)
            else {
                continue;
            };
            self.create(index, Pose::at(weld.position), None, random);
            self.emitters
                .last_mut()
                .expect("created drone welding system")
                .lifetime = Some(weld.lifetime);
            if !self.repair_sound.is_empty() {
                self.sounds.push((self.repair_sound.clone(), weld.position));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_welding_template_skips_system_sound_and_random_draws() {
        let mut particles = super::super::tests::system(Vec::new());
        particles.repair_sound = "RepairSparks".into();
        let mut draws = 0;
        particles.drone_welds(
            &[Weld {
                template: "Missing".into(),
                position: [0.0; 3],
                lifetime: 240.0,
            }],
            &mut |low, _| {
                draws += 1;
                low
            },
        );
        assert!(particles.emitters.is_empty());
        assert!(particles.sounds.is_empty());
        assert_eq!(particles.next_id, 0);
        assert_eq!(draws, 0);
    }

    #[test]
    fn welding_overrides_particle_lifetime_without_extending_emission_or_drawing_lifetime_rng() {
        let mut template = super::super::tests::template("BlueSparks");
        template.lifetime.low = 4.0;
        template.lifetime.high = 6.0;
        template.alpha[1].frame = 100;
        let mut particles = super::super::tests::system(vec![template]);
        particles.repair_sound = "RepairSparks".into();
        let mut sampled_lifetime = false;
        let mut random = |low: f32, high: f32| {
            sampled_lifetime |=
                low.to_bits() == 4.0_f32.to_bits() && high.to_bits() == 6.0_f32.to_bits();
            high
        };
        particles.drone_welds(
            &[Weld {
                template: "BlueSparks".into(),
                position: [10.0, 20.0, 30.0],
                lifetime: 240.0,
            }],
            &mut random,
        );
        particles.logic(10, &mut random, &|_| 0.0);
        assert_eq!(
            particles.emitters[0].particles[0].remaining.to_bits(),
            239.0f32.to_bits()
        );
        assert_eq!(particles.emitters[0].remaining.to_bits(), 0.0f32.to_bits());
        assert_eq!(particles.sounds.len(), 1);
        assert_eq!(
            particles.sounds[0].1.map(f32::to_bits),
            [10.0f32, 20.0, 30.0].map(f32::to_bits)
        );
        particles.logic(11, &mut random, &|_| 0.0);
        assert_eq!(particles.emitters[0].particles.len(), 1);
        assert!(!sampled_lifetime);
    }
}
