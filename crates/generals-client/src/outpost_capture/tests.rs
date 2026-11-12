#[ignore = "requires original game installation"]
#[test]
fn paid_native_outpost_fixture_keeps_two_riders_and_a_visible_live_encounter() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = super::fixture::prepare(&mut battle.simulation).unwrap();
    let game = &battle.simulation;
    assert!(game.entities[setup.outpost].stealthed());
    assert_eq!(game.passengers(setup.outpost).len(), 2);
    assert!(setup.enemies.iter().all(|&id| game.entities[id].alive()));
    assert!(game.logically_visible(0, setup.enemies[0]));
    let source = game.object_position(setup.outpost);
    let target = game.object_position(setup.enemies[0]);
    let squares = std::array::from_fn::<_, 3, _>(|i| {
        let delta = source[i] - target[i];
        delta * delta
    });
    assert!((squares[0] + squares[1] + squares[2]).sqrt() <= 250.0);
}
