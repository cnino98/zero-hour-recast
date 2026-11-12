//! Device-free command-path regression; visible meshes remain a native capture gate.
mod camera;
use super::*;

fn original_game() -> Skirmish {
    let root = &generals_assets::game_root().unwrap();
    crate::playtest::map_fixture::Fixture::load(root, map_name())
        .unwrap()
        .into_simulation()
        .unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn installed_paid_bridge_area_damage_preserves_original_death_and_radar_timing() {
    let mut game = original_game();
    let setup = setup::prepare(&mut game).unwrap();
    let mut check = Check {
        setup,
        phase: 1,
        started: None,
        water: None,
        destroyed: None,
        radar: Vec::new(),
        tail_seen: false,
        emitter_id: None,
        waiting: None,
    };
    let mut time = Time::<Virtual>::default();
    for _ in 0..1800 {
        advance_scenario(&mut check, &mut game, &mut time).unwrap();
        if check.phase == 3 {
            break;
        }
        game.step();
    }
    assert_eq!(
        check.phase, 3,
        "actual paid Crusader area attack must create WaterSpray"
    );
    let (water, _) = check.water.unwrap();
    assert_eq!(game.entities[water].producer(), Some(check.setup.hydrant));
    let rules = &game.catalog[game.entities[water].type_index];
    assert!(rules.live_physics.is_none());
    assert_eq!(rules.behavior.lifetimes, [[150, 150]]);
    crate::skirmish_verification::advance_until(
        &mut game,
        151,
        "original WaterSpray LifetimeUpdate",
        |game| !game.entities[water].exists(),
    )
    .unwrap();

    // Only presentation waits are omitted: the ordinary commands and actual
    // damage progression use the same functions as the native capture.
    check.phase = 6;
    for _ in 0..18000 {
        advance_scenario(&mut check, &mut game, &mut time).unwrap();
        if check.phase == 11 {
            break;
        }
        game.step();
    }
    assert_eq!(
        check.phase, 11,
        "original paid artillery must destroy the original 2000-health bridge"
    );
    assert!(!game.entities[check.setup.bridge].alive());
    assert!(!game.radar_bridge_working(check.setup.layer));
    assert!(!game.take_radar_terrain_refresh());
    for _ in 0..91 {
        game.step();
    }
    assert!(game.take_radar_terrain_refresh());
    check.phase = 12;
    advance_scenario(&mut check, &mut game, &mut time).unwrap();
    assert_eq!(check.phase, 13);
}
