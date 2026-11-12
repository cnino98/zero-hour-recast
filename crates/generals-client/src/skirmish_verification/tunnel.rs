//! Paid original Tunnel Network construction, free defenders and remote exits.
use super::{
    advance_until, construct, construct_type, construct_type_at, construction_site,
    friendly_entity, produce, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub entrances: [usize; 2],
    pub tank: usize,
    pub infantry: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    construct(game, worker, Kind::Barracks, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == "GLATunnelNetwork")
        .ok_or("original Tunnel Network absent")?;
    let first = construct_type(game, worker, index, 0)?;
    let point = game.entities[first].position;
    let second_position =
        construction_site(game, worker, index, [point[0] - 400.0, point[1] + 40.0])
            .ok_or("second Tunnel Network site unavailable")?;
    let second = construct_type_at(game, worker, index, 0, second_position)?;
    let defender = game
        .catalog
        .iter()
        .position(|unit| unit.name == "GLAInfantryTunnelDefender")
        .ok_or("original Tunnel Defender absent")?;
    advance_until(game, 30, "two original free defenders per tunnel", |game| {
        game.entities
            .iter()
            .filter(|actor| actor.owner == 0 && actor.alive() && actor.type_index == defender)
            .count()
            == 4
    })?;
    let infantry = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && actor.type_index == defender)
        .ok_or("spawned Tunnel Defender absent")?;
    let tank = produce(game, factory, Kind::Tank, 0)?;
    for id in [infantry, tank] {
        game.enter(id, first)?;
    }
    advance_until(game, 6000, "mixed infantry/vehicle tunnel entry", |game| {
        [infantry, tank]
            .into_iter()
            .all(|id| game.entities[id].contained_by() == Some(first))
    })?;
    require(
        game.passengers(first) == game.passengers(second) && game.passengers(second).len() == 2,
        "tunnel entrances do not share the original passenger list",
    )?;
    let rules = game.container_rules(first).ok_or("tunnel rules absent")?;
    require(
        rules.capacity == 10
            && rules
                .tunnel
                .as_ref()
                .is_some_and(|rules| rules.full_heal_frames.to_bits() == 150.0_f32.to_bits()),
        "installed tunnel capacity/healing differs from original",
    )?;
    println!(
        "Installed Tunnel Network: two paid entrances, four free RPG Troopers, shared mixed passenger list and original heal/capacity data passed at frame {}",
        game.tick
    );
    Ok(Setup {
        entrances: [first, second],
        tank,
        infantry,
    })
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let destination = setup.entrances[1];
    let point = game.entities[destination].position;
    game.set_rally_point(
        destination,
        [
            point[0],
            point[1] - 100.0,
            game.ground_height([point[0], point[1] - 100.0]),
        ],
    )?;
    game.exit_container_at(setup.tank, destination)?;
    advance_until(game, 30, "remote tank exit", |game| {
        game.entities[setup.tank].contained_by().is_none()
    })?;
    let tank = game.entities[setup.tank].position;
    require(
        (tank[0] - point[0]).hypot(tank[1] - point[1]) < 100.0,
        "tank exited at the entry tunnel instead of selected remote tunnel",
    )?;
    require(
        game.passengers(setup.entrances[0]) == [setup.infantry],
        "remote exit left stale shared membership",
    )?;
    game.evacuate(destination)?;
    advance_until(game, 30, "remote tunnel evacuation", |game| {
        game.passengers(destination).is_empty()
    })?;
    println!(
        "Installed Tunnel Network: remote individual exit, original rally and shared evacuation passed"
    );
    Ok(())
}
