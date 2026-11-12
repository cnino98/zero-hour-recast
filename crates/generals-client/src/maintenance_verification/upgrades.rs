//! Installed prerequisites, construction, research and secondary income via player commands.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    let plant = construct(game, worker, Kind::PowerPlant, 0)?;
    let second = construct(game, worker, Kind::PowerPlant, 0)?;
    let before = game.players[0].power;
    let unit = &game.catalog[game.entities[plant].type_index];
    let bonus = unit
        .maintenance
        .power_upgrade
        .as_ref()
        .ok_or("installed control rods module missing")?
        .bonus;
    super::research(game, plant, "Upgrade_AmericaAdvancedControlRods")?;
    require(
        game.players[0].power == before + bonus,
        "control rods failed to add original EnergyBonus",
    )?;
    require(
        !game.object_has_upgrade(second, "Upgrade_AmericaAdvancedControlRods"),
        "object research leaked to another power plant",
    )?;
    advance_until(game, 60, "source control rods extension", |game| {
        game.entities[plant]
            .upgrade_conditions(game.tick)
            .contains(&"POWER_PLANT_UPGRADED")
    })?;
    println!(
        "Installed research: object-scoped Control Rods add source power and finish original extension state"
    );

    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    super::research(game, factory, "Upgrade_ChinaChainGuns")?;
    let gattling = type_index(game, "ChinaTankGattling")?;
    let produced = produce_type(game, factory, gattling, 0)?;
    require(
        game.has_upgrade(0, "Upgrade_ChinaChainGuns") && game.entities[produced].complete(),
        "China Chain Guns normal research/production did not complete",
    )?;
    println!("Installed research: China Chain Guns purchases through the factory's original queue");

    prepare(game)?;
    crate::transport_verification::verify_palace(game)?;
    Ok(())
}

pub fn prepare(game: &mut Skirmish) -> Result<Vec<usize>, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    let palace_type = type_index(game, "GLAPalace")?;
    let market_type = type_index(game, "GLABlackMarket")?;
    require(
        !game.prerequisites_met(0, palace_type) && !game.prerequisites_met(0, market_type),
        "GLA advanced structures bypass their original prerequisite chain",
    )?;
    let stash = construct(game, worker, Kind::SupplyCenter, 0)?;
    let collector_type = game.entities[worker].type_index;
    let collector = produce_type(game, stash, collector_type, 0)?;
    let supply = game
        .supplies
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let distance = |point: [f32; 2]| {
                (point[0] - game.entities[stash].position[0])
                    .hypot(point[1] - game.entities[stash].position[1])
            };
            distance(game.entities[a.object].position)
                .total_cmp(&distance(game.entities[b.object].position))
        })
        .map(|(id, _)| id)
        .ok_or("no original supplies")?;
    game.harvest(collector, supply)?;
    construct(game, worker, Kind::Barracks, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let palace = build_type(game, worker, palace_type)?;
    let market = build_type(game, worker, market_type)?;
    let deposit = game.catalog[market_type]
        .maintenance
        .deposits
        .first()
        .ok_or("Black Market lacks AutoDepositUpdate")?;
    require(
        deposit.amount == 20 && deposit.interval == 60 && deposit.first_capture == 0,
        "Black Market installed deposit values differ from source",
    )?;
    game.stop(collector)?;
    let money = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    advance_until(game, 60, "Black Market income", |game| {
        game.players[0].cash > money
    })?;
    require(
        game.players[0].cash == money + 20
            && game.players[0].statistics.money_earned == earned + 20,
        "Black Market income or earned statistics differ from native deposit",
    )?;
    game.harvest(collector, supply)?;
    research_gla(game, palace, market)?;
    require(
        game.entities[palace]
            .upgrade_conditions(game.tick)
            .contains(&"USER_1")
            && game.entities[market]
                .upgrade_conditions(game.tick)
                .contains(&"GARRISONED"),
        "Fortified Structures did not select original model conditions",
    )?;
    let quad = type_index(game, "GLAVehicleQuadCannon")?;
    advance_until(
        game,
        18_000,
        "original income funding Quad Cannon",
        |game| game.players[0].cash >= game.catalog[quad].cost,
    )?;
    produce_type(game, factory, quad, 0)?;
    require(
        game.catalog[palace_type]
            .maintenance
            .expose_hole
            .first()
            .is_some_and(|hole| hole.template == "GLAHolePalace")
            && game.catalog[market_type]
                .maintenance
                .expose_hole
                .first()
                .is_some_and(|hole| hole.template == "GLAHoleBlackMarket"),
        "advanced GLA structures lack their actual reconstruction holes",
    )?;
    println!(
        "Installed research: Palace/Black Market prerequisites, actual construction, $20/60-frame income, Fortified/AP Bullets/Junk Repair/Worker Shoes and future production passed"
    );
    Ok(vec![market])
}

fn research_gla(game: &mut Skirmish, palace: usize, market: usize) -> Result<(), Box<dyn Error>> {
    for (producer, upgrade) in [
        (palace, "Upgrade_GLAFortifiedStructure"),
        (market, "Upgrade_GLAAPBullets"),
        (market, "Upgrade_GLAJunkRepair"),
        (market, "Upgrade_GLAWorkerShoes"),
    ] {
        let research = game.catalog[game.entities[producer].type_index]
            .maintenance
            .research
            .iter()
            .find(|entry| entry.name == upgrade)
            .ok_or("expected installed upgrade command missing")?;
        let cost = research.cost;
        advance_until(
            game,
            36_000,
            "original collection and market funding research",
            |game| game.players[0].cash >= cost,
        )?;
        super::research(game, producer, upgrade)?;
        require(
            game.has_upgrade(0, upgrade),
            "installed player research did not complete",
        )?;
    }
    Ok(())
}

fn type_index(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing represented original type {name}").into())
}

fn build_type(
    game: &mut Skirmish,
    worker: usize,
    type_index: usize,
) -> Result<usize, Box<dyn Error>> {
    let cost = game.catalog[type_index].cost;
    advance_until(
        game,
        36_000,
        "original income funding advanced structure",
        |game| game.players[0].cash >= cost,
    )?;
    let center = friendly_entity(game, Kind::CommandCenter, 0)?;
    let base = game.entities[center].position;
    let position = game
        .placement_near(type_index, [base[0] + 180.0, base[1] + 150.0])
        .ok_or("no legal advanced structure site")?;
    let id = game.build(worker, type_index, position)?;
    advance_until(game, 18_000, "advanced structure construction", |game| {
        game.entities[id].complete()
    })?;
    Ok(id)
}
