//! Real paid anti-air damage, retained falling Raptor, physical OCL and retirement.
use super::{
    Setup as Aircraft, advance_until, construct, details, friendly_entity, produce_type, require,
    template,
};
use generals_sim::skirmish::{Kind, Order, Skirmish};
use std::error::Error;

pub struct Setup {
    pub aircraft: Aircraft,
    pub attacker: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let aircraft = super::prepare(game)?;
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::SupplyCenter, 1)?;
    let factory = construct(game, builder, Kind::Factory, 1)?;
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
