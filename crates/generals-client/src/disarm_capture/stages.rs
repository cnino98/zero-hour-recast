use super::{Check, Control, Error, Stage, View, disarm_verification, require};
use crate::{
    input::{self, PickTarget},
    source_audio::Voice,
};
use bevy::prelude::Vec2;
use generals_sim::skirmish::Order;

pub(super) fn progress(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    match check.stage {
        Stage::Hover => hover(check, view, control)?,
        Stage::Prime => {
            if check.animation.is_some() && view.body(&control.battle, check.setup.builder) {
                check.stage = Stage::Dig;
                control.time.unpause();
            }
        }
        Stage::Dig => digging(check, view, control)?,
        Stage::DigCaptured => {
            check.stage = Stage::Clearing;
            control.time.unpause();
        }
        Stage::Clearing => cleared(check, view, control)?,
        Stage::ClearedCaptured => {
            check.stage = Stage::Reload;
            control.time.unpause();
        }
        Stage::Reload
            if check.trace.reloaded
                && control
                    .battle
                    .simulation
                    .mine_clearing_ready(check.setup.builder, 0) =>
        {
            disarm_verification::verify_removed(&control.battle.simulation, &check.setup)?;
            if view.body(&control.battle, check.setup.builder)
                && view.scorch.disarm(check.setup.position)?
                && !view.body(&control.battle, check.setup.trap)
            {
                check.stage = Stage::AreaMode;
            }
        }
        Stage::AreaMode
        | Stage::AreaPress
        | Stage::AreaRelease
        | Stage::AreaOrder
        | Stage::AreaRun => super::area::progress(check, view, control)?,
        _ => {}
    }
    Ok(())
}

fn hover(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let game = &control.battle.simulation;
    require(
        game.entities[check.setup.scout].alive() && game.entities[check.setup.trap].detected(),
        "paid detector/visible trap missing",
    )?;
    if !view.body(&control.battle, check.setup.builder)
        || !view.body(&control.battle, check.setup.trap)
    {
        return Ok(());
    }
    view.controls(&control.battle, check.setup.builder)?;
    if check.pointer.is_none() {
        check.pointer = view.target_pointer(&control.battle, check.setup.trap);
        control.mouse.hovered = Some(check.setup.trap);
        return Ok(());
    }
    if control.mouse.hovered != Some(check.setup.trap)
        || crate::attack_cursor::context(&mut control.battle, &control.mouse) != Some(0)
        || !view.cursor_visible()
    {
        return Ok(());
    }
    let point = Vec2::new(check.setup.position[0], check.setup.position[1]);
    require(
        input::context_order(
            &mut control.battle,
            Some(point),
            Some(PickTarget::Unit(check.setup.trap)),
            &mut control.mouse,
        ),
        "original Dozer context disarm rejected",
    )?;
    require(
        matches!(control.battle.simulation.entities[check.setup.builder].ai.order, Order::Attack(target) if target == check.setup.trap)
            && control
                .mouse
                .voices
                .last()
                .is_some_and(|request| matches!(request.voice(), Voice::Disarm)),
        "context attack omitted original disarm order/voice",
    )?;
    check.stage = Stage::Approach;
    control.time.unpause();
    Ok(())
}

fn digging(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    if !control.time.is_paused() {
        return Ok(());
    }
    let game = &control.battle.simulation;
    require(
        check
            .trace
            .preattack
            .is_some_and(|start| game.tick == start + 32)
            && check.trace.shot.is_none()
            && game.entities[check.setup.trap].alive(),
        "dig capture missed the original preattack window",
    )?;
    let generation = control.battle.generation;
    if check.animated
        && view.body(&control.battle, check.setup.builder)
        && view
            .particles
            .drawable_state(check.setup.builder, "DozerDirtFall", generation)?
        && view
            .particles
            .drawable_state(check.setup.builder, "DozerSmokeHeavy", generation)?
    {
        control.capture.intermediate("disarm-dig.png");
        check.stage = Stage::DigCaptured;
    }
    Ok(())
}

fn cleared(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    if !control.time.is_paused() {
        return Ok(());
    }
    let Some(shot) = &check.trace.shot else {
        return Ok(());
    };
    let game = &control.battle.simulation;
    require(
        game.tick == shot.frame + 2,
        "clearing capture missed original flash color frame",
    )?;
    disarm_verification::verify_removed(game, &check.setup)?;
    if check.audio.iter().all(|heard| *heard)
        && view.body(&control.battle, check.setup.builder)
        && view.particles.weapon_fire(
            game.entities[check.setup.builder].type_index,
            shot.weapon,
            shot.veterancy,
            shot.frame,
            control.battle.generation,
        )?
        && view.scorch.disarm(check.setup.position)?
    {
        control.capture.intermediate("disarm-cleared.png");
        check.stage = Stage::ClearedCaptured;
    }
    Ok(())
}
