//! Installed original Center inventory, masked Hackers and scattered income text.
use super::Floating;
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, Mouse, PickTarget},
    playtest::Match,
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{advance_until, require},
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    center: usize,
    phase: u8,
    issued: bool,
    payment: Option<(u64, u32, u32)>,
    elapsed_from: Option<f64>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    battle.simulation.set_match_options(50_000, 1);
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
            && [
                (10, "Command_UpgradeChinaSatelliteHackOne"),
                (12, "Command_UpgradeChinaMines"),
            ]
            .into_iter()
            .all(|(slot, name)| {
                commands
                    .iter()
                    .any(|command| command.slot == slot && command.name == name)
            }),
        "Internet Center lost original Satellite/Mines or Evacuate/Sell buttons",
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
    input::commands::step(battle, &mut mouse);
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
    input::commands::step(battle, &mut mouse);
    advance_until(&mut battle.simulation, 9000, "Center UI reentry", |game| {
        game.entities[id].contained_by() == Some(center)
    })?;
    battle.select_objects(&[center], true);
    mouse.voices.clear();
    app.insert_resource(mouse).insert_resource(Check {
        center,
        phase: 0,
        issued: false,
        payment: None,
        elapsed_from: None,
    });
    println!(
        "Installed Internet Center UI: original Dozer build command, eight inventory slots, Evacuate9/Sell14, individual exit/context reentry and native Satellite/Mines buttons passed"
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
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
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

type IncomeText = (
    &'static Floating,
    &'static ComputedNode,
    &'static Node,
    &'static Text,
);

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    inventory: Inventory<'w, 's>,
    frame: crate::capture_framing::AircraftFrame<'w, 's>,
    particles: crate::particles::ParticleInspection<'w, 's>,
    animations: Res<'w, crate::model_animation::Animations>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Visibility)>,
    texts: Query<'w, 's, IncomeText>,
}

fn inspect(
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    view: View,
    mut check: ResMut<Check>,
    mut completion: Completion,
) {
    if check.phase == u8::MAX || !completion.capture.ready_for_scenario() {
        return;
    }
    let now = completion.real.elapsed_secs_f64();
    let started = *check.elapsed_from.get_or_insert(now);
    let result = require(
        now - started < 110.0,
        "native Internet Center research capture timed out",
    )
    .and_then(|()| {
        observe(
            &mut battle,
            &mut mouse,
            &mut check,
            view,
            &mut completion.clock,
        )
    });
    match result {
        Ok(true) => {
            completion.clock.pause();
            check.elapsed_from = None;
            check.issued = false;
            if check.phase == 0 {
                check.phase = 1;
                completion
                    .capture
                    .intermediate("p03-internet-satellite-one-mines.png");
            } else {
                check.phase = u8::MAX;
                completion.capture.finish_scenario();
            }
        }
        Ok(false) => {}
        Err(error) => {
            check.phase = u8::MAX;
            error!("Internet Center capture: {error}");
            completion.exit.write(AppExit::error());
        }
    }
}

fn observe(
    battle: &mut Match,
    mouse: &mut Mouse,
    check: &mut Check,
    view: View,
    clock: &mut Time<Virtual>,
) -> Result<bool, Box<dyn Error>> {
    let View {
        inventory,
        frame,
        particles,
        animations,
        visuals,
        texts,
    } = view;
    let slots = if check.phase == 0 {
        ["Upgrade_ChinaSatelliteHackOne", "Upgrade_ChinaMines"]
    } else {
        ["Upgrade_ChinaSatelliteHackTwo", "Upgrade_ChinaEMPMines"]
    };
    if !check.issued {
        let game = &battle.simulation;
        let player = &game.players[game.entities[check.center].owner];
        let balance = player.cash.wrapping_sub(player.statistics.money_earned);
        let tick = game.tick;
        let mut cost = 0;
        for (slot, name) in [10, 12].into_iter().zip(slots) {
            cost += issue_research(battle, mouse, &inventory.data, check.center, slot, name)?;
        }
        check.payment = Some((tick, balance, cost));
        check.issued = true;
        clock.unpause();
        return Ok(false);
    }
    let game = &battle.simulation;
    let center = &game.entities[check.center];
    if let Some((issued, balance, cost)) = check.payment {
        if game.tick == issued {
            return Ok(false);
        }
        let player = &game.players[center.owner];
        require(
            player.cash.wrapping_sub(player.statistics.money_earned) == balance.wrapping_sub(cost),
            "original Internet Center UI did not pay both authored costs",
        )?;
        check.payment = None;
    }
    if !game.has_upgrade(center.owner, slots[0])
        || !game.object_has_upgrade(check.center, slots[1])
        || center.command_set_override() != Some("ChinaInternetCenterCommandSetTwoUpgrade")
    {
        return Ok(false);
    }
    if !frame.visible_mesh(battle, check.center, &animations, "NBINTCNT_AC.RADARDISH")?
        || check.phase == 1
            && !frame.visible_mesh(battle, check.center, &animations, "NBINTCNT_AG.RADARDISH")?
    {
        return Ok(false);
    }
    let (mine, effect) = if check.phase == 0 {
        ("ChinaStandardMine", "MineFX")
    } else {
        ("ChinaEMPMine", "MineFXPurple")
    };
    for (id, actor) in game.entities.iter().enumerate() {
        if actor.alive()
            && actor.producer() == Some(check.center)
            && game.catalog[actor.type_index].name == mine
            && particles.drawable_state(id, effect, battle.generation)?
        {
            return Ok(inventory_and_cash(
                battle,
                &inventory,
                &visuals,
                &texts,
                check.center,
            ));
        }
    }
    Ok(false)
}

fn issue_research(
    battle: &mut Match,
    mouse: &mut Mouse,
    data: &UiData,
    center: usize,
    slot: usize,
    name: &str,
) -> Result<u32, Box<dyn Error>> {
    let catalog = &battle.simulation.catalog[battle.simulation.entities[center].type_index];
    let action = selected_command(battle, data, slot)
        .map(|command| command.action)
        .ok_or("original Internet Center research button missing")?;
    let CommandAction::Research(index) = action else {
        return Err("Internet Center button does not issue research".into());
    };
    let research = &catalog.maintenance.research[index];
    require(
        research.name == name,
        "Internet Center exposes the wrong research",
    )?;
    let cost = research.cost;
    dispatch_command(battle, mouse, action);
    Ok(cost)
}

fn inventory_and_cash(
    battle: &Match,
    inventory: &Inventory,
    visuals: &Query<(&UnitVisual, &Visibility)>,
    texts: &Query<IncomeText>,
    center: usize,
) -> bool {
    let data = &inventory.data;
    let icons = &inventory.icons;
    let game = &battle.simulation;
    let passengers = game.passengers(center);
    let present = |id| {
        visuals
            .iter()
            .find(|(visual, _)| visual.index == id && visual.generation == battle.generation)
            .map(|(_, visibility)| *visibility)
    };
    if present(center) != Some(Visibility::Visible)
        || passengers
            .iter()
            .any(|&id| present(id) != Some(Visibility::Hidden))
    {
        return false;
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
    let center = game.entities[center].position;
    filled == 8
        && texts.iter().any(|(text, computed, node, label)| {
            text.generation == battle.generation
                && text.alpha > 100
                && computed.size().x > 0.0
                && computed.size().y > 0.0
                && node.display == Display::Flex
                && !label.is_empty()
                && !label.contains("%d")
                && (text.map_position[0] - center[0]).abs() <= 15.0
                && (text.map_position[1] - center[1]).abs() <= 15.0
        })
}
