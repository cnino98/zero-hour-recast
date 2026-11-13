use super::CommandMode;
use crate::{
    control_bar::{self, CommandAction, UiData},
    playtest::Match,
    playtest_ui::{dispatch_command, selected_command},
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{DeathType, Faction, Kind, Skirmish};

#[ignore = "requires original game installation"]
#[test]
fn installed_bomb_truck_paid_disguise_and_all_payload_combinations() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let root = generals_assets::game_root().unwrap();
    let options = root.join("UserData/Options.ini");
    let data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = crate::input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    let game = &mut battle.simulation;
    game.reset_sandbox(Faction::Gla).unwrap();
    let worker = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, worker, Kind::SupplyCenter, 0).unwrap();
    construct(game, worker, Kind::Barracks, 0).unwrap();
    let dealer = construct(game, worker, Kind::Factory, 0).unwrap();
    let palace_type = template(game, "GLAPalace").unwrap();
    let palace = construct_type(game, worker, palace_type, 0).unwrap();
    let truck_type = template(game, "GLAVehicleBombTruck").unwrap();
    let technical_type = template(game, "GLAVehicleTechnical").unwrap();
    let infantry_type = template(game, "GLAInfantryRebel").unwrap();
    let barracks = friendly_entity(game, Kind::Barracks, 0).unwrap();
    let technical = paid_unit(game, dealer, technical_type);
    let infantry = paid_unit(game, barracks, infantry_type);
    let range = isolated_range(game, technical, palace_type);
    let blast_target = paid_unit(game, barracks, infantry_type);
    move_near(game, blast_target, [range[0] + 22.0, range[1]]);
    for upgraded in [false, true] {
        if upgraded {
            let game = &mut battle.simulation;
            let beta = research_index(game, palace, "Upgrade_GLAAnthraxBeta");
            let cost = game.catalog[palace_type].maintenance.research[beta].cost;
            afford(game, cost);
            let cash = game.players[0].cash;
            game.queue_upgrade(palace, beta).unwrap();
            assert_eq!(game.players[0].cash, cash - cost);
            advance_until(game, 3_000, "paid Anthrax Beta", |game| {
                game.has_upgrade(0, "Upgrade_GLAAnthraxBeta")
            })
            .unwrap();
        }
        for (bio, high, effect, field) in payload_cases(upgraded) {
            let truck = paid_unit(&mut battle.simulation, dealer, truck_type);
            battle.select_objects(&[truck], true);
            if bio {
                check_payload_upgrade(
                    &mut battle,
                    &mut mouse,
                    &data,
                    truck,
                    7,
                    "Upgrade_GLABombTruckBioBomb",
                );
            }
            if high {
                check_payload_upgrade(
                    &mut battle,
                    &mut mouse,
                    &data,
                    truck,
                    8,
                    "Upgrade_GLABombTruckHighExplosiveBomb",
                );
            }
            if !upgraded && bio && high {
                check_disguise(&mut battle, &mut mouse, &data, truck, technical, infantry);
                reveal_by_approach(&mut battle.simulation, truck, infantry);
            }
            if bio || high {
                let load = match (bio, high) {
                    (true, true) => "Bombload04",
                    (true, false) => "Bombload02",
                    (false, true) => "Bombload03",
                    (false, false) => unreachable!(),
                };
                assert!(
                    battle.simulation.entities[truck]
                        .sub_object_overrides()
                        .iter()
                        .any(|entry| entry.name == load && entry.visible)
                );
            }
            let action = selected_command(&battle, &data, 3).unwrap().action;
            assert!(matches!(action, CommandAction::BombTruckDetonate));
            move_near(&mut battle.simulation, truck, range);
            dispatch_command(&mut battle, &mut mouse, action);
            check_detonation(&mut battle.simulation, truck, high, effect, field);
            if !upgraded && !bio && !high {
                assert!(!battle.simulation.entities[blast_target].alive());
            }
        }
    }
}

fn payload_cases(upgraded: bool) -> [(bool, bool, &'static str, Option<&'static str>); 3] {
    if upgraded {
        [
            (
                true,
                false,
                "BombTruckAnthraxBombEffect",
                Some("PoisonFieldUpgradedMedium"),
            ),
            (
                true,
                true,
                "BombTruckHighExplosiveAnthraxBombEffect",
                Some("PoisonFieldUpgradedMedium"),
            ),
            (false, true, "BombTruckHighExplosiveBombEffect", None),
        ]
    } else {
        [
            (false, false, "BombTruckDefaultBombEffect", None),
            (
                true,
                false,
                "BombTruckBioBombEffect",
                Some("PoisonFieldMedium"),
            ),
            (
                true,
                true,
                "BombTruckHighExplosiveBioBombEffect",
                Some("PoisonFieldMedium"),
            ),
        ]
    }
}

fn afford(game: &mut Skirmish, cost: u32) {
    advance_until(
        game,
        18_000,
        "GLA supply income for Bomb Truck acceptance",
        |game| game.players[0].cash >= cost,
    )
    .unwrap();
}

fn paid_unit(game: &mut Skirmish, producer: usize, kind: usize) -> usize {
    afford(game, game.catalog[kind].cost);
    produce_type(game, producer, kind, 0).unwrap()
}

fn isolated_range(game: &Skirmish, vehicle: usize, footprint: usize) -> [f32; 2] {
    let base = game.entities[friendly_entity(game, Kind::CommandCenter, 0).unwrap()].position;
    (0_u8..16)
        .find_map(|index| {
            let angle = f32::from(index) * std::f32::consts::TAU / 16.0;
            let point = [
                angle.cos().mul_add(650.0, base[0]),
                angle.sin().mul_add(650.0, base[1]),
            ];
            game.placement_near(footprint, point).filter(|site| {
                game.build_site_reachable(vehicle, *site)
                    && game
                        .entities
                        .iter()
                        .filter(|actor| actor.complete())
                        .all(|actor| {
                            (actor.position[0] - site[0]).hypot(actor.position[1] - site[1]) > 300.0
                        })
            })
        })
        .expect("isolated accessible detonation site")
}

fn move_near(game: &mut Skirmish, unit: usize, point: [f32; 2]) {
    game.move_to(unit, point).unwrap();
    advance_until(game, 3_000, "movement to Bomb Truck test site", |game| {
        let position = game.entities[unit].position;
        (position[0] - point[0]).hypot(position[1] - point[1]) < 12.0
    })
    .unwrap();
    game.stop(unit).unwrap();
}

fn research_index(game: &Skirmish, id: usize, name: &str) -> usize {
    game.catalog[game.entities[id].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == name)
        .unwrap()
}

fn check_payload_upgrade(
    battle: &mut Match,
    mouse: &mut super::Mouse,
    data: &UiData,
    truck: usize,
    slot: usize,
    name: &str,
) {
    let index = research_index(&battle.simulation, truck, name);
    let unit = battle.simulation.entities[truck].type_index;
    let research = &battle.simulation.catalog[unit].maintenance.research[index];
    assert_eq!(
        (research.cost, research.frames, research.player),
        (500, 150, false)
    );
    afford(&mut battle.simulation, 500);
    let cash = battle.simulation.players[0].cash;
    let action = selected_command(battle, data, slot).unwrap().action;
    assert!(matches!(action, CommandAction::Research(value) if value == index));
    dispatch_command(battle, mouse, action);
    assert_eq!(battle.simulation.players[0].cash, cash - 500);
    advance_until(&mut battle.simulation, 160, name, |game| {
        game.object_has_upgrade(truck, name)
    })
    .unwrap();
}

fn check_disguise(
    battle: &mut Match,
    mouse: &mut super::Mouse,
    data: &UiData,
    truck: usize,
    technical: usize,
    infantry: usize,
) {
    let action = selected_command(battle, data, 1).unwrap().action;
    assert!(matches!(action, CommandAction::BombTruckDisguise));
    dispatch_command(battle, mouse, action);
    assert!(matches!(
        mouse.pending_command,
        CommandMode::BombTruckDisguise
    ));
    crate::input::targeted_order(
        battle,
        mouse,
        battle.simulation.entities[infantry].position.into(),
        Some(infantry),
    );
    assert!(matches!(
        mouse.pending_command,
        CommandMode::BombTruckDisguise
    ));
    crate::input::targeted_order(
        battle,
        mouse,
        battle.simulation.entities[technical].position.into(),
        Some(technical),
    );
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
    advance_until(
        &mut battle.simulation,
        120,
        "Bomb Truck drawable swap",
        |game| game.entities[truck].disguised_appearance().is_some(),
    )
    .unwrap();
    let game = &battle.simulation;
    let appearance = game.entities[truck].disguised_appearance().unwrap();
    assert_eq!(appearance.type_index, game.entities[technical].type_index);
    assert_eq!(appearance.owner, 0);
    assert_eq!(game.entities[truck].owner, 0);
    assert!(
        game.presentation
            .object_sounds
            .iter()
            .any(|event| { event.object == truck && event.name == "BombTruckDisguiseStarted" })
    );
}

fn reveal_by_approach(game: &mut Skirmish, truck: usize, target: usize) {
    game.force_attack(truck, target).unwrap();
    advance_until(
        game,
        300,
        "Bomb Truck enters its original reveal distance",
        |game| {
            let at = game.entities[truck].position;
            let goal = game.entities[target].position;
            (at[0] - goal[0]).hypot(at[1] - goal[1]) <= 90.0
        },
    )
    .unwrap();
    game.step();
    game.stop(truck).unwrap();
    advance_until(
        game,
        90,
        "Bomb Truck restores its original drawable",
        |game| game.entities[truck].disguised_appearance().is_none(),
    )
    .unwrap();
    assert!(
        game.presentation
            .object_fx_events
            .iter()
            .any(|event| { event.object == truck && event.effect == "FX_BombTruckDisguiseReveal" })
    );
}

fn check_detonation(
    game: &mut Skirmish,
    truck: usize,
    high: bool,
    effect: &str,
    field: Option<&str>,
) {
    let damage = if high {
        "BombTruckHighExplosionBombDamage"
    } else {
        "BombTruckDefaultBombDamage"
    };
    let mut shots = Vec::new();
    advance_until(game, 120, "Bomb Truck detonation and hulk", |game| {
        shots.extend(
            game.presentation
                .shots
                .iter()
                .filter(|shot| shot.source == truck)
                .filter_map(|shot| {
                    game.catalog[shot.weapon_type].weapon_template(shot.weapon_index)
                })
                .map(|weapon| weapon.name.clone()),
        );
        game.entities.iter().any(|entity| {
            entity.producer() == Some(truck)
                && game.catalog[entity.type_index].name == "DeadBombTruckHulk"
        })
    })
    .unwrap();
    assert!(matches!(
        game.entities[truck].death_type(),
        DeathType::Suicided
    ));
    assert_eq!(shots, ["SuicideWeapon", damage, effect]);
    assert_eq!(
        game.hazards
            .iter()
            .find(|hazard| hazard.source == truck)
            .map(
                |hazard| game.catalog[game.entities[hazard.object].type_index]
                    .name
                    .as_str()
            ),
        field
    );
}
