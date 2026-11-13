//! Native captured neutral tech, artillery projectile and docked vehicle views.
pub mod damage;
use crate::{
    capture_framing::AircraftFrame,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::{
    error::Error,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Check {
    platform: usize,
    requires_shell: bool,
    vehicle: Option<usize>,
    started: Instant,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (platform, _) = crate::neutral_expanded_verification::prepare_shot(&mut battle.simulation)?;
    for _ in 0..4 {
        battle.simulation.step();
    }
    require(
        battle
            .simulation
            .projectiles
            .iter()
            .any(|projectile| projectile.source == platform),
        "original artillery shell expired before view",
    )?;
    battle.started = true;
    battle.select_objects(&[platform], true);
    register(app, platform, true, None);
    Ok(())
}

pub fn prepare_repair_bay(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let platform =
        crate::neutral_expanded_verification::prepare_repair_bay(&mut battle.simulation)?;
    battle.started = true;
    battle.select_objects(&[platform], true);
    register(app, platform, false, None);
    Ok(())
}

pub fn prepare_repair_pad(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (pad, vehicle) =
        crate::neutral_expanded_verification::prepare_repair_pad(&mut battle.simulation)?;
    battle.started = true;
    battle.select_objects(&[vehicle], true);
    register(app, pad, false, Some(vehicle));
    Ok(())
}

fn register(app: &mut App, platform: usize, requires_shell: bool, vehicle: Option<usize>) {
    app.insert_resource(Check {
        platform,
        requires_shell,
        vehicle,
        started: Instant::now(),
        done: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        Update,
        focus
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

#[derive(SystemParam)]
struct View<'w, 's> {
    frame: AircraftFrame<'w, 's>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    parts: Query<'w, 's, &'static ViewVisibility, With<Mesh3d>>,
}

fn inspect(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    view: View,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done {
        return;
    }
    if check.started.elapsed() > Duration::from_secs(45) {
        check.done = true;
        error!("Neutral tech native verification timed out");
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    let View {
        frame,
        projectiles,
        parts,
    } = view;
    let platform = check.platform;
    let building = frame.visible(battle, check.platform);
    let shell = projectiles.iter().any(|(visual, children)| {
        visual.generation == battle.generation
            && battle
                .simulation
                .projectiles
                .iter()
                .any(|projectile| projectile.id == visual.id && projectile.source == platform)
            && children
                .iter()
                .any(|child| parts.get(child).is_ok_and(|visible| visible.get()))
    });
    check.done = true;
    let result = building.and_then(|building| {
        let vehicle = check
            .vehicle
            .map_or(Ok(true), |id| frame.visible(battle, id))?;
        require(
            building && (!check.requires_shell || shell) && vehicle,
            "captured tech geometry, requested live shell or docked tank is cropped or covered",
        )
    });
    match result {
        Ok(()) => {
            capture.finish_scenario();
            println!(
                "Rendered neutral tech: original captured building and requested subject meshes visible"
            );
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
    camera.distance = camera.settings.max_height;
    let mut center = battle.simulation.object_position(check.platform);
    let actor = &battle.simulation.entities[check.platform];
    center[2] += battle.simulation.catalog[actor.type_index]
        .geometry_bounds
        .center_height;
    crate::capture_framing::focus_on(&mut camera, terrain.into_inner(), center);
}
