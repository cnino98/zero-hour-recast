use super::{Check, Kind, Phase, issue};
use crate::{
    capture_framing::AircraftFrame,
    control_bar::UiData,
    input::Mouse,
    model_animation::{Animations, Players},
    particles::ParticleInspection,
    playtest::Match,
    playtest_view::{Capture, UnitVisual, WorldCamera},
    projectile_render::ProjectileVisual,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::visibility::Status;
use std::error::Error;

#[derive(SystemParam)]
pub(super) struct View<'w, 's> {
    evidence: Evidence<'w, 's>,
    ui: Res<'w, UiData>,
    clock: Res<'w, Time<Real>>,
}

pub(super) fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == Phase::Finished
        || check.framed != Some(check.phase)
        || !capture.ready_for_scenario()
    {
        return;
    }
    let View {
        evidence,
        ui,
        clock,
    } = view;
    let now = clock.into_inner().elapsed_secs_f64();
    let started = *check.started.get_or_insert(now);
    let outcome = if now - started >= 180.0 {
        let game = &battle.simulation;
        let aircraft = check.spawned(game, check.aircraft).count();
        let payload = check.spawned(game, check.payload).count();
        Err(format!(
            "{} timed out during {:?} after creating {aircraft} original aircraft and {payload} original payloads",
            check.source.power, check.phase,
        ).into())
    } else if check.phase == Phase::Ready {
        evidence.frame.visible(&battle, check.provider)
    } else if check.issued {
        time.unpause();
        observe(&check, &evidence, &mut battle)
    } else {
        issue(&mut check, &mut battle, ui.into_inner(), &mut mouse).map(|()| {
            time.unpause();
            false
        })
    };
    match outcome {
        Ok(true) => {
            time.pause();
            check.started = None;
            let phase = check.phase;
            let image = phase.intermediate().map(|index| check.kind.images()[index]);
            println!(
                "Native P03 {} {phase:?} at frame {}",
                check.source.power, battle.simulation.tick
            );
            check.phase = phase.next();
            if let Some(image) = image {
                capture.intermediate(image);
            } else {
                capture.finish_scenario();
            }
        }
        Ok(false) => {}
        Err(error) => {
            error!("Native P03 USA air support: {error}");
            check.phase = Phase::Finished;
            exit.write(AppExit::error());
        }
    }
}

#[derive(SystemParam)]
struct Evidence<'w, 's> {
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    frame: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    feedback: Res<'w, crate::selection_feedback::Feedback>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    draws: Query<'w, 's, (&'static UnitVisual, &'static Players)>,
    animations: Res<'w, Animations>,
}

fn observe(check: &Check, view: &Evidence, battle: &mut Match) -> Result<bool, Box<dyn Error>> {
    if battle
        .simulation
        .power_cooldown_remaining(check.provider, check.source.power)
        == 0
    {
        return Ok(false);
    }
    let game = &battle.simulation;
    match check.phase {
        Phase::Approach => {
            let Some(id) = check.live_actor(game, check.aircraft) else {
                return Ok(false);
            };
            if game.height_above_ground(id) <= 5.0 {
                return Ok(false);
            }
            let status = game.visibility_status(0, id);
            if !matches!(check.kind, Kind::A10) && !matches!(status, Status::Clear) {
                return Ok(false);
            }
            if matches!(check.kind, Kind::Spectre)
                && !matches!(
                    game.visibility_cell(0, game.entities[id].position),
                    Status::Clear
                )
            {
                return Ok(false);
            }
            let visible = view.frame.visible(battle, id)?;
            if visible {
                let status = match status {
                    Status::Clear => "clear",
                    Status::Partial => "partial",
                    Status::Fogged => "fogged",
                    Status::Shrouded => "shrouded",
                };
                println!("Native P03 original aircraft {id} shroud: {status}");
            }
            Ok(visible)
        }
        Phase::Payload => payload(check, view, battle),
        Phase::Effect => effect(check, view, battle),
        Phase::Cleared => {
            let aircraft_done = !check
                .spawned(game, check.aircraft)
                .any(|id| game.entities[id].exists() || game.entities[id].radius_decal().is_some());
            let payload_done = !check
                .spawned(game, check.payload)
                .any(|id| game.entities[id].exists())
                && check.projectile(game).is_none();
            let retired = |name| {
                view.particles
                    .original_system_retired(name, battle.generation)
            };
            let particles_done = check.source.particle.is_none_or(retired)
                && (!matches!(check.kind, Kind::Daisy)
                    || [
                        "DaisyExplosionScatterIgnite1",
                        "DaisyExplosionScatterIgnite2",
                        "DaisyFlameShockwaveSmoke",
                    ]
                    .into_iter()
                    .all(retired));
            let recovered = if let Some(victim) = check.leaflet_victim {
                game.entities[victim].alive()
                    && game.visible_to(0, victim)
                    && !game.object_disabled(victim)
                    && !game.entities[victim].drawable_disabled_tint()
                    && view.feedback.status_released(victim, battle.generation)
                    && view.frame.visible(battle, victim)?
            } else {
                true
            };
            Ok(aircraft_done && payload_done && particles_done && recovered)
        }
        Phase::Ready | Phase::Finished => Ok(false),
    }
}

fn effect(check: &Check, view: &Evidence, battle: &Match) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    if let Some(victim) = check.leaflet_victim {
        return Ok(game.entities[victim].alive()
            && game.object_disabled(victim)
            && game.entities[victim].drawable_disabled_tint()
            && view
                .particles
                .incendiary("LeafletParticles1", battle.generation)?
            && view.frame.visible(battle, victim)?);
    }
    if matches!(check.kind, Kind::Daisy) {
        let (camera, transform) = *view.camera;
        let quads = view.particles.additive_quads(
            "DaisyFlameShockwave",
            battle.generation,
            (camera, transform),
        )?;
        // Wait for the native additive ring to expand beyond its first frame.
        return Ok(view.frame.uncovered_viewport().is_some_and(|viewport| {
            quads.iter().any(|quad| {
                game.tick.saturating_sub(quad.born) >= 5
                    && quad.color[..3].iter().any(|&channel| channel > 0.0)
                    && viewport.contains(quad.bounds.center())
            })
        }));
    }
    check.source.particle.map_or_else(
        || {
            if check
                .spawned(game, check.payload)
                .any(|id| game.entities[id].exists())
            {
                return Ok(false);
            }
            check
                .landed_ranger(game)
                .map_or(Ok(false), |ranger| view.frame.visible(battle, ranger))
        },
        |name| view.particles.incendiary(name, battle.generation),
    )
}

fn payload(check: &Check, view: &Evidence, battle: &Match) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    if matches!(check.kind, Kind::A10 | Kind::Spectre) {
        let Some(projectile) = check.projectile(game) else {
            return Ok(false);
        };
        if matches!(check.kind, Kind::Spectre)
            && game.entities[projectile.source].spectre_doors_open() != Some(true)
        {
            return Ok(false);
        }
        let Some((_, children)) = view.projectiles.iter().find(|(visual, _)| {
            visual.generation == battle.generation && visual.id == projectile.id
        }) else {
            return Ok(false);
        };
        return Ok(view.frame.isolated_projectile(battle, children)?.is_some());
    }
    let Some(id) = check.airborne_payload(game) else {
        return Ok(false);
    };
    if matches!(check.kind, Kind::Paradrop) {
        let Some((_, players)) = view
            .draws
            .iter()
            .find(|(visual, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        if !view
            .animations
            .inspect(players, check.payload)?
            .iter()
            .any(|draw| draw.clip == "pmparacht_skl.pmparacht_opn" && draw.complete)
        {
            return Ok(false);
        }
    }
    if matches!(check.kind, Kind::Leaflet) {
        // The stock container has no drawable model. Its attached leaflets
        // are the visible falling payload.
        return leaflet_payload(check, view, battle, id);
    }
    if let Some(carrier) = check.live_actor(game, check.aircraft) {
        return view.frame.separated_actor(battle, carrier, id);
    }
    view.frame.visible(battle, id)
}

fn leaflet_payload(
    check: &Check,
    view: &Evidence,
    battle: &Match,
    payload: usize,
) -> Result<bool, Box<dyn Error>> {
    let Some(viewport) = view.frame.uncovered_viewport() else {
        return Ok(false);
    };
    let bomber = if let Some(carrier) = check.live_actor(&battle.simulation, check.aircraft) {
        let Some(bounds) =
            view.frame
                .mesh_projection(battle, carrier, &view.animations, "AVBOMBER.CHASSIS")?
        else {
            return Ok(false);
        };
        Some(bounds)
    } else {
        None
    };
    view.particles.attached_alpha_quad_clear(
        payload,
        "LeafletParticles1",
        battle.generation,
        *view.camera,
        viewport,
        bomber,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn actual_air_support_inspector_initializes_without_resource_conflicts() {
        use bevy::{ecs::system::System, prelude::*};
        let mut inspect = IntoSystem::into_system(super::inspect);
        inspect.initialize(&mut World::new());
    }
}
