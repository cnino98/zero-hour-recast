//! Original paid Helix attachment, physical bomb and firestorm capture.
use crate::{
    capture_framing::{self, AircraftFrame},
    control_bar::CommandAction,
    helix_verification::{self, Setup},
    input::{self, CommandMode, Mouse},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::dispatch_command,
    playtest_view::Capture,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
    strategy_verification::template,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    bomb: Option<usize>,
    hull: Option<usize>,
    sounds: [bool; 4],
    captured_camera: Option<(Vec2, f32)>,
    pose_wait: Option<(u8, f64)>,
    crash_started: Option<u64>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = helix_verification::prepare(&mut battle.simulation)?;
    helix_verification::arm_bomb(&mut battle.simulation, setup.helicopters[1])?;
    battle.select_objects(&[setup.helicopters[0]], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        bomb: None,
        hull: None,
        sounds: [false; 4],
        captured_camera: None,
        pose_wait: None,
        crash_started: None,
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
    let check = check.into_inner();
    if let Some((focus, distance)) = check.captured_camera {
        // Camera controls change only focus and distance. Restore the inspected
        // view after input, before its transform is rebuilt for asynchronous readback.
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
    if control.real.elapsed_secs() > 150.0 {
        error!(
            "Helix capture timed out phase={} bomb={:?} audio={:?}",
            check.phase, check.bomb, check.sounds
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if check.captured_camera.is_some() {
        if !control.capture.ready_for_scenario() {
            return;
        }
        check.captured_camera = None;
        if check.phase == 254 {
            check.phase = 255;
            control.exit.write(AppExit::error());
            return;
        }
    }
    if check.phase == 255 || !control.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let id = check.setup.helicopters[1];
        if check.phase == 1 {
            control.battle.select_objects(&[id], true);
            dispatch_command(
                &mut control.battle,
                &mut control.mouse,
                CommandAction::HelixBomb,
            );
            require(
                matches!(control.mouse.pending_command, CommandMode::HelixBomb),
                "original Helix bomb button did not enter target mode",
            )?;
            let target = separated_bomb_target(&control.battle.simulation, &check.setup)?;
            input::targeted_order(&mut control.battle, &mut control.mouse, target, None);
            require(
                matches!(control.mouse.pending_command, CommandMode::Normal),
                "original Helix ground targeting did not accept its command",
            )?;
            check.phase = 2;
            control.time.unpause();
        } else if check.phase == 2 {
            if let Some(bomb) = control.battle.simulation.entities[id].helix_bomb_object() {
                let game = &control.battle.simulation;
                if game.entities[bomb].alive()
                    && game.object_position(id)[2] - game.object_position(bomb)[2] > 15.0
                {
                    // Frame the actual fall from the side. A height difference alone
                    // can still leave the bomb behind the aircraft's fuselage.
                    control.camera.settings.yaw = game.entities[id].facing;
                    control.camera.distance = (control.camera.settings.min_height
                        + control.camera.settings.max_height)
                        * 0.5;
                    check.bomb = Some(bomb);
                    check.phase = 3;
                }
            }
        } else if check.phase == 4 {
            control.camera.distance = control.camera.settings.max_height;
            dispatch_command(&mut control.battle, &mut control.mouse, CommandAction::Stop);
            check.phase = 5;
            control.time.unpause();
        }
        advance_crash(&mut check, &mut control)?;
        let game = &control.battle.simulation;
        let focus = if check.phase >= 6 {
            game.object_position(check.hull.unwrap_or(check.setup.helicopters[0]))
        } else if check.phase == 0 {
            std::array::from_fn(|axis| {
                check
                    .setup
                    .helicopters
                    .iter()
                    .map(|&id| game.object_position(id)[axis])
                    .sum::<f32>()
                    / 3.0
            })
        } else if let Some(bomb) = check.bomb {
            if check.phase >= 4 {
                game.object_position(bomb)
            } else {
                std::array::from_fn(|axis| {
                    (game.object_position(id)[axis] + game.object_position(bomb)[axis]) * 0.5
                })
            }
        } else {
            game.object_position(id)
        };
        capture_framing::focus_on(&mut control.camera, &control.terrain, focus);
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Helix native command phase {}: {error}", check.phase);
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}

fn separated_bomb_target(
    game: &generals_sim::skirmish::Skirmish,
    setup: &Setup,
) -> Result<Vec2, Box<dyn Error>> {
    let id = setup.helicopters[1];
    let actor = &game.entities[id];
    let point = actor.position;
    let interior = game
        .visibility_dimensions()
        .map(|cells| f32::from(cells.saturating_sub(1)) * game.visibility_cell_size());
    let factory = template(game, "ChinaWarFactory")?;
    let storm = template(game, "BlackNapalmFirestormSmall")?;
    let radius = game.catalog[storm]
        .fire
        .as_ref()
        .unwrap()
        .storm
        .as_ref()
        .unwrap()
        .geometry
        .final_size[1];
    let mut rejected = Vec::new();
    // Try perpendicular and then diagonal approaches, without moving along
    // the parked formation. Every proposal receives the full field checks.
    for distance in [100.0, 150.0, 200.0] {
        for direction in [
            [0.0_f32, 1.0],
            [0.0, -1.0],
            [-1.0, 1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, -1.0],
        ] {
            let target = [
                direction[0].mul_add(distance, point[0]),
                direction[1].mul_add(distance, point[1]),
            ];
            let inside = target
                .into_iter()
                .zip(interior)
                .all(|(coordinate, limit)| coordinate >= 0.0 && coordinate < limit);
            let terrain = inside && game.can_place(factory, target);
            let clear = helix_verification::clear_bombing_point(game, target, radius);
            let separated = setup
                .helicopters
                .iter()
                .filter(|&&other| other != id)
                .all(|&other| {
                    let parked = &game.entities[other];
                    // Reserve two authored bounding spheres per aircraft as a
                    // capture margin, instead of accepting near-coincident bodies.
                    let spacing = 2.0
                        * (game.catalog[actor.type_index].geometry_bounds.sphere_radius
                            + game.catalog[parked.type_index]
                                .geometry_bounds
                                .sphere_radius);
                    let dx = target[0] - parked.position[0];
                    let dy = target[1] - parked.position[1];
                    dy.mul_add(dy, dx * dx) > spacing * spacing
                });
            if terrain && clear && separated {
                println!("  Native Helix separated bombing target {target:?} from {point:?}");
                return Ok(Vec2::from_array(target));
            }
            rejected.push(format!("{target:?}: interior={inside} terrain={terrain} clearance={clear} separated={separated}"));
        }
    }
    Err(format!(
        "no legal separated Helix bombing approach from {point:?}, interior={interior:?}, storm radius={radius}: {}",
        rejected.join("; ")
    ).into())
}

fn audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if check.phase == 0 {
        check.sounds[0] |= audio.played([(check.setup.helicopters[0], "HelixAmbientLoop")])[0];
    } else if (1..=3).contains(&check.phase) {
        check.sounds[1] |= audio.played([(check.setup.helicopters[1], "HelixVoiceAttack")])[0];
    } else if check.phase >= 6 {
        check.sounds[3] |= audio.played([(check.setup.helicopters[0], "HelixDamagedLoop")])[0];
    } else if check.phase >= 4 {
        check.sounds[2] |= audio.ambient(["FireStormLoop"])[0];
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    framing: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    camera: Res<'w, CameraRig>,
    real: Res<'w, Time<Real>>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if check.phase >= 254 || !view.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        if check.phase == 0 && check.sounds[0] {
            for id in check.setup.helicopters {
                let child = view.battle.simulation.entities[id]
                    .attachment_child()
                    .ok_or("missing original Helix attachment")?;
                if !view.framing.visible(&view.battle, id)?
                    || !view.framing.visible(&view.battle, child)?
                {
                    return Ok(());
                }
            }
            check.captured_camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("helix-attachments.png");
            check.phase = 1;
            println!(
                "Original Helix attachments capture: three paid aircraft, Bunker/Gattling/Speaker dependent bodies fully inside the uncovered viewport; five hidden infantry and advancing engine audio"
            );
        } else if check.phase == 3 && check.sounds[1] {
            let bomb = check.bomb.ok_or("missing released Helix bomb")?;
            let helicopter = check.setup.helicopters[1];
            require(
                view.battle.simulation.entities[bomb].alive(),
                "Helix bomb hit the ground before its falling body cleared the fuselage in view",
            )?;
            if view.framing.visible(&view.battle, helicopter)?
                && view
                    .framing
                    .separated_actor(&view.battle, helicopter, bomb)?
                && view.framing.physical_pose(&view.battle, bomb)?
            {
                view.time.pause();
                check.captured_camera = Some((view.camera.focus, view.camera.distance));
                view.capture.intermediate("helix-bomb.png");
                check.phase = 4;
                println!(
                    "Original Helix napalm capture: real command targeting, visible upgraded aircraft and falling physical bomb, original advancing attack audio"
                );
            }
        } else if check.phase == 5
            && check.sounds[2]
            && check
                .bomb
                .is_some_and(|id| !view.battle.simulation.entities[id].alive())
            && view
                .particles
                .incendiary("FireStormMain", view.battle.generation)?
        {
            view.time.pause();
            check.captured_camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("helix-firestorm.png");
            check.phase = 6;
            println!(
                "Original Helix firestorm capture: persistent physical bomb created its original visible FireOCL and advancing fire audio"
            );
        }
        inspect_crash(&mut check, &mut view)?;
        diagnose_stalled_pose(&mut check, &mut view)?;
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Helix native render phase {}: {error}", check.phase);
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}

fn diagnose_stalled_pose(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    if !matches!(check.phase, 0 | 3 | 8 | 11) {
        check.pose_wait = None;
        return Ok(());
    }
    let now = view.real.elapsed_secs_f64();
    if check
        .pose_wait
        .is_none_or(|(phase, _)| phase != check.phase)
    {
        check.pose_wait = Some((check.phase, now));
        return Ok(());
    }
    if check
        .pose_wait
        .is_none_or(|(_, started)| now - started < 20.0)
    {
        return Ok(());
    }
    let game = &view.battle.simulation;
    let mut bodies = Vec::new();
    for id in check
        .setup
        .helicopters
        .into_iter()
        .chain(check.bomb)
        .chain(check.hull)
    {
        bodies.push((
            id,
            game.object_position(id),
            view.framing.visible(&view.battle, id)?,
        ));
        if let Some(child) = game.entities[id].attachment_child() {
            bodies.push((
                child,
                game.object_position(child),
                view.framing.visible(&view.battle, child)?,
            ));
        }
    }
    error!(
        "Helix pose stalled phase={} audio={:?} camera_focus={:?} distance={} body_XYZ_framed={bodies:?}; saving helix-capture-failure.png before exit",
        check.phase, check.sounds, view.camera.focus, view.camera.distance
    );
    check.captured_camera = Some((view.camera.focus, view.camera.distance));
    view.capture.intermediate("helix-capture-failure.png");
    check.phase = 254;
    Ok(())
}

fn advance_crash(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let id = check.setup.helicopters[0];
    if check.phase == 6 {
        control.battle.select_objects(&[id], true);
        helix_verification::crash::approach(&mut control.battle.simulation, &check.setup)?;
        check.phase = 7;
        control.time.unpause();
    } else if check.phase == 7 {
        let game = &control.battle.simulation;
        if game.entities[id].helicopter_corpse() {
            check.crash_started.get_or_insert(game.tick);
        }
        if check
            .crash_started
            .is_some_and(|frame| game.tick >= frame + 15)
            && !game.entities[id].helicopter_hit_ground()
        {
            require(
                game.entities[id]
                    .last_attacker()
                    .is_some_and(|source| check.setup.defenders.contains(&source)),
                "native Helix death did not come from paid AA",
            )?;
            control.time.pause();
            check.phase = 8;
        }
    } else if check.phase == 9 {
        control.time.unpause();
        check.phase = 10;
    } else if check.phase == 10 {
        check.hull = control
            .battle
            .simulation
            .entities
            .iter()
            .enumerate()
            .find_map(|(id, actor)| {
                (actor.physical_present()
                    && control.battle.simulation.catalog[actor.type_index].name
                        == "HelixRubbleHull")
                    .then_some(id)
            });
        if check.hull.is_some() {
            control.time.pause();
            check.phase = 11;
        }
    }
    Ok(())
}

fn inspect_crash(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    let id = check.setup.helicopters[0];
    if check.phase == 8
        && check.sounds[3]
        && view.framing.visible(&view.battle, id)?
        && view.framing.physical_pose(&view.battle, id)?
        && view
            .particles
            .object_attached(id, "SootySmokeTrail", view.battle.generation)?
    {
        check.captured_camera = Some((view.camera.focus, view.camera.distance));
        view.capture.intermediate("helix-spiral.png");
        check.phase = 9;
        println!(
            "Original Helix spiral capture: real paid Patriot combat, visible airborne corpse with physical orientation, attached original smoke and advancing damaged loop"
        );
    } else if check.phase == 11 {
        let hull = check.hull.ok_or("missing physical Helix hull")?;
        if view.framing.visible(&view.battle, hull)?
            && view.framing.physical_pose(&view.battle, hull)?
        {
            let game = &view.battle.simulation;
            require(
                game.entities[id].masked()
                    && check
                        .setup
                        .passengers
                        .iter()
                        .all(|&passenger| !game.entities[passenger].alive()),
                "native Helix hull appeared before loaded aircraft destruction",
            )?;
            check.captured_camera = Some((view.camera.focus, view.camera.distance));
            view.capture.finish_scenario();
            check.phase = 255;
            println!(
                "Original Helix capture PASSED: paid attachments/passengers, targeted persistent napalm and visible firestorm, original AA spiral and fully visible physical HelixRubbleHull"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_verification::advance_until;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_capture_target_releases_a_separated_original_bomb() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let setup = helix_verification::prepare(game).unwrap();
        let id = setup.helicopters[1];
        helix_verification::arm_bomb(game, id).unwrap();
        let target = separated_bomb_target(game, &setup).unwrap();
        let first_new = game.entities.len();
        let health = setup
            .helicopters
            .map(|id| game.entities[id].health.to_bits());
        game.helix_bomb(id, target.to_array()).unwrap();
        advance_until(game, 1800, "native separated Helix bomb release", |game| {
            game.entities[id]
                .helix_bomb_object()
                .is_some_and(|bomb| bomb >= first_new)
        })
        .unwrap();
        let bomb = game.entities[id].helix_bomb_object().unwrap();
        assert_eq!(
            game.catalog[game.entities[bomb].type_index].name,
            "NapalmBomb"
        );
        assert_eq!(
            game.object_position(bomb).map(f32::to_bits),
            game.object_position(id).map(f32::to_bits)
        );
        advance_until(game, 300, "native falling bomb capture pose", |game| {
            game.entities[bomb].alive()
                && game.object_position(id)[2] - game.object_position(bomb)[2] > 15.0
        })
        .unwrap();
        assert_eq!(
            setup
                .helicopters
                .map(|id| game.entities[id].health.to_bits()),
            health
        );
        let actor = &game.entities[id];
        for other in setup.helicopters.into_iter().filter(|&other| other != id) {
            let parked = &game.entities[other];
            let radius = game.catalog[actor.type_index].geometry_bounds.sphere_radius
                + game.catalog[parked.type_index]
                    .geometry_bounds
                    .sphere_radius;
            let dx = actor.position[0] - parked.position[0];
            let dy = actor.position[1] - parked.position[1];
            assert!(dy.mul_add(dy, dx * dx) > radius * radius);
        }
        assert!(game.entities[bomb].physical_present());
        assert!(game.object_position(bomb)[2] > game.ground_height(game.entities[bomb].position));
    }
}
