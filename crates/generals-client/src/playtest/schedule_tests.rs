//! Build the actual game schedules without a window, GPU or system execution.
use super::{configure_app, configure_fixed_update};
use bevy::{ecs::schedule::Schedules, prelude::*};

#[test]
fn actual_update_and_fixed_graphs_initialize_without_cycles() {
    let mut app = App::new();
    configure_app(&mut app);
    configure_fixed_update(&mut app);
    let world = app.world_mut();
    let mut schedules = world.remove_resource::<Schedules>().unwrap();
    schedules
        .get_mut(Update)
        .unwrap()
        .initialize(world)
        .unwrap();
    schedules
        .get_mut(FixedUpdate)
        .unwrap()
        .initialize(world)
        .unwrap();
    let names = schedules
        .get(Update)
        .unwrap()
        .systems()
        .unwrap()
        .map(|(_, system)| system.name().to_string())
        .collect::<Vec<_>>();
    let position = |suffix: &str| {
        names
            .iter()
            .position(|name| name.ends_with(suffix))
            .unwrap()
    };
    let flashes = position("selection_feedback::update_flashes");
    let fade = position("stealth_view::fade");
    let render = position("playtest_view::render");
    let supply = position("supply_presentation::update");
    let bars = position("selection_feedback::health_bars");
    assert!(position("sway::update") < flashes);
    assert!(flashes < fade && fade < render);
    assert!(render < supply && supply < bars);
    let fixed = schedules
        .get(FixedUpdate)
        .unwrap()
        .systems()
        .unwrap()
        .map(|(_, system)| system.name().to_string())
        .collect::<Vec<_>>();
    let records = fixed
        .iter()
        .enumerate()
        .filter_map(|(index, name)| name.ends_with("weapon_fx::record").then_some(index))
        .collect::<Vec<_>>();
    let step = fixed
        .iter()
        .position(|name| name.ends_with("playtest::step"))
        .unwrap();
    assert_eq!(records.len(), 2);
    assert!(records[0] < step && step < records[1]);
}

// Exercises real registration only: system parameters initialize, but systems do
// not run and no simulated actors, renderer or GPU resources are fabricated.
pub fn assert_capture_graph(register: fn(&mut App), observer: &str) {
    let mut app = App::new();
    // Capture diagnostics register render-world systems against this real SubApp;
    // initializing schedules does not require a GPU or executing those systems.
    app.insert_sub_app(bevy::render::RenderApp, bevy::app::SubApp::new());
    configure_app(&mut app);
    configure_fixed_update(&mut app);
    register(&mut app);
    let world = app.world_mut();
    let mut schedules = world.remove_resource::<Schedules>().unwrap();
    for (label, schedule) in schedules.iter_mut() {
        schedule
            .initialize(world)
            .unwrap_or_else(|error| panic!("{label:?}: {error}"));
    }
    let fixed = schedules
        .get(FixedUpdate)
        .unwrap()
        .systems()
        .unwrap()
        .map(|(_, system)| system.name().to_string())
        .collect::<Vec<_>>();
    let records = fixed
        .iter()
        .enumerate()
        .filter_map(|(index, name)| name.ends_with("weapon_fx::record").then_some(index))
        .collect::<Vec<_>>();
    let position = |suffix: &str| {
        fixed
            .iter()
            .position(|name| name.ends_with(suffix))
            .unwrap_or_else(|| panic!("missing {suffix} in {fixed:?}"))
    };
    assert_eq!(records.len(), 2);
    assert!(records[0] < position("playtest::step"));
    assert!(position("playtest::step") < records[1]);
    assert!(records[1] < position("laser_beam::record"));
    assert!(position("laser_beam::record") < position(observer));
    if observer.ends_with("infantry_verification::recording::record") {
        assert!(position(observer) < position("particles::effects::step"));
    }
}

#[test]
fn affected_capture_graphs_initialize_after_current_tick_effects() {
    for (register, observer) in [
        (
            crate::microwave_capture::register as fn(&mut App),
            "microwave_capture::record",
        ),
        (crate::ecm_capture::register, "ecm_capture::record"),
        (
            crate::structure_capture::register,
            "structure_capture::record",
        ),
        (
            crate::china_aircraft_capture::register,
            "china_aircraft_capture::record",
        ),
        (
            crate::aircraft_capture::register,
            "aircraft_capture::record_shots",
        ),
    ] {
        assert_capture_graph(register, observer);
    }
    assert_capture_graph(
        register_infantry,
        "infantry_verification::recording::record",
    );
}

fn register_infantry(app: &mut App) {
    super::register_diagnostics(
        app,
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
        &super::ScreenCapture::Infantry(generals_sim::skirmish::Faction::Usa),
        None,
        None,
        None,
    );
}
