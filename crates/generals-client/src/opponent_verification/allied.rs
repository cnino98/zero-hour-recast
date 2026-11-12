//! Ordinary-command coverage of the reported USA/China versus two GLA setup.
use super::{Faction, Kind, OpponentDifficulty, Order, Skirmish};
use crate::skirmish_verification::{construct, friendly_entity, produce, require};
use generals_sim::skirmish::PlayerSlot;
use std::error::Error;

pub(super) fn run(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    reset(game)?;
    println!("Fallen Empire: human USA and Hard China versus two Hard GLA");
    let tank = prepare(game)?;
    let before = game.entities[tank].position;
    game.move_to(tank, [before[0] + 80.0, before[1] + 80.0])?;
    let mut fired = [false; 4];
    let mut guarded = [false; 4];
    let mut moved = false;
    for iteration in 0..18_000 {
        game.step();
        moved |= game.entities[tank].position.map(f32::to_bits) != before.map(f32::to_bits);
        for shot in &game.presentation.shots {
            if let Some(fired) = fired.get_mut(shot.owner) {
                *fired = true;
            }
        }
        for actor in &game.entities {
            if actor.complete()
                && matches!(actor.ai.order, Order::Guard(_))
                && let Some(guarded) = guarded.get_mut(actor.owner)
            {
                *guarded = true;
            }
        }
        if iteration > 60 && iteration % 120 == 0 {
            command_army(game);
        }
        if game.tick.is_multiple_of(3000) {
            super::multiple_progress(game, fired);
        }
        if moved && super::multiple_complete(game, fired, false) {
            break;
        }
        if game.ended_at.is_some() {
            break;
        }
    }
    super::multiple_progress(game, fired);
    require(moved, "ordinary tank move did not move the selected tank")?;
    require(
        super::multiple_complete(game, fired, false),
        "allied Hard match did not record income, construction and firing from all three computers",
    )?;
    println!(
        "Allied Hard scenario: two barracks, two factories, ordinary tank movement and three active computers passed at frame {}; guarding observed {guarded:?}. This is setup coverage, not a reproduction of the historical crash.",
        game.tick
    );
    Ok(())
}

pub(super) fn reset(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let factions = [Faction::Usa, Faction::China, Faction::Gla, Faction::Gla];
    game.reset_slots(
        factions
            .into_iter()
            .enumerate()
            .map(|(owner, faction)| PlayerSlot {
                faction: Some(faction),
                occupied: true,
                difficulty: (owner != 0).then_some(OpponentDifficulty::Hard),
                team: Some(u8::from(owner >= 2)),
                start: None,
                color: Some(owner),
            })
            .collect(),
    )?;
    Ok(())
}

pub(super) fn prepare(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    produce(game, barracks, Kind::Infantry, 0)?;
    let tank = produce(game, factory, Kind::Tank, 0)?;
    construct(game, builder, Kind::Barracks, 0)?;
    let second = construct(game, builder, Kind::Factory, 0)?;
    produce(game, second, Kind::Tank, 0)?;
    Ok(tank)
}

pub(super) fn command_army(game: &mut Skirmish) {
    let Some(target) = game
        .entities
        .iter()
        .find(|actor| actor.alive() && game.enemies(0, actor.owner))
        .map(|actor| actor.position)
    else {
        return;
    };
    let army = game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, actor)| {
            actor.owner == 0
                && actor.complete()
                && matches!(actor.ai.order, Order::Idle)
                && matches!(
                    game.catalog[actor.type_index].kind,
                    Kind::Infantry | Kind::Tank
                )
        })
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    for id in army {
        let _ = game.attack_move(id, target);
    }
}
