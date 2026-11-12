//! Real paid bodies retained through asynchronous-view pause transitions.
use super::*;
use bevy::{ecs::system::RunSystemOnce, time::TimeUpdateStrategy};

#[ignore = "requires original game installation"]
#[test]
fn installed_paid_crash_quarter_views_keep_one_snapshot_until_last_falling_view() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = crash::prepare(&mut battle.simulation).unwrap();
    crash::begin(&mut battle.simulation, &setup).unwrap();
    for _ in 0..7200 {
        crash::attack_when_visible(&mut battle.simulation, &setup).unwrap();
        battle.simulation.step();
        if !battle.simulation.entities[setup.aircraft.raptor].alive() {
            break;
        }
    }
    let death = crash::verify_death(&battle.simulation, &setup).unwrap();
    let body = setup.aircraft.raptor;
    battle.started = true;
    battle.paused = false;
    let mut app = App::new();
    app.add_plugins(bevy::time::TimePlugin);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f64(1.0 / 30.0),
    ));
    app.insert_resource(battle);
    app.insert_resource(crate::model_animation::Clock::new());
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.update();
    app.world_mut()
        .run_system_once(crate::model_animation::update_clock)
        .unwrap();
    let snapshot = paused_body(&app, body);
    let mut check = Check {
        setup,
        stage: Stage::FallingCaptured,
        death: Some(death),
        ground: None,
        hulk: None,
        sounds: [true; 3],
        began: None,
        seen: [true; 4],
        snapshot: Some(snapshot),
        view_index: 0,
    };
    for view in 0..4 {
        assert_eq!(check.view_index, view);
        verify_paused_updates(&mut app, check.snapshot.as_ref().unwrap());
        check.next_view(&mut app.world_mut().resource_mut::<Time<Virtual>>());
        if view < 3 {
            assert!(matches!(check.stage, Stage::FallingView));
            assert!(app.world().resource::<Time<Virtual>>().is_paused());
            // Only the completed intermediate screenshot makes control eligible
            // again; real Capture's existing waiting handshake supplies that edge.
            check.stage = Stage::FallingCaptured;
        }
    }
    assert!(matches!(check.stage, Stage::Ground));
    assert!(check.snapshot.is_none());
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    let hulk = ordinary_hulk(&mut app, body);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    let snapshot = paused_body(&app, hulk);
    verify_paused_updates(&mut app, &snapshot);
    app.world_mut().resource_mut::<Match>().simulation.step();
    assert!(
        snapshot
            .unchanged(
                app.world().resource::<Match>(),
                app.world().resource::<crate::model_animation::Clock>(),
                app.world().resource::<Time<Virtual>>(),
            )
            .is_err(),
        "an actual simulation step must invalidate the frozen source snapshot"
    );
}

fn paused_body(app: &App, body: usize) -> views::Snapshot {
    views::Snapshot::new(
        app.world().resource::<Match>(),
        app.world().resource::<crate::model_animation::Clock>(),
        app.world().resource::<Time<Virtual>>(),
        body,
        0.0,
    )
}

fn verify_paused_updates(app: &mut App, snapshot: &views::Snapshot) {
    for _ in 0..3 {
        app.update();
        app.world_mut()
            .run_system_once(crate::model_animation::update_clock)
            .unwrap();
        snapshot
            .unchanged(
                app.world().resource::<Match>(),
                app.world().resource::<crate::model_animation::Clock>(),
                app.world().resource::<Time<Virtual>>(),
            )
            .unwrap();
    }
}

fn ordinary_hulk(app: &mut App, producer: usize) -> usize {
    for _ in 0..7200 {
        let mut battle = app.world_mut().resource_mut::<Match>();
        battle.simulation.step();
        if let Some(id) = battle.simulation.entities.iter().position(|actor| {
            actor.producer() == Some(producer)
                && actor.physical_present()
                && battle.simulation.catalog[actor.type_index].name == "AmericaJetRaptorHulk"
        }) {
            return id;
        }
    }
    panic!("ordinary original paid crash must produce its native hulk");
}
