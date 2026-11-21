//! Original inventory commands checked against installed vehicles and rendered UI.
use super::{available, passenger, visible};
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
#[cfg(test)]
mod tests;

#[derive(Resource)]
struct Check {
    carrier: usize,
    riders: Vec<usize>,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = crate::transport_verification::prepare(&mut battle.simulation)?;
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("missing pointer state")?;
    let data = app.world().resource::<UiData>();
    battle.select_objects(&[setup.medic], true);
    for index in 0..3 {
        require(
            visible(battle, index) && passenger(battle, index).is_some(),
            "Ambulance inventory slot absent",
        )?;
    }
    require(
        !visible(battle, 3),
        "Ambulance exposed an overflow inventory slot",
    )?;
    let rider = passenger(battle, 0).ok_or("missing first Ambulance passenger")?;
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(0));
    input::commands::step(battle, &mut mouse);
    advance_until(&mut battle.simulation, 100, "UI individual exit", |game| {
        game.entities[rider].contained_by().is_none()
    })?;
    battle.select_objects(&[rider], true);
    verify_local_ownership(battle, &mut mouse, rider, setup.medic)?;
    require(
        input::context_order(
            battle,
            None,
            Some(PickTarget::Unit(setup.medic)),
            &mut mouse,
        ),
        "original context command failed to board transport",
    )?;
    input::commands::step(battle, &mut mouse);
    advance_until(&mut battle.simulation, 3000, "UI reboarding", |game| {
        game.entities[rider].contained_by() == Some(setup.medic)
    })?;
    battle.select_objects(&[setup.medic], true);
    dispatch_command(battle, &mut mouse, CommandAction::Evacuate);
    input::commands::step(battle, &mut mouse);
    advance_until(&mut battle.simulation, 1000, "UI evacuation", |game| {
        game.passengers(setup.medic).is_empty()
    })?;
    require(
        available(battle, CommandAction::Evacuate) == Some(false),
        "empty transport evacuation remained enabled",
    )?;
    for index in 0..3 {
        require(
            visible(battle, index)
                && available(battle, CommandAction::ExitContainer(index)) == Some(false),
            "empty original inventory slot was hidden or enabled",
        )?;
        dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(index));
    }
    input::commands::step(battle, &mut mouse);
    require(
        battle.simulation.passengers(setup.medic).is_empty(),
        "empty inventory action produced a passenger",
    )?;
    battle.select_objects(&[setup.humvee], true);
    let order = battle.simulation.passengers(setup.humvee);
    require(
        order == &setup.riders[..5],
        &format!(
            "inventory lost native append order: actual={order:?}, boarded={:?}",
            &setup.riders[..5]
        ),
    )?;
    for slot in 4..=8 {
        let command =
            selected_command(battle, data, slot).ok_or("original Humvee exit button missing")?;
        require(
            matches!(command.action, CommandAction::ExitContainer(index) if index == slot-4),
            "passenger button uses wrong native command slot",
        )?;
    }
    // No synthetic sound events remain from the accelerated command check.
    mouse.voices.clear();
    let riders = battle.simulation.passengers(setup.humvee).to_vec();
    app.insert_resource(mouse).insert_resource(Check {
        carrier: setup.humvee,
        riders,
        done: false,
    });
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
    println!(
        "Installed transport UI: native slots/order, overflow hiding, empty-slot safety, enemy selection no-op, local context boarding, individual exit and evacuation passed"
    );
    Ok(())
}

fn verify_local_ownership(
    battle: &mut Match,
    mouse: &mut Mouse,
    rider: usize,
    carrier: usize,
) -> Result<(), Box<dyn Error>> {
    // These valid installed templates also form a legal opposing USA pair.
    // Isolate input ownership from transport admission without advancing time.
    let hovered = mouse.hovered;
    mouse.hovered = Some(carrier);
    battle.simulation.entities[rider].owner = 1;
    battle.simulation.entities[carrier].owner = 1;
    require(
        battle.simulation.can_enter(rider, carrier),
        "enemy ownership fixture did not permit native passenger admission",
    )?;
    let order = std::mem::discriminant(&battle.simulation.entities[rider].ai.order);
    let voices = mouse.voices.len();
    let cursor = crate::attack_cursor::context(battle, mouse);
    let issued = input::context_order(battle, None, Some(PickTarget::Unit(carrier)), mouse);
    let unchanged = order == std::mem::discriminant(&battle.simulation.entities[rider].ai.order);
    battle.simulation.entities[rider].owner = 0;
    battle.simulation.entities[carrier].owner = 0;
    require(
        cursor.is_none() && !issued && unchanged && mouse.voices.len() == voices,
        "enemy selection exposed a boarding cursor, command or voice",
    )?;
    require(
        crate::attack_cursor::context(battle, mouse) == Some(1),
        "local passenger did not expose the original entry cursor",
    )?;
    mouse.hovered = hovered;
    Ok(())
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w> {
    capture: ResMut<'w, Capture>,
    virtual_time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
    real_time: Res<'w, Time<Real>>,
}

fn inspect(
    battle: Res<Match>,
    data: Res<UiData>,
    icons: Res<Assets<PromotionIcon>>,
    visuals: Query<(&UnitVisual, &Visibility)>,
    buttons: Query<(&Action, &Node, &MaterialNode<PromotionIcon>)>,
    mut check: ResMut<Check>,
    mut completion: Completion,
) {
    if check.done {
        return;
    }
    if completion.real_time.elapsed_secs() > 60.0 {
        check.done = true;
        error!("Rendered transport check did not receive the expected scene");
        completion.exit.write(AppExit::error());
        return;
    }
    let battle = battle.into_inner();
    let data = data.into_inner();
    let icons = icons.into_inner();
    let present = |id| {
        visuals
            .iter()
            .find(|(visual, _)| visual.index == id && visual.generation == battle.generation)
            .map(|(_, visibility)| *visibility)
    };
    if present(check.carrier).is_none() || check.riders.iter().any(|&id| present(id).is_none()) {
        return;
    }
    let hidden = check
        .riders
        .iter()
        .all(|&id| present(id) == Some(Visibility::Hidden));
    let filled = buttons
        .iter()
        .filter(|(action, node, material)| {
            let Action::Slot(slot @ 4..=8) = **action else {
                return false;
            };
            let Some(id) = passenger(battle, slot - 4) else {
                return false;
            };
            let expected =
                data.icon(&data.units[battle.simulation.entities[id].type_index].button_image);
            node.display != Display::None
                && icons.get(&material.0).is_some_and(|icon| {
                    icon.image == expected.image
                        && icon.uv_rect == expected.uv_rect
                        && icon.state.x < 0.5
                })
        })
        .count();
    check.done = true;
    if !hidden || filled != 5 || present(check.carrier) != Some(Visibility::Visible) {
        error!(
            "Rendered transport inventory or enclosed visibility failed: hidden={hidden}, filled={filled}"
        );
        completion.exit.write(AppExit::error());
        return;
    }
    completion.virtual_time.pause();
    completion.capture.finish_scenario();
    println!(
        "Rendered transport UI: five original passenger portraits and visible carrier; enclosed passenger drawables hidden"
    );
}

pub fn structure_inventory(
    app: &mut App,
    battle: &mut Match,
    host: usize,
    rider: usize,
) -> Result<(), Box<dyn Error>> {
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("missing pointer state")?;
    let data = app.world().resource::<UiData>();
    battle.select_objects(&[host], true);
    require(
        data.units[battle.simulation.entities[host].type_index].structure_inventory,
        "civilian building lacks source structure inventory context",
    )?;
    for slot in 1..=10 {
        require(selected_command(battle,data,slot).is_some_and(|command|matches!(command.action,CommandAction::ExitContainer(index) if index==slot-1)),"original structure exit slot mapping changed")?;
    }
    require(
        selected_command(battle, data, 11)
            .is_some_and(|command| matches!(command.action, CommandAction::Stop))
            && selected_command(battle, data, 12)
                .is_some_and(|command| matches!(command.action, CommandAction::Evacuate)),
        "structure Stop/Evacuate source slots changed",
    )?;
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(0));
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        120,
        "structure inventory exit",
        |game| game.entities[rider].contained_by().is_none(),
    )?;
    require(
        battle.simulation.entities[host].owner == battle.simulation.neutral_owner(),
        "UI individual exit retained civilian control",
    )?;
    for slot in [1, 11, 12] {
        let command =
            selected_command(battle, data, slot).ok_or("empty neutral inventory disappeared")?;
        require(
            !crate::playtest_ui::command_icons::available(battle, command),
            "empty neutral inventory permits command",
        )?;
    }
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(9));
    input::commands::step(battle, &mut mouse);
    require(
        battle.simulation.entities[rider].contained_by().is_none(),
        "empty civilian inventory click changed rider",
    )?;
    battle.select_objects(&[rider], true);
    require(
        input::context_order(battle, None, Some(PickTarget::Unit(host)), &mut mouse),
        "normal context order did not enter civilian building",
    )?;
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        3000,
        "context civilian reentry",
        |game| game.entities[rider].contained_by() == Some(host),
    )?;
    battle.select_objects(&[host], true);
    app.insert_resource(mouse);
    println!(
        "Original neutral structure UI: ten exit slots, Stop11/Evacuate12, empty neutral inventory disabled, actual individual exit and context reentry passed"
    );
    Ok(())
}
