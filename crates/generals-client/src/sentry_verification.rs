//! Paid original Sentry Drone production, research, concealment and deployment.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub drone: usize,
    pub factory: usize,
    pub target: usize,
}

impl Setup {
    pub fn approach_drone(&self, game: &mut Skirmish) -> Result<[f32; 2], &'static str> {
        let point = game.entities[self.drone].position;
        // Use the open ground the drone just crossed, clear of the ridge
        // beyond its stopping point. Keep the dozer moving until it arrives.
        let approach = [point[0] - 85.0, point[1]];
        game.stop(self.drone)?;
        game.move_to(self.target, approach)?;
        Ok(approach)
    }
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let drone_type = template(game, "AmericaVehicleSentryDrone")?;
    let drone = produce_type(game, factory, drone_type, 0)?;
    let center = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    let destination = [center[0] + 400.0, center[1] + 350.0];
    game.move_to(drone, destination)?;
    advance_until(game, 18_000, "unarmed Sentry Drone movement", |game| {
        let position = game.entities[drone].position;
        (position[0] - destination[0]).hypot(position[1] - destination[1]) < 15.0
    })?;
    game.stop(drone)?;
    advance_until(
        game,
        180,
        "Sentry Drone's original concealment delay",
        |game| game.entities[drone].stealthed(),
    )?;
    let target = friendly_entity(game, Kind::Builder, 1)?;
    require(
        !game.weapon_available(drone, 0) && game.attack(drone, target).is_err(),
        "unresearched Sentry Drone received a weapon",
    )?;
    Ok(Setup {
        drone,
        factory,
        target,
    })
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    upgrades(game, &setup)?;
    deployment(game, &setup)?;
    combat(game, &setup)?;
    println!(
        "Installed Sentry Drone: paid production, unarmed movement/concealment, Gun/Drone Armor research, guard deployment, turret centering/packing and ordered gunfire passed"
    );
    Ok(())
}

fn upgrades(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    research(game, setup.factory, "Upgrade_AmericaSentryDroneGun")?;
    require(
        game.weapon_available(setup.drone, 0),
        "Sentry Drone Gun research failed to allocate its native weapon",
    )?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let center_type = template(game, "AmericaStrategyCenter")?;
    advance_until(
        game,
        18_000,
        "supplies funding drone armor prerequisites",
        |game| game.players[0].cash >= game.catalog[center_type].cost,
    )?;
    let center = construct_type(game, builder, center_type, 0)?;
    research(game, center, "Upgrade_AmericaDroneArmor")?;
    require(
        game.entities[setup.drone].max_health.to_bits() == 375.0_f32.to_bits(),
        "Sentry Drone Armor did not add the native 75 health",
    )
}

fn deployment(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let position = game.entities[setup.drone].position;
    game.guard(setup.drone, position)?;
    advance_until(game, 600, "guard idle Sentry Drone deployment", |game| {
        game.entities[setup.drone].deployment_condition() == Some("UNPACKING")
    })?;
    let began = game.tick;
    advance_until(
        game,
        60,
        "Sentry Drone finishing native unpack time",
        |game| game.entities[setup.drone].deployment_condition() == Some("DEPLOYED"),
    )?;
    require(
        game.tick - began == 30,
        "Sentry Drone deployed before its original one-second delay",
    )?;
    let next = [position[0] + 120.0, position[1]];
    game.move_to(setup.drone, next)?;
    advance_until(
        game,
        600,
        "Sentry Drone centering turret then packing",
        |game| game.entities[setup.drone].deployment_condition() == Some("PACKING"),
    )?;
    let began = game.tick;
    advance_until(
        game,
        60,
        "Sentry Drone finishing original pack time",
        |game| game.entities[setup.drone].deployment_condition().is_none(),
    )?;
    require(
        game.tick - began == 30,
        "Sentry Drone packing skipped its original delay",
    )?;
    advance_until(
        game,
        1500,
        "researched Sentry Drone reaching move goal",
        |game| {
            let position = game.entities[setup.drone].position;
            (position[0] - next[0]).hypot(position[1] - next[1]) < 15.0
        },
    )?;
    game.stop(setup.drone)?;
    Ok(())
}

fn combat(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let position = game.entities[setup.drone].position;
    let range = game.entities[setup.drone]
        .active_weapon(&game.catalog[game.entities[setup.drone].type_index])
        .range;
    let approach = setup.approach_drone(game)?;
    advance_until(
        game,
        18_000,
        "enemy entering Sentry Drone's native range",
        |game| {
            let target = game.entities[setup.target].position;
            (target[0] - position[0]).hypot(target[1] - position[1]) < range
                && (target[0] - approach[0]).hypot(target[1] - approach[1]) < 15.0
        },
    )
    .map_err(|error| {
        format!(
            "{error}; Sentry={}; target={}",
            generals_sim::skirmish::diagnostics::movement_details(game, setup.drone),
            generals_sim::skirmish::diagnostics::movement_details(game, setup.target)
        )
    })?;
    game.stop(setup.target)?;
    let health = game.entities[setup.target].health;
    game.attack(setup.drone, setup.target)?;
    advance_until(
        game,
        1500,
        "Sentry Drone gun damaging its ordered enemy",
        |game| game.entities[setup.target].health < health,
    )?;
    require(
        game.entities[setup.drone].deployment_condition() == Some("DEPLOYED"),
        "ordered Sentry Drone fired before deployment",
    )
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_sentry_drone_uses_original_research_and_deploy_move_combat_loop() {
        let root = generals_assets::game_root().unwrap();
        let mut game = crate::infiltrator_verification::load_game(&root).unwrap();
        super::verify(&mut game).unwrap();
    }
}
