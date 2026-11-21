use super::movement::{cell_center, distance_squared};
use super::*;
use std::f32::consts::PI;

mod performance;
mod placement;

fn fixture_kind_flags(kind: Kind) -> Vec<String> {
    let mut flags = vec![fixture_kind(kind).into()];
    if kind.is_building() {
        flags.push("IMMOBILE".into());
    }
    if kind == Kind::SupplyCenter {
        flags.push("CANNOT_BUILD_NEAR_SUPPLIES".into());
    }
    if kind == Kind::Builder {
        flags.push("DOZER".into());
    }
    flags
}

pub(super) fn fixture_type(faction: Faction, kind: Kind) -> UnitType {
    let mut unit = fixture_data(faction, kind);
    register_fixture_updates(&mut unit);
    unit
}

fn fixture_data(faction: Faction, kind: Kind) -> UnitType {
    UnitType {
        command_set: String::new(),
        pristine_draws: Vec::new(),
        sway_updates: 0,
        combat_drop_kill_fx: None,
        laser_update: None,
        behavior: fixture_behavior(kind),
        projectile_rules: Vec::new(),
        body_kind: BodyKind::Active,
        body_damage_thresholds: [0.5, 0.1],
        deliver_payload: None,
        update_modules: fixture_update_helpers(matches!(kind, Kind::Tank | Kind::Infantry)),
        behavior_modules: Vec::new(),
        build_rules: fixture_build_rules(),
        neutral_tech: NeutralTechRules::none(),
        minimum_health: 0.0,
        battle_bus: None,
        combat_bike: None,
        live_physics: None,
        aircraft: None,
        parking: None,
        fire: None,

        hive_body: None,
        death_lifecycle: None,
        structure_death: None,
        demo_trap: Vec::new(),
        spectre: None,
        spectre_deployments: Vec::new(),
        special_powers: Vec::new(),
        unpause_special_powers: Vec::new(),
        deployment: None,
        hijacker: None,
        parachute: None,
        physical: None,
        battle_plan: None,
        variations: Vec::new(),
        reskin: None,
        salvage: salvage::Rules::none(),
        attachment_bone: None,
        attachment_point: [0.0; 3],
        attachment_thresholds: [0.5, 0.1],
        contain: None,
        transport_slots: 1,
        kind_of: fixture_kind_flags(kind),
        turret: None,
        secondary_turret: None,
        turrets_linked: false,
        enter_guard: false,
        sight: visibility::Sight::new(150.0),
        maintenance: fixture_maintenance(kind),
        veterancy: super::VeterancyRules::none(),
        name: String::from("Fixture"),
        faction,
        kind,
        selectable: true,
        scoring: fixture_scoring(kind),
        progression: UnitProgression::default(),
        ai: fixture_ai(faction, kind),
        cost: if kind.is_building() { 1500 } else { 200 },
        build_seconds: if kind.is_building() { 10.0 } else { 5.0 },
        max_health: if kind.is_building() { 1000.0 } else { 100.0 },
        initial_health: if kind.is_building() { 1000.0 } else { 100.0 },
        speed: if kind.is_building() { 0.0 } else { 30.0 },
        locomotor: fixture_locomotor(!kind.is_building()),
        locomotor_sets: LocomotorSets::none(),
        weapon: fixture_weapon(matches!(kind, Kind::Tank | Kind::Infantry)),
        secondary_weapon: None,
        tertiary_weapon: None,
        weapon_set_rules: WeaponSetRules::independent(),
        weapon_upgrade_triggers: Vec::new(),
        armor: fixture_armor(),
        damage_fx: Vec::new(),
        damaged_sound: String::new(),
        really_damaged_sound: String::new(),
        fear_sound: String::new(),
        supply: fixture_supply(kind),
        geometry: fixture_geometry(!kind.is_building()),
        geometry_bounds: fixture_bounds(!kind.is_building()),

        occlusion_delay: 90,
        fence_width: 0.0,

        placement_angle: 0.0,
        preferred_height: 0.0,
        chinook: None,
        movement_surface: super::MovementSurface::Ground,
        crush: fixture_crush(!kind.is_building()),
        group_kind: fixture_group(kind),
        buildable_objects: vec!["Fixture".into()],
        location_weapons: 0,
        production: fixture_production(kind),
        additional_production: Vec::new(),
        production_queue_limit: 9,
        production_disabled_types: 1 << 3,
        production_doors: parking::DoorRules::NONE,
        factory_bib: FactoryBib::NONE,
        energy: 0,
    }
}

fn register_fixture_updates(unit: &mut UnitType) {
    if unit.kind == Kind::SupplyCenter {
        register_behavior(unit, "SupplyCenterCreate", 8);
        register_update(unit, "SupplyCenterDockUpdate");
    }
    let family = match unit.ai.update {
        UnitAiUpdate::Worker => "WorkerAIUpdate",
        UnitAiUpdate::Dozer => "DozerAIUpdate",
        UnitAiUpdate::SupplyTruck => "SupplyTruckAIUpdate",
        _ => "AIUpdateInterface",
    };
    register_update(unit, family);
    if unit.production.is_some() {
        register_update(unit, "ProductionUpdate");
    }
    if unit.behavior.production_exit.is_some() {
        register_update(unit, "DefaultProductionExitUpdate");
    }
}

fn fixture_behavior(kind: Kind) -> BehaviorData {
    BehaviorData {
        particle_uplinks: Vec::new(),
        ocl_powers: Vec::new(),
        power_weapons: Vec::new(),
        cash_hack_powers: Vec::new(),
        spy_vision_powers: Vec::new(),
        nuclear_launcher: None,
        special_power_completions: Vec::new(),
        weapon_bonuses: Vec::new(),
        leaflet_drops: Vec::new(),
        smart_bombs: Vec::new(),
        emp: None,
        neutron_missile: None,
        gps_scramblers: Vec::new(),
        dynamic_shroud: Vec::new(),
        create_object_die: Vec::new(),
        eject_pilots: Vec::new(),
        bridges: Vec::new(),
        neutron_blast: Vec::new(),
        bone_fx: Vec::new(),
        damage_transitions: Vec::new(),
        command_button_hunts: Vec::new(),
        mine_generators: Vec::new(),
        minefields: Vec::new(),
        hijackers: Vec::new(),
        sticky_bombs: Vec::new(),
        special_abilities: Vec::new(),
        assisted_targeting: Vec::new(),
        mobs: Vec::new(),
        spawns: Vec::new(),
        spawn_exits: Vec::new(),
        slaves: Vec::new(),
        prone: Vec::new(),
        supply_crippling: Vec::new(),
        toppling: Vec::new(),
        granted_sciences: Vec::new(),
        lock_weapons: Vec::new(),
        object_creation: Vec::new(),
        auto_find_healing: Vec::new(),
        lifetimes: Vec::new(),
        deletions: Vec::new(),
        height_die: Vec::new(),
        ocl_update: None,
        spy_vision: Vec::new(),
        radar_updates: Vec::new(),
        radar_upgrades: Vec::new(),
        radar_grants: Vec::new(),
        money_crate: None,
        veterancy_crates: Vec::new(),
        pilot_find_vehicle: Vec::new(),
        stealth: None,
        detectors: Vec::new(),
        flammable: Vec::new(),
        fire_spread: Vec::new(),
        death_weapons: Vec::new(),
        instant_death: Vec::new(),
        death_fx: Vec::new(),
        crush_die: Vec::new(),
        upgrade_die: Vec::new(),
        destroy_die: Vec::new(),
        car_bomb_converter: None,
        sabotage: Vec::new(),
        drone_upgrades: Vec::new(),
        point_defense: Vec::new(),
        countermeasures: Vec::new(),
        bunker_busters: Vec::new(),
        cleanup: Vec::new(),
        cleanup_area: Vec::new(),
        fire_weapons: Vec::new(),
        collision_weapons: Vec::new(),
        damage_weapons: Vec::new(),
        firestorms: Vec::new(),
        contamination: Vec::new(),
        poisoned: Vec::new(),
        attachments: Vec::new(),
        replacement_names: Vec::new(),
        sub_object_upgrades: Vec::new(),
        propaganda: None,
        horde: None,
        hacking: None,
        production_exit: fixture_production_exit(kind),
    }
}

// These synthetic templates explicitly use ActiveBody, never SHRUBBERY or
// CAN_BE_REPULSED. `armed` is the same authored weapon presence as fixture_weapon.
pub(super) fn fixture_update_helpers(armed: bool) -> Vec<module_updates::ModuleRegistration> {
    use module_updates::{Helper, ModuleRegistration};
    let mut helpers = vec![
        Helper::StateMachine,
        Helper::StatusDamage,
        Helper::SubdualDamage,
        Helper::Defection,
    ];
    if armed {
        helpers.extend([
            Helper::WeaponStatus,
            Helper::FiringTracker,
            Helper::TemporaryWeaponBonus,
        ]);
    }
    helpers
        .into_iter()
        .map(ModuleRegistration::Helper)
        .collect()
}

fn fixture_bounds(mobile: bool) -> GeometryBounds {
    GeometryBounds {
        center_height: 5.0,
        sphere_radius: if mobile { 5.0 } else { 25.0 },
        max_height: 10.0,
    }
}

fn fixture_build_rules() -> BuildRules {
    BuildRules {
        status: Buildable::Yes,
        objects: Vec::new(),
        sciences: Vec::new(),
        limit: TypeLimit::Count(0),
        linked_limit: None,
    }
}

fn fixture_maintenance(kind: Kind) -> MaintenanceRules {
    let mut rules = MaintenanceRules::none();
    rules.healing_kind_flags = fixture_healing_kind(kind);
    rules
}

fn fixture_ai(faction: Faction, kind: Kind) -> UnitAi {
    UnitAi {
        update: match kind {
            Kind::Builder if faction == Faction::Gla => UnitAiUpdate::Worker,
            Kind::Builder => UnitAiUpdate::Dozer,
            Kind::Harvester => UnitAiUpdate::SupplyTruck,
            _ => UnitAiUpdate::Ordinary,
        },
        vision_range: 150.0,
        acquisition: UnitAi::IDLE,
        ..UnitAi::default()
    }
}

const fn fixture_healing_kind(kind: Kind) -> u8 {
    match kind {
        Kind::Infantry | Kind::Builder => 1,
        _ if !kind.is_building() => 2,
        _ => 0,
    }
}

fn fixture_production_exit(kind: Kind) -> Option<ProductionExit> {
    kind.is_building().then_some(ProductionExit {
        use_spawn_rally_point: false,
        create: [0.0, 0.0, 0.0],
        rally: [35.0, 0.0, 0.0],
        snap_rally: false,
        extend_rally: true,
        queue: None,
    })
}

fn fixture_production(kind: Kind) -> Option<Vec<(String, u32)>> {
    matches!(
        kind,
        Kind::CommandCenter | Kind::SupplyCenter | Kind::Barracks | Kind::Factory
    )
    .then(Vec::new)
}

const fn fixture_armor() -> [f32; crate::skirmish::DAMAGE_TYPE_COUNT] {
    let mut armor = [1.0; crate::skirmish::DAMAGE_TYPE_COUNT];
    armor[DamageType::HazardCleanup as usize] = 0.0;
    armor
}

const fn fixture_crush(mobile: bool) -> CrushRules {
    CrushRules {
        crusher_level: 0,
        crushable_level: 255,
        squish_module: None,
        physics_module: if mobile { Some(0) } else { None },
        hijacker: false,
    }
}

fn fixture_supply(kind: Kind) -> supply::CollectorRules {
    supply::CollectorRules {
        capacity: u32::from(kind == Kind::Builder),
        depleted_voice: String::new(),
        warehouse: None,
        scan_distance: 700.0,
        dock: fixture_dock(kind),
        gather_seconds: 0.15,
        dropoff_seconds: 0.15,
    }
}

fn fixture_dock(kind: Kind) -> Option<SupplyDock> {
    (kind == Kind::SupplyCenter).then_some(SupplyDock {
        number_approaches: -1,
        approach_bones: 0,
        temporary_stealth_frames: 0,
        approaches: Vec::new(),
        enter: None,
        action: None,
        exit: None,
        allows_passthrough: false,
    })
}

const fn fixture_group(kind: Kind) -> GroupKind {
    match kind {
        Kind::Infantry | Kind::Builder => GroupKind::Infantry,
        other if !other.is_building() => GroupKind::Vehicle,
        _ => GroupKind::Other,
    }
}

fn fixture_geometry(mobile: bool) -> crate::collision_geometry::Geometry {
    crate::collision_geometry::Geometry {
        shape: crate::collision_geometry::Shape::Cylinder,
        is_small: mobile,
        major: if mobile { 5.0 } else { 25.0 },
        minor: if mobile { 5.0 } else { 25.0 },
        height: 10.0,
    }
}

fn fixture_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Infantry | Kind::Builder => "INFANTRY",
        _ if !kind.is_building() => "VEHICLE",
        _ => "STRUCTURE",
    }
}

fn fixture_weapon(armed: bool) -> WeaponRules {
    WeaponRules {
        damage: if armed { 60.0 } else { 0.0 },
        range: if armed { 150.0 } else { 0.0 },
        delay: if armed { 2.0 } else { 0.0 },
        speed: if armed { 600.0 } else { 0.0 },
        ..WeaponRules::unarmed()
    }
}

fn fixture_locomotor(mobile: bool) -> Locomotor {
    Locomotor {
        move_priority: 2,
        wander_length: mobile.then_some(1.0),
        wander_about_point_radius: 0.0,
        wander_width: 0.0,
        stick_to_ground: false,
        friction_when_airborne: false,
        works_when_dead: false,
        flight: None,
        legal_surfaces: 1,
        airborne_targeting_height: 2_147_483_648.0,
        ground: None,
        speed_damaged: if mobile { 30.0 } else { 0.0 },
        turn_rate: if mobile { PI * STEP_SECONDS } else { 0.0 },
        turn_rate_damaged: if mobile { PI * STEP_SECONDS } else { 0.0 },
        turn_pivot_offset: 0.0,
        damage_health_threshold: 0.35,
    }
}

pub(super) fn game() -> Skirmish {
    let kinds = [
        Kind::CommandCenter,
        Kind::Builder,
        Kind::SupplyCenter,
        Kind::Barracks,
        Kind::Factory,
        Kind::Infantry,
        Kind::Tank,
    ];
    let catalog = kinds
        .into_iter()
        .map(|kind| fixture_type(Faction::Gla, kind))
        .collect();
    let mut game = Skirmish::new(
        crate::terrain::TerrainMap::classify(80, 60, [], []).unwrap(),
        catalog,
        Faction::Gla,
        Faction::Gla,
        EconomyRules {
            allowed_build_height_variation: 0.0,
            min_build_edge_distance: 0.0,
            starting_cash: 10_000,
            supply_value: 75,
            supply_build_border: 20.0,
            low_energy_penalty_modifier: 1.0,
            min_low_energy_production_speed: 0.2,
            max_low_energy_production_speed: 0.5,
        },
        AiRules {
            guard_inner_human: 1.0,
            guard_outer_human: 1.0,
            guard_inner_ai: 1.0,
            guard_outer_ai: 1.0,
            guard_chase_frames: 300,
            ..AiRules::default()
        },
        MapLayout {
            height_map: None,
            map_sides: Vec::new(),
            map_teams: Vec::new(),
            map_build_lists: Vec::new(),
            map_template_order: Vec::new(),
            scripts: Vec::new(),
            fences: Vec::new(),
            obstacle_teams: Vec::new(),
            world_objects: Vec::new(),
            world_object_order: Vec::new(),
            obstacle_objects: Vec::new(),
            obstacle_order: Vec::new(),
            partition_cell_size: 40.0,
            random_seed: 1,
            regular_bridges: Vec::new(),
            neutral_objects: Vec::new(),
            starts: vec![
                PlayerStart {
                    position: [185.0, 305.0],
                    rally: None,
                },
                PlayerStart {
                    position: [615.0, 305.0],
                    rally: None,
                },
            ],
            supplies: Vec::new(),
            obstacles: Vec::new(),
        },
    )
    .unwrap();
    for position in [[185.0, 175.0], [615.0, 175.0]] {
        fixture_warehouse(&mut game, position, 16.0, 400, false);
    }
    game.update_visibility();
    game
}

fn ground_game() -> Skirmish {
    let mut game = game();
    for unit in &mut game.catalog {
        let appearance = match unit.kind {
            Kind::Infantry | Kind::Builder => locomotor::GroundAppearance::Legs,
            Kind::Tank => locomotor::GroundAppearance::Treads,
            _ => continue,
        };
        unit.locomotor.ground = Some(ground_movement::tests::ground_rules(appearance));
    }
    game
}

// AIInternalMoveToState exits at the locomotor's CloseEnoughDist, before
// reaching the exact final node. The authored waypoints retain full precision.
fn assert_ground_arrival(game: &Skirmish, id: usize, goal: [f32; 2]) {
    let actor = &game.entities[id];
    let locomotor = actor.active_locomotor(&game.catalog[actor.type_index]);
    let close = locomotor
        .ground
        .as_ref()
        .expect("ground locomotor")
        .close_enough;
    assert!(
        distance_squared(actor.position, goal) < close * close,
        "ground actor {id} at {:?} expected within {close} of {goal:?}",
        actor.position
    );
}

pub(in crate::skirmish) fn fixture_warehouse(
    game: &mut Skirmish,
    position: [f32; 2],
    radius: f32,
    boxes: u32,
    delete_when_empty: bool,
) -> usize {
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.name = format!("Warehouse{}", game.catalog.len());
    unit.kind_of = vec!["SUPPLY_SOURCE".into(), "IMMOBILE".into()];
    unit.geometry.major = radius;
    unit.geometry.minor = radius;
    unit.geometry_bounds.sphere_radius = radius;
    unit.supply.warehouse = Some(supply::WarehouseRules {
        starting_boxes: boxes,
        delete_when_empty,
    });
    unit.supply.dock = Some(SupplyDock {
        number_approaches: -1,
        approach_bones: 0,
        temporary_stealth_frames: 0,
        approaches: Vec::new(),
        enter: None,
        action: None,
        exit: None,
        allows_passthrough: true,
    });
    unit.behavior_modules.clear();
    unit.update_modules.clear();
    register_behavior(&mut unit, "SupplyWarehouseCreate", 8);
    register_update(&mut unit, "SupplyWarehouseDockUpdate");
    let name = unit.name.clone();
    game.catalog.push(unit);
    let neutral = game.layout.neutral_objects.len();
    game.layout.neutral_objects.push(NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        original_team: "team".into(),
        script_name: None,
        name,
        position: [position[0], position[1], 0.0],
        angle: 0.0,
        script_targetable: false,
        indestructible: false,
        selectable: None,
    });
    game.layout.supplies.push(neutral);
    game.spawn_neutral_object(neutral, false).unwrap();
    game.supplies.len() - 1
}

#[test]
fn construction_pays_once_then_grants_one_collector_and_deposits_supplies() {
    let mut game = ground_game();
    game.catalog[0].energy = 1;
    game.update_power();
    game.step();
    let supply = game.type_for(0, Kind::SupplyCenter).unwrap();
    let worker = 1;
    let original_cash = game.players[0].cash;
    assert!(game.build(worker, supply, [-10.0, 100.0]).is_err());
    assert_eq!(game.players[0].cash, original_cash);
    let site = game.placement_near(supply, [185.0, 215.0]).unwrap();
    let building = game.build(worker, supply, site).unwrap();
    assert_eq!(
        game.players[0].cash,
        original_cash - game.catalog[supply].cost
    );
    assert!((game.entities[building].health - 1.0).abs() < f32::EPSILON);
    for _ in 0..1800 {
        game.step();
    }
    assert!(game.entities[building].complete());
    assert_eq!(
        game.entities
            .iter()
            .filter(
                |entity| entity.owner == 0 && game.catalog[entity.type_index].kind == Kind::Builder
            )
            .count(),
        2
    );
    let earned = game.players[0].cash - (original_cash - game.catalog[supply].cost);
    assert!(
        earned >= 75,
        "The free worker must finish an actual pickup/dropoff cycle"
    );
    assert_eq!(earned % 75, 0);
    assert_eq!(game.players[0].statistics.money_earned, earned);
    assert_eq!(
        game.players[0].statistics.money_spent,
        game.catalog[supply].cost + game.catalog[0].cost
    );
    assert_eq!(game.players[0].statistics.buildings_built, 2);
    assert_eq!(game.players[0].statistics.units_built, 2);
    assert!(game.supplies[0].boxes < 400);
}

#[test]
fn unit_queue_charges_once_and_completes_after_source_duration() {
    let mut game = game();
    game.catalog[0].energy = 1;
    game.update_power();
    game.catalog[1].name = "Worker".into();
    game.catalog[0].buildable_objects = vec!["Worker".into()];
    let builder = game.type_for(0, Kind::Builder).unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    assert!(game.train(0, tank).is_err());
    assert_eq!(game.players[0].cash, 10_000);
    game.train(0, builder).unwrap();
    assert_eq!(game.players[0].cash, 9800);
    for _ in 0..149 {
        game.step();
    }
    assert_eq!(game.players[0].statistics.units_built, 1);
    assert_eq!(
        game.entities
            .iter()
            .filter(|entity| entity.owner == 0)
            .count(),
        2
    );
    game.step();
    assert_eq!(
        game.entities
            .iter()
            .filter(|entity| entity.owner == 0)
            .count(),
        3
    );
    assert!(game.entities[0].queue.is_empty());
    assert_eq!(game.players[0].cash, 9800);
    assert_eq!(game.players[0].statistics.units_built, 2);
    // Original ProductionUpdate books unit counts, but only upgrades book spent money.
    assert_eq!(game.players[0].statistics.money_spent, game.catalog[0].cost);
}

#[test]
fn scorekeeper_excludes_foundations_and_friendly_kill_credit() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let own = game.spawn(tank, 0, [300.0, 300.0], true);
    game.score_kill(1, own);
    assert_eq!(game.players[0].statistics.units_lost, 1);
    assert_eq!(game.players[0].statistics.units_destroyed, 0);
    game.score_kill(1, 2);
    assert_eq!(game.players[0].statistics.buildings_destroyed, 1);
    assert_eq!(game.players[1].statistics.buildings_lost, 1);
    game.entities[2].build_progress = 0.5;
    game.score_kill(1, 2);
    assert_eq!(game.players[0].statistics.buildings_destroyed, 1);
    assert_eq!(game.players[0].statistics.calculate_score(), 300);
}

#[test]
fn unfinished_victory_building_keeps_player_alive_but_noncounting_structure_does_not() {
    let mut game = game();
    let barracks = game.type_for(0, Kind::Barracks).unwrap();
    let foundation = game.spawn(barracks, 0, [300.0, 300.0], false);
    game.entities[0].health = 0.0;
    game.step();
    assert!(game.ended_at.is_none());
    game.catalog[barracks].scoring.victory = false;
    game.step();
    assert_eq!(game.outcome, Some(1));
    assert!(!game.entities[foundation].alive());
    assert!(!game.entities[1].alive());
    assert_eq!(game.players[0].cash, 0);
    assert_eq!(game.players[0].statistics.units_lost, 0);
    assert_eq!(game.players[1].statistics.units_destroyed, 0);
}

#[test]
fn unoccupied_skirmish_slot_has_no_objects_or_end_scripts_and_can_restart() {
    let mut game = game();
    game.reset_sandbox(Faction::Gla).unwrap();
    assert_eq!(game.occupied, [true, false, false]);
    assert!(
        game.entities
            .iter()
            .enumerate()
            .all(|(id, actor)| actor.owner == 0
                || (actor.owner == game.neutral_owner()
                    && game.supplies.iter().any(|dock| dock.object == id)))
    );
    game.step();
    assert!(game.ended_at.is_none());
    let local_center = game
        .entities
        .iter_mut()
        .find(|actor| {
            actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::CommandCenter
        })
        .unwrap();
    local_center.health = 0.0;
    game.step();
    assert!(game.ended_at.is_none());
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.occupied, [true, true, false]);
    assert!(game.entities.iter().any(|entity| entity.owner == 1));
    let enemy_center = game
        .entities
        .iter()
        .position(|actor| {
            actor.owner == 1 && game.catalog[actor.type_index].kind == Kind::CommandCenter
        })
        .unwrap();
    game.entities[enemy_center].health = 0.0;
    game.step();
    assert_eq!(game.outcome, Some(0));
}

#[test]
fn simultaneous_elimination_ends_once_and_reset_clears_results() {
    let mut game = game();
    game.entities[0].health = 0.0;
    game.entities[2].health = 0.0;
    game.step();
    assert_eq!(game.defeated, [true, true, false]);
    assert!(game.outcome.is_none());
    assert_eq!(game.ended_at, Some(1));
    game.start_end_game_timer();
    game.step();
    assert_eq!(game.tick, 2);
    for _ in 0..120 {
        game.step();
    }
    assert_eq!(game.tick, 121);
    assert!(game.move_to(1, [300.0, 300.0]).is_err());
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(game.ended_at.is_none());
    assert!(game.end_game_at.is_none());
    assert_eq!(game.defeated, [false, false, false]);
}

#[test]
fn global_outcome_waits_for_scripted_input_disable_and_end_timer() {
    let mut game = game();
    game.entities[2].health = 0.0;
    game.step();
    assert_eq!(game.outcome, Some(0));
    assert!(game.move_to(1, [300.0, 300.0]).is_ok());
    for _ in 0..180 {
        game.step();
    }
    assert_eq!(game.tick, 181);
    assert!(game.end_game_at.is_none());
    game.start_end_game_timer();
    assert!(game.move_to(1, [400.0, 400.0]).is_err());
    for _ in 0..121 {
        game.step();
    }
    assert_eq!(game.tick, 301);
    assert_eq!(game.outcome, Some(0));
}

#[test]
fn projectile_arrival_kills_producer_before_it_can_finish_a_queue() {
    let mut game = game();
    let builder = game.type_for(0, Kind::Builder).unwrap();
    let tank = game.type_for(1, Kind::Tank).unwrap();
    game.entities[0].health = 30.0; // A damaged structure is a realistic match state.
    game.train(0, builder).unwrap();
    let base = game.entities[0].position;
    // The fixture tank's source placement angle is zero (+X). Start west of
    // the target so this projectile-arrival test does not first require a turn.
    let attacker = game.spawn(tank, 1, [base[0] - 100.0, base[1]], true);
    game.attack(attacker, 0).unwrap();
    game.step();
    assert!(
        game.entities[0].alive(),
        "A fired shell must travel before applying damage"
    );
    game.step(); // Aim enters Fire before its next update launches the shell.
    assert_eq!(game.projectiles.len(), 1);
    for _ in 0..300 {
        game.step();
    }
    assert!(!game.entities[0].alive());
    assert!(game.entities[0].queue.is_empty());
    assert_eq!(
        game.entities
            .iter()
            .filter(|entity| entity.owner == 0)
            .count(),
        2
    );
    assert_eq!(game.outcome, Some(1));
    assert!(game.train(0, builder).is_err());
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.tick, 0);
    assert!(game.outcome.is_none());
    assert_eq!(game.players[0].cash, 10_000);
}

#[test]
fn movement_rejects_invalid_points_and_turns_before_advancing() {
    let mut game = game();
    for destination in [[f32::NAN, 0.0], [f32::INFINITY, 10.0]] {
        assert!(game.move_to(1, destination).is_err());
    }
    // Pathfinder clips a finite out-of-map request to a legal edge cell.
    for edge in [[-1.0, 0.0], [1000.0, 10.0]] {
        game.move_to(1, edge).unwrap();
        assert!(
            matches!(game.entities[1].ai.order, Order::Move(goal) if game.cell_at(goal).is_some())
        );
    }
    let start = game.entities[1].position;
    game.set_object_orientation(1, 0.0);
    game.move_to(1, [start[0] - 30.0, start[1]]).unwrap();
    game.step();
    assert!(distance_squared(game.entities[1].position, start) < f32::EPSILON);
    assert!(game.entities[1].facing.abs() > 0.0);
    for _ in 0..90 {
        game.step();
    }
    assert!(game.entities[1].position[0] < start[0] - 20.0);
}

#[test]
fn native_goal_claims_adjust_later_allies_without_moving_the_first_goal() {
    let mut game = game();
    let first = game.spawn(1, 0, [105.0, 105.0], true);
    let second = game.spawn(1, 0, [105.0, 125.0], true);
    let requested = [345.25, 105.75];
    game.materialize_selection_group(0, &[first]);
    game.move_to(first, requested).unwrap();
    game.materialize_selection_group(0, &[second]);
    game.move_to(second, requested).unwrap();
    let Order::Move(second_goal) = game.entities[second].ai.order else {
        panic!("Second move was not accepted")
    };
    assert!(distance_squared(second_goal, [355.0, 105.0]) < 0.001);
    let Order::Move(first_goal) = game.entities[first].ai.order else {
        panic!("First move was replaced")
    };
    assert!(distance_squared(first_goal, requested) < 0.001);
    for _ in 0..350 {
        game.step();
    }
    assert!(distance_squared(game.entities[first].position, requested) < 0.001);
    assert!(distance_squared(game.entities[second].position, second_goal) < 0.001);
    assert!(
        distance_squared(
            game.entities[first].position,
            game.entities[second].position
        ) > 90.0
    );
}

#[test]
fn goal_claims_cover_the_native_even_diameter_body_and_release_on_death() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].geometry.major = 12.0;
    game.catalog[tank].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    let large = game.spawn(tank, 0, [305.0, 105.0], true);
    let small = game.spawn(1, 0, [305.0, 145.0], true);
    let requested = [445.0, 105.0];
    game.move_to(large, requested).unwrap();
    game.move_to(small, requested).unwrap();
    let Order::Move(goal) = game.entities[small].ai.order else {
        panic!("Small unit has no move")
    };
    assert!(distance_squared(goal, [435.0, 115.0]) < 0.001);
    let large_cells = game.body_cells(large, game.movement_cell(large, requested).unwrap());
    assert!(!large_cells.contains(&game.movement_cell(small, goal).unwrap()));
    game.entities[large].health = 0.0;
    game.move_to(small, requested).unwrap();
    let Order::Move(goal) = game.entities[small].ai.order else {
        panic!("Released goal was rejected")
    };
    assert!(distance_squared(goal, requested) < 0.001);
}

#[test]
fn infantry_group_uses_native_columns_and_arrives_at_distinct_claimed_goals() {
    let mut game = ground_game();
    game.ai_rules.distance_requires_group = 500.0;
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[infantry].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    let mut members = Vec::new();
    for x in [245.0, 265.0] {
        for y in [435.0, 455.0, 475.0] {
            members.push(game.spawn(infantry, 0, [x, y], true));
        }
    }
    game.group_move_to(&members, [665.0, 455.0], false).unwrap();
    let goals: Vec<_> = members
        .iter()
        .map(|&id| {
            assert!(matches!(game.entities[id].ai.order, Order::FollowPath));
            {
                let point = game.entities[id].ai.waypoints.points.last().unwrap();
                [point[0], point[1]]
            }
        })
        .collect();
    for (index, &goal) in goals.iter().enumerate() {
        assert!(
            goals[..index]
                .iter()
                .all(|&other| distance_squared(goal, other) >= 99.0)
        );
    }
    for _ in 0..750 {
        game.step();
    }
    for (&id, goal) in members.iter().zip(goals) {
        assert_ground_arrival(&game, id, goal);
        assert!(matches!(game.entities[id].ai.order, Order::Idle));
    }
}

#[test]
fn mixed_group_keeps_group_snapping_after_infantry_columns_take_the_other_members() {
    let mut game = game();
    game.ai_rules.distance_requires_group = 500.0;
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let mut members: Vec<_> = [435.0, 455.0, 475.0]
        .into_iter()
        .map(|y| game.spawn(infantry, 0, [245.0, y], true))
        .collect();
    let vehicle = game.spawn(tank, 0, [265.0, 455.0], true);
    members.push(vehicle);
    let clicked = [665.25, 455.75];
    game.group_move_to(&members, clicked, false).unwrap();
    assert!(
        members[..3]
            .iter()
            .all(|&id| matches!(game.entities[id].ai.order, Order::FollowPath))
    );
    let Order::Move(goal) = game.entities[vehicle].ai.order else {
        panic!("The lone remaining vehicle needs the individual group branch")
    };
    // AIPathfind uses the complete group's count, so the lone remaining
    // vehicle must not receive the single-selection exact-click exception.
    assert!((goal[0].rem_euclid(CELL_SIZE) - 5.0).abs() < 0.001);
    assert!((goal[1].rem_euclid(CELL_SIZE) - 5.0).abs() < 0.001);
    assert!(distance_squared(goal, clicked) > 0.1);
}

#[test]
fn gathering_preserves_native_nearest_first_claims_instead_of_stacking() {
    let mut game = game();
    let members: Vec<_> = [[305.0, 405.0], [325.0, 405.0], [315.0, 425.0]]
        .into_iter()
        .map(|point| game.spawn(1, 0, point, true))
        .collect();
    game.group_move_to(&members, [315.0, 410.0], false).unwrap();
    let goals: Vec<_> = members
        .iter()
        .map(|&id| {
            let Order::Move(goal) = game.entities[id].ai.order else {
                panic!("Gather must use movement")
            };
            goal
        })
        .collect();
    for (index, &goal) in goals.iter().enumerate() {
        assert!(
            goals[..index]
                .iter()
                .all(|&other| distance_squared(goal, other) >= 99.0)
        );
    }
    for _ in 0..120 {
        game.step();
    }
    for (&id, goal) in members.iter().zip(goals) {
        assert!(distance_squared(game.entities[id].position, goal) < 0.001);
    }
}

#[test]
fn alt_appending_to_an_active_move_preserves_the_first_destination() {
    let mut game = ground_game();
    let unit = game.spawn(1, 0, [105.0, 105.0], true);
    let first = [305.25, 105.75];
    let second = [305.5, 205.5];
    game.materialize_selection_group(0, &[unit]);
    game.move_to(unit, first).unwrap();
    game.queue_move_to(unit, second).unwrap();
    assert_eq!(game.entities[unit].ai.waypoints.points.len(), 2);
    assert!(
        distance_squared(
            [
                game.entities[unit].ai.waypoints.points[0][0],
                game.entities[unit].ai.waypoints.points[0][1]
            ],
            first
        ) < 0.001
    );
    assert!(
        distance_squared(
            [
                game.entities[unit].ai.waypoints.points[1][0],
                game.entities[unit].ai.waypoints.points[1][1]
            ],
            second
        ) < 0.001
    );
    for _ in 0..350 {
        game.step();
    }
    assert!(matches!(game.entities[unit].ai.order, Order::Idle));
    let goal = game.path_position(unit, game.movement_cell(unit, second).unwrap());
    assert_ground_arrival(&game, unit, goal);
}

#[test]
fn intermediate_waypoint_does_not_usurp_another_units_final_goal() {
    let (mut game, mover) = crate::skirmish::ground_movement::tests::moving_tank();
    game.set_object_position(mover, [105.0, 105.0, 0.0]);
    let tank = game.entities[mover].type_index;
    let other = game.spawn(tank, 0, [305.0, 85.0], true);
    let corner = [305.0, 105.0];
    let final_goal = [305.0, 205.0];
    let node = crate::grid::LayerCell {
        cell: game.movement_cell(other, corner).unwrap(),
        layer: game.entities[other].layer,
    };
    // The other unit must own the corner before the first waypoint is issued.
    game.move_to(other, corner).unwrap();
    assert!(!game.destination_unclaimed_on(mover, node));
    assert!(game.destination_unclaimed_on(other, node));
    game.queue_move_to(mover, corner).unwrap();
    game.queue_move_to(mover, final_goal).unwrap();
    game.step();
    assert!(!game.destination_unclaimed_on(mover, node));
    assert!(game.destination_unclaimed_on(other, node));
    let (points, index) = game.entities[mover].waypoint_path().unwrap();
    assert_eq!(index, 0);
    assert_eq!(
        points[0].map(f32::to_bits),
        [corner[0], corner[1], 0.0].map(f32::to_bits)
    );
    let mut advanced = false;
    for _ in 0..900 {
        game.step();
        advanced |= game.entities[mover]
            .waypoint_path()
            .is_some_and(|(_, index)| index == 1);
        if matches!(game.entities[mover].ai.order, Order::Idle)
            && matches!(game.entities[other].ai.order, Order::Idle)
        {
            break;
        }
    }
    assert!(advanced);
    assert!(matches!(game.entities[mover].ai.order, Order::Idle));
    assert!(matches!(game.entities[other].ai.order, Order::Idle));
    let close = game.catalog[tank]
        .locomotor
        .ground
        .as_ref()
        .unwrap()
        .close_enough;
    assert!(distance_squared(game.entities[mover].position, final_goal) < close * close);
    assert!(distance_squared(game.entities[other].position, corner) < close * close);
}

#[test]
fn failed_nested_dozer_move_keeps_the_native_construction_task() {
    let mut game = ground_game();
    game.catalog[0].energy = 1;
    game.update_power();
    // Native task selection uses zero as "no order"; issue this player command
    // after the first real frame and reveal the build site with a real scout.
    game.step();
    let scout = game.type_for(0, Kind::Infantry).unwrap();
    game.spawn(scout, 0, [345.0, 185.0], true);
    game.update_visibility();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let site = game.placement_near(building_type, [345.0, 105.0]).unwrap();
    let building = game.build(1, building_type, site).unwrap();

    let wall = (0..60).map(|y| Cell { x: 25, y });
    game.grid = Grid::new(80, 60, wall.clone()).unwrap();
    game.configure_movement_terrain(
        crate::terrain::TerrainMap::classify(80, 60, wall, []).unwrap(),
    )
    .unwrap();
    let initial_position = game.entities[1].position;
    assert!(game.movement_path(1, initial_position, site).is_none());
    for _ in 0..30 {
        game.step();
    }
    // The primary movement can fail and become Idle. The separate native task
    // remains assigned, rather than being represented by the old Order::Build.
    assert_eq!(game.entities[1].build_task_target(), Some(building));
    assert!(
        initial_position[0] < 250.0 && game.entities[1].position[0] < 250.0,
        "builder crossed the cliff barrier"
    );
    assert!(game.entities[building].awaiting_construction());
    game.grid = Grid::new(80, 60, []).unwrap();
    game.configure_movement_terrain(crate::terrain::TerrainMap::classify(80, 60, [], []).unwrap())
        .unwrap();
    for _ in 0..900 {
        game.step();
        if game.entities[building].complete() {
            break;
        }
    }
    assert!(game.entities[building].complete());
}

#[test]
fn approach_uses_a_reachable_cell_when_nearest_candidate_is_isolated() {
    let mut game = game();
    let isolated = Cell { x: 18, y: 20 };
    game.navigation = Grid::new(
        80,
        60,
        [
            Cell { x: 17, y: 20 },
            Cell { x: 19, y: 20 },
            Cell { x: 18, y: 19 },
            Cell { x: 18, y: 21 },
        ],
    )
    .unwrap();
    let from = [165.0, 205.0];
    let target = [205.0, 205.0];
    assert!(
        game.navigation
            .path(Cell { x: 16, y: 20 }, isolated)
            .is_none()
    );
    let path = game.approach_path(1, from, target, 20.0).unwrap().optimized;
    assert!(path.back().map(|node| node.cell) != Some(isolated));
    assert!(
        path.iter()
            .all(|&cell| !game.navigation.is_blocked(cell.cell))
    );
    assert!(distance_squared(cell_center(path.back().unwrap().cell), target) <= 400.0);
}

#[test]
fn optimized_move_travels_directly_without_grid_center_turns() {
    let mut game = game();
    let unit = game.spawn(1, 0, [105.0, 105.0], true);
    game.entities[unit].facing = 0.6_f32.atan2(0.8);
    let destination = [425.25, 345.1875];
    game.materialize_selection_group(0, &[unit]);
    game.move_to(unit, destination).unwrap();
    for _ in 0..60 {
        game.step();
    }
    let position = game.entities[unit].position;
    assert!((position[0] - 153.0).abs() < 0.001);
    assert!((position[1] - 141.0).abs() < 0.001);
    for _ in 0..400 {
        game.step();
    }
    assert!(distance_squared(game.entities[unit].position, destination) < 0.001);
    assert!(matches!(game.entities[unit].ai.order, Order::Idle));
}

#[test]
fn air_locomotor_crosses_a_ground_wall_that_blocks_ground_orders() {
    let mut game = game();
    game.navigation = Grid::new(80, 60, (0..60).map(|y| Cell { x: 40, y })).unwrap();
    let unit = game.spawn(1, 0, [205.0, 105.0], true);
    let destination = [605.0, 105.0];
    assert!(game.move_to(unit, destination).is_err());
    game.catalog[1].movement_surface = MovementSurface::Air;
    game.move_to(unit, destination).unwrap();
    for _ in 0..410 {
        game.step();
    }
    assert!(distance_squared(game.entities[unit].position, destination) < 0.001);
}

#[test]
fn exact_docking_segment_retains_order_and_reaches_subcell_coordinate() {
    let mut game = game();
    let unit = game.spawn(1, 0, [105.0, 105.0], true);
    game.entities[unit].ai.order = Order::Harvest(0);
    let destination = [109.25, 108.5];
    let mut arrived = false;
    for _ in 0..60 {
        if game.travel_to_exact(unit, destination) {
            arrived = true;
            break;
        }
    }
    assert!(arrived);
    assert!(distance_squared(game.entities[unit].position, destination) < 0.001);
    assert!(matches!(game.entities[unit].ai.order, Order::Harvest(0)));
}

#[test]
fn source_dozer_and_rotated_power_plant_geometry_reaches_a_build_dock() {
    let mut game = ground_game();
    game.grid = Grid::new(120, 100, []).unwrap();
    game.movement_terrain = crate::terrain::TerrainMap::classify(120, 100, [], []).unwrap();
    // Installed AmericaVehicleDozer and AmericaPowerPlant footprint values,
    // at the actual positions from the failed Tournament Desert command check.
    game.catalog[1].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[1].geometry.major = 15.0;
    game.catalog[1].geometry.minor = 10.0;
    game.set_object_position(1, [925.0, 545.0, 0.0]);
    let building_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    game.catalog[building_type].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[building_type].geometry.major = 22.0;
    game.catalog[building_type].geometry.minor = 30.0;
    game.catalog[building_type].placement_angle = -std::f32::consts::FRAC_PI_4;
    game.catalog[building_type].energy = 5;
    game.rebuild_navigation();
    let power = game.players[0].power;
    let position = [845.0, 705.0];
    let scout = game.type_for(0, Kind::Infantry).unwrap();
    game.spawn(scout, 0, [845.0, 605.0], true);
    game.configure_visibility(visibility::VisibilityRules {
        cell_size: 40.0,
        unlook_persist_frames: 30,
        use_shroud: true,
    })
    .unwrap();
    game.step();
    let building = game.build(1, building_type, position).unwrap();
    game.step(); // Idle selects the new task.
    game.step(); // Pick submits the nested move to the selected dock.
    let Order::Move(dock) = game.entities[1].ai.order else {
        panic!("Missing nested move to the selected dock")
    };
    assert_eq!(game.entities[1].build_task_target(), Some(building));
    assert!(game.movement_path(1, [925.0, 545.0], dock).is_some());
    for _ in 0..1800 {
        game.step();
        if game.entities[building].complete() {
            break;
        }
    }
    assert!(game.entities[building].complete());
    let goal = game.path_position(1, game.movement_cell(1, dock).unwrap());
    assert_ground_arrival(&game, 1, goal);
    assert_eq!(game.players[0].power, power + 5);
}

#[test]
fn build_clear_path_uses_terrain_connectivity_without_structure_goal_restrictions() {
    let mut game = game();
    game.entities[1].position = [105.0, 105.0];
    game.grid = Grid::new(80, 60, (0..60).map(|y| Cell { x: 40, y })).unwrap();
    game.navigation = game.grid.with_obstacles((0..60).map(|y| Cell { x: 20, y }));
    assert!(
        game.movement_path(1, [105.0, 105.0], [305.0, 105.0])
            .is_none()
    );
    assert!(game.build_site_reachable(1, [305.0, 105.0]));
    assert!(!game.build_site_reachable(1, [505.0, 105.0]));
}

#[test]
fn flying_dock_probe_ignores_only_the_actual_dock_object() {
    let mut game = game();
    game.catalog[1].movement_surface = MovementSurface::Air;
    game.catalog[1].locomotor.legal_surfaces = 8;
    let unit = game.spawn(1, 0, [105.0, 105.0], true);
    let dock_id = game.supplies[0].object;
    let dock = game.object_footprint(dock_id);
    let center = [dock.position[0], dock.position[1], 0.0];
    game.update_ground_contacts();
    assert_eq!(
        game.find_native_dock_position(unit, dock_id, center)
            .map(f32::to_bits),
        center.map(f32::to_bits)
    );

    fixture_warehouse(&mut game, dock.position, dock.major, 400, false);
    game.update_ground_contacts();
    let [x, y, _] = game.find_native_dock_position(unit, dock_id, center);
    assert!(
        distance_squared([x, y], dock.position) > (dock.major + 5.0).powi(2),
        "colocated second warehouse must block the flyer at {x}, {y}"
    );
}

#[test]
fn deleted_pile_releases_its_build_keepout_but_empty_warehouse_does_not() {
    let mut game = game();
    let supply_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    let position = [245.0, 175.0];
    assert!(!game.can_place(supply_type, position));
    game.supplies[0].boxes = 0;
    assert!(!game.can_place(supply_type, position));
    let warehouse = game.supplies[0].object;
    let kind = game.entities[warehouse].type_index;
    game.catalog[kind]
        .supply
        .warehouse
        .as_mut()
        .unwrap()
        .delete_when_empty = true;
    game.destroy_maintenance_object(warehouse);
    // GameLogic::destroyObject queues the destructor. Object::~Object removes
    // the pathfinder obstacle only when that queue is drained.
    assert!(!game.can_place(supply_type, position));
    game.process_update_destruction();
    assert!(game.can_place(supply_type, position));
}

#[test]
fn loaded_collector_can_return_after_depot_empties_and_placement_preserves_access() {
    let mut game = ground_game();
    let supply = game.type_for(0, Kind::SupplyCenter).unwrap();
    let cash = game.players[0].cash;
    assert!(game.build(1, supply, game.supply_position(0)).is_err());
    assert_eq!(game.players[0].cash, cash);
    let site = game.placement_near(supply, [185.0, 245.0]).unwrap();
    let center = game.spawn(supply, 0, site, true);
    game.native_build_complete(center);
    game.update_visibility();
    game.entities[1].cargo = 1; // A collector stopped after picking up the final box.
    game.supplies[0].boxes = 0;
    assert!(game.harvest(1, 0).is_err());
    game.deliver_supplies(1, center).unwrap();
    assert!(matches!(game.entities[1].ai.order, Order::ReturnSupply));
    for _ in 0..600 {
        game.step();
        // Observe this delivery before native autopilot begins the next trip.
        if game.entities[1].cargo == 0 {
            break;
        }
    }
    assert_eq!(game.entities[1].cargo, 0);
    assert_eq!(game.players[0].cash, cash + 75);
    assert!(game.harvest(1, 0).is_err());
}

#[test]
fn collector_docking_flags_follow_entry_processing_exit_and_cancel() {
    use super::docking_flags::{ACTIVE, BEGINNING, DOCKING, ENDING};
    let mut game = ground_game();
    game.harvest(1, 0).unwrap();
    let mut beginning_seen = false;
    let mut active_seen = false;
    for _ in 0..900 {
        game.step();
        let flags = game.entities[1].docking_flags;
        if flags & BEGINNING != 0 {
            beginning_seen = true;
            assert!(flags & DOCKING != 0);
            assert!(game.entities[game.supplies[0].object].docking_flags & BEGINNING != 0);
            assert!(flags & ACTIVE == 0);
        }
        if flags & ACTIVE != 0 {
            active_seen = true;
            assert!(flags & DOCKING != 0);
            assert!(game.entities[game.supplies[0].object].docking_flags & ACTIVE != 0);
        }
        if flags & ENDING != 0 {
            assert!(beginning_seen && active_seen);
            assert_eq!(flags & (DOCKING | ACTIVE), 0);
            assert_eq!(game.entities[1].cargo, 1);
            assert!(game.entities[game.supplies[0].object].docking_flags & ENDING != 0);
            break;
        }
    }
    assert!(game.entities[1].docking_flags & ENDING != 0);
    game.harvest(1, 0).unwrap();
    for _ in 0..900 {
        game.step();
        if game.entities[1].docking_flags & ACTIVE != 0 {
            game.stop(1).unwrap();
            let flags = game.entities[1].docking_flags;
            assert_eq!(flags, 0);
            let flags = game.entities[game.supplies[0].object].docking_flags;
            assert_eq!(flags, 0);
            return;
        }
    }
    panic!("collector did not begin its second source docking");
}

#[test]
fn direct_supply_commands_replace_the_single_preferred_dock() {
    let mut game = game();
    let supply_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    let near = game.spawn(supply_type, 0, [225.0, 305.0], true);
    let far = game.spawn(supply_type, 0, [405.0, 205.0], true);
    game.native_build_complete(near);
    game.native_build_complete(far);
    game.update_visibility();
    game.entities[1].cargo = 1;
    assert_eq!(game.closest_supply_center(1), Some(near));
    game.deliver_supplies(1, far).unwrap();
    assert_eq!(game.closest_supply_center(1), Some(far));
    game.harvest(1, 0).unwrap();
    assert_eq!(game.closest_supply_center(1), Some(near));
    assert!(matches!(game.entities[1].ai.order, Order::Harvest(0)));
    game.entities[1].cargo = 0;
    assert!(game.deliver_supplies(1, far).is_err());
}

#[test]
fn depleted_supply_releases_dock_and_retargets_within_source_scan_distance() {
    let mut game = ground_game();
    // A human collector only considers a revealed warehouse.
    game.spawn(1, 0, [585.0, 130.0], true);
    game.update_visibility();
    let supply_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    let site = game.placement_near(supply_type, [185.0, 245.0]).unwrap();
    let center = game.spawn(supply_type, 0, site, true);
    game.native_build_complete(center);
    game.update_visibility();
    game.supplies[0].boxes = 1;
    let warehouse = game.supplies[0].object;
    let kind = game.entities[warehouse].type_index;
    game.catalog[kind]
        .supply
        .warehouse
        .as_mut()
        .unwrap()
        .delete_when_empty = true;
    game.harvest(1, 0).unwrap();
    let initial = game.players[0].cash;
    for _ in 0..900 {
        game.step();
        if game.players[0].cash > initial && matches!(game.entities[1].ai.order, Order::Harvest(1))
        {
            break;
        }
    }
    assert_eq!(game.supplies[0].boxes, 0);
    assert!(game.players[0].cash >= initial + 75);
    assert_eq!(game.best_supply(1), Some(1));
    assert!(matches!(game.entities[1].ai.order, Order::Harvest(1)));
    assert!(matches!(
        game.entities[1].ai.docking.as_ref().map(|dock| dock.target),
        Some(economy::DockTarget::Warehouse(1))
    ));
}

#[test]
fn a_small_pile_disappearing_during_approach_cancels_its_reservation() {
    let mut game = ground_game();
    // A human collector only considers a revealed warehouse.
    game.spawn(1, 0, [585.0, 130.0], true);
    game.update_visibility();
    let warehouse = game.supplies[0].object;
    let kind = game.entities[warehouse].type_index;
    game.catalog[kind]
        .supply
        .warehouse
        .as_mut()
        .unwrap()
        .delete_when_empty = true;
    game.harvest(1, 0).unwrap();
    game.step();
    assert!(game.entities[1].ai.docking.is_some());
    // Another collector takes the last box while this one is approaching.
    game.supplies[0].boxes = 1;
    let collector_type = game.entities[1].type_index;
    let other = game.spawn(collector_type, 0, game.supply_position(0), true);
    assert!(!game.native_warehouse_action(0, other));
    for _ in 0..300 {
        game.step();
        if matches!(game.entities[1].ai.order, Order::Harvest(1)) {
            break;
        }
    }
    assert!(matches!(game.entities[1].ai.order, Order::Harvest(1)));
    assert!(matches!(
        game.entities[1].ai.docking.as_ref().map(|dock| dock.target),
        Some(economy::DockTarget::Warehouse(1))
    ));
    assert_eq!(game.entities[1].cargo, 0);
}

#[test]
fn docked_builder_preserves_source_percent_arithmetic_at_completion_boundary() {
    let mut game = ground_game();
    game.catalog[0].energy = 1;
    game.update_power();
    game.step();
    let barracks = game.type_for(0, Kind::Barracks).unwrap();
    let site = [275.0, 305.0];
    game.set_object_position(1, [310.0, 305.0, 0.0]);
    let building = game.build(1, barracks, site).unwrap();
    for _ in 0..300 {
        game.step();
        if game.entities[building].construction_percent > 0.0 {
            break;
        }
    }
    assert_eq!(
        game.entities[building].construction_percent.to_bits(),
        (100.0_f32 / 300.0).to_bits()
    );
    for _ in 1..299 {
        game.step();
    }
    assert!(!game.entities[building].complete());
    game.step();
    assert!(
        game.entities[building].complete(),
        "Source adds100/300 percent each frame; normalized1/300 accumulates a different rounding result"
    );
}

#[test]
fn rotated_non_square_building_checks_its_actual_corners_and_navigation_cells() {
    let mut game = game();
    let factory = game.type_for(0, Kind::Factory).unwrap();
    // Installed GLAArmsDealer geometry and placement orientation.
    game.catalog[factory].geometry.major = 40.0;
    game.catalog[factory].geometry.minor = 62.0;
    game.catalog[factory].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[factory].placement_angle = (-135.0_f32).to_radians();
    let position = [385.0, 305.0];
    assert!(game.can_place(factory, position));
    let corner_cell = Cell { x: 44, y: 29 };
    assert!(
        game.catalog[factory]
            .footprint(position, game.catalog[factory].placement_angle)
            .contains(cell_center(corner_cell))
    );
    game.grid = Grid::new(80, 60, [corner_cell]).unwrap();
    assert!(
        !game.can_place(factory, position),
        "The long rotated edge reaches beyond the old major-radius square"
    );
    // A cell in the axis-aligned bounding box but outside the rotated shape is legal.
    game.grid = Grid::new(80, 60, [Cell { x: 33, y: 24 }]).unwrap();
    assert!(game.can_place(factory, position));
    game.layout.obstacles.push(Footprint {
        position: cell_center(corner_cell),
        angle: 0.0,
        major: 3.0,
        minor: 3.0,
        shape: crate::collision_geometry::Shape::Box,
    });
    assert!(!game.can_place(factory, position));
    game.layout.obstacles.clear();
    game.spawn(factory, 0, position, true);
    game.rebuild_navigation();
    assert!(game.navigation.is_blocked(corner_cell));
}

#[test]
fn supply_build_border_uses_source_flag_and_expanded_rotated_depot() {
    let mut game = game();
    let supply = game.type_for(0, Kind::SupplyCenter).unwrap();
    let factory = game.type_for(0, Kind::Factory).unwrap();
    let point = [295.0, 165.0];
    let existing: Vec<_> = game.supplies.iter().map(|supply| supply.object).collect();
    for object in existing {
        game.destroy_maintenance_object(object);
    }
    game.process_update_destruction();
    let source = fixture_warehouse(&mut game, [405.0, 165.0], 50.0, 400, false);
    let object = game.supplies[source].object;
    let kind = game.entities[object].type_index;
    game.catalog[kind].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[kind].geometry.minor = 61.0;
    game.set_object_orientation(object, std::f32::consts::FRAC_PI_4);
    game.unregister_object_obstacle(object);
    game.restore_object_obstacle(object);
    assert!(
        !game.can_place(supply, point),
        "Supply center overlaps the expanded source dock rectangle"
    );
    assert!(
        game.can_place(factory, point),
        "An object without CANNOT_BUILD_NEAR_SUPPLIES must not receive the invented global keepout"
    );
    game.rules.supply_build_border = 0.0;
    assert!(game.can_place(supply, point));
}

#[test]
fn starting_dozer_clears_the_native_rotated_command_center_without_moving_the_waypoint() {
    let mut game = game();
    game.grid = Grid::new(204, 270, []).unwrap();
    game.configure_movement_terrain(
        crate::terrain::TerrainMap::classify(204, 270, [], []).unwrap(),
    )
    .unwrap();
    game.layout.starts = vec![
        PlayerStart {
            position: [1004.8322, 583.634],
            rally: None,
        },
        PlayerStart {
            position: [1010.8063, 2102.689],
            rally: None,
        },
    ];
    let center = game.type_for(0, Kind::CommandCenter).unwrap();
    let builder = game.type_for(0, Kind::Builder).unwrap();
    // Installed USA command-center and dozer geometry, on Tournament Desert's
    // exact native start waypoints. The previous nearest-cell exit overlapped.
    game.catalog[center].geometry.major = 60.0;
    game.catalog[center].geometry.minor = 70.0;
    game.catalog[center].geometry.height = 49.0;
    game.catalog[center].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[center].placement_angle = -3.0 * PI / 4.0;
    game.catalog[builder].geometry.major = 15.0;
    game.catalog[builder].geometry.minor = 10.0;
    game.catalog[builder].geometry.height = 15.0;
    game.catalog[builder].geometry.shape = crate::collision_geometry::Shape::Box;
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    let owned = |game: &Skirmish, owner, kind| {
        game.entities
            .iter()
            .position(|actor| actor.owner == owner && game.catalog[actor.type_index].kind == kind)
            .expect("Starting player object")
    };
    for owner in 0..2 {
        let base = &game.entities[owned(&game, owner, Kind::CommandCenter)];
        let dozer = &game.entities[owned(&game, owner, Kind::Builder)];
        assert!(distance_squared(base.position, game.layout.starts[owner].position) < f32::EPSILON);
        let base_shape = game.catalog[base.type_index].footprint(base.position, base.facing);
        let dozer_shape = game.catalog[dozer.type_index].footprint(dozer.position, dozer.facing);
        assert!(!super::movement::footprints_overlap(
            &base_shape,
            &dozer_shape
        ));
        assert!(
            !game
                .navigation
                .is_blocked(game.cell_at(dozer.position).unwrap())
        );
    }
    let exits = [0, 1].map(|owner| game.entities[owned(&game, owner, Kind::Builder)].position);
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    for (owner, position) in exits.into_iter().enumerate() {
        let id = owned(&game, owner, Kind::Builder);
        assert!(distance_squared(position, game.entities[id].position) < f32::EPSILON);
    }
}

#[test]
fn starting_builder_search_fails_on_blocked_rings_instead_of_moving_the_base() {
    let mut game = game();
    game.movement_terrain = crate::terrain::TerrainMap::classify(
        80,
        60,
        (0..60).flat_map(|y| (0..80).map(move |x| Cell { x, y })),
        [],
    )
    .unwrap();
    assert!(game.reset(Faction::Gla, Faction::Gla).is_err());
    let player_objects: Vec<_> = game
        .entities
        .iter()
        .filter(|actor| actor.owner == 0)
        .collect();
    assert_eq!(player_objects.len(), 1);
    assert!(
        distance_squared(player_objects[0].position, game.layout.starts[0].position) < f32::EPSILON
    );
}

#[test]
fn rotated_arms_dealer_exits_from_native_points_instead_of_the_trapped_north_cell() {
    let mut game = ground_game();
    // Reproduce the observed isolated-cell connectivity at the installed test's
    // old [795,2225] spawn in this fixture's navigation grid.
    let cliffs = [
        Cell { x: 80, y: 222 },
        Cell { x: 79, y: 223 },
        Cell { x: 78, y: 222 },
        Cell { x: 79, y: 221 },
    ];
    game.grid = Grid::new(204, 270, cliffs).unwrap();
    game.configure_movement_terrain(
        crate::terrain::TerrainMap::classify(204, 270, cliffs, []).unwrap(),
    )
    .unwrap();
    let factory_type = game.type_for(1, Kind::Factory).unwrap();
    game.catalog[factory_type].energy = 1;
    let tank_type = game.type_for(1, Kind::Tank).unwrap();
    let factory = &mut game.catalog[factory_type];
    factory.geometry.major = 40.0;
    factory.geometry.minor = 62.0;
    factory.geometry.height = 25.0;
    factory.geometry.shape = crate::collision_geometry::Shape::Box;
    factory.placement_angle = -3.0 * PI / 4.0;
    factory.factory_bib.exit_width = 25.0;
    factory.behavior.production_exit = Some(ProductionExit {
        use_spawn_rally_point: false,
        queue: None,
        create: [-20.0, -15.0, 0.0],
        rally: [40.0, -15.0, 0.0],
        snap_rally: false,
        extend_rally: true,
    });
    let producer = game.spawn(factory_type, 1, [795.0, 2165.0], true);
    game.update_power();
    game.rebuild_navigation();
    assert!(
        game.navigation
            .path(Cell { x: 79, y: 222 }, Cell { x: 73, y: 222 })
            .is_none()
    );
    let first_new = game.entities.len();
    game.train(producer, tank_type).unwrap();
    for _ in 0..150 {
        game.step();
    }
    assert_eq!(game.entities.len(), first_new + 1);
    let created = &game.entities[first_new];
    let expected =
        super::production_exit::transform_point([795.0, 2165.0], -3.0 * PI / 4.0, [-20.0, -15.0]);
    assert!(distance_squared(created.position, expected) < 0.001);
    assert!(
        game.catalog[factory_type]
            .footprint([795.0, 2165.0], -3.0 * PI / 4.0)
            .contains(created.position)
    );
    assert_eq!(created.ai.ignored_producer, Some(producer));
    assert!(matches!(created.ai.order, Order::Exit(_)));
    let start = created.position;
    // (-135-degree rotation of (40,-15) extended by 20 along its normalized
    // direction) + the native producer position, within source f32 precision.
    let expected_rally = [737.9018, 2139.0464];
    let direction = [expected_rally[0] - start[0], expected_rally[1] - start[1]];
    for _ in 0..900 {
        if game.entities[first_new].ai.ignored_producer.is_none() {
            break;
        }
        game.step();
        let position = game.entities[first_new].position;
        let products = [
            (position[0] - start[0]) * direction[1],
            (position[1] - start[1]) * direction[0],
        ];
        assert!(
            (products[0] - products[1]).abs() < 0.5,
            "Factory quick path must stay on its source segment, not make grid detours"
        );
    }
    let tank = &game.entities[first_new];
    assert!(
        tank.ai.ignored_producer.is_none(),
        "Tank must actually finish its producer-ignored exit path"
    );
    assert_ground_arrival(&game, first_new, expected_rally);
    assert!(
        !game.catalog[factory_type]
            .footprint([795.0, 2165.0], -3.0 * PI / 4.0)
            .contains(tank.position)
    );
    let destination = [tank.position[0] - 60.0, tank.position[1]];
    assert!(game.move_to(first_new, destination).is_ok());
}

#[test]
fn leaving_production_exit_releases_the_producer_ignore_for_new_commands() {
    let mut game = game();
    let factory_type = game.type_for(1, Kind::Factory).unwrap();
    let tank_type = game.type_for(1, Kind::Tank).unwrap();
    let producer = game.spawn(factory_type, 1, [405.0, 405.0], true);
    game.rebuild_navigation();
    let stopped = game
        .spawn_produced_unit(producer, tank_type, false)
        .unwrap();
    game.stop(stopped).unwrap();
    assert!(game.entities[stopped].ai.ignored_producer.is_none());
    assert!(matches!(game.entities[stopped].ai.order, Order::Idle));
    let moved = game
        .spawn_produced_unit(producer, tank_type, false)
        .unwrap();
    game.move_to(moved, [485.0, 405.0]).unwrap();
    assert!(game.entities[moved].ai.ignored_producer.is_none());
    assert!(matches!(game.entities[moved].ai.order, Order::Move(_)));
    let attacking = game
        .spawn_produced_unit(producer, tank_type, false)
        .unwrap();
    game.attack(attacking, 0).unwrap();
    assert!(game.entities[attacking].ai.ignored_producer.is_none());
    assert!(matches!(
        game.entities[attacking].ai.order,
        Order::Attack(0)
    ));
    let supply_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    game.spawn(supply_type, 0, [345.0, 165.0], true);
    let builder_type = game.type_for(0, Kind::Builder).unwrap();
    let collector = game.spawn_produced_unit(0, builder_type, false).unwrap();
    game.update_visibility();
    game.harvest(collector, 0).unwrap();
    assert!(game.entities[collector].ai.ignored_producer.is_none());
    assert!(matches!(
        game.entities[collector].ai.order,
        Order::Harvest(0)
    ));
}

#[test]
fn stopped_unit_can_leave_its_occupied_factory_start_without_restoring_ignore() {
    let mut game = game();
    let factory_type = game.type_for(1, Kind::Factory).unwrap();
    let tank_type = game.type_for(1, Kind::Tank).unwrap();
    let producer = game.spawn(factory_type, 1, [405.0, 405.0], true);
    game.rebuild_navigation();
    let tank = game
        .spawn_produced_unit(producer, tank_type, false)
        .unwrap();
    for _ in 0..5 {
        game.step();
    }
    assert!(
        game.navigation
            .is_blocked(game.cell_at(game.entities[tank].position).unwrap())
    );
    game.stop(tank).unwrap();
    assert!(game.entities[tank].ai.ignored_producer.is_none());
    let destination = [485.0, 405.0];
    game.move_to(tank, destination).unwrap();
    for _ in 0..180 {
        game.step();
    }
    assert!(distance_squared(game.entities[tank].position, destination) < 0.001);
    assert!(game.entities[tank].ai.ignored_producer.is_none());
    assert!(
        game.navigation.is_blocked(Cell { x: 40, y: 40 }),
        "The factory obstruction remains; only the trapped start search may tunnel out"
    );
}

#[test]
fn free_collector_uses_parent_exit_before_earning_supply_income() {
    let mut game = ground_game();
    let supply_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    game.catalog[supply_type].placement_angle = PI / 2.0;
    game.catalog[supply_type].behavior.production_exit = Some(ProductionExit {
        use_spawn_rally_point: false,
        queue: None,
        create: [0.0, 0.0, 0.0],
        rally: [0.0, 35.0, 0.0],
        snap_rally: false,
        extend_rally: false,
    });
    let producer = game.spawn(supply_type, 0, [345.0, 175.0], true);
    game.native_build_complete(producer);
    game.rebuild_navigation();
    let collector = game.entities.len();
    let cash = game.players[0].cash;
    game.step();
    assert_eq!(game.entities.len(), collector + 1);
    assert!(distance_squared(game.entities[collector].position, [345.0, 175.0]) < 0.001);
    let Order::Exit(rally) = game.entities[collector].ai.order else {
        panic!("Free collector must use its parent's production exit");
    };
    assert!(distance_squared([rally[0], rally[1]], [310.0, 175.0]) < 0.001);
    assert_eq!(game.entities[collector].ai.ignored_producer, Some(producer));
    assert_eq!(game.players[0].cash, cash);
    for _ in 0..1800 {
        game.step();
    }
    assert!(game.entities[collector].ai.ignored_producer.is_none());
    assert!(
        game.players[0].cash > cash,
        "Collector must leave and deliver real supply boxes"
    );
    assert_eq!(
        game.entities
            .iter()
            .filter(
                |entity| entity.owner == 0 && game.catalog[entity.type_index].kind == Kind::Builder
            )
            .count(),
        2
    );
}

#[test]
fn factory_exit_rectangle_prevents_a_building_from_blocking_the_native_exit() {
    let mut game = game();
    let factory_type = game.type_for(0, Kind::Factory).unwrap();
    let barracks_type = game.type_for(0, Kind::Barracks).unwrap();
    let factory = &mut game.catalog[factory_type];
    factory.geometry.shape = crate::collision_geometry::Shape::Box;
    factory.geometry.major = 40.0;
    factory.geometry.minor = 62.0;
    factory.placement_angle = -3.0 * PI / 4.0;
    factory.factory_bib.exit_width = 25.0;
    let barracks = &mut game.catalog[barracks_type];
    barracks.geometry.shape = crate::collision_geometry::Shape::Box;
    barracks.geometry.major = 5.0;
    barracks.geometry.minor = 5.0;
    let center = [405.0, 405.0];
    let blocker = super::production_exit::transform_point(center, -3.0 * PI / 4.0, [55.0, 0.0]);
    game.spawn(barracks_type, 0, blocker, true);
    assert!(!game.can_place(factory_type, center));
    game.catalog[factory_type].factory_bib.exit_width = 0.0;
    assert!(
        game.can_place(factory_type, center),
        "Only the native reserved exit area intersects the blocker"
    );
}

#[test]
fn idle_scan_honors_flag_cadence_and_human_does_not_chase() {
    let mut game = ground_game();
    game.ai_rules.force_idle_frames = 5;
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let worker = game.type_for(1, Kind::Builder).unwrap();
    game.catalog[tank].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    let actor = game.spawn(tank, 0, [305.0, 405.0], true);
    let target = game.spawn(worker, 1, [405.0, 405.0], true);
    for _ in 0..10 {
        game.step();
    }
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    game.catalog[tank].ai.acquisition |= crate::skirmish::UnitAi::IDLE;
    game.stop(actor).unwrap();
    for _ in 0..4 {
        game.step();
    }
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    game.step();
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(id) if id == target));
    game.set_object_position(target, [505.0, 405.0, 0.0]);
    game.update_ground_contacts();
    let origin = game.entities[actor].position;
    game.step();
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    assert!(distance_squared(origin, game.entities[actor].position) < f32::EPSILON);
    game.attack(actor, target).unwrap();
    for _ in 0..90 {
        game.step();
    }
    assert!(distance_squared(origin, game.entities[actor].position) > 100.0);
}

#[test]
fn idle_acquisition_excludes_unarmed_buildings_unless_enabled() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let center = game.type_for(1, Kind::CommandCenter).unwrap();
    let actor = game.spawn(tank, 0, [305.0, 505.0], true);
    let building = game.spawn(center, 1, [405.0, 505.0], true);
    for _ in 0..180 {
        game.step();
    }
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    game.catalog[tank].ai.acquisition |= crate::skirmish::UnitAi::BUILDINGS;
    game.stop(actor).unwrap();
    game.step();
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(id) if id == building));
}

#[test]
fn attack_move_fights_then_resumes_its_retained_destination() {
    let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
    game.set_object_position(actor, [305.0, 505.0, 0.0]);
    let worker = game.type_for(1, Kind::Builder).unwrap();
    let target = game.spawn(worker, 1, [365.0, 505.0], true);
    game.attack_move(actor, [455.0, 505.0]).unwrap();
    for _ in 0..900 {
        game.step();
    }
    assert!(!game.entities[target].alive());
    // Native CloseEnoughDist=1 ends movement without snapping the world pose.
    assert!(distance_squared(game.entities[actor].position, [455.0, 505.0]) < 1.0);
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
}

#[test]
fn waypoints_preserve_requested_goals_advance_and_cancel_on_normal_orders() {
    let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
    game.set_object_position(actor, [305.0, 405.0, 0.0]);
    let first = [405.0, 405.0];
    let second = [405.0, 505.0];
    game.queue_move_to(actor, first).unwrap();
    game.queue_move_to(actor, second).unwrap();
    let requested = [[first[0], first[1], 0.0], [second[0], second[1], 0.0]];
    let (points, index) = game.entities[actor].waypoint_path().unwrap();
    assert_eq!(index, 0);
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].map(f32::to_bits), requested[0].map(f32::to_bits));
    assert_eq!(points[1].map(f32::to_bits), requested[1].map(f32::to_bits));
    for _ in 0..900 {
        game.step();
        if game.entities[actor]
            .waypoint_path()
            .is_some_and(|(_, index)| index == 1)
        {
            break;
        }
    }
    let (points, index) = game.entities[actor].waypoint_path().unwrap();
    assert_eq!(index, 1);
    assert_eq!(
        points[index].map(f32::to_bits),
        requested[1].map(f32::to_bits)
    );
    assert_eq!(
        game.entities[actor]
            .ai
            .command
            .move_to
            .position
            .map(f32::to_bits),
        requested[1].map(f32::to_bits)
    );
    for _ in 0..900 {
        game.step();
        if game.entities[actor].waypoint_path().is_none() {
            break;
        }
    }
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    let unit = &game.catalog[game.entities[actor].type_index];
    let close = unit.locomotor.ground.as_ref().unwrap().close_enough;
    assert!(distance_squared(game.entities[actor].position, second) < close * close);
    game.queue_move_to(actor, first).unwrap();
    game.move_to(actor, [505.0, 505.0]).unwrap();
    assert!(game.entities[actor].waypoint_path().is_none());
    game.queue_move_to(actor, first).unwrap();
    game.stop(actor).unwrap();
    assert!(game.entities[actor].waypoint_path().is_none());
}

#[test]
fn blocked_guard_center_stays_exact_while_only_the_return_goal_is_adjusted() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let actor = game.spawn(tank, 0, [305.0, 505.0], true);
    let center = [185.25, 305.125]; // Inside the original friendly command center.
    game.guard(actor, center).unwrap();
    assert!(
        matches!(game.entities[actor].ai.order, Order::Guard(point) if point.map(f32::to_bits) == center.map(f32::to_bits))
    );
    let returned = game.entities[actor].ai.guard.return_position;
    assert_ne!(
        [returned[0], returned[1]].map(f32::to_bits),
        center.map(f32::to_bits)
    );
    for _ in 0..60 {
        game.step();
    }
    assert!(
        matches!(game.entities[actor].ai.order, Order::Guard(point) if point.map(f32::to_bits) == center.map(f32::to_bits))
    );
}

#[test]
fn player_guard_edge_clipping_keeps_source_nearest_edge_and_corner_tie_order() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let actor = game.spawn(tank, 0, [305.0, 505.0], true);
    for (point, expected) in [
        ([-10.0, 300.0], [0.0, 300.0]),
        ([810.0, 300.0], [800.0, 300.0]),
        ([400.0, -10.0], [400.0, 0.0]),
        ([400.0, 610.0], [400.0, 600.0]),
        ([-10.0, -10.0], [-10.0, 0.0]),
        ([400.125, 599.75], [400.125, 599.75]),
    ] {
        game.guard(actor, point).unwrap();
        assert!(
            matches!(game.entities[actor].ai.order, Order::Guard(center) if center.map(f32::to_bits) == expected.map(f32::to_bits))
        );
    }
    assert!(game.guard(actor, [f32::NAN, 30.0]).is_err());
    assert!(game.guard(0, [30.0, 30.0]).is_err());
}

#[test]
fn point_guard_pursues_within_its_leash_then_returns_to_guard_center() {
    let mut game = ground_game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let worker = game.type_for(1, Kind::Builder).unwrap();
    game.ai_rules.guard_outer_human = 1.5;
    let actor = game.spawn(tank, 0, [305.0, 505.0], true);
    let target = game.spawn(worker, 1, [425.0, 505.0], true);
    game.guard(actor, [305.0, 505.0]).unwrap();
    for _ in 0..40 {
        game.step();
    }
    assert_eq!(game.entities[actor].ai.ai_target, Some(target));
    // Source guard tracks the same nemesis outside the inner range; after it
    // leaves the outer range the unit returns, preserving its guard order.
    game.set_object_position(target, [505.0, 505.0, 0.0]);
    game.update_ground_contacts();
    for _ in 0..40 {
        game.step();
    }
    assert!(game.entities[actor].position[0] > 305.0);
    game.set_object_position(target, [755.0, 505.0, 0.0]);
    game.update_ground_contacts();
    for _ in 0..300 {
        game.step();
    }
    assert!(matches!(game.entities[actor].ai.order, Order::Guard(_)));
    assert_ground_arrival(&game, actor, [305.0, 505.0]);
}

#[test]
fn produced_unit_completes_native_exit_before_following_user_rally() {
    let mut game = ground_game();
    let builder = game.type_for(0, Kind::Builder).unwrap();
    game.set_rally_point(0, [405.0, 405.0, 0.0]).unwrap();
    let path = game.producer_rally_path(0).unwrap();
    assert_eq!(path.len(), 3);
    assert!(distance_squared([path[1][0], path[1][1]], [220.0, 305.0]) < 0.001);
    let actor = game.spawn_produced_unit(0, builder, false).unwrap();
    let Order::Exit(natural) = game.entities[actor].ai.order else {
        panic!("Missing exit order")
    };
    assert!(distance_squared([natural[0], natural[1]], [240.0, 305.0]) < 0.001);
    let user_rally = game.entities[actor]
        .ai
        .waypoints
        .points
        .get(1)
        .copied()
        .unwrap();
    for _ in 0..900 {
        game.step();
        // A produced GLA worker resumes source supply scanning after its exit
        // finishes. Observe exit completion before that next order moves it.
        if !matches!(game.entities[actor].ai.order, Order::Exit(_)) {
            break;
        }
    }
    assert!(game.entities[actor].ai.ignored_producer.is_none());
    assert_ground_arrival(&game, actor, [user_rally[0], user_rally[1]]);
}

#[test]
fn guard_idle_damage_hook_uses_source_chase_deadline_before_next_scan() {
    let mut game = ground_game();
    game.ai_rules.guard_chase_frames = 7;
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let actor = game.spawn(tank, 0, [305.0, 505.0], true);
    game.guard(actor, [305.0, 505.0]).unwrap();
    for _ in 0..30 {
        game.step();
        if matches!(game.entities[actor].ai.guard.phase, ai::GuardPhase::Idle) {
            break;
        }
    }
    assert!(matches!(
        game.entities[actor].ai.guard.phase,
        ai::GuardPhase::Idle
    ));
    // Isolate the asynchronous damaged-by hook from the ordinary scan timer.
    game.entities[actor].ai.guard.next_scan = game.tick + 1000;
    let attacker = game.spawn(tank, 1, [405.0, 505.0], true);
    game.attack(attacker, actor).unwrap();
    for _ in 0..60 {
        game.step();
        if matches!(
            game.entities[actor].ai.guard.phase,
            ai::GuardPhase::Aggressor
        ) {
            break;
        }
    }
    assert!(game.entities[actor].health < game.catalog[tank].initial_health);
    assert!(matches!(
        game.entities[actor].ai.guard.phase,
        ai::GuardPhase::Aggressor
    ));
    assert_eq!(game.entities[actor].ai.ai_target, Some(attacker));
    assert_eq!(game.entities[actor].ai.guard.deadline, game.tick + 7);
    assert!(game.entities[actor].ai.guard.next_scan < game.tick + 1000);
}

#[test]
fn unarmed_member_accepts_attack_move_without_acquiring_or_firing() {
    let (mut game, tank) = crate::skirmish::ground_movement::tests::moving_tank();
    let ground = game.catalog[game.entities[tank].type_index]
        .locomotor
        .ground
        .take();
    let worker = game.type_for(0, Kind::Builder).unwrap();
    game.catalog[worker].locomotor.ground = ground;
    let actor = game.spawn(worker, 0, [305.0, 505.0], true);
    let enemy = game.spawn(worker, 1, [365.0, 505.0], true);
    game.attack_move(actor, [455.0, 505.0]).unwrap();
    for _ in 0..240 {
        game.step();
    }
    // Native CloseEnoughDist=1 ends movement without snapping the world pose.
    assert!(distance_squared(game.entities[actor].position, [455.0, 505.0]) < 1.0);
    assert!(
        (game.entities[enemy].health - game.catalog[worker].initial_health).abs() < f32::EPSILON
    );
}

#[test]
fn guard_return_route_failure_retains_guard_and_enters_inner_scan_state() {
    let mut game = ground_game();
    game.clear_match_state();
    game.initialize_map_side_players().unwrap();
    game.grid = Grid::new(200, 200, []).unwrap();
    game.navigation = game.grid.clone();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].geometry.major = 25.0;
    game.catalog[tank].geometry.minor = 25.0;
    // A barrier can yield a successful closest path. Fill the native path-info
    // pool with disjoint five-cell-wide footprints to force an actual failure.
    for y in 0..30_u16 {
        for x in 0..40_u16 {
            game.spawn(
                tank,
                0,
                [f32::from(25 + 50 * x), f32::from(25 + 50 * y)],
                true,
            );
        }
    }
    game.refresh_occupancy();
    assert_eq!(game.route_units(0).info_available, 0);
    let goal = [1905.0, 1805.0];
    game.guard(0, goal).unwrap();
    game.process_path_service();
    assert!(game.entities[0].ai.path_request.failed);
    game.update_order(0);
    assert!(matches!(game.entities[0].ai.order, Order::Guard(_)));
    assert!(matches!(
        game.entities[0].ai.guard.phase,
        ai::GuardPhase::Inner
    ));
    assert_eq!(
        game.entities[0].position.map(f32::to_bits),
        [25.0_f32, 25.0].map(f32::to_bits)
    );
}

#[test]
fn invalid_continuous_fire_values_cannot_enter_authoritative_cooldowns() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].weapon.continuous_fire = Some(ContinuousFire {
        mean_shots: 2,
        fast_shots: 6,
        coast_frames: 30.0,
    });
    assert!(validate_catalog(&game.catalog, &game.rules).is_ok());
    game.catalog[tank].weapon.bonuses.template[3][3] = 0.0;
    assert!(validate_catalog(&game.catalog, &game.rules).is_err());
    game.catalog[tank].weapon.bonuses.template[3][3] = 3.0;
    game.catalog[tank]
        .weapon
        .continuous_fire
        .as_mut()
        .unwrap()
        .coast_frames = f32::NAN;
    assert!(validate_catalog(&game.catalog, &game.rules).is_err());
}

#[test]
fn projectiles_use_damage_category_armor_and_reload_after_the_last_clip_round() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let worker = game.type_for(1, Kind::Builder).unwrap();
    game.catalog[tank].weapon.damage = 20.0;
    game.catalog[tank].weapon.damage_type = DamageType::InfantryMissile;
    game.catalog[tank].weapon.delay = 0.1;
    game.catalog[tank].weapon.clip_size = 3;
    game.catalog[tank].weapon.reload_seconds = 0.7;
    game.catalog[worker].armor[..7].copy_from_slice(&[1.0, 0.5, 0.25, 0.75, 0.125, 1.0, 1.0]);
    let actor = game.spawn(tank, 0, [305.0, 505.0], true);
    let target = game.spawn(worker, 1, [405.0, 505.0], true);
    game.attack(actor, target).unwrap();
    let mut firing_frames = Vec::new();
    for frame in 1..=35 {
        game.step();
        if game.presentation.shots.iter().any(|shot| shot.owner == 0) {
            firing_frames.push(frame);
        }
        if frame == 7 {
            assert!((game.entities[target].health - 100.0).abs() < f32::EPSILON);
        }
        if frame == 8 {
            assert!((game.entities[target].health - 95.0).abs() < f32::EPSILON);
        }
        if frame == 28 {
            assert!((game.entities[target].health - 85.0).abs() < f32::EPSILON);
        }
    }
    // The first Aim callback enters Fire; its next callback fires at frame two.
    // AIStates.cpp Aim succeeds when the weapon becomes ready (three frames),
    // then Fire shoots on its next update. After the third round the 21-frame
    // clip reload likewise precedes that Aim-to-Fire callback. RTS_GENERALS=0
    // fast_float_ceil rounds the exact five-frame flight upward to six; the
    // fourth projectile is still in flight at the end of this sample.
    assert_eq!(firing_frames, [2, 6, 10, 32]);
    assert!((game.entities[target].health - 85.0).abs() < f32::EPSILON);
}

#[test]
fn native_barracks_exit_has_an_open_diagonal_when_cardinal_goals_hit_buildings() {
    let mut game = ground_game();
    game.grid = Grid::new(300, 300, []).unwrap();
    game.movement_terrain = crate::terrain::TerrainMap::classify(300, 300, [], []).unwrap();
    let barracks = game.type_for(1, Kind::Barracks).unwrap();
    let factory = game.type_for(1, Kind::Factory).unwrap();
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    // Installed GLABarracks and GLAArmsDealer geometry at the observed legal
    // Tournament Desert diagnostic placements. Preserve both source exit points.
    for (kind, major, minor) in [(barracks, 42.0, 52.0), (factory, 40.0, 62.0)] {
        game.catalog[kind].geometry.major = major;
        game.catalog[kind].geometry.minor = minor;
        game.catalog[kind].geometry.shape = crate::collision_geometry::Shape::Box;
        game.catalog[kind].placement_angle = -135.0_f32.to_radians();
    }
    game.catalog[barracks]
        .behavior
        .production_exit
        .as_mut()
        .unwrap()
        .rally = [42.0, 0.0, 0.0];
    let producer = game.spawn(barracks, 1, [855.0, 2225.0], true);
    game.spawn(factory, 1, [775.0, 2145.0], true);
    game.rebuild_navigation();
    let unit = game.spawn_produced_unit(producer, infantry, false).unwrap();
    for _ in 0..300 {
        if !matches!(game.entities[unit].ai.order, Order::Exit(_)) {
            break;
        }
        game.step();
    }
    let start = game.entities[unit].position;
    assert_ground_arrival(&game, unit, [811.159_36, 2_181.159_4]);
    assert!(!game.navigation.is_blocked(game.cell_at(start).unwrap()));
    for delta in [[60.0, 0.0], [0.0, 60.0], [-60.0, 0.0], [0.0, -60.0]] {
        let goal = [start[0] + delta[0], start[1] + delta[1]];
        assert!(game.navigation.is_blocked(game.cell_at(goal).unwrap()));
        assert!(game.movement_path(unit, start, goal).is_none());
    }
    game.move_to(unit, [start[0] + 60.0, start[1] - 60.0])
        .unwrap();
    for _ in 0..300 {
        game.step();
    }
    assert!(distance_squared(game.entities[unit].position, start) > 100.0);
    assert!(matches!(game.entities[unit].ai.order, Order::Idle));
}

fn progression_game() -> Skirmish {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
        unit.progression.skill_point_value = [800; 4];
        unit.progression.vehicle = unit.kind == Kind::Tank;
        unit.progression.cash_hack_target = unit.kind == Kind::SupplyCenter;
    }
    game.catalog[0].progression.powers = vec!["Repair".to_owned(), "Hack".to_owned()];
    game.catalog[0].progression.cash_bounties =
        vec![("Bounty1".to_owned(), 0.05), ("Bounty2".to_owned(), 0.2)];
    let science = |name: &str, prerequisites: &[&str], cost| Science {
        name: name.to_owned(),
        prerequisites: prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        cost,
    };
    game.configure_progression(ProgressionRules {
        multiplayer_colors: crate::skirmish::tests::original_multiplayer_colors(),
        ranks: [0, 800, 1500, 2500, 5000]
            .into_iter()
            .enumerate()
            .map(|(index, skill_points)| Rank {
                skill_points,
                science_points: if index == 4 { 3 } else { 1 },
                sciences: vec![format!("Rank{}", index + 1)],
            })
            .collect(),
        sciences: vec![
            science("Faction", &[], 0),
            science("OtherFaction", &[], 0),
            science("Rank1", &[], 0),
            science("Rank2", &[], 0),
            science("Rank3", &[], 0),
            science("Rank4", &[], 0),
            science("Rank5", &[], 0),
            science("Repair1", &["Rank3"], 1),
            science("Repair2", &["Repair1"], 1),
            science("Hack1", &["Rank3", "Faction"], 1),
            science("Hack2", &["Hack1"], 1),
            science("Bounty1", &["Rank3", "Faction"], 1),
            science("Bounty2", &["Bounty1"], 1),
            science("WrongFaction", &["OtherFaction"], 1),
        ],
        factions: vec![FactionProgression {
            side: "GLA".into(),
            preferred_color: 0xff00_ff00,
            playable_side: true,
            faction: Faction::Gla,
            intrinsic_points: 0,
            intrinsic_sciences: vec!["Faction".to_owned()],
        }],
        powers: vec![
            PowerDefinition {
                name: "Repair".to_owned(),
                required_science: "Repair1".to_owned(),
                reload_frames: 7200,
                effect: PowerEffect::EmergencyRepair {
                    burst: RepairBurst {
                        amount: 100.0,
                        radius: 100.0,
                        delay_frames: 1,
                    },
                    upgrades: vec![(
                        "Repair2".to_owned(),
                        RepairBurst {
                            amount: 200.0,
                            radius: 100.0,
                            delay_frames: 1,
                        },
                    )],
                },
            },
            PowerDefinition {
                name: "Hack".to_owned(),
                required_science: "Hack1".to_owned(),
                reload_frames: 7200,
                effect: PowerEffect::CashHack {
                    amount: 1000,
                    upgrades: vec![("Hack2".to_owned(), 2000)],
                },
            },
        ],
    })
    .unwrap();
    game
}

fn weapon_kill(game: &mut Skirmish, source: usize, victim: usize) {
    let amount = game.entities[victim].health;
    weapon_hit(game, source, victim, amount);
    game.step();
}

fn weapon_hit(game: &mut Skirmish, source: usize, victim: usize, amount: f32) {
    let kind = game.entities[source].type_index;
    let previous = game.catalog[kind].weapon.damage;
    game.catalog[kind].weapon.damage = amount;
    let position = game.target_center(victim);
    game.deal_bound_weapon_damage(
        source,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        Some(victim),
        position,
        1.0,
    );
    game.catalog[kind].weapon.damage = previous;
}

#[test]
fn direct_rifle_damage_is_immediate_and_actual_shot_overrides_reload_pose() {
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[infantry].ai.acquisition = 0;
    game.catalog[infantry].weapon.speed = 999_999.0;
    game.catalog[infantry].weapon.clip_size = 1;
    game.catalog[infantry].weapon.reload_seconds = 0.7;
    game.catalog[infantry].weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| vec![super::LaunchTransform::IDENTITY]),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    let attacker = game.spawn(infantry, 0, [300.0, 100.0], true);
    let victim = game.spawn(infantry, 1, [400.0, 100.0], true);
    game.entities[victim].ai.mood.next_check = u64::MAX;
    game.attack(attacker, victim).unwrap();
    game.step();
    assert_eq!(game.entities[victim].health.to_bits(), 100.0_f32.to_bits());
    game.step();
    assert!((game.entities[victim].health - 40.0).abs() < f32::EPSILON);
    assert!(game.projectiles.is_empty());
    assert_eq!(game.entities[attacker].weapon_condition(), Some("FIRING_A"));
    game.step();
    assert_eq!(
        game.entities[attacker].weapon_condition(),
        Some("RELOADING_A")
    );
    game.stop(attacker).unwrap();
    game.step(); // The FINAL weapon status helper updates the drawable.
    assert_eq!(game.entities[attacker].weapon_condition(), None);
}

fn promote_player(game: &mut Skirmish) {
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    for _ in 0..2 {
        let victim = game.spawn(infantry, 1, [450.0, 100.0], true);
        weapon_kill(game, 1, victim);
    }
    assert_eq!(game.players[0].rank, 3);
}

#[test]
fn kills_award_source_rank_thresholds_once_but_not_for_unfinished_or_friendly_objects() {
    let mut game = progression_game();
    assert_eq!(game.players[0].science_points, 1);
    assert!(!game.can_purchase_science(0, "Repair1"));
    promote_player(&mut game);
    assert_eq!(game.players[0].skill_points, 1600);
    assert_eq!(game.players[0].science_points, 3);
    assert!((game.rank_progress(0) - 0.1).abs() < 0.000_01);
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    let unfinished = game.spawn(infantry, 1, [450.0, 100.0], false);
    weapon_kill(&mut game, 1, unfinished);
    let friendly = game.spawn(infantry, 0, [450.0, 100.0], true);
    weapon_kill(&mut game, 1, friendly);
    assert_eq!(game.players[0].skill_points, 1600);
    for _ in 0..8 {
        let victim = game.spawn(infantry, 1, [450.0, 100.0], true);
        weapon_kill(&mut game, 1, victim);
        weapon_kill(&mut game, 1, victim);
    }
    assert_eq!(game.players[0].skill_points, 5000);
    assert_eq!(game.players[0].rank, 5);
    assert_eq!(game.players[0].science_points, 7);
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.players[0].rank, 1);
    assert_eq!(game.players[0].science_points, 1);
}

#[test]
fn science_purchase_and_cash_hack_preserve_prerequisites_and_shared_recharge() {
    let mut game = progression_game();
    promote_player(&mut game);
    assert!(!game.can_purchase_science(0, "Hack2"));
    game.purchase_science(0, "Hack1").unwrap();
    assert!(game.purchase_science(0, "Hack1").is_err());
    let supply = game.type_for(1, Kind::SupplyCenter).unwrap();
    let target = game.spawn(supply, 1, [450.0, 100.0], true);
    assert!(game.cast_power(0, "Hack", PowerTarget::Object(2)).is_err());
    assert!(game.can_use_power(0, "Hack"));
    let own_cash = game.players[0].cash;
    let enemy_cash = game.players[1].cash;
    game.cast_power(0, "Hack", PowerTarget::Object(target))
        .unwrap();
    assert_eq!(game.players[0].cash, own_cash + 1000);
    assert_eq!(game.players[1].cash, enemy_cash - 1000);
    assert_eq!(game.power_cooldown_remaining(0, "Hack"), 7200);
    game.purchase_science(0, "Hack2").unwrap();
    let replacement = game.spawn(0, 0, [100.0, 100.0], true);
    assert!(!game.can_use_power(replacement, "Hack"));
    assert_eq!(game.power_cooldown_remaining(replacement, "Hack"), 7200);
    for _ in 0..7200 {
        game.step();
    }
    assert!(game.can_use_power(replacement, "Hack"));
    game.players[1].cash = 1234;
    game.cast_power(replacement, "Hack", PowerTarget::Object(target))
        .unwrap();
    assert_eq!(game.players[1].cash, 0);
    assert_eq!(game.players[0].cash, own_cash + 2234);
}

#[test]
fn bounty_uses_source_percent_ceiling_and_persists_when_command_center_dies() {
    let mut game = progression_game();
    promote_player(&mut game);
    game.purchase_science(0, "Bounty1").unwrap();
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    game.catalog[infantry].cost = 601;
    let victim = game.spawn(infantry, 1, [450.0, 100.0], true);
    let cash = game.players[0].cash;
    weapon_kill(&mut game, 1, victim);
    assert_eq!(game.players[0].cash, cash + 31);
    game.purchase_science(0, "Bounty2").unwrap();
    game.entities[0].health = 0.0;
    // Another owned building keeps the match alive after the source module dies.
    game.spawn(
        game.type_for(0, Kind::Barracks).unwrap(),
        0,
        [100.0, 100.0],
        true,
    );
    let victim = game.spawn(infantry, 1, [450.0, 100.0], true);
    weapon_kill(&mut game, 1, victim);
    assert_eq!(game.players[0].cash, cash + 31 + 121);
}

#[test]
fn emergency_repair_is_a_delayed_owner_vehicle_burst_with_source_radius_and_health_cap() {
    let mut game = progression_game();
    promote_player(&mut game);
    game.purchase_science(0, "Repair1").unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].max_health = 500.0;
    let inside = game.spawn(tank, 0, [300.0, 100.0], true);
    let boundary = game.spawn(tank, 0, [400.0, 100.0], true);
    let outside = game.spawn(tank, 0, [400.1, 100.0], true);
    let enemy = game.spawn(tank, 1, [300.0, 100.0], true);
    let infantry = game.spawn(
        game.type_for(0, Kind::Infantry).unwrap(),
        0,
        [300.0, 100.0],
        true,
    );
    game.entities[infantry].health = 50.0;
    game.entities[boundary].health = 450.0;
    game.cast_power(0, "Repair", PowerTarget::Position([300.0, 100.0]))
        .unwrap();
    assert!((game.entities[inside].health - 100.0).abs() < f32::EPSILON);
    game.step();
    assert!((game.entities[inside].health - 200.0).abs() < f32::EPSILON);
    assert!((game.entities[boundary].health - 500.0).abs() < f32::EPSILON);
    assert!((game.entities[outside].health - 100.0).abs() < f32::EPSILON);
    assert!((game.entities[enemy].health - 100.0).abs() < f32::EPSILON);
    assert!((game.entities[infantry].health - 50.0).abs() < f32::EPSILON);
    game.step();
    assert!((game.entities[inside].health - 200.0).abs() < f32::EPSILON);
}

#[test]
fn ordinary_move_keeps_float_goal_even_within_one_cell_and_animates_turning() {
    let mut game = game();
    let unit_type = game.type_for(0, Kind::Infantry).unwrap();
    let unit = game.spawn(unit_type, 0, [100.1, 100.2], true);
    game.entities[unit].facing = PI;
    let destination = [108.3, 107.7];
    game.materialize_selection_group(0, &[unit]);
    game.move_to(unit, destination).unwrap();
    let initial = game.entities[unit].position;
    game.step();
    assert!(game.entities[unit].moving());
    assert!(distance_squared(initial, game.entities[unit].position) < f32::EPSILON);
    for _ in 0..100 {
        game.step();
    }
    assert!(distance_squared(game.entities[unit].position, destination) < f32::EPSILON);
    assert!(!game.entities[unit].moving());
    assert!(matches!(game.entities[unit].ai.order, Order::Idle));
}

#[test]
fn dozer_model_conditions_follow_docking_cancellation_and_completion() {
    let mut game = ground_game();
    game.catalog[0].energy = 1;
    game.update_power();
    game.step();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let site = game.placement_near(building_type, [260.0, 250.0]).unwrap();
    let building = game.build(1, building_type, site).unwrap();
    assert!(game.entities[building].awaiting_construction());
    assert!(!game.entities[building].partially_constructed());
    for _ in 0..500 {
        game.step();
        if game.entities[1].actively_constructing() {
            break;
        }
    }
    assert!(game.entities[1].actively_constructing());
    assert!(game.entities[building].actively_being_constructed());
    assert!(game.entities[building].partially_constructed());
    assert!(!game.entities[building].awaiting_construction());
    game.stop(1).unwrap();
    assert!(!game.entities[1].actively_constructing());
    // Pinned internalTaskCompleteOrCancelled intentionally leaves crane animation active.
    assert!(game.entities[building].actively_being_constructed());
    game.assist_build(1, building).unwrap();
    for _ in 0..500 {
        game.step();
    }
    assert!(game.entities[building].complete());
    assert!(!game.entities[building].actively_being_constructed());
    assert!(!game.entities[building].partially_constructed());
}

#[test]
fn a_repair_burst_on_full_health_does_not_block_a_later_benefactor_after_damage() {
    let mut game = progression_game();
    // Separate allied marker emitters can overlap in the original. This fixture
    // has two source-shaped module templates to exercise their shared heal lock.
    let mut rules = game.progression.take().unwrap();
    rules.powers.push(PowerDefinition {
        name: "SecondRepair".to_owned(),
        required_science: "Repair1".to_owned(),
        reload_frames: 7200,
        effect: PowerEffect::EmergencyRepair {
            burst: RepairBurst {
                amount: 100.0,
                radius: 100.0,
                delay_frames: 1,
            },
            upgrades: Vec::new(),
        },
    });
    game.catalog[0]
        .progression
        .powers
        .push("SecondRepair".to_owned());
    game.configure_progression(rules).unwrap();
    promote_player(&mut game);
    game.purchase_science(0, "Repair1").unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let target = game.spawn(tank, 0, [300.0, 100.0], true);
    game.cast_power(0, "Repair", PowerTarget::Position([300.0, 100.0]))
        .unwrap();
    game.step();
    weapon_hit(&mut game, 3, target, 50.0);
    game.cast_power(0, "SecondRepair", PowerTarget::Position([300.0, 100.0]))
        .unwrap();
    game.step();
    assert!((game.entities[target].health - 100.0).abs() < f32::EPSILON);
}

#[test]
fn prerequisites_require_each_row_but_accept_object_alternatives() {
    let mut game = progression_game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let factory_type = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[factory_type].name = "Factory".to_owned();
    game.catalog[tank].build_rules.objects =
        vec![vec!["AbsentAlternative".to_owned(), "Factory".to_owned()]];
    game.catalog[tank].name = "Tank".into();
    game.catalog[factory_type].buildable_objects = vec!["Tank".into()];
    game.catalog[tank].build_rules.sciences = vec!["Bounty1".to_owned()];
    let factory = game.spawn(factory_type, 0, [305.0, 205.0], false);
    assert!(!game.prerequisites_met(0, tank));
    game.entities[factory].build_progress = 1.0;
    game.entities[factory].health = 1000.0;
    let cash = game.players[0].cash;
    assert!(game.train(factory, tank).is_err());
    assert_eq!(game.players[0].cash, cash);
    promote_player(&mut game);
    game.purchase_science(0, "Bounty1").unwrap();
    assert!(game.prerequisites_met(0, tank));
    game.train(factory, tank).unwrap();
    game.catalog[tank]
        .build_rules
        .objects
        .push(vec!["MissingTech".to_owned()]);
    assert!(!game.prerequisites_met(0, tank));
}

#[test]
fn radius_healing_obeys_masks_and_keeps_one_benefactor_at_deadline() {
    let mut game = game();
    let healer_type = game.catalog.len();
    let mut healer = fixture_type(Faction::Gla, Kind::Vehicle);
    healer.maintenance.auto_heal.push(AutoHeal {
        damage_delay: 0,
        starts_active: true,
        triggers: Vec::new(),
        conflicts: Vec::new(),
        requires_all: false,
        amount: 5.0,
        delay: 30,
        targets: crate::skirmish::AutoHealTargets {
            radius: 100.0,
            affects_whole_player: false,
            kinds: Some(vec!["VEHICLE".into()]),
            forbidden: vec!["AIRCRAFT".into()],
            skip_self: true,
        },
        single_burst: false,
    });
    crate::skirmish::tests::register_update(&mut healer, "AutoHealBehavior");
    game.catalog.push(healer);
    let first = game.spawn(healer_type, 0, [305.0, 105.0], true);
    let second = game.spawn(healer_type, 0, [315.0, 105.0], true);
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let target = game.spawn(tank_type, 0, [320.0, 105.0], true);
    let enemy = game.spawn(tank_type, 1, [325.0, 105.0], true);
    let boundary = game.spawn(tank_type, 0, [305.0, 205.0], true);
    let mut aircraft = fixture_type(Faction::Gla, Kind::Harvester);
    aircraft.kind_of.push("AIRCRAFT".into());
    let aircraft_type = game.catalog.len();
    game.catalog.push(aircraft);
    let airborne = game.spawn(aircraft_type, 0, [325.0, 105.0], true);
    for id in [target, enemy, boundary, airborne] {
        game.entities[id].health = 50.0;
    }
    game.update_ground_contacts();
    game.wake_family_update(first, "AutoHealBehavior", 0, 1);
    game.wake_family_update(second, "AutoHealBehavior", 0, 1);
    game.tick = 1;
    game.run_object_updates();
    assert!((game.entities[target].health - 55.0).abs() < f32::EPSILON);
    assert!((game.entities[enemy].health - 50.0).abs() < f32::EPSILON);
    assert!((game.entities[boundary].health - 50.0).abs() < f32::EPSILON);
    assert!((game.entities[airborne].health - 50.0).abs() < f32::EPSILON);
    game.tick = 31;
    game.run_object_updates();
    assert!((game.entities[target].health - 60.0).abs() < f32::EPSILON);
    let benefactor = game.entities[target]
        .maintenance
        .healing_benefactor
        .unwrap();
    assert!([first, second].contains(&benefactor));
    game.destroy_maintenance_object(benefactor);
    game.process_update_destruction();
    game.tick = 61;
    game.run_object_updates();
    assert!((game.entities[target].health - 60.0).abs() < f32::EPSILON);
    game.tick = 91;
    game.run_object_updates();
    assert!((game.entities[target].health - 65.0).abs() < f32::EPSILON);
}

#[test]
fn weapon_damage_uses_native_sphere_rings_and_relationship_filters() {
    let mut game = game();
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank_type].weapon.damage = 80.0;
    game.catalog[tank_type].weapon.area = Some(AreaDamage {
        at_source: false,
        primary_radius: 5.0,
        secondary_radius: 10.0,
        secondary_damage: 20.0,
        affects: 7,
    });
    let source = game.spawn(tank_type, 0, [300.0, 105.0], true);
    let primary = game.spawn(tank_type, 1, [305.0, 105.0], true);
    let ally = game.spawn(tank_type, 0, [317.0, 105.0], true);
    let outside = game.spawn(tank_type, 1, [320.0, 105.0], true);
    game.update_ground_contacts();
    game.deal_bound_weapon_damage(
        source,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        None,
        [305.0, 105.0, 5.0],
        1.0,
    );
    assert!((game.entities[source].health - 100.0).abs() < f32::EPSILON);
    assert!((game.entities[primary].health - 20.0).abs() < f32::EPSILON);
    assert!((game.entities[ally].health - 80.0).abs() < f32::EPSILON);
    assert!((game.entities[outside].health - 100.0).abs() < f32::EPSILON);
}

#[test]
fn infantry_scatter_detaches_target_and_uses_ground_height() {
    let mut game = game();
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let infantry_type = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[tank_type].weapon.infantry_scatter = 10.0;
    let victim = game.spawn(infantry_type, 1, [305.0, 105.0], true);
    let seed = game.random;
    let (point, tracked) = game.weapon_destination(tank_type, 0, victim);
    assert!(tracked.is_none());
    assert!(point[2].abs() < f32::EPSILON);
    assert!(distance_squared([point[0], point[1]], game.entities[victim].position) <= 100.0);
    assert_ne!(game.random, seed);
    let vehicle = game.spawn(tank_type, 1, [305.0, 105.0], true);
    let seed = game.random;
    let (_, tracked) = game.weapon_destination(tank_type, 0, vehicle);
    assert_eq!(tracked, Some(vehicle));
    assert_eq!(game.random, seed);
}

#[test]
fn two_barrel_clip_keeps_native_between_shot_and_reload_delays() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[tank];
    unit.weapon.damage = 1.0;
    unit.weapon.speed = 999_999.0;
    unit.weapon.delay = 0.3;
    unit.weapon.clip_size = 2;
    unit.weapon.reload_seconds = 2.0;
    unit.weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| {
            vec![LaunchTransform::IDENTITY, LaunchTransform::IDENTITY]
        }),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    let source = game.spawn(tank, 0, [305.0, 105.0], true);
    let target = game.spawn(tank, 1, [405.0, 105.0], true);
    game.entities[target].ai.mood.next_check = u64::MAX;
    game.update_visibility();
    game.attack(source, target).unwrap();
    let mut shots = Vec::new();
    for _ in 0..75 {
        game.step();
        for shot in game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == source)
        {
            shots.push((game.tick, shot.barrel));
        }
    }
    // Aim and Fire alternate while the gun is cooling. The nine-frame delay
    // expires on an Aim frame; the sixty-frame reload expires on a Fire frame.
    assert_eq!(shots, [(2, 0), (12, 1), (72, 0)]);
}

#[test]
fn emergency_repair_includes_allied_vehicles_but_excludes_neutral_objects() {
    let mut game = progression_game();
    promote_player(&mut game);
    game.slots[0].team = Some(0);
    game.slots[1].team = Some(0);
    game.purchase_science(0, "Repair1").unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].max_health = 500.0;
    let ally = game.spawn(tank, 1, [300.0, 100.0], true);
    let neutral = game.spawn(tank, game.neutral_owner(), [300.0, 100.0], true);
    game.cast_power(0, "Repair", PowerTarget::Position([300.0, 100.0]))
        .unwrap();
    game.step();
    assert_eq!(game.entities[ally].health.to_bits(), 200.0_f32.to_bits());
    assert_eq!(game.entities[neutral].health.to_bits(), 100.0_f32.to_bits());
}

fn fixture_scoring(kind: Kind) -> SourceScoring {
    SourceScoring {
        create: true,
        destroy: true,
        victory: kind.is_building(),
    }
}

pub(super) fn register_behavior(unit: &mut UnitType, family: &str, interfaces: u32) {
    use super::module_updates::{BehaviorRegistration, ModuleSource};
    unit.behavior_modules.push(BehaviorRegistration {
        family: family.into(),
        tag: format!("TestModule{}", unit.behavior_modules.len()),
        interfaces,
        upgrade: crate::skirmish::module_updates::UpgradeData::none(),
        source: ModuleSource {
            path: "test.ini".into(),
            origin: "explicit callback fixture".into(),
            line: 1,
        },
    });
}

pub(super) fn register_update(unit: &mut UnitType, family: &str) {
    use super::module_updates::{ModuleRegistration, ModuleSource};
    let interfaces = match family {
        "AutoHealBehavior" | "FireWeaponWhenDamagedBehavior" => 0x0085,
        "CountermeasuresBehavior" | "SpyVisionUpdate" => 0x0081,
        "ProductionUpdate" => 0x0003,
        "BridgeBehavior" => 0x0007,
        "PhysicsBehavior" => 0x0011,
        "TunnelContain" | "CaveContain" => 0x005f,
        "OpenContain"
        | "ParachuteContain"
        | "HealContain"
        | "TransportContain"
        | "GarrisonContain"
        | "InternetHackContain"
        | "HelixContain"
        | "OverlordContain" => 0x0057,
        "BaseRegenerateUpdate"
        | "FlammableUpdate"
        | "PoisonedBehavior"
        | "SupplyWarehouseCripplingBehavior" => 0x0005,
        _ => 0x0001,
    };
    register_behavior(unit, family, interfaces);
    let module = unit.behavior_modules.last().unwrap();
    unit.update_modules.push(ModuleRegistration::Behavior {
        family: family.into(),
        tag: module.tag.clone(),
        source: ModuleSource {
            path: "test.ini".into(),
            origin: "explicit callback fixture".into(),
            line: 1,
        },
    });
}

pub(super) fn self_heal(amount: f32, delay: u32, damage_delay: u32, triggers: &[&str]) -> AutoHeal {
    AutoHeal {
        amount,
        delay,
        damage_delay,
        starts_active: false,
        triggers: triggers.iter().map(|name| (*name).into()).collect(),
        conflicts: Vec::new(),
        requires_all: false,
        targets: crate::skirmish::AutoHealTargets {
            radius: 0.0,
            affects_whole_player: false,
            kinds: None,
            forbidden: Vec::new(),
            skip_self: false,
        },
        single_burst: false,
    }
}

/// A native source field template for tests which author custom numeric field values.
/// Actual Body/helpers/FireWeapon/Lifetime/DIE declarations match System.ini's field envelope.
fn native_fixture_bounds(geometry: &crate::collision_geometry::Geometry) -> GeometryBounds {
    // GeometryInfo::calcBoundingStuff derives the center and sphere from the authored
    // geometry; the generic Vehicle fixture's ten-unit bounds do not apply.
    let half_height = geometry.height * 0.5;
    match geometry.shape {
        crate::collision_geometry::Shape::Sphere => GeometryBounds {
            center_height: 0.0,
            sphere_radius: geometry.major,
            max_height: geometry.major,
        },
        crate::collision_geometry::Shape::Cylinder => GeometryBounds {
            center_height: half_height,
            sphere_radius: half_height.max(geometry.major),
            max_height: geometry.height,
        },
        crate::collision_geometry::Shape::Box => {
            let major_squared = geometry.major * geometry.major;
            let minor_squared = geometry.minor * geometry.minor;
            let height_squared = half_height * half_height;
            GeometryBounds {
                center_height: half_height,
                sphere_radius: ((major_squared + minor_squared) + height_squared).sqrt(),
                max_height: geometry.height,
            }
        }
    }
}

pub(super) fn native_hazard_fixture(
    game: &mut Skirmish,
    launcher: usize,
    origin: hazards::Origin,
) -> usize {
    let rules = game.catalog[launcher]
        .hazard_rules(origin)
        .expect("authored test hazard");
    if let Some(index) = game.catalog.iter().position(|unit| unit.name == rules.name) {
        return index;
    }
    let mut unit = fixture_type(Faction::Civilian, Kind::Effect);
    unit.name = rules.name.clone();
    unit.kind_of = [
        "IMMOBILE",
        "CLEANUP_HAZARD",
        "STICK_TO_TERRAIN_SLOPE",
        "INERT",
        "NO_COLLIDE",
    ]
    .map(str::to_owned)
    .to_vec();
    unit.kind_of.push("NO_COLLIDE".into());
    unit.initial_health = rules.health;
    unit.max_health = rules.health;
    unit.geometry = crate::collision_geometry::Geometry {
        shape: rules.geometry.shape,
        is_small: rules.geometry.is_small,
        major: rules.geometry.major,
        minor: rules.geometry.minor,
        height: rules.geometry.height,
    };
    unit.geometry_bounds = native_fixture_bounds(&unit.geometry);
    unit.armor.fill(0.0);
    unit.armor[DamageType::Unresistable as usize] = 1.0;
    unit.armor[DamageType::HazardCleanup as usize] = 1.0;
    unit.behavior.lifetimes = vec![[rules.lifetime; 2]];
    unit.weapon = WeaponRules::unarmed();
    unit.ai = UnitAi::default();
    let mut pulse = WeaponRules::unarmed();
    pulse.name = "NativeTestPoisonPulse".into();
    pulse.damage = rules.damage;
    pulse.damage_type = DamageType::Poison;
    pulse.death_type = rules.death_type;
    pulse.delay = progression::integer_real(rules.interval) * (1.0 / 30.0);
    pulse.range = 15.0;
    pulse.min_range = 10.0;
    pulse.area = Some(AreaDamage {
        at_source: false,
        primary_radius: rules.radius,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 1 | 2 | 4 | 64,
    });
    let mut core = WeaponRules::unarmed();
    core.name = "HazardFieldCoreWeapon".into();
    core.damage = rules.core_damage;
    core.damage_type = DamageType::HazardCleanup;
    core.delay = 1.0 / 30.0;
    core.clip_size = 1;
    core.reload_seconds = 100.0;
    core.range = 15.0;
    core.area = Some(AreaDamage {
        at_source: false,
        primary_radius: rules.core_radius,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 1 | 2 | 4,
    });
    unit.behavior.fire_weapons = vec![
        fire::weapon_update::Rules {
            weapon: Some(pulse),
            initial_delay: 0,
            exclusive_delay: 0,
        },
        fire::weapon_update::Rules {
            weapon: Some(core),
            initial_delay: 0,
            exclusive_delay: 0,
        },
    ];
    unit.behavior.destroy_die.push(death_conditions());
    unit.behavior.death_fx.push(death_fx(&rules.death_fx));
    register_hazard_modules(&mut unit);
    let index = game.catalog.len();
    game.catalog.push(unit);
    index
}

fn register_hazard_modules(unit: &mut UnitType) {
    use module_updates::{BehaviorRegistration, Helper, ModuleRegistration, ModuleSource};
    unit.update_modules = [
        Helper::StateMachine,
        Helper::StatusDamage,
        Helper::SubdualDamage,
        Helper::Defection,
    ]
    .into_iter()
    .map(ModuleRegistration::Helper)
    .collect();
    for (family, tag, interfaces) in [
        ("ActiveBody", "ModuleTag_02", 32),
        ("FireWeaponUpdate", "ModuleTag_03", 1),
        ("LifetimeUpdate", "ModuleTag_04", 1),
        ("FireWeaponUpdate", "ModuleTag_05", 1),
        ("DestroyDie", "ModuleTag_06", 2),
        ("FXListDie", "ModuleTag_07", 0x82),
    ] {
        let source = || ModuleSource {
            path: "Data/INI/Object/System.ini".into(),
            origin: "source field envelope with authored test numeric overrides".into(),
            line: 1823,
        };
        unit.behavior_modules.push(BehaviorRegistration {
            family: family.into(),
            tag: tag.into(),
            interfaces,
            upgrade: crate::skirmish::module_updates::UpgradeData::none(),
            source: source(),
        });
        if interfaces & 1 != 0 {
            unit.update_modules.push(ModuleRegistration::Behavior {
                family: family.into(),
                tag: tag.into(),
                source: source(),
            });
        }
    }
}

pub(super) fn poison_damage_fixture(game: &mut Skirmish, type_index: usize) {
    use module_updates::{BehaviorRegistration, ModuleRegistration, ModuleSource};
    for ordinal in 0..game.catalog[type_index].behavior.poisoned.len() {
        let tag = game.catalog[type_index]
            .update_modules
            .iter()
            .filter_map(|entry| match entry {
                ModuleRegistration::Behavior { family, tag, .. }
                    if family == "PoisonedBehavior" =>
                {
                    Some(tag.clone())
                }
                _ => None,
            })
            .nth(ordinal)
            .unwrap_or_else(|| format!("PoisonDamageFixture{ordinal}"));
        let source = || ModuleSource {
            path: "PoisonedBehavior.cpp".into(),
            origin: "source poison damage interface fixture".into(),
            line: 105,
        };
        if !game.catalog[type_index]
            .behavior_modules
            .iter()
            .any(|module| module.tag == tag)
        {
            game.catalog[type_index]
                .behavior_modules
                .push(BehaviorRegistration {
                    family: "PoisonedBehavior".into(),
                    tag: tag.clone(),
                    interfaces: 5,
                    upgrade: crate::skirmish::module_updates::UpgradeData::none(),
                    source: source(),
                });
        }
        let count = game.catalog[type_index].update_modules.iter().filter(|entry| matches!(entry, ModuleRegistration::Behavior { family, .. } if family == "PoisonedBehavior")).count();
        if count <= ordinal {
            game.catalog[type_index]
                .update_modules
                .push(ModuleRegistration::Behavior {
                    family: "PoisonedBehavior".into(),
                    tag,
                    source: source(),
                });
        }
    }
}

pub(super) fn death_conditions() -> death_callbacks::Conditions {
    death_callbacks::Conditions {
        death_mask: u32::MAX,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
    }
}

pub(super) fn death_fx(effect: &str) -> death_callbacks::FxRules {
    death_callbacks::FxRules {
        conditions: death_conditions(),
        effect: effect.into(),
        orient_to_object: true,
        starts_active: true,
        triggers: Vec::new(),
        conflicts: Vec::new(),
        requires_all: false,
        removals: Vec::new(),
        upgrade_fx: String::new(),
    }
}

// Authored RadarUpgrade fixture: register before spawning to exercise its constructor.
pub(super) fn radar_upgrade(unit: &mut UnitType, name: &str, granted: bool, disable_proof: bool) {
    unit.behavior
        .radar_upgrades
        .push(super::radar_update::Upgrade {
            triggers: vec![name.into()],
            conflicts: Vec::new(),
            requires_all: false,
            disable_proof,
        });
    register_behavior(unit, "RadarUpgrade", 0x0080);
    unit.behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers
        .push(name.into());
    if granted {
        unit.behavior.radar_grants.push(super::radar_update::Grant {
            name: Some(name.into()),
            academy_radar: false,
            player: false,
            on_created: true,
        });
        register_behavior(unit, "GrantUpgradeCreate", 0x0008);
    }
}

// Exact original INIZH.big multiplayer.ini palette in authored declaration order.
pub(super) fn original_multiplayer_colors() -> Vec<u32> {
    vec![
        0xffdd_e20d,
        0xffff_0000,
        0xff43_68fe,
        0xff3e_d12e,
        0xffff_a019,
        0xff32_d7e6,
        0xff96_00c8,
        0xffff_96ff,
    ]
}

#[test]
fn inactive_body_zero_health_admission_does_not_admit_active_zero_health() {
    let mut game = game();
    game.catalog[0].body_kind = BodyKind::Inactive;
    game.catalog[0].max_health = 0.0;
    game.catalog[0].initial_health = 0.0;
    assert!(validate_catalog(&game.catalog, &game.rules).is_ok());
    game.catalog[0].body_kind = BodyKind::Active;
    assert!(validate_catalog(&game.catalog, &game.rules).is_err());
}
