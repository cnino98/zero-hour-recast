//! Paid original Bunker and authored civilian-building command paths.
use crate::skirmish_verification::{
    advance_until, construct, construct_type_at, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Order, ProjectileFxStage, Skirmish, WeaponFxEvent};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    verify_bunker(game)?;
    let (host, ranger, neutral) = prepare_neutral(game)?;
    game.exit_container(ranger)?;
    advance_until(game, 120, "neutral garrison individual exit", |game| {
        game.passengers(host).is_empty()
    })?;
    require(
        game.entities[host].owner == neutral,
        "empty map building did not restore original civilian owner",
    )?;
    // GarrisonContain::exitObjectViaDoor installs its own follow-path command
    // which ignores the warehouse until the Ranger clears its footprint.
    advance_until(
        game,
        3000,
        "Ranger following the original warehouse exit",
        |game| matches!(game.entities[ranger].ai.order, Order::Idle),
    )
    .map_err(|error| {
        format!(
            "{error}; {}",
            generals_sim::skirmish::diagnostics::movement_details(game, ranger)
        )
    })?;
    let barracks = friendly_entity(game, Kind::Barracks, 0)?;
    let [x, y] = game.entities[barracks].position;
    let home = [x + 100.0, y];
    game.move_to(ranger, home)?;
    advance_until(game, 18_000, "Ranger leaving neutral entry route", |game| {
        (game.entities[ranger].position[0] - home[0])
            .hypot(game.entities[ranger].position[1] - home[1])
            < 80.0
    })
    .map_err(|error| {
        format!(
            "{error}; {}",
            generals_sim::skirmish::diagnostics::movement_details(game, ranger)
        )
    })?;
    let enemy_worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy_worker, Kind::PowerPlant, 1)?;
    let barracks = construct(game, enemy_worker, Kind::Barracks, 1)?;
    let rider = produce_type(
        game,
        barracks,
        type_index(game, "ChinaInfantryRedguard")?,
        1,
    )?;
    board(game, rider, host)?;
    require(
        game.entities[host].owner == 1 && game.clearable_garrison(host),
        "enemy failed to take empty civilian garrison",
    )?;
    let barracks = friendly_entity(game, Kind::Barracks, 0)?;
    let research = game.catalog[game.entities[barracks].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_AmericaRangerFlashBangGrenade")
        .ok_or("Flashbang research absent")?;
    game.queue_upgrade(barracks, research)?;
    advance_until(game, 3000, "paid Flashbang research", |game| {
        game.weapon_available(ranger, 1)
    })?;
    game.switch_weapon(ranger, 1)?;
    let health = game.entities[host].health;
    game.attack(ranger, host)?;
    advance_until(
        game,
        18_000,
        "native Flashbang building clear impact",
        |game| {
            game.presentation.weapon_fx.iter().any(|event|matches!(event,WeaponFxEvent::Projectile(event) if event.object==Some(host) && matches!(event.stage,ProjectileFxStage::GarrisonKill)))
        },
    )?;
    require(
        !game.entities[rider].alive()
            && game.entities[host].health.to_bits() == health.to_bits()
            && game.entities[host].owner == neutral,
        "Flashbang clear did not kill the held rider while preserving building health/neutral ownership",
    )?;
    super::rubble::verify(game, host, ranger)?;
    println!(
        "Installed map garrisons: authored placement, infantry approach/boarding, civilian ownership restore, enemy takeover, paid Flashbang projectile clearing and target-bound original FX passed"
    );
    Ok(())
}

fn prepare_neutral(game: &mut Skirmish) -> Result<(usize, usize, usize), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let host =
        crate::neutral_tech_verification::civilian_structure(game, "WaterfrontWherehouse01")?;
    let original_owner = game.entities[host].owner;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let ranger = produce_type(
        game,
        barracks,
        type_index(game, "AmericaInfantryRanger")?,
        0,
    )?;
    board(game, ranger, host)?;
    require(
        game.entities[host].owner == 0 && game.entities[ranger].masked(),
        "civilian garrison did not adopt rider's controller and hide enclosed actor",
    )?;
    Ok((host, ranger, original_owner))
}

fn verify_bunker(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let index = type_index(game, "ChinaBunker")?;
    // Place the bunker on the front side of the barracks so its windows
    // face the incoming unit without firing through the occupied structure.
    let [x, y] = game.entities[barracks].position;
    let position = game
        .placement_near(index, [x, y + 150.0])
        .ok_or("Bunker build location absent")?;
    let host = construct_type_at(game, worker, index, 0, position)?;
    let rules = game.catalog[index]
        .contain
        .as_ref()
        .ok_or("Bunker container missing")?;
    require(
        rules.capacity == 5
            && rules
                .garrison
                .as_ref()
                .is_some_and(|rules| rules.immune_to_clear),
        "China Bunker native capacity/clear immunity changed",
    )?;
    for name in ["ChinaInfantryRedguard", "ChinaInfantryTankHunter"] {
        let rider = produce_type(game, barracks, type_index(game, name)?, 0)?;
        board(game, rider, host)?;
    }
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    let center = game.entities[host].position;
    game.move_to(enemy, [center[0] + 100.0, center[1] + 80.0])?;
    advance_until(game, 18_000, "enemy approaching Bunker", |game| {
        game.container_can_attack(host, enemy)
    })?;
    game.attack(host, enemy)
        .map_err(|error| bunker_attack_error(game, host, enemy, error))?;
    advance_until(
        game,
        1200,
        "Bunker occupant firing from authored windows",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| game.passengers(host).contains(&shot.source))
        },
    )
    .map_err(|error| bunker_attack_error(game, host, enemy, &error.to_string()))?;
    game.evacuate(host)?;
    advance_until(game, 120, "Bunker evacuation", |game| {
        game.passengers(host).is_empty()
    })?;
    require(
        game.entities[host].owner == 0 && !game.clearable_garrison(host),
        "Bunker evacuation changed original owner/immunity",
    )?;
    println!(
        "Installed China Bunker: paid construction, original five-place immune garrison, Red Guard/Tank Hunter approach/window firing and evacuation passed"
    );
    Ok(())
}

fn bunker_attack_error(game: &Skirmish, host: usize, enemy: usize, error: &str) -> String {
    let occupants: Vec<_> = game
        .passengers(host)
        .iter()
        .map(|&rider| {
            format!(
                "{}: {}",
                game.catalog[game.entities[rider].type_index].name,
                generals_sim::skirmish::diagnostics::weapon_details(game, rider, enemy)
            )
        })
        .collect();
    format!(
        "{error}; target={:?} health={} bunker={:?} occupants={occupants:?}",
        game.entities[enemy].position, game.entities[enemy].health, game.entities[host].position
    )
}

fn type_index(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing garrison diagnostic type {name}").into())
}

fn board(game: &mut Skirmish, rider: usize, host: usize) -> Result<(), Box<dyn Error>> {
    game.enter(rider, host)?;
    advance_until(game, 18_000, "authored garrison entry", |game| {
        game.entities[rider].contained_by() == Some(host)
    })
    .map_err(|error| super::boarding_error(game, rider, host, &error.to_string()).into())
}

pub fn prepare_neutral_display(game: &mut Skirmish) -> Result<(usize, Vec<usize>), Box<dyn Error>> {
    let (host, rider, _) = prepare_neutral(game)?;
    Ok((host, vec![rider]))
}
