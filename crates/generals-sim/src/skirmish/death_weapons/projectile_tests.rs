use super::*;
use crate::skirmish::{
    ProjectileFxStage, UnitType, WeaponFxEvent,
    projectiles::fixtures::{launch_shell, shell_template},
    tests::game,
};

fn scorpion_death_modules(unit: &mut UnitType) {
    for (name, active, triggers, conflicts, all) in [
        (
            "ToxinShellWeapon",
            false,
            vec!["Upgrade_GLAToxinShells"],
            vec!["Upgrade_GLAAnthraxBeta", "Chem_Upgrade_GLAAnthraxGamma"],
            false,
        ),
        (
            "ToxinShellWeaponUpgraded",
            false,
            vec!["Upgrade_GLAAnthraxBeta", "Upgrade_GLAToxinShells"],
            vec!["Chem_Upgrade_GLAAnthraxGamma"],
            true,
        ),
        (
            "Chem_ToxinShellWeaponGamma",
            false,
            vec!["Chem_Upgrade_GLAAnthraxGamma", "Upgrade_GLAToxinShells"],
            vec![],
            true,
        ),
        (
            "ScorpionTankGunFXWeapon",
            true,
            vec![],
            vec!["Upgrade_GLAToxinShells"],
            false,
        ),
    ] {
        let mut weapon = WeaponRules::unarmed();
        weapon.name = name.into();
        crate::skirmish::tests::register_behavior(unit, "FireWeaponWhenDeadBehavior", 0x0082);
        unit.behavior.death_weapons.push(Rules {
            weapon: Some(weapon),
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
            removals: Vec::new(),
            upgrade_fx: String::new(),
            death_mask: u32::MAX,
            starts_active: active,
            triggers: triggers.into_iter().map(String::from).collect(),
            conflicts: conflicts.into_iter().map(String::from).collect(),
            requires_all: all,
        });
        let rules = unit.behavior.death_weapons.last().unwrap();
        let upgrade = &mut unit.behavior_modules.last_mut().unwrap().upgrade;
        upgrade.triggers.clone_from(&rules.triggers);
        upgrade.conflicts.clone_from(&rules.conflicts);
        upgrade.requires_all = all;
    }
}

fn fixture(lifespan: u32, kill: bool) -> (Skirmish, usize, usize, usize) {
    let mut game = game();
    let source = game.spawn(6, 0, [20.0, 20.0], true);
    let target = game.spawn(5, 1, [180.0, 20.0], true);
    let mut shell = shell_template(lifespan, kill);
    scorpion_death_modules(&mut shell);
    let kind = game.catalog.len();
    game.catalog.push(shell);
    game.set_projectile_terrain(32, 32, [0.0; 2], vec![0.0; 1024], 40.0)
        .unwrap();
    game.tick = 10;
    let (_, object) = launch_shell(&mut game, source, kind, target);
    (game, source, target, object)
}

#[test]
fn toxin_shell_death_masks_use_projectile_owner_and_all_triggers_before_conflicts() {
    for (upgrades, expected) in [
        (vec![], "ScorpionTankGunFXWeapon"),
        (vec!["Upgrade_GLAAnthraxBeta"], "ScorpionTankGunFXWeapon"),
        (vec!["Upgrade_GLAToxinShells"], "ToxinShellWeapon"),
        (
            vec!["Upgrade_GLAToxinShells", "Upgrade_GLAAnthraxBeta"],
            "ToxinShellWeaponUpgraded",
        ),
        (
            vec![
                "Upgrade_GLAToxinShells",
                "Upgrade_GLAAnthraxBeta",
                "Chem_Upgrade_GLAAnthraxGamma",
            ],
            "Chem_ToxinShellWeaponGamma",
        ),
    ] {
        let (mut game, source, _, object) = fixture(1, true);
        game.players[0].upgrades = upgrades.into_iter().map(String::from).collect();
        game.activate_upgrade_modules(object);
        game.entities[source].owner = 1;
        game.entities[source].veterancy = 3;
        game.players[1].upgrades = vec!["Upgrade_GLAToxinShells".into()];
        game.presentation.weapon_fx.clear();
        game.tick = 11;
        game.run_object_updates();
        assert!(game.entities[object].is_effectively_dead());
        assert_eq!(game.presentation.shots.len(), 1);
        let shot = &game.presentation.shots[0];
        assert_eq!(shot.source, object);
        assert_eq!(
            game.catalog[shot.weapon_type]
                .weapon_template(shot.weapon_index)
                .unwrap()
                .name,
            expected
        );
        assert_eq!(shot.owner, 0);
        // ExperienceTracker's sink forwards XP; it does not copy the launcher's veterancy level.
        assert_eq!(shot.veterancy, 0);
        assert_eq!(
            shot.source_position.map(f32::to_bits),
            [20.0_f32, 20.0, 10.0].map(f32::to_bits)
        );
        assert_eq!(
            shot.target_position.map(f32::to_bits),
            shot.source_position.map(f32::to_bits)
        );
        assert!(shot.target.is_none());
        assert!(
            matches!(game.presentation.weapon_fx.as_slice(), [WeaponFxEvent::Projectile(event), WeaponFxEvent::BodyParticles(index), WeaponFxEvent::Shot(0)]
            if matches!(event.stage, ProjectileFxStage::Detonation)
                && game.presentation.body_particles[*index].pose.object == object)
        );
        game.tick += 1;
        game.run_object_updates();
        assert_eq!(game.presentation.shots.len(), 1);
    }
}

#[test]
fn retained_dead_target_finishes_path_and_destroy_only_shell_omits_die_callbacks() {
    for kill in [false, true] {
        let (mut game, source, target, object) = fixture(300, kill);
        game.apply_hit(target, source, game.entities[target].max_health);
        assert!(!game.entities[target].alive());
        assert!(game.entities[target].exists());
        let mut moved = false;
        for _ in 0..300 {
            game.tick += 1;
            game.run_object_updates();
            moved |= game.object_position(object)[0] > 20.0;
            if game.entities[object].is_effectively_dead()
                || game.update_objects[game.entities[object].update_owner].destroy_requested
            {
                break;
            }
        }
        assert!(moved);
        assert_eq!(game.entities[object].is_effectively_dead(), kill);
        assert_eq!(
            game.update_objects[game.entities[object].update_owner].destroy_requested,
            !kill
        );
        assert_eq!(game.presentation.shots.len(), usize::from(kill));
        if kill {
            assert!(game.presentation.shots[0].source_position[0] > 100.0);
        }
        game.tick += 1;
        game.run_object_updates();
        assert_eq!(game.presentation.shots.len(), usize::from(kill));
        game.process_update_destruction();
        if !kill {
            assert!(!game.entities[object].exists());
            assert!(game.projectiles.is_empty());
        }
    }
}
