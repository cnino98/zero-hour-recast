use super::Grant;
use crate::skirmish::{
    Faction, Kind, Skirmish,
    tests::{self, register_behavior},
};

fn template(game: &mut Skirmish, player: bool) -> usize {
    let mut unit = tests::fixture_type(Faction::Gla, Kind::Barracks);
    unit.build_seconds = 1.0 / 30.0;
    unit.behavior.radar_grants = vec![
        Grant {
            name: None,
            player: false,
            on_created: true,
            academy_radar: false,
        },
        Grant {
            name: Some("GrantedRadar".into()),
            player,
            on_created: true,
            academy_radar: true,
        },
    ];
    register_behavior(&mut unit, "GrantUpgradeCreate", 0x8);
    register_behavior(&mut unit, "GrantUpgradeCreate", 0x8);
    unit.maintenance
        .experience_upgrades
        .push(crate::skirmish::ExperienceUpgrade {
            trigger: "GrantedRadar".into(),
            scalar: 1.0,
        });
    register_behavior(&mut unit, "ExperienceScalarUpgrade", 0x80);
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["GrantedRadar".into()];
    let kind = game.catalog.len();
    game.catalog.push(unit);
    kind
}

#[test]
fn authored_null_grant_keeps_the_next_create_ordinal_and_records_granted_radar() {
    let mut game = tests::game();
    let kind = template(&mut game, false);
    let id = game.spawn(kind, 0, [350.0, 250.0], true);
    assert_eq!(game.catalog[kind].behavior.radar_grants.len(), 2);
    assert!(game.object_has_upgrade(id, "GrantedRadar"));
    assert!(!game.has_upgrade(0, "GrantedRadar"));
    assert!(game.players[0].academy.researched_radar);
    assert_eq!(
        game.entities[id].experience_scalar().to_bits(),
        2.0_f32.to_bits()
    );
}

#[test]
fn paid_construction_grants_at_build_only_once_and_never_records_create_academy() {
    let mut game = tests::game();
    let kind = template(&mut game, true);
    let builder = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    let position = game.placement_near(kind, [350.0, 250.0]).unwrap();
    game.step();
    let id = game.build(builder, kind, position).unwrap();
    assert!(!game.has_upgrade(0, "GrantedRadar"));
    assert!(!game.players[0].academy.researched_radar);
    for _ in 0..600 {
        game.step();
        if game.entities[id].complete() {
            break;
        }
    }
    assert!(game.entities[id].complete());
    assert!(game.has_upgrade(0, "GrantedRadar"));
    assert!(!game.players[0].academy.researched_radar);
    assert_eq!(
        game.entities[id].experience_scalar().to_bits(),
        2.0_f32.to_bits()
    );
    // removeUpgrade resets the matching object mux even if the trigger is
    // still present in the player mask. A repeated build callback must not grant it again.
    game.remove_native_object_upgrade(id, "GrantedRadar");
    assert!(!game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 0));
    game.native_build_complete(id);
    assert_eq!(
        game.entities[id].experience_scalar().to_bits(),
        2.0_f32.to_bits()
    );
    assert!(!game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 0));
}
