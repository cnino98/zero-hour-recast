//! Original hulk modules through factory placement, Lifetime and scheduled `SlowDeath`.
use crate::{playtest::MAP_NAME, terrain::Terrain, world_data};
use generals_assets::{
    map::{MapData, MapObject, MapValue},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{
    Skirmish, WeaponFxEvent,
    map_scripts::{Action, Condition, Script},
};
use std::collections::BTreeMap;

fn placed(template: &str, height: f32) -> (Skirmish, usize) {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut map = MapData::parse(&assets.read(MAP_NAME).unwrap().bytes).unwrap();
    map.objects.push(MapObject {
        template: template.into(),
        position: [400.0, 400.0, height],
        angle: 0.0,
        flags: 0,
        properties: BTreeMap::from([
            (
                "originalOwner".into(),
                MapValue::Ascii("teamPlyrCivilian".into()),
            ),
            (
                "objectName".into(),
                MapValue::Ascii("OriginalHulkLifecycle".into()),
            ),
        ]),
    });
    let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
    let settings = world_data::camera_settings(&assets, root).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let mut animations =
        crate::model_animation::load(&assets, &mut data.models, &data.types).unwrap();
    animations.bind_logical_bones(&mut data.types);
    crate::skirmish_data::structure_death::bind_bones(&mut data.types, &data.models, &animations);
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations).unwrap();
    let mut game = data.into_simulation(movement, layout).unwrap();
    terrain
        .configure_projectiles(&mut game, projectiles.partition_cell_size)
        .unwrap();
    let id = game.named_map_object("OriginalHulkLifecycle").unwrap();
    assert_eq!(game.catalog[game.entities[id].type_index].name, template);
    (game, id)
}
fn kill_named(game: &mut Skirmish) {
    game.install_map_scripts(vec![Script {
        owner: "player0".into(),
        name: "KillOriginalPaladin".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::NamedKill("OriginalHulkLifecycle".into())],
        false_actions: Vec::new(),
    }]);
}

#[ignore = "requires original game installation"]
#[test]
fn original_paladin_hulk_airborne_placement_proves_lifetime_then_sink_and_retirement() {
    // Synthetic high map placement of the unchanged original template isolates
    // Lifetime precedence over its equally real KillWhenRestingOnGround callback.
    let (mut game, id) = placed("DeadPaladinHulk", 4096.0);
    let born = game.tick;
    assert!(game.entities[id].physical_present());
    assert!(!game.entities[id].corpse_present());
    assert!(!game.entities[id].disabled_held());
    let mut death = None;
    for _ in 0..49 {
        game.step();
        if game.entities[id].corpse_present() {
            death = Some(game.tick);
            break;
        }
        assert!(game.tick - born < 48);
    }
    let death = death.expect("actual original Lifetime must kill its inactive Body");
    assert!((45..=48).contains(&(death - born)));
    assert!(game.object_position(id)[2] > game.ground_height(game.entities[id].position));
    while game.tick < death + 44 {
        game.step();
        assert!(!game.entities[id].disabled_held());
        assert!(game.entities[id].exists());
    }
    game.step();
    assert_eq!(game.tick, death + 45);
    assert!(game.entities[id].disabled_held());
    let before = game.object_position(id)[2];
    let rate = game.catalog[game.entities[id].type_index]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .slow[0]
        .sink_rate;
    assert_eq!(rate.to_bits(), (2.0_f32 * (1.0 / 30.0)).to_bits());
    game.step();
    assert_eq!(
        game.object_position(id)[2].to_bits(),
        (before - rate).to_bits()
    );
    while game.tick < death + 239 {
        game.step();
        assert!(game.entities[id].exists() && game.entities[id].has_drawable());
    }
    game.step();
    assert_eq!(game.tick, death + 240);
    assert!(!game.entities[id].exists());
    assert!(!game.entities[id].has_drawable());
    assert_eq!(
        game.presentation
            .weapon_fx
            .iter()
            .filter(
                |event| matches!(event, WeaponFxEvent::DrawableDestroyed(object) if *object == id)
            )
            .count(),
        1
    );
}

#[ignore = "requires original game installation"]
#[test]
fn original_buggy_lifetime_preserves_initial_fx_final_fx_and_twelve_real_debris() {
    let (mut game, id) = placed("GLARocketBuggyFullDebris", 0.0);
    let born = game.tick;
    assert!(game.entities[id].alive());
    game.step();
    let death = game.tick;
    assert_eq!(death - born, 1); // Native Lifetime 0..0 becomes sleep1.
    assert!(game.entities[id].corpse_present());
    assert_eq!(game.presentation.object_fx_events.iter().filter(|event| event.object == id && event.effect == "FX_RocketBuggyAirDeathGroundPart").count(), 1);
    let mut final_count = 0;
    let mut destroyed = None;
    for _ in 0..61 {
        game.step();
        assert!(
            !game
                .presentation
                .object_fx_events
                .iter()
                .any(|event| event.object == id
                    && event.effect == "FX_RocketBuggyAirDeathGroundPart")
        );
        final_count += game
            .presentation
            .object_fx_events
            .iter()
            .filter(|event| event.object == id && event.effect == "FX_RocketBuggyAirDeathAirPart")
            .count();
        if !game.entities[id].exists() {
            destroyed = Some(game.tick);
            break;
        }
    }
    let destroyed = destroyed.expect("original scheduled FINAL must retire parent");
    assert!((15..=60).contains(&(destroyed - death)));
    assert_eq!(final_count, 1);
    let children: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter_map(|(child, actor)| (actor.producer() == Some(id)).then_some(child))
        .collect();
    assert_eq!(children.len(), 12);
    assert_eq!(
        children
            .iter()
            .filter(|&&child| game.entities[child].physical_model() == Some("UVRockBug_D2"))
            .count(),
        4
    );
    assert_eq!(
        children
            .iter()
            .filter(|&&child| game.entities[child].physical_model() == Some("UVRockBug_D3"))
            .count(),
        8
    );
    assert!(children.iter().all(|&child| game.entities[child].exists()
        && game.entities[child].physical_present()
        && game.catalog[game.entities[child].type_index].name == "GenericDebris"));
    let final_fx = game.presentation.weapon_fx.iter().position(|event| matches!(event, WeaponFxEvent::ObjectFx(index) if game.presentation.object_fx_events[*index].object == id && game.presentation.object_fx_events[*index].effect == "FX_RocketBuggyAirDeathAirPart")).unwrap();
    let deletion = game
        .presentation
        .weapon_fx
        .iter()
        .position(
            |event| matches!(event, WeaponFxEvent::DrawableDestroyed(object) if *object == id),
        )
        .unwrap();
    assert!(final_fx < deletion);
}

#[ignore = "requires original game installation"]
#[test]
fn original_paladin_named_kill_creates_actual_hulk_then_retires_it() {
    let (mut game, parent) = placed("AmericaTankPaladin", 0.0);
    assert!(game.entities[parent].alive());
    kill_named(&mut game);
    let maximum = game.catalog[game.entities[parent].type_index]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .slow
        .iter()
        .map(|rule| rule.destruction_delay[0] + rule.destruction_delay[1])
        .max()
        .unwrap();
    let mut hulk = None;
    for _ in 0..maximum + 2 {
        game.step();
        hulk = game.entities.iter().enumerate().find_map(|(id, actor)| {
            (actor.producer() == Some(parent)
                && game.catalog[actor.type_index].name == "DeadPaladinHulk")
                .then_some(id)
        });
        if hulk.is_some() {
            break;
        }
    }
    let hulk = hulk.expect("original Paladin's actual death OCL creates its authored hulk");
    let born = game.tick;
    assert!(game.entities[hulk].exists() && game.entities[hulk].has_drawable());
    assert!(game.entities[hulk].physical_present());
    let mut active = false;
    for _ in 0..48 + 240 + 2 {
        game.step();
        active |= game.entities[hulk].corpse_present();
        if !game.entities[hulk].exists() {
            break;
        }
    }
    assert!(active, "actual hulk SlowDeath must run before deletion");
    assert!(!game.entities[hulk].exists() && !game.entities[hulk].has_drawable());
    assert!(game.tick - born <= 48 + 240 + 1);
}
