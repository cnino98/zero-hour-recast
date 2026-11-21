//! Paid capture research and real original command/cursor/countdown/animation/flash/audio.
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, CommandMode, Mouse},
    model_animation::{Animations, Players},
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, dispatch_command},
    playtest_view::{Capture, UnitVisual, WorldCamera},
    selection_feedback::CaptureTintInspection,
    skirmish_verification::{advance_until, require},
    source_audio::CaptureAudioInspection,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    actor: usize,
    target: usize,
    slot: usize,
    image: String,
    previous_frame: Option<f32>,
    animated: bool,
    unpacked: bool,
    stage: Stage,
    observed: [bool; 4],
    last_report: u64,
    last_condition: &'static str,
}

enum Stage {
    Waiting,
    Running,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let [actor, target] = crate::maintenance_verification::prepare_capture(&mut battle.simulation)?;
    battle.select_objects(&[actor], true);
    require(
        battle.simulation.can_capture(actor, target),
        "Paid scout must stop at the original capture visibility boundary",
    )?;
    println!(
        "Capture input boundary: original action accepts target, fully clear={}",
        battle.simulation.visible_to(0, target)
    );
    let data = app.world().resource::<UiData>();
    let command = data.units[battle.simulation.entities[actor].type_index]
        .commands
        .iter()
        .find(|command| matches!(command.action, CommandAction::Capture))
        .ok_or("Installed Ranger capture command missing")?;
    let slot = command.slot;
    let image = command.image.clone();
    require(!image.is_empty(), "Original capture command lacks artwork")?;
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("pointer missing")?;
    mouse.hovered = Some(target);
    require(
        crate::attack_cursor::context(battle, &mouse) == Some(7),
        "Neutral oil lacks context capture cursor",
    )?;
    let builder = crate::skirmish_verification::friendly_entity(
        &battle.simulation,
        generals_sim::skirmish::Kind::Builder,
        0,
    )?;
    battle.selected.insert(0, builder);
    require(
        input::capture::context_target(battle, &mouse, Some(target)).is_none(),
        "Capture hint must inspect first selected command set",
    )?;
    battle.select_objects(&[actor], true);
    dispatch_command(battle, &mut mouse, CommandAction::Capture);
    require(
        matches!(mouse.pending_command, CommandMode::Capture),
        "Original capture button did not target",
    )?;
    mouse.hovered = Some(actor);
    require(
        crate::attack_cursor::context(battle, &mouse) == Some(3),
        "Own infantry must show invalid capture cursor",
    )?;
    mouse.hovered = Some(target);
    require(
        crate::attack_cursor::context(battle, &mouse) == Some(7),
        "Capture target mode lacks original cursor",
    )?;
    let point = Vec2::from_array(battle.simulation.entities[target].position);
    input::targeted_order(battle, &mut mouse, point, Some(target));
    require(
        matches!(mouse.pending_command, CommandMode::Normal),
        "Accepted capture failed to leave target mode",
    )?;
    input::commands::step(battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        18_000,
        "original capture approach and facing before native unpacking",
        |game| game.entities[actor].capture_condition() == Some("UNPACKING"),
    )?;
    app.insert_resource(mouse).insert_resource(Check {
        actor,
        target,
        slot,
        image,
        previous_frame: None,
        animated: false,
        unpacked: false,
        stage: Stage::Waiting,
        observed: [false; 4],
        last_report: 0,
        last_condition: "pending",
    });
    println!(
        "Installed capture input: original researched button, context/explicit CaptureBuilding cursor, first-selected lookup, invalid friendly and accepted command passed"
    );
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let focus = (Vec2::from_array(game.entities[check.actor].position)
        + Vec2::from_array(game.entities[check.target].position))
        * 0.05;
    let mut rig = app.world_mut().resource_mut::<CameraRig>();
    rig.focus = focus;
    rig.distance = rig.settings.min_height;
    app.world_mut()
        .resource_mut::<Capture>()
        .wait_for_scenario();
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
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
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Players)>,
    animations: Res<'w, Animations>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w, 's> {
    capture: ResMut<'w, Capture>,
    clock: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    exit: MessageWriter<'w, AppExit>,
    tint: CaptureTintInspection<'w, 's>,
    audio: CaptureAudioInspection<'w, 's>,
}

fn inspect(
    battle: Res<Match>,
    mut check: ResMut<Check>,
    presentation: Presentation,
    completion: Completion,
) {
    let battle = battle.into_inner();
    let Presentation {
        data,
        icons,
        buttons,
        visuals,
        animations,
        camera,
    } = presentation;
    let Completion {
        mut capture,
        mut clock,
        real,
        mut exit,
        tint,
        audio,
    } = completion;
    let (camera, camera_transform) = camera.into_inner();
    if matches!(check.stage, Stage::Done) {
        return;
    }
    if real.elapsed_secs() > 75.0 {
        error!(
            "Infantry capture timeout: last condition={} unpacked={} animated={} observed[clock, viewport, tint, audio]={:?} actor={:?} target={:?} target_owner={}",
            check.last_condition,
            check.unpacked,
            check.animated,
            check.observed,
            battle.simulation.entities[check.actor].position,
            battle.simulation.entities[check.target].position,
            battle.simulation.entities[check.target].owner
        );
        check.stage = Stage::Done;
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    if matches!(check.stage, Stage::Waiting) {
        clock.unpause();
        check.stage = Stage::Running;
    }
    let game = &battle.simulation;
    if !observe_phase(&mut check, battle) {
        return;
    }
    if let Err(error) = observe_animation(&mut check, battle, &visuals, &animations) {
        error!("Capture pose error: {error}");
        check.stage = Stage::Done;
        exit.write(AppExit::error());
        return;
    }
    let clock_state = command_clock(&check, &data, &icons, &buttons);
    let clock_visible = clock_state.is_some_and(|state| {
        state.x.to_bits() == 0.0_f32.to_bits() && (0.0..100.0).contains(&state.z)
    });
    let Some(viewport) = camera.logical_viewport_rect() else {
        return;
    };
    let projected = [check.actor, check.target].map(|id| {
        let point = crate::playtest_view::unit_position(game, id);
        camera.world_to_viewport(camera_transform, point).ok()
    });
    let in_view = projected
        .into_iter()
        .all(|point| point.is_some_and(|point| viewport.contains(point)));
    let seen = [
        clock_visible,
        in_view,
        tint.visible(check.target, battle.generation),
        audio.tick_playing(check.target),
    ];
    for (observed, current) in check.observed.iter_mut().zip(seen) {
        *observed |= current;
    }
    if game.tick >= check.last_report + 150 {
        eprintln!(
            "Capture presentation: frame={} unpacked={} animated={} clock={clock_state:?} projected={projected:?} current[clock, viewport, tint, audio]={seen:?} observed={:?}",
            game.tick, check.unpacked, check.animated, check.observed
        );
        check.last_report = game.tick;
    }
    if check.unpacked && check.animated && seen.into_iter().all(std::convert::identity) {
        check.stage = Stage::Done;
        clock.pause();
        capture.finish_scenario();
        println!(
            "Rendered infantry capture: original unpack/raising-flag animation with advancing authored frames, inverse recharge clock, target-only house-color material flash, advancing original tick audio and actor/target inside viewport passed"
        );
    }
}

fn observe_phase(check: &mut Check, battle: &Match) -> bool {
    let game = &battle.simulation;
    let actor = &game.entities[check.actor];
    let condition = actor.capture_condition().unwrap_or("inactive");
    if check.last_condition != condition {
        eprintln!(
            "Capture phase: frame={} condition={condition} actor={:?} target={:?}",
            game.tick, actor.position, game.entities[check.target].position
        );
        check.last_condition = condition;
    }
    if condition == "UNPACKING" {
        check.unpacked = true;
    }
    condition == "RAISING_FLAG"
}

fn command_clock(
    check: &Check,
    data: &UiData,
    icons: &Assets<PromotionIcon>,
    buttons: &Query<(&Action, &Node, &MaterialNode<PromotionIcon>)>,
) -> Option<Vec4> {
    let expected = data.icon(&check.image);
    buttons.iter().find_map(|(action, node, handle)| {
        if !matches!(action, Action::Slot(slot) if *slot == check.slot)
            || node.display == Display::None
        {
            return None;
        }
        icons
            .get(&handle.0)
            .filter(|icon| icon.image == expected.image && icon.uv_rect == expected.uv_rect)
            .map(|icon| icon.state)
    })
}

fn observe_animation(
    check: &mut Check,
    battle: &Match,
    visuals: &Query<(&UnitVisual, &Players)>,
    animations: &Animations,
) -> Result<(), generals_assets::w3d::Error> {
    let actor_id = check.actor;
    for (visual, players) in visuals {
        if visual.index != actor_id || visual.generation != battle.generation {
            continue;
        }
        let draws = animations.inspect(players, battle.simulation.entities[actor_id].type_index)?;
        if let Some(draw) = draws
            .iter()
            .find(|draw| !draw.clip.is_empty() && draw.bounds.is_some())
        {
            if check
                .previous_frame
                .is_some_and(|previous| (draw.frame - previous).abs() > 0.001)
            {
                check.animated = true;
            }
            check.previous_frame = Some(draw.frame);
        }
    }
    Ok(())
}
