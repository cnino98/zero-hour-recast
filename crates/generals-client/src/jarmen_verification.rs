//! Paid Jarmen production, original pilot weapon, neutrality and infantry takeover.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub sniper: usize,
    pub vehicle: usize,
    pub driver: usize,
    pub infantry: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let jarmen = template(game, "GLAInfantryJarmenKell")?;
    let cash = game.players[0].cash;
    require(
        game.train(barracks, jarmen).is_err() && game.players[0].cash == cash,
        "Jarmen bypassed the original Palace prerequisite",
    )?;
    construct(game, worker, Kind::Factory, 0)?;
    construct_type(game, worker, template(game, "GLAPalace")?, 0)?;
    let sniper = produce_type(game, barracks, jarmen, 0)?;
    let cash = game.players[0].cash;
    require(
        game.train(barracks, jarmen).is_err() && game.players[0].cash == cash,
        "Jarmen original one-per-player limit failed",
    )?;
    let driver = produce_type(game, barracks, template(game, "GLAInfantryRebel")?, 0)?;
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy, Kind::PowerPlant, 1)?;
    construct(game, enemy, Kind::SupplyCenter, 1)?;
    let enemy_barracks = construct(game, enemy, Kind::Barracks, 1)?;
    let infantry = produce_type(
        game,
        enemy_barracks,
        template(game, "AmericaInfantryRanger")?,
        1,
    )?;
    let command = friendly_entity(game, Kind::CommandCenter, 1)?;
    let vehicle = produce_type(game, command, template(game, "AmericaVehicleDozer")?, 1)?;
    approach(game, vehicle, game.entities[sniper].position, 140.0)?;
    advance_until(
        game,
        180,
        "Jarmen's original stationary cloak delay",
        |game| game.entities[sniper].stealthed(),
    )?;
    require(
        game.can_snipe_vehicle(sniper, vehicle) && !game.weapon_target_reachable(sniper, vehicle),
        "Jarmen ordinary rifle cursor incorrectly admitted the pilot weapon",
    )?;
    Ok(Setup {
        sniper,
        vehicle,
        driver,
        infantry,
    })
}

fn approach(
    game: &mut Skirmish,
    actor: usize,
    target: [f32; 2],
    range: f32,
) -> Result<(), Box<dyn Error>> {
    game.move_to(actor, target)?;
    advance_until(game, 18_000, "ordinary approach to Jarmen", |game| {
        let point = game.entities[actor].position;
        (point[0] - target[0]).hypot(point[1] - target[1]) < range
    })?;
    game.stop(actor)?;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let health = game.entities[setup.vehicle].health.to_bits();
    let losses = game.players[1].statistics.units_lost;
    let kills = game.players[0].statistics.units_destroyed;
    game.snipe_vehicle(setup.sniper, setup.vehicle, 1)?;
    advance_until(game, 1500, "real Jarmen pilot shot", |game| {
        game.entities[setup.vehicle].unmanned()
    })?;
    require(
        game.entities[setup.vehicle].owner == game.neutral_owner()
            && game.entities[setup.vehicle].health.to_bits() == health
            && game.players[1].statistics.units_lost == losses
            && game.players[0].statistics.units_destroyed == kills,
        "Pilot damage changed vehicle health or credited a vehicle kill",
    )?;
    require(
        !game.pilot_weapon_ready(setup.sniper, 1)
            && game.pilot_reload_progress(setup.sniper, 1).is_some(),
        "Original pilot clip did not enter its timed reload",
    )?;
    let driver_losses = game.players[0].statistics.units_lost;
    game.enter(setup.driver, setup.vehicle)?;
    advance_until(
        game,
        9000,
        "paid Rebel contacting the neutral unmanned vehicle",
        |game| game.entities[setup.vehicle].owner == 0 && !game.entities[setup.vehicle].unmanned(),
    )?;
    require(
        !game.entities[setup.driver].alive()
            && game.players[0].statistics.units_lost == driver_losses,
        "Taking control killed/scored the entering infantry",
    )?;
    game.stop(setup.sniper)?;
    let market = construct_type(
        game,
        friendly_entity(game, Kind::Builder, 0)?,
        template(game, "GLABlackMarket")?,
        0,
    )?;
    research(game, market, "Upgrade_GLAAPBullets")?;
    let lost = game.players[1].statistics.units_lost;
    game.attack(setup.sniper, setup.infantry)?;
    advance_until(
        game,
        18_000,
        "Jarmen primary rifle killing real paid enemy infantry",
        |game| !game.entities[setup.infantry].alive(),
    )?;
    require(
        game.players[1].statistics.units_lost == lost + 1
            && game.entities[setup.sniper].experience > 0,
        "Original primary rifle kill did not grant its source credit",
    )?;
    println!(
        "Installed Jarmen: paid prerequisites/one-unit limit, original stealth and pilot command, unchanged-health neutrality, timed reload, paid infantry takeover, AP Bullets and primary-rifle kill passed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_jarmen_snipes_recrews_and_uses_original_primary_rifle() {
        let root = generals_assets::game_root().unwrap();
        let mut game = crate::infiltrator_verification::load_game(&root).unwrap();
        super::verify(&mut game).unwrap();
    }
}
