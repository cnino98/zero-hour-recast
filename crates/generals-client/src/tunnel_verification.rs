//! Original Tunnel inventory, remote command dispatch and rendered acceptance.
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, CommandMode, Mouse, PickTarget},
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual, WorldCamera},
    skirmish_verification::{advance_until, require, tunnel::Setup},
};
use bevy::{app::AppExit, prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = crate::skirmish_verification::tunnel::prepare(&mut battle.simulation)?;
    let remote = setup.entrances[1];
    battle.select_objects(&[remote], true);
    let data = app.world().resource::<UiData>();
    for (slot, name) in [
        (1, "Command_StructureExit"),
        (11, "Command_TunnelEvacuate"),
        (13, "Command_SetRallyPoint"),
        (14, "Command_Sell"),
    ] {
        require(
            selected_command(battle, data, slot).is_some_and(|command| command.name == name),
            "original tunnel inventory/command missing",
        )?;
    }
    let slot = battle
        .simulation
        .passengers(remote)
        .iter()
        .position(|&id| id == setup.tank)
        .ok_or("tank missing from shared tunnel inventory")?;
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("pointer missing")?;
    dispatch_command(battle, &mut mouse, CommandAction::Rally);
    require(
        matches!(mouse.pending_command, CommandMode::Rally),
        "rally button failed to enter original target mode",
    )?;
    let point = battle.simulation.entities[remote].position;
    require(
        input::rally_order(
            battle,
            &mut mouse,
            Vec3::new(
                point[0],
                point[1] - 100.0,
                battle
                    .simulation
                    .ground_height([point[0], point[1] - 100.0]),
            ),
        ),
        "remote tunnel rally rejected",
    )?;
    input::commands::step(battle, &mut mouse);
    require(
        battle
            .simulation
            .producer_rally_path(remote)
            .is_some_and(|path| path.len() == 3),
        "selected tunnel lacks original center/natural/user rally feedback",
    )?;
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(slot));
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        30,
        "original remote inventory exit",
        |game| game.entities[setup.tank].contained_by().is_none(),
    )?;
    let tank = battle.simulation.entities[setup.tank].position;
    require(
        (tank[0] - point[0]).hypot(tank[1] - point[1]) < 100.0,
        "original inventory exit used wrong tunnel",
    )?;
    battle.select_objects(&[setup.tank], true);
    require(
        input::context_order(battle, None, Some(PickTarget::Unit(remote)), &mut mouse),
        "original tunnel context re-entry failed",
    )?;
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        3000,
        "tank returning into remote tunnel",
        |game| game.entities[setup.tank].contained_by() == Some(remote),
    )?;
    battle.select_objects(&[remote], true);
    mouse.voices.clear();
    app.insert_resource(mouse)
        .insert_resource(Check { setup, done: false });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let point = {
        let check = app.world().resource::<Check>();
        let battle = app.world().resource::<Match>();
        let [first, second] = check
            .setup
            .entrances
            .map(|id| Vec2::from_array(battle.simulation.entities[id].position));
        (first + second) * 0.05
    };
    app.world_mut().resource_mut::<CameraRig>().focus = point;
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct Presentation<'w, 's> {
    data: Res<'w, UiData>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static ViewVisibility)>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static ComputedNode,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    icons: Res<'w, Assets<PromotionIcon>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
}

fn inspect(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    presentation: Presentation,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done || !capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    let Presentation {
        data,
        visuals,
        buttons,
        icons,
        camera,
    } = presentation;
    let (camera, transform) = *camera;
    let Some(viewport) = camera.logical_viewport_rect() else {
        return;
    };
    let game = &battle.simulation;
    let ready = check.setup.entrances.iter().all(|&id| {
        let point = crate::playtest_view::unit_position(game, id);
        camera
            .world_to_viewport(transform, point)
            .is_ok_and(|point| viewport.contains(point))
            && visuals.iter().any(|(visual, visible)| {
                visual.generation == battle.generation && visual.index == id && visible.get()
            })
    });
    if !ready {
        return;
    }
    if game.passengers(check.setup.entrances[1]).len() != 2 {
        check.done = true;
        error!("Original tunnel shared inventory changed before rendering");
        exit.write(AppExit::error());
        return;
    }
    let painted = [1, 2, 11, 13, 14].into_iter().all(|slot| {
        buttons.iter().any(|(action, node, material)| {
            matches!(action, Action::Slot(index) if *index == slot)
                && node.size().min_element() > 0.0
                && selected_command(battle, &data, slot).is_some_and(|command| {
                    let image = if slot <= 2 {
                        let id = battle.simulation.passengers(check.setup.entrances[1])[slot - 1];
                        &data.units[battle.simulation.entities[id].type_index].button_image
                    } else {
                        &command.image
                    };
                    let expected = data.icon(image);
                    icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
                })
        })
    });
    check.done = true;
    if !painted {
        error!("Original tunnel inventory failed live render checks");
        exit.write(AppExit::error());
        return;
    }
    println!(
        "Rendered Tunnel Network: original entrance models, two shared passenger portraits, remote individual exit/re-entry, rally and evacuation command art passed"
    );
    capture.finish_scenario();
}
