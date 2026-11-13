//! Paid original base-USA Supply Lines and Chemical Suits consumers.
use crate::skirmish_verification::{
    advance_until, construct, construct_type, produce_type, require,
};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub center: usize,
    pub infantry: Vec<usize>,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let setup = crate::strategy_verification::prepare(game)?;
    let builder = crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let ranger = crate::strategy_verification::template(game, "AmericaInfantryRanger")?;
    let defender = crate::strategy_verification::template(game, "AmericaInfantryMissileDefender")?;
    let burton = crate::strategy_verification::template(game, "AmericaInfantryColonelBurton")?;
    let existing = produce_type(game, barracks, ranger, 0)?;
    let colonel = produce_type(game, barracks, burton, 0)?;
    require(
        !game.entities[existing].chemical_suit() && !game.entities[colonel].chemical_suit(),
        "Ranger received unpaid Chemical Suits",
    )?;
    crate::strategy_verification::research(game, setup.center, "Upgrade_AmericaSupplyLines")?;
    crate::strategy_verification::research(game, setup.center, "Upgrade_AmericaChemicalSuits")?;
    let future = produce_type(game, barracks, ranger, 0)?;
    let missile = produce_type(game, barracks, defender, 0)?;
    let infantry = vec![existing, colonel, future, missile];
    for &id in &infantry {
        require(
            game.entities[id].chemical_suit(),
            "paid existing/future infantry lost Chemical Suits",
        )?;
    }
    for name in [
        "AmericaInfantryRanger",
        "AmericaInfantryMissileDefender",
        "AmericaInfantryPathfinder",
        "AmericaInfantryColonelBurton",
    ] {
        let index = crate::strategy_verification::template(game, name)?;
        let module = game.catalog[index]
            .maintenance
            .armor_upgrade
            .as_ref()
            .ok_or("original infantry chemical armor missing")?;
        require(
            module.trigger == "Upgrade_AmericaChemicalSuits",
            "chemical armor trigger differs from original",
        )?;
    }
    verify_delivery(game)?;
    Ok(Setup {
        center: setup.center,
        infantry,
    })
}

fn verify_delivery(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let collector = game
        .entities
        .iter()
        .position(|actor| {
            actor.owner == 0
                && actor.complete()
                && game.catalog[actor.type_index].kind == Kind::Harvester
        })
        .ok_or("paid supply center did not supply original Chinook")?;
    let center = crate::skirmish_verification::friendly_entity(game, Kind::SupplyCenter, 0)?;
    let capacity = game.catalog[game.entities[collector].type_index]
        .supply
        .capacity;
    require(capacity == 8, "original Chinook capacity changed")?;
    advance_until(
        game,
        18_000,
        "original Chinook collecting full cargo",
        |game| game.entities[collector].cargo == capacity,
    )?;
    game.stop(collector)?;
    let cash = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    game.deliver_supplies(collector, center)?;
    advance_until(game, 18_000, "paid Supply Lines delivery", |game| {
        game.entities[collector].cargo == 0
    })?;
    require(
        game.players[0].cash == cash + 660
            && game.players[0].statistics.money_earned == earned + 660,
        "full Chinook Supply Lines delivery differs from original $660",
    )?;
    game.stop(collector)?;
    println!(
        "Installed USA research: paid original queues, existing/future Chemical Suits and exact $660 Chinook cash/statistics passed"
    );
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    require(
        game.entities[setup.center].complete() && setup.infantry.len() == 4,
        "USA research setup incomplete",
    )?;
    verify_pathfinders(game)
}

fn verify_pathfinders(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (existing, _) = crate::progression_verification::pathfinder::prepare_capture(game)?;
    require(
        !game.entities[existing].chemical_suit(),
        "Pathfinder received unpaid Chemical Suits",
    )?;
    let builder = crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0)?;
    let center_type = crate::strategy_verification::template(game, "AmericaStrategyCenter")?;
    let cost = game.catalog[center_type].cost;
    advance_until(
        game,
        18_000,
        "supplies funding Strategy Center for promoted Pathfinder",
        |game| game.players[0].cash >= cost,
    )?;
    let center = construct_type(game, builder, center_type, 0)?;
    crate::strategy_verification::research(game, center, "Upgrade_AmericaChemicalSuits")?;
    let barracks = crate::skirmish_verification::friendly_entity(game, Kind::Barracks, 0)?;
    let pathfinder = crate::strategy_verification::template(game, "AmericaInfantryPathfinder")?;
    let cost = game.catalog[pathfinder].cost;
    advance_until(
        game,
        18_000,
        "supplies funding future promoted Pathfinder",
        |game| game.players[0].cash >= cost,
    )?;
    let future = produce_type(game, barracks, pathfinder, 0)?;
    require(
        game.entities[existing].chemical_suit() && game.entities[future].chemical_suit(),
        "paid existing/future promoted Pathfinder lost Chemical Suits",
    )?;
    println!(
        "Installed Chemical Suits: combat-earned Pathfinder science and paid existing/future consumers passed"
    );
    Ok(())
}
