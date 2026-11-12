//! Immutable callback inputs keep the original projectile registered during effects.
use crate::skirmish::{Projectile, ProjectileFxEvent, ProjectileFxStage, Skirmish, WeaponFxEvent};

pub(in crate::skirmish) struct ProjectileEvent {
    pub weapon_type: usize,
    pub weapon_index: usize,
    pub weapon_slot: usize,
    pub id: u64,
    pub owner: usize,
    pub source: usize,
    pub target: Option<usize>,
    pub hazard_target: Option<u64>,
    pub veterancy: u8,
    transform: Option<([f32; 3], [[f32; 3]; 3])>,
}

impl Projectile {
    pub(in crate::skirmish) fn callback_snapshot(
        &self,
        entities: &[crate::skirmish::Entity],
    ) -> ProjectileEvent {
        ProjectileEvent {
            weapon_type: self.weapon_type,
            weapon_index: self.weapon_index,
            weapon_slot: self.weapon_slot,
            id: self.id,
            owner: self.owner,
            source: self.source,
            target: self.target,
            hazard_target: self.hazard_target,
            veterancy: self.veterancy,
            transform: self.effect_transform(entities),
        }
    }
}

impl ProjectileEvent {
    pub(in crate::skirmish) const fn effect_transform(&self) -> Option<([f32; 3], [[f32; 3]; 3])> {
        self.transform
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn record_projectile_event(
        &mut self,
        projectile: &ProjectileEvent,
        position: [f32; 3],
        stage: ProjectileFxStage,
    ) {
        if let Some((_, axes)) = projectile.transform {
            self.presentation
                .weapon_fx
                .push(WeaponFxEvent::Projectile(ProjectileFxEvent {
                    weapon_type: projectile.weapon_type,
                    weapon_index: projectile.weapon_index,
                    object: None,
                    object_geometry_radius: None,
                    weapon_slot: projectile.weapon_slot,
                    projectile: projectile.id,
                    source: projectile.source,
                    position,
                    axes,
                    veterancy: projectile.veterancy,
                    stage,
                }));
        }
    }
}
