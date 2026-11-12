//! SH `OpenContain` flag forwarding and `Weapon::computeBonus` table order.
use crate::skirmish::{Entity, Skirmish, WeaponRules};

pub const NAMES: [&str; 27] = [
    "GARRISONED",
    "HORDE",
    "CONTINUOUS_FIRE_MEAN",
    "CONTINUOUS_FIRE_FAST",
    "NATIONALISM",
    "PLAYER_UPGRADE",
    "DRONE_SPOTTING",
    "DEMORALIZED_OBSOLETE",
    "ENTHUSIASTIC",
    "VETERAN",
    "ELITE",
    "HERO",
    "BATTLEPLAN_BOMBARDMENT",
    "BATTLEPLAN_HOLDTHELINE",
    "BATTLEPLAN_SEARCHANDDESTROY",
    "SUBLIMINAL",
    "SOLO_HUMAN_EASY",
    "SOLO_HUMAN_NORMAL",
    "SOLO_HUMAN_HARD",
    "SOLO_AI_EASY",
    "SOLO_AI_NORMAL",
    "SOLO_AI_HARD",
    "TARGET_FAERIE_FIRE",
    "FANATICISM",
    "FRENZY_ONE",
    "FRENZY_TWO",
    "FRENZY_THREE",
];
pub const DAMAGE: usize = 0;
pub const RADIUS: usize = 1;
pub const RANGE: usize = 2;
pub const RATE: usize = 3;
pub const PRE_ATTACK: usize = 4;

pub struct BonusSet {
    pub global: [[f32; 5]; 27],
    pub template: [[f32; 5]; 27],
}

impl BonusSet {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            global: [[1.0; 5]; 27],
            template: [[1.0; 5]; 27],
        }
    }

    #[must_use]
    pub fn compute(&self, flags: u32, field: usize) -> f32 {
        let mut value = 1.0;
        for table in [&self.global, &self.template] {
            for (index, row) in table.iter().enumerate() {
                if flags & (1 << index) != 0 {
                    let addition = row[field] - 1.0;
                    value += addition;
                }
            }
        }
        value
    }
}

impl Entity {
    pub(in crate::skirmish) fn weapon_bonus_flags(&self) -> u32 {
        let mut flags = u32::from(self.garrison_weapon_bonus());
        let horde = u32::from(self.horde_bonuses());
        flags |= ((horde & 1) << 1) | ((horde & 2) << 3) | ((horde & 4) << 21);
        flags |= self.firing.continuous_flags;
        flags |= u32::from(self.weapon_upgrade_active()) << 5;
        flags |= u32::from(self.firing.drone_spotting) << 6;
        flags |= u32::from(self.attachment.bonuses & 1 != 0) << 8;
        flags |= u32::from(self.firing.veterancy_flags) << 9;
        flags |= u32::from(self.battle_bonuses.mask) << 12;
        flags |= u32::from(self.attachment.bonuses & 2 != 0) << 15;
        flags |= u32::from(self.firing.painted_target) << 22;
        flags | self.helpers.temporary.flags
    }

    pub(in crate::skirmish) fn weapon_factor(
        &self,
        weapon: &WeaponRules,
        extra: u32,
        field: usize,
    ) -> f32 {
        weapon
            .bonuses
            .compute(self.weapon_bonus_flags() | extra, field)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn passenger_bonus_flags(&self, id: usize) -> u32 {
        self.entities[id]
            .contained_by()
            .filter(|&carrier| {
                self.container_rules(carrier)
                    .is_some_and(|rules| rules.weapons.pass_bonuses)
            })
            .map_or(0, |carrier| self.entities[carrier].weapon_bonus_flags())
    }

    pub(in crate::skirmish) fn weapon_factor(
        &self,
        id: usize,
        weapon: &WeaponRules,
        field: usize,
    ) -> f32 {
        self.entities[id].weapon_factor(weapon, self.passenger_bonus_flags(id), field)
    }

    pub(in crate::skirmish) fn effective_weapon_range(&self, id: usize) -> f32 {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        actor.weapon_range(unit, self.passenger_bonus_flags(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn riders() -> (Skirmish, usize, usize) {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
        let index = game.entities[carrier].type_index;
        game.catalog[index]
            .contain
            .as_mut()
            .unwrap()
            .weapons
            .pass_bonuses = true;
        let weapon = &mut game.catalog[5].weapon;
        weapon.bonuses.global[9] = [1.25; 5];
        weapon.bonuses.template[9] = [1.5; 5];
        weapon.bonuses.global[10] = [1.5; 5];
        weapon.bonuses.template[10] = [1.25; 5];
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.board(rider, carrier);
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
        (game, carrier, rider)
    }

    #[test]
    fn carrier_promotion_is_dynamic_same_rank_is_once_and_distinct_ranks_both_append() {
        let (mut game, carrier, rider) = riders();
        game.set_veterancy(rider, 1);
        game.set_veterancy(carrier, 1);
        for field in 0..5 {
            assert_eq!(
                game.weapon_factor(rider, &game.catalog[5].weapon, field)
                    .to_bits(),
                1.75_f32.to_bits()
            );
        }
        // A carrier edge does not broadcast onWeaponBonusChange to its riders.
        game.entities[rider].weapon_set.weapons[0].cooldown = 7.0;
        game.set_veterancy(carrier, 2);
        assert_eq!(
            game.entities[rider].weapon_set.weapons[0]
                .cooldown
                .to_bits(),
            7.0_f32.to_bits()
        );
        for field in 0..5 {
            assert_eq!(
                game.weapon_factor(rider, &game.catalog[5].weapon, field)
                    .to_bits(),
                2.5_f32.to_bits()
            );
        }
        let extra = game.passenger_bonus_flags(rider);
        let expected = ((game.catalog[5].weapon.delay * 30.0).ceil() / 2.5)
            .floor()
            .max(1.0);
        game.entities[rider].spend_weapon_round(&game.catalog[5], game.tick, extra);
        assert_eq!(
            game.entities[rider].weapon_set.weapons[0]
                .cooldown
                .to_bits(),
            expected.to_bits()
        );
        game.remove_passenger(rider, carrier);
        assert_eq!(game.passenger_bonus_flags(rider), 0);
        assert_eq!(
            game.weapon_factor(rider, &game.catalog[5].weapon, DAMAGE)
                .to_bits(),
            1.75_f32.to_bits()
        );
    }

    fn apply_morale(game: &mut Skirmish, carrier: usize, rider: usize) {
        use crate::skirmish::{Faction, Kind, horde, overlord::Propaganda};
        game.remove_passenger(rider, carrier);
        for id in [rider, carrier] {
            let index = game.entities[id].type_index;
            game.catalog[index].behavior.horde = Some(horde::Rules {
                update_frames: 30,
                count: 2,
                radius: 80.0,
                rub_off_radius: 20.0,
                // Native isKindOfMulti requires every listed flag; an empty
                // mask lets this mixed two-member formation count both kinds.
                kind_of: Vec::new(),
                allies_only: true,
                exact_match: false,
                allowed_nationalism: true,
                action: horde::Action::Classic,
            });
        }
        game.players[0].upgrades.extend([
            "Upgrade_Nationalism".into(),
            "Upgrade_Fanaticism".into(),
            "Subliminal".into(),
        ]);
        game.update_ground_contacts();
        game.tick = 60;
        game.update_horde_module(rider);
        game.update_horde_module(carrier);
        for id in [rider, carrier] {
            assert_eq!(game.entities[id].horde_bonuses(), 7);
        }
        let carrier_type = game.entities[carrier].type_index;
        game.catalog[carrier_type].weapon.damage = 1.0;
        let mut tower = crate::skirmish::tests::fixture_type(Faction::China, Kind::Defense);
        tower.kind_of = vec!["STRUCTURE".into()];
        tower.behavior.propaganda = Some(Propaganda {
            radius: 200.0,
            delay: 1,
            heal: [0.0; 2],
            upgrade: "Subliminal".into(),
            pulse: ["Pulse".into(), "SubliminalPulse".into()],
            affects_self: false,
        });
        let index = game.catalog.len();
        game.catalog.push(tower);
        let tower = game.spawn(index, 0, [355.0, 255.0], true);
        game.update_ground_contacts();
        game.update_propaganda_module(tower);
        for id in [rider, carrier] {
            assert_eq!(game.entities[id].attachment.bonuses, 3);
        }
        game.board(rider, carrier);
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    }

    #[test]
    fn horde_and_propaganda_flags_union_once_and_use_the_passenger_weapon_tables() {
        let (mut game, carrier, rider) = riders();
        apply_morale(&mut game, carrier, rider);
        for flag in [1, 4, 23, 8, 15] {
            game.catalog[5].weapon.bonuses.global[flag] = [1.25; 5];
            game.catalog[5].weapon.bonuses.template[flag] = [1.125; 5];
            let index = game.entities[carrier].type_index;
            game.catalog[index].weapon.bonuses.global[flag] = [9.0; 5];
        }
        for field in 0..5 {
            assert_eq!(
                game.weapon_factor(rider, &game.catalog[5].weapon, field)
                    .to_bits(),
                2.875_f32.to_bits()
            );
        }
    }

    #[test]
    fn launch_snapshot_keeps_carrier_conditions_after_unload_and_later_promotion() {
        let (mut game, carrier, rider) = riders();
        game.set_veterancy(rider, 1);
        game.set_veterancy(carrier, 2);
        let saved = game.entities[rider].damage_snapshot(game.passenger_bonus_flags(rider));
        game.remove_passenger(rider, carrier);
        game.set_veterancy(carrier, 3);
        assert_eq!(
            saved.factor(&game.catalog[5].weapon, 0, DAMAGE).to_bits(),
            2.5_f32.to_bits()
        );
        assert_eq!(
            saved.factor(&game.catalog[5].weapon, 0, RADIUS).to_bits(),
            2.5_f32.to_bits()
        );
        assert_eq!(
            game.entities[rider]
                .weapon_damage_bonus(game.entities[rider].active_weapon(&game.catalog[5]), 0)
                .to_bits(),
            1.75_f32.to_bits()
        );
    }

    #[test]
    fn passenger_preattack_and_area_radius_consume_the_same_live_carrier_flags() {
        let (mut game, carrier, rider) = riders();
        game.set_veterancy(carrier, 1);
        game.catalog[5].weapon.pre_attack_frames = 4;
        game.catalog[5].weapon.area = Some(crate::skirmish::AreaDamage {
            primary_radius: 10.0,
            secondary_radius: 0.0,
            secondary_damage: 0.0,
            affects: 2,
            at_source: false,
        });
        game.catalog[5].weapon.damage = 10.0;
        game.catalog[6].geometry_bounds.sphere_radius = 0.0;
        let target = game.spawn(6, 1, [400.0, 300.0], true);
        game.tick = 10;
        game.prefire_native_weapon(rider, Some(target));
        assert!(matches!(
            game.entities[rider].weapon_status(0, &game.catalog[5], 10),
            Some(crate::skirmish::ai_dispatch::WeaponStatus::PreAttack)
        ));
        assert!(matches!(
            game.entities[rider].weapon_status(0, &game.catalog[5], 16),
            Some(crate::skirmish::ai_dispatch::WeaponStatus::PreAttack)
        ));
        assert!(matches!(
            game.entities[rider].weapon_status(0, &game.catalog[5], 17),
            Some(crate::skirmish::ai_dispatch::WeaponStatus::Ready)
        ));
        let health = game.entities[target].health;
        let center = game.target_center(target);
        game.update_ground_contacts();
        game.deal_bound_weapon_damage(
            rider,
            crate::skirmish::WeaponBinding {
                type_index: game.entities[rider].type_index,
                weapon_index: 0,
                weapon_slot: game.entities[rider].weapon_slot(),
            },
            None,
            [center[0] - 15.0, center[1], center[2]],
            1.75,
        );
        assert_eq!(
            game.entities[target].health.to_bits(),
            (health - 17.5).to_bits()
        );
    }
    #[test]
    fn native_veterancy_edges_restart_shared_reload_after_body_change_and_dead_units_keep_flags() {
        let mut game = crate::skirmish::tests::game();
        game.catalog[5].weapon_set_rules.share_reload = true;
        game.catalog[5].weapon.delay = 1.0;
        game.catalog[5].weapon.reload_seconds = 6.0;
        game.catalog[5].weapon.clip_size = 1;
        game.catalog[5].weapon.bonuses.global[9][RATE] = 2.0;
        game.catalog[5].weapon.bonuses.global[11][RATE] = 4.0;
        let id = game.spawn(5, 0, [300.0, 300.0], true);
        game.set_veterancy(id, 3);
        let position = game.object_position(id);
        let target = crate::skirmish::damage::WeaponTarget {
            object: None,
            position,
            projectile_position: position,
            scatter_radius: 0.0,
        };
        assert!(game.fire_current_weapon_payload(id, target).is_some());
        game.set_veterancy(id, 1);
        // Setting VETERAN while HERO remains marks the shared slot reloading;
        // clearing HERO then restarts its full clip delay, not its shot delay.
        assert!(matches!(
            game.entities[id].weapon_set.weapons[0].status,
            crate::skirmish::ai_dispatch::WeaponStatus::Reloading
        ));
        assert_eq!(
            game.entities[id].weapon_set.weapons[0].cooldown.to_bits(),
            90.0_f32.to_bits()
        );
        assert_eq!(game.entities[id].weapon_bonus_flags() & (7 << 9), 1 << 9);
        game.kill_object(id, crate::skirmish::DeathType::Normal);
        game.set_veterancy(id, 3);
        assert_eq!(game.entities[id].veterancy, 3);
        assert_eq!(game.entities[id].weapon_bonus_flags() & (7 << 9), 1 << 9);
    }

    #[test]
    fn global_conditions_append_before_template_conditions_in_native_float_order() {
        let mut tables = BonusSet::none();
        tables.global[0][DAMAGE] = 1.1;
        tables.global[9][DAMAGE] = 1.1;
        tables.template[0][DAMAGE] = 2.0;
        // Global GARRISONED, global VETERAN, then local GARRISONED. Interleaving
        // each condition's two tables instead produces 0x400ccccc.
        assert_eq!(tables.compute(1 | (1 << 9), DAMAGE).to_bits(), 0x400c_cccd);
    }
}
