use super::*;
use crate::skirmish::{Kind, PlayerStart, startup, visibility::Status};

fn four_player_game() -> Skirmish {
    let mut game = crate::skirmish::tests::game();
    game.layout.starts = [
        [140.0, 140.0],
        [660.0, 140.0],
        [660.0, 460.0],
        [140.0, 460.0],
    ]
    .into_iter()
    .map(|position| PlayerStart {
        position,
        rally: None,
    })
    .collect();
    let slots = (0..4)
        .map(|owner| PlayerSlot {
            faction: Some(Faction::Gla),
            occupied: true,
            difficulty: None,
            team: None,
            start: Some(owner),
            color: Some(owner),
        })
        .collect();
    game.reset_slots(slots).unwrap();
    game
}

fn defeat(game: &mut Skirmish, owner: usize) {
    for entity in &mut game.entities {
        if entity.owner == owner {
            entity.health = 0.0;
        }
    }
    game.update_victory();
}

#[test]
fn individual_ai_elimination_does_not_finish_four_player_match() {
    let mut game = four_player_game();
    defeat(&mut game, 1);
    assert!(game.defeated[1]);
    assert!(game.ended_at.is_none());
    assert!(!game.player_active(1));
    assert!(game.player_active(2));
    defeat(&mut game, 2);
    assert!(game.ended_at.is_none());
    defeat(&mut game, 3);
    assert_eq!(game.outcome, Some(0));
    assert!(game.victorious(0));
    assert!(!game.victorious(game.neutral_owner()));
}

#[test]
fn one_alliance_has_no_elimination_scripts_even_with_four_occupied_slots() {
    let mut game = four_player_game();
    for slot in &mut game.slots {
        slot.team = Some(1);
    }
    game.update_victory();
    assert!(game.ended_at.is_none());
    defeat(&mut game, 1);
    assert!(game.ended_at.is_none());
    assert!(!game.defeated[1]);
}

#[test]
fn victorious_alliance_includes_previously_defeated_ally_and_stays_latched() {
    let mut game = four_player_game();
    let mut slots = game.slots.clone();
    slots[0].team = Some(0);
    slots[2].team = Some(0);
    game.reset_slots(slots).unwrap();
    game.tick = 10;
    defeat(&mut game, 0);
    assert!(game.ended_at.is_none());
    let unseen = [400.0, 300.0];
    assert!(matches!(game.visibility_cell(0, unseen), Status::Clear));
    // The permanent defeated-player reveal deliberately bypasses allied sight.
    assert!(matches!(game.visibility_cell(2, unseen), Status::Shrouded));
    defeat(&mut game, 1);
    defeat(&mut game, 3);
    assert_eq!(game.outcome, Some(2));
    assert!(game.victorious(0));
    // Marking the surviving alliance victorious does not reveal its map.
    assert!(matches!(game.visibility_cell(2, unseen), Status::Shrouded));
    defeat(&mut game, 2);
    assert_eq!(game.outcome, Some(2));
    assert!(game.victorious(0));
}

#[test]
fn explicit_starts_do_not_seed_native_team_first_position() {
    let mut game = four_player_game();
    let mut slots = game.slots.clone();
    for slot in &mut slots {
        slot.start = None;
        slot.team = Some(0);
    }
    slots[0].start = Some(0);
    game.reset_slots(slots).unwrap();
    // Native manual-start loop leaves teamPosIdx unset. First random member
    // goes farthest from the assigned corner; next teammate goes nearest it.
    assert_eq!(
        game.start_positions,
        vec![Some(0), Some(2), Some(1), Some(3), None]
    );
    for owner in 0..4 {
        let center = game
            .entities
            .iter()
            .find(|entity| {
                entity.owner == owner && game.catalog[entity.type_index].kind == Kind::CommandCenter
            })
            .unwrap();
        assert_eq!(
            center.position.map(f32::to_bits),
            game.player_start(owner).unwrap().position.map(f32::to_bits)
        );
    }
}

#[test]
fn empty_slots_consume_no_start_rng_and_invalid_selections_leave_match_intact() {
    let mut game = four_player_game();
    let mut slots = game.slots.clone();
    for slot in &mut slots {
        slot.start = None;
    }
    slots[1].occupied = false;
    slots[3].occupied = false;
    game.reset_slots(slots.clone()).unwrap();
    // The fixture also contains original warehouse actors. Check starting
    // armies by their actual owner and kind, independently of neutral content.
    for (owner, slot) in slots.iter().enumerate() {
        let owned: Vec<_> = game
            .entities
            .iter()
            .filter(|actor| actor.owner == owner)
            .collect();
        if slot.occupied {
            assert_eq!(owned.len(), 2);
            assert!(
                owned
                    .iter()
                    .any(|actor| game.catalog[actor.type_index].kind == Kind::CommandCenter)
            );
            assert!(
                owned
                    .iter()
                    .any(|actor| game.catalog[actor.type_index].kind == Kind::Builder)
            );
        } else {
            assert!(owned.is_empty());
        }
    }
    assert_eq!(game.start_positions[1], None);
    assert_eq!(game.start_positions[3], None);
    assert!(!game.enemies(0, 1));
    assert!(!game.enemies(0, game.neutral_owner()));
    let starts = game.start_positions.clone();
    let random = game.random;
    game.reset_slots(slots.clone()).unwrap();
    assert_eq!(game.start_positions, starts);
    assert_eq!(game.random, random);
    slots[0].start = Some(1);
    slots[2].start = Some(1);
    assert!(game.reset_slots(slots).is_err());
    assert_eq!(game.start_positions, starts);
}

#[test]
fn random_colors_reserve_explicit_choices_and_restart_replays_the_logic_rng() {
    let mut game = four_player_game();
    let mut slots = game.slots.clone();
    for slot in &mut slots {
        slot.start = None;
        slot.color = None;
    }
    slots[1].color = Some(7);
    game.reset_slots(slots.clone()).unwrap();
    let colors = game.player_colors.clone();
    let starts = game.start_positions.clone();
    let random = game.random;
    assert_eq!(colors[1], Some(7));
    for (owner, color) in colors.iter().enumerate() {
        assert!(color.is_some());
        assert!(!colors[..owner].contains(color));
    }
    for _ in 0..97 {
        game.random_between(100);
    }
    game.reset_slots(slots).unwrap();
    assert_eq!(game.player_colors, colors);
    assert_eq!(game.start_positions, starts);
    assert_eq!(game.random, random);
    assert_eq!(game.slots[0].color, None);
}

#[test]
fn duplicate_colors_keep_first_owner_and_empty_random_slot_draws_nothing() {
    let mut game = four_player_game();
    let mut slots = game.slots.clone();
    slots[1].color = Some(0);
    slots[2].occupied = false;
    slots[2].color = None;
    game.reset_slots(slots.clone()).unwrap();
    assert_eq!(game.player_colors[0], Some(0));
    assert_ne!(game.player_colors[1], Some(0));
    assert_eq!(game.player_colors[2], None);
    let random = game.random;
    slots[2].color = (0..8).find(|color| !game.player_colors.contains(&Some(*color)));
    game.reset_slots(slots).unwrap();
    assert_eq!(game.random, random);
}

#[test]
fn native_random_factions_interleave_colors_and_replay_original_choices_on_restart() {
    let mut game = four_player_game();
    for faction in [Faction::Usa, Faction::China] {
        for kind in [Kind::CommandCenter, Kind::Builder] {
            game.catalog
                .push(crate::skirmish::tests::fixture_type(faction, kind));
        }
    }
    game.layout.random_seed = 31;
    let mut slots = game.slots.clone();
    for slot in &mut slots {
        slot.faction = None;
        slot.color = None;
        slot.start = None;
    }
    slots[1].faction = Some(Faction::China);
    slots[1].color = Some(6);
    slots[2].occupied = false;
    let mut random = startup::seed_random(31);
    let mut colors = vec![None, Some(6), None, None];
    let mut factions = vec![Faction::Civilian; 4];
    for owner in [0, 1, 3] {
        factions[owner] = slots[owner].faction.unwrap_or_else(|| {
            for _ in 0..3 {
                startup::random_value(&mut random);
            }
            let index = usize::try_from(startup::random_value(&mut random) % 1001 % 3).unwrap();
            [Faction::Usa, Faction::China, Faction::Gla][index]
        });
        if colors[owner].is_none() {
            loop {
                let color = usize::try_from(startup::random_value(&mut random) % 8).unwrap();
                if !colors.contains(&Some(color)) {
                    colors[owner] = Some(color);
                    break;
                }
            }
        }
    }
    // Fix starts for this first-phase oracle so resolution consumes only the
    // native side/color draws calculated above. The restart check below keeps
    // the original random starts and exercises the full combined sequence.
    let mut fixed_starts = slots.clone();
    for (owner, slot) in fixed_starts.iter_mut().enumerate() {
        slot.start = Some(owner);
    }
    let starts: Vec<_> = game
        .layout
        .starts
        .iter()
        .map(|start| start.position)
        .collect();
    let resolved = resolve_player_slots(&fixed_starts, 31, 8, &starts).unwrap();
    assert!(resolved.factions == factions);
    assert_eq!(resolved.colors, colors);
    assert_eq!(resolved.random, random);
    game.reset_slots(slots.clone()).unwrap();
    let resolved: Vec<_> = game.players.iter().map(|player| player.faction).collect();
    let positions = game.start_positions.clone();
    let random = game.random;
    for _ in 0..97 {
        game.random_between(100);
    }
    game.reset_slots(slots).unwrap();
    assert!(
        game.players
            .iter()
            .map(|player| player.faction)
            .collect::<Vec<_>>()
            == resolved
    );
    assert_eq!(game.player_colors, colors);
    assert_eq!(game.start_positions, positions);
    assert_eq!(game.random, random);
    assert!(game.slots[0].faction.is_none());
    assert!(game.slots[3].faction.is_none());
}
