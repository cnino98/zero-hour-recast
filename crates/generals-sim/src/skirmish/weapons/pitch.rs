//! Original `Weapon::isWithinTargetPitch` and `GeometryInfo::calcPitches`.
use super::{Skirmish, WeaponRules};
use crate::collision_geometry::Shape;

impl Skirmish {
    pub(in crate::skirmish) fn any_weapon_within_target_pitch(
        &self,
        id: usize,
        target: usize,
    ) -> bool {
        (0..3).any(|slot| {
            self.allocated_weapon(id, slot)
                .is_some_and(|weapon| self.within_target_pitch(id, target, weapon))
        })
    }

    pub(in crate::skirmish) fn within_target_pitch(
        &self,
        id: usize,
        target: usize,
        weapon: &WeaponRules,
    ) -> bool {
        // RATIONALIZE_ATTACK_RANGE is defined in the pinned Weapon.cpp.
        if super::super::CELL_SIZE.mul_add(-0.25, weapon.range) < super::super::CELL_SIZE
            || (weapon.target_pitch[0] <= -std::f32::consts::PI
                && weapon.target_pitch[1] >= std::f32::consts::PI)
        {
            return true;
        }
        let source = self.object_position(id);
        let target_position = self.object_position(target);
        if (target_position[2] - source[2]).abs() < 10.0 {
            return true;
        }
        let source_type = &self.catalog[self.entities[id].type_index];
        let target_type = &self.catalog[self.entities[target].type_index];
        let source_center = source[2] + source_type.geometry_bounds.center_height;
        let dx = target_position[0] - source[0];
        let dy = target_position[1] - source[1];
        let xx = dx * dx;
        let yy = dy * dy;
        let horizontal = (xx + yy).sqrt();
        let below = if matches!(target_type.geometry.shape, Shape::Sphere) {
            target_type.geometry.major
        } else {
            0.0
        };
        let upper = (target_position[2] + target_type.geometry_bounds.max_height - source_center)
            .atan2(horizontal);
        let lower = (target_position[2] - below - source_center).atan2(horizontal);
        overlaps([lower, upper], weapon.target_pitch)
    }
}

fn overlaps([minimum, maximum]: [f32; 2], [allowed_min, allowed_max]: [f32; 2]) -> bool {
    (minimum >= allowed_min && minimum <= allowed_max)
        || (maximum >= allowed_min && maximum <= allowed_max)
        || (minimum <= allowed_min && maximum >= allowed_max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::tests::game;

    #[test]
    fn native_height_bypass_is_strict_and_geometry_extent_can_intersect_pitch_range() {
        let mut game = game();
        game.catalog[5].weapon.target_pitch = [45.0_f32.to_radians(), 80.0_f32.to_radians()];
        let source = game.spawn(5, 0, [100.0, 100.0], true);
        let target = game.spawn(6, 1, [200.0, 100.0], true);
        game.set_object_position(target, [200.0, 100.0, 9.999]);
        assert!(game.within_target_pitch(source, target, &game.catalog[5].weapon));
        game.set_object_position(target, [200.0, 100.0, 10.0]);
        assert!(!game.within_target_pitch(source, target, &game.catalog[5].weapon));
        game.catalog[6].geometry_bounds.max_height = 150.0;
        assert!(game.within_target_pitch(source, target, &game.catalog[5].weapon));
        game.set_object_position(target, [200.0, 100.0, -200.0]);
        assert!(!game.within_target_pitch(source, target, &game.catalog[5].weapon));
        game.catalog[5].weapon.range = 10.0;
        assert!(game.within_target_pitch(source, target, &game.catalog[5].weapon));
    }

    #[test]
    fn pitch_intersection_includes_exact_endpoints_and_enclosing_target_extent() {
        let allowed = [0.5, 1.0];
        assert!(overlaps([0.0, 0.5], allowed));
        assert!(overlaps([1.0, 1.5], allowed));
        assert!(overlaps([0.0, 1.5], allowed));
        assert!(!overlaps([-0.5, 0.49], allowed));
        assert!(!overlaps([1.01, 1.5], allowed));
    }

    #[test]
    fn admission_and_slot_choice_filter_pitch_without_changing_combined_anti_mask() {
        let mut game = game();
        game.catalog[5].weapon.target_pitch = [45.0_f32.to_radians(), 80.0_f32.to_radians()];
        game.catalog[6].preferred_height = 20.0;
        game.catalog[5].weapon.anti_mask |= 0x01;
        let source = game.spawn(5, 0, [100.0, 100.0], true);
        let target = game.spawn(6, 1, [200.0, 100.0], true);
        assert!(game.weapon_can_target(source, target));
        assert!(!game.weapon_target_reachable(source, target));
        assert!(!game.choose_weapon(source, target));
        assert!(game.attack(source, target).is_err());
        // isAnyWithinTargetPitch examines allocated weapons independently of
        // their damage/anti-mask; chooseBestWeapon has its separate filters.
        game.catalog[5].secondary_weapon = Some(WeaponRules::unarmed());
        assert!(game.weapon_target_reachable(source, target));
        assert!(!game.choose_weapon(source, target));
        game.catalog[5].secondary_weapon = None;
        game.switch_weapon(source, 0).unwrap();
        assert!(game.choose_weapon(source, target));
        assert!(!game.weapon_target_reachable(source, target));
        game.catalog[5].weapon.target_pitch = [-std::f32::consts::PI, std::f32::consts::PI];
        assert!(game.weapon_target_reachable(source, target));
        assert!(game.choose_weapon(source, target));
    }
}
