use crate::skirmish::{
    AssistedTargeting, Faction, Kind, Order, Regeneration, WeaponMotion, WeaponRules,
    tests::{fixture_type, game},
    turret::Rules,
};

fn weapon(range: f32, mask: u8) -> WeaponRules {
    let mut weapon = fixture_type(Faction::Gla, Kind::Tank).weapon;
    weapon.damage = 1.0;
    weapon.range = range;
    weapon.anti_mask = mask;
    weapon.clip_size = 4;
    weapon.reload_seconds = 2.0;
    weapon.delay = 0.25;
    weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| Vec::new()),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    weapon
}

#[test]
fn feedback_uses_constructed_first_instance_templates_and_real_laser_lifetime() {
    let (mut game, requester, assistant, target) = defenses(None);
    let mut laser = fixture_type(Faction::Civilian, Kind::Effect);
    laser.name = "PatriotFeedback".into();
    laser.laser_update = Some(laser_rules());
    laser.behavior.deletions.push([3, 3]);
    crate::skirmish::tests::register_update(&mut laser, "DeletionUpdate");
    let template = game.catalog.len();
    game.catalog.push(laser);
    let kind = game.entities[assistant].type_index;
    game.catalog[kind].behavior.assisted_targeting[0].lasers =
        ["PatriotFeedback".into(), "PatriotFeedback".into()];
    game.catalog[kind]
        .behavior
        .assisted_targeting
        .push(AssistedTargeting {
            clip_size: 1,
            weapon_slot: 0,
            lasers: ["Missing".into(), "Missing".into()],
        });
    crate::skirmish::tests::register_update(&mut game.catalog[kind], "AssistedTargetingUpdate");
    // Existing instances retain the null template lookup from their constructor.
    game.request_weapon_assistance_for(requester, target, 200.0);
    assert!(game.presentation.laser_initializations.is_empty());
    let assistant = game.spawn(kind, 0, [100.0, 100.0], true);
    assert_eq!(
        game.entities[assistant].behaviors.assisted_targeting,
        [[Some(template); 2], [None; 2]]
    );
    game.request_weapon_assistance_for(requester, target, 200.0);
    let lasers: Vec<_> = game
        .presentation
        .laser_initializations
        .iter()
        .map(|event| event.object)
        .collect();
    assert_eq!(lasers.len(), 2);
    let first = &game.entities[lasers[0]];
    assert_eq!(first.behaviors.deletion_frames, [3]);
    assert_eq!(
        game.object_position(lasers[0]).map(f32::to_bits),
        game.object_position(requester).map(f32::to_bits)
    );
    assert_eq!(
        game.presentation.laser_initializations[0].parent_drawable,
        Some(assistant)
    );
    assert_eq!(
        game.presentation.laser_initializations[0]
            .parent_position
            .unwrap()
            .map(f32::to_bits),
        game.entities[assistant]
            .drawable_position()
            .unwrap()
            .map(f32::to_bits)
    );
    assert_eq!(
        game.presentation.laser_initializations[0]
            .init
            .end
            .unwrap()
            .map(f32::to_bits),
        game.object_position(assistant).map(f32::to_bits)
    );
    assert_eq!(
        game.presentation.laser_initializations[1]
            .init
            .end
            .unwrap()
            .map(f32::to_bits),
        game.object_position(target).map(f32::to_bits)
    );
    assert_eq!(
        game.team_membership.team_of(lasers[0]),
        Some(game.team_membership.default_team(0))
    );
    game.tick = 3;
    game.run_object_updates();
    assert!(lasers.iter().all(|&id| game.entities[id].exists()));
    game.process_update_destruction();
    assert!(lasers.iter().all(|&id| !game.entities[id].exists()));
}

fn defenses(
    regeneration: Option<Regeneration>,
) -> (crate::skirmish::Skirmish, usize, usize, usize) {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    game.catalog[0].energy = 100;
    let mut unit = fixture_type(Faction::Gla, Kind::Defense);
    unit.name = "PatriotFixture".into();
    unit.kind_of = vec!["POWERED".into(), "STRUCTURE".into(), "IMMOBILE".into()];
    unit.speed = 0.0;
    unit.energy = -3;
    unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    unit.weapon = weapon(225.0, 2);
    unit.weapon.request_assist_range = 200.0;
    unit.secondary_weapon = Some(weapon(450.0, 3));
    unit.secondary_weapon.as_mut().unwrap().auto_choose = 0;
    unit.tertiary_weapon = Some(weapon(350.0, 1));
    unit.weapon_set_rules.share_reload = true;
    unit.behavior.assisted_targeting = vec![AssistedTargeting {
        clip_size: 4,
        weapon_slot: 1,
        lasers: [String::new(), String::new()],
    }];
    let mut turret = Rules::source_defaults();
    turret.controlled_slots = 7;
    turret.turn_rate = 0.1;
    unit.turret = Some(turret);
    unit.update_modules = crate::skirmish::tests::fixture_update_helpers(true);
    unit.behavior_modules
        .retain(|module| module.family != "AIUpdateInterface");
    crate::skirmish::tests::register_update(&mut unit, "AIUpdateInterface");
    crate::skirmish::tests::register_update(&mut unit, "AssistedTargetingUpdate");
    unit.maintenance.regeneration = regeneration;
    if unit.maintenance.regeneration.is_some() {
        crate::skirmish::tests::register_update(&mut unit, "BaseRegenerateUpdate");
    }
    let index = game.catalog.len();
    game.catalog.push(unit);
    let requester = game.spawn(index, 0, [300.0, 100.0], true);
    let assistant = game.spawn(index, 0, [100.0, 100.0], true);
    let victim = game.spawn(6, 1, [450.0, 100.0], true);
    game.update_power();
    (game, requester, assistant, victim)
}

#[test]
fn assistance_fires_exactly_four_from_outside_normal_range_and_shares_reload_without_refilling_other_clips()
 {
    let (mut game, requester, assistant, target) = defenses(None);
    game.attack(requester, target).unwrap();
    let mut assisted_shots = 0;
    for _ in 0..35 {
        game.step();
        for shot in &game.presentation.shots {
            if shot.source == assistant {
                assisted_shots += 1;
                assert_eq!(shot.weapon_slot, 1);
            }
        }
    }
    assert_eq!(assisted_shots, 4);
    assert!(matches!(game.entities[assistant].ai.order, Order::Idle));
    assert_eq!(
        game.entities[assistant].weapon_set.weapons[0].clip_remaining,
        4
    );
    let clocks = game.entities[assistant]
        .weapon_set
        .weapons
        .each_ref()
        .map(|state| state.cooldown.to_bits());
    assert_eq!(clocks, [clocks[0]; 3]);
    assert!(
        game.entities[assistant]
            .weapon_set
            .weapons
            .iter()
            .all(|state| matches!(
                state.status,
                crate::skirmish::ai_dispatch::WeaponStatus::Reloading
            ))
    );
}

#[test]
fn assistance_excludes_enemy_under_construction_and_reloading_batteries() {
    let (mut game, requester, assistant, target) = defenses(None);
    let kind = game.entities[assistant].type_index;
    let enemy = game.spawn(kind, 1, [350.0, 100.0], true);
    let foundation = game.spawn(kind, 0, [320.0, 200.0], false);
    game.entities[assistant].weapon_set.weapons[0].status =
        crate::skirmish::ai_dispatch::WeaponStatus::BetweenShots;
    game.entities[assistant].weapon_set.weapons[0].possible_next_shot = 1;
    // The cached UI counter is deliberately stale; native readiness uses its deadline.
    game.entities[assistant].weapon_set.weapons[0].cooldown = 0.0;
    game.request_weapon_assistance_for(
        requester,
        target,
        game.entities[requester]
            .active_weapon(&game.catalog[game.entities[requester].type_index])
            .request_assist_range,
    );
    assert!(game.presentation.assistance_events.is_empty());
    assert_eq!(game.entities[enemy].weapon_slot(), 0);
    assert_eq!(game.entities[foundation].weapon_slot(), 0);
    game.tick = 1;
    game.entities[assistant].weapon_set.weapons[0].cooldown = 1.0;
    game.players[0].power = -1;
    game.request_weapon_assistance_for(
        requester,
        target,
        game.entities[requester]
            .active_weapon(&game.catalog[game.entities[requester].type_index])
            .request_assist_range,
    );
    // Native admission is independent of power; only AI updates are disabled.
    assert_eq!(game.presentation.assistance_events.len(), 1);
    assert_eq!(game.presentation.assistance_events[0].assistant, assistant);
}

#[test]
fn brownout_freezes_weapon_and_turret_and_power_recovery_resumes_without_moving_the_structure() {
    let (mut game, requester, _, target) = defenses(None);
    game.entities[target].position = [300.0, 210.0];
    game.attack(requester, target).unwrap();
    game.catalog[0].energy = 0;
    let original = game.entities[requester].position.map(f32::to_bits);
    for _ in 0..20 {
        game.step();
        assert!(game.presentation.shots.is_empty());
    }
    assert_eq!(
        game.entities[requester].ai.turret.angle.to_bits(),
        0.0_f32.to_bits()
    );
    game.attack(requester, target).unwrap();
    game.fire_weapon(requester, target);
    let slot = game.entities[requester].weapon_slot();
    let shot_frame = game.entities[requester].weapon_set.weapons[slot].last_shot;
    assert_eq!(shot_frame, Some(game.tick));
    for _ in 0..10 {
        game.step();
        assert!(game.presentation.shots.is_empty());
    }
    let kind = game.entities[requester].type_index;
    let frame = game.update_frame();
    assert!(matches!(
        game.entities[requester].weapon_status(slot, &game.catalog[kind], frame),
        Some(crate::skirmish::ai_dispatch::WeaponStatus::Ready)
    ));
    assert_eq!(
        game.entities[requester].weapon_set.weapons[slot].last_shot,
        shot_frame
    );
    game.catalog[0].energy = 100;
    let mut fired = false;
    for _ in 0..30 {
        game.step();
        fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == requester);
    }
    assert!(fired);
    assert_eq!(
        game.entities[requester].position.map(f32::to_bits),
        original
    );
    assert_eq!(game.entities[requester].facing.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn native_tertiary_air_slot_uses_shared_clock_and_current_slot_conditions() {
    let (mut game, requester, _, target) = defenses(None);
    let target_type = game.entities[target].type_index;
    game.catalog[target_type].kind_of = vec!["VEHICLE".into()];
    game.catalog[target_type].preferred_height = 40.0;
    game.catalog[target_type]
        .locomotor
        .airborne_targeting_height = 1.0;
    let point = game.entities[target].position;
    game.set_object_position(target, [point[0], point[1], 40.0]);
    assert!(game.choose_weapon(requester, target));
    assert_eq!(game.entities[requester].weapon_slot(), 2);
    game.tick = 1;
    game.fire_weapon(requester, target);
    assert_eq!(game.presentation.shots.last().unwrap().weapon_slot, 2);
    game.run_object_updates();
    assert_eq!(
        game.entities[requester].weapon_condition(),
        Some("FIRING_C")
    );
}

#[test]
fn base_regeneration_damage_restarts_native_delay_and_works_during_brownout() {
    let (mut game, requester, _, target) = defenses(Some(Regeneration {
        percent_per_second: 0.003,
        delay: 90,
    }));
    game.catalog[0].energy = 0;
    game.apply_hit(requester, target, 20.0);
    let health = game.entities[requester].health;
    for _ in 0..89 {
        game.step();
    }
    assert_eq!(game.entities[requester].health.to_bits(), health.to_bits());
    game.apply_hit(requester, target, 1.0);
    for _ in 0..89 {
        game.step();
    }
    assert_eq!(
        game.entities[requester].health.to_bits(),
        (health - 1.0).to_bits()
    );
    game.step();
    let amount = 3.0 * (game.entities[requester].max_health * 0.003) / 30.0;
    assert_eq!(
        game.entities[requester].health.to_bits(),
        (health - 1.0 + amount).to_bits()
    );
}

#[test]
fn immobile_admission_uses_current_slot_range_before_selecting_a_longer_range_weapon() {
    let (mut game, _, assistant, target) = defenses(None);
    assert!(game.weapon_can_target(assistant, target));
    assert!(!game.weapon_target_reachable(assistant, target));
    assert!(game.attack(assistant, target).is_err());
    game.entities[assistant].weapon_set.current = 1;
    assert!(game.weapon_target_reachable(assistant, target));
    // Unlike the range query, a later unlocked selector may return to primary.
    game.entities[assistant].weapon_set.lock = super::Lock::Temporary;
    assert!(game.attack(assistant, target).is_ok());
}

#[test]
fn assisted_targeting_neutral_request_is_ai_sourced_independent_of_old_player_force_mode() {
    let (mut game, requester, assistant, target) = defenses(None);
    game.entities[target].owner = game.neutral_owner();
    assert!(!game.entities[target].script_targetable());
    game.entities[assistant].ai.command.from_ai = false;
    game.entities[assistant].ai.attack_context.forced = true;
    assert!(game.attack(requester, target).is_err());
    game.force_attack(requester, target).unwrap();
    game.request_weapon_assistance_for(
        requester,
        target,
        game.entities[requester]
            .active_weapon(&game.catalog[game.entities[requester].type_index])
            .request_assist_range,
    );
    assert!(game.entities[assistant].ai.command.from_ai);
    assert!(!game.entities[assistant].ai.attack_context.forced);
    assert!(matches!(game.entities[assistant].ai.order, Order::Attack(id) if id == target));
    assert!(game.update_attack(assistant, target));
    let mut assisted_shots = game
        .presentation
        .shots
        .iter()
        .filter(|shot| shot.source == assistant)
        .count();
    for _ in 0..35 {
        game.step();
        assisted_shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == assistant)
            .count();
    }
    assert_eq!(assisted_shots, 4);
    assert!(matches!(game.entities[assistant].ai.order, Order::Idle));
}

#[test]
fn assistance_follows_native_team_membership_order_after_reassignment() {
    let (mut game, requester, first, target) = defenses(None);
    let kind = game.entities[first].type_index;
    let later = game.spawn(kind, 0, [150.0, 100.0], true);
    let prototype = game
        .team_membership
        .add_prototype(0, "DefensiveTeam".into());
    let team = game.team_membership.create_team(prototype);
    game.transfer_maintenance_team(later, team);
    game.request_weapon_assistance_for(requester, target, 200.0);
    assert_eq!(
        game.presentation
            .assistance_events
            .iter()
            .map(|event| event.assistant)
            .collect::<Vec<_>>(),
        [first, later]
    );
    for id in [first, later] {
        assert_eq!(game.entities[id].weapon_slot(), 1);
        assert!(game.entities[id].last_command_from_ai());
        assert!(game.entities[id].ai.attack.is_some());
    }
}

#[test]
fn equivalent_reskin_responds_but_native_no_attack_status_refuses() {
    let (mut game, requester, first, target) = defenses(None);
    game.entities[first].object_status |= 1 << 6;
    let mut variant = fixture_type(Faction::Gla, Kind::Tank);
    variant.name = "PatriotReskinFixture".into();
    variant.reskin = Some(game.catalog[game.entities[first].type_index].name.clone());
    variant.behavior.assisted_targeting = vec![AssistedTargeting {
        clip_size: 2,
        weapon_slot: 0,
        lasers: [String::new(), String::new()],
    }];
    crate::skirmish::tests::register_update(&mut variant, "AssistedTargetingUpdate");
    let kind = game.catalog.len();
    game.catalog.push(variant);
    let equivalent = game.spawn(kind, 0, [320.0, 100.0], true);
    game.request_weapon_assistance_for(requester, target, 200.0);
    assert_eq!(
        game.presentation
            .assistance_events
            .iter()
            .map(|event| event.assistant)
            .collect::<Vec<_>>(),
        [equivalent]
    );
    assert_eq!(
        game.entities[equivalent].weapon_set.weapons[0].max_shots,
        Some(2)
    );
    assert!(matches!(game.entities[first].ai.order, Order::Idle));
}

#[test]
fn feedback_checks_the_created_variation_and_captures_only_an_existing_parent_drawable() {
    let (mut game, requester, assistant, target) = defenses(None);
    let mut actual = fixture_type(Faction::Civilian, Kind::Effect);
    actual.name = "ActualFeedback".into();
    actual.laser_update = Some(laser_rules());
    let actual_index = game.catalog.len();
    game.catalog.push(actual);
    let mut wrapper = fixture_type(Faction::Civilian, Kind::Effect);
    wrapper.name = "FeedbackChoice".into();
    wrapper.variations.push("ActualFeedback".into());
    let wrapper_index = game.catalog.len();
    game.catalog.push(wrapper);
    game.entities[assistant].drawable.present = false;
    let laser = game.entities.len();
    game.make_feedback_laser(wrapper_index, assistant, requester, target);
    assert_eq!(game.entities[laser].type_index, actual_index);
    let event = game.presentation.laser_initializations.last().unwrap();
    assert_eq!(event.object, laser);
    assert_eq!(event.parent_drawable, None);
    assert_eq!(event.init.parent, Some(assistant));
    assert_eq!(
        event.init.end.unwrap().map(f32::to_bits),
        game.object_position(target).map(f32::to_bits)
    );
    game.entities[assistant].drawable.present = true;
    game.set_object_position(assistant, [150.0, 150.0, 30.0]);
    // The captured native Drawable ID remains invalid after later creation.
    assert_eq!(
        game.presentation
            .laser_initializations
            .last()
            .unwrap()
            .parent_drawable,
        None
    );
    // Inverse case: the wrapper advertises LaserUpdate but the created child does not.
    game.catalog[wrapper_index].laser_update = Some(laser_rules());
    game.catalog[actual_index].laser_update = None;
    let rejected = game.entities.len();
    game.make_feedback_laser(wrapper_index, assistant, requester, target);
    assert_eq!(game.entities[rejected].type_index, actual_index);
    assert!(
        game.presentation
            .laser_initializations
            .iter()
            .all(|event| event.object != rejected)
    );
    game.process_update_destruction();
    assert!(!game.entities[rejected].exists());
    assert!(game.entities[laser].exists());
}

fn laser_rules() -> crate::skirmish::laser_update::Rules {
    crate::skirmish::laser_update::Rules {
        particles: [String::new(), String::new()],
        punch_through: 0.0,
    }
}
