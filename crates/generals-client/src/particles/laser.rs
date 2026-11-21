//! `LaserUpdate` creates one emitter at each end and retires them on deletion.
use super::{Particles, Pose, random};
use crate::model_animation::Animations;

impl Particles {
    pub fn particle_uplink_emitter(
        &mut self,
        name: &str,
        matrix: [[f32; 4]; 3],
        animations: &mut Animations,
        generation: u32,
        frame: u64,
    ) -> Option<u64> {
        self.begin(generation, frame);
        let template = self
            .templates
            .iter()
            .position(|template| template.name == name)?;
        let pose = Pose {
            position: matrix.map(|row| row[3]),
            axes: std::array::from_fn(|column| std::array::from_fn(|row| matrix[row][column])),
        };
        Some(self.create(template, pose, None, &mut random(animations, generation)))
    }

    pub fn particle_uplink_scorch(
        &mut self,
        battle: &crate::playtest::Match,
        index: usize,
        animations: &mut Animations,
    ) {
        self.begin(battle.generation, battle.simulation.tick);
        let scorch = &battle.simulation.presentation.particle_uplink_scorches[index];
        self.scorches.add(
            scorch.position,
            scorch.radius,
            animations.effect_scorch(battle.generation),
        );
    }

    pub fn optional_laser_emitters(
        &mut self,
        names: &[String; 2],
        points: [[f32; 3]; 2],
        animations: &mut Animations,
        generation: u32,
        frame: u64,
    ) -> [Option<u64>; 2] {
        self.begin(generation, frame);
        let mut random = random(animations, generation);
        std::array::from_fn(|index| {
            if names[index].is_empty() || names[index].eq_ignore_ascii_case("NONE") {
                return None;
            }
            let template = self
                .templates
                .iter()
                .position(|template| template.name == names[index])?;
            Some(self.create(template, Pose::at(points[index]), None, &mut random))
        })
    }

    pub fn position_laser_emitters(
        &mut self,
        systems: [Option<u64>; 2],
        points: [[f32; 3]; 2],
        visible: bool,
    ) {
        for (id, point) in systems.into_iter().zip(points) {
            if let Some(id) = id
                && let Some(emitter) = self.emitters.iter_mut().find(|emitter| emitter.id == id)
            {
                emitter.transform = Pose::at(point);
                emitter.shrouded = !visible;
            }
        }
    }

    pub fn retire_optional_laser_emitters(&mut self, systems: [Option<u64>; 2]) {
        for id in systems.into_iter().flatten() {
            self.destroy(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::tests::{system, template};

    #[test]
    fn laser_retirement_destroys_endpoint_slave_chains_without_destroying_unrelated_effects() {
        let mut muzzle = template("muzzle");
        muzzle.slave = Some(1);
        let mut particles = system(vec![muzzle, template("slave"), template("unrelated")]);
        let mut random = |low: f32, _high: f32| low;
        let muzzle = particles.create(0, Pose::at([0.0; 3]), None, &mut random);
        let unrelated = particles.create(2, Pose::at([0.0; 3]), None, &mut random);
        let slave = particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == muzzle)
            .unwrap()
            .slave
            .unwrap();
        particles.retire_optional_laser_emitters([Some(muzzle), None]);
        for id in [muzzle, slave] {
            assert!(
                particles
                    .emitters
                    .iter()
                    .find(|emitter| emitter.id == id)
                    .unwrap()
                    .lifecycle
                    .destroyed
            );
        }
        assert!(
            !particles
                .emitters
                .iter()
                .find(|emitter| emitter.id == unrelated)
                .unwrap()
                .lifecycle
                .destroyed
        );
    }
}
