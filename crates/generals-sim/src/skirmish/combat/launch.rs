//! `WeaponTemplate` creates the native Object before assigning producer and launching.
use crate::skirmish::firing::WeaponBinding;
use crate::skirmish::{Projectile, Skirmish, damage::WeaponTarget, projectiles::DirectHit};

impl Skirmish {
    pub(in crate::skirmish) fn register_delivered_projectile(
        &mut self,
        carrier: usize,
        payload: usize,
        binding: WeaponBinding,
        destination: [f32; 3],
    ) {
        // The direct ProjectileUpdate fire path never sets its launcher bonus
        // flags. The frame's actual payload flags are read at impact.
        let actor = &self.entities[payload];
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("delivery weapon");
        self.projectile_serial = self.projectile_serial.wrapping_add(1);
        let serial = self.projectile_serial;
        self.projectiles.push(Projectile {
            object: Some(payload),
            direct: None,
            hazard_target: None,
            weapon_type: binding.type_index,
            weapon_index: binding.weapon_index,
            weapon_slot: binding.weapon_slot,
            id: serial,
            position: actor.position,
            destination: [destination[0], destination[1]],
            owner: actor.owner,
            target: None,
            source: carrier,
            veterancy: self.entities[carrier].veterancy,
            damage_bonus: weapon
                .bonuses
                .compute(0, crate::skirmish::containment::bonuses::DAMAGE),
            damage_state: crate::skirmish::battle_plan::Damage::unmodified(),
        });
        self.entities[payload].native_projectile = Some(serial);
        self.rebuild_projectile_indices();
    }

    pub(in crate::skirmish) fn create_weapon_projectile(
        &mut self,
        source: usize,
        target: &WeaponTarget,
        binding: WeaponBinding,
        damage_bonus: f32,
        barrel: usize,
    ) {
        let actor = &self.entities[source];
        let extra = self.passenger_bonus_flags(source);
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("fired weapon");
        let native_type = weapon
            .motion
            .as_ref()
            .and_then(|motion| motion.projectile_type);
        let at_source = weapon.area.as_ref().is_some_and(|area| area.at_source);
        let direct_position = if at_source {
            self.object_position(source)
        } else {
            target.position
        };
        let tracked = if target.scatter_radius > 0.0 {
            None
        } else {
            target.object
        };
        // WeaponTemplate passes victimPos to ProjectileUpdateInterface; its
        // geometry-center FX target is a separate value. Individual projectile
        // callbacks may then choose a center (DumbProjectileBehavior does).
        let end = if native_type.is_some() {
            target.projectile_position
        } else {
            tracked.map_or(target.projectile_position, |victim| {
                self.target_center(victim)
            })
        };
        let direct = if native_type.is_none() {
            let frames = self.direct_damage_frames(source, binding, target.position);
            if frames < 1.0 {
                self.deal_bound_weapon_damage(
                    source,
                    binding,
                    if at_source { None } else { target.object },
                    direct_position,
                    damage_bonus,
                );
                return;
            }
            Some(self.delayed_weapon_hit(source, binding, frames, direct_position))
        } else {
            None
        };
        self.projectile_serial = self.projectile_serial.wrapping_add(1);
        let serial = self.projectile_serial;
        let owner = actor.owner;
        self.projectiles.push(Projectile {
            object: None,
            direct,
            hazard_target: None,
            weapon_type: binding.type_index,
            weapon_index: binding.weapon_index,
            weapon_slot: binding.weapon_slot,
            id: serial,
            position: actor.position,
            destination: [end[0], end[1]],
            owner,
            target: if native_type.is_none() {
                if at_source { None } else { target.object }
            } else {
                tracked
            },
            source,
            veterancy: actor.veterancy,
            damage_bonus,
            damage_state: actor.damage_snapshot(extra),
        });
        self.rebuild_projectile_indices();
        let Some(kind) = native_type else {
            return;
        };
        let object = self.spawn_native_projectile(kind, owner, [0.0; 3], serial);
        let index = self
            .projectile_index(serial)
            .expect("projectile birth retained across constructors");
        self.projectiles[index].object = Some(object);
        self.entities[object].producer = Some(source);
        self.transfer_special_power_completion(source, object);
        let snapshot = self.projectiles[index].callback_snapshot(&self.entities);
        self.record_stream_projectile(&snapshot, end);
        self.launch_native_projectile(object, source, binding, barrel, end, tracked);
        let index = self
            .projectile_index(serial)
            .expect("deferred native projectile");
        let snapshot = self.projectiles[index].callback_snapshot(&self.entities);
        if let Some((position, _)) = snapshot.effect_transform() {
            self.record_projectile_event(
                &snapshot,
                position,
                crate::skirmish::ProjectileFxStage::Created,
            );
        }
        self.report_projectile_countermeasures(object, target.object);
    }
    fn delayed_weapon_hit(
        &self,
        source: usize,
        binding: WeaponBinding,
        frames: f32,
        position: [f32; 3],
    ) -> DirectHit {
        let actor = &self.entities[source];
        let extra = self.passenger_bonus_flags(source);
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("fired weapon");
        // REAL_TO_INT_CEIL returns signed 32-bit long. Native x86 conversion
        // returns integer-indefinite for infinity/NaN (authored DEPLOY has
        // WeaponSpeed=0), then WeaponTemplate assigns those bits to unsigned.
        let delay = crate::native_real::ceil(frames);
        DirectHit {
            deadline: self
                .update_frame()
                .wrapping_add(u32::from_ne_bytes(delay.to_ne_bytes())),
            radius: actor.weapon_factor(
                weapon,
                extra,
                crate::skirmish::containment::bonuses::RADIUS,
            ),
            position,
        }
    }
}
