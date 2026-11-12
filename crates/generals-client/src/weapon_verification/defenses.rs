//! Paid installed defense construction, assistance, brownout and Chain Guns.
use crate::skirmish_verification::{
    advance_until, construct, construct_type_at, friendly_entity, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (batteries, target, plant) = prepare(game)?;
    let [requester, assistant] = batteries;
    let mut shots = game
        .presentation
        .shots
        .iter()
        .filter(|shot| shot.source == assistant && shot.weapon_slot == 1)
        .count();
    for _ in 0..90 {
        game.stop(requester)?;
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == assistant && shot.weapon_slot == 1)
            .count();
        if shots >= 4 {
            break;
        }
    }
    if shots != 4 {
        return Err(format!("Patriot fired {shots} of four assisted missiles; requester at {:?}, assistant at {:?}, target health={} position={:?}; {}",
            game.entities[requester].position, game.entities[assistant].position, game.entities[target].health,
            game.entities[target].position, generals_sim::skirmish::diagnostics::weapon_details(game, assistant, target)).into());
    }
    let unit = &game.catalog[game.entities[requester].type_index];
    require(
        unit.weapon_set_rules.share_reload
            && unit.weapon_in_slot(2).is_some_and(|weapon| {
                weapon.name == "PatriotMissileWeaponAir" && weapon.anti_mask & 1 != 0
            }),
        "Patriot shared reload or tertiary AA data missing",
    )?;
    game.sell(0, plant)?;
    advance_until(game, 300, "defense plant sale", |game| {
        !game.entities[plant].alive()
    })?;
    for _ in 0..60 {
        game.step();
        require(
            game.presentation
                .shots
                .iter()
                .all(|shot| !batteries.contains(&shot.source)),
            "unpowered installed defense still fired",
        )?;
    }
    require(
        !game.has_operational_power(requester),
        "defense did not brown out",
    )?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    require(
        game.has_operational_power(requester),
        "defense failed power recovery",
    )?;
    verify_gattling(game)?;
    verify_air_target(game, Faction::Usa, 2)?;
    verify_air_target(game, Faction::China, 1)?;
    println!(
        "Installed defenses: paid Patriot/Gattling construction, shared reload, four-shot assistance, tertiary AA data, power loss/recovery and Chain Guns fire passed"
    );
    Ok(())
}

pub fn prepare(game: &mut Skirmish) -> Result<([usize; 2], usize, usize), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    let plant = construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    let first = construct(game, worker, Kind::Defense, 0)?;
    // Keep their firing lanes side by side. A battery directly behind the
    // requester receives its feedback stream but cannot shoot through it.
    let [x, y] = game.entities[first].position;
    let kind = game.entities[first].type_index;
    let flank = game
        .placement_near(kind, [x - 120.0, y + 30.0])
        .ok_or("no position for the flanking Patriot")?;
    let second = construct_type_at(game, worker, kind, 0, flank)?;
    let target = friendly_entity(game, Kind::Builder, 1)?;
    game.move_to(target, game.entities[first].position)?;
    advance_until(game, 18_000, "native Patriot assistance request", |game| {
        !game.presentation.assistance_events.is_empty()
    })?;
    let event = &game.presentation.assistance_events[0];
    require(
        [first, second].contains(&event.requester) && [first, second].contains(&event.assistant),
        "Patriot assistance did not connect the constructed pair",
    )?;
    let pair = [event.requester, event.assistant];
    require(
        game.projectiles.iter().any(|projectile| {
            pair.contains(&projectile.source)
                && projectile.render_transform(&game.entities).is_some()
        }),
        "Patriot fired without original missile geometry",
    )?;
    Ok((pair, target, plant))
}

fn verify_gattling(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    let cannon = construct(game, worker, Kind::Defense, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let kind = game.entities[factory].type_index;
    let upgrade = game.catalog[kind]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_ChinaChainGuns")
        .ok_or("Chain Guns research missing")?;
    game.queue_upgrade(factory, upgrade)?;
    advance_until(game, 9_000, "Chain Guns for existing defense", |game| {
        game.has_upgrade(0, "Upgrade_ChinaChainGuns")
    })?;
    let target = friendly_entity(game, Kind::Builder, 1)?;
    let health = game.entities[target].health;
    game.move_to(target, game.entities[cannon].position)?;
    advance_until(game, 18_000, "Gattling Cannon's original firing", |game| {
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == cannon)
    })?;
    let weapon = &game.catalog[game.entities[cannon].type_index].weapon;
    let armor = game.catalog[game.entities[target].type_index].armor[weapon.damage_type as usize];
    let expected_damage = weapon.damage * 1.25 * armor;
    require(
        (game.entities[target].health - (health - expected_damage)).abs() < 0.01,
        "Gattling defense did not apply researched Chain Guns damage",
    )?;
    advance_until(game, 60, "Gattling Cannon native barrel spin", |game| {
        game.entities[cannon].continuous_fire_condition() == Some("CONTINUOUS_FIRE_MEAN")
            || game.entities[cannon].continuous_fire_condition() == Some("CONTINUOUS_FIRE_FAST")
    })?;
    Ok(())
}

fn verify_air_target(
    game: &mut Skirmish,
    faction: Faction,
    slot: usize,
) -> Result<(), Box<dyn Error>> {
    game.reset(faction, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    let defense = construct(game, worker, Kind::Defense, 0)?;
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy, Kind::PowerPlant, 1)?;
    let center = construct(game, enemy, Kind::SupplyCenter, 1)?;
    let chinook = super::airborne_supply_chinook(game, center)?;
    game.move_to(chinook, game.entities[defense].position)?;
    advance_until(game, 18_000, "defense's original anti-air slot", |game| {
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == defense && shot.target == Some(chinook))
    })?;
    require(
        game.presentation
            .shots
            .iter()
            .filter(|shot| shot.source == defense)
            .all(|shot| shot.weapon_slot == slot),
        "static defense used a ground weapon against airborne Chinook",
    )?;
    Ok(())
}
