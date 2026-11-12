//! Paid original tanks and Palace research exercise shell death FX and poison OCLs.
use crate::{
    cleanup_verification,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    },
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{
    DeathType, Faction, Kind, ProjectileFxStage, Skirmish, WeaponFxEvent,
};

fn terminal_shot(game: &mut Skirmish, tank: usize, target: usize, expected: Option<&str>) {
    game.attack(tank, target).unwrap();
    advance_until(
        game,
        12_000,
        "paid original tank approaching and firing shell",
        |game| {
            game.projectiles
                .iter()
                .any(|projectile| projectile.source == tank)
        },
    )
    .unwrap();
    let projectile = game
        .projectiles
        .iter()
        .find(|projectile| projectile.source == tank)
        .unwrap();
    let object = projectile.object.unwrap();
    let projectile = projectile.id;
    game.stop(tank).unwrap();
    // An in-flight shell retains its controlling player after its launcher changes hands.
    game.entities[tank].owner = 1;
    let health = game.entities[target].health;
    for _ in 0..300 {
        game.step();
        let callbacks: Vec<_> = game
            .presentation
            .shots
            .iter()
            .enumerate()
            .filter(|(_, shot)| {
                shot.source == object
                    && game.catalog[game.entities[object].type_index]
                        .behavior
                        .death_weapons
                        .iter()
                        .any(|rules| {
                            rules.weapon.as_ref().is_some_and(|rules| {
                                game.catalog[shot.weapon_type]
                                    .weapon_template(shot.weapon_index)
                                    .is_some_and(|weapon| weapon.name == rules.name)
                            })
                        })
            })
            .collect();
        if game.projectiles.iter().any(|shot| shot.id == projectile) {
            assert!(callbacks.is_empty());
            continue;
        }
        assert!(game.entities[target].health < health);
        assert_terminal_callback(game, object, projectile, expected, &callbacks);
        game.entities[tank].owner = 0;
        for _ in 0..10 {
            game.step();
            assert!(game.presentation.shots.iter().all(|shot| {
                game.catalog[shot.weapon_type]
                    .weapon_template(shot.weapon_index)
                    .is_none_or(|weapon| {
                        !matches!(
                            weapon.name.as_str(),
                            "ToxinShellWeapon"
                                | "ToxinShellWeaponUpgraded"
                                | "Chem_ToxinShellWeaponGamma"
                                | "ScorpionTankGunFXWeapon"
                        )
                    })
            }));
        }
        return;
    }
    panic!("paid original shell never reached its terminal callback");
}

fn assert_terminal_callback(
    game: &Skirmish,
    object: usize,
    projectile: u64,
    expected: Option<&str>,
    callbacks: &[(usize, &generals_sim::skirmish::Shot)],
) {
    assert_eq!(callbacks.len(), usize::from(expected.is_some()));
    if let Some(expected) = expected {
        let (shot_index, shot) = callbacks[0];
        let unit = &game.catalog[shot.weapon_type];
        let weapon = unit.weapon_template(shot.weapon_index).unwrap();
        assert_eq!(weapon.name, expected);
        assert_eq!(shot.owner, 0);
        assert!(shot.target.is_none());
        assert_eq!(
            shot.source_position.map(f32::to_bits),
            shot.target_position.map(f32::to_bits)
        );
        let detonation = game.presentation.weapon_fx.iter().position(|event| {
            matches!(event, WeaponFxEvent::Projectile(event) if event.projectile == projectile && matches!(event.stage, ProjectileFxStage::Detonation) && event.position.map(f32::to_bits) == shot.source_position.map(f32::to_bits))
        }).unwrap();
        let death_fx = game
            .presentation
            .weapon_fx
            .iter()
            .position(|event| matches!(event, WeaponFxEvent::Shot(index) if *index == shot_index))
            .unwrap();
        assert!(detonation < death_fx);
        if let Some(rules) = &weapon.hazard {
            let field = game
                .hazards
                .iter()
                .find(|field| field.source == object && field.created == game.tick)
                .unwrap();
            assert_eq!(field.owner, 0);
            assert_eq!(
                field.position.map(f32::to_bits),
                [
                    shot.source_position[0],
                    shot.source_position[1],
                    game.ground_height([shot.source_position[0], shot.source_position[1]])
                ]
                .map(f32::to_bits)
            );
            assert_eq!(
                rules.name,
                if expected == "ToxinShellWeapon" {
                    "PoisonFieldSmall"
                } else {
                    "PoisonFieldUpgradedSmall"
                }
            );
            assert!(
                matches!(rules.death_type, DeathType::Poisoned) == (expected == "ToxinShellWeapon")
            );
            assert!(
                matches!(rules.death_type, DeathType::PoisonedBeta)
                    == (expected == "ToxinShellWeaponUpgraded")
            );
        } else {
            assert_eq!(expected, "ScorpionTankGunFXWeapon");
            assert!(game.hazards.is_empty());
        }
    } else {
        assert!(game.hazards.is_empty());
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paid_scorpion_and_marauder_toxin_shells_select_original_normal_then_beta_fields() {
    for name in ["GLATankScorpion", "GLATankMarauder"] {
        let (_, mut battle) = cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        game.reset(Faction::Gla, Faction::Usa).unwrap();
        let worker = friendly_entity(game, Kind::Builder, 0).unwrap();
        construct(game, worker, Kind::SupplyCenter, 0).unwrap();
        construct(game, worker, Kind::Barracks, 0).unwrap();
        let factory = construct(game, worker, Kind::Factory, 0).unwrap();
        let palace_kind = template(game, "GLAPalace").unwrap();
        let palace = construct_type(game, worker, palace_kind, 0).unwrap();
        if name == "GLATankMarauder" {
            game.purchase_science(0, "SCIENCE_MarauderTank").unwrap();
        }
        let kind = template(game, name).unwrap();
        let tank = produce_type(game, factory, kind, 0).unwrap();
        let target = friendly_entity(game, Kind::CommandCenter, 1).unwrap();
        terminal_shot(
            game,
            tank,
            target,
            (name == "GLATankScorpion").then_some("ScorpionTankGunFXWeapon"),
        );
        research(game, palace, "Upgrade_GLAToxinShells").unwrap();
        terminal_shot(game, tank, target, Some("ToxinShellWeapon"));
        research(game, palace, "Upgrade_GLAAnthraxBeta").unwrap();
        terminal_shot(game, tank, target, Some("ToxinShellWeaponUpgraded"));
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_shell_callback_inventory_keeps_authored_order_masks_and_gamma_field() {
    let (_, battle) = cleanup_verification::tests::installed();
    for (name, expected) in [
        (
            "GLATankScorpion",
            vec![
                "ToxinShellWeapon",
                "ToxinShellWeaponUpgraded",
                "Chem_ToxinShellWeaponGamma",
                "ScorpionTankGunFXWeapon",
            ],
        ),
        (
            "GLATankMarauder",
            vec![
                "Chem_ToxinShellWeaponGamma",
                "ToxinShellWeaponUpgraded",
                "ToxinShellWeapon",
            ],
        ),
    ] {
        let kind = template(&battle.simulation, name).unwrap();
        let launcher = &battle.simulation.catalog[kind];
        let projectile = launcher
            .weapon
            .motion
            .as_ref()
            .unwrap()
            .projectile_type
            .unwrap();
        let unit = &battle.simulation.catalog[projectile];
        let names: Vec<_> = unit
            .behavior
            .death_weapons
            .iter()
            .map(|rules| rules.weapon.as_ref().unwrap().name.as_str())
            .collect();
        assert_eq!(names, expected);
        let gamma = unit
            .behavior
            .death_weapons
            .iter()
            .find(|rules| {
                rules
                    .weapon
                    .as_ref()
                    .is_some_and(|weapon| weapon.name == "Chem_ToxinShellWeaponGamma")
            })
            .unwrap();
        assert!(gamma.requires_all);
        assert_eq!(
            gamma.triggers,
            ["Chem_Upgrade_GLAAnthraxGamma", "Upgrade_GLAToxinShells"]
        );
        assert!(matches!(
            gamma
                .weapon
                .as_ref()
                .unwrap()
                .hazard
                .as_ref()
                .unwrap()
                .death_type,
            DeathType::PoisonedGamma
        ));
        let normal = unit
            .behavior
            .death_weapons
            .iter()
            .find(|rules| {
                rules
                    .weapon
                    .as_ref()
                    .is_some_and(|weapon| weapon.name == "ToxinShellWeapon")
            })
            .unwrap();
        assert_eq!(
            normal.conflicts,
            ["Upgrade_GLAAnthraxBeta", "Chem_Upgrade_GLAAnthraxGamma"]
        );
    }
}
