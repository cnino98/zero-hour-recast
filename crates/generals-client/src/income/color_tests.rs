//! Actual Cairo map players extend beyond the occupied lobby color vector.
use super::owner_color;
use generals_sim::skirmish::Faction;

#[ignore = "requires original game installation"]
#[test]
fn installed_cairo_income_uses_named_player_color_and_survives_restart() {
    let root = &generals_assets::game_root().unwrap();
    let fixture = crate::playtest::map_fixture::Fixture::load(
        root,
        "Maps\\Cairo Commandos\\Cairo Commandos.map",
    )
    .unwrap();
    let preferred: Vec<_> = fixture
        .data
        .progression
        .factions
        .iter()
        .map(|template| (template.faction, template.preferred_color))
        .collect();
    let selected = fixture.data.progression.multiplayer_colors[7];
    let mut game = fixture.into_simulation().unwrap();
    game.reset_sandbox(Faction::Usa).unwrap();
    assert_eq!(game.player_colors.len(), 3);
    assert!(game.players.len() > 4);
    let mut slots = game.slots.clone();
    slots[0].color = Some(7);
    game.reset_slots(slots).unwrap();
    assert_eq!(game.players[0].color, Some(selected | 0xff00_0000));
    assert_eq!(owner_color(&game, 0).map(f32::to_bits), channels(selected));
    let named_color = preferred
        .iter()
        .find(|(faction, _)| *faction == game.players[4].faction)
        .unwrap()
        .1;
    assert_eq!(game.players[4].color, Some(named_color | 0xff00_0000));
    assert_eq!(
        owner_color(&game, 4).map(f32::to_bits),
        channels(named_color)
    );
    let neutral = game.player_colors.len();
    assert_eq!(game.players[neutral].color, Some(0xffff_ffff));
    assert_eq!(
        owner_color(&game, neutral).map(f32::to_bits),
        [1.0_f32.to_bits(); 3]
    );

    let deadline = game
        .entities
        .iter()
        .filter(|actor| actor.owner == 4)
        .flat_map(|actor| &game.catalog[actor.type_index].maintenance.deposits)
        .map(|rules| rules.interval)
        .max()
        .unwrap();
    let mut observed = false;
    for _ in 0..deadline + 2 {
        game.step();
        if let Some(event) = game
            .presentation
            .deposit_events
            .iter()
            .find(|event| event.owner == 4)
        {
            assert_eq!(game.entities[event.object].owner, 4);
            assert!(event.amount > 0);
            assert_eq!(
                owner_color(&game, event.owner).map(f32::to_bits),
                channels(named_color)
            );
            observed = true;
            break;
        }
    }
    assert!(
        observed,
        "original named Cairo player must produce an actual income callback"
    );
    game.reset_sandbox(Faction::Usa).unwrap();
    assert!(game.presentation.deposit_events.is_empty());
    assert_eq!(
        owner_color(&game, 4).map(f32::to_bits),
        channels(named_color)
    );
}

fn channels(packed: u32) -> [u32; 3] {
    let [_, red, green, blue] = packed.to_be_bytes();
    [red, green, blue].map(|byte| (f32::from(byte) / 255.0).to_bits())
}
