//! Installed original Center inventory, masked Hackers and scattered income text.
use super::Floating;
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, Mouse, PickTarget},
    playtest::Match,
    playtest_ui::{Action, PromotionIcon, dispatch_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{advance_until, require},
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    center: usize,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let center = crate::income_verification::internet::prepare(&mut battle.simulation)?;
    battle.select_objects(&[center], true);
    let data = app.world().resource::<UiData>();
    let center_type = battle.simulation.entities[center].type_index;
    let commands = &data.units[center_type].commands;
    for slot in 1..=8 {
        require(commands.iter().any(|command| command.slot == slot
            && matches!(command.action, CommandAction::ExitContainer(index) if index == slot - 1)),
            "Internet Center lacks original eight inventory slots")?;
    }
    require(
        commands
            .iter()
            .any(|command| command.slot == 9 && matches!(command.action, CommandAction::Evacuate))
            && commands
                .iter()
                .any(|command| command.slot == 14 && matches!(command.action, CommandAction::Sell))
            && commands
                .iter()
                .all(|command| command.slot != 10 && command.slot != 12),
        "Internet Center command set exposed unsupported research or lost native Evacuate/Sell",
    )?;
    let worker = battle
        .simulation
        .type_for(0, generals_sim::skirmish::Kind::Builder)
        .ok_or("China builder missing")?;
    require(
        data.units[worker].commands.iter().any(|command| {
            matches!(command.action,
        CommandAction::Produce(index) if index == center_type)
        }),
        "Dozer lacks original Center build button",
    )?;
    let id = battle.simulation.passengers(center)[0];
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("pointer state missing")?;
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(0));
    advance_until(&mut battle.simulation, 1800, "Center UI exit", |game| {
        game.entities[id].contained_by().is_none()
            && matches!(
                game.entities[id].ai.order,
                generals_sim::skirmish::Order::Idle
            )
    })?;
    battle.select_objects(&[id], true);
    require(
        input::context_order(battle, None, Some(PickTarget::Unit(center)), &mut mouse),
        "Hacker context UI did not issue native entry",
    )?;
    advance_until(&mut battle.simulation, 9000, "Center UI reentry", |game| {
        game.entities[id].contained_by() == Some(center)
    })?;
    battle.select_objects(&[center], true);
    mouse.voices.clear();
    app.insert_resource(mouse).insert_resource(Check {
        center,
        done: false,
    });
    println!(
        "Installed Internet Center UI: original Dozer build command, eight inventory slots, Evacuate9/Sell14, individual exit/context reentry and absent unsupported Satellite/Mines buttons passed"
    );
    Ok(())
}

pub fn register(app: &mut App) {
    app.world_mut()
        .resource_mut::<Capture>()
        .wait_for_scenario();
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w> {
    capture: ResMut<'w, Capture>,
    clock: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    exit: MessageWriter<'w, AppExit>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Inventory<'w, 's> {
    data: Res<'w, UiData>,
    icons: Res<'w, Assets<PromotionIcon>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static Node,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
}

fn inspect(
    battle: Res<Match>,
    inventory: Inventory,
    visuals: Query<(&UnitVisual, &Visibility)>,
    texts: Query<(&Floating, &ComputedNode, &Node, &Text)>,
    mut check: ResMut<Check>,
    mut completion: Completion,
) {
    if check.done {
        return;
    }
    if completion.real.elapsed_secs() > 60.0 {
        check.done = true;
        error!("Internet Center capture timed out before native inventory and income were visible");
        completion.exit.write(AppExit::error());
        return;
    }
    if !completion.capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    let data = inventory.data.into_inner();
    let icons = inventory.icons.into_inner();
    let game = &battle.simulation;
    let passengers = game.passengers(check.center);
    let present = |id| {
        visuals
            .iter()
            .find(|(visual, _)| visual.index == id && visual.generation == battle.generation)
            .map(|(_, visibility)| *visibility)
    };
    if present(check.center) != Some(Visibility::Visible)
        || passengers
            .iter()
            .any(|&id| present(id) != Some(Visibility::Hidden))
    {
        return;
    }
    let filled = inventory
        .buttons
        .iter()
        .filter(|(action, node, material)| {
            let Action::Slot(slot @ 1..=8) = **action else {
                return false;
            };
            let Some(&id) = passengers.get(slot - 1) else {
                return false;
            };
            let expected = data.icon(&data.units[game.entities[id].type_index].button_image);
            node.display != Display::None
                && icons.get(&material.0).is_some_and(|icon| {
                    icon.image == expected.image
                        && icon.uv_rect == expected.uv_rect
                        && icon.state.x < 0.5
                })
        })
        .count();
    let center = game.entities[check.center].position;
    let cash = texts.iter().any(|(text, computed, node, label)| {
        text.generation == battle.generation
            && text.alpha > 100
            && computed.size().x > 0.0
            && computed.size().y > 0.0
            && node.display == Display::Flex
            && !label.is_empty()
            && !label.contains("%d")
            && (text.map_position[0] - center[0]).abs() <= 15.0
            && (text.map_position[1] - center[1]).abs() <= 15.0
    });
    if filled != 8 || !cash {
        return;
    }
    check.done = true;
    completion.clock.pause();
    completion.capture.finish_scenario();
    println!(
        "Rendered Internet Center: original building, eight Hacker inventory portraits, hidden enclosed units and visible source-scattered cash text passed"
    );
}
