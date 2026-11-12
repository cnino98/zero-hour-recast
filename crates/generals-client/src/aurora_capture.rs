//! Original paid Aurora parked, airborne and supersonic bombing capture.
use crate::{
    aurora_verification::{self, Setup},
    capture_framing::{self, AircraftFrame},
    input::{self, Mouse, PickTarget},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    sounds: [bool; 2],
    camera: Option<(Vec2, f32)>,
    moved: bool,
    origin: [f32; 2],
    supersonic: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = aurora_verification::prepare(&mut battle.simulation)?;
    let origin = battle.simulation.entities[setup.aircraft].position;
    battle.select_objects(&[setup.aircraft], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        sounds: [false; 2],
        camera: None,
        moved: false,
        origin,
        supersonic: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, (begin, audio).chain());
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
    if let Some((focus, distance)) = check.into_inner().camera {
        camera.focus = focus;
        camera.distance = distance;
    }
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    camera: ResMut<'w, CameraRig>,
    terrain: Res<'w, Terrain>,
    time: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    capture: Res<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    if control.real.elapsed_secs() > 120.0 {
        error!(
            "Aurora native capture timed out phase={} audio={:?}",
            check.phase, check.sounds
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    check.camera = None;
    let result = (|| {
        let id = check.setup.aircraft;
        if check.phase == 1 {
            aurora_verification::approach(&mut control.battle.simulation, id, check.setup.target)?;
            check.phase = 2;
        } else if check.phase == 3 {
            let point =
                Vec2::from_array(control.battle.simulation.entities[check.setup.target].position);
            require(
                input::context_order(
                    &mut control.battle,
                    Some(point),
                    Some(PickTarget::Unit(check.setup.target)),
                    &mut control.mouse,
                ),
                "native Aurora ordinary context Attack was rejected",
            )?;
            check.phase = 4;
            control.time.unpause();
        }
        let game = &control.battle.simulation;
        let actor = &game.entities[id];
        require(actor.alive(), "Aurora died before native bombing capture")?;
        check.moved |= (actor.position[0] - check.origin[0])
            .hypot(actor.position[1] - check.origin[1])
            > 100.0;
        if let Some(motion) = actor.aircraft_draw_motion(&game.catalog[actor.type_index], game.tick)
        {
            let attack = game.catalog[actor.type_index]
                .aircraft
                .as_ref()
                .and_then(|rules| rules.attacking.as_ref())
                .and_then(|set| set.locomotor.flight.as_ref())
                .ok_or("Aurora lost original supersonic locomotor")?;
            check.supersonic |= std::ptr::eq(motion.rules, &raw const attack.draw);
        }
        let mut focus = game.object_position(id);
        if let Some((position, _)) = game
            .projectiles
            .iter()
            .find(|bomb| bomb.source == id)
            .and_then(|projectile| projectile.render_transform(&game.entities))
        {
            focus = std::array::from_fn(|axis| (focus[axis] + position[axis]) * 0.5);
        }
        capture_framing::focus_on(&mut control.camera, &control.terrain, focus);
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Aurora native phase {}: {error}", check.phase);
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}

fn audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    let id = check.setup.aircraft;
    for (heard, played) in check.sounds.iter_mut().zip(audio.played([
        (id, "AuroraBomberVoiceAttack"),
        (id, "AuroraBomberAmbientLoop"),
    ])) {
        *heard |= played;
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    framing: AircraftFrame<'w, 's>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    camera: Res<'w, CameraRig>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if check.phase == 255 || !view.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let id = check.setup.aircraft;
        if !view.framing.visible(&view.battle, id)? {
            return Ok(());
        }
        if check.phase == 0 {
            check.camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("aurora-parked.png");
            check.phase = 1;
        } else if check.phase == 2 && check.moved {
            check.camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("aurora-airborne.png");
            check.phase = 3;
        } else if check.phase == 4
            && check.supersonic
            && check.sounds.into_iter().all(|heard| heard)
        {
            for (visual, children) in &view.projectiles {
                if visual.generation != view.battle.generation
                    || !view
                        .battle
                        .simulation
                        .projectiles
                        .iter()
                        .any(|bomb| bomb.id == visual.id && bomb.source == id)
                {
                    continue;
                }
                if view
                    .framing
                    .separated_projectile(&view.battle, id, children)?
                {
                    view.time.pause();
                    check.camera = Some((view.camera.focus, view.camera.distance));
                    view.capture.finish_scenario();
                    check.phase = 255;
                    println!(
                        "Original Aurora native capture PASSED: paid parked/airborne geometry, actual movement, observed supersonic sortie, visibly separated original falling bomb mesh above HUD and advancing attack/engine audio"
                    );
                    break;
                }
            }
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Aurora native render phase {}: {error}", check.phase);
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}
