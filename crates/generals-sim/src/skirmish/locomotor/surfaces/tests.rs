use crate::skirmish::{
    Faction, GroundAppearance, GroundRules, Kind, LocomotorVariant, Order, Skirmish,
    tests::{fixture_type, game},
};
use crate::{Cell, Grid, terrain::TerrainMap};

fn rules(appearance: GroundAppearance) -> GroundRules {
    GroundRules {
        wander: None,
        appearance,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 50.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15 / 30.0,
        lateral_friction: 0.15 / 30.0,
        extra_friction: 0.0,
    }
}

fn setup(water: bool) -> (Skirmish, usize) {
    let mut game = game();
    let base = &mut game.catalog[5];
    base.speed = 30.0;
    base.locomotor.legal_surfaces = 17;
    base.locomotor.speed_damaged = 20.0;
    base.locomotor.ground = Some(rules(GroundAppearance::Legs));
    let mut cliff = fixture_type(Faction::Gla, Kind::Infantry).locomotor;
    cliff.legal_surfaces = 4;
    cliff.speed_damaged = 15.0;
    cliff.ground = Some(rules(GroundAppearance::Climber));
    base.locomotor_sets.normal.push(LocomotorVariant {
        speed: 20.0,
        locomotor: cliff,
    });
    let terrain = TerrainMap::classify(
        80,
        60,
        (0..60).map(|y| Cell { x: 35, y }),
        (0..60).filter(|_| water).map(|y| Cell { x: 45, y }),
    )
    .unwrap();
    game.grid = Grid::new(80, 60, terrain.blocked_ground_cells()).unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    let id = game.spawn(5, 0, [300.0, 400.0], true);
    game.entities[id].ai.mood.next_check = u64::MAX;
    (game, id)
}

#[test]
fn ordered_ground_cliff_ground_selection_retains_previous_for_invalid_surface() {
    let (mut game, id) = setup(true);
    assert_eq!(game.entities[id].ai.surface_locomotor, 0);
    game.entities[id].position = [350.0, 400.0];
    game.select_surface_locomotor(id);
    assert_eq!(game.entities[id].ai.surface_locomotor, 1);
    assert_eq!(
        super::super::speed(&game.entities[id], &game.catalog[5]).to_bits(),
        20.0_f32.to_bits()
    );
    game.entities[id].health = game.entities[id].max_health * 0.35;
    assert_eq!(
        super::super::speed(&game.entities[id], &game.catalog[5]).to_bits(),
        15.0_f32.to_bits()
    );
    game.entities[id].position = [450.0, 400.0];
    game.select_surface_locomotor(id);
    assert_eq!(game.entities[id].ai.surface_locomotor, 1);
    game.entities[id].position = [400.0, 400.0];
    game.select_surface_locomotor(id);
    assert_eq!(game.entities[id].ai.surface_locomotor, 0);
    assert_eq!(
        super::super::speed(&game.entities[id], &game.catalog[5]).to_bits(),
        20.0_f32.to_bits()
    );
    // Selection is authored order, even if a later member accepts the same cell.
    game.catalog[5].locomotor_sets.normal[0]
        .locomotor
        .legal_surfaces |= 1;
    game.select_surface_locomotor(id);
    assert_eq!(game.entities[id].ai.surface_locomotor, 0);
}

#[test]
fn cliff_routing_crosses_the_wall_but_keeps_water_and_static_obstacles() {
    let (mut game, id) = setup(true);
    assert!(
        game.movement_path(id, [300.0, 400.0], [410.0, 400.0])
            .is_some()
    );
    assert!(
        game.movement_path(id, [300.0, 400.0], [500.0, 400.0])
            .is_none()
    );
    let ordinary = game.spawn(1, 0, [300.0, 420.0], true);
    assert!(
        game.movement_path(ordinary, [300.0, 420.0], [410.0, 420.0])
            .is_none()
    );
    let building = game.spawn(0, 1, [410.0, 400.0], true);
    // Direct fixture spawn bypasses BuildAssistant's obstacle registration.
    game.rebuild_navigation();
    game.refresh_occupancy();
    assert!(
        game.navigation_for(id)
            .is_blocked(game.cell_at(game.entities[building].position).unwrap())
    );
    game.entities[building].health = 0.0;
    game.rebuild_navigation();
    game.refresh_occupancy();
    game.move_to(id, [415.0, 405.0]).unwrap();
    let mut saw_cliff = false;
    for _ in 0..1000 {
        game.step();
        saw_cliff |= game.entities[id].ai.surface_locomotor == 1;
        if matches!(game.entities[id].ai.order, Order::Idle) {
            break;
        }
    }
    assert!(saw_cliff);
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert_eq!(game.entities[id].ai.surface_locomotor, 0);
    assert!((game.entities[id].position[0] - 415.0).abs() < 1.0);
}

#[test]
fn cliff_path_keeps_both_ground_boundary_anchors_and_their_heights() {
    let (mut game, id) = setup(false);
    let heights: Vec<_> = (0..60)
        .flat_map(|_| (0..80).map(|x| if x <= 35 { 0.0 } else { 40.0 }))
        .collect();
    game.set_projectile_terrain(80, 60, [0.0, 0.0], heights, 10.0)
        .unwrap();
    let route = game
        .movement_path(id, [300.0, 400.0], [415.0, 405.0])
        .unwrap();
    let boundaries: Vec<_> = game
        .navigation_for(id)
        .cliff_boundary_cells(&route.raw)
        .collect();
    assert_eq!(boundaries.len(), 2);
    // classifyMap expands the authored x35 cliff through x34..36, then marks
    // the outer ground band pinched. Its protected ground nodes are x33/37.
    assert_eq!(boundaries[0].cell.x, 33);
    assert_eq!(boundaries[1].cell.x, 37);
    assert!(boundaries.iter().all(|node| node.layer == 1));
    assert!(boundaries.iter().all(|node| matches!(
        game.movement_terrain.cell(node.cell).unwrap().kind,
        crate::terrain::CellType::Clear
    )));
    assert!(boundaries.iter().all(|node| route.optimized.contains(node)));
    let goal = *route.optimized.back().unwrap();
    game.entities[id].ai.path = route;
    game.entities[id].ai.path_goal = Some(goal.cell);
    game.entities[id].ai.path_endpoint = Some([415.0, 405.0]);
    game.entities[id].ai.order = Order::Move([415.0, 405.0]);
    game.install_ground_path(id);
    let before = game.path_position(id, boundaries[0].cell);
    let after = game.path_position(id, boundaries[1].cell);
    let mut path = game.entities[id].ai.ground_path.take().unwrap();
    // More than halfway along the initial segment must still sample its
    // protected ground-side boundary, not a midpoint inside the cliff run.
    let sample = path.sample_layer([before[0] - 2.0, before[1], 0.0], 1, |_, _, _| true);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [before[0], before[1], 0.0].map(f32::to_bits)
    );
    let sample = path.sample_layer([after[0] - 2.0, after[1], 40.0], 1, |_, _, _| true);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [after[0], after[1], 40.0].map(f32::to_bits)
    );
}

#[test]
fn cliff_reachability_uses_the_selected_wander_vector() {
    let (mut game, id) = setup(false);
    let cliff = game.catalog[5].locomotor_sets.normal.remove(0);
    let mut ground = fixture_type(Faction::Gla, Kind::Infantry).locomotor;
    ground.legal_surfaces = 1;
    ground.ground = Some(rules(GroundAppearance::Legs));
    game.catalog[5].locomotor_sets.wander = vec![
        LocomotorVariant {
            speed: 30.0,
            locomotor: ground,
        },
        cliff,
    ];
    game.rebuild_navigation();
    game.choose_locomotor_set(id, super::set::WANDER);
    assert!(
        game.movement_path(id, [300.0, 400.0], [410.0, 400.0])
            .is_some()
    );
    game.choose_locomotor_set(id, super::set::NORMAL);
    assert!(
        game.movement_path(id, [300.0, 400.0], [410.0, 400.0])
            .is_none()
    );
}

#[test]
fn generic_panic_role_uses_its_own_speed_damage_and_live_template() {
    let (mut game, id) = setup(false);
    let mut panic = fixture_type(Faction::Usa, Kind::Vehicle).locomotor;
    panic.speed_damaged = 200.0;
    panic.works_when_dead = true;
    panic.move_priority = 0;
    game.catalog[5].locomotor_sets.panic.push(LocomotorVariant {
        speed: 290.0,
        locomotor: panic,
    });
    game.choose_locomotor_set(id, super::set::PANIC);
    let actor = &game.entities[id];
    assert_eq!(actor.ai.locomotor_set, super::set::PANIC);
    assert!(actor.active_locomotor(&game.catalog[5]).works_when_dead);
    assert_eq!(actor.active_locomotor(&game.catalog[5]).move_priority, 0);
    assert_eq!(
        super::super::speed(actor, &game.catalog[5]).to_bits(),
        290.0_f32.to_bits()
    );
    game.entities[id].health = 1.0;
    assert_eq!(
        super::super::speed(&game.entities[id], &game.catalog[5]).to_bits(),
        200.0_f32.to_bits()
    );
    game.choose_locomotor_set(id, super::set::NORMAL);
    assert_eq!(
        super::super::speed(&game.entities[id], &game.catalog[5]).to_bits(),
        20.0_f32.to_bits()
    );
}
