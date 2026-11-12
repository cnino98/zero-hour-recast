use super::*;
use generals_sim::skirmish::{Faction, Kind};

#[ignore = "requires original game installation"]
#[test]
fn installed_fake_buildings_use_original_shadow_texture_dimensions_without_horde_fade() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let mut data = load(&assets, &battle.simulation.catalog).unwrap();
    for (name, size) in [
        ("FakeGLACommandCenter", [165.0, 145.0]),
        ("FakeGLABarracks", [120.0, 110.0]),
        ("FakeGLASupplyStash", [105.0, 105.0]),
        ("FakeGLAArmsDealer", [127.0, 144.0]),
        ("FakeGLABlackMarket", [108.0, 96.0]),
    ] {
        let index = crate::strategy_verification::template(&battle.simulation, name).unwrap();
        let template = data.templates[index].as_ref().unwrap();
        assert!(template.fake && !template.chemical_suit);
        assert_eq!(template.size, Vec2::from_array(size));
        assert_eq!(template.offset, Vec2::ZERO);
    }
    assert_eq!(TEXTURES[5], "fake_supply");
    battle.simulation.reset(Faction::Gla, Faction::Usa).unwrap();
    let worker = crate::skirmish_verification::friendly_entity(
        &battle.simulation,
        generals_sim::skirmish::Kind::Builder,
        0,
    )
    .unwrap();
    crate::fake_building_verification::switch(&mut battle.simulation, worker, true).unwrap();
    let kind =
        crate::strategy_verification::template(&battle.simulation, "FakeGLABarracks").unwrap();
    let building =
        crate::skirmish_verification::construct_type(&mut battle.simulation, worker, kind, 0)
            .unwrap();
    data.advance(&battle, 0.0);
    assert_eq!(data.objects[building].opacity.to_bits(), 1.0_f32.to_bits());
    battle.observer_player = Some(1);
    // A currently active local player still owns the camera's information scope.
    assert!(fake_visible(&battle, 0));
    assert!(!fake_visible(&battle, 1));
}

#[ignore = "requires original game installation"]
#[test]
fn fake_markings_follow_allies_neutral_and_observed_player_relationships() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    battle.simulation.reset(Faction::Gla, Faction::Usa).unwrap();
    assert!(fake_visible(&battle, 0));
    assert!(!fake_visible(&battle, 1));
    assert!(fake_visible(&battle, battle.simulation.neutral_owner()));
    let base =
        crate::skirmish_verification::friendly_entity(&battle.simulation, Kind::CommandCenter, 0)
            .unwrap();
    battle.simulation.sell(0, base).unwrap();
    crate::skirmish_verification::advance_until(
        &mut battle.simulation,
        900,
        "local sale enables observer scope",
        |game| !game.player_active(0),
    )
    .unwrap();
    assert!(fake_visible(&battle, 0) && fake_visible(&battle, 1));
    battle.observer_player = Some(1);
    assert!(fake_visible(&battle, 1));
    assert!(!fake_visible(&battle, 0));
    let mut slots = battle.simulation.slots.clone();
    for slot in &mut slots {
        slot.team = Some(0);
    }
    battle.simulation.reset_slots(slots).unwrap();
    assert!(fake_visible(&battle, 0) && fake_visible(&battle, 1));
}
