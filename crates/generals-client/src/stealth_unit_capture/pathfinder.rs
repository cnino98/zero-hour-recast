use super::{Check, Control, Error, View, attack_order, finish, move_order, require};
use crate::control_bar::CommandAction;
use bevy::prelude::*;
use generals_sim::skirmish::Order;

pub(super) fn progress(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    match check.phase {
        0 => begin(check, view, control)?,
        1 => {
            let game = &control.battle.simulation;
            let actor = &game.entities[check.actor];
            control.camera.focus = Vec2::from_array(actor.position) * 0.1;
            if actor.moving()
                && !actor.stealthed()
                && view.visible(&control.battle, check.actor, false)
            {
                view.animation(
                    &control.battle,
                    check.actor,
                    "aipfdr_skl.aipfdr_rna",
                    &mut check.animation,
                )?;
            }
            if check.animation.advanced
                && check.speech_finished
                && matches!(actor.ai.order, Order::Idle)
                && !actor.moving()
                && actor.stealthed()
                && view.visible(&control.battle, check.actor, true)
            {
                require(
                    !game.logically_visible(1, check.actor),
                    "stationary Pathfinder remained visible to enemy",
                )?;
                control.clock.pause();
                control.capture.intermediate("pathfinder-cloak.png");
                check.phase = 2;
            }
        }
        2 => {
            require(
                control.battle.simulation.entities[check.target].alive(),
                "Pathfinder paid target died before native Attack command",
            )?;
            attack_order(control, check.target)?;
            check.phase = 3;
        }
        3 => complete(check, view, control)?,
        // Require the next camera/visibility update after the prior focus change.
        4 if view.visible(&control.battle, check.actor, true) => {
            finish(check, control);
            println!(
                "Original Pathfinder native: combat-earned science/paid production, original commands and running animation, moving reveal/stationary cloak, one credited sniper kill and three advancing source sounds passed"
            );
        }
        _ => {}
    }
    Ok(())
}

fn begin(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    require(
        matches!(
            view.command(&control.battle, 11)?,
            CommandAction::AttackMove
        ) && matches!(view.command(&control.battle, 13)?, CommandAction::Guard)
            && matches!(view.command(&control.battle, 14)?, CommandAction::Stop),
        "Pathfinder original AttackMove/Guard/Stop commands differ",
    )?;
    let game = &control.battle.simulation;
    let actor = &game.entities[check.actor];
    let victim = &game.entities[check.target];
    let delta = Vec2::from_array(actor.position) - Vec2::from_array(victim.position);
    require(
        delta.length() > 330.0,
        "Pathfinder capture needs movement outside native firing range",
    )?;
    let point = Vec2::from_array(victim.position) + delta.normalize() * 330.0;
    move_order(control, point.to_array())?;
    check.phase = 1;
    Ok(())
}

fn complete(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let game = &control.battle.simulation;
    let actor = &game.entities[check.actor];
    // Retain the actor framing already verified by the cloak capture.
    control.camera.focus = Vec2::from_array(actor.position) * 0.1;
    if game.entities[check.target].alive() {
        return Ok(());
    }
    require(
        check.shots == 1
            && actor.alive()
            && actor.stealthed()
            && actor.experience > check.experience
            && game.players[0].statistics.units_destroyed == check.credit[0] + 1
            && game.players[1].statistics.units_lost == check.credit[1] + 1,
        "Pathfinder native sniper shot, cloak or credited kill differs",
    )?;
    if check.sounds.iter().all(|heard| *heard) && view.visible(&control.battle, check.actor, true) {
        control.clock.pause();
        check.phase = 4;
    }
    Ok(())
}
