use super::*;
use crate::skirmish::{
    AreaDamage, ClipReload, DamageType, DeathType, LaunchTransform, WeaponMotion, tests::game,
};

fn fixture() -> (Skirmish, usize) {
    let mut game = game();
    for (index, unit) in game.catalog.iter_mut().enumerate() {
        unit.name = format!("Fixture{index}");
        unit.geometry_bounds.center_height = 0.0;
    }
    let unit = &mut game.catalog[5];
    unit.name = "GLAInfantryTerrorist".into();
    unit.weapon = WeaponRules {
        name: "TerroristSuicideWeapon".into(),
        damage: 999_999.0,
        range: 1.0,
        speed: 99_999.0,
        damage_type: DamageType::Explosion,
        death_type: DeathType::Suicided,
        clip_size: 1,
        reload: ClipReload::None,
        area: Some(AreaDamage {
            at_source: true,
            primary_radius: 1.0,
            secondary_radius: 0.0,
            secondary_damage: 0.0,
            affects: 8 | 32,
        }),
        motion: Some(WeaponMotion {
            turret_mounts: None,
            launches: std::array::from_fn(|_| vec![LaunchTransform::IDENTITY]),
            damage_thresholds: [0.5, 0.1],
            shots_per_barrel: 1,
            projectile_type: None,
            collide_mask: 0,
            shell_speed_scale: None,
        }),
        ..WeaponRules::unarmed()
    };
    unit.behavior.death_weapons.push(Rules {
        weapon: Some(WeaponRules {
            name: "SuicideDynamitePack".into(),
            damage: 500.0,
            range: 5.0,
            speed: 99_999.0,
            damage_type: DamageType::Explosion,
            death_type: DeathType::Suicided,
            clip_size: 1,
            reload: ClipReload::None,
            area: Some(AreaDamage {
                at_source: true,
                primary_radius: 18.0,
                secondary_radius: 50.0,
                secondary_damage: 300.0,
                affects: 63,
            }),
            ..WeaponRules::unarmed()
        }),
        death_mask: [
            DeathType::Suicided,
            DeathType::Crushed,
            DeathType::Splatted,
            DeathType::Lasered,
            DeathType::Burned,
            DeathType::Exploded,
        ]
        .iter()
        .fold(0, |bits, kind| bits | (1 << *kind as u32)),
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
        removals: Vec::new(),
        upgrade_fx: String::new(),
        starts_active: true,
        triggers: Vec::new(),
        conflicts: Vec::new(),
        requires_all: false,
    });
    crate::skirmish::tests::register_behavior(unit, "FireWeaponWhenDeadBehavior", 0x0082);
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    (game, actor)
}

#[test]
fn suicide_shot_kills_source_then_native_rings_hit_allies_but_not_equivalent_allies() {
    let (mut game, actor) = fixture();
    let target = game.spawn(6, 1, [309.0, 300.0], true);
    let ally = game.spawn(1, 0, [322.0, 300.0], true);
    let outer = game.spawn(6, 1, [350.0, 300.0], true);
    let matching = game.spawn(5, 0, [304.0, 300.0], true);
    for id in [target, ally, outer] {
        game.entities[id].health = 2_000.0;
    }
    game.update_ground_contacts();
    game.attack(actor, target).unwrap();
    assert!(game.entities[actor].attacking());
    assert!(!game.update_attack(actor, target));
    assert!(!game.entities[actor].alive());
    assert!(matches!(
        game.entities[actor].damage_state.death_type,
        DeathType::Suicided
    ));
    assert_eq!(
        game.entities[target].health.to_bits(),
        1_500.0_f32.to_bits()
    );
    assert_eq!(game.entities[ally].health.to_bits(), 1_500.0_f32.to_bits());
    assert_eq!(game.entities[outer].health.to_bits(), 1_700.0_f32.to_bits());
    assert!(game.entities[matching].alive());
    assert_eq!(game.presentation.shots.len(), 2);
    assert_eq!(
        game.presentation.shots[1].weapon_index,
        game.catalog[5].death_weapon_start()
    );
    assert!(game.projectiles.is_empty());
}

#[test]
fn dead_suicide_source_keeps_positive_player_xp_and_exact_loss_credit() {
    use crate::skirmish::{Faction, FactionProgression, Kind, ProgressionRules, Rank};

    let (mut game, actor) = fixture();
    let mut rebel = crate::skirmish::tests::fixture_type(Faction::Gla, Kind::Infantry);
    rebel.name = "GLAInfantryRebel".into();
    rebel.max_health = 120.0;
    rebel.initial_health = 120.0;
    rebel.veterancy.experience_value = [15, 15, 30, 40];
    rebel.progression.skill_point_value = [15, 15, 30, 40];
    let kind = game.catalog.len();
    game.catalog.push(rebel);
    game.configure_progression(ProgressionRules {
        multiplayer_colors: crate::skirmish::tests::original_multiplayer_colors(),
        ranks: [0, 1000]
            .into_iter()
            .map(|skill_points| Rank {
                skill_points,
                science_points: 1,
                sciences: Vec::new(),
            })
            .collect(),
        sciences: Vec::new(),
        factions: vec![FactionProgression {
            side: "GLA".into(),
            preferred_color: 0xff00_ff00,
            playable_side: true,
            faction: Faction::Gla,
            intrinsic_points: 0,
            intrinsic_sciences: Vec::new(),
        }],
        powers: Vec::new(),
    })
    .unwrap();
    let target = game.spawn(kind, 1, [309.0, 300.0], true);
    assert!(
        !game.catalog[game.entities[actor].type_index]
            .veterancy
            .trainable
    );
    game.update_ground_contacts();
    game.attack(actor, target).unwrap();
    assert!(game.entities[actor].attacking());
    assert!(!game.update_attack(actor, target));
    assert!(!game.entities[actor].alive() && !game.entities[target].alive());
    assert_eq!(game.players[0].skill_points, 15);
    assert_eq!(game.entities[actor].experience, 0);
    assert_eq!(game.players[0].statistics.units_lost, 1);
    assert_eq!(game.players[1].statistics.units_lost, 1);
    assert_eq!(game.players[0].statistics.units_destroyed, 1);
    assert_eq!(game.players[1].statistics.units_destroyed, 0);
    game.apply_typed_hit(target, actor, 500.0, DeathType::Suicided);
    assert_eq!(game.players[0].skill_points, 15);
    assert_eq!(game.players[0].statistics.units_destroyed, 1);
}

#[test]
fn promotion_health_applies_before_the_victims_death_weapon_hits_the_attacker() {
    let (mut game, victim) = fixture();
    game.catalog[5].veterancy.experience_value = [25; 4];
    game.catalog[6].veterancy.trainable = true;
    game.catalog[6].veterancy.required = [0, 25, 200, 400];
    game.catalog[6].veterancy.health_bonus = [1.0, 1.2, 1.4, 1.6];
    game.catalog[6].max_health = 500.0;
    game.catalog[6].initial_health = 500.0;
    let attacker = game.spawn(6, 1, [309.0, 300.0], true);
    game.entities[attacker].health = 450.0;
    game.update_ground_contacts();
    game.apply_typed_hit(victim, attacker, 999_999.0, DeathType::Exploded);
    assert!(!game.entities[victim].alive());
    assert!(game.entities[attacker].alive());
    assert_eq!(game.entities[attacker].experience, 25);
    assert_eq!(game.entities[attacker].veterancy, 1);
    assert_eq!(game.entities[attacker].health.to_bits(), 40.0_f32.to_bits());
}

#[test]
fn only_authored_death_reasons_detonate_and_repeat_damage_cannot_fire_twice() {
    for (reason, expected) in [
        (DeathType::Normal, 0),
        (DeathType::Poisoned, 0),
        (DeathType::PoisonedBeta, 0),
        (DeathType::Crushed, 1),
        (DeathType::Exploded, 1),
        (DeathType::Lasered, 1),
    ] {
        let (mut game, actor) = fixture();
        game.apply_typed_hit(actor, 2, 999_999.0, reason);
        game.apply_typed_hit(actor, 2, 999_999.0, DeathType::Suicided);
        assert_eq!(game.presentation.shots.len(), expected);
    }
}

#[test]
fn incomplete_actor_does_not_fire_its_death_weapon() {
    let (mut game, actor) = fixture();
    game.entities[actor].build_progress = 0.5;
    game.apply_typed_hit(actor, 2, 999_999.0, DeathType::Exploded);
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn suicide_respects_damage_armor_and_nonreloading_clip_stays_empty() {
    let (mut game, actor) = fixture();
    game.catalog[5].armor[DamageType::Explosion as usize] = 0.0;
    let target = game.spawn(6, 1, [309.0, 300.0], true);
    game.attack(actor, target).unwrap();
    game.update_attack(actor, target);
    assert!(game.entities[actor].alive());
    assert_eq!(game.presentation.shots.len(), 1);
    game.entities[actor].weapon_set.weapons[0].cooldown = 0.0;
    assert!(matches!(
        game.entities[actor].weapon_status(0, &game.catalog[5], 100),
        Some(crate::skirmish::ai_dispatch::WeaponStatus::OutOfAmmo)
    ));
    assert_eq!(game.entities[actor].weapon_set.weapons[0].clip_remaining, 0);
}

#[test]
fn contact_attack_reaches_structure_geometry_instead_of_its_bounding_circle() {
    let (mut game, actor) = fixture();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    game.catalog[5].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 5.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    game.catalog[0].geometry = crate::collision_geometry::Geometry {
        shape: crate::collision_geometry::Shape::Box,
        is_small: false,
        major: 45.0,
        minor: 10.0,
        height: 30.0,
    };
    let target = game.spawn(0, 1, [300.0, 350.0], true);
    game.rebuild_navigation();
    assert!(!game.within_attack_range(actor, target));
    game.attack(actor, target).unwrap();
    for _ in 0..600 {
        game.step();
        if !game.entities[actor].alive() {
            break;
        }
    }
    assert!(!game.entities[actor].alive());
    assert!(game.entities[actor].position[1] >= 335.0);
    assert_eq!(game.entities[target].health.to_bits(), 500.0_f32.to_bits());
}

fn latched_rule(effect: &str, trigger: &str, removal: &str) -> Rules {
    Rules {
        weapon: None,
        death_mask: u32::MAX,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
        starts_active: true,
        triggers: if trigger.is_empty() {
            Vec::new()
        } else {
            vec![trigger.into()]
        },
        conflicts: Vec::new(),
        requires_all: false,
        removals: if removal.is_empty() {
            Vec::new()
        } else {
            vec![removal.into()]
        },
        upgrade_fx: effect.into(),
    }
}

#[test]
fn starts_active_constructor_removal_only_resets_preceding_constructed_behaviors() {
    let mut game = game();
    let unit = &mut game.catalog[5];
    for rule in [
        latched_rule("First", "Pulse", ""),
        latched_rule("Remover", "", "Pulse"),
        latched_rule("Last", "Pulse", ""),
    ] {
        crate::skirmish::tests::register_behavior(unit, "FireWeaponWhenDeadBehavior", 0x0082);
        let module = unit.behavior_modules.last_mut().unwrap();
        module.upgrade.triggers.clone_from(&rule.triggers);
        module.upgrade.removals.clone_from(&rule.removals);
        module.upgrade.fx.clone_from(&rule.upgrade_fx);
        unit.behavior.death_weapons.push(rule);
    }
    unit.maintenance.research.push(crate::skirmish::Research {
        name: "Pulse".into(),
        cost: 0,
        frames: 1,
        player: false,
    });
    let id = game.spawn(5, 0, [300.0; 2], true);
    assert_eq!(
        game.entities[id].behaviors.death_weapons,
        [false, true, true]
    );
    assert_eq!(
        game.presentation
            .object_fx_events
            .iter()
            .map(|event| event.effect.as_str())
            .collect::<Vec<_>>(),
        ["First", "Remover", "Last"]
    );
    game.finish_research(id, 0);
    assert_eq!(
        game.entities[id].behaviors.death_weapons,
        [true, true, true]
    );
    game.remove_native_object_upgrade(id, "Pulse");
    assert_eq!(
        game.entities[id].behaviors.death_weapons,
        [false, true, false]
    );
    assert_eq!(game.presentation.object_fx_events.len(), 4);
}

#[test]
fn scheduled_destruction_does_not_skip_later_authored_death_weapon() {
    let (mut game, original) = fixture();
    let unit = &mut game.catalog[5];
    unit.behavior
        .instant_death
        .push(crate::skirmish::instant_death::Rules {
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
            effects: vec![Some("BeforeWeapon".into())],
            objects: Vec::new(),
            weapons: Vec::new(),
        });
    crate::skirmish::tests::register_behavior(unit, "InstantDeathBehavior", 0x0002);
    let instant = unit.behavior_modules.pop().unwrap();
    unit.behavior_modules.insert(0, instant);
    let id = game.spawn(5, 1, [500.0; 2], true);
    game.entities[original].position = [50.0; 2];
    game.kill_object(id, DeathType::Exploded);
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert!(game.entities[id].exists());
    assert_eq!(game.presentation.shots.len(), 1);
    let before = game.presentation.weapon_fx.iter().position(|event| matches!(event,
        crate::skirmish::WeaponFxEvent::ObjectFx(index) if game.presentation.object_fx_events[*index].effect == "BeforeWeapon")).unwrap();
    let shot = game
        .presentation
        .weapon_fx
        .iter()
        .position(|event| matches!(event, crate::skirmish::WeaponFxEvent::Shot(0)))
        .unwrap();
    assert!(before < shot);
}
