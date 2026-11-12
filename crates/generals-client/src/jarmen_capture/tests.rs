use super::*;
use crate::{control_bar, playtest_ui::selected_command, source_audio::Voice};
use generals_sim::skirmish::Order;
mod animation;

#[ignore = "requires original game installation"]
#[test]
fn paid_native_jarmen_preparation_preserves_original_targeting_and_both_live_victims() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = prepare_targets(&mut battle.simulation).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    drop(crate::attack_cursor::load(&assets).unwrap());
    battle.select_objects(&[setup.sniper], true);
    let command = selected_command(&battle, &data, 1).unwrap();
    assert!(matches!(command.action, CommandAction::SnipeVehicle(1)));
    dispatch_command(&mut battle, &mut mouse, command.action);
    assert!(matches!(
        mouse.pending_command,
        CommandMode::SnipeVehicle(1)
    ));
    assert!(matches!(
        mouse.voices.last().unwrap().voice(),
        Voice::SnipeMode
    ));
    mouse.hovered = Some(setup.vehicle);
    assert_eq!(crate::attack_cursor::context(&mut battle, &mouse), Some(11));
    let health = battle.simulation.entities[setup.vehicle].health.to_bits();
    assert!(input::pilot::issue(
        &mut battle,
        &mut mouse,
        setup.vehicle,
        1
    ));
    assert!(matches!(
        mouse.voices.last().unwrap().voice(),
        Voice::SnipePilot
    ));
    advance_until(&mut battle.simulation, 1500, "native pilot shot", |game| {
        game.entities[setup.vehicle].unmanned()
    })
    .unwrap();
    assert_eq!(
        battle.simulation.entities[setup.vehicle].health.to_bits(),
        health
    );
    assert!(
        battle
            .simulation
            .pilot_reload_progress(setup.sniper, 1)
            .is_some()
    );
    assert!(battle.simulation.entities[setup.infantry].alive());
    battle.select_objects(&[setup.driver], true);
    let point = Vec2::from_array(battle.simulation.entities[setup.vehicle].position);
    assert!(input::context_order(
        &mut battle,
        Some(point),
        Some(PickTarget::Unit(setup.vehicle)),
        &mut mouse
    ));
    advance_until(
        &mut battle.simulation,
        9000,
        "native paid infantry takeover",
        |game| game.entities[setup.vehicle].owner == 0 && !game.entities[setup.vehicle].unmanned(),
    )
    .unwrap();
    assert!(!battle.simulation.entities[setup.driver].alive());
    assert!(battle.simulation.entities[setup.infantry].alive());
    battle.select_objects(&[setup.sniper], true);
    let point = Vec2::from_array(battle.simulation.entities[setup.infantry].position);
    assert!(input::context_order(
        &mut battle,
        Some(point),
        Some(PickTarget::Unit(setup.infantry)),
        &mut mouse
    ));
    assert!(matches!(
        mouse.voices.last().unwrap().voice(),
        Voice::Attack
    ));
    let battle = animation::rifle_timeline(&assets, battle, &setup);
    assert!(battle.simulation.entities[setup.sniper].experience > 0);
}

#[ignore = "requires original game installation"]
#[test]
fn paid_capture_preparation_runs_native_cloak_updates_before_freezing_the_image() {
    use crate::stealth_view::{self, Cloaks};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn step(battle: ResMut<Match>) {
        battle.into_inner().simulation.step();
    }

    let (_, mut battle, animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            34,
        )))
        .insert_resource(animations)
        .insert_resource(Cloaks::new())
        .add_systems(FixedUpdate, (step, stealth_view::record).chain());
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    prepare(&mut app, &mut battle).unwrap();
    let actor = app.world().resource::<Check>().setup.sniper;
    let initial_tick = battle.simulation.tick;
    assert!(battle.simulation.entities[actor].stealthed());
    assert!(
        app.world()
            .resource::<Cloaks>()
            .base_opacity(actor)
            .is_none()
    );
    app.insert_resource(battle);
    for _ in 0..4 {
        app.update();
    }
    let battle = app.world().resource::<Match>();
    assert!(battle.simulation.tick > initial_tick);
    assert!(battle.simulation.entities[actor].stealthed());
    let opacity = app
        .world()
        .resource::<Cloaks>()
        .base_opacity(actor)
        .unwrap();
    assert!(
        opacity > 0.0 && opacity < 1.0,
        "native cloak opacity={opacity}"
    );

    // The actual image stage pauses only after its rendered cloak check passes.
    let tick = battle.simulation.tick;
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(app.world().resource::<Match>().simulation.tick, tick);
    assert_eq!(
        app.world()
            .resource::<Cloaks>()
            .base_opacity(actor)
            .unwrap()
            .to_bits(),
        opacity.to_bits()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn shared_primary_reload_rejects_pilot_input_until_the_native_clock_is_ready() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let kind = crate::strategy_verification::template(&battle.simulation, "GLAInfantryJarmenKell")
        .unwrap();
    // Authored custom-map weapon variant: one rifle round shares its clip clock
    // with the existing pilot weapon. Allocate the paid unit from that variant.
    battle.simulation.catalog[kind].weapon.clip_size = 1;
    battle.simulation.catalog[kind].weapon.reload_seconds = 4.0;
    battle.simulation.catalog[kind]
        .weapon_set_rules
        .share_reload = true;
    let setup = prepare_targets(&mut battle.simulation).unwrap();
    battle.select_objects(&[setup.sniper], true);
    let previous_shots = battle.simulation.presentation.shots.len();
    battle
        .simulation
        .attack(setup.sniper, setup.infantry)
        .unwrap();
    advance_until(
        &mut battle.simulation,
        1500,
        "actual shared rifle reload",
        |game| {
            game.presentation
                .shots
                .iter()
                .skip(previous_shots)
                .any(|shot| shot.source == setup.sniper && shot.weapon_slot == 0)
        },
    )
    .unwrap();
    battle.simulation.stop(setup.sniper).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let mut mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    mouse.pending_command = CommandMode::SnipeVehicle(1);
    mouse.hovered = Some(setup.vehicle);
    assert!(
        battle
            .simulation
            .pilot_reload_progress(setup.sniper, 1)
            .is_some()
    );
    assert!(!input::pilot::can_target(&mut battle, setup.vehicle, 1));
    assert_eq!(crate::attack_cursor::context(&mut battle, &mouse), Some(3));
    let voices = mouse.voices.len();
    assert!(matches!(
        battle.simulation.entities[setup.sniper].ai.order,
        Order::Idle
    ));
    assert!(!input::pilot::issue(
        &mut battle,
        &mut mouse,
        setup.vehicle,
        1
    ));
    assert_eq!(mouse.voices.len(), voices);
    assert!(matches!(
        battle.simulation.entities[setup.sniper].ai.order,
        Order::Idle
    ));
    for _ in 0..122 {
        battle.simulation.step();
    }
    assert!(input::pilot::can_target(&mut battle, setup.vehicle, 1));
    assert_eq!(crate::attack_cursor::context(&mut battle, &mouse), Some(11));
    assert!(input::pilot::issue(
        &mut battle,
        &mut mouse,
        setup.vehicle,
        1
    ));
    assert!(matches!(battle.simulation.entities[setup.sniper].ai.order,
        Order::Attack(target) if target == setup.vehicle));
}
