use super::*;
use crate::skirmish::{ArmorUpgrade, Kind};

fn contact() -> (Skirmish, usize, usize) {
    let mut game = crate::skirmish::tests::game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let tank = game.type_for(1, Kind::Tank).unwrap();
    game.catalog[infantry].live_physics =
        crate::skirmish::projectiles::fixtures::shell_template(100, false).live_physics;
    crate::skirmish::tests::register_update(&mut game.catalog[infantry], "PhysicsBehavior");
    game.catalog[infantry].crush.squish_module = Some(1);
    crate::skirmish::tests::register_behavior(&mut game.catalog[infantry], "SquishCollide", 0x10);
    game.catalog[infantry].crush.crushable_level = 255;
    game.catalog[tank].crush.crusher_level = 1;
    let crusher = game.spawn(tank, 1, [400.0, 400.0], true);
    let victim = game.spawn(infantry, 0, [405.0, 400.0], true);
    game.entities[crusher].ai.motion.velocity = [1.0, 0.0];
    (game, victim, crusher)
}

#[test]
fn crushing_contact_uses_damage_callbacks_and_credits_one_death() {
    for before in [true, false] {
        let (mut game, victim, crusher) = contact();
        let infantry = game.entities[victim].type_index;
        game.catalog[infantry].crush.squish_module = Some(usize::from(!before));
        game.catalog[infantry].crush.physics_module = Some(usize::from(before));
        let squish = game.catalog[infantry].behavior_modules.pop().unwrap();
        if before {
            game.catalog[infantry].behavior_modules.insert(0, squish);
        } else {
            game.catalog[infantry].behavior_modules.push(squish);
        }
        let score = game.players[1].statistics.units_destroyed;
        // The true flag suppresses PhysicsBehavior, never SquishCollide.
        game.object_contact(victim, crusher, true);
        assert!(!game.entities[victim].alive());
        assert!(matches!(
            game.entities[victim].damage_state.death_type,
            DeathType::Crushed
        ));
        assert_eq!(
            game.entities[victim].damage_state.last_attacker,
            Some(crusher)
        );
        assert!(matches!(game.entities[victim].ai.order, Order::Idle));
        assert_eq!(game.players[1].statistics.units_destroyed, score + 1);
        game.object_contact(victim, crusher, true);
        assert_eq!(game.players[1].statistics.units_destroyed, score + 1);
    }
}

#[test]
fn squish_requires_strictly_positive_velocity_toward_the_infantry() {
    for velocity in [[0.0, 0.0], [-1.0, 0.0], [0.0, 1.0]] {
        let (mut game, victim, crusher) = contact();
        game.entities[crusher].ai.motion.velocity = velocity;
        game.squish_contact(victim, crusher);
        assert!(game.entities[victim].alive());
    }
    let (mut game, victim, crusher) = contact();
    game.entities[crusher].ai.motion.velocity = [f32::EPSILON, 0.0];
    game.squish_contact(victim, crusher);
    assert!(!game.entities[victim].alive());
}

#[test]
fn full_infantry_overlap_does_not_replace_the_native_one_unit_squish_radius() {
    let (mut game, victim, crusher) = contact();
    game.entities[victim].position = [407.0, 400.0];
    game.squish_contact(victim, crusher);
    assert!(game.entities[victim].alive());
    game.entities[victim].position[0] = 406.0;
    game.squish_contact(victim, crusher);
    assert!(!game.entities[victim].alive());
}

#[test]
fn squish_uses_the_source_geometry_query_height_gate() {
    let (mut game, victim, crusher) = contact();
    let infantry = game.entities[victim].type_index;
    let tank = game.entities[crusher].type_index;
    game.catalog[tank].geometry.shape = Shape::Sphere;
    game.set_object_position(crusher, [400.0, 400.0, 12.0]);
    // The tank sphere overlaps the victim's top at ten, but the source's
    // preliminary canonical-position gate excludes its below-position half.
    game.squish_contact(victim, crusher);
    assert!(game.entities[victim].alive());
    game.set_object_position(crusher, [400.0, 400.0, 10.0]);
    game.catalog[infantry].geometry.major = 20.0;
    game.entities[victim].position[0] = 400.5;
    game.squish_contact(victim, crusher);
    assert!(!game.entities[victim].alive());
}

#[test]
fn squish_requires_its_module_crusher_physics_and_enemy_relationship() {
    for excluded in 0..4 {
        let (mut game, victim, crusher) = contact();
        let infantry = game.entities[victim].type_index;
        let tank = game.entities[crusher].type_index;
        match excluded {
            0 => game.catalog[infantry].crush.squish_module = None,
            1 => game.catalog[tank].crush.physics_module = None,
            2 => game.catalog[tank].crush.crusher_level = 0,
            _ => game.entities[victim].owner = game.entities[crusher].owner,
        }
        game.squish_contact(victim, crusher);
        assert!(game.entities[victim].alive());
    }
}

#[test]
fn crush_armor_can_prevent_or_reduce_damage_instead_of_forcing_death() {
    let (mut game, victim, crusher) = contact();
    let infantry = game.entities[victim].type_index;
    game.catalog[infantry].armor[crate::skirmish::DamageType::Crush as usize] = 0.0;
    let health = game.entities[victim].health;
    game.squish_contact(victim, crusher);
    assert_eq!(game.entities[victim].health.to_bits(), health.to_bits());
    game.catalog[infantry].armor[crate::skirmish::DamageType::Crush as usize] = 0.000_01;
    game.squish_contact(victim, crusher);
    let damage = 999_999.0_f32 * 0.000_01;
    let expected = health - damage;
    assert_eq!(game.entities[victim].health.to_bits(), expected.to_bits());
    assert!(matches!(
        game.entities[victim].damage_state.death_type,
        DeathType::Normal
    ));
}

#[test]
fn active_armor_upgrade_and_battle_plan_apply_to_crush_damage() {
    let (mut game, victim, crusher) = contact();
    let infantry = game.entities[victim].type_index;
    game.catalog[infantry].maintenance.armor_upgrade = Some(ArmorUpgrade {
        trigger: "CrushArmor".into(),
        coefficients: [
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.000_01, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0, 1.0,
        ],
    });
    crate::skirmish::tests::register_behavior(&mut game.catalog[infantry], "ArmorUpgrade", 0x80);
    game.catalog[infantry]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers = vec!["CrushArmor".into()];
    game.destroy_maintenance_object(victim);
    game.process_update_destruction();
    let victim = game.spawn(infantry, 0, [405.0, 400.0], true);
    game.players[0].upgrades.push("CrushArmor".into());
    game.activate_upgrade_modules(victim);
    game.entities[victim].battle_bonuses.armor = 0.5;
    let health = game.entities[victim].health;
    game.squish_contact(victim, crusher);
    let armored_damage = 999_999.0_f32 * 0.000_01;
    let damage = armored_damage * 0.5;
    let expected = health - damage;
    assert_eq!(game.entities[victim].health.to_bits(), expected.to_bits());
}

#[test]
fn hijacker_immunity_only_applies_to_the_current_goal_object() {
    let (mut game, victim, crusher) = contact();
    let infantry = game.entities[victim].type_index;
    game.catalog[infantry].crush.hijacker = true;
    game.entities[victim].ai.command.ability_goal = Some(crusher);
    game.squish_contact(victim, crusher);
    assert!(game.entities[victim].alive());
    game.entities[victim].ai.command.ability_goal = None;
    game.squish_contact(victim, crusher);
    assert!(!game.entities[victim].alive());
}

#[test]
fn partition_contact_dispatch_squishes_for_both_dirty_list_orientations() {
    for reversed in [false, true] {
        let (mut game, victim, crusher) = contact();
        game.partition.clear();
        let ids = if reversed {
            [victim, crusher]
        } else {
            [crusher, victim]
        };
        for id in ids {
            game.register_object_partition(id);
        }
        game.update_ground_contacts();
        assert!(!game.entities[victim].alive());
        assert_eq!(
            game.entities[victim].damage_state.last_attacker,
            Some(crusher)
        );
    }
}

#[test]
fn no_collisions_stops_the_object_visitor_before_squish() {
    let (mut game, victim, crusher) = contact();
    game.entities[victim].object_status |= 1 << 5;
    game.object_contact(victim, crusher, false);
    assert!(game.entities[victim].alive());
    game.entities[victim].object_status &= !(1 << 5);
    game.object_contact(victim, crusher, false);
    assert!(!game.entities[victim].alive());
}

#[test]
fn unknown_collide_registration_is_rejected_with_authored_identity() {
    let mut unit =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Vehicle);
    crate::skirmish::tests::register_behavior(&mut unit, "UntranslatedCollide", 0x10);
    let error = validate(&unit).unwrap_err();
    assert!(error.contains("unbound COLLIDE UntranslatedCollide TestModule"));
    assert!(error.contains("test.ini:1"));
}

#[test]
fn squish_uses_copied_object_geometry_without_changing_the_template() {
    let (mut game, victim, crusher) = contact();
    let donor = game.spawn(game.entities[crusher].type_index, 1, [600.0, 400.0], true);
    game.set_dynamic_geometry(donor, [20.0, 20.0, 20.0]);
    game.set_object_position(victim, [415.0, 400.0, 0.0]);
    game.squish_contact(victim, crusher);
    assert!(game.entities[victim].alive());
    let template_radius = game.catalog[game.entities[crusher].type_index]
        .geometry
        .major;
    game.copy_object_geometry(crusher, donor);
    game.squish_contact(victim, crusher);
    assert!(!game.entities[victim].alive());
    assert_eq!(
        game.catalog[game.entities[crusher].type_index]
            .geometry
            .major
            .to_bits(),
        template_radius.to_bits()
    );
}
