use super::{Check, Control, Error, Stage, View, require};
use crate::{
    control_bar::CommandAction,
    input::{self, PickTarget},
    playtest_ui::dispatch_command,
    source_audio::Voice,
};
use bevy::prelude::*;
use generals_sim::skirmish::Order;

pub(super) fn research(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    let action = view.command(&control.battle, 9)?;
    let CommandAction::Research(index) = action else {
        return Err("Sentry factory slot 9 is not its original research".into());
    };
    let game = &control.battle.simulation;
    let upgrade = &game.catalog[game.entities[check.setup.factory].type_index]
        .maintenance
        .research[index];
    require(
        upgrade.name == "Upgrade_AmericaSentryDroneGun"
            && upgrade.cost == 1000
            && upgrade.frames == 900,
        "Sentry Gun button changed original research, price or duration",
    )?;
    let cash = game.players[0].cash;
    check.research = game.tick;
    dispatch_command(&mut control.battle, &mut control.mouse, action);
    require(
        control.battle.simulation.players[0].cash + 1000 == cash,
        "original Sentry Gun button did not pay its research cost",
    )?;
    check.stage = Stage::Research;
    control.time.unpause();
    Ok(())
}

pub(super) fn guard(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    require(
        matches!(view.command(&control.battle, 13)?, CommandAction::Guard),
        "Sentry slot 13 is not original Guard",
    )?;
    let point = Vec2::from_array(control.battle.simulation.entities[check.setup.drone].position);
    dispatch_command(
        &mut control.battle,
        &mut control.mouse,
        CommandAction::Guard,
    );
    input::targeted_order(&mut control.battle, &mut control.mouse, point, None);
    require(
        matches!(
            control.battle.simulation.entities[check.setup.drone]
                .ai
                .order,
            Order::Guard(_)
        ) && control
            .mouse
            .voices
            .last()
            .is_some_and(|voice| matches!(voice.voice(), Voice::Guard)),
        "original Guard click failed to issue its order and response",
    )?;
    check.stage = Stage::Deploy;
    control.time.unpause();
    Ok(())
}

pub(super) fn move_drone(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let point = control.battle.simulation.entities[check.setup.drone].position;
    check.next = [point[0] + 120.0, point[1]];
    require(
        input::context_order(
            &mut control.battle,
            Some(Vec2::from_array(check.next)),
            None,
            &mut control.mouse,
        ),
        "Sentry original context Move rejected",
    )?;
    require(
        control
            .mouse
            .voices
            .last()
            .is_some_and(|voice| matches!(voice.voice(), Voice::Move)),
        "Sentry context Move omitted original response",
    )?;
    check.stage = Stage::Move;
    control.time.unpause();
    Ok(())
}

pub(super) fn attack(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let game = &mut control.battle.simulation;
    game.stop(check.setup.target)?;
    check.target_health = game.entities[check.setup.target].health;
    let position = Vec2::from_array(game.entities[check.setup.target].position);
    let actor = Vec2::from_array(game.entities[check.setup.drone].position);
    control.camera.focus = (actor + position) * 0.05;
    require(
        input::context_order(
            &mut control.battle,
            Some(position),
            Some(PickTarget::Unit(check.setup.target)),
            &mut control.mouse,
        ),
        "Sentry original context Attack rejected",
    )?;
    require(
        control
            .mouse
            .voices
            .last()
            .is_some_and(|voice| matches!(voice.voice(), Voice::Attack)),
        "Sentry context Attack omitted original response",
    )?;
    check.stage = Stage::Fire;
    Ok(())
}
