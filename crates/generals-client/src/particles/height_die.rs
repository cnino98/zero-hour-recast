//! `ParticleSystemManager::destroyAttachedSystems` preserves existing particles.
use super::Particles;

impl Particles {
    pub fn destroy_height_die_attachments(&mut self, object: usize) {
        let systems = self
            .emitters
            .iter()
            .filter(|emitter| {
                emitter
                    .object_attachment
                    .as_ref()
                    .is_some_and(|(id, _)| *id == object)
            })
            .map(|emitter| emitter.id)
            .collect::<Vec<_>>();
        for system in systems {
            self.destroy(system);
        }
    }
}
