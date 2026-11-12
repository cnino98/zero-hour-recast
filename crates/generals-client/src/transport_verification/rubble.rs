//! Original paid combat and construction against an authored retained building.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish, host: usize, rider: usize) -> Result<(), Box<dyn Error>> {
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let tank_type = crate::strategy_verification::template(game, "AmericaTankCrusader")?;
    let tank = produce_type(game, factory, tank_type, 0)?;
    // Clearing the civilian warehouse restored its neutral owner. Use the
    // original player force-attack command to destroy the empty structure.
    game.force_attack(tank, host)?;
    advance_until(
        game,
        18_000,
        "ordinary tank destruction of civilian garrison",
        |game| !game.entities[host].alive(),
    )?;
    require(
        game.retained_rubble(host) && !game.can_enter(rider, host),
        "destroyed civilian garrison did not retain non-boardable original rubble",
    )?;
    require(
        game.attack(tank, host).is_err() && game.force_attack(tank, host).is_err(),
        "dead civilian rubble remained an attack target",
    )?;
    let position = game.entities[host].position;
    let power = game
        .type_for(0, Kind::PowerPlant)
        .ok_or("USA power plant absent")?;
    // The preview accepts uninitialized zones until the deferred pathfinder
    // recalculation. Check the GROUND-only Dozer's actual settled zone first.
    advance_until(
        game,
        1800,
        "rubble center becoming unreachable to the Dozer",
        |game| !game.build_site_reachable(worker, position),
    )?;
    require(
        !game.can_build_facing(worker, power, position, game.catalog[power].placement_angle)
            && game.retained_rubble(host),
        "placement preview admitted the ground-only Dozer inside the rubble",
    )?;
    let cash = game.players[0].cash;
    let cost = game.build_cost(0, power);
    let building = build_overlapping(game, worker, power, host)?;
    require(
        game.players[0].cash == cash - cost,
        "overlapping foundation did not charge its original cost",
    )?;
    // BuildAssistant queues the corpse for native deferred destruction.
    game.step();
    require(
        game.entities[host].masked() && !game.retained_rubble(host),
        "paid construction failed to remove intersecting rubble",
    )?;
    advance_until(
        game,
        18_000,
        "ordinary construction over cleared rubble",
        |game| game.entities[building].complete(),
    )?;
    println!(
        "Installed rubble: paid Crusader kills original civilian warehouse, retained corpse rejects attack/entry, and paid normal construction clears its footprint and completes"
    );
    Ok(())
}

fn build_overlapping(
    game: &mut Skirmish,
    worker: usize,
    index: usize,
    host: usize,
) -> Result<usize, Box<dyn Error>> {
    use generals_sim::collision_geometry::{Body, intersects};
    let position = game.entities[host].position;
    // Diagnostic placements only: use the real map obstacles and original
    // collision predicates, since this warehouse is surrounded by authored props.
    for y in -16_i16..=16 {
        for x in -16_i16..=16 {
            let offset = [f32::from(x) * 10.0, f32::from(y) * 10.0];
            let center = [position[0] + offset[0], position[1] + offset[1]];
            for angle in [0.0, std::f32::consts::FRAC_PI_2] {
                let rubble = &game.entities[host];
                if !intersects(
                    &Body {
                        geometry: &game.catalog[rubble.type_index].geometry,
                        position: [position[0], position[1], 0.0],
                        angle: rubble.facing,
                    },
                    &Body {
                        geometry: &game.catalog[index].geometry,
                        position: [center[0], center[1], 0.0],
                        angle,
                    },
                ) || !game.can_build_facing(worker, index, center, angle)
                {
                    continue;
                }
                return Ok(game.build_facing(worker, index, center, angle)?);
            }
        }
    }
    Err("authored civilian rubble has no legal reachable overlapping foundation".into())
}
