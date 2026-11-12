//! Observe the original China Hacker recipe during continued ordinary play.
use super::{Skirmish, allied, performance};
use generals_sim::skirmish::income::Effect;
use std::error::Error;

pub(super) fn run(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let mut center = None;
    let mut hacker = None;
    let mut outdoor_income = 0;
    for iteration in 0_u32..18_000 {
        if game.ended_at.is_some() {
            break;
        }
        if iteration.is_multiple_of(30) {
            performance::queue_ready(game)?;
        }
        if iteration.is_multiple_of(120) {
            allied::command_army(game);
        }
        game.step();
        for actor in &game.entities {
            if actor.owner != 1 || !actor.complete() {
                continue;
            }
            let unit = &game.catalog[actor.type_index];
            if unit.name == "ChinaInternetCenter" && center.is_none() {
                center = Some(game.tick);
                println!(
                    "Original China AI completed Internet Center at {}",
                    game.tick
                );
            }
            if unit.behavior.hacking.is_some() && hacker.is_none() {
                hacker = Some(game.tick);
                println!("Original China AI produced Hacker at {}", game.tick);
            }
        }
        for event in &game.presentation.hacking_events {
            let actor = &game.entities[event.object];
            if actor.owner != 1 {
                continue;
            }
            let Effect::Cash { amount, .. } = event.effect else {
                continue;
            };
            if let Some(host) = actor.contained_by() {
                if game.catalog[game.entities[host].type_index].name != "ChinaInternetCenter" {
                    return Err("AI Hacker deposited income inside an unexpected host".into());
                }
                println!(
                    "Original China AI Center/Hacker chain passed at {}: center {center:?}, Hacker {hacker:?}, contained deposit ${amount}, prior outdoor income ${outdoor_income}",
                    game.tick
                );
                return Ok(());
            }
            outdoor_income += u64::from(amount);
        }
    }
    println!(
        "Original China AI contained income not observed before frame {} / outcome {:?}: center {center:?}, Hacker {hacker:?}, outdoor income ${outdoor_income}. No recipe conditions or gameplay state were changed.",
        game.tick, game.outcome
    );
    Ok(())
}
