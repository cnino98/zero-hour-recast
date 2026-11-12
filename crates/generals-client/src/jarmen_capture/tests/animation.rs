//! Actual paid one-shot rifle state and its real animated drawable.
use super::*;
use crate::{model_animation, playtest_view::animation_fixture};
use bevy::{asset::AssetPlugin, ecs::system::RunSystemOnce, time::TimeUpdateStrategy};
use generals_assets::virtual_assets::VirtualAssets;
use std::time::Duration;

fn fixture(assets: &VirtualAssets, battle: Match, setup: &Setup) -> (App, Entity) {
    let mut models = Vec::new();
    let mut animations =
        model_animation::load(assets, &mut models, &battle.simulation.catalog).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>();
    let geometry =
        animation_fixture::geometry(&models, &mut app.world_mut().resource_mut::<Assets<Mesh>>());
    animations.retain_models(models, 123);
    let root = app
        .world_mut()
        .spawn((
            crate::playtest_view::UnitVisual {
                index: setup.sniper,
                generation: battle.generation,
            },
            model_animation::Players(Vec::new()),
            Transform::IDENTITY,
            Visibility::Visible,
        ))
        .id();
    app.insert_resource(geometry)
        .insert_resource(animations)
        .insert_resource(model_animation::Clock::new())
        .insert_resource(battle)
        .add_systems(
            Update,
            (model_animation::update_clock, model_animation::update).chain(),
        );
    app.update();
    (app, root)
}

fn pose(app: &App, root: Entity, model: usize) -> crate::jarmen_capture::animation::Pose {
    let world = app.world();
    let meshes = world.resource::<Assets<Mesh>>();
    crate::jarmen_capture::animation::Pose::from_meshes(
        world
            .get::<Children>(root)
            .unwrap()
            .iter()
            .filter_map(|child| {
                let part = world.get::<model_animation::AnimatedPart>(child)?;
                let mesh = world.get::<Mesh3d>(child)?;
                // This device-free fixture samples the actual posed parts;
                // the native View additionally requires ViewVisibility.
                (part.model == model && *world.get::<Visibility>(child)? != Visibility::Hidden)
                    .then(|| meshes.get(&mesh.0).map(|mesh| (child, mesh)))
                    .flatten()
            }),
    )
    .expect("real animated Jarmen mesh missing")
}

fn capture_check(battle: &Match, setup: &Setup) -> Check {
    let game = &battle.simulation;
    Check {
        setup: Setup {
            sniper: setup.sniper,
            vehicle: setup.vehicle,
            driver: setup.driver,
            infantry: setup.infantry,
        },
        stage: 7,
        audio: Heard {
            sounds: [false; 6],
            mode_finished: false,
            pilot_finished: false,
        },
        shots: [0; 2],
        pilot_event: false,
        vehicle_health: game.entities[setup.vehicle].health.to_bits(),
        losses: [0, 1].map(|owner| game.players[owner].statistics.units_lost),
        kills: game.players[0].statistics.units_destroyed,
        animation: crate::jarmen_capture::animation::Proof::new(),
        held_overstep: None,
    }
}

fn step(battle: ResMut<Match>) {
    battle.into_inner().simulation.step();
}

fn observe(
    app: &App,
    root: Entity,
    setup: &Setup,
    proof: &mut crate::jarmen_capture::animation::Proof,
) {
    let game = &app.world().resource::<Match>().simulation;
    let actor = &game.entities[setup.sniper];
    let firing = actor.weapon_condition() == Some("FIRING_A")
        && game.presentation.shots.iter().any(|shot| {
            shot.source == setup.sniper
                && shot.target == Some(setup.infantry)
                && shot.weapon_slot == 0
        });
    for sample in app
        .world()
        .resource::<model_animation::Animations>()
        .inspect(
            app.world().get::<model_animation::Players>(root).unwrap(),
            actor.type_index,
        )
        .unwrap()
    {
        proof.observe(
            &sample.clip,
            sample.frame,
            pose(app, root, sample.model),
            actor.moving(),
            firing,
        );
        if firing {
            assert_eq!(sample.clip, "uihero_skl.uihero_ata");
            assert_eq!(sample.frame.to_bits(), 0.0_f32.to_bits());
        }
    }
}

fn held_time_is_restored(app: &mut App, root: Entity, setup: &Setup) {
    let saved = app.world().resource::<Check>().held_overstep.unwrap();
    let tick = app.world().resource::<Match>().simulation.tick;
    let timestep = app.world().resource::<Time<Fixed>>().timestep();
    assert!(saved > Duration::ZERO);
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
    assert_eq!(
        app.world().resource::<Time<Fixed>>().overstep(),
        Duration::ZERO
    );
    // Actual fixed scheduling must retain the shot state through more than one
    // rendered frame, even when a frame normally consumes several logic ticks.
    for _ in 0..2 {
        app.update();
        assert_eq!(app.world().resource::<Match>().simulation.tick, tick);
        let mut proof = crate::jarmen_capture::animation::Proof::new();
        observe(app, root, setup, &mut proof);
        assert_eq!(app.world().resource::<Check>().held_overstep, Some(saved));
    }
    app.world_mut()
        .run_system_once(
            |mut check: ResMut<Check>,
             mut clock: ResMut<Time<Virtual>>,
             mut fixed: ResMut<Time<Fixed>>| {
                stages::resume_after_rifle_pose(&mut check, &mut clock, &mut fixed);
            },
        )
        .unwrap();
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    assert!(app.world().resource::<Check>().held_overstep.is_none());
    assert_eq!(app.world().resource::<Time<Fixed>>().overstep(), saved);
    // With no new virtual time, only the preserved remainder may be consumed.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    let consumed = u64::try_from(saved.as_nanos() / timestep.as_nanos()).unwrap();
    assert_eq!(
        app.world().resource::<Match>().simulation.tick,
        tick + consumed
    );
    assert_eq!(
        app.world().resource::<Time<Fixed>>().overstep().as_nanos(),
        saved.as_nanos() % timestep.as_nanos()
    );
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        34,
    )));
    app.update();
    let game = &app.world().resource::<Match>().simulation;
    assert!(game.tick > tick);
    assert_ne!(
        game.entities[setup.sniper].weapon_condition(),
        Some("FIRING_A")
    );
    assert_eq!(app.world().resource::<Check>().shots[0], 1);
}

pub(super) fn rifle_timeline(assets: &VirtualAssets, battle: Match, setup: &Setup) -> Match {
    let check = capture_check(&battle, setup);
    let (mut app, root) = fixture(assets, battle, setup);
    app.insert_resource(check)
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            150,
        )))
        .add_systems(FixedUpdate, (step, record).chain());
    let mut proof = crate::jarmen_capture::animation::Proof::new();
    observe(&app, root, setup, &mut proof);
    for _ in 0..1200 {
        app.update();
        observe(&app, root, setup, &mut proof);
        if app.world().resource::<Check>().held_overstep.is_some() {
            break;
        }
    }
    assert_eq!(app.world().resource::<Check>().shots[0], 1);
    assert!(!app.world().resource::<Match>().simulation.entities[setup.infantry].alive());
    assert!(
        proof.movement,
        "ordinary approach did not advance its actual visible mesh"
    );
    assert!(
        proof.shot,
        "actual rifle shot did not select a changed authored attack pose"
    );
    held_time_is_restored(&mut app, root, setup);
    app.world_mut().remove_resource::<Match>().unwrap()
}
