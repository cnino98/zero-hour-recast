//! Paid original Demo Trap construction, modes and delayed final weapon.
use crate::{
    skirmish_verification::{advance_until, construct, friendly_entity, require},
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub trap: usize,
    pub victim: usize,
}

/// Leave the paid trap in manual mode with an ordinary enemy inside its range.
pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::China)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    construct(game, worker, Kind::Barracks, 0)?;
    let index = template(game, "GLADemoTrap")?;
    let position = game
        .placement_near(index, game.entities[worker].position)
        .ok_or("no legal Demo Trap site")?;
    let cash = game.players[0].cash;
    require(
        game.build(worker, index, position).is_err() && game.players[0].cash == cash,
        "Demo Trap bypassed the original Arms Dealer prerequisite",
    )?;
    construct(game, worker, Kind::Factory, 0)?;
    let base = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    let position = game
        .placement_near(index, [base[0] + 320.0, base[1] + 320.0])
        .ok_or("no open Demo Trap site")?;
    crate::skirmish_verification::scout_construction_site(game, worker, index, position)?;
    let cash = game.players[0].cash;
    let trap = game.build(worker, index, position)?;
    require(
        game.players[0].cash == cash - 400,
        "Demo Trap construction did not charge its original cost",
    )?;
    advance_until(game, 18_000, "paid Demo Trap construction", |game| {
        game.entities[trap].complete()
    })?;
    require(
        game.entities[trap].weapon_slot() == 1 && game.entities[trap].veterancy == 0,
        "Demo Trap did not begin in its original proximity mode",
    )?;
    game.switch_weapon(trap, 2)?;
    let victim = friendly_entity(game, Kind::Builder, 1)?;
    let center = game.entities[trap].position;
    game.move_to(victim, [center[0] + 18.0, center[1]])?;
    advance_until(
        game,
        18_000,
        "enemy approaching the manual Demo Trap",
        |game| {
            let point = game.entities[victim].position;
            (point[0] - center[0]).hypot(point[1] - center[1]) < 25.0
        },
    )
    .map_err(|error| {
        format!(
            "{error}; trap={center:?}, dozer={:?}, moving={}, alive={}",
            game.entities[victim].position,
            game.entities[victim].moving(),
            game.entities[victim].alive()
        )
    })?;
    game.stop(victim)?;
    advance_until(
        game,
        180,
        "Demo Trap's native zero-delay concealment",
        |game| game.entities[trap].stealthed(),
    )?;
    require(
        game.entities[trap].alive() && !game.logically_visible(1, trap),
        "manual mode detonated or exposed the concealed Demo Trap",
    )?;
    Ok(Setup { trap, victim })
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    for mode in [0, 1] {
        let setup = prepare(game)?;
        let health = game.entities[setup.victim].health;
        game.switch_weapon(setup.trap, mode)?;
        advance_until(game, 3, "Demo Trap command beginning its warning", |game| {
            !game.entities[setup.trap].alive()
        })?;
        require(
            game.entities[setup.trap].corpse_present()
                && game.entities[setup.victim].health.to_bits() == health.to_bits(),
            "Demo Trap skipped its delayed warning or damaged the victim early",
        )?;
        let warning = game.tick;
        advance_until(game, 60, "original Demo Trap final weapon", |game| {
            !game.entities[setup.trap].corpse_present()
        })?;
        require(
            game.tick - warning == 30 && game.entities[setup.victim].health < health,
            "Demo Trap did not detonate after the original thirty frames",
        )?;
    }
    println!(
        "Installed Demo Trap: paid prerequisites/construction, original proximity/manual commands, concealment and thirty-frame warning/final weapon passed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_demo_trap_modes_keep_original_warning_and_explosion() {
        let root = generals_assets::game_root().unwrap();
        let mut game = crate::infiltrator_verification::load_game(&root).unwrap();
        super::verify(&mut game).unwrap();
    }
}
