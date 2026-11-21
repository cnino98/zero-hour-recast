mod departure;

use super::*;
use crate::skirmish::{
    Faction, Kind, MovementSurface, Order, flight, model_bones,
    module_updates::ModuleRegistration,
    physical_ocl, reinforcement_timer,
    tests::{fixture_type, register_update},
};
use std::collections::{BTreeMap, BTreeSet};

fn payload_type() -> crate::skirmish::UnitType {
    let mut rider = fixture_type(Faction::Usa, Kind::Tank);
    rider.name = "DeliveryTank".into();
    rider.kind_of = vec!["VEHICLE".into()];
    rider.live_physics = Some(physical_ocl::LiveRules {
        motion: physical_ocl::MotionRules {
            center_of_mass_offset: 0.5,
            mass: 50.0,
            gravity: -64.0 / 900.0,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: false,
        collision: physical_ocl::CollisionRules {
            stiffness: 0.5,
            rubble_height: 5.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.0; 3],
        min_fall_speed: 999.0,
        fall_damage_factor: 1.0,
    });
    register_update(&mut rider, "PhysicsBehavior");
    rider
}

fn transport_type() -> crate::skirmish::UnitType {
    let mut plane = fixture_type(Faction::Usa, Kind::Vehicle);
    plane.name = "DeliveryPlane".into();
    plane.kind_of = vec!["AIRCRAFT".into(), "VEHICLE".into()];
    plane.update_modules.retain(|module|!matches!(module,ModuleRegistration::Behavior{family,..} if family=="AIUpdateInterface"));
    plane
        .behavior_modules
        .retain(|module| module.family != "AIUpdateInterface");
    plane.locomotor.ground = None;
    let mut wings = flight::tests::rules(flight::Appearance::Wings);
    // Original B52Locomotor maintains forward flight while approaching/dropping.
    wings.min_speed = 60.0 / 30.0;
    wings.braking = 20.0 / 900.0;
    plane.locomotor.flight = Some(wings);
    plane.locomotor.works_when_dead = true;
    plane.locomotor.friction_when_airborne = true;
    plane.locomotor.legal_surfaces = 8;
    plane.locomotor.turn_rate = 25.0 * (std::f32::consts::PI / 180.0) / 30.0;
    plane.locomotor.turn_rate_damaged = 10.0 * (std::f32::consts::PI / 180.0) / 30.0;
    plane.locomotor.speed_damaged = 75.0;
    plane.speed = 125.0;
    plane.movement_surface = MovementSurface::Air;
    plane.ai.acquisition = 0;
    let mut contain = crate::skirmish::containment::tests::rules(0.0, 0.0);
    contain.capacity = 100;
    contain.orient_like_container_on_exit = true;
    contain.allowed = vec!["INFANTRY".into(), "VEHICLE".into(), "PARACHUTABLE".into()];
    contain.forbidden.clear();
    contain.exit_paths = 0;
    contain.door_frames = 0;
    contain.exit_frames = 0;
    contain.death.destroy_if_not_free = true;
    contain.exit_flags = 0;
    plane.contain = Some(contain);
    plane.deliver_payload = Some(TransportRules {
        door_delay: 15,
        exit_bone: "WeaponA01".into(),
        exit_pitch_rate: std::f32::consts::PI / 180.0,
        dive_sound: String::new(),
        strafe_effects: Vec::new(),
    });
    plane.pristine_draws = vec![model_bones::Draw {
        ignored: BTreeSet::new(),
        states: vec![model_bones::State {
            conditions: vec![BTreeSet::new()],
            transforms: BTreeMap::from([(
                "weapona01".into(),
                [
                    [1.0, 0.0, 0.0, 10.0],
                    [0.0, 1.0, 0.0, 2.0],
                    [0.0, 0.0, 1.0, -3.0],
                ],
            )]),
        }],
    }];
    crate::skirmish::aircraft_death::tests::install_physics(&mut plane);
    register_update(&mut plane, "DeliverPayloadAIUpdate");
    register_update(&mut plane, "TransportContain");
    plane
}

fn fixture() -> (Skirmish, usize, usize) {
    let (mut game, chute) = crate::skirmish::parachute::tests::fixture_catalog();
    game.catalog[chute].name = "DeliveryChute".into();
    game.catalog[chute].kind_of.push("PARACHUTABLE".into());
    let contain = game.catalog[chute].contain.as_mut().unwrap();
    contain.capacity = 10;
    contain.allowed = vec!["INFANTRY".into(), "VEHICLE".into()];
    game.catalog.push(payload_type());
    let plane_kind = game.catalog.len();
    game.catalog.push(transport_type());
    let mut pad = fixture_type(Faction::Civilian, Kind::TechBuilding);
    pad.name = "TechReinforcementPad".into();
    pad.behavior.ocl_update = Some(reinforcement_timer::Rules {
        min_delay: 3,
        max_delay: 3,
        ocl: None,
        faction_ocl: vec![reinforcement_timer::FactionOcl {
            faction: "America".into(),
            ocl: "PadList".into(),
        }],
        create_at_edge: true,
        faction_triggered: true,
    });
    register_update(&mut pad, "OCLUpdate");
    let pad_kind = game.catalog.len();
    game.catalog.push(pad);
    game.players[0].playable_side = true;
    game.players[0].side = Some("America".into());
    game.players[0].color = Some(0xffdd_e20d);
    game.configure_delivery_lists(vec![Rules {
        name: "PadList".into(),
        transport: "DeliveryPlane".into(),
        payload: vec![Payload {
            object: "DeliveryTank".into(),
            count: 1,
        }],
        container: "DeliveryChute".into(),
        transport_start: super::TransportStart {
            preferred_height: true,
            max_speed: true,
            delay_delivery_max: 0,
        },
        release: super::Release {
            drop_offset: [0.0, 0.0, -5.0],
            drop_delay: 12,
            parachute_directly: true,
            drop_variance: [0.0; 3],
            inherit_velocity: false,
            exit_pitch_rate: 0.0,
            fire_weapon: false,
        },
        attempts: 4,
        delivery_distance: 250.0,
        pre_open_distance: 0.0,
        formation_size: 1,
        formation_spacing: 25.0,
        convergence: 0.0,
        error_radius: 0.0,
        self_destruct: false,
        visible: None,
        dive: None,
        decal: None,
    }])
    .unwrap();
    (game, pad_kind, plane_kind)
}

fn create() -> (Skirmish, usize, usize, usize, usize) {
    let (mut game, pad_kind, _) = fixture();
    let pad = game.spawn(pad_kind, 0, [355.0, 255.0], true);
    let plane = game.create_delivery(
        pad,
        0,
        game.delivery_closest_edge(game.object_position(pad)),
    );
    (game, pad, plane, plane + 1, plane + 2)
}

#[test]
fn formation_members_keep_aim_convergence_and_spawn_at_distinct_positions() {
    let (mut game, pad_kind, plane_kind) = fixture();
    game.delivery_lists[0].formation_size = 3;
    game.delivery_lists[0].formation_spacing = 35.0;
    game.delivery_lists[0].convergence = 0.5;
    game.delivery_lists[0].payload.clear();
    let pad = game.spawn(pad_kind, 0, [300.0, 200.0], true);
    let start = [100.0, 200.0, 0.0];
    let target = [500.0, 200.0, 0.0];
    let first = game.create_delivery_at(pad, 0, start, target, true);
    assert_eq!(game.entities[first].type_index, plane_kind);
    assert_eq!(
        game.delivery_mission(first).target.map(f32::to_bits),
        target.map(f32::to_bits)
    );
    let left = game.delivery_mission(first + 1);
    let right = game.delivery_mission(first + 2);
    assert!((left.move_to[1] - 165.0).abs() < 0.001);
    assert!((right.move_to[1] - 235.0).abs() < 0.001);
    assert!((left.target[1] - 182.5).abs() < 0.001);
    assert!((right.target[1] - 217.5).abs() < 0.001);
    assert_eq!(game.entities[first + 2].producer, Some(pad));
}

#[test]
fn shot_down_delivery_keeps_updating_the_owner_decal_until_deletion() {
    let (mut game, pad_kind, _) = fixture();
    game.delivery_lists[0].decal = Some((
        50.0,
        crate::skirmish::radius_decal::Template {
            texture: "SCCA10Strike_USA".into(),
            opacity: [0.25, 0.5],
            throb_frames: 15,
            color: [255, 156, 0, 255],
            only_owner: true,
            additive: false,
        },
    ));
    let pad = game.spawn(pad_kind, 0, [350.0, 250.0], true);
    let aim = game.object_position(pad);
    let start = game.delivery_closest_edge(aim);
    let plane = game.create_delivery(pad, 0, start);
    assert_eq!(game.entities[plane].radius_decal().unwrap().frame, Some(0));
    game.mark_ai_dead(plane);
    game.tick = 18;
    game.update_delivery_module(plane);
    let decal = game.entities[plane].radius_decal().unwrap();
    assert_eq!(decal.frame, Some(18));
    assert_eq!(decal.position.map(f32::to_bits), aim.map(f32::to_bits));
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::Approach
    ));
    game.destroy_maintenance_object(plane);
    assert!(game.entities[plane].radius_decal().is_some());
    game.process_update_destruction();
    assert!(!game.entities[plane].exists());
    assert!(game.entities[plane].radius_decal().is_none());
}

#[test]
fn authored_timer_capture_creates_real_nested_transport_then_lands_original_rider_and_retires() {
    let (mut game, pad_kind, plane_kind) = fixture();
    let pad = game.spawn(pad_kind, game.neutral_owner(), [355.0, 255.0], true);
    game.step();
    assert!(
        !game
            .entities
            .iter()
            .any(|actor| actor.type_index == plane_kind)
    );
    game.transfer_maintenance_owner(pad, 0);
    for _ in 0..3 {
        game.step();
    }
    assert!(
        !game
            .entities
            .iter()
            .any(|actor| actor.type_index == plane_kind)
    );
    game.step();
    let plane = game
        .entities
        .iter()
        .position(|actor| actor.type_index == plane_kind)
        .unwrap();
    let rider = plane + 1;
    let chute = plane + 2;
    assert_eq!(game.entities[plane].producer, Some(pad));
    assert_eq!(game.entities[rider].producer, Some(plane));
    assert_eq!(game.entities[chute].producer, Some(plane));
    assert_eq!(game.passengers(plane), &[chute]);
    assert_eq!(game.passengers(chute), &[rider]);
    assert!(game.entities[rider].disabled_held() && game.entities[chute].disabled_held());
    assert_eq!(game.entities[rider].owner, 0);
    assert_eq!(
        game.team_membership.team_of(plane),
        game.team_membership.team_of(rider)
    );
    assert!(game.entities[plane].script_targetable());
    assert!(game.object_position(plane)[2] > 90.0);
    // Losing the pad after CREATE does not recolor or transfer the existing delivery.
    game.transfer_maintenance_owner(pad, game.neutral_owner());
    let mut opened = false;
    let mut released = false;
    for _ in 0..1500 {
        game.step();
        opened |= game.entities[chute].parachute_opened();
        released |= game.entities[rider].contained_by().is_none();
        if released && !game.entities[plane].exists() {
            break;
        }
    }
    assert!(opened && released);
    assert!(game.entities[rider].alive());
    assert_eq!(game.entities[rider].owner, 0);
    assert!(!game.entities[rider].parachute_rider());
    assert!(!game.entities[plane].exists());
}

#[test]
fn weapon_bone_exit_uses_full_object_basis_and_source_motive_velocity_before_world_drop_offset() {
    let (mut game, _, plane, rider, chute) = create();
    game.step(); // Execute the original starting motive force through Physics.
    let basis = [[0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]];
    game.set_object_transform(plane, [300.0, 200.0, 100.0], basis);
    game.delivery_state_mut(plane).free_to_exit = true;
    let velocity = game.entities[plane].physics_velocity();
    assert!(velocity[0].abs() + velocity[1].abs() > 0.1);
    game.native_ai_exit(chute, plane);
    // AI_EXIT entry queues the door request; its later update performs removal.
    assert_eq!(game.entities[chute].contained_by(), Some(plane));
    assert!(matches!(
        game.entities[chute].ai.order,
        Order::ExitContainer
    ));
    game.update_container_exit(chute);
    assert_eq!(game.entities[chute].contained_by(), None);
    // Full basis * pristine [10,2,-3], then Object translation.
    assert_eq!(
        game.object_position(chute).map(f32::to_bits),
        [298.0_f32, 203.0, 110.0].map(f32::to_bits)
    );
    let acceleration = game.entities[chute]
        .live_physics
        .as_ref()
        .unwrap()
        .motion
        .acceleration;
    for axis in 0..3 {
        assert!((acceleration[axis] - velocity[axis]).abs() < 0.00001);
    }
    // applyMotiveForce schedules acceleration; it does not assign velocity.
    assert_eq!(
        game.entities[chute].physics_velocity().map(f32::to_bits),
        [0; 3]
    );
    assert_eq!(game.entities[rider].contained_by(), Some(chute));
    assert!(game.entities[rider].disabled_held());
}

#[test]
fn door_delay_and_empty_check_are_distinct_native_updates() {
    let (mut game, _, plane, _, chute) = create();
    game.set_object_position(plane, [355.0, 255.0, 100.0]);
    game.enter_delivery_phase(plane, Phase::Delivering);
    for _ in 0..15 {
        game.update_delivery_state(plane);
        assert_eq!(game.entities[chute].contained_by(), Some(plane));
    }
    game.update_delivery_state(plane);
    assert_eq!(game.entities[chute].contained_by(), Some(plane));
    assert!(matches!(
        game.entities[chute].ai.order,
        Order::ExitContainer
    ));
    game.update_container_exit(chute);
    assert_eq!(game.entities[chute].contained_by(), None);
    assert!(game.delivery_state(plane).free_to_exit);
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::Delivering
    ));
    for _ in 0..12 {
        game.update_delivery_state(plane);
    }
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::Delivering
    ));
    game.update_delivery_state(plane);
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::HeadOffMap
    ));
    assert!(!game.delivery_state(plane).accepting_commands);
    assert_eq!(
        game.entities[chute]
            .parachute
            .as_ref()
            .unwrap()
            .landing_override,
        Some([355.0, 255.0, 0.0])
    );
}

#[test]
fn consider_entry_conditions_run_immediately_but_update_success_beats_offmap_condition() {
    let (mut game, _, plane, _, _) = create();
    game.enter_delivery_phase(plane, Phase::Consider);
    assert!(matches!(game.delivery_mission(plane).phase, Phase::Recover));
    assert!(game.entities[plane].drawable.hidden);
    assert!(!game.partition.registered(plane));
    assert_eq!(
        game.entities[plane].physics_velocity().map(f32::to_bits),
        [0; 3]
    );
    game.set_object_position(plane, [355.0, 255.0, 100.0]);
    game.enter_delivery_phase(plane, Phase::Consider);
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::Consider
    ));
    game.set_object_position(plane, [355.0, -100.0, 100.0]);
    game.ai_idle_from_ai(plane);
    game.update_delivery_state(plane);
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::Approach
    ));
}

#[test]
fn peaceful_retry_retirement_destroys_nested_cargo_through_removal_callbacks() {
    let (mut game, _, plane, rider, chute) = create();
    game.delivery_mission_mut(plane).entries = 4;
    game.enter_delivery_phase(plane, Phase::Consider);
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::HeadOffMap
    ));
    game.set_object_position(plane, [355.0, -100.0, 100.0]);
    let health = [plane, chute, rider].map(|id| game.entities[id].health.to_bits());
    game.update_delivery_state(plane);
    game.process_update_destruction();
    assert!(
        !game.entities[plane].exists()
            && !game.entities[chute].exists()
            && !game.entities[rider].exists()
    );
    assert_eq!(
        [plane, chute].map(|id| game.entities[id].health.to_bits()),
        [health[0], health[1]]
    );
    assert!(!game.entities[rider].parachute_rider());
    assert!(game.entities[chute].object_status & (1 << 5) != 0);
}

#[test]
fn shot_down_transport_uses_generic_not_free_to_exit_destruction_and_never_parachute_die_damage() {
    let (mut game, _, plane, rider, chute) = create();
    game.set_object_position(plane, [355.0, 255.0, 100.0]);
    game.delivery_state_mut(plane).free_to_exit = true;
    let health = [chute, rider].map(|id| game.entities[id].health.to_bits());
    game.kill_object(plane, crate::skirmish::DeathType::Normal);
    game.process_update_destruction();
    assert!(game.entities[plane].health <= 0.0);
    assert!(!game.entities[chute].exists() && !game.entities[rider].exists());
    assert_eq!(
        [chute, rider].map(|id| game.entities[id].health.to_bits()),
        health
    );
}

#[test]
fn approach_retries_for_retained_idle_during_temporary_busy() {
    let (mut game, _, plane, _, _) = create();
    game.set_object_position(plane, [50.0, 50.0, 100.0]);
    game.ai_idle_from_ai(plane);
    assert!(game.entities[plane].ai_idle());
    game.set_temporary_busy(plane);
    game.update_delivery_state(plane);
    assert!(matches!(
        game.delivery_mission(plane).phase,
        Phase::Consider
    ));
    assert!(matches!(
        game.entities[plane].ai.order,
        crate::skirmish::Order::Move(_)
    ));
    assert!(!game.entities[plane].ai.blocking.temporary_move_to());
    assert_eq!(game.delivery_mission(plane).entries, 1);
}

#[test]
fn nested_destroy_runs_parachute_removal_before_child_delete() {
    let (mut game, _, plane, rider, chute) = create();
    game.set_object_position(plane, [355.0, 255.0, 100.0]);
    assert!(game.entities[rider].parachute_rider());
    game.destroy_maintenance_object(plane);
    assert!(!game.entities[rider].parachute_rider());
    assert!(game.entities[chute].object_status & (1 << 5) != 0);
    assert!(game.entities[rider].contained_by().is_none());
    assert!(game.entities[chute].contained_by().is_none());
    assert!(game.passengers(plane).is_empty() && game.passengers(chute).is_empty());
}

#[test]
fn declared_exit_count_controls_walkout_path_after_transport_bone_callback() {
    for (count, expected) in [(0, [310.0_f32, 202.0, 97.0]), (1, [300.0, 185.0, 100.0])] {
        let (mut game, _, plane, _, chute) = create();
        let kind = game.entities[plane].type_index;
        game.catalog[kind].contain.as_mut().unwrap().exit_paths = count;
        game.set_object_transform(
            plane,
            [300.0, 200.0, 100.0],
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        );
        game.delivery_state_mut(plane).free_to_exit = true;
        game.native_ai_exit(chute, plane);
        game.update_container_exit(chute);
        assert_eq!(game.entities[chute].contained_by(), None);
        assert_eq!(
            game.object_position(chute).map(f32::to_bits),
            expected.map(f32::to_bits)
        );
        assert_eq!(
            matches!(game.entities[chute].ai.order, Order::FollowPath),
            count > 0
        );
    }
}
