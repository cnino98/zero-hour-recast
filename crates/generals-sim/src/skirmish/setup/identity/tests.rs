use crate::skirmish::{Faction, FactionProgression, Kind, ProgressionRules, Rank, Skirmish, tests};

fn rules() -> ProgressionRules {
    ProgressionRules {
        multiplayer_colors: tests::original_multiplayer_colors(),
        ranks: vec![Rank {
            skill_points: 0,
            science_points: 0,
            sciences: Vec::new(),
        }],
        sciences: Vec::new(),
        powers: Vec::new(),
        factions: [
            (Faction::Usa, "America", 0xff00_00ff, true),
            (Faction::China, "China", 0xffff_0000, true),
            (Faction::Gla, "GLA", 0xff00_ff00, true),
            (Faction::Civilian, "Civilian", 0xffff_ffff, false),
        ]
        .into_iter()
        .map(
            |(faction, side, preferred_color, playable_side)| FactionProgression {
                faction,
                side: side.into(),
                preferred_color,
                playable_side,
                intrinsic_points: 0,
                intrinsic_sciences: Vec::new(),
            },
        )
        .collect(),
    }
}

fn game() -> Skirmish {
    let mut game = tests::game();
    for faction in [Faction::Usa, Faction::China] {
        for kind in [Kind::CommandCenter, Kind::Builder] {
            game.catalog.push(tests::fixture_type(faction, kind));
        }
    }
    game.configure_progression(rules()).unwrap();
    game
}

#[test]
fn player_identity_uses_template_side_and_actual_slot_palette_and_resets_stale_values() {
    let mut game = game();
    let mut slots = game.slots.clone();
    slots[0].faction = Some(Faction::Usa);
    slots[0].color = Some(7);
    slots[1].faction = Some(Faction::China);
    slots[1].color = Some(2);
    game.reset_slots(slots.clone()).unwrap();
    assert_eq!(game.players[0].side.as_deref(), Some("America"));
    assert!(game.players[0].playable_side);
    assert_eq!(game.players[0].color, Some(0xffff_96ff));
    assert_eq!(game.players[1].side.as_deref(), Some("China"));
    assert_eq!(game.players[1].color, Some(0xff43_68fe));
    game.players[0].side = Some("StaleCase".into());
    game.players[0].color = Some(0);
    game.players[0].playable_side = false;
    game.reset_slots(slots).unwrap();
    assert_eq!(game.players[0].side.as_deref(), Some("America"));
    assert_eq!(game.players[0].color, Some(0xffff_96ff));
    assert!(game.players[0].playable_side);
}

#[test]
fn player_identity_random_restart_and_duplicate_color_resolution_retain_original_rng() {
    let mut game = game();
    game.set_random_seed(31);
    let mut slots = game.slots.clone();
    for slot in &mut slots {
        slot.faction = None;
        slot.color = None;
    }
    game.reset_slots(slots.clone()).unwrap();
    let identities: Vec<_> = game
        .players
        .iter()
        .map(|player| (player.side.clone(), player.color))
        .collect();
    let random = game.random;
    for owner in 0..game.slots.len() {
        let index = game.player_colors[owner].unwrap();
        assert_eq!(
            game.players[owner].color,
            Some(tests::original_multiplayer_colors()[index])
        );
    }
    game.random_between(100);
    game.reset_slots(slots.clone()).unwrap();
    assert_eq!(game.random, random);
    assert_eq!(
        game.players
            .iter()
            .map(|player| (player.side.clone(), player.color))
            .collect::<Vec<_>>(),
        identities
    );
    for slot in &mut slots {
        slot.color = Some(0);
    }
    game.reset_slots(slots).unwrap();
    assert_eq!(game.players[0].color, Some(0xffdd_e20d));
    assert_ne!(game.players[1].color, Some(0xffdd_e20d));
}

#[test]
fn player_identity_distinguishes_native_neutral_from_named_civilian_template_and_closed_slot() {
    let mut game = game();
    let mut source = rules();
    source
        .factions
        .iter_mut()
        .find(|template| template.faction == Faction::Civilian)
        .unwrap()
        .preferred_color = 0xff11_2233;
    game.configure_progression(source).unwrap();
    game.layout
        .map_sides
        .push(crate::skirmish::map_scripts::SideDefinition {
            name: "civilian1".into(),
            faction: "FactionCivilian".into(),
            human: false,
            template_money: 0,
            start_money: 0,
            gatherers: [0; 3],
            allies: Vec::new(),
            enemies: Vec::new(),
        });
    let mut slots = game.slots.clone();
    slots[1].occupied = false;
    game.reset_slots(slots).unwrap();
    assert!(game.players[1].side.is_none());
    assert!(game.players[1].color.is_none());
    let neutral = game.neutral_owner();
    assert!(game.players[neutral].side.is_none());
    assert!(!game.players[neutral].playable_side);
    assert_eq!(game.players[neutral].color, Some(0xffff_ffff));
    let named = neutral + 1;
    assert_eq!(game.players[named].side.as_deref(), Some("Civilian"));
    assert!(!game.players[named].playable_side);
    assert_eq!(game.players[named].color, Some(0xff11_2233));
}

#[test]
fn player_identity_unconfigured_is_explicit_and_configured_missing_inputs_are_errors() {
    let mut game = tests::game();
    assert!(game.players[0].side.is_none());
    assert!(game.players[0].color.is_none());
    let mut source = rules();
    source
        .factions
        .retain(|template| template.faction != Faction::Gla);
    assert!(game.configure_progression(source).is_err());
    assert!(game.players[0].side.is_none());
    let mut source = rules();
    source.multiplayer_colors.clear();
    assert!(game.configure_progression(source).is_err());
    game.configure_progression(rules()).unwrap();
    let mut slots = game.slots.clone();
    slots[0].color = Some(8);
    assert!(game.reset_slots(slots).is_err());
}
