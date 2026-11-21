use super::{
    Check, CommandAction, CommandMode, Control, Error, PickTarget, View, dispatch_command, input,
    require,
};
use bevy::prelude::*;

pub(super) fn progress(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    match check.stage {
        0 if view.visible(&control.battle, check.setup.sniper, true) => {
            require(
                control.battle.simulation.entities[check.setup.sniper].stealthed(),
                "original Jarmen stationary cloak did not reach the rendered body",
            )?;
            view.command(&control.battle, false)?;
            control.clock.pause();
            control.capture.intermediate("jarmen-cloak.png");
            check.stage = 1;
        }
        1 => {
            dispatch_command(
                &mut control.battle,
                &mut control.mouse,
                CommandAction::SnipeVehicle(1),
            );
            require(
                matches!(control.mouse.pending_command, CommandMode::SnipeVehicle(1)),
                "original Jarmen command did not enter pilot-target mode",
            )?;
            control.mouse.hovered = Some(check.setup.vehicle);
            require(
                crate::attack_cursor::context(&mut control.battle, &control.mouse) == Some(11),
                "original pilot command lacks the SnipeVehicle cursor",
            )?;
            control.mouse.hovered = Some(check.setup.sniper);
            require(
                crate::attack_cursor::context(&mut control.battle, &control.mouse) == Some(3),
                "pilot command admitted friendly infantry",
            )?;
            check.stage = 2;
        }
        2 if check.audio.mode_finished => {
            let point =
                Vec2::from_array(control.battle.simulation.entities[check.setup.vehicle].position);
            input::targeted_order(
                &mut control.battle,
                &mut control.mouse,
                point,
                Some(check.setup.vehicle),
            );
            require(
                matches!(control.mouse.pending_command, CommandMode::Normal),
                "original targeted pilot shot was rejected",
            )?;
            control.clock.unpause();
            check.stage = 3;
        }
        3 => pilot(check, view, control)?,
        4 if check.audio.pilot_finished => {
            control.battle.select_objects(&[check.setup.driver], true);
            context(control, check.setup.vehicle)?;
            check.stage = 5;
        }
        5 => takeover(check, control)?,
        6 => {
            control.battle.select_objects(&[check.setup.sniper], true);
            context(control, check.setup.infantry)?;
            check.stage = 7;
        }
        7 => rifle(check, view, control)?,
        // Camera and visibility must have updated after the final focus change.
        8 if view.visible(&control.battle, check.setup.sniper, true) => {
            control.clock.pause();
            control.capture.finish_scenario();
            check.stage = 255;
            println!(
                "Original Jarmen native: paid hero, cloak/body/commands, source pilot cursor and reload clock, one pilot shot and unchanged-health neutral vehicle, paid infantry takeover, advancing approach geometry, authored shot-tick rifle pose/credited kill and six advancing source sounds passed"
            );
        }
        _ => {}
    }
    Ok(())
}

fn pilot(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let game = &control.battle.simulation;
    let vehicle = &game.entities[check.setup.vehicle];
    if !vehicle.unmanned() {
        return Ok(());
    }
    require(
        check.shots[1] == 1
            && check.pilot_event
            && vehicle.health.to_bits() == check.vehicle_health
            && vehicle.owner == game.neutral_owner()
            && game.players[1].statistics.units_lost == check.losses[1]
            && game.players[0].statistics.units_destroyed == check.kills,
        "pilot effect changed source health/neutrality/shot or score semantics",
    )?;
    if check.audio.sounds[..3].iter().all(|heard| *heard)
        && check.audio.sounds[5]
        && view.visible(&control.battle, check.setup.sniper, false)
        && view.visible(&control.battle, check.setup.vehicle, false)
    {
        view.command(&control.battle, true)?;
        control.clock.pause();
        control.capture.intermediate("jarmen-pilot.png");
        check.stage = 4;
    }
    Ok(())
}

fn takeover(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let game = &control.battle.simulation;
    let vehicle = &game.entities[check.setup.vehicle];
    if vehicle.owner != 0 || vehicle.unmanned() {
        return Ok(());
    }
    require(
        !game.entities[check.setup.driver].alive()
            && game.players[0].statistics.units_lost == check.losses[0],
        "ordinary infantry takeover failed or credited a driver death",
    )?;
    control.battle.select_objects(&[check.setup.vehicle], true);
    control.clock.pause();
    check.stage = 6;
    Ok(())
}

fn rifle(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let game = &control.battle.simulation;
    let actor = &game.entities[check.setup.sniper];
    control.camera.focus = Vec2::from_array(actor.position) * 0.1;
    view.animation(&control.battle, &check.setup, &mut check.animation)?;
    if check.animation.shot {
        resume_after_rifle_pose(check, &mut control.clock, &mut control.fixed);
    }
    if game.entities[check.setup.infantry].alive() || !actor.stealthed() {
        return Ok(());
    }
    require(
        check.shots[0] == 1
            && actor.alive()
            && actor.experience > 0
            && game.players[1].statistics.units_lost == check.losses[1] + 1
            && game.players[0].statistics.units_destroyed == check.kills + 1,
        "ordinary Jarmen rifle did not produce its single credited infantry kill",
    )?;
    if check.audio.sounds.iter().all(|heard| *heard) && check.animation.complete() {
        check.stage = 8;
    }
    Ok(())
}

pub(super) fn resume_after_rifle_pose(
    check: &mut Check,
    clock: &mut Time<Virtual>,
    fixed: &mut Time<Fixed>,
) {
    if let Some(remaining) = check.held_overstep.take() {
        fixed.accumulate_overstep(remaining);
        clock.unpause();
    }
}

fn context(control: &mut Control, target: usize) -> Result<(), Box<dyn Error>> {
    let point = Vec2::from_array(control.battle.simulation.entities[target].position);
    require(
        input::context_order(
            &mut control.battle,
            Some(point),
            Some(PickTarget::Unit(target)),
            &mut control.mouse,
        ),
        "original context command was rejected",
    )?;
    control.clock.unpause();
    Ok(())
}
