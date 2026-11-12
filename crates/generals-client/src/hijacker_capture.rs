//! Native paid Hijacker input, original meshes and the live hidden-driver conversion.
use crate::{
    capture_framing::AircraftFrame,
    hijacker_verification::{self, Setup},
    input::{self, Mouse, PickTarget},
    model_animation::AnimatedPart,
    playtest::{CameraRig, Match},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::Order;
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    began: Option<f64>,
    host_health: u32,
    lost: u32,
    heard: bool,
    captured_camera: Option<(Vec2, f32)>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = hijacker_verification::prepare(&mut battle.simulation)?;
    // The paid destruction attacker belongs to the headless lifecycle check.
    // Move it back to its own command center before observing capture health.
    let command = crate::skirmish_verification::friendly_entity(
        &battle.simulation,
        generals_sim::skirmish::Kind::CommandCenter,
        1,
    )?;
    let point = battle.simulation.entities[command].position;
    battle.simulation.move_to(setup.attacker, point)?;
    crate::skirmish_verification::advance_until(
        &mut battle.simulation,
        18_000,
        "paid attacker returning before native hijack",
        |game| {
            let position = game.entities[setup.attacker].position;
            (position[0] - point[0]).hypot(position[1] - point[1]) < 150.0
        },
    )?;
    battle.simulation.stop(setup.attacker)?;
    let host_health = battle.simulation.entities[setup.host].health.to_bits();
    let lost = battle.simulation.players[0].statistics.units_lost;
    battle.select_objects(&[setup.actor], true);
    battle.observer_player = None;
    app.insert_resource(Check {
        setup,
        phase: 0,
        began: None,
        host_health,
        lost,
        heard: false,
        captured_camera: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let battle = app.world().resource::<Match>();
    let focus = (Vec2::from_array(battle.simulation.entities[check.setup.actor].position)
        + Vec2::from_array(battle.simulation.entities[check.setup.host].position))
        * 0.05;
    let captured_camera = {
        let mut camera = app.world_mut().resource_mut::<CameraRig>();
        camera.focus = focus;
        camera.distance = camera.settings.min_height;
        (camera.focus, camera.distance)
    };
    app.world_mut().resource_mut::<Check>().captured_camera = Some(captured_camera);
    app.add_systems(
        Update,
        hold_camera
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

fn hold_camera(check: Res<Check>, mut camera: ResMut<CameraRig>) {
    let check = check.into_inner();
    if let Some((focus, distance)) = check.captured_camera {
        // Preserve the trial's checked view before its first gate and throughout
        // asynchronous screenshot readback; the capture process exits on completion.
        camera.focus = focus;
        camera.distance = distance;
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    roots: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    parts: Query<'w, 's, &'static ViewVisibility, With<AnimatedPart>>,
    audio: FiringAudioInspection<'w, 's>,
    framing: AircraftFrame<'w, 's>,
    camera: Res<'w, CameraRig>,
}

fn visible(view: &View, battle: &Match, actor: usize) -> bool {
    view.roots.iter().any(|(visual, children)| {
        visual.index == actor
            && visual.generation == battle.generation
            && children.iter().any(|child| {
                view.parts
                    .get(child)
                    .is_ok_and(|visibility| visibility.get())
            })
    })
}

fn inspect(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    view: View,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == 3 {
        return;
    }
    let now = view.clock.elapsed_secs_f64();
    let began = *check.began.get_or_insert(now);
    if now - began >= 60.0 {
        error!(
            "Hijacker native acceptance timed out at phase {} frame {} actor_visible={} actor_framed={:?} host_framed={:?} camera_focus={:?} camera_distance={}",
            check.phase,
            battle.simulation.tick,
            visible(&view, &battle, check.setup.actor),
            view.framing.visible(&battle, check.setup.actor),
            view.framing.visible(&battle, check.setup.host),
            view.camera.focus,
            view.camera.distance,
        );
        check.phase = 3;
        exit.write(AppExit::error());
        return;
    }
    let actor_visible = visible(&view, &battle, check.setup.actor);
    check.heard |= view.audio.played([(check.setup.actor, "HijackDriver")])[0];
    if !capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let actor_framed = check.phase == 0 && view.framing.visible(&battle, check.setup.actor)?;
        let host_framed = view.framing.visible(&battle, check.setup.host)?;
        match check.phase {
            0 if actor_framed && host_framed => {
                check.captured_camera = Some((view.camera.focus, view.camera.distance));
                capture.intermediate("hijacker-before-command.png");
                check.phase = 1;
            }
            1 => {
                require(
                    input::context_order(
                        &mut battle,
                        None,
                        Some(PickTarget::Unit(check.setup.host)),
                        &mut mouse,
                    ),
                    "Native Hijacker context input was rejected",
                )?;
                require(
                    matches!(battle.simulation.entities[check.setup.actor].ai.order, Order::Enter(target) if target == check.setup.host),
                    "Native hijack input did not issue original Enter",
                )?;
                time.unpause();
                check.phase = 2;
            }
            2 if battle.simulation.entities[check.setup.host].hijacked()
                && host_framed
                && !actor_visible
                && check.heard =>
            {
                let game = &battle.simulation;
                require(
                    game.entities[check.setup.host].owner == 0
                        && game.entities[check.setup.host].health.to_bits() == check.host_health,
                    "Native hijacking lost owner or unchanged-health contract",
                )?;
                require(
                    game.entities[check.setup.actor].alive()
                        && game.entities[check.setup.actor].driving_vehicle()
                            == Some(check.setup.host)
                        && game.players[0].statistics.units_lost == check.lost,
                    "Native host failed stored living driver/no-death accounting",
                )?;
                time.pause();
                battle.select_objects(&[check.setup.host], true);
                check.captured_camera = Some((view.camera.focus, view.camera.distance));
                check.phase = 3;
                capture.finish_scenario();
                println!(
                    "HIJACKER_RENDERED original_meshes=true native_context_enter=true host_visible=true stored_driver_hidden_alive=true unchanged_health=true no_driver_loss=true original_hijack_audio_advancing=true"
                );
            }
            _ => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Hijacker native acceptance: {error}");
        check.phase = 3;
        exit.write(AppExit::error());
    }
}
