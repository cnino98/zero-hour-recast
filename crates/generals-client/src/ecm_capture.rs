//! Original paid ECM beam, jammer pulse and JAMMED missile particles.
use crate::{
    capture_framing::{self, AircraftFrame},
    ecm_verification::{self, Setup},
    input::{self, Mouse, PickTarget},
    laser_beam::PulseInspection,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

mod pulse;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    began: Option<f64>,
    health: f32,
    emission: Option<(u64, u8)>,
    evidence: Option<pulse::Evidence>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = ecm_verification::prepare(&mut battle.simulation)?;
    let health = battle.simulation.entities[setup.vehicle].health;
    battle.select_objects(&[setup.tank], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        began: None,
        health,
        emission: None,
        evidence: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, begin);
    app.add_systems(FixedUpdate, record.after(crate::laser_beam::record));
    app.add_systems(
        Update,
        frame
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

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    time: ResMut<'w, Time<Virtual>>,
    clock: Res<'w, Time<Real>>,
    capture: Res<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    let now = control.clock.elapsed_secs_f64();
    if now - *check.began.get_or_insert(now) > 90.0 {
        error!("ECM native capture timed out phase={}", check.phase);
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() || !matches!(check.phase, 0 | 2 | 4 | 6) {
        return;
    }
    if matches!(check.phase, 4 | 6) {
        check.phase += 1;
        control.time.unpause();
        return;
    }
    let result = if check.phase == 0 {
        let target = check.setup.vehicle;
        let point = Vec2::from_array(control.battle.simulation.entities[target].position);
        require(
            input::context_order(
                &mut control.battle,
                Some(point),
                Some(PickTarget::Unit(target)),
                &mut control.mouse,
            ),
            "native ordinary ECM Attack rejected",
        )
    } else {
        ecm_verification::start_jammer(&mut control.battle.simulation, &check.setup)
    };
    if let Err(error) = result {
        error!("ECM native setup: {error}");
        check.phase = 255;
        control.exit.write(AppExit::error());
    } else {
        check.phase += 1;
        control.time.unpause();
    }
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    if let Some(evidence) = &check.evidence {
        evidence.restore_camera(&mut camera);
        return;
    }
    let game = &battle.simulation;
    let tank = game.object_position(check.setup.tank);
    let target = game.object_position(if check.phase <= 2 {
        check.setup.vehicle
    } else {
        check.setup.launcher
    });
    let focus = std::array::from_fn(|axis| (tank[axis] + target[axis]) * 0.5);
    camera.settings.yaw = (target[1] - tank[1]).atan2(target[0] - tank[0]);
    camera.distance = camera.settings.max_height;
    capture_framing::focus_on(&mut camera, terrain, focus);
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    framing: AircraftFrame<'w, 's>,
    pulses: PulseInspection<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    rig: Res<'w, CameraRig>,
    camera: Single<
        'w,
        's,
        (&'static Camera, &'static GlobalTransform),
        With<crate::playtest_view::WorldCamera>,
    >,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if !matches!(check.phase, 1 | 3 | 5 | 7) || !view.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        if matches!(check.phase, 5 | 7) {
            return pulse::inspect(&mut check, &mut view);
        }
        let game = &view.battle.simulation;
        let tank = check.setup.tank;
        require(game.entities[tank].alive(), "native paid ECM died")?;
        require(
            game.entities[check.setup.vehicle].health.to_bits() == check.health.to_bits(),
            "native ECM damaged original vehicle health",
        )?;
        if !view.framing.visible(&view.battle, tank)? {
            return Ok(());
        }
        if check.phase == 1 {
            if !game.entities[check.setup.vehicle].subdued()
                || !view.framing.visible(&view.battle, check.setup.vehicle)?
                || !view.pulses.visible(tank, "ECMTankVehicleDisabler")?
            {
                return Ok(());
            }
            view.time.pause();
            view.capture.intermediate("ecm-disabled-vehicle.png");
            check.phase = 2;
        } else {
            let Some(projectile) = game.projectiles.iter().find(|projectile| {
                projectile.source == check.setup.launcher && projectile.jammed(&game.entities)
            }) else {
                return Ok(());
            };
            require(
                projectile.owner == 1
                    && projectile
                        .object
                        .is_some_and(|id| game.entities[id].alive()),
                "jammed missile lost original launch owner or live flight",
            )?;
            let Some((frame, veterancy)) = check.emission else {
                return Ok(());
            };
            let Some(viewport) = view.framing.uncovered_viewport() else {
                return Ok(());
            };
            let (point, _) = projectile
                .effect_transform(&game.entities)
                .ok_or("jammed missile lacks flight transform")?;
            let (camera, transform) = *view.camera;
            let position = Vec3::new(point[0], point[2], -point[1]) * 0.1;
            if !camera
                .world_to_viewport(transform, position)
                .is_ok_and(|point| viewport.contains(point))
                || !view.framing.visible(&view.battle, check.setup.launcher)?
                || !view
                    .particles
                    .jammed_missile(projectile.id, view.battle.generation)?
            {
                return Ok(());
            }
            let kind = game.entities[tank].type_index;
            let index = game.catalog[kind]
                .fire_weapon_index(0)
                .ok_or("missing ECM jammer weapon")?;
            if !view.particles.weapon_fire_above_hud(
                (kind, index, veterancy),
                frame,
                view.battle.generation,
                ((camera, transform), viewport),
            )? {
                return Ok(());
            }
            view.time.pause();
            view.capture.intermediate("ecm-jammed-missile.png");
            check.phase = 4;
            println!(
                "Original paid ECM jam state captured: vehicle disable with unchanged health, original beam, actual live enemy missile jam and JAMMED SparksMedium above HUD; additive pulse pair pending"
            );
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("ECM native capture phase {}: {error}", check.phase);
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    let battle = battle.into_inner();
    if check.phase != 3 {
        return;
    }
    let game = &battle.simulation;
    let index = game.catalog[game.entities[check.setup.tank].type_index].fire_weapon_index(0);
    if let Some(shot) = game
        .presentation
        .shots
        .iter()
        .find(|shot| shot.source == check.setup.tank && Some(shot.weapon_index) == index)
    {
        check.emission = Some((game.tick, shot.veterancy));
    }
}
