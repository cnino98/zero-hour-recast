use super::{
    Check, Control, Error, Voice, attack, dispatch_command, enter, input, require, view::View,
};
use bevy::prelude::*;

pub(super) fn progress(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    if check.stage <= 6 {
        inventory(check, view, control)
    } else if check.stage <= 10 {
        route(check, view, control)
    } else {
        combat(check, view, control)
    }
}

fn inventory(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let host = check.setup.outpost;
    let game = &control.battle.simulation;
    require(
        game.entities[host].alive(),
        "Outpost died before passenger acceptance",
    )?;
    match check.stage {
        0 if view.body(&control.battle, host, false) => {
            view.inventory(&control.battle, check)?;
            let command = view.command(&control.battle, 1)?;
            dispatch_command(&mut control.battle, &mut control.mouse, command);
            control.clock.unpause();
            check.stage = 1;
        }
        1 if game.entities[check.setup.riders[0]]
            .contained_by()
            .is_none()
            && view.body(&control.battle, check.setup.riders[0], false) =>
        {
            enter(control, &check.setup.riders[..1], host)?;
            check.stage = 2;
        }
        2 if game.passengers(host).len() == 2 => {
            let command = view.command(&control.battle, 12)?;
            dispatch_command(&mut control.battle, &mut control.mouse, command);
            check.stage = 3;
        }
        3 if check
            .setup
            .riders
            .iter()
            .all(|&id| game.entities[id].contained_by().is_none()) =>
        {
            let actor = &game.entities[host];
            require(
                actor
                    .active_weapon(&game.catalog[actor.type_index])
                    .name
                    .is_empty(),
                "empty Outpost retained armed-rider weapon",
            )?;
            enter(control, &check.setup.riders, host)?;
            check.stage = 4;
        }
        4 if game.passengers(host).len() == 2 => {
            let actor = &game.entities[host];
            require(
                actor.active_weapon(&game.catalog[actor.type_index]).name
                    == "ListeningOutpostUpgradedDummyWeapon",
                "reentered riders did not restore original weapon set",
            )?;
            let position = Vec2::from_array(actor.position);
            let enemy = Vec2::from_array(game.entities[check.setup.enemies[0]].position);
            let goal = (position + (enemy - position).normalize_or_zero() * 10.0).to_array();
            move_order(control, goal)?;
            check.move_goal = Some(goal);
            check.stage = 5;
        }
        5 if !game.entities[host].moving()
            && check.move_goal.is_some_and(|goal| {
                let point = game.entities[host].position;
                (point[0] - goal[0]).hypot(point[1] - goal[1]) < 15.0
            }) =>
        {
            let command = view.command(&control.battle, 14)?;
            dispatch_command(&mut control.battle, &mut control.mouse, command);
            check.stage = 6;
        }
        6 if game.entities[host].stealthed()
            && check.scan
            && check.animated
            && check.audio[0]
            && view.body(&control.battle, host, true)
            && view.scan(control.battle.generation)? =>
        {
            view.inventory(&control.battle, check)?;
            control.clock.pause();
            control.capture.intermediate("outpost-loaded.png");
            check.stage = 7;
            println!(
                "Outpost native passenger exit/context reentry/evacuation, restored two portraits, hidden riders, stationary original dish animation, friendly cloak and IRLenzflare passed; IRPing/IRPingLoud are authored silent"
            );
        }
        _ => {}
    }
    Ok(())
}

fn route(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let host = check.setup.outpost;
    let enemy = check.setup.enemies[0];
    match check.stage {
        7 => {
            let Some(pointer) = view.pick_enemy(&control.battle, enemy) else {
                return Ok(());
            };
            // The tank is at its factory exit. Place the route in the open
            // approach between the two vehicles so the goal and SCMNode
            // remain in the world camera, clear of the factory.
            let game = &mut control.battle.simulation;
            let point = Vec2::from_array(game.entities[enemy].position);
            let host_point = Vec2::from_array(game.entities[host].position);
            let goal = point + (host_point - point).normalize_or_zero() * 60.0;
            game.move_to(enemy, goal.to_array())?;
            control.mouse.waypoint_mode = false;
            check.pointer = Some(pointer);
            check.stage = 8;
        }
        8 if control.mouse.hovered == Some(enemy) && view.route() => {
            require(
                !control.mouse.waypoint_mode,
                "Outpost route incorrectly required Alt",
            )?;
            control.capture.intermediate("outpost-route.png");
            check.stage = 9;
            println!(
                "Outpost revealed enemy route: actual triangle pick, ordinary enemy Move goal, original orange EXLaser and SCMNode visible without Alt"
            );
        }
        9 => {
            check.pointer = None;
            control.battle.simulation.stop(enemy)?;
            let point = control.battle.simulation.entities[enemy].position;
            move_order(control, point)?;
            check.stage = 10;
        }
        10 => {
            let game = &control.battle.simulation;
            let point = game.entities[host].position;
            let target = game.entities[enemy].position;
            if game.logically_visible(1, host)
                && (point[0] - target[0]).hypot(point[1] - target[1]) < 200.0
            {
                for id in check.setup.enemies {
                    control.battle.simulation.attack(id, host)?;
                }
                attack(control, enemy)?;
                check.stage = 11;
            }
        }
        _ => {}
    }
    Ok(())
}

fn combat(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let host = check.setup.outpost;
    let game = &control.battle.simulation;
    match check.stage {
        11 => {
            require(
                game.entities[host].alive(),
                "Outpost died before its rider missile was rendered",
            )?;
            if check.shots > 0
                && check.audio[1]
                && view.missile(&control.battle, &check.setup.riders)?
            {
                control.clock.pause();
                control.capture.intermediate("outpost-combat.png");
                check.stage = 12;
            }
        }
        12 => {
            control.clock.unpause();
            check.stage = 13;
        }
        13 if game.entities[host].corpse_present()
            && view.body(&control.battle, host, false)
            && view.debris(&control.battle, host) =>
        {
            control.clock.pause();
            control.capture.intermediate("outpost-dying.png");
            check.stage = 14;
        }
        14 => {
            control.clock.unpause();
            check.stage = 15;
        }
        15 if check.final_death.is_some() && view.hull(&control.battle, host) => {
            require(
                check
                    .death
                    .zip(check.final_death)
                    .is_some_and(|(began, ended)| ended - began == 54),
                "Outpost retained body did not follow its original 1789ms deadline",
            )?;
            require(
                check.audio.iter().all(|heard| *heard) && check.shots > 0,
                "Outpost native audible command or rider weapon proof missing",
            )?;
            control.clock.pause();
            control.capture.finish_scenario();
            check.stage = 255;
            println!(
                "Outpost native passed: paid source vehicle/two riders, original inventory/context controls, concealment/scan, enemy route, real rider missile/exhaust/weapon audio, retained dying body/debris and final hull. Original Attack voice request preserved; installed vlisata-e samples are absent."
            );
        }
        _ => {}
    }
    Ok(())
}

fn move_order(control: &mut Control, goal: [f32; 2]) -> Result<(), Box<dyn Error>> {
    require(
        input::context_order(
            &mut control.battle,
            Some(Vec2::from_array(goal)),
            None,
            &mut control.mouse,
        ),
        "Outpost original context Move rejected",
    )?;
    require(
        control
            .mouse
            .voices
            .last()
            .is_some_and(|request| matches!(request.voice(), Voice::Move)),
        "Outpost Move did not request original speech",
    )?;
    control.clock.unpause();
    Ok(())
}
