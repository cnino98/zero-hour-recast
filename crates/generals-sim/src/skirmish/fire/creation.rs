//! Original bounded `OCLCreateObject` dispositions used by incendiary weapons.
use super::{Creation, Disposition, frame};
use crate::skirmish::{Entity, Projectile, Skirmish, startup};

pub(in crate::skirmish) struct Impact {
    source: usize,
    object: usize,
    binding: crate::skirmish::WeaponBinding,
    owner: usize,
    position: [f32; 3],
    facing: f32,
    velocity: [f32; 3],
    bonus: f32,
    radius: f32,
    inflict_damage: bool,
}

impl Impact {
    pub(in crate::skirmish) fn new(
        projectile: &Projectile,
        entities: &[Entity],
        position: [f32; 3],
        bonus: f32,
        radius: f32,
    ) -> Self {
        let flight = projectile
            .object
            .and_then(|id| entities[id].projectile_flight.as_ref());
        let (facing, velocity) = flight.map_or((0.0, [0.0; 3]), |flight| {
            (
                flight.motion.axes[0][1].atan2(flight.motion.axes[0][0]),
                flight.motion.velocity,
            )
        });
        Self {
            source: projectile.source,
            object: projectile.object.expect("native projectile impact object"),
            binding: crate::skirmish::WeaponBinding {
                type_index: projectile.weapon_type,
                weapon_index: projectile.weapon_index,
                weapon_slot: projectile.weapon_slot,
            },
            owner: projectile.owner,
            position,
            facing,
            velocity,
            bonus,
            radius,
            inflict_damage: true,
        }
    }

    pub(in crate::skirmish) const fn without_damage(mut self) -> Self {
        self.inflict_damage = false;
        self
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn apply_projectile_impact(&mut self, impact: &Impact) {
        let weapon = self.catalog[impact.binding.type_index]
            .weapon_template(impact.binding.weapon_index)
            .expect("projectile weapon");
        let creation = weapon
            .incendiary
            .as_ref()
            .and_then(|fire| fire.impact.clone());
        // WeaponTemplate::fireWeaponTemplate emits ProjectileDetonationOCL even
        // when a diverted missile suppresses damage and historic damage.
        if let Some(creation) = creation {
            self.create_fire_objects(
                &creation,
                impact.owner,
                impact.position,
                impact.facing,
                impact.velocity,
            );
        }
        if !impact.inflict_damage {
            return;
        }
        let weapon = self.catalog[impact.binding.type_index]
            .weapon_template(impact.binding.weapon_index)
            .expect("projectile weapon");
        let history = weapon
            .incendiary
            .as_ref()
            .and_then(|fire| fire.historic.as_ref())
            .and_then(|historic| {
                self.fire_history
                    .entry(weapon.name.clone())
                    .or_default()
                    .impact(
                        &historic.rules,
                        frame(self.tick),
                        [impact.position[0], impact.position[1]],
                    )
                    .then(|| historic.creation.clone())
            });
        if let Some(creation) = history {
            self.create_fire_objects(
                &creation,
                impact.owner,
                impact.position,
                impact.facing,
                impact.velocity,
            );
        }
        self.deal_bound_weapon_damage_with_radius(
            &crate::skirmish::damage::Source::Projectile {
                launcher: impact.source,
                object: impact.object,
            },
            impact.binding,
            None,
            impact.position,
            impact.bonus,
            impact.radius,
        );
    }

    pub(in crate::skirmish) fn create_fire_objects(
        &mut self,
        creation: &Creation,
        owner: usize,
        position: [f32; 3],
        facing: f32,
        velocity: [f32; 3],
    ) {
        for _ in 0..creation.count {
            let last = u32::try_from(creation.names.len() - 1).expect("source object list");
            let index = usize::try_from(self.random_between(last)).expect("source object pick");
            let name = &creation.names[index];
            let kind = self
                .catalog
                .iter()
                .position(|unit| &unit.name == name)
                .expect("validated OCL object");
            let id = self.spawn(kind, owner, [position[0], position[1]], true);
            match creation.disposition {
                Disposition::GroundAligned => {
                    let angle = startup::random_real(&mut self.random, 0.0, std::f32::consts::TAU);
                    self.set_object_orientation(id, angle);
                }
                Disposition::ExistingWithVelocity => {
                    self.set_object_orientation(id, facing);
                    self.entities[id].fire.as_mut().expect("fire object").height = position[2];
                    self.set_flight_position(id, position);
                    self.apply_flight_force(id, velocity);
                }
            }
            self.observe_ground_partition(id);
        }
    }
}
