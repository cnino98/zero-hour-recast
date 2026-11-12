//! Render inspection of the original captured Hospital and Refinery map records.
use crate::{
    playtest::{CameraRig, Match},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::prelude::*;
use std::{
    error::Error,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Check {
    index: usize,
    started: Instant,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match, refinery: bool) -> Result<(), Box<dyn Error>> {
    let (target, ranger) =
        crate::neutral_tech_verification::prepare(&mut battle.simulation, refinery)?;
    battle.started = true;
    battle.select_objects(&[target], true);
    let position = battle.simulation.entities[target].position;
    // Keep the paid capturer beside its authored captured structure for the view.
    battle
        .simulation
        .move_to(ranger, [position[0] + 80.0, position[1] + 50.0])?;
    app.insert_resource(Check {
        index: target,
        started: Instant::now(),
        done: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(PreUpdate, focus);
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}

fn inspect(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    roots: Query<(&UnitVisual, &Children)>,
    parts: Query<&ViewVisibility, With<Mesh3d>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done {
        return;
    }
    if check.started.elapsed() > Duration::from_secs(45) {
        check.done = true;
        error!("Neutral tech native verification timed out waiting for prepared capture");
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    let visible = roots
        .iter()
        .find(|(visual, _)| visual.index == check.index && visual.generation == battle.generation)
        .is_some_and(|(_, children)| {
            children
                .iter()
                .any(|child| parts.get(child).is_ok_and(|visible| visible.get()))
        });
    check.done = true;
    match require(
        visible,
        "captured original neutral tech model is not rendered",
    ) {
        Ok(()) => {
            capture.finish_scenario();
            println!("Rendered neutral tech: original captured structure mesh is visible");
        }
        Err(error) => {
            error!("Neutral tech native verification: {error}");
            exit.write(AppExit::error());
        }
    }
}

fn focus(
    check: Res<Check>,
    battle: Res<Match>,
    mut camera: ResMut<CameraRig>,
    terrain: Res<Terrain>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    let actor = &battle.simulation.entities[check.index];
    let mut center = battle.simulation.object_position(check.index);
    center[2] += battle.simulation.catalog[actor.type_index]
        .geometry_bounds
        .center_height;
    crate::capture_framing::focus_on(&mut camera, terrain, center);
}
