//! Paid original Hacker and opposing producer, with no simulation-state edits.
use crate::skirmish_verification::{
    advance_until, construct, construct_type_at, friendly_entity, require,
};
use generals_sim::skirmish::{Kind, Order, Skirmish};
use std::error::Error;

pub fn prepare(game: &mut Skirmish) -> Result<[usize; 2], Box<dyn Error>> {
    let hacker = super::prepare(game)?;
    game.stop(hacker)?;
    advance_until(
        game,
        500,
        "Hacker finishing original income packing",
        |game| {
            game.entities[hacker].hacking_condition().is_none()
                && matches!(game.entities[hacker].ai.order, Order::Idle)
        },
    )?;
    // The opposing player constructs an ordinary forward Barracks with its
    // initial Dozer. Neither side receives money, units or modified positions.
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::PowerPlant, 1)?;
    let barracks = game
        .type_for(1, Kind::Barracks)
        .ok_or("Missing USA Barracks")?;
    let position = game.entities[hacker].position;
    let point = game
        .placement_near(barracks, [position[0] + 220.0, position[1] + 80.0])
        .ok_or("No legal forward Barracks site")?;
    let target = construct_type_at(game, worker, barracks, 1, point)?;
    let approach = [point[0] - 100.0, point[1]];
    game.move_to(hacker, approach)?;
    advance_until(
        game,
        6000,
        "Hacker walking into target visibility",
        |game| {
            matches!(game.entities[hacker].ai.order, Order::Idle)
                && game.can_disable_building(hacker, target)
        },
    )?;
    let rules = game.catalog[game.entities[hacker].type_index]
        .behavior
        .special_abilities
        .iter().find(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialHackerDisableBuilding))
        .ok_or("Original Hacker disable module absent")?;
    require(
        rules.approach.start_range.to_bits() == 150.0_f32.to_bits()
            && rules.timing.unpack == 219
            && rules.timing.preparation == 90
            && rules.timing.persistent == 10
            && rules.timing.effect == 60
            && game.catalog[game.entities[hacker].type_index]
                .special_powers
                .iter()
                .any(|power| {
                    power.name == "SpecialAbilityHackerDisableBuilding"
                        && power.timer.reload_frames == 15
                }),
        "Installed Hacker disable values differ from original",
    )?;
    Ok([hacker, target])
}

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let [hacker, target] = prepare(game)?;
    game.disable_building(hacker, target)?;
    advance_until(
        game,
        1200,
        "Hacker facing/unpacking and starting BinaryDataStream",
        |game| game.entities[hacker].hacker_disable_target() == Some(target),
    )?;
    let started = game.tick;
    let ranger = game
        .catalog
        .iter()
        .position(|unit| unit.name == "AmericaInfantryRanger")
        .ok_or("Missing original Ranger")?;
    game.train(target, ranger)?;
    for _ in 0..89 {
        game.step();
        require(
            !game.entities[target].hacked(),
            "Hacker disabled target before 90-frame preparation",
        )?;
    }
    game.step();
    require(
        game.entities[target].hacked(),
        "Hacker failed to disable original producer",
    )?;
    let remaining = game.entities[target]
        .queue
        .front()
        .ok_or("Ranger queue unexpectedly completed")?
        .remaining_seconds;
    for _ in 0..80 {
        game.step();
        require(
            game.entities[target].hacked(),
            "Persistent disable developed a gap",
        )?;
    }
    require(
        game.entities[target]
            .queue
            .front()
            .is_some_and(|entry| entry.remaining_seconds.to_bits() == remaining.to_bits()),
        "Disabled producer advanced its paid queue",
    )?;
    game.stop(hacker)?;
    for _ in 0..59 {
        game.step();
        require(
            game.entities[target].hacked(),
            "Target reenabled before source effect deadline",
        )?;
    }
    game.step();
    require(
        !game.entities[target].hacked(),
        "Target failed to reenable at the end of its deadline frame",
    )?;
    require(
        game.entities[target]
            .queue
            .front()
            .is_some_and(|entry| entry.remaining_seconds.to_bits() == remaining.to_bits()),
        "Producer advanced during its deadline frame",
    )?;
    game.step();
    require(
        game.entities[target]
            .queue
            .front()
            .is_some_and(|entry| entry.remaining_seconds < remaining),
        "Reenabled producer did not resume",
    )?;
    println!(
        "Installed Hacker Disable Building: paid Hacker/forward Barracks, original movement/facing/unpack, 90-frame preparation, 10-frame persistent pulses, frozen paid queue, stop and 60-frame expiry/resume passed; preparation began {started}"
    );
    Ok(())
}
