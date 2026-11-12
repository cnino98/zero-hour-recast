//! `WeaponStore::update` follows pending Object destruction at frame end.
use crate::skirmish::{Skirmish, damage::Source};

impl Skirmish {
    pub(in crate::skirmish) fn update_direct_weapon_damage(&mut self) {
        let mut index = 0;
        while index < self.projectiles.len() {
            let projectile = &self.projectiles[index];
            let Some(direct) = &projectile.direct else {
                index += 1;
                continue;
            };
            if self.update_frame() < direct.deadline {
                index += 1;
                continue;
            }
            let serial = projectile.id;
            let source = projectile.source;
            let binding = crate::skirmish::WeaponBinding {
                type_index: projectile.weapon_type,
                weapon_index: projectile.weapon_index,
                weapon_slot: projectile.weapon_slot,
            };
            let target = projectile.target;
            let position = direct.position;
            let radius = direct.radius;
            let bonus = projectile.damage_bonus;
            self.deal_bound_weapon_damage_with_radius(
                &Source::Object(source),
                binding,
                target,
                position,
                bonus,
                radius,
            );
            // The native list entry remains present until its effects return.
            let retired = self.projectile_index(serial).expect("delayed damage entry");
            self.projectiles.remove(retired);
            self.rebuild_projectile_indices();
        }
    }
}
