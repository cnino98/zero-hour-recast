//! Paid combat setup: remove the original idle repairer before attacking its base.
use crate::skirmish_verification::{advance_until, construct, friendly_entity, produce};
use generals_sim::skirmish::{Kind, Skirmish, diagnostics::movement_details};
use std::error::Error;

struct Attack {
    tank: usize,
    repairer: usize,
    base: usize,
}

fn prepare_attack(game: &mut Skirmish) -> Result<Attack, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::SupplyCenter, 1)?;
    let factory = construct(game, builder, Kind::Factory, 1)?;
    Ok(Attack {
        tank: produce(game, factory, Kind::Tank, 1)?,
        repairer: friendly_entity(game, Kind::Builder, 0)?,
        base: friendly_entity(game, Kind::CommandCenter, 0)?,
    })
}

pub(super) fn defeat(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let attack = prepare_attack(game)?;
    // SH DozerPrimaryIdleState::update finds the damaged nearby command center.
    // Its original repair rate exceeds a lone Battlemaster's damage rate. Use
    // ordinary target selection rather than disabling repair or granting damage.
    game.attack(attack.tank, attack.repairer)?;
    advance_until(game, 18_000, "defending repairer destroyed", |game| {
        !game.entities[attack.repairer].alive()
    })
    .map_err(|error| failure(game, &attack, error.as_ref()))?;
    game.attack(attack.tank, attack.base)?;
    advance_until(game, 30 * 60 * 30, "original combat result", |game| {
        game.ended_at.is_some()
    })
    .map_err(|error| failure(game, &attack, error.as_ref()))
}

fn failure(game: &Skirmish, attack: &Attack, error: &dyn Error) -> Box<dyn Error> {
    format!(
        "{error}; tank={}; repairer={}; base={}",
        movement_details(game, attack.tank),
        movement_details(game, attack.repairer),
        movement_details(game, attack.base),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_sim::skirmish::Faction;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_single_tank_cannot_outdamage_the_original_idle_repairer() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        battle.start(Faction::Usa);
        let game = &mut battle.simulation;
        let attack = prepare_attack(game).unwrap();
        game.attack(attack.tank, attack.base).unwrap();
        advance_until(game, 18_000, "first command center hit", |game| {
            game.entities[attack.base].health < game.entities[attack.base].max_health
        })
        .unwrap();

        let mut shots = 0;
        let mut repairs = 0;
        let mut restorations = 0;
        let mut previous_health = game.entities[attack.base].health;
        for _ in 0..3600 {
            game.step();
            shots += game
                .presentation
                .shots
                .iter()
                .filter(|shot| shot.source == attack.tank)
                .count();
            if game.entities[attack.repairer].repair_task_target() == Some(attack.base)
                && game.entities[attack.base].health > previous_health
            {
                repairs += 1;
            }
            let base = &game.entities[attack.base];
            if previous_health < base.max_health
                && base.health.to_bits() == base.max_health.to_bits()
            {
                restorations += 1;
            }
            previous_health = base.health;
        }
        assert!(shots >= 20, "ordinary tank must keep firing: {shots}");
        assert!(repairs > 0, "the actual idle Dozer must restore health");
        assert!(
            restorations >= 2,
            "repair must repeatedly restore full health"
        );
        assert!(game.entities[attack.repairer].alive());
        assert!(game.entities[attack.base].alive());
        assert!(game.ended_at.is_none());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_defeat_preparation_reaches_the_original_two_player_result() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        battle.start(Faction::Usa);
        super::super::prepare(&mut battle.simulation, false).unwrap();
        assert_eq!(battle.simulation.outcome, Some(1));
        assert!(battle.simulation.ended_at.is_some());
        assert!(battle.simulation.players[1].statistics.units_destroyed > 0);
        assert!(battle.simulation.players[0].statistics.units_lost > 0);
        assert!(battle.simulation.players[1].statistics.buildings_destroyed > 0);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_victory_preparation_reaches_the_original_two_player_result() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        // ScreenCapture::Results(true) loads the original opponent rules before
        // Match::start; the ordinary paid fixture intentionally omits that AI.
        let map = generals_assets::map::MapData::parse(
            &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
        )
        .unwrap();
        let opponent = crate::opponent_data::load(&assets, &map).unwrap();
        battle
            .simulation
            .configure_opponent(
                opponent,
                1,
                generals_sim::skirmish::OpponentDifficulty::Easy,
            )
            .unwrap();
        battle.start(Faction::Usa);
        super::super::prepare(&mut battle.simulation, true).unwrap();
        assert_eq!(battle.simulation.outcome, Some(0));
        assert!(battle.simulation.ended_at.is_some());
        assert!(battle.simulation.players[0].statistics.buildings_destroyed > 0);
        assert!(battle.simulation.players[1].statistics.money_earned > 0);
    }
}
