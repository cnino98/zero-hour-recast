//! Paid installed Chinook transport commands; no actor placement or resource grants.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

pub fn prepare(game: &mut Skirmish) -> Result<(usize, Vec<usize>), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    let supply = construct(game, builder, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let aircraft_type = unit(game, "AmericaVehicleChinook")?;
    let host = produce_type(game, supply, aircraft_type, 0)?;
    let rules = game.catalog[aircraft_type]
        .contain
        .as_ref()
        .ok_or("Chinook container missing")?;
    require(
        rules.capacity == 8
            && rules.exit_frames == 3
            && rules.exit_paths == 1
            && !rules.weapons.fire,
        "Chinook source transport rules differ",
    )?;
    require(
        rules.art.iter().all(|art| {
            art.exits.len() == 1
                && art.exits[0]
                    .0
                    .iter()
                    .chain(&art.exits[0].1)
                    .all(|coordinate| *coordinate == 0.0)
        }),
        "Chinook literal exit bones must retain native object-center fallback",
    )?;
    let source = game
        .supplies
        .iter()
        .position(|stock| stock.boxes > 0)
        .ok_or("supply warehouse missing")?;
    game.harvest(host, source)?;
    advance_until(
        game,
        18_000,
        "Chinook collecting before transport duty",
        |game| game.entities[host].cargo > 0,
    )?;
    game.stop(host)?;
    let home = game.player_start(0).ok_or("human start missing")?.position;
    let landing = [home[0] + 180.0, home[1] + 120.0];
    game.move_to(host, landing)?;
    advance_until(game, 6000, "transport reaching pickup location", |game| {
        matches!(game.entities[host].ai.order, Order::Idle)
    })?;
    let vehicle = produce_type(game, factory, unit(game, "AmericaVehicleHumvee")?, 0)?;
    let slots = game.catalog[game.entities[vehicle].type_index].transport_slots;
    require(
        slots > 1 && slots < 8,
        "installed Humvee transport slot count missing",
    )?;
    let mut riders = vec![vehicle];
    for _ in slots..8 {
        riders.push(produce_type(
            game,
            barracks,
            unit(game, "AmericaInfantryRanger")?,
            0,
        )?);
    }
    let cargo = game.entities[host].cargo;
    require(cargo > 0, "pickup preparation lost supplies before landing")?;
    for &id in &riders {
        game.enter(id, host)?;
    }
    advance_until(
        game,
        18_000,
        "original Chinook landing and mixed passenger boarding",
        |game| game.passengers(host).len() == riders.len(),
    )
    .map_err(|error| {
        format!(
            "{error}; host={:?} height={:?} passengers={:?}",
            game.entities[host].position,
            game.entities[host].flight_height(),
            game.passengers(host)
        )
    })?;
    require(
        game.entities[host].cargo == 0,
        "transport landing did not discard supplies",
    )?;
    require(
        riders.iter().all(|&id| game.entities[id].masked()),
        "air transport exposed held passengers",
    )?;
    advance_until(game, 1800, "loaded Chinook taking off", |game| {
        game.height_above_ground(host) > 97.0
    })?;
    Ok((host, riders))
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (host, riders) = prepare(game)?;
    let barracks = friendly_entity(game, Kind::Barracks, 0)?;
    let extra = produce_type(game, barracks, unit(game, "AmericaInfantryRanger")?, 0)?;
    require(
        !game.can_enter(extra, host),
        "Chinook admitted a ninth occupied slot",
    )?;
    // Direct aiEvacuate commands the occupants to exit individually through
    // TransportContain's authored ExitDelay.
    game.evacuate(host)?;
    let mut previous = None;
    let mut count = riders.len();
    while count > 0 {
        advance_until(game, 3000, "Chinook timed evacuation", |game| {
            game.passengers(host).len() < count
        })?;
        let remaining = game.passengers(host).len();
        require(
            remaining + 1 == count && game.height_above_ground(host) <= 3.0,
            "ordinary evacuation dumped passengers or unloaded in flight",
        )?;
        if let Some(frame) = previous {
            require(game.tick >= frame + 3, "Chinook ExitDelay was skipped")?;
        }
        previous = Some(game.tick);
        count = remaining;
    }
    require(
        riders
            .iter()
            .all(|&id| game.entities[id].alive() && !game.entities[id].masked()),
        "Chinook evacuation lost a passenger",
    )?;
    advance_until(game, 1800, "empty Chinook taking off", |game| {
        game.height_above_ground(host) > 97.0
    })?;
    verify_group_evacuation(game, host, &riders)?;
    let source = game
        .supplies
        .iter()
        .position(|stock| stock.boxes > 0)
        .ok_or("supply warehouse depleted")?;
    game.harvest(host, source)?;
    advance_until(game, 18_000, "transport returning to pickup", |game| {
        game.entities[host].cargo > 0
    })?;
    let before = game.players[0].statistics.money_earned;
    advance_until(game, 18_000, "Chinook returning to supply duty", |game| {
        game.players[0].statistics.money_earned > before && game.entities[host].cargo == 0
    })?;
    println!(
        "Installed Chinook: paid aircraft/Humvee/Rangers,8-slot mixed capacity, original landing and cargo loss, physical boarding, takeoff, timed individual exits, airborne group landing and evacuation, and supply return passed"
    );
    Ok(())
}

fn verify_group_evacuation(
    game: &mut Skirmish,
    host: usize,
    riders: &[usize],
) -> Result<(), Box<dyn Error>> {
    for &rider in riders {
        game.enter(rider, host)?;
    }
    advance_until(
        game,
        18_000,
        "mixed passengers reboarding the Chinook",
        |game| game.passengers(host).len() == riders.len() && game.height_above_ground(host) > 97.0,
    )?;
    // AIGroup::groupEvacuate on airborne aircraft dispatches MoveToAndEvacuate.
    // The Chinook's EVAC_AND_TAKEOFF state removes all occupants together.
    game.group_evacuate(host)?;
    advance_until(game, 3000, "airborne group landing to evacuate", |game| {
        game.passengers(host).len() < riders.len()
    })?;
    require(
        game.passengers(host).is_empty()
            && game.height_above_ground(host) <= 3.0
            && riders
                .iter()
                .all(|&id| game.entities[id].alive() && !game.entities[id].masked()),
        "Chinook group evacuation did not unload all passengers together after landing",
    )
}

fn unit(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing {name}").into())
}
