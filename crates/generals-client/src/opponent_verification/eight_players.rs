//! Regression for the reported Whiteout match: seven Hard computers and a sold-off human.
use super::{Faction, Kind, OpponentDifficulty, Skirmish, load_game};
use generals_sim::skirmish::PlayerSlot;

fn reset(game: &mut Skirmish, cash: u32) {
    game.set_match_options(cash, 0);
    let factions = [
        Faction::Usa,
        Faction::Usa,
        Faction::China,
        Faction::China,
        Faction::Gla,
        Faction::Gla,
        Faction::Usa,
        Faction::China,
    ];
    let starts = [6, 5, 7, 3, 2, 1, 0, 4];
    game.reset_slots(
        factions
            .into_iter()
            .enumerate()
            .map(|(owner, faction)| PlayerSlot {
                faction: Some(faction),
                occupied: true,
                difficulty: (owner != 0).then_some(OpponentDifficulty::Hard),
                team: None,
                start: Some(starts[owner]),
                color: Some(owner),
            })
            .collect(),
    )
    .unwrap();
}

fn economy_ready(game: &Skirmish, owner: usize) -> bool {
    let stats = &game.players[owner].statistics;
    stats.units_built > 1
        && stats.buildings_built > 2
        && stats.money_earned > 0
        && game.entities.iter().any(|actor| {
            actor.owner == owner
                && actor.complete()
                && game.catalog[actor.type_index].kind == Kind::SupplyCenter
        })
        && (game.players[owner].faction == Faction::Gla
            || game.entities.iter().any(|actor| {
                actor.owner == owner
                    && actor.complete()
                    && game.catalog[actor.type_index].kind == Kind::PowerPlant
            }))
}

fn case(game: &mut Skirmish, cash: u32) {
    reset(game, cash);
    for _ in 0..6000 {
        game.step();
        if (1..8).all(|owner| economy_ready(game, owner)) {
            break;
        }
        assert!(
            game.ended_at.is_none(),
            "Whiteout ended during initial AI economy"
        );
    }
    for owner in 1..8 {
        let stats = &game.players[owner].statistics;
        assert!(
            economy_ready(game, owner),
            "Whiteout cash {cash} AI {owner} at {}: {} units, {} buildings, {} income",
            game.tick,
            stats.units_built,
            stats.buildings_built,
            stats.money_earned
        );
    }
    assert!(
        !game.defeated[0],
        "human was lost before pre-sale progression checks"
    );
    let structures: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter(|(id, _)| game.can_sell(0, *id))
        .map(|(id, _)| id)
        .collect();
    assert!(!structures.is_empty());
    for id in structures {
        game.sell(0, id).unwrap();
    }
    let mut fired = [false; 8];
    for _ in 0..18_000 {
        game.step();
        for shot in &game.presentation.shots {
            if let Some(did_fire) = fired.get_mut(shot.owner) {
                *did_fire = true;
            }
        }
        if game.defeated[0] && (1..8).all(|owner| fired[owner]) {
            break;
        }
        assert!(
            game.ended_at.is_none(),
            "spare enemy alliances must keep observer match running"
        );
    }
    assert!(
        game.defeated[0],
        "normal human sale did not reach elimination"
    );
    assert!(
        game.ended_at.is_none(),
        "human elimination prematurely ended eight-player match"
    );
    for (owner, did_fire) in fired.into_iter().enumerate().skip(1) {
        assert!(
            did_fire,
            "Whiteout cash {cash} AI {owner} did not fire by {}",
            game.tick
        );
        assert!(
            game.players[owner].statistics.units_built > 3,
            "Whiteout AI {owner} did not produce a paid army"
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_whiteout_seven_hard_computers_progress_before_and_after_human_sale() {
    let root = &generals_assets::game_root().unwrap();
    let mut game = load_game(root, "maps/whiteout/whiteout.map").unwrap();
    for cash in [50_000, 70_000] {
        case(&mut game, cash);
    }
}
