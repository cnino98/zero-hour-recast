//! Original paid Burton and its HUD consumers against the installed Zero Hour archives.
use crate::{
    control_bar::{self, CommandAction, UiData},
    input::{self, CommandMode, Mouse},
    playtest::Match,
    playtest_ui::{dispatch_command, selected_command},
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    },
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{DamageType, Faction, Kind, Skirmish, special_ability::BurtonCharge};

fn issue_at(battle: &mut Match, mouse: &mut Mouse, data: &UiData, slot: usize, target: usize) {
    let action = selected_command(battle, data, slot)
        .unwrap_or_else(|| {
            panic!(
                "Burton slot {slot} unavailable with selection {:?}",
                battle.selected
            )
        })
        .action;
    dispatch_command(battle, mouse, action);
    let position = battle.simulation.entities[target].position;
    input::targeted_order(battle, mouse, position.into(), Some(target));
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
}

fn approach(game: &mut Skirmish, actor: usize, destination: [f32; 2], radius: f32) {
    game.move_to(actor, destination).unwrap();
    advance_until(game, 18_000, "ordinary approach to Burton", |game| {
        let [x, y] = game.entities[actor].position;
        (x - destination[0]).hypot(y - destination[1]) < radius
    })
    .unwrap();
    game.stop(actor).unwrap();
}

fn charge_child(game: &Skirmish, kind: usize, target: usize) -> Option<usize> {
    game.entities.iter().position(|actor| {
        actor.type_index == kind && actor.producer() == Some(target) && actor.alive()
    })
}

fn paid_scene(game: &mut Skirmish) -> (usize, usize, usize, usize, usize) {
    game.reset(Faction::Usa, Faction::China).unwrap();
    let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, builder, Kind::PowerPlant, 0).unwrap();
    construct(game, builder, Kind::SupplyCenter, 0).unwrap();
    let barracks = construct(game, builder, Kind::Barracks, 0).unwrap();
    let burton = template(game, "AmericaInfantryColonelBurton").unwrap();
    let cash = game.players[0].cash;
    assert!(game.train(barracks, burton).is_err());
    assert_eq!(game.players[0].cash, cash);
    construct(game, builder, Kind::Factory, 0).unwrap();
    let center = construct_type(
        game,
        builder,
        template(game, "AmericaStrategyCenter").unwrap(),
        0,
    )
    .unwrap();
    let actor = produce_type(game, barracks, burton, 0).unwrap();
    assert!(!game.entities[actor].chemical_suit());
    assert!(game.train(barracks, burton).is_err());
    research(game, center, "Upgrade_AmericaChemicalSuits").unwrap();
    assert!(game.entities[actor].chemical_suit());
    let armor = game.catalog[burton]
        .maintenance
        .armor_upgrade
        .as_ref()
        .unwrap();
    for damage in [
        DamageType::Poison,
        DamageType::Radiation,
        DamageType::Microwave,
    ] {
        assert_eq!(
            armor.coefficients[damage as usize].to_bits(),
            0.2_f32.to_bits()
        );
    }
    let enemy = friendly_entity(game, Kind::Builder, 1).unwrap();
    construct(game, enemy, Kind::PowerPlant, 1).unwrap();
    construct(game, enemy, Kind::SupplyCenter, 1).unwrap();
    let enemy_barracks = construct(game, enemy, Kind::Barracks, 1).unwrap();
    let guard = produce_type(
        game,
        enemy_barracks,
        template(game, "ChinaInfantryRedguard").unwrap(),
        1,
    )
    .unwrap();
    let command = friendly_entity(game, Kind::CommandCenter, 1).unwrap();
    let timed_target = produce_type(
        game,
        command,
        template(game, "ChinaVehicleDozer").unwrap(),
        1,
    )
    .unwrap();
    let remote_target = produce_type(
        game,
        command,
        template(game, "ChinaVehicleDozer").unwrap(),
        1,
    )
    .unwrap();
    let position = game.entities[actor].position;
    approach(game, remote_target, position, 220.0);
    approach(game, guard, position, 130.0);
    advance_until(game, 120, "Burton stationary stealth", |game| {
        game.entities[actor].stealthed()
    })
    .unwrap();

    (burton, actor, guard, timed_target, remote_target)
}

#[ignore = "requires original game installation"]
#[test]
fn original_burton_paid_prerequisites_commands_charges_and_chemical_suits() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let (burton, actor, guard, timed_target, remote_target) = paid_scene(&mut battle.simulation);
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let catalog = &battle.simulation.catalog;
    let data = control_bar::load(&assets, catalog, &options).unwrap();
    let mut mouse = input::load(&assets, &options, catalog).unwrap();
    drop(crate::attack_cursor::load(&assets).unwrap());
    battle.select_objects(&[actor], true);
    assert!(matches!(
        selected_command(&battle, &data, 1).unwrap().action,
        CommandAction::BurtonKnife
    ));
    assert!(matches!(
        selected_command(&battle, &data, 2).unwrap().action,
        CommandAction::BurtonCharge(BurtonCharge::Timed)
    ));
    assert!(matches!(
        selected_command(&battle, &data, 4).unwrap().action,
        CommandAction::BurtonCharge(BurtonCharge::Remote)
    ));
    assert!(matches!(
        selected_command(&battle, &data, 6).unwrap().action,
        CommandAction::BurtonDetonate
    ));
    assert!(!battle.simulation.burton_can_detonate(actor));

    issue_at(&mut battle, &mut mouse, &data, 1, guard);
    let game = &mut battle.simulation;
    let thresholds = game.catalog[burton].body_damage_thresholds;
    advance_until(game, 2_000, "Burton's authored knife windup", |game| {
        game.model_flags(&game.entities[actor], thresholds)
            .contains("PREATTACK_B")
    })
    .unwrap();
    assert!(game.entities[actor].stealthed());
    assert!(game.entities[guard].alive());
    advance_until(game, 90, "Burton's knife strike", |game| {
        !game.entities[guard].alive()
    })
    .unwrap();
    assert_eq!(game.entities[guard].last_attacker(), Some(actor));

    let timed = verify_remote_charge(&mut battle, &mut mouse, &data, actor, remote_target);
    let game = &mut battle.simulation;
    let position = game.entities[actor].position;
    approach(game, timed_target, position, 148.0);

    issue_at(&mut battle, &mut mouse, &data, 2, timed_target);
    let game = &mut battle.simulation;
    advance_until(game, 2_000, "Burton planted original timed C4", |game| {
        charge_child(game, timed, timed_target).is_some()
    })
    .unwrap();
    let timed_child = charge_child(game, timed, timed_target).unwrap();
    let placed = game.tick;
    advance_until(game, 605, "original timed C4 damage", |game| {
        !game.entities[timed_target].alive()
    })
    .unwrap();
    // LifetimeUpdate kills the charge at 600. SlowDeath fires its FINAL weapon
    // on the following logic frame after the authored 1 ms destruction delay.
    assert_eq!(game.tick - placed, 601);
    assert!(!game.entities[timed_child].alive());
}

fn verify_remote_charge(
    battle: &mut Match,
    mouse: &mut Mouse,
    data: &UiData,
    actor: usize,
    remote_target: usize,
) -> usize {
    issue_at(battle, mouse, data, 4, remote_target);
    let game = &mut battle.simulation;
    let remote = template(game, "RemoteC4Charge").unwrap();
    let timed = template(game, "TimedC4Charge").unwrap();
    advance_until(game, 2_000, "Burton planted original remote C4", |game| {
        charge_child(game, remote, remote_target).is_some()
    })
    .unwrap();
    let remote_child = charge_child(game, remote, remote_target).unwrap();
    assert!(!game.burton_charge_target(actor, BurtonCharge::Timed, remote_target));
    assert!(game.burton_can_detonate(actor));
    let remote_position = game.entities[remote_target].position;
    let escape = game.entities[friendly_entity(game, Kind::CommandCenter, 0).unwrap()].position;
    game.move_to(actor, escape).unwrap();
    advance_until(
        game,
        2_000,
        "Burton clears his remote charge blast",
        |game| {
            let [x, y] = game.entities[actor].position;
            (x - remote_position[0]).hypot(y - remote_position[1]) > 125.0
        },
    )
    .unwrap();
    game.stop(actor).unwrap();
    assert_eq!(
        game.catalog[timed].behavior.lifetimes.as_slice(),
        &[[600; 2]]
    );
    assert_eq!(
        game.catalog[remote].behavior.sticky_bombs[0].ping_sound,
        "BombTickRemote"
    );
    assert_eq!(
        game.catalog[timed].behavior.sticky_bombs[0].ping_sound,
        "BombTickTimed"
    );

    let action = selected_command(battle, data, 6).unwrap().action;
    dispatch_command(battle, mouse, action);
    let game = &mut battle.simulation;
    let mut detonated = false;
    advance_until(game, 30, "original C4 damage", |game| {
        detonated |= game.presentation.object_fx_events.iter().any(|event| {
            event.object == remote_child && event.effect == "WeaponFX_TNTStickyBombDetonation"
        });
        !game.entities[remote_target].alive()
    })
    .unwrap();
    assert!(!game.entities[remote_child].alive());
    assert!(detonated);
    assert!(game.entities[actor].alive());

    timed
}
