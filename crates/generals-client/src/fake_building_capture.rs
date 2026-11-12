//! Native original fake-building mesh, command and singleton-selection replacement.
use crate::{
    capture_framing::AircraftFrame,
    control_bar::CommandAction,
    fake_building_verification,
    input::Mouse,
    playtest::{CameraRig, Match},
    playtest_ui::dispatch_command,
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    old: usize,
    next: Option<usize>,
    phase: u8,
    began: Option<f64>,
    camera: Option<(Vec2, f32)>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = fake_building_verification::prepare(&mut battle.simulation)?;
    let old = setup.fakes[1];
    let kind = battle.simulation.entities[old].type_index;
    let cost = battle.simulation.catalog[kind].maintenance.research[0].cost;
    crate::skirmish_verification::advance_until(
        &mut battle.simulation,
        18000,
        "native conversion funding",
        |game| game.players[0].cash >= cost,
    )?;
    battle.select_objects(&[old], true);
    battle.observer_player = None;
    app.insert_resource(Check {
        old,
        next: None,
        phase: 0,
        began: None,
        camera: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        hold_camera
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

fn hold_camera(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    if check.camera.is_none() {
        crate::capture_framing::focus_on(
            &mut camera,
            terrain,
            battle.simulation.object_position(check.old),
        );
        camera.distance = camera.settings.max_height;
        check.camera = Some((camera.focus, camera.distance));
    }
    let (focus, distance) = check
        .camera
        .expect("initialized original building capture camera");
    camera.focus = focus;
    camera.distance = distance;
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    framing: AircraftFrame<'w, 's>,
    camera: Res<'w, CameraRig>,
}

fn inspect(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    view: View,
    mut exit: MessageWriter<AppExit>,
) {
    let View {
        clock,
        framing,
        camera,
    } = view;
    let camera = camera.into_inner();
    if check.phase == 3 {
        return;
    }
    let now = clock.elapsed_secs_f64();
    let began = *check.began.get_or_insert(now);
    if now - began >= 60.0 {
        error!(
            "Fake-building native timeout phase {} frame {} old {} next {:?} old_framed={:?} next_framed={:?} camera_focus={:?} camera_distance={} source_min={} source_max={}",
            check.phase,
            battle.simulation.tick,
            check.old,
            check.next,
            framing.visible(&battle, check.old),
            check.next.map(|next| framing.visible(&battle, next)),
            camera.focus,
            camera.distance,
            camera.settings.min_height,
            camera.settings.max_height
        );
        check.phase = 3;
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        match check.phase {
            0 if framing.visible(&battle, check.old)? => {
                capture.intermediate("fake-barracks-before-conversion.png");
                check.phase = 1;
            }
            1 => {
                dispatch_command(&mut battle, &mut mouse, CommandAction::Research(0));
                require(
                    !battle.simulation.entities[check.old].queue.is_empty(),
                    "Native conversion command was rejected",
                )?;
                time.unpause();
                check.phase = 2;
            }
            2 => {
                if check.next.is_none() {
                    check.next = battle
                        .simulation
                        .presentation
                        .replacement_events
                        .iter()
                        .find(|event| event.previous == check.old)
                        .map(|event| event.next);
                }
                if let Some(next) = check.next
                    && framing.visible(&battle, next)?
                {
                    let game = &battle.simulation;
                    let actor = &game.entities[next];
                    require(
                        game.entities[check.old].masked()
                            && !game.catalog[actor.type_index].fake_building()
                            && game.catalog[actor.type_index].name == "GLABarracks"
                            && actor.health.to_bits()
                                == game.catalog[actor.type_index].initial_health.to_bits(),
                        "Native conversion failed original replacement contract",
                    )?;
                    require(
                        battle.selected == vec![next],
                        "Native first-selected replacement did not restore singleton selection",
                    )?;
                    time.pause();
                    check.phase = 3;
                    capture.finish_scenario();
                    println!(
                        "FAKE_BUILDINGS_RENDERED original_fake_and_real_meshes=true native_upgrade_command=true replacement_selected=true full_uncovered_bounds=true"
                    );
                }
            }
            _ => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Fake-building native acceptance: {error}");
        check.phase = 3;
        exit.write(AppExit::error());
    }
}
