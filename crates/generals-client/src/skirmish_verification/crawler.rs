//! Paid original Troop Crawler production and its eight created Red Guards.
use super::{advance_until, construct, friendly_entity, produce_type, require};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

pub struct Setup {
    pub carrier: usize,
    pub riders: Vec<usize>,
    pub target: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaVehicleTroopCrawler")
        .ok_or("original Troop Crawler missing")?;
    let carrier = produce_type(game, factory, index, 0)?;
    let rules = game.catalog[index]
        .contain
        .as_ref()
        .ok_or("crawler container missing")?;
    require(
        rules.capacity == 8
            && rules.exit_frames == 8
            && rules.exit_paths == 3
            && rules.regen_percent.to_bits() == 10.0_f32.to_bits()
            && rules.death.damage.to_bits() == 0.1_f32.to_bits()
            && rules.assault_heal_ratio == Some(0.5)
            && !rules.weapons.fire,
        "original Troop Crawler containment/assault values changed",
    )?;
    let riders = game.passengers(carrier).to_vec();
    require(
        riders.len() == 8
            && riders.iter().all(|&id| {
                let actor = &game.entities[id];
                game.catalog[actor.type_index].name == "ChinaInfantryRedguard"
                    && actor.owner == 0
                    && actor.masked()
            }),
        "crawler did not create exactly eight enclosed original Red Guards",
    )?;
    let point = game.entities[carrier].position;
    game.move_to(carrier, [point[0] - 250.0, point[1] - 100.0])?;
    advance_until(game, 3000, "crawler moving with initial infantry", |game| {
        matches!(game.entities[carrier].ai.order, Order::Idle)
    })?;
    let enemy_builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy_builder, Kind::PowerPlant, 1)?;
    let barracks = construct(game, enemy_builder, Kind::Barracks, 1)?;
    let ranger = game
        .catalog
        .iter()
        .position(|unit| unit.name == "AmericaInfantryRanger")
        .ok_or("original enemy Ranger missing")?;
    let target = produce_type(game, barracks, ranger, 1)?;
    let point = game.entities[carrier].position;
    game.move_to(target, [point[0] + 110.0, point[1]])?;
    advance_until(game, 12_000, "paid assault target approaching", |game| {
        matches!(game.entities[target].ai.order, Order::Idle)
    })?;
    Ok(Setup {
        carrier,
        riders,
        target,
    })
}

pub fn assault(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    game.attack(setup.carrier, setup.target)?;
    advance_until(
        game,
        6000,
        "crawler assault deploy and infantry combat",
        |game| !game.entities[setup.target].alive(),
    )?;
    require(
        setup
            .riders
            .iter()
            .any(|&id| game.entities[id].experience > 0),
        "assault target died without infantry kill experience",
    )?;
    require(
        game.entities[setup.carrier].experience == 0,
        "dummy crawler deploy weapon took damage credit",
    )?;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    assault(game, &setup)?;
    advance_until(
        game,
        6000,
        "assault troops returning after target death",
        |game| game.passengers(setup.carrier).len() == 8,
    )?;
    game.evacuate(setup.carrier)?;
    advance_until(game, 1500, "original crawler evacuation command", |game| {
        game.passengers(setup.carrier).is_empty()
    })?;
    require(
        setup
            .riders
            .iter()
            .all(|&id| game.entities[id].alive() && !game.entities[id].masked()),
        "crawler evacuation hid or lost a living Red Guard",
    )?;
    println!(
        "Installed Troop Crawler: paid factory production, eight silent initial Red Guards, enclosed transport, native DEPLOY assault, infantry kill credit, retrieval and timed evacuation passed"
    );
    Ok(())
}
