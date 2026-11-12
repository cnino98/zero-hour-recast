use super::*;
use crate::collision_geometry::{Geometry, Shape};

fn rules() -> Rules {
    Rules {
        slot: 1,
        minimum_shots: 4,
        lifetime_per_second: 10_000,
        maximum_lifetime: 5_400,
        triggers: Vec::new(),
        conflicts: Vec::new(),
        requires_all: false,
        field: hazards::Rules {
            name: "PoisonFieldMedium".into(),
            damage: 2.0,
            radius: 80.0,
            interval: 15,
            airborne_height: 0.64,
            lifetime: 900,
            health: 100.0,
            geometry_radius: 40.0,
            geometry: Geometry {
                shape: Shape::Cylinder,
                major: 40.0,
                minor: 1.0,
                height: 1.0,
                is_small: false,
            },
            core_radius: 10.0,
            core_damage: 200.0,
            death_type: crate::skirmish::DeathType::Poisoned,
            fire_fx: String::new(),
            death_fx: String::new(),
            ambient: String::new(),
        },
    }
}

fn observation(shot: Option<u32>, ready: u32) -> Observation {
    Observation {
        selected: true,
        present: true,
        upgrades: true,
        last_shot: shot,
        possible_next_shot: ready,
    }
}

fn spray(state: &mut State, rules: &Rules) {
    assert!(state.update(rules, 1, &observation(None, 0)).is_none());
    for frame in [2, 8, 14, 20] {
        assert!(
            state
                .update(rules, frame, &observation(Some(frame - 1), frame + 5))
                .is_none()
        );
    }
}

#[test]
fn strict_ready_deadline_uses_actual_shots_and_source_float_lifetime() {
    let rules = rules();
    let mut state = State::new();
    spray(&mut state, &rules);
    assert!(
        state
            .update(&rules, 25, &observation(Some(19), 25))
            .is_none()
    );
    assert_eq!(
        state.update(&rules, 26, &observation(Some(19), 25)),
        Some(240)
    );
    assert!(
        state
            .update(&rules, 27, &observation(Some(19), 25))
            .is_none()
    );
    spray(&mut state, &rules);
    assert_eq!(
        state.update(&rules, 30_000, &observation(Some(19), 25)),
        Some(5_400)
    );
}

#[test]
fn first_validity_edge_and_upgrade_rejection_reset_without_creating_a_field() {
    let rules = rules();
    let mut state = State::new();
    // A shot seen on the first activation is counted then discarded by native resetStats.
    assert!(state.update(&rules, 2, &observation(Some(1), 7)).is_none());
    for frame in [8, 14, 20] {
        assert!(
            state
                .update(&rules, frame, &observation(Some(frame - 1), frame + 5))
                .is_none()
        );
    }
    assert!(
        state
            .update(&rules, 26, &observation(Some(19), 25))
            .is_none()
    );
    assert!(
        state
            .update(&rules, 27, &observation(Some(26), 32))
            .is_none()
    );
    let mut blocked = observation(Some(26), 32);
    blocked.upgrades = false;
    assert!(state.update(&rules, 28, &blocked).is_none());
    assert!(
        state
            .update(&rules, 40, &observation(Some(26), 32))
            .is_none()
    );
    assert_eq!(state.shots, 0);
}

#[test]
fn switching_slots_creates_field_even_if_the_old_upgrade_branch_now_conflicts() {
    let rules = rules();
    let mut state = State::new();
    spray(&mut state, &rules);
    let mut switched = observation(Some(19), 25);
    switched.selected = false;
    switched.upgrades = false;
    assert_eq!(state.update(&rules, 21, &switched), Some(190));
    assert_eq!(state.shots, 0);
}

#[test]
fn normal_phase_creation_preserves_owner_position_lifetime_and_immediate_field_damage() {
    let mut game = crate::skirmish::tests::game();
    game.catalog[6].behavior.contamination = vec![rules()];
    crate::skirmish::tests::native_hazard_fixture(&mut game, 6, hazards::Origin::Cooldown(0));
    crate::skirmish::tests::register_update(
        &mut game.catalog[6],
        "FireOCLAfterWeaponCooldownUpdate",
    );

    game.catalog[6].location_weapons = 2;
    game.catalog[6].secondary_weapon = Some(super::super::WeaponRules::unarmed());
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    game.entities[id].weapon_set.current = 1;
    for frame in [1, 2, 8, 14, 20] {
        game.tick = frame;
        if frame > 1 {
            game.entities[id].weapon_set.weapons[1].last_shot = Some(frame - 1);
            game.entities[id].weapon_set.weapons[1].possible_next_shot = frame + 5;
        }
        game.update_contamination_module(id, 0);
    }
    game.tick = 26;
    game.update_ground_contacts();
    let before = game.entities[id].health;
    game.update_contamination_module(id, 0);
    assert_eq!(game.hazards.len(), 1);
    assert_eq!(game.hazards[0].owner, 0);
    assert_eq!(game.hazards[0].created, 26);
    assert_eq!(game.hazards[0].expires, 266);
    assert_eq!(
        game.hazards[0].position.map(f32::to_bits),
        [300.0, 300.0, 0.0].map(f32::to_bits)
    );
    game.run_object_updates();
    assert!(game.entities[id].health < before);
    let field = &game.hazards[0];
    let field_type = game.entities[field.object].type_index;
    assert_eq!(
        game.presentation
            .shots
            .iter()
            .filter(|shot| shot.source == field.object
                && shot.weapon_index == game.catalog[field_type].fire_weapon_start())
            .count(),
        1
    );
}

#[test]
fn original_location_command_locks_secondary_emits_missiles_and_stop_finishes_contamination() {
    use crate::skirmish::Order;
    let mut game = crate::skirmish::tests::game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    game.catalog[6].behavior.contamination = vec![rules()];
    crate::skirmish::tests::native_hazard_fixture(&mut game, 6, hazards::Origin::Cooldown(0));
    crate::skirmish::tests::register_update(
        &mut game.catalog[6],
        "FireOCLAfterWeaponCooldownUpdate",
    );

    game.catalog[6].location_weapons = 2;
    let projectile_type = sprayer_projectile(&mut game);
    game.catalog[6].secondary_weapon = Some(sprayer(projectile_type));
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    assert!(game.fire_weapon_at(id, 0, [305.0, 300.0]).is_err());
    assert!(game.fire_weapon_at(id, 1, [-1.0, 300.0]).is_err());
    game.fire_weapon_at(id, 1, [305.0, 300.0]).unwrap();
    assert!(matches!(game.entities[id].ai.order, Order::FireAt { .. }));
    assert_eq!(game.entities[id].weapon_slot(), 1);
    let mut shots = 0;
    for _ in 0..30 {
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == id && shot.weapon_slot == 1 && shot.target.is_none())
            .count();
        assert!(game.hazards.is_empty());
    }
    assert_eq!(shots, 5);
    assert!(
        game.projectiles
            .iter()
            .any(|projectile| projectile.source == id && projectile.target.is_none())
    );
    game.stop(id).unwrap();
    for _ in 0..10 {
        game.step();
    }
    assert_eq!(game.hazards.len(), 1);
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
}

#[test]
fn typed_poison_uses_post_armor_amount_and_recurring_kill_retains_launcher_experience() {
    use crate::skirmish::{DamageType, DeathType};
    let mut game = crate::skirmish::tests::game();
    game.catalog[5].veterancy.trainable = true;
    game.catalog[5].veterancy.required = [0, 100, 200, 300];
    game.catalog[5].weapon.damage = 80.0;
    game.catalog[5].weapon.damage_type = DamageType::Poison;
    game.catalog[5].weapon.death_type = DeathType::PoisonedBeta;
    game.catalog[6].armor[crate::skirmish::DamageType::Poison as usize] = 0.5;
    game.catalog[6].behavior.poisoned = vec![hazards::PoisonRules {
        interval: 3,
        duration: 9,
    }];
    game.catalog[6].update_modules.push(
        crate::skirmish::module_updates::ModuleRegistration::Behavior {
            family: "PoisonedBehavior".into(),
            tag: "PoisonRegression".into(),
            source: crate::skirmish::module_updates::ModuleSource {
                path: "PoisonedBehavior.cpp".into(),
                origin: "source regression".into(),
                line: 1,
            },
        },
    );

    crate::skirmish::tests::poison_damage_fixture(&mut game, 6);
    game.catalog[6].veterancy.experience_value = [7; 4];
    let source = game.spawn(5, 0, [300.0, 300.0], true);
    let target = game.spawn(6, 1, [310.0, 300.0], true);
    game.deal_bound_weapon_damage(
        source,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        Some(target),
        [310.0, 300.0, 0.0],
        1.0,
    );
    assert_eq!(game.entities[target].health.to_bits(), 60.0_f32.to_bits());
    game.tick = 3;
    game.run_object_updates();
    assert_eq!(game.entities[target].health.to_bits(), 20.0_f32.to_bits());
    game.tick = 6;
    game.run_object_updates();
    assert!(!game.entities[target].alive());
    assert!(matches!(
        game.entities[target].damage_state.death_type,
        DeathType::PoisonedBeta
    ));
    assert_eq!(game.entities[source].experience, 7);
}

#[test]
fn stopping_zero_primary_sprayer_can_reacquire_with_the_original_primary_weapon() {
    let mut game = crate::skirmish::tests::game();
    game.catalog[6].ai.acquisition |= crate::skirmish::UnitAi::IDLE;
    game.catalog[6].secondary_weapon = Some(crate::skirmish::WeaponRules {
        range: 15.0,
        auto_choose: 0,
        ..crate::skirmish::WeaponRules::unarmed()
    });
    let actor = game.spawn(6, 0, [300.0, 300.0], true);
    let target = game.spawn(6, 1, [360.0, 300.0], true);
    game.entities[actor].weapon_set.current = 1;
    game.entities[actor].weapon_set.lock = crate::skirmish::weapons::Lock::Temporary;
    game.stop(actor).unwrap();
    game.tick = game.entities[actor].ai.mood.next_check;
    game.update_visibility();
    assert_eq!(game.mood_target(actor, true, false), Some(target));
    // A human attack immediately after Stop must admit the allocated primary,
    // before an idle update has had a chance to change the current sprayer slot.
    game.attack(actor, target).unwrap();
    game.step();
    assert_eq!(game.entities[actor].weapon_slot(), 0);
}

fn sprayer_projectile(game: &mut crate::skirmish::Skirmish) -> usize {
    let kind = game.catalog.len();
    let rules = crate::skirmish::point_defense::tests::missile_rules();
    game.catalog
        .push(crate::skirmish::point_defense::tests::missile_template(
            rules, 1.0,
        ));
    kind
}

fn sprayer(projectile_type: usize) -> crate::skirmish::WeaponRules {
    use crate::skirmish::{WeaponMotion, WeaponRules};
    WeaponRules {
        name: "ToxinTruckSprayer".into(),
        range: 15.0,
        delay: 0.2,
        aim_delta: std::f32::consts::PI,
        speed: 1.0,
        motion: Some(WeaponMotion {
            turret_mounts: None,
            launches: std::array::from_fn(|_| Vec::new()),
            damage_thresholds: [0.5, 0.1],
            shots_per_barrel: 1,
            projectile_type: Some(projectile_type),
            collide_mask: 4,
            shell_speed_scale: None,
        }),
        ..WeaponRules::unarmed()
    }
}

#[test]
fn targeted_clip_exhaustion_releases_temporary_lock_and_finishes_scatter_order() {
    use crate::skirmish::Order;
    let mut game = crate::skirmish::tests::game();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    game.catalog[6].location_weapons = 4;
    let missing = game.spawn(6, 0, [350.0, 300.0], true);
    assert!(!game.location_weapon_ready(missing, 2));
    assert!(game.fire_weapon_at(missing, 2, [355.0, 300.0]).is_err());
    let projectile_type = sprayer_projectile(&mut game);
    let mut weapon = sprayer(projectile_type);
    weapon.clip_size = 3;
    weapon.reload_seconds = 1.0;
    weapon.scatter_targets = vec![[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]];
    weapon.scatter_target_scalar = 1.0;
    game.catalog[6].tertiary_weapon = Some(weapon);
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    game.fire_weapon_at(id, 2, [305.0, 300.0]).unwrap();
    let mut destinations = Vec::new();
    for _ in 0..90 {
        game.step();
        destinations.extend(
            game.presentation
                .shots
                .iter()
                .filter(|shot| shot.source == id)
                .map(|shot| {
                    assert_eq!(shot.weapon_slot, 2);
                    assert!(shot.target.is_none());
                    shot.target_position.map(f32::to_bits)
                }),
        );
    }
    destinations.sort_unstable();
    let mut expected = [
        [306.0_f32, 300.0, 0.0],
        [305.0, 301.0, 0.0],
        [304.0, 300.0, 0.0],
    ]
    .map(|point| point.map(f32::to_bits));
    expected.sort_unstable();
    assert_eq!(destinations, expected);
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert_eq!(game.entities[id].weapon_slot(), 0);
}

#[test]
fn research_and_salvage_replacing_locked_sprayer_end_location_attack_without_primary_shot() {
    use crate::skirmish::{ConditionalWeapon, Order, WeaponTemplateSet};
    for research in [false, true] {
        let mut game = crate::skirmish::tests::game();
        game.catalog[6].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
        let projectile_type = sprayer_projectile(&mut game);
        let unit = &mut game.catalog[6];
        unit.behavior.contamination = vec![rules()];
        crate::skirmish::tests::register_update(unit, "FireOCLAfterWeaponCooldownUpdate");
        unit.location_weapons = 2;
        unit.secondary_weapon = Some(sprayer(projectile_type));
        unit.weapon_upgrade_triggers = vec!["Upgrade_GLAAnthraxBeta".into()];
        crate::skirmish::tests::register_behavior(unit, "WeaponSetUpgrade", 0x80);
        unit.behavior_modules
            .last_mut()
            .unwrap()
            .upgrade
            .triggers
            .clone_from(&unit.weapon_upgrade_triggers);
        unit.weapon_set_rules.templates = vec![
            ConditionalWeapon {
                rules: sprayer(projectile_type),
                flags: 1,
                slot: 1,
            },
            ConditionalWeapon {
                rules: sprayer(projectile_type),
                flags: 2,
                slot: 1,
            },
        ];
        unit.weapon_set_rules.replacements = [(0, 1), (1, 7), (2, 8)]
            .into_iter()
            .map(|(flags, secondary)| WeaponTemplateSet {
                flags,
                weapons: [Some(0), Some(secondary), None],
                share_lock: false,
            })
            .collect();
        crate::skirmish::tests::native_hazard_fixture(&mut game, 6, hazards::Origin::Cooldown(0));
        let actor = game.spawn(6, 0, [300.0, 300.0], true);
        game.fire_weapon_at(actor, 1, [305.0, 300.0]).unwrap();
        game.step(); // Aim transitions to Fire. The payload runs on the following update.
        game.step();
        assert!(
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == actor && shot.weapon_slot == 1)
        );
        if research {
            game.players[0]
                .upgrades
                .push("Upgrade_GLAAnthraxBeta".into());
            game.activate_upgrade_modules(actor);
        } else {
            game.entities[actor].salvage.level = 1;
            game.replace_weapon_set(actor);
        }
        game.step();
        assert!(matches!(game.entities[actor].ai.order, Order::Idle));
        assert!(
            !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == actor)
        );
    }
}
