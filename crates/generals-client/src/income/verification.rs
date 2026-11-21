use super::Floating;
use crate::{
    control_bar::{CommandAction, UiData},
    input::Mouse,
    playtest::Match,
    playtest_view::Capture,
    skirmish_verification::{advance_until, require},
};
use bevy::prelude::*;
use generals_sim::skirmish::Order;
use std::error::Error;

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let id = crate::income_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[id], true);
    battle.simulation.stop(id)?;
    advance_until(
        &mut battle.simulation,
        500,
        "Hacker UI setup pack",
        |game| matches!(game.entities[id].ai.order, Order::Idle),
    )?;
    let data = app.world().resource::<UiData>();
    let unit = &data.units[battle.simulation.entities[id].type_index];
    let action = unit
        .commands
        .iter()
        .find(|command| command.name == "Command_ChinaInfantryHackerInternetHack")
        .ok_or("original Hacker command button missing")?
        .action;
    require(
        matches!(action, CommandAction::HackInternet),
        "Hacker button action differs",
    )?;
    let mut mouse = app.world_mut().resource_mut::<Mouse>();
    crate::playtest_ui::dispatch_command(battle, &mut mouse, action);
    crate::input::commands::step(battle, &mut mouse);
    require(
        battle.simulation.entities[id].hacking_condition() == Some("UNPACKING"),
        "original UI Hack command failed to start unpacking",
    )
}

pub fn register(app: &mut App) {
    app.world_mut()
        .resource_mut::<Capture>()
        .wait_for_scenario();
    app.add_systems(PostUpdate, verify.after(bevy::ui::UiSystems::Layout));
}

fn verify(
    battle: Res<Match>,
    mut capture: ResMut<Capture>,
    texts: Query<(&Floating, &ComputedNode, &Node, &Text)>,
    mut clock: ResMut<Time<Virtual>>,
    time: Res<Time<Real>>,
    mut started: Local<Option<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    let started = started.get_or_insert(time.elapsed_secs());
    if battle.paused {
        return;
    }
    if time.elapsed_secs() - *started >= 60.0 {
        eprintln!("Hacker render diagnostic timed out before original income text became visible");
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    let id = battle.selected[0];
    if battle.simulation.entities[id].hacking_condition() != Some("FIRING_A")
        || battle.simulation.entities[id].experience == 0
    {
        return;
    }
    if texts.iter().any(|(text, computed, node, label)| {
        text.generation == battle.generation
            && text.alpha > 100
            && computed.size().x > 0.0
            && computed.size().y > 0.0
            && node.display == Display::Flex
            && !label.is_empty()
            && !label.contains("%d")
    }) {
        println!(
            "Hacker render: original active state, earned cash/XP and visible localized floating text verified"
        );
        clock.pause();
        capture.finish_scenario();
    }
}
