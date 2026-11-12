//! Installed Palace using the normally purchased GLA upgrade scenario.
use crate::skirmish_verification::{advance_until, friendly_entity, produce_type, require};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub fn verify_palace(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (palace, riders) = prepare_passengers(game)?;
    engage(game, palace, &riders)?;
    game.exit_container(riders[0])?;
    advance_until(game, 120, "individual Palace exit", |game| {
        game.entities[riders[0]].contained_by().is_none()
    })?;
    game.evacuate(palace)?;
    advance_until(game, 120, "Palace evacuation", |game| {
        game.passengers(palace).is_empty()
    })?;
    require(
        game.entities[palace].owner == 0,
        "evacuation changed the Palace original owner",
    )?;
    require(
        riders
            .iter()
            .all(|&id| !game.entities[id].garrisoned() && !game.entities[id].masked()),
        "evacuation retained held weapon/visibility state",
    )?;
    println!(
        "Installed Palace: paid Rebel/RPG production, original approach/admission,40-point firing, immune clear-building rules, individual exit and evacuation passed"
    );
    Ok(())
}

pub fn prepare_palace(game: &mut Skirmish) -> Result<(usize, Vec<usize>), Box<dyn Error>> {
    crate::maintenance_verification::prepare(game)?;
    let (palace, riders) = prepare_passengers(game)?;
    engage(game, palace, &riders)?;
    Ok((palace, riders))
}

pub fn engage(game: &mut Skirmish, palace: usize, riders: &[usize]) -> Result<(), Box<dyn Error>> {
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    let center = game.entities[palace].position;
    game.move_to(enemy, [center[0] + 120.0, center[1] + 80.0])?;
    advance_until(
        game,
        18_000,
        "enemy entering original garrison weapon range",
        |game| game.container_can_attack(palace, enemy),
    )?;
    game.attack(palace, enemy)?;
    advance_until(
        game,
        1200,
        "garrison firing from original windows",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| riders.contains(&shot.source))
        },
    )?;
    require(
        game.garrison_points(palace)
            .is_some_and(|points| points.iter().flatten().any(|id| riders.contains(id))),
        "garrison shot did not occupy a source fire point",
    )?;
    Ok(())
}

fn prepare_passengers(game: &mut Skirmish) -> Result<(usize, Vec<usize>), Box<dyn Error>> {
    let palace = game
        .entities
        .iter()
        .position(|actor| {
            actor.owner == 0
                && actor.complete()
                && game.catalog[actor.type_index].name == "GLAPalace"
        })
        .ok_or("paid Palace missing for garrison verification")?;
    let unit = &game.catalog[game.entities[palace].type_index];
    let rules = unit
        .contain
        .as_ref()
        .ok_or("original Palace garrison missing")?;
    require(
        rules.capacity == 5
            && rules
                .garrison
                .as_ref()
                .is_some_and(|garrison| garrison.immune_to_clear),
        "original Palace capacity/clear immunity changed",
    )?;
    require(
        rules.art.iter().all(|art| art.fire_points.len() == 40),
        "Palace lacks the native40-point fire arrays",
    )?;
    let barracks = friendly_entity(game, Kind::Barracks, 0)?;
    let mut riders = Vec::new();
    for name in ["GLAInfantryRebel", "GLAInfantryTunnelDefender"] {
        let unit_type = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .ok_or("installed GLA garrison infantry missing")?;
        advance_until(game, 18_000, "income funding garrison infantry", |game| {
            game.players[0].cash >= game.catalog[unit_type].cost
        })?;
        let rider = produce_type(game, barracks, unit_type, 0)?;
        game.enter(rider, palace)?;
        advance_until(game, 18_000, "original Palace approach and entry", |game| {
            game.entities[rider].contained_by() == Some(palace)
        })
        .map_err(|error| super::boarding_error(game, rider, palace, &error.to_string()))?;
        require(
            game.entities[rider].garrisoned() && game.entities[rider].masked(),
            "Palace passenger remains outside enclosing garrison",
        )?;
        riders.push(rider);
    }
    require(
        game.passengers(palace) == riders,
        "garrison list differs from native entry order",
    )?;
    Ok((palace, riders))
}
