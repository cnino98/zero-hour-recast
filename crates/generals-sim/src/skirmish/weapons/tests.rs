use super::Lock;
use crate::skirmish::{
    DamageType, Faction, Kind, Order, Skirmish, WeaponMotion,
    ai_dispatch::WeaponStatus,
    tests::{fixture_type, game},
};

#[test]
fn original_self_damage_weapons_allow_zero_range_delay_and_no_rotating_actor() {
    let mut unit = fixture_type(Faction::Gla, Kind::Defense);
    unit.locomotor.turn_rate = 0.0;
    unit.turret = None;
    unit.weapon.damage = 5.0;
    unit.weapon.range = 0.0;
    unit.weapon.delay = 0.0;
    unit.weapon.clip_size = 0;
    unit.weapon.reload_seconds = 0.0;
    assert!(super::validate(&unit).is_ok());
    unit.weapon.damage = f32::NAN;
    assert_eq!(super::validate(&unit), Err("Invalid weapon source values"));
    unit.weapon.damage = 5.0;
    unit.weapon.range = -1.0;
    assert_eq!(super::validate(&unit), Err("Invalid weapon source values"));
}

fn armed() -> (crate::skirmish::Skirmish, usize, usize) {
    armed_with_upgrades(&[])
}

fn armed_with_upgrades(triggers: &[&str]) -> (crate::skirmish::Skirmish, usize, usize) {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    let mut secondary = fixture_type(Faction::Gla, Kind::Infantry).weapon;
    secondary.damage = 35.0;
    secondary.delay = 0.0;
    secondary.clip_size = 1;
    secondary.reload_seconds = 2.0;
    secondary.damage_type = DamageType::Surrender;
    secondary.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| Vec::new()),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    game.catalog[5].secondary_weapon = Some(secondary);
    if !triggers.is_empty() {
        let unit = &mut game.catalog[5];
        unit.weapon_upgrade_triggers = triggers.iter().map(|name| (*name).into()).collect();
        // PLAYER_UPGRADE changes template-set identity even when PRIMARY keeps
        // its template; native updateWeaponSet then reallocates all slots.
        unit.weapon_set_rules.replacements = vec![
            super::WeaponTemplateSet {
                flags: 0,
                weapons: [Some(0), None, None],
                share_lock: false,
            },
            super::WeaponTemplateSet {
                flags: 1,
                weapons: [Some(0), Some(1), None],
                share_lock: false,
            },
        ];
        crate::skirmish::tests::register_behavior(unit, "WeaponSetUpgrade", 0x80);
        unit.behavior_modules.last_mut().unwrap().upgrade.triggers =
            unit.weapon_upgrade_triggers.clone();
    }
    let actor = game.spawn(5, 0, [300.0, 100.0], true);
    let target = game.spawn(6, 1, [400.0, 100.0], true);
    (game, actor, target)
}

// AIStates.cpp5181–5213: Fire onEnter only pre-fires; its next update fires.
fn step_first_shot(game: &mut Skirmish, actor: usize) {
    let commanded = game.tick;
    assert!(game.entities[actor].weapon_set.status.aiming);
    assert!(!game.entities[actor].weapon_set.status.firing);
    game.step();
    assert!(game.presentation.shots.is_empty());
    assert!(!game.entities[actor].weapon_set.status.aiming);
    assert!(game.entities[actor].weapon_set.status.firing);
    game.step();
    assert_eq!(game.tick, commanded + 2);
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.presentation.shots[0].source, actor);
    let slot = game.entities[actor].weapon_slot();
    assert_eq!(
        game.entities[actor].weapon_set.weapons[slot].last_shot,
        Some(game.tick)
    );
}

fn set_target_height(game: &mut Skirmish, target: usize, height: f32) {
    let index = game.entities[target].type_index;
    game.catalog[index].locomotor.airborne_targeting_height = 10.0;
    let [x, y] = game.entities[target].position;
    game.set_object_position(target, [x, y, game.ground_height([x, y]) + height]);
    game.update_ground_locomotor(target);
    assert_eq!(game.height_above_ground(target).to_bits(), height.to_bits());
    assert_eq!(game.entities[target].is_airborne_target(), height > 10.0);
}

#[test]
fn switching_retains_per_weapon_reload_and_does_not_refill_ammunition() {
    let (mut game, actor, target) = armed();
    game.entities[actor].weapon_set.weapons[0].status = WeaponStatus::BetweenShots;
    let primary_ready = game.tick + 17;
    game.entities[actor].weapon_set.weapons[0].possible_next_shot = primary_ready;
    game.switch_weapon(actor, 1).unwrap();
    game.attack(actor, target).unwrap();
    step_first_shot(&mut game, actor);
    assert_eq!(game.presentation.shots[0].weapon_slot, 1);
    let secondary_ready = game.tick + 60;
    assert_eq!(
        game.entities[actor].weapon_set.weapons[1].possible_next_shot,
        secondary_ready
    );
    assert_eq!(
        game.entities[actor].weapon_set.weapons[1].last_reload_started,
        game.tick
    );
    assert!(matches!(
        game.entities[actor].weapon_set.weapons[1].status,
        WeaponStatus::Reloading
    ));
    assert_eq!(game.entities[actor].weapon_rounds_remaining(1), 0);
    assert_eq!(primary_ready - game.tick, 15);
    assert_eq!(
        game.entities[actor].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        15.0f32.to_bits()
    );
    game.switch_weapon(actor, 0).unwrap();
    game.switch_weapon(actor, 1).unwrap();
    assert_eq!(
        game.entities[actor].weapon_set.weapons[0].possible_next_shot,
        primary_ready
    );
    assert_eq!(
        game.entities[actor].weapon_set.weapons[1].possible_next_shot,
        secondary_ready
    );
    assert_eq!(game.entities[actor].weapon_rounds_remaining(1), 0);
    game.step();
    assert!(game.presentation.shots.is_empty());
    assert_eq!(secondary_ready - game.tick, 59);
    assert_eq!(
        game.entities[actor].weapon_set.weapons[1]
            .cooldown
            .to_bits(),
        59.0f32.to_bits()
    );
}

#[test]
fn delayed_damage_retains_fired_slot_after_switching_and_uses_surrender_armor() {
    let (mut game, actor, target) = armed();
    game.catalog[6].armor[DamageType::Surrender as usize] = 0.5;
    game.catalog[6].armor[DamageType::ArmorPiercing as usize] = 0.0;
    game.switch_weapon(actor, 1).unwrap();
    game.attack(actor, target).unwrap();
    step_first_shot(&mut game, actor);
    assert_eq!(game.projectiles[0].weapon_slot, 1);
    game.switch_weapon(actor, 0).unwrap();
    game.set_order(actor, Order::Idle);
    for _ in 0..8 {
        game.step();
    }
    assert_eq!(game.entities[target].health.to_bits(), 82.5f32.to_bits());
}

#[test]
fn original_reverse_selection_prefers_ready_damage_and_primary_ties_respects_masks_and_locks() {
    let (mut game, actor, target) = armed();
    game.catalog[5].weapon.damage = 35.0;
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    game.entities[actor].weapon_set.weapons[0].status = WeaponStatus::BetweenShots;
    game.entities[actor].weapon_set.weapons[0].possible_next_shot = game.tick + 1;
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    game.catalog[5]
        .secondary_weapon
        .as_mut()
        .unwrap()
        .auto_choose = 2;
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    game.catalog[5]
        .secondary_weapon
        .as_mut()
        .unwrap()
        .auto_choose = 4;
    // Source tests enum value 4 as the fallback, allowing this player command.
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    game.catalog[5]
        .secondary_weapon
        .as_mut()
        .unwrap()
        .auto_choose = 16;
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    game.catalog[5]
        .secondary_weapon
        .as_mut()
        .unwrap()
        .auto_choose = 4;
    game.entities[actor].ai.attack_context.source = crate::skirmish::builder_disarm::Source::Ai;
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    game.switch_weapon(actor, 0).unwrap();
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    assert!(matches!(
        game.entities[actor].weapon_set.lock,
        Lock::Permanent
    ));
}

#[test]
fn weapon_set_upgrade_replaces_slots_once_and_applies_to_existing_future_and_captured_units() {
    let (mut game, actor, _) = armed_with_upgrades(&["Flashbang"]);
    assert!(!game.weapon_available(actor, 1));
    game.entities[actor].weapon_set.weapons[0].cooldown = 12.0;
    game.players[0].upgrades.push("Flashbang".into());
    game.activate_upgrade_modules(actor);
    assert!(game.weapon_available(actor, 1));
    assert_eq!(
        game.entities[actor].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        0.0f32.to_bits()
    );
    let future = game.spawn(5, 0, [350.0, 200.0], true);
    assert!(game.weapon_available(future, 1));
    game.switch_weapon(actor, 1).unwrap();
    game.entities[actor].weapon_set.weapons[1].cooldown = 15.0;
    game.activate_upgrade_modules(actor);
    game.transfer_maintenance_owner(actor, 1);
    assert!(game.weapon_available(actor, 1));
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    assert_eq!(
        game.entities[actor].weapon_set.weapons[1]
            .cooldown
            .to_bits(),
        15.0f32.to_bits()
    );
}

#[test]
fn airborne_vehicle_rejects_ground_orders_acquisition_and_existing_attacks() {
    let (mut game, actor, target) = armed();
    game.attack(actor, target).unwrap();
    set_target_height(&mut game, target, 100.0);
    game.catalog[5].ai.acquisition |= crate::skirmish::UnitAi::IDLE;
    game.update_visibility();
    assert!(!game.weapon_can_target(actor, target));
    assert!(game.attack(actor, target).is_err());
    assert!(!game.update_attack(actor, target));
    for _ in 0..120 {
        game.step();
        assert!(
            game.presentation
                .shots
                .iter()
                .all(|shot| shot.source != actor)
        );
        assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    }
}

#[test]
fn airborne_boundary_and_individual_slot_masks_choose_the_original_weapon() {
    let (mut game, actor, target) = armed();
    set_target_height(&mut game, target, 10.0);
    game.catalog[5].secondary_weapon.as_mut().unwrap().anti_mask = 0x01;
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    set_target_height(&mut game, target, 10.01);
    assert!(game.weapon_can_target(actor, target));
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    game.attack(actor, target).unwrap();
    step_first_shot(&mut game, actor);
    assert_eq!(game.presentation.shots[0].weapon_slot, 1);
}

#[test]
fn explicit_weapon_lock_remains_after_the_combined_source_mask_gate() {
    let (mut game, actor, target) = armed();
    set_target_height(&mut game, target, 100.0);
    game.catalog[5].secondary_weapon.as_mut().unwrap().anti_mask = 0x01;
    game.switch_weapon(actor, 0).unwrap();
    // Native getAbleToUse uses the total mask; chooseBest returns early when locked.
    assert!(game.weapon_can_target(actor, target));
    assert!(game.choose_weapon(actor, target));
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    game.catalog[5].secondary_weapon.as_mut().unwrap().anti_mask = 0x02;
    assert!(!game.weapon_can_target(actor, target));
    assert!(game.attack(actor, target).is_err());
}

#[test]
fn airborne_status_does_not_reject_already_fired_direct_damage() {
    let (mut game, actor, target) = armed();
    game.switch_weapon(actor, 1).unwrap();
    game.attack(actor, target).unwrap();
    step_first_shot(&mut game, actor);
    game.stop(actor).unwrap();
    set_target_height(&mut game, target, 100.0);
    assert!(!game.weapon_can_target(actor, target));
    for _ in 0..8 {
        game.step();
    }
    assert_eq!(game.entities[target].health.to_bits(), 65.0f32.to_bits());
}

fn rocket_sets() -> (crate::skirmish::Skirmish, usize, usize) {
    rocket_sets_with_upgrades(&["ScorpionRocket"])
}

fn rocket_sets_with_upgrades(triggers: &[&str]) -> (crate::skirmish::Skirmish, usize, usize) {
    let (mut game, actor, target) = armed_with_upgrades(triggers);
    let unit = &mut game.catalog[5];
    unit.salvage.weapons = (0..2)
        .map(|_| fixture_type(Faction::Gla, Kind::Infantry).weapon)
        .collect();
    for (clip, damage) in [(1, 35.0), (2, 60.0)] {
        let mut rules = fixture_type(Faction::Gla, Kind::Infantry).weapon;
        rules.clip_size = clip;
        rules.reload_seconds = 15.0;
        rules.damage = damage;
        unit.weapon_set_rules
            .templates
            .push(super::ConditionalWeapon {
                rules,
                flags: 1,
                slot: 1,
            });
    }
    unit.weapon_set_rules.replacements = [
        (0, [Some(0), None, None]),
        (1, [Some(0), Some(7), None]),
        (2, [Some(3), None, None]),
        (3, [Some(3), Some(7), None]),
        (4, [Some(4), None, None]),
        (5, [Some(4), Some(8), None]),
    ]
    .into_iter()
    .map(|(flags, weapons)| super::WeaponTemplateSet {
        flags,
        weapons,
        share_lock: false,
    })
    .collect();
    game.entities[actor].weapon_set = super::State::new(unit);
    (game, actor, target)
}

#[test]
fn research_and_salvage_choose_all_six_source_sets_in_both_upgrade_orders() {
    for research_first in [true, false] {
        let (mut game, actor, _) = rocket_sets();
        assert!(!game.weapon_available(actor, 1));
        assert!(!game.weapon_available(actor, 3));
        if research_first {
            game.players[0].upgrades.push("ScorpionRocket".into());
            game.activate_upgrade_modules(actor);
            assert_eq!(game.entities[actor].weapon_index(1), 7);
        }
        for level in 1..=2 {
            game.entities[actor].salvage.level = level;
            game.replace_weapon_set(actor);
            assert_eq!(game.entities[actor].weapon_index(0), usize::from(level) + 2);
            assert_eq!(game.weapon_available(actor, 1), research_first);
            if research_first {
                assert_eq!(
                    game.entities[actor].weapon_set.weapons[1].clip_remaining,
                    u32::from(level)
                );
            }
        }
        if !research_first {
            game.players[0].upgrades.push("ScorpionRocket".into());
            game.activate_upgrade_modules(actor);
        }
        assert_eq!(game.entities[actor].weapon_index(1), 8);
        assert_eq!(game.entities[actor].weapon_set.weapons[1].clip_remaining, 2);
        game.switch_weapon(actor, 1).unwrap();
        game.entities[actor].weapon_set.weapons[1].cooldown = 42.0;
        game.activate_upgrade_modules(actor);
        game.replace_weapon_set(actor);
        game.transfer_maintenance_owner(actor, 1);
        assert_eq!(game.entities[actor].weapon_index(1), 8);
        assert_eq!(game.entities[actor].weapon_slot(), 1);
        assert_eq!(
            game.entities[actor].weapon_set.weapons[1]
                .cooldown
                .to_bits(),
            42.0_f32.to_bits()
        );
        let future = game.spawn(5, 0, [350.0, 200.0], true);
        assert_eq!(game.entities[future].weapon_index(1), 7);
    }
}

#[test]
fn conditional_missile_keeps_original_damage_after_new_set_is_allocated() {
    let (mut game, actor, target) = rocket_sets();
    game.players[0].upgrades.push("ScorpionRocket".into());
    game.activate_upgrade_modules(actor);
    game.switch_weapon(actor, 1).unwrap();
    game.attack(actor, target).unwrap();
    step_first_shot(&mut game, actor);
    assert_eq!(game.projectiles[0].weapon_index, 7);
    assert_eq!(game.entities[actor].weapon_rounds_remaining(1), 0);
    game.entities[actor].salvage.level = 2;
    game.replace_weapon_set(actor);
    assert_eq!(game.entities[actor].weapon_index(1), 8);
    game.update_weapon_status(actor);
    assert_eq!(game.entities[actor].weapon_rounds_remaining(1), 2);
    game.set_order(actor, Order::Idle);
    for _ in 0..8 {
        game.step();
    }
    assert_eq!(game.entities[target].health.to_bits(), 65.0_f32.to_bits());
    let state = &mut game.entities[actor].weapon_set.weapons[1];
    state.clip_remaining = 0;
    assert!(state.reload_owned(
        game.catalog[5].weapon_template(8).unwrap(),
        u32::try_from(game.tick).unwrap(),
        1.0,
        false,
        false,
    ));
    let ready = state.possible_next_shot;
    assert_eq!(ready, game.tick + 450);
    // Sample Object's FINAL weapon-status callback at the two native deadlines.
    game.tick = ready - 1;
    game.update_weapon_status(actor);
    assert_eq!(game.entities[actor].weapon_rounds_remaining(1), 0);
    game.tick = ready;
    game.update_weapon_status(actor);
    assert_eq!(game.entities[actor].weapon_rounds_remaining(1), 2);
}

#[test]
fn conditional_set_sparse_match_prefers_flags_then_fewer_extras_and_keeps_first_tie() {
    let sets = [3, 1, 1, 0].map(|flags| super::WeaponTemplateSet {
        flags,
        weapons: [None; 3],
        share_lock: false,
    });
    assert_eq!(super::best_set(&sets, 0), Some(3));
    assert_eq!(super::best_set(&sets, 1), Some(1));
    assert_eq!(super::best_set(&sets, 3), Some(0));
}

#[test]
fn heroic_upgrade_is_an_original_alternative_to_researched_weapon_upgrade() {
    let (mut game, actor, _) =
        rocket_sets_with_upgrades(&["ScorpionRocket", "Upgrade_Veterancy_HEROIC"]);
    assert!(!game.entities[actor].weapon_set_upgraded());
    game.set_veterancy(actor, 3);
    assert!(game.object_has_upgrade(actor, "Upgrade_Veterancy_HEROIC"));
    assert!(game.entities[actor].weapon_set_upgraded());
    assert_eq!(game.entities[actor].weapon_index(1), 7);
    game.set_veterancy(actor, 0);
    assert!(game.object_has_upgrade(actor, "Upgrade_Veterancy_HEROIC"));
    assert!(game.entities[actor].weapon_set_upgraded());
}

#[test]
fn authored_fire_indices_preserve_null_ordinals_and_later_weapon_ownership() {
    use crate::skirmish::{
        WeaponRules,
        fire::{collide, weapon_update},
    };
    let mut unit = fixture_type(Faction::Usa, Kind::Tank);
    unit.behavior.fire_weapons = [Some("First"), None, Some("Third")]
        .into_iter()
        .map(|name| weapon_update::Rules {
            weapon: name.map(|name| WeaponRules {
                name: name.into(),
                ..WeaponRules::unarmed()
            }),
            initial_delay: 0,
            exclusive_delay: 0,
        })
        .collect();
    unit.behavior.collision_weapons = vec![collide::Rules {
        weapon: WeaponRules {
            name: "Collision".into(),
            ..WeaponRules::unarmed()
        },
        required: 0,
        forbidden: 0,
        fire_once: false,
    }];
    let first = unit.fire_weapon_index(0).unwrap();
    let third = unit.fire_weapon_index(2).unwrap();
    assert_eq!(third, first + 2);
    assert!(unit.fire_weapon_index(1).is_none());
    assert!(unit.fire_weapon_index(3).is_none());
    assert!(unit.weapon_template(first + 1).is_none());
    assert!(unit.weapon_template_mut(first + 1).is_none());
    assert!(std::ptr::eq(
        unit.weapon_template(third).unwrap(),
        unit.behavior.fire_weapons[2].weapon.as_ref().unwrap()
    ));
    unit.weapon_template_mut(third).unwrap().damage = 17.0;
    assert_eq!(
        unit.behavior.fire_weapons[2]
            .weapon
            .as_ref()
            .unwrap()
            .damage
            .to_bits(),
        17.0_f32.to_bits()
    );
    let collision = unit.collision_weapon_start();
    assert_eq!(collision, third + 1);
    assert_eq!(unit.weapon_template_count(), collision + 1);
    assert_eq!(unit.weapon_template(collision).unwrap().name, "Collision");
    unit.weapon_template_mut(collision).unwrap().damage = 19.0;
    assert_eq!(
        unit.behavior.collision_weapons[0].weapon.damage.to_bits(),
        19.0_f32.to_bits()
    );
}
