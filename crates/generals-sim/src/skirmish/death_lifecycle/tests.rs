use super::*;
use crate::skirmish::DeathType;
use crate::skirmish::{GroundAppearance, GroundRules, STEP_SECONDS, WeaponFxEvent, tests::game};

fn slow(kind: DeathType) -> SlowDeath {
    SlowDeath {
        derived: crate::skirmish::death_lifecycle::Derived::None,
        tag: "Slow".into(),
        conditions: crate::skirmish::death_callbacks::Conditions {
            death_mask: 1 << kind as u32,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
        },
        probability: 10,
        overkill_bonus: 0.0,
        sink_delay: [90, 0],
        destruction_delay: [240, 0],
        sink_rate: 0.5 * STEP_SECONDS,
        fling: None,
        fx: [vec!["FX_TerroristDie".into()], vec![], vec![]],
        objects: std::array::from_fn(|_| Vec::new()),
        weapons: std::array::from_fn(|_| Vec::new()),
    }
}

fn rules(kind: DeathType) -> Rules {
    Rules {
        on_die: Vec::new(),
        slow: vec![slow(kind)],
        weapons: Vec::new(),
        physics: Some(Physics {
            gravity: -1.0,
            mass: 5.0,
            stiffness: 0.5,
            forward: 0.15,
            lateral: 0.15,
            aerodynamic: 0.0,
            stick_to_ground: true,
            airborne_friction: false,
        }),
    }
}

pub(super) fn fixture(kind: DeathType) -> (Skirmish, usize) {
    let mut game = game();
    let unit = &mut game.catalog[5];
    unit.death_lifecycle = Some(rules(kind));
    unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    unit.locomotor.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 0.1,
        acceleration_damaged: 0.1,
        braking: 0.1,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 5.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    game.update_visibility();
    (game, actor)
}

pub(super) fn kill(game: &mut Skirmish, actor: usize, kind: DeathType) {
    game.apply_typed_hit(actor, 0, 999_999.0, kind);
}

#[test]
fn normal_corpse_preserves_source_frame_delays_then_sinks_and_disappears() {
    let (mut game, actor) = fixture(DeathType::Normal);
    // Factory placement has already emitted its real terrain/Drawable timeline.
    let first_death_callback = game.presentation.weapon_fx.len();
    kill(&mut game, actor, DeathType::Normal);
    let death_frame = game.tick;
    assert!(game.entities[actor].corpse_present());
    assert_eq!(game.entities[actor].death_condition(), Some("DYING"));
    assert_eq!(
        game.presentation.object_fx_events[0].effect,
        "FX_TerroristDie"
    );
    let WeaponFxEvent::BodyParticles(body) = game.presentation.weapon_fx[first_death_callback]
    else {
        panic!("native body visual refresh must precede onDie FX");
    };
    assert_eq!(game.presentation.body_particles[body].pose.object, actor);
    assert!(matches!(
        game.presentation.weapon_fx[first_death_callback + 1],
        WeaponFxEvent::ObjectFx(0)
    ));
    assert_eq!(game.presentation.object_fx_events[0].object, actor);
    game.tick = death_frame + 89;
    game.update_deaths(game.entities.len());
    assert_eq!(game.object_position(actor)[2].to_bits(), 0.0_f32.to_bits());
    game.tick += 1;
    game.update_deaths(game.entities.len());
    assert_eq!(
        game.object_position(actor)[2].to_bits(),
        (-0.5 * STEP_SECONDS).to_bits()
    );
    // Multiple scheduler entry points cannot run the same update twice in one frame.
    game.update_deaths(game.entities.len());
    assert_eq!(
        game.object_position(actor)[2].to_bits(),
        (-0.5 * STEP_SECONDS).to_bits()
    );
    game.tick = death_frame + 240;
    game.update_deaths(game.entities.len());
    assert!(game.entities[actor].masked());
    assert!(!game.entities[actor].corpse_present());
}

#[test]
fn suicide_initial_fx_sees_fling_pose_and_reentrant_death_does_not_reroll() {
    let (mut game, actor) = fixture(DeathType::Suicided);
    let rule = &mut game.catalog[5].death_lifecycle.as_mut().unwrap().slow[0];
    rule.fling = Some(Fling {
        force: [8.0, 3.0],
        pitch: [60.0_f32.to_radians(), 10.0_f32.to_radians()],
    });
    rule.fx[0][0] = "FX_TerroristExplode".into();
    kill(&mut game, actor, DeathType::Suicided);
    assert_eq!(
        game.entities[actor].death_condition(),
        Some("EXPLODED_FLAILING")
    );
    let event = &game.presentation.object_fx_events[0];
    assert_eq!(event.position[2].to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        event.basis.map(|row| row.map(f32::to_bits)),
        crate::skirmish::physical_ocl::transform::yaw(game.entities[actor].facing)
            .map(|row| row.map(f32::to_bits))
    );
    assert_ne!(event.visible_players & 1, 0);
    let random = game.random;
    game.begin_slow_death(actor);
    assert_eq!(game.random, random);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    let initial_sink = game.entities[actor]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .slow
        .as_ref()
        .unwrap()
        .sink;
    game.tick += 1;
    game.integrate_death_body(actor);
    game.update_deaths(game.entities.len());
    assert!(game.object_position(actor)[2] > 1.0);
    assert_eq!(
        game.entities[actor]
            .death_lifecycle
            .as_ref()
            .unwrap()
            .slow
            .as_ref()
            .unwrap()
            .sink,
        initial_sink + 1
    );
}

#[test]
fn typed_poison_creates_original_replacement_then_lifetime_uses_normal_death() {
    let (mut game, actor) = fixture(DeathType::Poisoned);
    let mut child = crate::skirmish::tests::fixture_type(
        crate::skirmish::Faction::Gla,
        crate::skirmish::Kind::Infantry,
    );
    child.name = "ToxicInfantry_TerroristOnly".into();
    child.death_lifecycle = Some(rules(DeathType::Normal));
    child.behavior.lifetimes = vec![[90; 2]];
    crate::skirmish::tests::register_update(&mut child, "LifetimeUpdate");
    let child_index = game.catalog.len();
    game.catalog.push(child);
    let rule = &mut game.catalog[5].death_lifecycle.as_mut().unwrap().slow[0];
    rule.destruction_delay = [0, 0];
    rule.sink_delay = [0, 0];
    rule.sink_rate = 0.0;
    rule.objects[0].push(Spawn::Object {
        name: "ToxicInfantry_TerroristOnly".into(),
        skip_airborne: true,
    });
    game.set_object_orientation(actor, 1.25);
    kill(&mut game, actor, DeathType::Poisoned);
    let child = game.entities.len() - 1;
    assert_eq!(game.entities[child].type_index, child_index);
    assert_eq!(game.entities[child].owner, 0);
    assert_eq!(game.entities[child].facing.to_bits(), 1.25_f32.to_bits());
    assert!(game.entities[child].alive());
    game.update_deaths(child);
    assert!(game.entities[actor].masked());
    assert!(game.entities[child].alive());
    game.tick += 90;
    game.run_object_updates();
    game.update_deaths(game.entities.len());
    assert!(!game.entities[child].alive());
    assert!(game.entities[child].corpse_present());
    assert!(matches!(
        game.entities[child].damage_state.death_type,
        DeathType::Normal
    ));
}

#[test]
fn native_lifetime_wake_starts_normal_death_and_hides_zero_delay_corpse() {
    let (mut game, actor) = fixture(DeathType::Normal);
    let rule = game.catalog[5].death_lifecycle.as_mut().unwrap();
    rule.slow[0].destruction_delay = [0, 0];
    game.catalog[5].behavior.lifetimes = vec![[1; 2]];
    crate::skirmish::tests::register_update(&mut game.catalog[5], "LifetimeUpdate");
    let actor = game.spawn(5, 0, game.entities[actor].position, true);
    game.tick += 1;
    game.run_object_updates();
    game.update_deaths(game.entities.len());
    assert!(game.entities[actor].masked());
    assert_eq!(game.presentation.object_fx_events.len(), 1);
}

#[test]
fn battle_bus_slow_death_overkill_uses_integer_modifier_after_source_percent() {
    let mut rule = slow(DeathType::Normal);
    rule.probability = 5;
    rule.overkill_bonus = 0.2;
    assert_eq!(probability(&rule, 2600, 650.0), 5);
    assert_eq!(probability(&rule, 3250, 650.0), 6);
    rule.overkill_bonus = -2.0;
    assert_eq!(probability(&rule, 3250, 650.0), 1);
}

#[test]
fn inactive_body_slow_death_uses_x86_indefinite_modifier_and_retires() {
    let (mut game, _) = fixture(DeathType::Normal);
    game.catalog[5].body_kind = crate::skirmish::BodyKind::Inactive;
    game.catalog[5].max_health = 0.0;
    game.catalog[5].initial_health = 0.0;
    let actor = game.spawn(5, 0, [350.0, 350.0], true);
    assert_eq!(game.entities[actor].max_health.to_bits(), 0.0_f32.to_bits());
    assert_eq!(probability(&slow(DeathType::Normal), 0, 0.0), 1);
    game.kill_object(actor, DeathType::Normal);
    assert!(
        game.entities[actor]
            .death_lifecycle
            .as_ref()
            .unwrap()
            .slow
            .is_some()
    );
    for _ in 0..300 {
        game.step();
    }
    assert!(!game.entities[actor].exists());
}
