use super::*;
use crate::skirmish::{Faction, GroundAppearance, GroundRules, WeaponMotion};

fn setup() -> (Skirmish, usize, usize) {
    let mut game = crate::skirmish::tests::game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    game.players[0].sciences.push("SCIENCE_GLA".into());
    game.players[1].sciences.push("SCIENCE_GLA".into());
    let unit = &mut game.catalog[6];
    unit.kind_of
        .extend(["SALVAGER".into(), "WEAPON_SALVAGER".into()]);
    unit.veterancy.trainable = true;
    unit.veterancy.required = [0, 50, 75, 150];
    unit.locomotor.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Treads,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.1,
        lateral_friction: 0.2,
        extra_friction: 0.0,
    });
    unit.salvage.weapons = [1, 2]
        .into_iter()
        .map(|level| {
            let mut weapon = WeaponRules::unarmed();
            weapon.name = format!("MarauderUpgrade{level}");
            weapon.damage = 60.0;
            weapon.range = 150.0;
            weapon.delay = 0.75;
            weapon.speed = 40.0;
            weapon.clip_size = if level == 2 { 2 } else { 0 };
            weapon.motion = Some(WeaponMotion {
                turret_mounts: None,
                launches: [Vec::new(), Vec::new(), Vec::new()],
                damage_thresholds: [0.5, 0.1],
                shots_per_barrel: 1,
                projectile_type: None,
                collide_mask: 0,
                shell_speed_scale: None,
            });
            weapon
        })
        .collect();
    unit.weapon_set_rules.replacements = [(0, 0), (2, 3), (4, 4)]
        .map(|(flags, primary)| crate::skirmish::WeaponTemplateSet {
            flags,
            weapons: [Some(primary), None, None],
            share_lock: false,
        })
        .into();
    let mut item = crate::skirmish::tests::fixture_type(Faction::Civilian, Kind::Crate);
    item.name = "SalvageCrate".into();
    item.initial_health = 0.0;
    item.max_health = 0.0;
    item.salvage.pickup = Some(Pickup {
        science: "SCIENCE_GLA".into(),
        visible_science: "SCIENCE_GLA".into(),
        forbidden: vec!["PROJECTILE".into()],
        weapon_chance: 1.0,
        level_chance: 0.25,
        money: [25, 75],
        lifetime: [900, 1050],
    });
    crate::skirmish::tests::register_behavior(&mut item, "SalvageCrateCollide", 0x10);
    let template = game.catalog.len();
    game.catalog.push(item);
    let tank = game.spawn(6, 0, [300.0, 100.0], true);
    (game, tank, template)
}
fn item(game: &mut Skirmish, template: usize, position: [f32; 2]) -> usize {
    game.spawn(template, game.neutral_owner(), position, true)
}

#[test]
fn authored_civilian_salvager_is_distinct_from_neutral() {
    let (mut game, _, template) = setup();
    game.layout
        .map_sides
        .push(crate::skirmish::map_scripts::SideDefinition {
            name: "PlyrCivilian".into(),
            faction: "FactionCivilian".into(),
            gatherers: [0; 3],
            human: false,
            template_money: 0,
            start_money: 0,
            allies: Vec::new(),
            enemies: Vec::new(),
        });
    game.reset_slots(game.slots.clone()).unwrap();
    let neutral = game.neutral_owner();
    let civilian = game.players.len() - 1;
    for owner in [neutral, civilian] {
        game.players[owner].sciences.push("SCIENCE_GLA".into());
    }
    let crate_id = item(&mut game, template, [300.0, 100.0]);
    let neutral_tank = game.spawn(6, neutral, [300.0, 100.0], true);
    let civilian_tank = game.spawn(6, civilian, [300.0, 100.0], true);
    assert_eq!(game.entities[crate_id].owner, neutral);
    assert!(!game.can_salvage(neutral_tank, crate_id));
    assert!(game.can_salvage(civilian_tank, crate_id));
}

#[test]
fn live_crate_visibility_requires_science_and_current_partition_visibility() {
    let (mut game, collector, template) = setup();
    game.catalog[template].sight.immobile = true;
    let position = game.entities[collector].position;
    let crate_id = item(&mut game, template, position);
    game.update_visibility();
    assert!(game.crate_visible_to(0, crate_id));
    game.players[0].sciences.clear();
    assert!(!game.crate_visible_to(0, crate_id));
    game.players[0].sciences.push("SCIENCE_GLA".into());
    assert!(game.crate_visible_to(0, crate_id));
    for id in 0..game.entities.len() {
        if game.entities[id].owner == 0 {
            game.entities[id].position = [650.0, 500.0];
            game.observe_visibility(id);
        }
    }
    game.update_visibility();
    game.tick += 31;
    game.update_visibility();
    assert!(game.entities[crate_id].crate_present());
    assert!(matches!(
        game.visibility_status(0, crate_id),
        crate::skirmish::visibility::Status::Fogged
    ));
    assert!(!game.crate_visible_to(0, crate_id));
}

#[test]
fn inactive_crate_contact_upgrades_once_and_replaces_weapon_with_full_ammo() {
    let (mut game, tank, template) = setup();
    let other = game.spawn(6, 0, [300.0, 100.0], true);
    let crate_id = item(&mut game, template, [300.0, 100.0]);
    assert!(!game.entities[crate_id].alive());
    assert!(game.entities[crate_id].crate_present());
    game.update_ground_contacts();
    assert!(!game.entities[crate_id].crate_present());
    assert_eq!(
        game.entities[tank].salvage_level() + game.entities[other].salvage_level(),
        1
    );
    assert_eq!(game.presentation.salvage_events.len(), 1);
    let winner = game.presentation.salvage_events[0].collector;
    let position = game.entities[winner].position;
    let next = item(&mut game, template, position);
    game.entities[winner].weapon_set.weapons[0].cooldown = 500.0;
    assert!(game.salvage_contact(winner, next));
    let actor = &game.entities[winner];
    assert_eq!(actor.salvage_level(), 2);
    assert_eq!(
        actor.active_weapon(&game.catalog[6]).name,
        "MarauderUpgrade2"
    );
    assert_eq!(
        actor.weapon_set.weapons[0].cooldown.to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(actor.weapon_set.weapons[0].clip_remaining, 2);
}
#[test]
fn pickup_rejects_missing_science_projectile_dead_and_non_salvager() {
    let (mut game, tank, template) = setup();
    let crate_id = item(&mut game, template, [300.0, 100.0]);
    assert!(game.can_salvage(tank, crate_id));
    game.players[0].sciences.clear();
    assert!(!game.can_salvage(tank, crate_id));
    game.players[0].sciences.push("SCIENCE_GLA".into());
    game.catalog[6].kind_of.push("PROJECTILE".into());
    assert!(!game.can_salvage(tank, crate_id));
    game.catalog[6].kind_of.pop();
    game.entities[tank].health = 0.0;
    assert!(!game.can_salvage(tank, crate_id));
    game.entities[tank].health = 1.0;
    game.catalog[6].kind_of.retain(|name| name != "SALVAGER");
    assert!(!game.can_salvage(tank, crate_id));
}
#[test]
fn full_weapon_salvage_rolls_next_level_then_money_without_money_chance_draw() {
    let (mut game, tank, template) = setup();
    game.entities[tank].salvage.level = 2;
    game.catalog[template]
        .salvage
        .pickup
        .as_mut()
        .unwrap()
        .level_chance = 1.0;
    let crate_id = item(&mut game, template, [300.0, 100.0]);
    let seed = game.random;
    game.entities[tank].experience = 30;
    game.salvage_contact(crate_id, tank);
    assert_eq!(game.entities[tank].veterancy, 1);
    assert_eq!(game.entities[tank].experience, 50);
    assert_eq!(game.random, seed);
    assert!(matches!(
        game.presentation.salvage_events[0].reward,
        Reward::Level
    ));
    game.entities[tank].veterancy = 3;
    let crate_id = item(&mut game, template, [300.0, 100.0]);
    let mut expected = game.random;
    let money = integer(&mut expected, 25, 75);
    let cash = game.players[0].cash;
    game.salvage_contact(crate_id, tank);
    assert_eq!(game.random, expected);
    assert_eq!(game.players[0].cash, cash + money);
    assert_eq!(game.players[0].statistics.money_earned, money);
    assert!(
        matches!(game.presentation.salvage_events[1].reward, Reward::Money(value) if value == money)
    );
}
#[test]
fn creation_chance_is_before_killer_science_and_ally_death_consumes_nothing() {
    let (mut game, tank, _) = setup();
    game.catalog[6].salvage.drops.push(Drop {
        chance: 1.0,
        killer_kinds: vec!["SALVAGER".into()],
        science: Some("SCIENCE_GLA".into()),
        template: "SalvageCrate".into(),
    });
    let ally = game.spawn(6, 0, [320.0, 100.0], true);
    let seed = game.random;
    game.create_salvage(ally, Some(tank));
    assert_eq!(game.random, seed);
    let enemy = game.spawn(6, 1, [500.0, 100.0], true);
    game.players[0].sciences.clear();
    let mut expected = game.random;
    startup::random_real(&mut expected, 0.0, 1.0);
    game.create_salvage(enemy, Some(tank));
    assert_eq!(game.random, expected);
    assert!(!game.entities.iter().any(Entity::crate_present));
    game.players[0].sciences.push("SCIENCE_GLA".into());
    game.apply_hit(enemy, tank, 1000.0);
    let dropped = game.entities.last().unwrap();
    assert!(dropped.crate_present());
    assert!(!dropped.alive());
    assert_eq!(dropped.owner, 2);
}
#[test]
fn crate_expiry_destroys_without_on_die_and_reset_removes_events() {
    let (mut game, _, template) = setup();
    game.catalog[template]
        .salvage
        .pickup
        .as_mut()
        .unwrap()
        .lifetime = [1, 1];
    let crate_id = item(&mut game, template, [300.0, 100.0]);
    let count = game.entities.len();
    game.step();
    assert!(!game.entities[crate_id].crate_present());
    assert_eq!(game.entities.len(), count);
    game.presentation.salvage_events.push(Event {
        frame: game.tick,
        collector: 0,
        position: [0.0; 3],
        reward: Reward::Weapon,
    });
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(game.presentation.salvage_events.is_empty());
    assert!(!game.entities.iter().any(Entity::crate_present));
}
#[test]
fn in_flight_damage_retains_the_fired_weapon_set_after_second_pickup() {
    let (mut game, tank, template) = setup();
    let victim = game.spawn(6, 1, [360.0, 100.0], true);
    game.entities[victim].health = 1000.0;
    game.entities[victim].max_health = 1000.0;
    let first = item(&mut game, template, [300.0, 100.0]);
    assert!(game.salvage_contact(tank, first));
    assert_eq!(game.entities[tank].salvage_level(), 1);
    game.catalog[6].salvage.weapons[1].damage = 3.0;
    game.set_object_orientation(tank, 0.0);
    game.attack(tank, victim).unwrap();
    for _ in 0..300 {
        game.step();
        if !game.projectiles.is_empty() {
            break;
        }
    }
    assert_eq!(game.projectiles.len(), 1);
    assert_eq!(game.projectiles[0].weapon_index, 3);
    game.stop(tank).unwrap();
    let crate_id = item(&mut game, template, [300.0, 100.0]);
    game.salvage_contact(crate_id, tank);
    assert_eq!(game.entities[tank].salvage_level(), 2);
    for _ in 0..300 {
        game.step();
        if game.projectiles.is_empty() {
            break;
        }
    }
    assert_eq!(game.entities[victim].health.to_bits(), 940.0_f32.to_bits());
}
#[test]
fn build_variation_selection_precedes_creation_and_never_spawns_the_wrapper() {
    for multiple in [false, true] {
        let (mut game, _, _) = setup();
        game.catalog[6].name = "ChassisOne".into();
        let mut second = crate::skirmish::tests::fixture_type(Faction::Gla, Kind::Vehicle);
        second.name = "ChassisTwo".into();
        let second_index = game.catalog.len();
        game.catalog.push(second);
        let mut wrapper = crate::skirmish::tests::fixture_type(Faction::Gla, Kind::Vehicle);
        wrapper.name = "Technical".into();
        wrapper.kind_of.push("WEAPON_SALVAGER".into());
        wrapper.initial_health = 0.0;
        wrapper.max_health = 0.0;
        wrapper.variations.push("ChassisOne".into());
        if multiple {
            wrapper.variations.push("ChassisTwo".into());
        }
        let index = game.catalog.len();
        game.catalog.push(wrapper);
        assert!(validate(&game.catalog[index], &game.catalog).is_ok());
        let mut expected = game.random;
        let selected = startup::random_between(&mut expected, u32::from(multiple));
        // ThingFactory chooses the variation before constructing its AI module;
        // the wrapper constructs none, and the selected NORMAL template draws three.
        for _ in 0..3 {
            startup::random_value(&mut expected);
        }
        // Three nested Idle machines precede the primary Idle entry.
        for _ in 0..4 {
            startup::random_between(&mut expected, 60);
        }
        let count = game.entities.len();
        let id = game.spawn(index, 0, [400.0, 100.0], true);
        assert_eq!(game.entities.len(), count + 1);
        assert_eq!(
            game.entities[id].type_index,
            if selected == 0 { 6 } else { second_index }
        );
        assert!(game.entities[id].alive());
        assert_eq!(game.random, expected);
    }
}
#[test]
fn missing_crate_or_variation_is_a_catalog_error_before_runtime() {
    let (mut game, _, _) = setup();
    game.catalog[6].variations.push("AbsentChassis".into());
    assert!(validate(&game.catalog[6], &game.catalog).is_err());
    game.catalog[6].variations.clear();
    game.catalog[6].salvage.drops.push(Drop {
        chance: 1.0,
        killer_kinds: Vec::new(),
        science: None,
        template: "AbsentCrate".into(),
    });
    assert!(validate(&game.catalog[6], &game.catalog).is_err());
}

#[test]
fn salvage_replaces_secondary_air_weapon_and_restores_its_clip() {
    let (mut game, tank, template) = setup();
    for level in 1..=2 {
        let mut weapon = WeaponRules::unarmed();
        weapon.name = format!("QuadUpgrade{level}Air");
        weapon.damage = 10.0;
        weapon.anti_mask = 1;
        weapon.clip_size = 9;
        game.catalog[6].salvage.secondary.push(weapon);
    }
    game.catalog[6].weapon_set_rules.replacements[1].weapons[1] = Some(5);
    game.catalog[6].weapon_set_rules.replacements[2].weapons[1] = Some(6);
    let position = game.entities[tank].position;
    for expected in [5, 6] {
        let crate_id = item(&mut game, template, position);
        game.salvage_contact(tank, crate_id);
        assert!(game.weapon_available(tank, 1));
        game.switch_weapon(tank, 1)
            .expect("native upgraded secondary");
        let actor = &game.entities[tank];
        assert_eq!(actor.weapon_index(1), expected);
        assert_eq!(actor.weapon_set.weapons[1].clip_remaining, 9);
        assert!(actor.active_weapon(&game.catalog[6]).name.ends_with("Air"));
    }
}
