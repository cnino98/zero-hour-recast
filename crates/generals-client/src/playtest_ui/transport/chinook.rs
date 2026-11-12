//! Original Chinook inventory actions and its authoritative landing transform.
use super::{passenger, visible};
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, Mouse, PickTarget},
    playtest::Match,
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{advance_until, require},
};
use bevy::{app::AppExit, prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    host: usize,
    riders: Vec<usize>,
    done: bool,
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
        done: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
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
    if check.done || !completion.capture.ready_for_scenario() {
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
    check.done = true;
    if *visibility != Visibility::Visible
        || !hidden
        || count != check.riders.len()
        || (transform.translation.y - height).abs() > 0.0001
    {
        error!(
            "Chinook render mismatch: visible={visibility:?} hidden={hidden} portraits={count} actualY={} nativeY={height}",
            transform.translation.y
        );
        completion.exit.write(AppExit::error());
        return;
    }
    completion.capture.finish_scenario();
    println!(
        "Rendered Chinook: original rotor model at authoritative landing height, {} mixed passenger portraits and hidden riders passed",
        check.riders.len()
    );
}
