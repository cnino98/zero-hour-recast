//! Real paid anti-air damage, retained falling Raptor, physical OCL and retirement.
use super::{
    Setup as Aircraft, advance_until, construct, details, friendly_entity, produce_type, require,
    template,
};
use generals_sim::skirmish::{Kind, Order, Skirmish, diagnostics::movement_details};
use std::error::Error;

pub struct Setup {
    pub aircraft: Aircraft,
    pub attacker: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let aircraft = super::prepare(game)?;
    stage(game, aircraft)
}

pub fn prepare_pilot_ejection(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let (aircraft, _) = prepare_veteran_raptor(game)?;
    stage(game, aircraft)
}

pub fn prepare_veteran_raptor(game: &mut Skirmish) -> Result<(Aircraft, usize), Box<dyn Error>> {
    let (aircraft, factory) = prepare_training(game)?;
    let tank = stage_training_tank(game, &aircraft, factory)?;
    earn_raptor_veterancy(game, aircraft.raptor, tank)?;
    advance_until(
        game,
        7200,
        "veteran Raptor safely rearmed before ejection trial",
        |game| {
            game.entities[aircraft.raptor].aircraft_phase() == Some("idle")
                && game.height_above_ground(aircraft.raptor) < 10.0
        },
    )?;
    Ok((aircraft, factory))
}

pub fn prepare_veteran_comanche(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (aircraft, factory) = prepare_training(game)?;
    let tank = stage_training_tank(game, &aircraft, factory)?;
    let heli = aircraft.comanche;
    if game.entities[tank].alive() {
        game.attack(heli, tank)?;
        advance_until(
            game,
            5400,
            "paid Comanche defeats original Battle Master",
            |game| !game.entities[tank].alive(),
        )
        .map_err(|error| {
            format!(
                "{error}; {}; {}",
                details(game, heli),
                movement_details(game, tank)
            )
        })?;
    }
    verify_veterancy(game, heli, tank)?;
    Ok((heli, factory))
}

fn prepare_training(game: &mut Skirmish) -> Result<(Aircraft, usize), Box<dyn Error>> {
    let aircraft = super::prepare(game)?;
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::SupplyCenter, 1)?;
    let factory = construct(game, builder, Kind::Factory, 1)?;
    Ok((aircraft, factory))
}

fn stage_training_tank(
    game: &mut Skirmish,
    aircraft: &Aircraft,
    factory: usize,
) -> Result<usize, Box<dyn Error>> {
    // The paid original 400-health Battle Master grants exactly the 100 XP a
    // each regular aircraft needs. Stage outside the opposing base.
    let kind = template(game, "ChinaTankBattleMaster")?;
    let tank = produce_type(game, factory, kind, 1)?;
    let base = game.entities[aircraft.target].position;
    let home = game.entities[aircraft.airfield].position;
    let direction = [home[0] - base[0], home[1] - base[1]];
    let length = direction[0].hypot(direction[1]);
    let [dx, dy] = direction.map(|axis| axis / length);
    let position = [0.0, -180.0, 180.0, -360.0, 360.0]
        .into_iter()
        .map(|side| {
            [
                dy.mul_add(-side, dx.mul_add(520.0, base[0])),
                dx.mul_add(side, dy.mul_add(520.0, base[1])),
            ]
        })
        .find(|&point| game.build_site_reachable(tank, point))
        .ok_or_else(|| {
            format!(
                "no reachable training ground from {base:?} toward {home:?}; {}",
                movement_details(game, tank)
            )
        })?;
    game.move_to(tank, position)?;
    advance_until(
        game,
        5400,
        "paid Battle Master reaches isolated flight range",
        |game| {
            let actor = &game.entities[tank];
            actor.alive()
                && !actor.moving()
                && matches!(actor.ai.order, Order::Idle)
                && (actor.position[0] - position[0]).hypot(actor.position[1] - position[1]) < 50.0
        },
    )
    .map_err(|error| {
        format!(
            "{error}; requested={position:?}; attacker={:?}; {}",
            game.entities[tank].last_attacker(),
            movement_details(game, tank)
        )
    })?;
    Ok(tank)
}

fn earn_raptor_veterancy(
    game: &mut Skirmish,
    jet: usize,
    tank: usize,
) -> Result<(), Box<dyn Error>> {
    for _ in 0..3 {
        super::approach(game, jet, tank)?;
        game.attack(jet, tank)?;
        advance_until(
            game,
            7200,
            "ordinary Raptor sortie against paid Battle Master",
            |game| {
                !game.entities[tank].alive()
                    || (game.entities[jet].aircraft_phase() == Some("idle")
                        && game.height_above_ground(jet) < 10.0
                        && game.entities[jet].weapon_rounds_remaining(0) == 4)
            },
        )?;
        if !game.entities[tank].alive() {
            break;
        }
    }
    verify_veterancy(game, jet, tank)
}

fn verify_veterancy(game: &Skirmish, aircraft: usize, tank: usize) -> Result<(), Box<dyn Error>> {
    require(
        !game.entities[tank].alive()
            && game.entities[tank].last_attacker() == Some(aircraft)
            && game.entities[aircraft].veterancy > 0,
        &format!(
            "paid Battle Master did not grant aircraft veterancy: health={} attacker={:?} XP={}; {}",
            game.entities[tank].health,
            game.entities[tank].last_attacker(),
            game.entities[aircraft].experience,
            details(game, aircraft)
        ),
    )
}

fn stage(game: &mut Skirmish, aircraft: Aircraft) -> Result<Setup, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    if friendly_entity(game, Kind::PowerPlant, 1).is_err() {
        construct(game, builder, Kind::PowerPlant, 1)?;
    }
    if friendly_entity(game, Kind::SupplyCenter, 1).is_err() {
        construct(game, builder, Kind::SupplyCenter, 1)?;
    }
    let factory = if let Ok(factory) = friendly_entity(game, Kind::Factory, 1) {
        factory
    } else {
        construct(game, builder, Kind::Factory, 1)?
    };
    let kind = template(game, "ChinaTankGattling")?;
    let attacker = produce_type(game, factory, kind, 1)?;
    let center = game.entities[aircraft.target].position;
    game.move_to(attacker, [center[0] + 600.0, center[1] - 350.0])?;
    advance_until(
        game,
        5400,
        "paid Gattling staging clear of aircraft approach",
        |game| {
            let actor = &game.entities[attacker];
            actor.alive()
                && !actor.moving()
                && matches!(actor.ai.order, Order::Idle)
                && (actor.position[0] - center[0]).hypot(actor.position[1] - center[1]) > 500.0
        },
    )?;
    let loiter = [center[0] + 100.0, center[1] - 500.0];
    game.move_to(aircraft.raptor, loiter)?;
    advance_until(
        game,
        5400,
        "Raptor airborne crash staging clear of command center",
        |game| {
            let actor = &game.entities[aircraft.raptor];
            actor.alive()
                && actor.aircraft_phase() == Some("idle")
                && game.height_above_ground(aircraft.raptor)
                    > game.catalog[actor.type_index]
                        .locomotor
                        .airborne_targeting_height
                && (actor.position[0] - loiter[0]).hypot(actor.position[1] - loiter[1]) < 150.0
        },
    )?;
    require(
        game.entities[aircraft.raptor].alive(),
        "Raptor died before the anti-air crash trial",
    )?;
    Ok(Setup { aircraft, attacker })
}

pub fn begin(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let id = setup.aircraft.raptor;
    let point = game.entities[id].position;
    require(
        game.height_above_ground(id)
            > game.catalog[game.entities[id].type_index]
                .locomotor
                .airborne_targeting_height,
        "paid Raptor must already be airborne before the crash trial",
    )?;
    // JetAIUpdate starts the authored 10-second return timer only while Idle.
    // An ordinary Guard order keeps this air-combat trial aloft during AA travel.
    game.guard(id, point)?;
    game.move_to(setup.attacker, point)?;
    Ok(())
}

pub fn attack_when_visible(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    if game.entities[setup.aircraft.raptor].alive()
        && game.logically_visible(1, setup.aircraft.raptor)
        && !matches!(game.entities[setup.attacker].ai.order, Order::Attack(id) if id == setup.aircraft.raptor)
    {
        game.attack(setup.attacker, setup.aircraft.raptor)
            .map_err(|error| {
                format!(
                    "paid crash anti-air order: {error}; {}; attacker={}",
                    details(game, setup.aircraft.raptor),
                    generals_sim::skirmish::diagnostics::movement_details(game, setup.attacker)
                )
            })?;
    }
    Ok(())
}

pub fn verify_death(game: &Skirmish, setup: &Setup) -> Result<u64, Box<dyn Error>> {
    let id = setup.aircraft.raptor;
    let actor = &game.entities[id];
    if !actor.jet_corpse() {
        let effects: Vec<_> = game
            .presentation
            .object_fx_events
            .iter()
            .filter(|event| event.object == id)
            .map(|event| (&event.effect, event.position))
            .collect();
        return Err(format!(
            "actual anti-air kill did not retain airborne jet: {}; altitude={} masked={} attacker={:?} effects={effects:?}; gun={}",
            details(game, id), game.height_above_ground(id), actor.masked(), actor.last_attacker(),
            generals_sim::skirmish::diagnostics::movement_details(game, setup.attacker)
        ).into());
    }
    require(
        actor.last_attacker() == Some(setup.attacker),
        "Raptor crash was not caused by paid Gattling fire",
    )?;
    let event = game
        .presentation
        .object_fx_events
        .iter()
        .find(|event| event.object == id && event.effect == "FX_RaptorDeathInitial")
        .ok_or("actual Raptor initial-death FX callback missing")?;
    require(
        game.entities.iter().any(|actor| {
            actor.producer() == Some(id)
                && actor.physical_present()
                && actor.physical_model() == Some("AVRaptor_D4")
        }),
        "initial Raptor death did not create its original wing debris",
    )?;
    Ok(event.frame)
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    begin(game, &setup)?;
    let mut shots = 0;
    let mut death = false;
    let mut hulk = false;
    let mut impact = false;
    for _ in 0..7200 {
        attack_when_visible(game, &setup)?;
        let actor = &game.entities[setup.aircraft.raptor];
        let previous = (
            actor.position,
            actor.flight_height(),
            actor.health,
            actor.aircraft_phase(),
        );
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == setup.attacker && shot.weapon_slot == 1)
            .count();
        if !death && !game.entities[setup.aircraft.raptor].alive() {
            verify_death(game, &setup).map_err(|error| format!("{error}; preceding frame (position,height,health,phase)={previous:?}; AA shots={shots}"))?;
            death = true;
        }
        impact |= game.presentation.object_fx_events.iter().any(|event| {
            event.object == setup.aircraft.raptor && event.effect == "FX_JetDeathHitGround"
        });
        hulk |= game.entities.iter().any(|actor| {
            actor.producer() == Some(setup.aircraft.raptor)
                && actor.physical_present()
                && game.catalog[actor.type_index].name == "AmericaJetRaptorHulk"
        });
        if death
            && game.entities[setup.aircraft.raptor].masked()
            && game.entities.iter().all(|actor| {
                actor.producer() != Some(setup.aircraft.raptor) || !actor.physical_present()
            })
        {
            require(
                shots > 0 && impact && hulk,
                "paid crash missed actual AA shots, ground FX, or original hulk",
            )?;
            println!(
                "Paid Raptor crash: Gattling AA shots={shots}, airborne retained body, source initial/ground FX, original debris/hulk and natural retirement passed"
            );
            return Ok(());
        }
    }
    Err(format!("Paid Raptor crash timed out: shots={shots} death={death} impact={impact} hulk={hulk}; {}; attacker={}", details(game, setup.aircraft.raptor), generals_sim::skirmish::diagnostics::movement_details(game, setup.attacker)).into())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_anti_air_kill_keeps_original_raptor_crash_debris_and_final_lifetimes() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        super::verify(&mut battle.simulation).unwrap();
    }
}
