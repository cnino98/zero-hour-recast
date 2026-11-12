use super::{Check, Control, Error, View, finish, move_order, require};
use crate::{control_bar::CommandAction, playtest_ui::dispatch_command};
use bevy::prelude::*;

pub(super) fn progress(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    match check.phase {
        0 => purchase(check, view, control)?,
        1 => {
            let game = &control.battle.simulation;
            if let Some(child) = game.produced_drone(check.actor) {
                let unit = &game.catalog[game.entities[child].type_index];
                require(
                    unit.name == "AmericaVehicleScoutDrone"
                        && unit.kind_of.iter().any(|flag| flag == "NO_SELECT")
                        && !game.entities[child].selectable(unit)
                        && game.entities[child].drone_parent() == Some(check.actor),
                    "paid Scout has wrong original template, parent or NO_SELECT flag",
                )?;
                view.animation(
                    &control.battle,
                    child,
                    "avscoutdr.avscoutdr",
                    &mut check.animation,
                )?;
                if check.animation.advanced
                    && check.speech_finished
                    && view.visible(&control.battle, child, false)
                {
                    let point = game.entities[check.target].position;
                    move_order(control, point)?;
                    check.phase = 2;
                }
            }
        }
        2 => {
            let game = &control.battle.simulation;
            let target = &game.entities[check.target];
            if target.stealthed() && target.detected() && game.logically_visible(0, check.target) {
                dispatch_command(&mut control.battle, &mut control.mouse, CommandAction::Stop);
                control.clock.pause();
                check.phase = 3;
            }
        }
        3..=6 => capture_detection(check, view, control)?,
        _ => {}
    }
    Ok(())
}

fn capture_detection(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    match check.phase {
        3 => {
            let game = &control.battle.simulation;
            let child = game
                .produced_drone(check.actor)
                .ok_or("Scout lost before body proof")?;
            control.camera.distance = control.camera.settings.min_height;
            view.focus_aircraft(&mut control.camera, game.object_position(child));
            // Accept only after the camera and visibility have processed this focus.
            check.phase = 4;
        }
        4 => {
            let child = control
                .battle
                .simulation
                .produced_drone(check.actor)
                .ok_or("Scout lost before body capture")?;
            if view.framed_aircraft(&control.battle, child)? {
                control.capture.intermediate("scout-body.png");
                check.phase = 5;
            }
        }
        5 => {
            let game = &control.battle.simulation;
            let child = game
                .produced_drone(check.actor)
                .ok_or("Scout lost before detection proof")?;
            let from = Vec3::from_array(game.object_position(child));
            let to = Vec3::from_array(game.object_position(check.target));
            // Keep the two subjects across the wide axis of the playable
            // viewport. Use the authored camera limits and both object radii
            // when fitting the ground-level target alongside the flying Scout.
            let delta = to - from;
            let radius = [child, check.target]
                .map(|id| game.catalog[game.entities[id].type_index].geometry.major)
                .into_iter()
                .sum::<f32>();
            let span = 2.0_f32.mul_add(radius, delta.truncate().length());
            let ground_height =
                span * 0.05 * control.camera.settings.pitch.sin() / 25.0_f32.to_radians().tan();
            let height = delta.z.abs().mul_add(0.05, ground_height);
            control.camera.distance = height.clamp(
                control.camera.settings.min_height,
                control.camera.settings.max_height,
            );
            control.camera.settings.yaw = delta.y.atan2(delta.x);
            view.focus_aircraft(&mut control.camera, ((from + to) * 0.5).to_array());
            check.phase = 6;
        }
        6 => {
            let game = &control.battle.simulation;
            let child = game
                .produced_drone(check.actor)
                .ok_or("Scout lost before detection proof")?;
            require(
                game.entities[check.target].alive(),
                "Scout victim died before native detection proof",
            )?;
            if check.sounds[..2].iter().all(|heard| *heard)
                && view.framed_aircraft(&control.battle, child)?
                && view.detected(&control.battle, check.target)
            {
                finish(check, control);
                println!(
                    "Original Scout native: paid $100 button, original NO_SELECT child/rotor in its own fully framed image, real Move voice, research voice and visible detection of concealed enemy passed"
                );
            }
        }
        _ => {}
    }
    Ok(())
}

fn purchase(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let action = view.command(&control.battle, 2)?;
    let CommandAction::Research(index) = action else {
        return Err("Scout slot is not original object upgrade".into());
    };
    let game = &control.battle.simulation;
    let actor = &game.entities[check.actor];
    let upgrade = &game.catalog[actor.type_index].maintenance.research[index];
    require(
        upgrade.name == "Upgrade_AmericaScoutDrone" && upgrade.cost == 100,
        "Scout button points at different original research",
    )?;
    require(
        !game.logically_visible(0, check.target) && game.entities[check.target].stealthed(),
        "Scout native check must start with undetected concealed target",
    )?;
    let cash = game.players[0].cash;
    dispatch_command(&mut control.battle, &mut control.mouse, action);
    require(
        control.battle.simulation.players[0].cash + 100 == cash,
        "Scout original command did not charge $100",
    )?;
    check.phase = 1;
    control.clock.unpause();
    Ok(())
}
