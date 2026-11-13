use super::{Faction, Kind, WeaponFxEvent, tests};

#[test]
fn normal_map_factory_is_logic_valid_and_failed_reset_restores_external_creation_context() {
    let mut game = tests::game();
    let creations: Vec<_> = game
        .presentation
        .weapon_fx
        .iter()
        .filter_map(|event| match event {
            WeaponFxEvent::DrawableCreated { validate_logic, .. } => Some(*validate_logic),
            _ => None,
        })
        .collect();
    assert!(creations.len() >= 4);
    assert_eq!(&creations[..4], &[true; 4]);
    assert!(!game.map_loading);
    assert!(game.reset(Faction::Usa, Faction::Usa).is_err());
    assert!(!game.map_loading);
    let kind = game.catalog.len();
    game.catalog
        .push(tests::fixture_type(Faction::Usa, Kind::Infantry));
    let object = game.spawn(kind, 0, [355.0, 255.0], true);
    assert!(
        game.presentation
            .weapon_fx
            .iter()
            .any(|event| matches!(event,
        WeaponFxEvent::DrawableCreated { object: id, type_index, validate_logic: false }
            if *id == object && *type_index == kind))
    );
}

#[test]
fn drawable_replacement_prepends_old_object_without_changing_object_identity() {
    let mut game = tests::game();
    let kind = game.catalog.len();
    game.catalog
        .push(tests::fixture_type(Faction::Usa, Kind::Infantry));
    let older = game.spawn(kind, 0, [355.0, 255.0], true);
    let newer = game.spawn(kind, 0, [365.0, 255.0], true);
    assert!(game.entities[older].drawable_birth() < game.entities[newer].drawable_birth());
    let count = game.entities.len();
    game.renew_drawable_birth(older);
    assert!(game.entities[older].drawable_birth() > game.entities[newer].drawable_birth());
    let latest = game.spawn(kind, 0, [375.0, 255.0], true);
    assert!(game.entities[latest].drawable_birth() > game.entities[older].drawable_birth());
    assert_eq!(latest, count);
    assert_eq!(
        game.entities[older].position.map(f32::to_bits),
        [355.0_f32, 255.0].map(f32::to_bits)
    );
}
