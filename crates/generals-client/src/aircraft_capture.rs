//! Paid aircraft controls, missiles, source exhaust, sound and rocket-pod barrage.
pub mod countermeasures;
mod rocket_pods;
use crate::{
    aircraft_verification,
    capture_framing::{self, AircraftFrame},
    input::{self, Mouse, PickTarget},
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::{Capture, UnitVisual},
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
    terrain::Terrain,
};
use bevy::prelude::*;
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: aircraft_verification::Setup,
    stage: Stage,
    began: u64,
    real_began: Option<f64>,
    sounds: [bool; 3],
    shots: [usize; 2],
    seen: [bool; 4],
    reported: u64,
    diagnostic_saved: [bool; 2],
    diagnostic_paused: bool,
    original_view: Option<(f32, f32)>,
}

enum Stage {
    Waiting,
    Raptor,
    RaptorCaptured,
    Approach,
    Comanche,
    RocketPods,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = aircraft_verification::prepare(&mut battle.simulation)?;
    aircraft_verification::research_rocket_pods(&mut battle.simulation, &setup)?;
    aircraft_verification::approach(&mut battle.simulation, setup.raptor, setup.target)?;
    battle.select_objects(&[setup.raptor], true);
    app.insert_resource(Check {
        began: battle.simulation.tick,
        setup,
        stage: Stage::Waiting,
        real_began: None,
        sounds: [false; 3],
        shots: [0; 2],
        seen: [false; 4],
        reported: 0,
        diagnostic_saved: [false; 2],
        diagnostic_paused: false,
        original_view: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    rocket_pods::register(app);
    app.add_systems(PreUpdate, (begin, inspect_audio).chain());
    app.add_systems(FixedUpdate, record_shots.after(crate::laser_beam::record));
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    camera: ResMut<'w, CameraRig>,
    time: ResMut<'w, Time<Virtual>>,
    capture: Res<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
    terrain: Res<'w, Terrain>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if matches!(check.stage, Stage::Done) || !control.capture.ready_for_scenario() {
        return;
    }
    let actor = if matches!(
        check.stage,
        Stage::Waiting | Stage::Raptor | Stage::RaptorCaptured
    ) {
        check.setup.raptor
    } else {
        check.setup.comanche
    };
    let game = &control.battle.simulation;
    let from = Vec2::from_array(game.entities[actor].position);
    let to = Vec2::from_array(game.entities[check.setup.target].position);
    let (yaw, distance) = *check
        .original_view
        .get_or_insert((control.camera.settings.yaw, control.camera.distance));
    if matches!(check.stage, Stage::Raptor | Stage::Comanche) {
        let mut focus = game.object_position(actor);
        // View across the shot's horizontal direction so the carrier does not
        // conceal the missile during the first visible frames.
        control.camera.settings.yaw = (to.y - from.y).atan2(to.x - from.x);
        if let Some((position, _)) = game
            .projectiles
            .iter()
            .find(|projectile| projectile.source == actor)
            .and_then(|projectile| projectile.render_transform(&game.entities))
        {
            focus = std::array::from_fn(|axis| (focus[axis] + position[axis]) * 0.5);
        }
        control.camera.distance =
            (control.camera.settings.min_height * 2.0).min(control.camera.settings.max_height);
        capture_framing::focus_on(&mut control.camera, &control.terrain, focus);
    } else {
        control.camera.settings.yaw = yaw;
        control.camera.distance = distance;
        control.camera.focus = (from + to) * 0.05;
    }
    if check.diagnostic_paused {
        check.diagnostic_paused = false;
        control.time.unpause();
    }
    let result = (|| {
        if matches!(check.stage, Stage::RaptorCaptured) {
            control.battle.simulation.stop(check.setup.raptor)?;
            control.battle.select_objects(&[check.setup.comanche], true);
            let point = [to.x + 100.0, to.y - 160.0];
            control
                .battle
                .simulation
                .move_to(check.setup.comanche, point)?;
            check.stage = Stage::Approach;
            check.sounds = [false; 3];
            check.seen = [false; 4];
            check.began = control.battle.simulation.tick;
            control.time.unpause();
        }
        let ready = matches!(check.stage, Stage::Waiting)
            || (matches!(check.stage, Stage::Approach)
                && from.distance(to) < 220.0
                && control
                    .battle
                    .simulation
                    .logically_visible(0, check.setup.target));
        if ready {
            require(
                input::context_order(
                    &mut control.battle,
                    Some(to),
                    Some(PickTarget::Unit(check.setup.target)),
                    &mut control.mouse,
                ),
                "original aircraft attack input rejected",
            )?;
            require(
                control
                    .mouse
                    .voices
                    .last()
                    .is_some_and(|request| matches!(request.voice(), Voice::Attack)),
                "aircraft attack did not request original voice",
            )?;
            check.stage = if matches!(check.stage, Stage::Waiting) {
                Stage::Raptor
            } else {
                Stage::Comanche
            };
            check.began = control.battle.simulation.tick;
            control.time.unpause();
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.stage = Stage::Done;
        error!("Native aircraft input: {error}");
        control.exit.write(AppExit::error());
    }
}

fn record_shots(mut check: ResMut<Check>, battle: Res<Match>) {
    let battle = battle.into_inner();
    for (index, actor) in [check.setup.raptor, check.setup.comanche]
        .into_iter()
        .enumerate()
    {
        check.shots[index] += battle
            .simulation
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == actor)
            .count();
    }
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    let events = match check.stage {
        Stage::Raptor => [
            (check.setup.raptor, "RaptorVoiceAttack"),
            (check.setup.raptor, "RaptorJetMissileWeapon"),
            (check.setup.raptor, "RaptorAmbientLoop"),
        ],
        Stage::Comanche => [
            (check.setup.comanche, "ComancheVoiceAttack"),
            (check.setup.comanche, "ComancheMissileWeapon"),
            (check.setup.comanche, "ComancheAmbientLoop"),
        ],
        _ => return,
    };
    for (heard, played) in check.sounds.iter_mut().zip(audio.played(events)) {
        *heard |= played;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    actors: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<'w, 's, (&'static AnimatedPart, &'static ViewVisibility)>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    visible: Query<'w, 's, &'static ViewVisibility>,
    particles: ParticleInspection<'w, 's>,
    framing: AircraftFrame<'w, 's>,
}

impl View<'_, '_> {
    fn observe(
        self,
        battle: &Match,
        actor: usize,
        target: usize,
    ) -> Result<[bool; 4], Box<dyn Error>> {
        let Self {
            actors,
            meshes,
            projectiles,
            visible,
            particles,
            framing,
        } = self;
        let body = [actor, target].map(|id| {
            actors.iter().any(|(visual, children)| {
                visual.index == id
                    && visual.generation == battle.generation
                    && children.iter().any(|child| {
                        meshes
                            .get(child)
                            .is_ok_and(|(_, visibility)| visibility.get())
                    })
            })
        });
        let missile = missile(battle, actor, &projectiles, &visible, &particles, &framing)?;
        Ok([body[0], body[1], missile[0], missile[1]])
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w> {
    battle: Res<'w, Match>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    camera: Res<'w, CameraRig>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, clock: Res<Time<Real>>, completion: Completion) {
    let clock = clock.into_inner();
    let Completion {
        battle,
        mut time,
        mut capture,
        camera,
        mut exit,
    } = completion;
    let battle = battle.into_inner();
    if matches!(check.stage, Stage::Done) {
        return;
    }
    let started = *check
        .real_began
        .get_or_insert_with(|| clock.elapsed_secs_f64());
    if clock.elapsed_secs_f64() - started > 120.0 {
        error!(
            "Native aircraft exceeded 120 real seconds; {}",
            diagnostic(&check, battle, camera.focus, capture.ready_for_scenario())
        );
        check.stage = Stage::Done;
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() || !matches!(check.stage, Stage::Raptor | Stage::Comanche) {
        return;
    }
    let actor = if matches!(check.stage, Stage::Raptor) {
        check.setup.raptor
    } else {
        check.setup.comanche
    };
    let result = (|| {
        require(
            battle.simulation.entities[actor].alive(),
            "native aircraft died before missile capture",
        )?;
        let current = view.observe(battle, actor, check.setup.target)?;
        for (seen, visible) in check.seen.iter_mut().zip(current) {
            *seen |= visible;
        }
        if battle.simulation.tick.saturating_sub(check.reported) >= 300 {
            check.reported = battle.simulation.tick;
            println!(
                "Native aircraft probe: current={current:?}; {}",
                diagnostic(&check, battle, camera.focus, true)
            );
        }
        if current.into_iter().all(|visible| visible) && check.sounds.iter().all(|heard| *heard) {
            time.pause();
            if matches!(check.stage, Stage::Raptor) {
                check.stage = Stage::RaptorCaptured;
                capture.intermediate("aircraft-raptor.png");
                println!(
                    "Original Raptor capture: paid aircraft, live body/target, original missile/exhaust and advancing attack/fire/engine audio passed"
                );
            } else {
                check.stage = Stage::RocketPods;
                capture.intermediate("aircraft-comanche.png");
                println!(
                    "Original Comanche capture: paid aircraft, live body/target, original missile/exhaust and advancing attack/fire/engine audio passed"
                );
            }
        } else if current[..3].iter().all(|visible| *visible) {
            let stage = usize::from(matches!(check.stage, Stage::Comanche));
            if !check.diagnostic_saved[stage] {
                check.diagnostic_saved[stage] = true;
                check.diagnostic_paused = true;
                time.pause();
                capture.intermediate(if stage == 0 {
                    "aircraft-raptor-diagnostic.png"
                } else {
                    "aircraft-comanche-diagnostic.png"
                });
                println!(
                    "Aircraft diagnostic image requested; current={current:?}; {}",
                    diagnostic(&check, battle, camera.focus, true)
                );
            }
        }
        require(
            battle.simulation.tick - check.began < 3600,
            "native aircraft missile or audio capture timed out",
        )
    })();
    if let Err(error) = result {
        error!(
            "Native aircraft: {error}; {}",
            diagnostic(&check, battle, camera.focus, capture.ready_for_scenario())
        );
        check.stage = Stage::Done;
        exit.write(AppExit::error());
    }
}

fn missile(
    battle: &Match,
    actor: usize,
    projectiles: &Query<(&ProjectileVisual, &Children)>,
    visible: &Query<&ViewVisibility>,
    particles: &ParticleInspection,
    framing: &AircraftFrame,
) -> Result<[bool; 2], Box<dyn Error>> {
    let mut body = false;
    for (visual, children) in projectiles {
        if visual.generation != battle.generation
            || !battle
                .simulation
                .projectiles
                .iter()
                .any(|missile| missile.id == visual.id && missile.source == actor)
            || !children
                .iter()
                .any(|child| visible.get(child).is_ok_and(|visibility| visibility.get()))
            || framing.isolated_projectile(battle, children)?.is_none()
        {
            continue;
        }
        body = true;
        if particles.missile(visual.id, battle.generation)? {
            return Ok([true, true]);
        }
    }
    Ok([body, false])
}

fn diagnostic(check: &Check, battle: &Match, focus: Vec2, ready: bool) -> String {
    let stage = match check.stage {
        Stage::Waiting => "waiting",
        Stage::Raptor => "raptor",
        Stage::RaptorCaptured => "raptor captured",
        Stage::Approach => "comanche approach",
        Stage::Comanche => "comanche",
        Stage::RocketPods => "rocket pods",
        Stage::Done => "done",
    };
    let game = &battle.simulation;
    format!(
        "stage={stage} ready={ready} audio={:?} shots={:?} seen_actor_target_missile_exhaust={:?} focus={focus:?}; {}; {}; Raptor weapon: {}; Comanche weapon: {}; target: {}",
        check.sounds,
        check.shots,
        check.seen,
        aircraft_verification::details(game, check.setup.raptor),
        aircraft_verification::details(game, check.setup.comanche),
        generals_sim::skirmish::diagnostics::weapon_details(
            game,
            check.setup.raptor,
            check.setup.target
        ),
        generals_sim::skirmish::diagnostics::weapon_details(
            game,
            check.setup.comanche,
            check.setup.target
        ),
        generals_sim::skirmish::diagnostics::movement_details(game, check.setup.target)
    )
}
