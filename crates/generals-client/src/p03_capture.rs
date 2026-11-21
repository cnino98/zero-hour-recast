//! Paid superweapons exercised through the original command bar and target order.
mod china;
pub mod gla;
mod usa;

use crate::{
    capture_framing::{AircraftFrame, focus_on},
    control_bar::UiData,
    input::{self, CommandMode, Mouse},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{dispatch_command, selected_command},
    playtest_view::Capture,
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::{Faction, Skirmish, hazards::Field};
use std::error::Error;

pub struct Scene {
    provider: usize,
    power: &'static str,
    target: [f32; 3],
    subject: usize,
}

#[derive(Resource)]
struct Check {
    faction: Faction,
    scene: Scene,
    phase: u8,
    framed_phase: u8,
    issued: bool,
    started: Option<f64>,
    scorch: Option<([f32; 3], f32)>,
    steering: Option<([f32; 2], [f32; 2], u64)>,
    heard_orbital: bool,
    seen_flight_geometry: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match, faction: Faction) -> Result<(), Box<dyn Error>> {
    battle.simulation.reset(faction, Faction::China)?;
    let scene = match faction {
        Faction::Usa => usa::prepare(&mut battle.simulation)?,
        Faction::China => china::prepare(&mut battle.simulation)?,
        Faction::Gla => gla::prepare(&mut battle.simulation)?,
        Faction::Civilian => return Err("civilian has no superweapon".into()),
    };
    battle.select_objects(&[scene.provider], true);
    app.insert_resource(Check {
        faction,
        scene,
        phase: 0,
        framed_phase: u8::MAX,
        issued: false,
        started: None,
        scorch: None,
        steering: None,
        heard_orbital: false,
        seen_flight_geometry: false,
    });
    if faction == Faction::Usa {
        app.add_systems(PreUpdate, hear_orbital);
    }
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
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
    Ok(())
}

fn hear_orbital(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    audio: crate::source_audio::FiringAudioInspection,
) {
    let game = &battle.into_inner().simulation;
    let provider = check.scene.provider;
    let unit = &game.catalog[game.entities[provider].type_index];
    if let Some(rules) = unit
        .behavior
        .particle_uplinks
        .iter()
        .find(|rules| rules.power == check.scene.power)
        && audio.played([(provider, &rules.sounds[3])])[0]
        && !check.heard_orbital
    {
        check.heard_orbital = true;
        println!("Native P03 orbital loop: {}", rules.sounds[3]);
    }
}

fn frame(
    check: ResMut<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    capture: Res<Capture>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    // Match the verified frame while the asynchronous screenshot is pending.
    if check.phase != 0 && !capture.into_inner().ready_for_scenario() {
        return;
    }
    let game = &battle.into_inner().simulation;
    let terrain = terrain.into_inner();
    let point = match (check.phase, check.faction) {
        (0, _) => game.object_position(check.scene.subject),
        (1, Faction::China | Faction::Gla) => game
            .projectiles
            .iter()
            .filter(|projectile| projectile.source == check.scene.provider)
            .find_map(|projectile| projectile.render_transform(&game.entities))
            .map_or(check.scene.target, |(position, _)| position),
        (2, Faction::Usa) => check
            .scorch
            .map_or(check.scene.target, |(position, _)| position),
        (2, Faction::Gla) => scud_aftermath(game, check.scene.provider)
            .map_or(check.scene.target, |field| field.position),
        _ => check.scene.target,
    };
    // The rising nuclear rocket exits the low downward camera before it clears the silo.
    camera.distance = if check.phase == 1 && check.faction == Faction::Gla {
        camera.settings.min_height
    } else {
        camera.settings.max_height
    };
    focus_on(&mut camera, terrain, point);
    check.framed_phase = check.phase;
}

#[derive(SystemParam)]
struct View<'w, 's> {
    frame: AircraftFrame<'w, 's>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    ui: Res<'w, UiData>,
    clock: Res<'w, Time<Real>>,
    beam: crate::particle_uplink::Inspection<'w, 's>,
    scorch: crate::scorch::Inspection<'w, 's>,
    particles: ParticleInspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    // The screenshot observer can finish in Update after the camera already ran.
    if check.phase == u8::MAX || check.framed_phase != check.phase || !capture.ready_for_scenario()
    {
        return;
    }
    let started = *check
        .started
        .get_or_insert_with(|| view.clock.elapsed_secs_f64());
    let result = if view.clock.elapsed_secs_f64() - started < 120.0 {
        observe(&mut check, view, &mut battle, &mut mouse, &mut time)
    } else {
        Err(format!(
            "native superweapon phase {} timed out, bound flight geometry={}",
            check.phase, check.seen_flight_geometry
        )
        .into())
    };
    match result {
        Ok(true) => {
            time.pause();
            check.started = None;
            let file = filename(check.faction, check.phase);
            println!("Native P03 power: {file}, frame {}", battle.simulation.tick);
            if check.phase == 2 {
                check.phase = u8::MAX;
                capture.finish_scenario();
            } else {
                check.phase += 1;
                capture.intermediate(file);
            }
        }
        Ok(false) => {}
        Err(error) => {
            error!("Native P03 power: {error}");
            check.phase = u8::MAX;
            exit.write(AppExit::error());
        }
    }
}

fn observe(
    check: &mut Check,
    view: View,
    battle: &mut Match,
    mouse: &mut Mouse,
    time: &mut Time<Virtual>,
) -> Result<bool, Box<dyn Error>> {
    let View {
        frame,
        projectiles,
        ui,
        beam,
        scorch,
        particles,
        ..
    } = view;
    let ui = ui.into_inner();
    if check.phase == 0 || !check.issued {
        let action = (1..=14)
            .filter_map(|slot| selected_command(battle, ui, slot))
            .find(|command| command.action.power() == Some(check.scene.power))
            .ok_or("original superweapon button missing")?
            .action;
        require(
            battle.simulation.can_use_power_at_location(
                check.scene.provider,
                check.scene.power,
                check.scene.target,
            ),
            "original superweapon or its target is not ready",
        )?;
        if check.phase == 0 {
            return frame.visible(battle, check.scene.provider);
        }
        dispatch_command(battle, mouse, action);
        require(
            matches!(mouse.pending_command, CommandMode::Power { caster, name, .. }
                if caster == check.scene.provider && name == check.scene.power),
            "original superweapon button did not enter targeting mode",
        )?;
        input::targeted_order(
            battle,
            mouse,
            Vec2::new(check.scene.target[0], check.scene.target[1]),
            None,
        );
        require(
            matches!(mouse.pending_command, CommandMode::Normal),
            "original superweapon target was rejected",
        )?;
        check.issued = true;
        time.unpause();
        return Ok(false);
    }
    time.unpause();
    if battle
        .simulation
        .power_cooldown_remaining(check.scene.provider, check.scene.power)
        == 0
    {
        return Ok(false);
    }
    let game = &battle.simulation;
    let marks = &game.presentation;
    for mark in &marks.particle_uplink_scorches {
        let previous = check.scorch.map_or(0.0, |(_, radius)| radius);
        if mark.radius > previous && scorch.at(mark.position, mark.radius)? {
            check.scorch = Some((mark.position, mark.radius));
        }
    }
    if let Some(mark) = marks.neutron_scorches.first() {
        check.scorch.get_or_insert((mark.position, mark.radius));
    }
    if check.phase == 1 {
        if matches!(check.faction, Faction::Usa) {
            return observe_steered_beam(check, &beam, battle, mouse);
        }
        return observe_flight(check, battle, &frame, &projectiles);
    }
    observe_impact(check, &scorch, &particles, battle)
}

fn observe_flight(
    check: &mut Check,
    battle: &Match,
    frame: &AircraftFrame<'_, '_>,
    visuals: &Query<'_, '_, (&ProjectileVisual, &Children)>,
) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    for projectile in &game.projectiles {
        if projectile.source != check.scene.provider {
            continue;
        }
        let Some(object) = projectile.object else {
            continue;
        };
        let Some((_, children)) = visuals.iter().find(|(visual, _)| {
            visual.generation == battle.generation && visual.id == projectile.id
        }) else {
            continue;
        };
        if !check.seen_flight_geometry {
            check.seen_flight_geometry = true;
            let name = &game.catalog[game.entities[object].type_index].name;
            println!(
                "Native P03 flight geometry: {name} at {:?}, {} mesh children",
                game.object_position(object),
                children.len(),
            );
        }
        if frame.isolated_projectile(battle, children)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn observe_steered_beam(
    check: &mut Check,
    beam: &crate::particle_uplink::Inspection<'_, '_>,
    battle: &mut Match,
    mouse: &mut Mouse,
) -> Result<bool, Box<dyn Error>> {
    let provider = check.scene.provider;
    let Some(position) = battle
        .simulation
        .particle_uplink_visuals(provider)
        .next()
        .and_then(|visual| visual.target)
        .map(|target| target.position)
    else {
        return Ok(false);
    };
    let origin = Vec2::new(position[0], position[1]);
    if let Some((start, direction, deadline)) = check.steering {
        let game = &mut battle.simulation;
        require(
            game.tick + u64::from(game.power_cooldown_remaining(provider, check.scene.power))
                == deadline,
            "driving the Particle Uplink changed its countdown deadline",
        )?;
        return Ok(
            (origin - Vec2::from_array(start)).dot(Vec2::from_array(direction)) >= 8.0
                && beam.visible_impact(battle, provider)?
                && check.heard_orbital,
        );
    }
    if !beam.visible_impact(battle, provider)? {
        return Ok(false);
    }
    let game = &battle.simulation;
    let target = [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y]
        .into_iter()
        .map(|direction| origin + 36.0 * direction)
        .find(|point| {
            let [x, y] = point.to_array();
            game.can_override_special_power_destination(
                provider,
                [x, y, game.ground_height([x, y])],
            )
        })
        .ok_or("no explored terrain for Particle Uplink steering")?;
    let deadline = battle.simulation.tick
        + u64::from(
            battle
                .simulation
                .power_cooldown_remaining(provider, check.scene.power),
        );
    require(
        input::context_order(battle, Some(target), None, mouse),
        "normal context order failed to drive the Particle Uplink",
    )?;
    check.steering = Some((
        origin.to_array(),
        (target - origin).normalize().to_array(),
        deadline,
    ));
    Ok(false)
}

fn observe_impact(
    check: &Check,
    scorch: &crate::scorch::Inspection,
    particles: &ParticleInspection,
    battle: &Match,
) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    match check.faction {
        Faction::Usa => {
            let provider = check.scene.provider;
            if game
                .particle_uplink_visuals(provider)
                .any(|visual| visual.target.is_some())
            {
                return Ok(false);
            }
            let unit = &game.catalog[game.entities[provider].type_index];
            let rules = unit
                .behavior
                .particle_uplinks
                .iter()
                .find(|rules| rules.power == check.scene.power)
                .ok_or("missing original Particle Uplink update")?;
            if !particles.actor_flash_finished(&rules.ground_hit_fx, battle.generation)? {
                return Ok(false);
            }
            check
                .scorch
                .map_or(Ok(false), |(position, radius)| scorch.at(position, radius))
        }
        Faction::China => {
            let Some((position, radius)) = check.scorch else {
                return Ok(false);
            };
            Ok(scorch.at(position, radius)?
                && particles.incendiary("NukeMushroomExplosion", battle.generation)?)
        }
        Faction::Gla => {
            let Some(field) = scud_aftermath(game, check.scene.provider) else {
                return Ok(false);
            };
            particles.mature_incendiary_at(
                "PoisonFieldLarge",
                battle.generation,
                [field.position[0], field.position[1]],
            )
        }
        Faction::Civilian => unreachable!("validated native faction"),
    }
}

fn scud_aftermath(game: &Skirmish, provider: usize) -> Option<&Field> {
    if game.entities[provider].radius_decal().is_some()
        || game
            .projectiles
            .iter()
            .any(|projectile| projectile.source == provider)
    {
        return None;
    }
    game.hazards.iter().rev().find(|field| {
        let actor = &game.entities[field.object];
        let missile = &game.entities[field.source];
        actor.alive()
            && field.owner == game.entities[provider].owner
            && missile.producer() == Some(provider)
            && game.catalog[missile.type_index].name == "ScudStormMissile"
            && game.catalog[actor.type_index].name == "PoisonFieldLarge"
    })
}

fn filename(faction: Faction, phase: u8) -> &'static str {
    match (faction, phase) {
        (Faction::Usa, 0) => "p03-particle-ready.png",
        (Faction::Usa, 1) => "p03-particle-beam.png",
        (Faction::Usa, _) => "p03-particle-scorch.png",
        (Faction::China, 0) => "p03-nuke-ready.png",
        (Faction::China, 1) => "p03-nuke-flight.png",
        (Faction::China, _) => "p03-nuke-blast.png",
        (Faction::Gla, 0) => "p03-scud-ready.png",
        (Faction::Gla, 1) => "p03-scud-flight.png",
        (Faction::Gla, _) => "p03-scud-toxin.png",
        (Faction::Civilian, _) => unreachable!("validated native faction"),
    }
}
