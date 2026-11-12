//! Installed capture staging, using the exact production map/resource loader.
mod bridge_camera;
use super::{ScreenCapture, load_map};
use bevy::prelude::*;
use generals_sim::skirmish::Kind;

fn original_world(bridge: bool) -> App {
    let root = &generals_assets::game_root().unwrap();
    let mut app = App::new();
    app.init_resource::<Time<Virtual>>();
    app.init_resource::<Time>();
    app.insert_resource(Time::<Fixed>::from_hz(30.0));
    app.insert_resource(crate::model_animation::Clock::new());
    load_map(
        app.world_mut(),
        root,
        &ScreenCapture::World(bridge),
        crate::world_capture::map_name(bridge),
        false,
    )
    .unwrap();
    app
}

fn finish_preparation(app: &mut App, battle: super::Match) -> super::Match {
    use bevy::ecs::system::RunSystemOnce;
    app.insert_resource(battle);
    let bounds = app
        .world()
        .resource::<crate::terrain::Terrain>()
        .tree_bounds();
    if !app.world().contains_resource::<crate::tree_buffer::Trees>() {
        app.insert_resource(crate::tree_buffer::Trees::new(
            bounds,
            crate::tree_buffer::Lighting {
                ambient: [1.0; 3],
                colors: [[0.0; 3]; 3],
                directions: [[0.0, 0.0, -1.0]; 3],
            },
        ));
    }
    let trees: Vec<_> = app
        .world()
        .resource::<crate::playtest_view::WorldObjects>()
        .0
        .iter()
        .filter_map(|object| object.tree_data.as_ref().map(|(id, _)| *id))
        .collect();
    assert!(!trees.is_empty());
    let tick = app.world().resource::<super::Match>().simulation.tick;
    assert_eq!(
        tick, 0,
        "preparation must not consume startup tree notifications"
    );
    app.world_mut()
        .run_system_once(crate::weapon_fx::record)
        .unwrap();
    {
        let game = app.world().resource::<super::Match>();
        assert_eq!(game.simulation.tick, tick);
        let buffer = app.world().resource::<crate::tree_buffer::Trees>();
        assert!(
            trees
                .iter()
                .all(|&id| crate::tree_buffer::verification::intact(buffer, id))
        );
    }
    let mut ready = false;
    for _ in 0..18000 {
        ready = crate::world_capture::test_preparation(app.world_mut()).unwrap();
        if ready {
            break;
        }
        app.world_mut()
            .run_system_once(crate::weapon_fx::record)
            .unwrap();
        app.world_mut()
            .resource_mut::<super::Match>()
            .simulation
            .step();
        app.world_mut()
            .run_system_once(crate::weapon_fx::record)
            .unwrap();
        let timestep = app.world().resource::<Time<Fixed>>().timestep();
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(timestep);
        app.world_mut()
            .run_system_once(crate::model_animation::update_clock)
            .unwrap();
        app.world_mut()
            .run_system_once(crate::tree_buffer::clear)
            .unwrap();
    }
    assert!(
        ready,
        "ordinary-frame paid world preparation did not complete"
    );
    assert!(
        app.world()
            .resource::<crate::model_animation::Clock>()
            .committed_milliseconds()
            > 0
    );
    app.world_mut().remove_resource::<super::Match>().unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn installed_rain_capture_paid_tank_reaches_original_shore_staging() {
    let mut app = original_world(false);
    let mut battle = app.world_mut().remove_resource::<super::Match>().unwrap();
    crate::world_capture::prepare(&mut app, &mut battle, false).unwrap();
    battle = finish_preparation(&mut app, battle);
    assert_eq!(battle.selected.len(), 1);
    let tank = battle.selected[0];
    let route =
        generals_sim::skirmish::diagnostics::ground_route_result(&battle.simulation, tank).unwrap();
    let original_shore = app
        .world()
        .resource::<crate::world_capture::Scene>()
        .shores
        .iter()
        .find(|point| [point[0], point[1]].map(f32::to_bits) == route.requested.map(f32::to_bits))
        .expect("the accepted goal is an original shoreline sample, not an adjusted replacement");
    let actual = battle.simulation.entities[tank].position;
    let dx = actual[0] - original_shore[0];
    let dy = actual[1] - original_shore[1];
    let horizontal = dx * dx;
    let vertical = dy * dy;
    assert!(horizontal + vertical < 25.0 * 25.0);
    assert_eq!(
        battle.simulation.catalog[battle.simulation.entities[battle.selected[0]].type_index].name,
        "AmericaTankCrusader"
    );
    assert_original_tree_site(&app, &battle);
    // Real restart emits a new source-ordered initial tree timeline. Retain the
    // previous client buffer so its actual generation reset is also exercised.
    battle.restart();
    crate::world_capture::prepare(&mut app, &mut battle, false).unwrap();
    battle = finish_preparation(&mut app, battle);
    assert_original_tree_site(&app, &battle);
}

fn assert_original_tree_site(app: &App, battle: &super::Match) {
    let game = &battle.simulation;
    let kind = game.type_for(0, Kind::PowerPlant).unwrap();
    let trees = app.world().resource::<crate::tree_buffer::Trees>();
    let mut counts = [0_usize; 5];
    let mut examples = Vec::new();
    let mut legal_examples = Vec::new();
    let mut shrouded_count = 0;
    for object in &app
        .world()
        .resource::<crate::playtest_view::WorldObjects>()
        .0
    {
        let Some((id, data)) = &object.tree_data else {
            continue;
        };
        let intact = crate::tree_buffer::verification::intact(trees, *id);
        let legal = game.can_place(kind, [object.position[0], object.position[1]]);
        let fogged = crate::tree_buffer::verification::all_status(
            game,
            0,
            object.position,
            &generals_sim::skirmish::visibility::Status::Fogged,
        );
        for (count, accepted) in
            counts
                .iter_mut()
                .zip([true, intact, legal, fogged, intact && legal && fogged])
        {
            *count += usize::from(accepted);
        }
        shrouded_count += usize::from(crate::tree_buffer::verification::all_status(
            game,
            0,
            object.position,
            &generals_sim::skirmish::visibility::Status::Shrouded,
        ));
        let center = match game.visibility_cell(0, [object.position[0], object.position[1]]) {
            generals_sim::skirmish::visibility::Status::Clear => "Clear",
            generals_sim::skirmish::visibility::Status::Partial => "Partial",
            generals_sim::skirmish::visibility::Status::Fogged => "Fogged",
            generals_sim::skirmish::visibility::Status::Shrouded => "Shrouded",
        };
        let sample = (
            *id,
            &data.model,
            object.position,
            intact,
            legal,
            fogged,
            center,
        );
        if examples.len() < 8 {
            examples.push(sample);
        }
        if legal && legal_examples.len() < 8 {
            legal_examples.push(sample);
        }
    }
    assert_eq!(
        shrouded_count, 0,
        "original UseShroud=No starts every unseen tree fogged"
    );
    assert!(
        counts[4] > 0,
        "original rainy setup lost every intact legal fogged tree site: generation={} tick={} counts[all,intact,legal,fogged,intersection]={counts:?} examples[id,model,position,intact,legal,four_fogged,center_status]={examples:?} legal_examples={legal_examples:?}",
        battle.generation,
        game.tick
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_bridge_capture_destination_is_reached_on_ground_after_paid_crossing() {
    let mut app = original_world(true);
    let mut battle = app.world_mut().remove_resource::<super::Match>().unwrap();
    assert!(
        battle.started,
        "the production World capture starts USA before selecting a feature"
    );
    let builder =
        crate::skirmish_verification::friendly_entity(&battle.simulation, Kind::Builder, 0)
            .unwrap();
    let home = battle.simulation.entities[builder].position;
    let distance = |point: [f32; 3]| {
        let dx = point[0] - home[0];
        let dy = point[1] - home[1];
        let horizontal = dx * dx;
        let vertical = dy * dy;
        horizontal + vertical
    };
    let scene = app.world().resource::<crate::world_capture::Scene>();
    assert_original_golden_bridges(scene);
    let &(layer, ends) = scene
        .bridge_staging
        .iter()
        .min_by(|(_, a), (_, b)| {
            distance(a[0])
                .min(distance(a[1]))
                .total_cmp(&distance(b[0]).min(distance(b[1])))
        })
        .unwrap();
    let (_, native_ends) = scene
        .bridges
        .iter()
        .find(|(candidate, _)| *candidate == layer)
        .unwrap();
    assert_ground_staging(&battle.simulation, *native_ends, ends);
    let (departure, destination) = if distance(ends[0]) < distance(ends[1]) {
        (ends[0], ends[1])
    } else {
        (ends[1], ends[0])
    };
    crate::world_capture::prepare(&mut app, &mut battle, true).unwrap();
    battle = finish_preparation(&mut app, battle);
    let tank = battle.selected[0];
    let game = &mut battle.simulation;
    assert_eq!(
        game.catalog[game.entities[tank].type_index].name,
        "AmericaTankCrusader"
    );
    let scouting = generals_sim::skirmish::diagnostics::ground_route_result(game, tank)
        .expect("paid scouting must complete an original near-bank route");
    assert_eq!(
        scouting.requested.map(f32::to_bits),
        [departure[0], departure[1]].map(f32::to_bits),
        "scouting must retain the original near-bank request"
    );
    assert_eq!(
        game.entities[tank].layer, 1,
        "the paid crossing starts on the near bank"
    );
    game.move_to_world(tank, destination).unwrap();
    let adjusted = game.entities[tank].ai_goal_position();
    assert_eq!(
        [adjusted[0], adjusted[1]].map(f32::to_bits),
        [destination[0], destination[1]].map(f32::to_bits)
    );
    let mut crossed = false;
    let mut arrived = false;
    for _ in 0..18000 {
        game.step();
        crossed |= game.entities[tank].layer == layer;
        arrived = {
            let actor = &game.entities[tank];
            let dx = actor.position[0] - destination[0];
            let dy = actor.position[1] - destination[1];
            let horizontal = dx * dx;
            let vertical = dy * dy;
            actor.layer == 1 && horizontal + vertical < 30.0 * 30.0
        };
        if arrived {
            break;
        }
    }
    let route = generals_sim::skirmish::diagnostics::ground_route_result(game, tank)
        .map(|route| (route.requested, route.endpoint));
    assert!(
        crossed && arrived,
        "crossed={crossed} arrived={arrived}; requested={destination:?} layer={} completed_route={route:?}; {}",
        game.entities[tank].layer,
        generals_sim::skirmish::diagnostics::movement_details(game, tank)
    );
    bridge_camera::assert_far_bank(&mut app, battle, tank);
}

fn assert_original_golden_bridges(scene: &crate::world_capture::Scene) {
    // All three original KandaharBridge 16/32 endpoint pairs, sampled by the
    // source's outward 0.7-cell endpoint helper. Nearest depends on player home;
    // the first authored bridge is not necessarily the chosen bridge.
    let mut expected = [
        [[1835.0_f32, 1405.0], [1685.0, 1595.0]],
        [[2715.0, 1805.0], [2515.0, 1665.0]],
        [[2635.0, 1155.0], [2955.0, 1555.0]],
    ]
    .map(|ends| ends.map(|point| point.map(f32::to_bits)));
    let mut actual: Vec<_> = scene
        .bridges
        .iter()
        .map(|(_, ends)| ends.map(|point| [point[0], point[1]].map(f32::to_bits)))
        .collect();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(
        actual, expected,
        "Golden Oasis original bridge endpoint inventory changed"
    );
}

fn assert_ground_staging(
    game: &generals_sim::skirmish::Skirmish,
    native_ends: [[f32; 3]; 2],
    ends: [[f32; 3]; 2],
) {
    let middle =
        std::array::from_fn::<_, 3, _>(|axis| (native_ends[0][axis] + native_ends[1][axis]) * 0.5);
    assert!(
        game.pick_bridge(
            [middle[0], middle[1], 10_000.0],
            [middle[0], middle[1], -10_000.0]
        )
        .is_some()
    );
    for end in ends {
        for dy in -30_i16..=30 {
            for dx in -30_i16..=30 {
                let x = end[0] + f32::from(dx);
                let y = end[1] + f32::from(dy);
                assert!(
                    game.pick_bridge([x, y, 10_000.0], [x, y, -10_000.0])
                        .is_none(),
                    "staging arrival neighborhood overlaps original deck"
                );
            }
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_skirmish_settings_clear_passive_shroud_before_original_tree_capture() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let file =
        generals_assets::content::scan(assets.read("data/ini/multiplayer.ini").unwrap()).unwrap();
    let settings =
        crate::skirmish_data::definition(std::slice::from_ref(&file), "MultiplayerSettings", "")
            .unwrap();
    assert_eq!(
        crate::skirmish_data::text(&settings.lines, 0, "UseShroud").unwrap(),
        "No"
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_rain_original_tree_scout_reaches_native_dock_adjusted_goals() {
    let mut app = original_world(false);
    let mut battle = app.world_mut().remove_resource::<super::Match>().unwrap();
    crate::world_capture::prepare(&mut app, &mut battle, false).unwrap();
    let mut battle = finish_preparation(&mut app, battle);
    let tank = battle.selected[0];
    let game = &mut battle.simulation;
    let builder = crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0).unwrap();
    // Exact original r9-selected tree; its presence/placement is validated,
    // not an invented replacement scouting position.
    let objects = app.world().resource::<crate::playtest_view::WorldObjects>();
    let tree = objects
        .0
        .iter()
        .find(|object| {
            object.tree_data.is_some()
                && (object.position[0] - 931.802_2).abs() < 0.01
                && (object.position[1] - 2_221.337_4).abs() < 0.01
        })
        .unwrap();
    let tree_position = tree.position;
    let kind = game.type_for(0, Kind::PowerPlant).unwrap();
    assert!(game.can_place(kind, [tree_position[0], tree_position[1]]));
    assert!(crate::tree_buffer::verification::all_status(
        game,
        0,
        tree_position,
        &generals_sim::skirmish::visibility::Status::Fogged
    ));
    let home = Vec2::from_array(game.entities[builder].position);
    let direction = (home - Vec2::new(tree_position[0], tree_position[1])).normalize();
    let point = Vec2::new(tree_position[0], tree_position[1]) + direction * 90.0;
    let scout = [point.x, point.y, game.ground_height(point.to_array())];
    let goals = [scout, [scout[0] + 25.0, scout[1], scout[2]]];
    let ids = [tank, builder];
    app.insert_resource(battle);
    assert!(!crate::world_capture::test_tree_travel(app.world_mut()).unwrap());
    let game = &app.world().resource::<super::Match>().simulation;
    let accepted = ids.map(|id| game.entities[id].ai_goal_position());
    // Original45-degree dock raster rejects both raw clicks. These are the
    // native counterclockwise spiral's accepted goals, not tolerance changes.
    let expected: [[f32; 3]; 2] = [[965.0, 2_315.0, 10.0], [985.0, 2_265.0, 10.3125]];
    assert_eq!(
        accepted.map(|point| point.map(f32::to_bits)),
        expected.map(|point| point.map(f32::to_bits))
    );
    for (requested, actual) in goals.into_iter().zip(accepted) {
        let delta = Vec2::new(requested[0] - actual[0], requested[1] - actual[1]);
        assert!(
            delta.length_squared() > 20.0 * 20.0,
            "old raw-click arrival must fail"
        );
    }
    let began = game.tick;
    let mut arrived = false;
    for _ in 0..18_000 {
        app.world_mut()
            .resource_mut::<super::Match>()
            .simulation
            .step();
        arrived = crate::world_capture::test_tree_travel(app.world_mut()).unwrap();
        if arrived {
            break;
        }
    }
    let game = &app.world().resource::<super::Match>().simulation;
    assert!(
        arrived,
        "actual tree fixture failed unchanged arrival and Clear guard"
    );
    for (id, requested) in ids.into_iter().zip(accepted) {
        let actor = &game.entities[id];
        let result = generals_sim::skirmish::diagnostics::ground_route_result(game, id);
        let route = result
            .as_ref()
            .map(|route| (route.requested, route.endpoint, &route.active_layers));
        let dx = actor.position[0] - requested[0];
        let dy = actor.position[1] - requested[1];
        let horizontal = dx * dx;
        let vertical = dy * dy;
        assert!(
            horizontal + vertical < 20.0 * 20.0,
            "original tree scouting endpoint outside unchanged20unit arrival: id={id} began={began} tick={} requested={requested:?} accepted={:?} route={route:?}; {}",
            game.tick,
            actor.ai_goal_position(),
            generals_sim::skirmish::diagnostics::movement_details(game, id)
        );
    }
    assert!(crate::tree_buffer::verification::all_status(
        game,
        0,
        tree_position,
        &generals_sim::skirmish::visibility::Status::Clear
    ));
}
