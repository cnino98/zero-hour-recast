//! `DumbProjectileBehavior::onCollide` retains the Object through all effects.
use super::{Control, FlightRules, ProjectileEvent};
use crate::skirmish::{ProjectileFxStage, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn projectile_contact(
        &mut self,
        id: usize,
        other: Option<usize>,
    ) -> bool {
        let unit = &self.catalog[self.entities[id].type_index];
        if !unit.kind_of.iter().any(|kind| kind == "PROJECTILE")
            || !unit
                .behavior_modules
                .iter()
                .any(|module| module.family == "PhysicsBehavior")
        {
            return false;
        }
        let Some(control) = self.entities[id]
            .projectile_flight
            .as_ref()
            .and_then(|flight| flight.controls.first())
        else {
            return false;
        };
        match control {
            Control::Shell(_) => self.resolve_shell_contact(id, 0, other),
            Control::Missile(_) => {
                if let Some(other) = other {
                    self.resolve_missile_contact(id, 0, other);
                } else {
                    self.missile_ground_contact(id, 0);
                }
            }
        }
        true
    }

    pub(in crate::skirmish::projectiles) fn resolve_shell_contact(
        &mut self,
        id: usize,
        ordinal: usize,
        other: Option<usize>,
    ) {
        let flight = self.entities[id]
            .projectile_flight
            .as_ref()
            .expect("shell object");
        if flight.no_collisions() {
            return;
        }
        let Control::Shell(shell) = &flight.controls[ordinal] else {
            unreachable!()
        };
        let Some(index) = shell
            .launched
            .then(|| self.projectile_component(id))
            .flatten()
        else {
            return;
        };
        let projectile = &self.projectiles[index];
        let FlightRules::Shell(rules) =
            &self.catalog[self.entities[id].type_index].projectile_rules[ordinal]
        else {
            unreachable!()
        };
        if let Some(target) = other {
            let mask = self.catalog[projectile.weapon_type]
                .weapon_template(projectile.weapon_index)
                .and_then(|weapon| weapon.motion.as_ref())
                .expect("launched shell weapon")
                .collide_mask;
            if !self.projectile_collides_with(target, projectile.source, projectile.target, mask) {
                return;
            }
            if let Some(clear) = rules
                .contact
                .as_ref()
                .and_then(|contact| contact.clear.as_ref())
                && clear.count > 0
                && self.clearable_garrison(target)
            {
                let count = clear.count;
                let allowed = clear.allowed.clone();
                let forbidden = clear.forbidden.clone();
                let event = projectile.callback_snapshot(&self.entities);
                if self.clear_garrison(event.source, target, count, &allowed, &forbidden) > 0 {
                    self.record_garrison_kill_fx(&event, target);
                    self.destroy_maintenance_object(id);
                    return;
                }
            }
        }
        self.detonate_shell(id, ordinal);
        self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("deferred shell destruction")
            .disable_collisions();
    }

    pub(in crate::skirmish) fn record_garrison_kill_fx(
        &mut self,
        projectile: &ProjectileEvent,
        target: usize,
    ) {
        let (sine, cosine) = self.entities[target].facing.sin_cos();
        self.presentation
            .weapon_fx
            .push(crate::skirmish::WeaponFxEvent::Projectile(
                crate::skirmish::ProjectileFxEvent {
                    weapon_type: projectile.weapon_type,
                    weapon_index: projectile.weapon_index,
                    object: Some(target),
                    object_geometry_radius: Some(self.object_radius(target)),
                    weapon_slot: projectile.weapon_slot,
                    projectile: projectile.id,
                    source: projectile.source,
                    position: self.object_position(target),
                    axes: [[cosine, sine, 0.0], [-sine, cosine, 0.0], [0.0, 0.0, 1.0]],
                    veterancy: projectile.veterancy,
                    stage: ProjectileFxStage::GarrisonKill,
                },
            ));
    }
}
