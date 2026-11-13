//! Actual installed map/catalog/capture/timer/AI/Physics delivery chain.
use crate::skirmish_verification::advance_until;
use generals_sim::skirmish::{Faction, Order, Skirmish};

pub(super) fn original_game(path: &str) -> (Skirmish, [[f32; 2]; 2]) {
    let root = &generals_assets::game_root().unwrap();
    let fixture = crate::playtest::map_fixture::Fixture::load(root, path).unwrap();
    let bounds = fixture.draw_region();
    (fixture.into_simulation().unwrap(), bounds)
}

fn verify_delivery(
    game: &mut Skirmish,
    bounds: [[f32; 2]; 2],
    pad: usize,
    plane_name: &str,
    tank_name: &str,
    retirement: &Retirement,
) {
    let start = game.entities.len();
    let captured = game.tick;
    let delay = game.catalog[game.entities[pad].type_index]
        .behavior
        .ocl_update
        .as_ref()
        .unwrap()
        .min_delay;
    let color = game.players[0].color.unwrap();
    let rally = crate::reinforcement_pad_capture::original_delivery_goal(game, pad).unwrap();
    advance_until(
        game,
        delay + 2,
        "original full-duration faction OCL timer",
        |game| {
            game.entities[start..]
                .iter()
                .any(|actor| game.catalog[actor.type_index].name == plane_name)
        },
    )
    .unwrap();
    assert!(game.tick >= captured + u64::from(delay));
    let plane = (start..game.entities.len())
        .find(|&id| game.catalog[game.entities[id].type_index].name == plane_name)
        .unwrap();
    let chute = *game.passengers(plane).first().unwrap();
    let tank = *game.passengers(chute).first().unwrap();
    assert_eq!(
        game.catalog[game.entities[chute].type_index].name,
        "LargeParachute"
    );
    assert_eq!(game.catalog[game.entities[tank].type_index].name, tank_name);
    for id in [plane, chute, tank] {
        assert_eq!(game.entities[id].owner, 0);
        assert_eq!(game.players[game.entities[id].owner].color, Some(color));
    }
    assert!(game.entities[tank].parachute_rider());
    let mut opened = false;
    let mut released = false;
    let mut approached = false;
    let mut reached = false;
    let mut died_at = None;
    for _ in 0..3600 {
        let healthy = game.entities[plane].health > 0.0;
        game.step();
        if healthy && game.entities[plane].health <= 0.0 {
            died_at = Some((game.tick, game.object_position(plane)));
        }
        opened |= game.entities[chute].parachute_opened();
        let position = game.object_position(plane);
        approached |= game.entities[plane].exists()
            && (0..2)
                .all(|axis| position[axis] >= bounds[0][axis] && position[axis] <= bounds[1][axis]);
        released |= !game.entities[tank].parachute_rider() && game.entities[tank].alive();
        let position = game.object_position(tank);
        let x = position[0] - rally[0];
        let y = position[1] - rally[1];
        reached |= released
            && matches!(game.entities[tank].ai.order, Order::Idle)
            && x.abs() <= 10.0
            && y.abs() <= 10.0
            && game.height_above_ground(tank).abs() <= 0.1;
        if reached && !game.entities[plane].exists() {
            break;
        }
    }
    assert!(
        approached && opened && released && reached,
        "original cargo/parachute/rally lifecycle incomplete: approached={approached} opened={opened} released={released} reached={reached} tick={} rally={rally:?}; plane exists={} {}; chute exists={} position={:?} carrier={:?}; tank exists={} carrier={:?} {}",
        game.tick,
        game.entities[plane].exists(),
        generals_sim::skirmish::diagnostics::movement_details(game, plane),
        game.entities[chute].exists(),
        game.object_position(chute),
        game.entities[chute].contained_by(),
        game.entities[tank].exists(),
        game.entities[tank].contained_by(),
        generals_sim::skirmish::diagnostics::movement_details(game, tank),
    );
    assert!(!game.entities[plane].exists());
    verify_retirement(game, bounds, plane, retirement, died_at);
    assert!(game.entities[tank].alive());
    assert_eq!(game.players[game.entities[tank].owner].color, Some(color));
}

fn verify_retirement(
    game: &Skirmish,
    bounds: [[f32; 2]; 2],
    plane: usize,
    retirement: &Retirement,
    died_at: Option<(u64, [f32; 3])>,
) {
    match retirement {
        Retirement::OffMap => {
            assert!(
                died_at.is_none(),
                "source peaceful offmap departure must not kill; health loss at {died_at:?}"
            );
            assert!(game.entities[plane].health > 0.0);
            let position = game.object_position(plane);
            assert!(
                (0..2).any(
                    |axis| position[axis] < bounds[0][axis] || position[axis] > bounds[1][axis]
                )
            );
        }
        Retirement::Deflected => {
            // All three original Death Valley cases observed the native turning/dot<0.3
            // abort (SH DeliverPayloadAIUpdate.cpp1189–1200), before map exit.
            let (_, position) =
                died_at.expect("original deflected departure must take actual kill path");
            assert!(game.entities[plane].health <= 0.0);
            assert!(
                (0..2)
                    .all(|axis| position[axis] >= bounds[0][axis]
                        && position[axis] <= bounds[1][axis])
            );
        }
    }
}

enum Retirement {
    OffMap,
    Deflected,
}

fn verify_original_delivery(
    map: &str,
    faction: Faction,
    plane: &str,
    tank: &str,
    retirement: &Retirement,
) {
    let (mut game, bounds) = original_game(map);
    let pad = crate::reinforcement_pad_capture::capture_original_pad(&mut game, faction).unwrap();
    verify_delivery(&mut game, bounds, pad, plane, tank, retirement);
}

#[ignore = "requires original game installation"]
#[test]
fn installed_cairo_usa_delivery_retirement() {
    verify_original_delivery(
        "Maps\\Cairo Commandos\\Cairo Commandos.map",
        Faction::Usa,
        "AmericaJetCargoPlane",
        "AmericaTankCrusader",
        &Retirement::OffMap,
    );
}
#[ignore = "requires original game installation"]
#[test]
fn installed_cairo_china_delivery_retirement() {
    verify_original_delivery(
        "Maps\\Cairo Commandos\\Cairo Commandos.map",
        Faction::China,
        "ChinaJetCargoPlane",
        "ChinaTankBattleMaster",
        &Retirement::OffMap,
    );
}
#[ignore = "requires original game installation"]
#[test]
fn installed_cairo_gla_delivery_retirement() {
    verify_original_delivery(
        "Maps\\Cairo Commandos\\Cairo Commandos.map",
        Faction::Gla,
        "GLAJetCargoPlane",
        "GLATankScorpion",
        &Retirement::OffMap,
    );
}
#[ignore = "requires original game installation"]
#[test]
fn installed_death_valley_usa_delivery_retirement() {
    verify_original_delivery(
        "Maps\\Death Valley\\Death Valley.map",
        Faction::Usa,
        "AmericaJetCargoPlane",
        "AmericaTankCrusader",
        &Retirement::Deflected,
    );
}
#[ignore = "requires original game installation"]
#[test]
fn installed_death_valley_china_delivery_retirement() {
    verify_original_delivery(
        "Maps\\Death Valley\\Death Valley.map",
        Faction::China,
        "ChinaJetCargoPlane",
        "ChinaTankBattleMaster",
        &Retirement::Deflected,
    );
}
#[ignore = "requires original game installation"]
#[test]
fn installed_death_valley_gla_delivery_retirement() {
    verify_original_delivery(
        "Maps\\Death Valley\\Death Valley.map",
        Faction::Gla,
        "GLAJetCargoPlane",
        "GLATankScorpion",
        &Retirement::Deflected,
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_paid_supply_drop_zone_delivers_six_physical_crates_and_retires() {
    use crate::strategy_verification::template;

    let (mut game, _) = original_game("Maps\\Cairo Commandos\\Cairo Commandos.map");
    let zone = crate::reinforcement_pad_capture::build_paid_zone(&mut game).unwrap();
    let first = game.entities.len();
    let completion = game.tick;
    let plane_type = template(&game, "AmericaJetCargoPlane").unwrap();
    let chute_type = template(&game, "AmericaCrateParachute").unwrap();
    let crate_type = template(&game, "SupplyDropZoneCrate").unwrap();
    advance_until(&mut game, 3602, "full Drop Zone timer", |game| {
        game.entities[first..]
            .iter()
            .any(|actor| actor.type_index == plane_type)
    })
    .unwrap();
    assert!(game.tick >= completion + 3599);
    let plane = (first..game.entities.len())
        .find(|&id| game.entities[id].type_index == plane_type)
        .unwrap();
    assert_eq!(game.entities[plane].producer(), Some(zone));
    let chutes = game.passengers(plane).to_vec();
    assert_eq!(chutes.len(), 6);
    let crates: Vec<_> = chutes
        .iter()
        .map(|&id| {
            assert_eq!(game.entities[id].type_index, chute_type);
            let [payload] = game.passengers(id) else {
                panic!("each original parachute contains one money crate");
            };
            assert_eq!(game.entities[*payload].type_index, crate_type);
            assert_eq!(game.entities[*payload].owner, 0);
            *payload
        })
        .collect();
    let mut pickups = Vec::new();
    advance_until(
        &mut game,
        3600,
        "six actual MoneyCrateCollide rewards and plane retirement",
        |game| {
            pickups.extend(std::mem::take(&mut game.presentation.money_crate_events));
            pickups.len() == 6 && !game.entities[plane].exists()
        },
    )
    .unwrap();
    let mut received = Vec::new();
    for event in pickups {
        received.push(event.item);
        assert_eq!(game.entities[event.collector].owner, 0);
        assert_eq!(event.amount, 250);
    }
    let mut expected = crates.clone();
    expected.sort_unstable();
    received.sort_unstable();
    assert_eq!(received, expected);
    assert!(crates.into_iter().all(|item| !game.entities[item].exists()));
    assert!(game.sell(0, zone).is_ok());
    let next = game.entities.len();
    for _ in 0..3600 {
        game.step();
    }
    assert!(!game.entities[zone].exists());
    assert!(
        !game.entities[next..]
            .iter()
            .any(|actor| actor.type_index == plane_type)
    );
}
