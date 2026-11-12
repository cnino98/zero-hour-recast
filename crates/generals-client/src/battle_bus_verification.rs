//! Paid Palace/Battle Bus production, armed cargo and ordinary incoming damage.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

pub struct Setup {
    pub bus: usize,
    pub riders: Vec<usize>,
    pub attacker: usize,
    pub target: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let dealer = construct(game, worker, Kind::Factory, 0)?;
    let kind = template(game, "GLAVehicleBattleBus")?;
    let cash = game.players[0].cash;
    require(
        game.train(dealer, kind).is_err() && game.players[0].cash == cash,
        "Battle Bus bypassed its original Palace prerequisite",
    )?;
    let palace = template(game, "GLAPalace")?;
    construct_type(game, worker, palace, 0)?;
    require(
        game.catalog[kind].cost == 1000
            && game.catalog[kind].build_seconds.to_bits() == 15.0_f32.to_bits(),
        "Battle Bus original production cost/time changed",
    )?;
    let bus = produce_type(game, dealer, kind, 0)?;
    let rebel = template(game, "GLAInfantryRebel")?;
    let mut riders = Vec::new();
    for _ in 0..2 {
        let id = produce_type(game, barracks, rebel, 0)?;
        game.enter(id, bus)?;
        advance_until(game, 7200, "paid infantry boarding Battle Bus", |game| {
            game.entities[id].contained_by() == Some(bus)
        })?;
        riders.push(id);
    }
    let dozer = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, dozer, Kind::PowerPlant, 1)?;
    construct(game, dozer, Kind::SupplyCenter, 1)?;
    let factory = construct(game, dozer, Kind::Factory, 1)?;
    let tank = template(game, "AmericaTankCrusader")?;
    let attacker = produce_type(game, factory, tank, 1)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let base = game.entities[target].position;
    let point = [base[0], base[1] - 200.0];
    game.move_to(attacker, [base[0] + 700.0, base[1]])?;
    advance_until(
        game,
        7200,
        "paid Crusader staging outside bus range",
        |game| matches!(game.entities[attacker].ai.order, Order::Idle),
    )?;
    game.move_to(bus, point)?;
    advance_until(game, 7200, "loaded paid Battle Bus movement", |game| {
        let actor = &game.entities[bus];
        matches!(actor.ai.order, Order::Idle)
            && (actor.position[0] - point[0]).hypot(actor.position[1] - point[1]) < 25.0
    })?;
    require(
        riders
            .iter()
            .all(|&id| game.entities[id].contained_by() == Some(bus)),
        "Battle Bus movement lost its passengers",
    )?;
    Ok(Setup {
        bus,
        riders,
        attacker,
        target,
    })
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let health = game.entities[setup.target].health;
    game.attack(setup.bus, setup.target)?;
    let mut passenger_shot = false;
    for _ in 0..1200 {
        game.step();
        passenger_shot |= game
            .presentation
            .shots
            .iter()
            .any(|shot| setup.riders.contains(&shot.source));
        if passenger_shot && game.entities[setup.target].health < health {
            break;
        }
    }
    require(
        passenger_shot && game.entities[setup.target].health < health,
        "Battle Bus attack omitted original armed passenger firing",
    )?;
    game.stop(setup.bus)?;
    game.attack(setup.attacker, setup.bus)?;
    advance_until(
        game,
        7200,
        "ordinary Crusader damage triggering bus second life",
        |game| game.entities[setup.bus].battle_bus_second_life(),
    )?;
    game.stop(setup.attacker)?;
    require(
        game.entities[setup.bus].alive()
            && game.entities[setup.bus].max_health.to_bits() == 650.0_f32.to_bits(),
        "Battle Bus first lethal damage missed its fully healed original second body",
    )?;
    require(
        setup.riders.iter().all(|&id| {
            game.entities[id].alive() && game.entities[id].contained_by() == Some(setup.bus)
        }),
        "Battle Bus first death killed or ejected surviving passengers",
    )?;
    advance_until(
        game,
        7200,
        "Battle Bus original throw returning to held hulk",
        |game| game.entities[setup.bus].battle_bus_grounded(),
    )?;
    let position = game.entities[setup.bus].position;
    let _movement = game.move_to(setup.bus, [position[0] + 100.0, position[1]]);
    for _ in 0..120 {
        game.step();
    }
    require(
        game.entities[setup.bus].position.map(f32::to_bits) == position.map(f32::to_bits),
        "held Battle Bus hulk resumed locomotion",
    )?;
    verify_held_fire(game, &setup)?;
    game.attack(setup.attacker, setup.bus)?;
    advance_until(
        game,
        9000,
        "ordinary Crusader damage causing bus final death",
        |game| !game.entities[setup.bus].alive(),
    )?;
    require(
        setup.riders.iter().all(|&id| !game.entities[id].alive()),
        "Battle Bus final death skipped original lethal cargo damage",
    )?;
    verify_final_hulk(game)?;
    println!(
        "Paid Battle Bus: Palace/cost/time, real load/move/passenger fire, ordinary incoming-damage second body, original held hulk armed cargo and final death passed at {}",
        game.tick
    );
    Ok(())
}

fn verify_held_fire(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let position = game.entities[setup.bus].position;
    game.move_to(setup.attacker, [position[0] + 70.0, position[1]])?;
    advance_until(
        game,
        7200,
        "paid enemy entering held passenger firing range",
        |game| game.container_can_attack(setup.bus, setup.attacker),
    )?;
    game.stop(setup.attacker)?;
    require(
        game.container_can_attack(setup.bus, setup.attacker),
        "paid enemy did not remain within held passenger firing range",
    )?;
    let health = game.entities[setup.attacker].health;
    game.attack(setup.bus, setup.attacker)?;
    let mut fired = false;
    for _ in 0..300 {
        game.step();
        fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| setup.riders.contains(&shot.source));
        if fired && game.entities[setup.attacker].health < health {
            break;
        }
    }
    require_hulk_fire(
        game,
        setup,
        setup.attacker,
        fired && game.entities[setup.attacker].health < health,
    )?;
    let bus = &game.entities[setup.bus];
    let state = format!(
        "frame={} before={position:?} after={:?} health={} alive={} held={}",
        game.tick,
        bus.position,
        bus.health,
        bus.alive(),
        bus.battle_bus_grounded()
    );
    require(
        bus.alive(),
        &format!("held passenger firing ended with a destroyed Battle Bus: {state}"),
    )?;
    require(
        bus.battle_bus_grounded(),
        &format!("held passenger firing cleared the Battle Bus held state: {state}"),
    )?;
    require(
        bus.position.map(f32::to_bits) == position.map(f32::to_bits),
        &format!("Battle Bus position changed during held passenger firing: {state}"),
    )?;
    Ok(())
}

fn require_hulk_fire(
    game: &Skirmish,
    setup: &Setup,
    target: usize,
    fired: bool,
) -> Result<(), Box<dyn Error>> {
    if fired {
        return Ok(());
    }
    let bus = &game.entities[setup.bus];
    let victim = &game.entities[target];
    let riders = setup
        .riders
        .iter()
        .map(|&id| {
            let actor = &game.entities[id];
            format!(
                "rider {id} alive={} health={} position={:?} carrier={:?} can_fire={}; {}",
                actor.alive(),
                actor.health,
                actor.position,
                actor.contained_by(),
                game.passenger_can_fire(id),
                generals_sim::skirmish::diagnostics::weapon_details(game, id, target)
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Err(format!("held Battle Bus passenger firing or target damage missing: frame={} bus={} position={:?} health={} held={}; target={} position={:?} health={} alive={} masked={}; {riders}",
        game.tick, setup.bus, bus.position, bus.health, bus.battle_bus_grounded(),
        target, victim.position, victim.health, victim.alive(), victim.masked()).into())
}

fn verify_final_hulk(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    advance_until(
        game,
        60,
        "original final Battle Bus physical hulk",
        |game| {
            game.entities.iter().any(|actor| {
                game.catalog[actor.type_index].name == "DeadBattleBusHulk"
                    && actor.physical_height().is_some()
            })
        },
    )?;
    require(
        ["UVBattBus_D2", "UVBattBus_D3", "UVBattBus_D4"]
            .iter()
            .all(|name| {
                game.entities
                    .iter()
                    .any(|actor| actor.physical_model() == Some(*name))
            }),
        "Battle Bus final death omitted authored flying debris",
    )?;
    Ok(())
}
