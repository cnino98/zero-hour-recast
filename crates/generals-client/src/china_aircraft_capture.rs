//! Paid China hangars, original `MiG` attack input and visible Black Napalm storm.
pub mod crash;
use crate::{
    capture_framing::{self, AircraftFrame},
    china_aircraft_verification::{self, Setup},
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
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    ready: [bool; 4],
    shots: [usize; 4],
    sounds: [bool; 4],
    began: u64,
    captured_camera: Option<(Vec2, f32)>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = china_aircraft_verification::prepare(&mut battle.simulation)?;
    china_aircraft_verification::upgrade(&mut battle.simulation, &setup)?;
    china_aircraft_verification::scout(&mut battle.simulation, &setup)?;
    battle.select_objects(&[setup.airfield], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        ready: [false; 4],
        shots: [0; 4],
        sounds: [false; 4],
        began: battle.simulation.tick,
        captured_camera: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, (begin, audio, storm_audio).chain());
    app.add_systems(FixedUpdate, record.after(crate::laser_beam::record));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    camera: ResMut<'w, CameraRig>,
    terrain: Res<'w, Terrain>,
    time: ResMut<'w, Time<Virtual>>,
    capture: Res<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if let Some((focus, distance)) = check.captured_camera {
        // GPU readback may span multiple frames or retry a missing drawable.
        // Keep the checked camera until Capture acknowledges the saved image.
        if !control.capture.ready_for_scenario() {
            control.camera.focus = focus;
            control.camera.distance = distance;
            return;
        }
        check.captured_camera = None;
    }
    if check.phase == 255 || !control.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        if check.phase == 0 {
            control.camera.focus =
                Vec2::from_array(control.battle.simulation.entities[check.setup.airfield].position)
                    * 0.1;
        } else if check.phase == 1 {
            control.battle.select_objects(&check.setup.migs, true);
            for (&id, offset) in check.setup.migs.iter().zip([-45.0, -15.0, 15.0, 45.0]) {
                control.battle.simulation.move_to(
                    id,
                    [check.setup.assembly[0] + offset, check.setup.assembly[1]],
                )?;
            }
            check.phase = 2;
            control.time.unpause();
        } else if check.phase == 2 {
            stage_formation(&mut check, &control.battle)?;
            if check.ready.into_iter().all(|ready| ready)
                && control
                    .battle
                    .simulation
                    .logically_visible(0, check.setup.target)
            {
                let point = Vec2::from_array(
                    control.battle.simulation.entities[check.setup.target].position,
                );
                require(
                    input::context_order(
                        &mut control.battle,
                        Some(point),
                        Some(PickTarget::Unit(check.setup.target)),
                        &mut control.mouse,
                    ),
                    "original MiG group attack input rejected",
                )?;
                require(
                    control
                        .mouse
                        .voices
                        .last()
                        .is_some_and(|request| matches!(request.voice(), Voice::Attack)),
                    "MiG attack input omitted original voice",
                )?;
                check.phase = 3;
                check.began = control.battle.simulation.tick;
            }
        } else if check.phase == 4 {
            check.phase = 5;
            control.time.unpause();
        }
        if check.phase >= 2 {
            let game = &control.battle.simulation;
            let from = game.object_position(check.setup.migs[0]);
            let target = game.object_position(check.setup.target);
            let focus = if check.phase >= 4 {
                target
            } else {
                std::array::from_fn(|axis| (from[axis] + target[axis]) * 0.5)
            };
            capture_framing::focus_on(&mut control.camera, &control.terrain, focus);
        }
        require(
            control.battle.simulation.tick - check.began < 7200,
            "native China aircraft timed out",
        )
    })();
    if let Err(error) = result {
        error!(
            "Native China aircraft input phase {}: {error}; {}",
            check.phase,
            china_aircraft_verification::details(&control.battle.simulation, &check.setup)
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}

fn stage_formation(check: &mut Check, battle: &Match) -> Result<(), Box<dyn Error>> {
    let game = &battle.simulation;
    let point = check.setup.assembly;
    for (index, &id) in check.setup.migs.iter().enumerate() {
        let actor = &game.entities[id];
        check.ready[index] = actor.aircraft_phase() == Some("idle")
            && game.height_above_ground(id) > 10.0
            && (actor.position[0] - point[0]).hypot(actor.position[1] - point[1]) < 200.0;
    }
    require(
        check
            .setup
            .migs
            .iter()
            .all(|&id| game.entities[id].weapon_rounds_remaining(0) == 2),
        "native MiG staging consumed missiles before attack",
    )
}

fn audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if check.phase < 3 || check.phase == 255 {
        return;
    }
    let actor = check.setup.migs[0];
    for (heard, played) in check.sounds[..3].iter_mut().zip(audio.played([
        (actor, "MigVoiceAttack"),
        (actor, "MigJetNapalmWeapon"),
        (actor, "MigAmbientLoop"),
    ])) {
        *heard |= played;
    }
}

fn storm_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if check.phase >= 3 && check.phase != 255 {
        check.sounds[3] |= audio.ambient(["FireStormLoop"])[0];
    }
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    let battle = battle.into_inner();
    if check.phase < 3 || check.phase == 255 {
        return;
    }
    let ids = check.setup.migs;
    for (count, actor) in check.shots.iter_mut().zip(ids) {
        *count += battle
            .simulation
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == actor)
            .count();
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    actors: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<'w, 's, (&'static AnimatedPart, &'static ViewVisibility)>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    visible: Query<'w, 's, &'static ViewVisibility>,
    particles: ParticleInspection<'w, 's>,
    framing: AircraftFrame<'w, 's>,
    camera: Res<'w, CameraRig>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

impl View<'_, '_> {
    fn airborne_body(&self, ids: &[usize; 4]) -> Result<bool, Box<dyn Error>> {
        for &id in ids {
            if self.battle.simulation.height_above_ground(id) > 10.0
                && self.framing.visible(&self.battle, id)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn body(&self, id: usize) -> bool {
        self.actors.iter().any(|(visual, children)| {
            visual.index == id
                && visual.generation == self.battle.generation
                && children.iter().any(|child| {
                    self.meshes
                        .get(child)
                        .is_ok_and(|(_, visible)| visible.get())
                })
        })
    }

    fn contrail(&self, ids: &[usize; 4]) -> Result<bool, Box<dyn Error>> {
        for &id in ids {
            if self
                .particles
                .drawable_state(id, "JetContrail", self.battle.generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn missile(&self, ids: &[usize; 4]) -> Result<bool, Box<dyn Error>> {
        for (visual, children) in &self.projectiles {
            if visual.generation == self.battle.generation
                && self
                    .battle
                    .simulation
                    .projectiles
                    .iter()
                    .any(|missile| missile.id == visual.id && ids.contains(&missile.source))
                && children
                    .iter()
                    .any(|child| self.visible.get(child).is_ok_and(|visible| visible.get()))
                && self.particles.missile(visual.id, self.battle.generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if !view.capture.ready_for_scenario() || check.phase == 255 {
        return;
    }
    let result = (|| {
        if check.phase == 0
            && view.body(check.setup.airfield)
            && check.setup.migs.iter().all(|&id| view.body(id))
        {
            check.captured_camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("china-airfield.png");
            check.phase = 1;
            println!(
                "Original China Airfield capture: original hangar structure, four paid parked MiGs and researched armor; roofs may obscure parked bodies"
            );
        } else if check.phase == 3
            && view.body(check.setup.target)
            && view.airborne_body(&check.setup.migs)?
            && view.missile(&check.setup.migs)?
            && view.contrail(&check.setup.migs)?
            && check.sounds[..3].iter().all(|heard| *heard)
        {
            view.time.pause();
            check.captured_camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("china-mig.png");
            check.phase = 4;
            println!(
                "Original MiG capture: real attack input, original aircraft/wingtip contrail/missile/exhaust and advancing voice/fire/engine audio passed"
            );
        } else if check.phase == 5
            && check.shots == [2; 4]
            && check.sounds[3]
            && view.body(check.setup.target)
            && view
                .particles
                .incendiary("FireStormMainUpgraded", view.battle.generation)?
        {
            view.time.pause();
            check.captured_camera = Some((view.camera.focus, view.camera.distance));
            view.capture.finish_scenario();
            check.phase = 255;
            println!(
                "Original China aircraft capture PASSED: four paid two-missile MiG clips, researched Black Napalm and visible shared eight-impact firestorm with original advancing audio"
            );
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!(
            "Native China aircraft render phase {}: {error}",
            check.phase
        );
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}
