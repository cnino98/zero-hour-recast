//! Installed original Overlord command-set changes and dependent model rendering.
use super::{passenger, visible};
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, Mouse, PickTarget},
    model_animation::{Animations, Players},
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual, WorldCamera},
    skirmish_verification::{advance_until, require},
};
use bevy::{app::AppExit, prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    tanks: [usize; 3],
    rider: usize,
    done: bool,
    started: u64,
    last_pulse: Option<u64>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = crate::skirmish_verification::overlord::prepare(&mut battle.simulation)?;
    let bunker = setup.tanks[2];
    battle.select_objects(&[bunker], true);
    let data = app.world().resource::<UiData>();
    for (slot, expected) in [
        (1, "Command_TransportExit"),
        (6, "Command_Evacuate"),
        (14, "Command_Stop"),
    ] {
        require(
            selected_command(battle, data, slot).is_some_and(|command| command.name == expected),
            "Overlord command-set upgrade did not select the original bunker controls",
        )?;
    }
    for &id in &setup.tanks[..2] {
        require(
            data.commands(&battle.simulation, id).iter().all(|command| {
                !matches!(
                    command.action,
                    CommandAction::Research(_)
                        | CommandAction::ExitContainer(_)
                        | CommandAction::Evacuate
                )
            }),
            "Cannon/speaker retained obsolete research or bunker controls",
        )?;
    }
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("pointer missing")?;
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(0));
    advance_until(
        &mut battle.simulation,
        300,
        "Overlord original inventory exit",
        |game| game.entities[setup.rider].contained_by().is_none(),
    )?;
    battle.select_objects(&[setup.rider], true);
    require(
        input::context_order(battle, None, Some(PickTarget::Unit(bunker)), &mut mouse),
        "Overlord right-click entry rejected",
    )?;
    advance_until(
        &mut battle.simulation,
        3000,
        "Overlord original context re-entry",
        |game| {
            game.entities[setup.rider].contained_by() == game.entities[bunker].attachment_child()
        },
    )?;
    battle.select_objects(&[bunker], true);
    // Preserve a genuine chassis turn in the captured native draw dependency.
    for id in setup.tanks {
        let point = battle.simulation.entities[id].position;
        battle
            .simulation
            .move_to(id, [point[0] - 40.0, point[1] - 70.0])?;
    }
    for _ in 0..12 {
        battle.simulation.step();
    }
    crate::skirmish_verification::overlord::engage(&mut battle.simulation, setup.tanks[0])?;
    mouse.voices.clear();
    app.insert_resource(mouse).insert_resource(Check {
        tanks: setup.tanks,
        rider: setup.rider,
        done: false,
        started: battle.simulation.tick,
        last_pulse: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}

/// Frame all prepared source objects without changing their simulation positions.
pub fn frame_camera(app: &mut App) {
    let check = app.world().resource::<Check>();
    let battle = app.world().resource::<Match>();
    let points = check
        .tanks
        .map(|id| Vec2::from_array(battle.simulation.entities[id].position));
    let minimum = points
        .into_iter()
        .reduce(Vec2::min)
        .expect("three prepared tanks");
    let maximum = points
        .into_iter()
        .reduce(Vec2::max)
        .expect("three prepared tanks");
    app.world_mut().resource_mut::<CameraRig>().focus = (minimum + maximum) * 0.05;
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w, 's> {
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
    icons: Res<'w, Assets<PromotionIcon>>,
    animations: Res<'w, Animations>,
    clock: ResMut<'w, Time<Virtual>>,
    particles: crate::particles::ParticleInspection<'w, 's>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
}

type Visual = (
    &'static UnitVisual,
    &'static Visibility,
    &'static Transform,
    &'static Players,
);

fn inspect(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    data: Res<UiData>,
    visuals: Query<Visual>,
    buttons: Query<(&Action, &Node, &MaterialNode<PromotionIcon>)>,
    mut completion: Completion,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    if check.done || !completion.capture.ready_for_scenario() {
        return;
    }
    let speaker = battle.simulation.entities[check.tanks[1]].attachment_child();
    if battle
        .simulation
        .presentation
        .propaganda_events
        .iter()
        .any(|pulse| Some(pulse.source) == speaker)
    {
        check.last_pulse = Some(battle.simulation.tick);
    }
    let result = verify(&check, battle, data, &visuals, &buttons, &completion);
    if let Err(message) = result {
        check.done = true;
        error!("Overlord native render validation: {message}");
        completion.exit.write(AppExit::error());
        return;
    }
    match completion.particles.propaganda(battle.generation) {
        Ok(true) => {}
        Ok(false) if battle.simulation.tick - check.started < 180 => {
            completion.clock.unpause();
            return;
        }
        result => {
            check.done = true;
            error!("Original speaker pulse was not visibly rendered: {result:?}");
            pulse_failure(&check, battle);
            completion.exit.write(AppExit::error());
            return;
        }
    }
    check.done = true;
    completion.clock.pause();
    completion.capture.finish_scenario();
    println!(
        "Rendered Overlords: all three original attachments follow current animated parent bones during chassis turns; original upgraded command sets, hidden bunker passenger, inventory portrait, exit and context re-entry, actual child Gattling fire and original visible propaganda pulse passed"
    );
}

fn pulse_failure(check: &Check, battle: &Match) {
    let game = &battle.simulation;
    let parent = check.tanks[1];
    let Some(child) = game.entities[parent].attachment_child() else {
        return;
    };
    let actor = &game.entities[child];
    let visibility = match game.visibility_status(0, child) {
        generals_sim::skirmish::visibility::Status::Clear => "Clear",
        generals_sim::skirmish::visibility::Status::Partial => "Partial",
        generals_sim::skirmish::visibility::Status::Fogged => "Fogged",
        generals_sim::skirmish::visibility::Status::Shrouded => "Shrouded",
    };
    error!(
        "Speaker pulse diagnostic: generation={} frame={} last_pulse={:?} child={} parent={} position={:?} owner={} complete={} parent_masked={} visibility={}",
        battle.generation,
        game.tick,
        check.last_pulse,
        child,
        parent,
        game.object_position(child),
        actor.owner,
        actor.complete(),
        game.entities[parent].masked(),
        visibility
    );
}

fn verify(
    check: &Check,
    battle: &Match,
    data: &UiData,
    visuals: &Query<Visual>,
    buttons: &Query<(&Action, &Node, &MaterialNode<PromotionIcon>)>,
    completion: &Completion,
) -> Result<(), &'static str> {
    let present = |id| {
        visuals
            .iter()
            .find(|(visual, _, _, _)| visual.index == id && visual.generation == battle.generation)
    };
    for &parent in &check.tanks {
        let actor = &battle.simulation.entities[parent];
        let child = actor.attachment_child().ok_or("missing child")?;
        let (_, visibility, transform, players) =
            present(parent).ok_or("missing parent drawable")?;
        let (_, child_visibility, child_transform, _) =
            present(child).ok_or("missing dependent drawable")?;
        require_in_view(transform.translation, completion)?;
        if *visibility != Visibility::Visible || child_visibility != visibility {
            return Err("attachment visibility does not follow parent");
        }
        let bones = completion
            .animations
            .effect_bones(actor, players, "FIREPOINT01")
            .map_err(|_| "invalid original bone")?;
        let bone = bones.first().ok_or("original FIREPOINT01 absent")?;
        let local = Vec3::new(
            bone.translation[0],
            bone.translation[2],
            -bone.translation[1],
        ) * 0.1;
        let expected = transform.transform_point(local);
        if child_transform.translation.distance(expected) > 0.0001 {
            return Err("dependent model did not follow current parent bone");
        }
    }
    if present(check.rider).is_none_or(|(_, visibility, _, _)| *visibility != Visibility::Hidden) {
        return Err("enclosed bunker passenger is visible");
    }
    let expected =
        data.icon(&data.units[battle.simulation.entities[check.rider].type_index].button_image);
    let displayed = buttons.iter().any(|(action, node, material)| {
        matches!(action, Action::Slot(1))
            && passenger(battle, 0) == Some(check.rider)
            && visible(battle, 0)
            && node.display != Display::None
            && completion.icons.get(&material.0).is_some_and(|icon| {
                icon.image == expected.image && icon.uv_rect == expected.uv_rect
            })
    });
    if !displayed {
        return Err("original bunker inventory portrait absent");
    }
    Ok(())
}

fn require_in_view(position: Vec3, completion: &Completion) -> Result<(), &'static str> {
    let (camera, transform) = &*completion.camera;
    let rect = camera
        .logical_viewport_rect()
        .ok_or("capture viewport missing")?;
    let point = camera
        .world_to_viewport(transform, position)
        .map_err(|_| "Overlord behind capture camera")?;
    let inside = Rect::from_corners(rect.min + Vec2::splat(64.0), rect.max - Vec2::splat(64.0));
    if !inside.contains(point) {
        return Err("Overlord center too close to capture edge to show its attachment");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_overlord_inventory_and_evacuate_keep_original_bunker() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let root = &generals_assets::game_root().unwrap();
        let options = root.join("UserData/Options.ini");
        let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
        let mut app = App::new();
        app.init_resource::<Time<Virtual>>()
            .insert_resource(data)
            .insert_resource(mouse);
        // This is the native capture's unchanged paid preparation, inventory
        // exit guard and ordinary right-click re-entry, through actual dispatch.
        prepare(&mut app, &mut battle).unwrap();
        let check = app.world().resource::<Check>();
        let parent = check.tanks[2];
        let rider = check.rider;
        let child = battle.simulation.entities[parent]
            .attachment_child()
            .unwrap();
        assert_eq!(
            battle.simulation.catalog[battle.simulation.entities[child].type_index].name,
            "ChinaTankOverlordBattleBunker"
        );
        assert_eq!(
            battle.simulation.entities[rider].contained_by(),
            Some(child)
        );
        assert_eq!(battle.simulation.passengers(parent), [rider]);
        assert!(battle.simulation.entities[rider].disabled_held());
        let data = app.world_mut().remove_resource::<UiData>().unwrap();
        let mut mouse = app.world_mut().remove_resource::<Mouse>().unwrap();
        let command = selected_command(&battle, &data, 1).unwrap().action;
        assert!(matches!(command, CommandAction::ExitContainer(0)));
        // Isolate native onExit's sender/rider ownership guard. This is an
        // explicit control-permission fixture, not a simulated capture callback.
        battle.simulation.entities[rider].owner = 1;
        dispatch_command(&mut battle, &mut mouse, command);
        for _ in 0..3 {
            battle.simulation.step();
        }
        assert_eq!(
            battle.simulation.entities[rider].contained_by(),
            Some(child)
        );
        battle.simulation.entities[rider].owner = 0;
        dispatch_command(&mut battle, &mut mouse, command);
        advance_until(
            &mut battle.simulation,
            300,
            "same real paid Overlord inventory exit",
            |game| game.entities[rider].contained_by().is_none(),
        )
        .unwrap();
        assert!(battle.simulation.passengers(parent).is_empty());
        assert_eq!(
            battle.simulation.entities[parent].attachment_child(),
            Some(child)
        );
        assert_eq!(
            battle.simulation.entities[child].contained_by(),
            Some(parent)
        );
        assert!(!battle.simulation.entities[rider].masked());
        assert!(!battle.simulation.entities[rider].disabled_held());
        battle.select_objects(&[rider], true);
        assert!(input::context_order(
            &mut battle,
            None,
            Some(PickTarget::Unit(parent)),
            &mut mouse,
        ));
        advance_until(
            &mut battle.simulation,
            3000,
            "paid Overlord ordinary context entry before evacuation",
            |game| game.entities[rider].contained_by() == Some(child),
        )
        .unwrap();
        battle.select_objects(&[parent], true);
        let command = selected_command(&battle, &data, 6).unwrap().action;
        assert!(matches!(command, CommandAction::Evacuate));
        dispatch_command(&mut battle, &mut mouse, command);
        advance_until(
            &mut battle.simulation,
            300,
            "same real paid Overlord ordinary evacuate",
            |game| game.entities[rider].contained_by().is_none(),
        )
        .unwrap();
        assert!(battle.simulation.passengers(parent).is_empty());
        assert_eq!(
            battle.simulation.entities[parent].attachment_child(),
            Some(child)
        );
        assert_eq!(
            battle.simulation.entities[child].contained_by(),
            Some(parent)
        );
        assert!(battle.simulation.entities[child].disabled_held());
        assert!(!battle.simulation.entities[rider].masked());
        assert!(!battle.simulation.entities[rider].disabled_held());
        assert!(!battle.simulation.entities[rider].unselectable());
    }
}
