//! Installed alternate weapons through paid research, ordinary commands and source flight.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub mod defenses;
pub mod research;
mod targeting;

pub fn airborne_supply_chinook(
    game: &mut Skirmish,
    center: usize,
) -> Result<usize, Box<dyn Error>> {
    let mut chinook = None;
    advance_until(
        game,
        1800,
        "Supply Center granting its Chinook and the Chinook taking off",
        |game| {
            chinook = game.entities.iter().enumerate().find_map(|(id, actor)| {
                let unit = &game.catalog[actor.type_index];
                (actor.producer() == Some(center)
                    && actor.complete()
                    && unit.name == "AmericaVehicleChinook"
                    && game.height_above_ground(id) > unit.locomotor.airborne_targeting_height)
                    .then_some(id)
            });
            chinook.is_some()
        },
    )?;
    chinook.ok_or_else(|| "Supply Center's airborne Chinook missing".into())
}

fn infantry(
    game: &mut Skirmish,
    name: &str,
    opponent: Faction,
) -> Result<(usize, usize), Box<dyn Error>> {
    game.reset(Faction::Usa, opponent)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let kind = game
        .catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or("weapon diagnostic infantry absent")?;
    Ok((produce_type(game, barracks, kind, 0)?, barracks))
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (ranger, _) = prepare_flashbang(game)?;
    let projectile = game
        .projectiles
        .iter()
        .find(|projectile| projectile.source == ranger && projectile.weapon_slot == 1)
        .ok_or("Flashbang firing lacked grenade flight")?;
    let id = projectile.id;
    require(
        projectile.render_transform(&game.entities).is_some(),
        "Flashbang grenade lacks original render pose",
    )?;
    game.switch_weapon(ranger, 0)?;
    game.stop(ranger)?;
    advance_until(game, 300, "native Flashbang detonation effect", |game| {
        game.presentation.weapon_fx.iter().any(|event| {
            matches!(event, generals_sim::skirmish::WeaponFxEvent::Projectile(event)
            if event.projectile == id && event.weapon_slot == 1
                && matches!(event.stage, generals_sim::skirmish::ProjectileFxStage::Detonation))
        })
    })?;
    let (defender, target) = prepare_laser(game)?;
    advance_until(game, 100, "prepared Laser Lock secondary missile", |game| {
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == defender && shot.weapon_slot == 1)
    })?;
    require(
        game.entities[defender].laser_target() == Some(target),
        "Laser Lock did not persist after first secondary shot",
    )?;
    require(
        game.projectiles
            .iter()
            .any(|projectile| projectile.source == defender && projectile.weapon_slot == 1),
        "Laser Lock lacked actual missile",
    )?;
    game.stop(defender)?;
    game.step(); // SpecialAbilityUpdate exits preparation and restores PRIMARY.
    require(
        game.entities[defender].laser_target().is_none()
            && game.entities[defender].weapon_slot() == 0,
        "cancelled Laser Lock did not retire beam and restore primary",
    )?;
    println!(
        "Installed weapons: paid Flashbang research, original B throw/grenade/detonation, persistent Laser Lock secondary missile and primary restoration passed"
    );
    targeting::verify(game)?;
    defenses::verify(game)?;
    Ok(())
}

pub fn prepare_flashbang(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (ranger, barracks) = infantry(game, "AmericaInfantryRanger", Faction::Gla)?;
    require(
        !game.weapon_available(ranger, 1),
        "unresearched Ranger had Flashbangs",
    )?;
    let research = game.catalog[game.entities[barracks].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_AmericaRangerFlashBangGrenade")
        .ok_or("original Flashbang research missing")?;
    game.queue_upgrade(barracks, research)?;
    advance_until(game, 30 * 90, "original Flashbang research", |game| {
        game.weapon_available(ranger, 1)
    })?;
    let target = friendly_entity(game, Kind::Builder, 1)?;
    // Flashbangs deal SURRENDER damage. Scout the GLA Worker, whose
    // HumanArmor admits the grenade; a vehicle Dozer's armor rejects it.
    let actor = &game.entities[ranger];
    let range = game.catalog[actor.type_index]
        .weapon_template(actor.weapon_index(1))
        .ok_or("missing researched flashbang")?
        .range;
    game.move_to(ranger, game.entities[target].position)?;
    advance_until(
        game,
        7200,
        "Ranger scouting an infantry flashbang target",
        |game| {
            let from = game.entities[ranger].position;
            let to = game.entities[target].position;
            (from[0] - to[0]).hypot(from[1] - to[1]) < range
                && matches!(
                    game.visibility_status(0, target),
                    generals_sim::skirmish::visibility::Status::Clear
                )
        },
    )?;
    game.switch_weapon(ranger, 1)?;
    game.attack(ranger, target)?;
    advance_until(
        game,
        30 * 180,
        "Ranger Flashbang throwing and native grenade creation",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == ranger && shot.weapon_slot == 1)
        },
    )?;
    require(
        game.entities[ranger].weapon_condition() == Some("FIRING_B"),
        "Flashbang did not request original throwing state",
    )?;
    Ok((ranger, target))
}

pub fn prepare_laser(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (defender, _) = infantry(game, "AmericaInfantryMissileDefender", Faction::China)?;
    let target = friendly_entity(game, Kind::Builder, 1)?;
    game.move_to(defender, game.entities[target].position)?;
    advance_until(
        game,
        30 * 180,
        "Missile Defender visibility for Laser Lock",
        |game| game.can_laser_lock(defender, target),
    )?;
    game.laser_lock(defender, target)?;
    advance_until(
        game,
        30 * 180,
        "original Laser Lock approach and beam preparation",
        |game| game.entities[defender].laser_target().is_some(),
    )?;
    Ok((defender, target))
}
