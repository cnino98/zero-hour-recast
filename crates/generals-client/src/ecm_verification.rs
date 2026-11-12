//! Paid ECM vehicle disable and missile deflection through ordinary commands.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub tank: usize,
    pub vehicle: usize,
    pub launcher: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let kind = template(game, "ChinaTankECM")?;
    crate::microwave_verification::original_emitter_index(
        &game.catalog[kind],
        "ECMTankMissileJammer",
    )?;
    let cash = game.players[0].cash;
    require(
        game.train(factory, kind).is_err() && game.players[0].cash == cash,
        "ECM bypassed paid Propaganda Center prerequisite",
    )?;
    let propaganda = template(game, "ChinaPropagandaCenter")?;
    construct_type(game, builder, propaganda, 0)?;
    let tank = produce_type(game, factory, kind, 0)?;
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy, Kind::PowerPlant, 1)?;
    let barracks = construct(game, enemy, Kind::Barracks, 1)?;
    let defender = template(game, "AmericaInfantryMissileDefender")?;
    let launcher = produce_type(game, barracks, defender, 1)?;
    // The original unarmed Dozer is a real SUBDUAL_VEHICLE target.
    // Keep the paid launcher distant until the independent jammer stage.
    let center = game.entities[factory].position;
    stage(game, tank, [center[0] + 250.0, center[1] + 180.0])?;
    let center = game.entities[tank].position;
    stage(game, enemy, [center[0] + 120.0, center[1]])?;
    require(
        game.entities[enemy].alive() && !game.entities[enemy].subdued(),
        "ECM staging changed original vehicle target",
    )?;
    Ok(Setup {
        tank,
        vehicle: enemy,
        launcher,
    })
}

fn stage(game: &mut Skirmish, actor: usize, point: [f32; 2]) -> Result<(), Box<dyn Error>> {
    game.move_to(actor, point)?;
    advance_until(game, 18_000, "ordinary paid ECM staging", |game| {
        let position = game.entities[actor].position;
        (position[0] - point[0]).hypot(position[1] - point[1]) < 25.0
    })?;
    game.stop(actor)?;
    Ok(())
}

pub fn start_jammer(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    game.stop(setup.tank)?;
    game.attack(setup.launcher, setup.tank)?;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let health = game.entities[setup.vehicle].health;
    game.attack(setup.tank, setup.vehicle)?;
    advance_until(game, 9000, "paid original ECM vehicle disable", |game| {
        game.entities[setup.vehicle].subdued()
    })?;
    require(
        game.entities[setup.vehicle].health.to_bits() == health.to_bits(),
        "ECM subdual changed original vehicle health",
    )?;
    game.stop(setup.tank)?;
    advance_until(game, 9000, "original vehicle subdual recovery", |game| {
        !game.entities[setup.vehicle].subdued()
    })?;
    start_jammer(game, &setup)?;
    advance_until(
        game,
        18_000,
        "paid original ECM missile deflection",
        |game| {
            game.projectiles.iter().any(|projectile| {
                projectile.source == setup.launcher && projectile.jammed(&game.entities)
            })
        },
    )?;
    let missile = game
        .projectiles
        .iter()
        .find(|projectile| projectile.source == setup.launcher && projectile.jammed(&game.entities))
        .ok_or("jammed missile disappeared")?;
    require(
        missile.owner == 1 && missile.object.is_some_and(|id| game.entities[id].alive()),
        "ECM deflection changed launch owner or destroyed missile",
    )?;
    require(
        game.entities[setup.tank].alive(),
        "paid ECM died before missile deflection",
    )?;
    println!(
        "Paid original ECM: prerequisite, vehicle disable/recovery without health damage, and live enemy missile deflection passed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_original_ecm_disables_vehicle_and_deflects_live_enemy_missile() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        super::verify(&mut battle.simulation).unwrap();
    }
}
