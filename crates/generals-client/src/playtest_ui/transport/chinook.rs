//! Original Chinook inventory actions and its authoritative landing transform.
use super::{passenger, visible};
use crate::{
    capture_framing::AircraftFrame,
    control_bar::{CommandAction, UiData},
    input::{self, Mouse, PickTarget},
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{advance_until, require},
    terrain::Terrain,
    weapon_fx::RopeSegment,
};
use bevy::{
    app::AppExit, ecs::system::SystemParam, prelude::*, ui_render::ui_material::MaterialNode,
};
use std::{error::Error, time::Duration};

#[derive(Resource)]
struct Check {
    host: usize,
    riders: Vec<usize>,
    phase: Phase,
    started: Option<Duration>,
}

#[derive(Debug)]
enum Phase {
    Landing,
    WaitForCapture,
    Unload,
    Board,
    Rappel,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (host, riders) = crate::transport_verification::chinook::prepare(&mut battle.simulation)?;
    battle.select_objects(&[host], true);
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("pointer missing")?;
    let rider = passenger(battle, 0).ok_or("Chinook first passenger absent")?;
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(0));
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        3000,
        "aircraft UI individual exit",
        |game| game.entities[rider].contained_by().is_none(),
    )?;
    battle.select_objects(&[rider], true);
    require(
        input::context_order(battle, None, Some(PickTarget::Unit(host)), &mut mouse),
        "Chinook context boarding rejected",
    )?;
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        6000,
        "aircraft UI reboarding and takeoff",
        |game| {
            game.entities[rider].contained_by() == Some(host)
                && game.height_above_ground(host) > 97.0
        },
    )?;
    battle.select_objects(&[host], true);
    dispatch_command(battle, &mut mouse, CommandAction::Evacuate);
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        1800,
        "aircraft UI evacuation landing",
        |game| game.height_above_ground(host) < 20.0 && !game.entities[host].landed(),
    )?;
    require(
        battle.simulation.passengers(host).len() == riders.len(),
        "Chinook unloaded before landing",
    )?;
    mouse.voices.clear();
    app.insert_resource(mouse).insert_resource(Check {
        host,
        riders,
        phase: Phase::Landing,
        started: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(PreUpdate, advance);
    app.add_systems(
        Update,
        focus
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
    app.add_systems(
        PostUpdate,
        inspect_rappel
            .after(bevy::ui::UiSystems::PostLayout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    println!(
        "Chinook UI: original mixed-slot inventory, individual exit/context reentry and evacuation command passed; capturing actual landing"
    );
    Ok(())
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w> {
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    data: Res<UiData>,
    visuals: Query<(&UnitVisual, &Visibility, &Transform)>,
    buttons: Query<(&Action, &Node, &MaterialNode<PromotionIcon>)>,
    icons: Res<Assets<PromotionIcon>>,
    mut completion: Completion,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let icons = icons.into_inner();
    if !matches!(check.phase, Phase::Landing) || !completion.capture.ready_for_scenario() {
        return;
    }
    let present = |id| {
        visuals
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
    };
    let Some((_, visibility, transform)) = present(check.host) else {
        return;
    };
    let count = buttons
        .iter()
        .filter(|(action, node, material)| {
            let Action::Slot(slot) = **action else {
                return false;
            };
            let Some(command) = selected_command(battle, data, slot) else {
                return false;
            };
            let CommandAction::ExitContainer(index) = command.action else {
                return false;
            };
            let Some(id) = passenger(battle, index) else {
                return false;
            };
            let expected =
                data.icon(&data.units[battle.simulation.entities[id].type_index].button_image);
            visible(battle, index)
                && node.display != Display::None
                && icons.get(&material.0).is_some_and(|icon| {
                    icon.image == expected.image
                        && icon.uv_rect == expected.uv_rect
                        && icon.state.x < 0.5
                })
        })
        .count();
    let hidden = check
        .riders
        .iter()
        .all(|&id| present(id).is_some_and(|(_, visibility, _)| *visibility == Visibility::Hidden));
    let height = battle.simulation.object_position(check.host)[2] * 0.1;
    if *visibility != Visibility::Visible
        || !hidden
        || count != check.riders.len()
        || (transform.translation.y - height).abs() > 0.0001
    {
        check.phase = Phase::Done;
        error!(
            "Chinook render mismatch: visible={visibility:?} hidden={hidden} portraits={count} actualY={} nativeY={height}",
            transform.translation.y
        );
        completion.exit.write(AppExit::error());
        return;
    }
    completion.capture.intermediate("chinook-landing.png");
    check.phase = Phase::WaitForCapture;
    log_phase(&check, &battle.simulation);
    println!(
        "Rendered Chinook: original rotor model at authoritative landing height, {} mixed passenger portraits and hidden riders passed",
        check.riders.len()
    );
}

fn advance(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    capture: Res<Capture>,
    clock: Res<Time<Real>>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let clock = clock.into_inner();
    let capture = capture.into_inner();
    if matches!(check.phase, Phase::Done) {
        return;
    }
    let now = clock.elapsed();
    if now.saturating_sub(*check.started.get_or_insert(now)) > Duration::from_mins(2) {
        let game = &battle.simulation;
        error!(
            "Original Chinook landing or rappel capture timed out phase={:?} frame={} selected={:?} virtual_paused={} capture_ready={}",
            check.phase,
            game.tick,
            battle.selected,
            time.is_paused(),
            capture.ready_for_scenario()
        );
        log_phase(&check, game);
        log_actor(game, check.host);
        for &rider in &check.riders {
            log_actor(game, rider);
        }
        check.phase = Phase::Done;
        exit.write(AppExit::error());
        return;
    }
    let host = check.host;
    let result = match check.phase {
        Phase::WaitForCapture if capture.ready_for_scenario() => {
            time.unpause();
            Ok(Phase::Unload)
        }
        Phase::Unload if battle.simulation.passengers(host).is_empty() => check
            .riders
            .iter()
            .try_for_each(|&id| battle.simulation.enter(id, host))
            .map(|()| Phase::Board),
        Phase::Board
            if battle.simulation.passengers(host).len() == check.riders.len()
                && battle.simulation.height_above_ground(host) > 97.0 =>
        {
            let target = battle.simulation.entities[host].position;
            if !battle.simulation.combat_drop_ready(host) {
                return;
            }
            dispatch_command(&mut battle, &mut mouse, CommandAction::CombatDrop);
            input::targeted_order(&mut battle, &mut mouse, Vec2::from_array(target), None);
            if matches!(mouse.pending_command, input::CommandMode::Normal) {
                Ok(Phase::Rappel)
            } else {
                Err("Chinook rejected the original ground Combat Drop order")
            }
        }
        _ => return,
    };
    match result {
        Ok(phase) => {
            check.phase = phase;
            log_phase(&check, &battle.simulation);
        }
        Err(error) => {
            error!("Chinook Combat Drop preparation: {error}");
            check.phase = Phase::Done;
            exit.write(AppExit::error());
        }
    }
}

fn log_phase(check: &Check, game: &generals_sim::skirmish::Skirmish) {
    let host = check.host;
    let actor = &game.entities[host];
    eprintln!(
        "CHINOOK_CAPTURE phase={:?} frame={} host={host} xyz={:?} hag={:.3} goal={:?} held={} passengers={:?} ropes={} {}",
        check.phase,
        game.tick,
        game.object_position(host),
        game.height_above_ground(host),
        actor.ai_goal_position(),
        actor.disabled_held(),
        game.passengers(host),
        game.rope_drawables.drawables.len(),
        actor.chinook_diagnostic()
    );
}

fn log_actor(game: &generals_sim::skirmish::Skirmish, id: usize) {
    let actor = &game.entities[id];
    eprintln!(
        "CHINOOK_CAPTURE actor={id} contained_by={:?} xyz={:?} hag={:.3} goal={:?} held={} {}",
        actor.contained_by(),
        game.object_position(id),
        game.height_above_ground(id),
        actor.ai_goal_position(),
        actor.disabled_held(),
        generals_sim::skirmish::diagnostics::movement_details(game, id)
    );
}

fn focus(
    check: Res<Check>,
    battle: Res<Match>,
    mut camera: ResMut<CameraRig>,
    terrain: Res<Terrain>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    if !matches!(check.phase, Phase::Rappel | Phase::Done) {
        return;
    }
    let mut position = battle.simulation.object_position(check.host);
    position[2] = battle
        .simulation
        .height_above_ground(check.host)
        .mul_add(-0.5, position[2]);
    // Minimum ground zoom places the camera at the Chinook's cruise altitude.
    // Use the original far zoom to include its rotors and the descending rider.
    camera.distance = camera.settings.max_height;
    crate::capture_framing::focus_on(&mut camera, terrain.into_inner(), position);
}

#[derive(SystemParam)]
struct RappelView<'w, 's> {
    framing: AircraftFrame<'w, 's>,
    ropes: Query<'w, 's, (&'static RopeSegment, &'static ViewVisibility)>,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect_rappel(mut check: ResMut<Check>, battle: Res<Match>, mut view: RappelView) {
    if !matches!(check.phase, Phase::Rappel) || !view.capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    let game = &battle.simulation;
    let Some(rider) = check.riders.iter().copied().find(|&id| {
        let actor = &game.entities[id];
        let unit = &game.catalog[actor.type_index];
        actor.alive()
            && actor.contained_by().is_none()
            && game.height_above_ground(id) > 5.0
            && game
                .model_flags(actor, unit.body_damage_thresholds)
                .contains("RAPPELLING")
    }) else {
        return;
    };
    let rope =
        view.ropes.iter().any(|(segment, visible)| {
            visible.get()
                && game.rope_drawables.drawables.iter().any(|rope| {
                    rope.id == segment.0 && rope.length > 0.0 && rope.released.is_none()
                })
        });
    let result = view.framing.visible(battle, check.host).and_then(|host| {
        view.framing
            .visible(battle, rider)
            .map(|rider| host && rider && rope)
    });
    match result {
        Ok(true) => {
            view.time.pause();
            view.capture.finish_scenario();
            check.phase = Phase::Done;
            log_phase(&check, game);
            println!(
                "Rendered Chinook Combat Drop: original aircraft, live rope segments and airborne Ranger"
            );
        }
        Ok(false) => {}
        Err(error) => {
            error!("Chinook Combat Drop capture: {error}");
            check.phase = Phase::Done;
            view.exit.write(AppExit::error());
        }
    }
}
