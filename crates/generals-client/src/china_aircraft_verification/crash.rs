//! Paid opposing Patriots kill an airborne `MiG` through ordinary anti-air combat.
use super::{
    Setup, advance_until, construct, details, friendly_entity, produce_type, require, template,
};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub(super) fn prepare(game: &mut Skirmish, setup: &Setup) -> Result<[usize; 2], Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    let mut defenders = [0; 2];
    let center = game.entities[setup.target].position;
    let kind = template(game, "AmericaPatriotBattery")?;
    for (index, defender) in defenders.iter_mut().enumerate() {
        let point = game
            .placement_near(
                kind,
                [center[0] + 900.0, center[1] - 350.0 + [0.0, 140.0][index]],
            )
            .ok_or("no legal Patriot site outside MiG bombardment")?;
        crate::skirmish_verification::scout_construction_site(game, builder, kind, point)?;
        let cash = game.players[1].cash;
        *defender = game.build(builder, kind, point)?;
        require(
            game.players[1].cash.checked_add(game.catalog[kind].cost) == Some(cash),
            "Patriot did not deduct its original price",
        )?;
        advance_until(game, 7200, "paid Patriot construction", |game| {
            game.entities[*defender].complete()
        })?;
    }
    require(
        defenders.iter().all(|&id| {
            (game.entities[id].position[0] - center[0])
                .hypot(game.entities[id].position[1] - center[1])
                > 500.0
        }),
        "Patriot defenses were placed in the earlier firestorm trial",
    )?;
    Ok(defenders)
}

pub(super) fn verify(
    game: &mut Skirmish,
    setup: &Setup,
    defenders: [usize; 2],
) -> Result<(), Box<dyn Error>> {
    let id = setup.migs[0];
    let point = game.entities[defenders[0]].position;
    game.move_to(id, point)?;
    let mut seen = [false; 4];
    let mut shots = 0;
    let mut engaged = false;
    for _ in 0..9000 {
        if !engaged
            && game.entities[id].alive()
            && game.entities[defenders[0]].alive()
            && game.logically_visible(0, defenders[0])
        {
            game.attack(id, defenders[0]).map_err(|error| {
                format!(
                    "MiG counterattack on visible Patriot: {error}; {}",
                    details(game, setup)
                )
            })?;
            engaged = true;
        }
        // Powered Patriot AI acquires the aircraft only inside its own original
        // weapon range; another battery's shared vision is not firing range.
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| defenders.contains(&shot.source))
            .count();
        seen[0] |= game.entities[id].jet_corpse()
            && game.entities[id]
                .last_attacker()
                .is_some_and(|attacker| defenders.contains(&attacker));
        seen[1] |= game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.object == id && event.effect == "FX_MigDeathInitial");
        seen[2] |= game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.object == id && event.effect == "FX_JetDeathHitGround");
        seen[3] |= game.entities.iter().any(|actor| {
            actor.producer() == Some(id)
                && actor.physical_present()
                && game.catalog[actor.type_index].name == "ChinaJetMIGHulk"
        });
        if seen.into_iter().all(|value| value)
            && game.entities[id].masked()
            && game
                .entities
                .iter()
                .all(|actor| actor.producer() != Some(id) || !actor.physical_present())
        {
            require(shots > 0, "MiG crash had no real AA shots")?;
            require(
                game.parking_available(setup.airfield, game.entities[id].type_index),
                "destroyed MiG did not free its original hangar",
            )?;
            let kind = game.entities[id].type_index;
            let replacement = produce_type(game, setup.airfield, kind, 0)?;
            require(
                game.entities[replacement].max_health.to_bits() == 200.0_f32.to_bits()
                    && game.entities[replacement]
                        .active_weapon(&game.catalog[kind])
                        .name
                        == "BlackNapalmMissileWeapon",
                "replacement paid MiG did not inherit completed player research",
            )?;
            println!(
                "Paid MiG crash: AA shots={shots}, original airborne death/ground effects, ChinaJetMIGHulk, natural physical retirement and released hangar passed"
            );
            return Ok(());
        }
    }
    let defenders = defenders.map(|id| (id, game.entities[id].position, game.entities[id].health));
    Err(format!("Paid MiG crash timed out shots={shots} corpse/initial/ground/hulk={seen:?} Patriots={defenders:?}; {}", details(game, setup)).into())
}
