use super::Rules;
use crate::skirmish::{Faction, Kind, tests};

#[test]
fn radar_ping_observes_native_sustain_shrink_and_interval_edges() {
    let mut game = tests::game();
    let mut marker = tests::fixture_type(Faction::Gla, Kind::Effect);
    marker.sight.clearing_range = 150.0;
    marker.behavior.dynamic_shroud.push(Rules {
        final_vision: 0.0,
        change_interval: 2,
        grow_interval: 0,
        shrink_delay: 225,
        shrink_time: 75,
        grow_delay: 0,
        grow_time: 0,
        grid_decal: None,
    });
    tests::register_update(&mut marker, "DynamicShroudClearingRangeUpdate");
    let template = game.catalog.len();
    game.catalog.push(marker);
    let id = game.spawn(template, 0, [400.0, 400.0], true);
    for _ in 0..225 {
        game.update_dynamic_shroud(id, 0);
    }
    assert_eq!(
        game.entities[id]
            .shroud_range(&game.catalog[template])
            .to_bits(),
        150.0_f32.to_bits()
    );
    game.update_dynamic_shroud(id, 0);
    assert_eq!(
        game.entities[id]
            .shroud_range(&game.catalog[template])
            .to_bits(),
        148.0_f32.to_bits()
    );
    game.update_dynamic_shroud(id, 0);
    game.update_dynamic_shroud(id, 0);
    assert_eq!(
        game.entities[id]
            .shroud_range(&game.catalog[template])
            .to_bits(),
        148.0_f32.to_bits()
    );
    game.update_dynamic_shroud(id, 0);
    assert_eq!(
        game.entities[id]
            .shroud_range(&game.catalog[template])
            .to_bits(),
        142.0_f32.to_bits()
    );
    for _ in 0..75 {
        game.update_dynamic_shroud(id, 0);
    }
    assert_eq!(
        game.entities[id]
            .shroud_range(&game.catalog[template])
            .to_bits(),
        0.0_f32.to_bits()
    );
    assert!(game.dynamic_shroud_grid_slots(id, 0).is_none());
}
