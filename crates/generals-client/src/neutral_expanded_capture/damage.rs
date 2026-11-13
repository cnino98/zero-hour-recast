//! Paid capture and force-fire exercise the original tech building's damage graph.
use crate::{
    capture_framing::{self, AircraftFrame},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::{advance_until, construct, friendly_entity, produce, require},
    source_audio::FiringAudioInspection,
    terrain::Terrain,
};
use bevy::prelude::*;
use generals_sim::skirmish::Kind;
use std::error::Error;

#[derive(Resource)]
struct Check {
    provider: usize,
    attacker: usize,
    stage: Stage,
    death: Option<u64>,
    detonation: Option<u64>,
    started: Option<f64>,
}

enum Stage {
    Waiting,
    Damage,
    Captured,
    Death,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (provider, attacker) = prepare_damage(&mut battle.simulation)?;
    battle.select_objects(&[provider], true);
    app.insert_resource(Check {
        provider,
        attacker,
        stage: Stage::Waiting,
        death: None,
        detonation: None,
        started: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

fn prepare_damage(
    game: &mut generals_sim::skirmish::Skirmish,
) -> Result<(usize, usize), Box<dyn Error>> {
    let (provider, _) =
        crate::neutral_expanded_verification::prepare(game, "TechArtilleryPlatform")?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let attacker = produce(game, factory, Kind::Tank, 0)?;
    let point = game.entities[provider].position;
    game.move_to(attacker, [point[0] + 130.0, point[1]])?;
    advance_until(
        game,
        18000,
        "paid tank approaching captured tech building",
        |game| {
            !game.entities[attacker].moving()
                && (game.entities[attacker].position[0] - point[0])
                    .hypot(game.entities[attacker].position[1] - point[1])
                    < 200.0
        },
    )?;
    Ok((provider, attacker))
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, focus);
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn focus(
    check: Res<Check>,
    battle: Res<Match>,
    mut camera: ResMut<CameraRig>,
    terrain: Res<Terrain>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    capture_framing::focus_on(
        &mut camera,
        terrain.into_inner(),
        battle.simulation.object_position(check.provider),
    );
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    for event in &battle.into_inner().simulation.presentation.object_fx_events {
        if event.object != check.provider {
            continue;
        }
        match event.effect.as_str() {
            "FX_BuildingDie" => check.death = Some(event.frame),
            "WeaponFX_BombTruckDefaultBombDetonation" => check.detonation = Some(event.frame),
            _ => {}
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    frame: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    audio: FiringAudioInspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    view: View,
    mut capture: ResMut<Capture>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if matches!(check.stage, Stage::Done) {
        return;
    }
    let now = view.clock.elapsed_secs_f64();
    let elapsed = now - *check.started.get_or_insert(now);
    let result = (|| {
        require(elapsed < 120.0, "tech building damage capture timed out")?;
        if !capture.ready_for_scenario() {
            return Ok(());
        }
        match check.stage {
            Stage::Waiting => {
                if view.frame.visible(&battle, check.provider)? {
                    battle
                        .simulation
                        .force_attack(check.attacker, check.provider)?;
                    check.stage = Stage::Damage;
                    time.unpause();
                }
            }
            Stage::Damage => {
                if view.particles.object_attached(
                    check.provider,
                    "StructureTransitionSmallSmoke",
                    battle.generation,
                )? && view.frame.visible(&battle, check.provider)?
                {
                    check.stage = Stage::Captured;
                    time.pause();
                    capture.intermediate("neutral-tech-damaged.png");
                }
            }
            Stage::Captured => {
                check.stage = Stage::Death;
                time.unpause();
            }
            Stage::Death => {
                if let Some(death) = check.death
                    && check.detonation == Some(death)
                    && view.frame.visible(&battle, check.provider)?
                    && view
                        .particles
                        .incendiary("BombTruckDefaultExplosionArms", battle.generation)?
                    && view.audio.object_fx("BuildingDie")
                {
                    require(
                        !battle.simulation.entities[check.provider].alive(),
                        "tech provider death FX preceded real death",
                    )?;
                    require(
                        battle.simulation.players[battle.simulation.entities[check.provider].owner]
                            .faction
                            == generals_sim::skirmish::Faction::Civilian,
                        "destroyed provider did not return to neutral",
                    )?;
                    check.stage = Stage::Done;
                    time.pause();
                    capture.finish_scenario();
                    println!(
                        "Rendered neutral provider: paid capture, real force-fire, authored transition smoke, both death FX and neutral rubble"
                    );
                }
            }
            Stage::Done => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!(
            "Neutral provider damage: {error}; health={} death={:?}",
            battle.simulation.entities[check.provider].health, check.death
        );
        check.stage = Stage::Done;
        exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_sim::skirmish::{Faction, structure_death::VisualAction};

    #[test]
    #[ignore = "requires original game installation"]
    fn paid_captured_provider_keeps_transition_smoke_and_both_death_callbacks() {
        let root = generals_assets::game_root().unwrap();
        let fixture = crate::playtest::map_fixture::Fixture::load(
            &root,
            crate::neutral_expanded_verification::map_name(),
        )
        .unwrap();
        let mut game = fixture.into_simulation().unwrap();
        let (provider, attacker) = prepare_damage(&mut game).unwrap();
        game.force_attack(attacker, provider).unwrap();
        let mut smoke = false;
        let mut death = None;
        let mut detonation = None;
        for _ in 0..3_600 {
            game.step();
            smoke |= game.presentation.structure_events.iter().any(|event| {
                event.object == provider && matches!(&event.action,
                    VisualAction::TransitionParticle { template, .. } if template == "StructureTransitionSmallSmoke")
            });
            for event in &game.presentation.object_fx_events {
                if event.object == provider {
                    match event.effect.as_str() {
                        "FX_BuildingDie" => death = Some(event.frame),
                        "WeaponFX_BombTruckDefaultBombDetonation" => detonation = Some(event.frame),
                        _ => {}
                    }
                }
            }
            if !game.entities[provider].alive() {
                assert!(
                    smoke,
                    "paid cannon fire skipped the authored damage transition"
                );
                assert!(death.is_some(), "missing authored death sound callback");
                assert_eq!(death, detonation);
                assert_eq!(game.entities[provider].last_attacker(), Some(attacker));
                assert!(game.entities[provider].exists());
                assert!(matches!(
                    game.players[game.entities[provider].owner].faction,
                    Faction::Civilian
                ));
                return;
            }
        }
        panic!("ordinary force-fire did not destroy the captured provider");
    }
}
