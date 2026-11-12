//! Installed anti-target flags through paid infantry and supply construction.
use super::{Faction, Kind, Skirmish, construct, friendly_entity, produce_type, require};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::Usa)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let ranger = produce_named(game, barracks, "AmericaInfantryRanger")?;
    let defender = produce_named(game, barracks, "AmericaInfantryMissileDefender")?;
    let enemy_builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy_builder, Kind::PowerPlant, 1)?;
    let center = construct(game, enemy_builder, Kind::SupplyCenter, 1)?;
    let chinook = super::airborne_supply_chinook(game, center)?;
    let unit = &game.catalog[game.entities[chinook].type_index];
    require(
        unit.preferred_height > unit.locomotor.airborne_targeting_height,
        "installed Chinook does not exceed its authored airborne targeting threshold",
    )?;
    require(
        !game.weapon_can_target(ranger, chinook) && game.attack(ranger, chinook).is_err(),
        "installed ground-only Ranger could attack an airborne Chinook",
    )?;
    require(
        game.weapon_can_target(defender, chinook),
        "installed Missile Defender lost AntiAirborneVehicle eligibility",
    )?;
    game.attack(defender, chinook)?;
    println!(
        "Installed anti-target masks: airborne Chinook rejected by Ranger and accepted by Missile Defender"
    );
    Ok(())
}

fn produce_named(
    game: &mut Skirmish,
    barracks: usize,
    name: &str,
) -> Result<usize, Box<dyn Error>> {
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or("installed targeting infantry is absent")?;
    produce_type(game, barracks, index, 0)
}
