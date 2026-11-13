//! Veterancy earned in combat permits the native Raptor pilot ejection.
use crate::{
    aircraft_verification::crash,
    capture_framing::{AircraftFrame, focus_on},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: crash::Setup,
    phase: Phase,
    began: Option<f64>,
    pilot: Option<usize>,
    canopy: Option<usize>,
    sounds: [bool; 3],
}

impl Check {
    fn require_ejection_audio(&self, pilot: usize) -> Result<(), Box<dyn Error>> {
        require(
            self.sounds == [true; 3],
            &format!(
                "native sinks for PilotVoiceEject, PilotSoundEject and pilot {pilot} ParachuteOpen: {:?}",
                self.sounds
            ),
        )?;
        println!(
            "Native ejection audio: PilotVoiceEject, PilotSoundEject and pilot {pilot} ParachuteOpen played"
        );
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
enum Phase {
    Waiting,
    Fight,
    Canopy,
    Landing,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = crash::prepare_pilot_ejection(&mut battle.simulation)?;
    battle.select_objects(&[setup.aircraft.raptor], true);
    app.insert_resource(Check {
        setup,
        phase: Phase::Waiting,
        began: None,
        pilot: None,
        canopy: None,
        sounds: [false; 3],
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        frame
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(FixedFirst, attack);
    app.add_systems(
        PostUpdate,
        (inspect_audio, inspect)
            .chain()
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if !matches!(check.phase, Phase::Fight | Phase::Canopy | Phase::Landing) {
        return;
    }
    let events = [
        (None, "PilotVoiceEject"),
        (None, "PilotSoundEject"),
        (check.pilot, "ParachuteOpen"),
    ];
    let mut played = audio.played_events(events);
    played[2] &= check.pilot.is_some();
    for (heard, played) in check.sounds.iter_mut().zip(played) {
        *heard |= played;
    }
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let game = &battle.into_inner().simulation;
    let id = check.pilot.unwrap_or(check.setup.aircraft.raptor);
    camera.distance = if game.height_above_ground(id) > 10.0 {
        camera.settings.max_height
    } else {
        camera.settings.min_height
    };
    focus_on(&mut camera, terrain.into_inner(), game.object_position(id));
}

fn attack(check: Res<Check>, mut battle: ResMut<Match>, mut exit: MessageWriter<AppExit>) {
    let check = check.into_inner();
    if check.phase == Phase::Fight
        && let Err(error) = crash::attack_when_visible(&mut battle.simulation, &check.setup)
    {
        error!("Native ejected pilot opposing Gattling command: {error}");
        exit.write(AppExit::error());
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    framing: AircraftFrame<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut capture: ResMut<Capture>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == Phase::Done || !capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = view.advance(&mut check, &mut battle, &mut capture, &mut time) {
        check.phase = Phase::Done;
        error!("Native ejected pilot: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn advance(
        self,
        check: &mut Check,
        battle: &mut Match,
        capture: &mut Capture,
        time: &mut Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let now = self.clock.elapsed_secs_f64();
        let start = *check.began.get_or_insert(now);
        let game = &battle.simulation;
        let jet = check.setup.aircraft.raptor;
        let actor = &game.entities[jet];
        require(
            now - start < 90.0,
            &format!(
                "ejection timed out at {} with aircraft alive={}, attacker={:?} and pilot {:?}",
                game.tick,
                actor.alive(),
                actor.last_attacker(),
                check.pilot
            ),
        )?;
        match check.phase {
            Phase::Waiting if self.framing.visible(battle, jet)? => {
                crash::begin(&mut battle.simulation, &check.setup)?;
                check.phase = Phase::Fight;
                time.unpause();
            }
            Phase::Fight if !actor.alive() => {
                require(
                    actor.last_attacker() == Some(check.setup.attacker),
                    "veteran jet did not fall to paid Gattling fire",
                )?;
                if check.pilot.is_none() {
                    check.pilot = game.entities.iter().position(|candidate| {
                        candidate.producer() == Some(jet)
                            && game.catalog[candidate.type_index].name == "AmericaInfantryPilot"
                    });
                }
                if let Some(pilot) = check.pilot
                    && let Some(chute) = game.entities[pilot].contained_by()
                    && game.entities[chute].parachute_opened()
                    && self.framing.visible(battle, chute)?
                    && self.framing.visible(battle, pilot)?
                {
                    let crew = &game.entities[pilot];
                    require(
                        crew.alive()
                            && crew.owner == 0
                            && crew.veterancy == actor.veterancy
                            && crew.parachute_rider()
                            && game.height_above_ground(chute) > 15.0,
                        "native OCL pilot lost owner, veterancy, or airborne canopy",
                    )?;
                    time.pause();
                    check.phase = Phase::Canopy;
                    check.canopy = Some(chute);
                    capture.intermediate("ejected-pilot-canopy.png");
                    println!(
                        "Native ejection: paid Gattling destroyed veteran Raptor {jet}; original pilot {pilot} visible under canopy {chute}"
                    );
                }
            }
            Phase::Canopy => {
                check.phase = Phase::Landing;
                time.unpause();
            }
            Phase::Landing => {
                let pilot = check.pilot.ok_or("landing has no ejected pilot")?;
                let crew = &game.entities[pilot];
                require(crew.alive(), "ordinary ejected pilot died before touchdown")?;
                if crew.contained_by().is_none()
                    && check
                        .canopy
                        .is_some_and(|id| !game.entities[id].has_drawable())
                    && self.framing.visible(battle, pilot)?
                {
                    require(
                        !crew.parachute_rider()
                            && !crew.masked()
                            && crew.selectable(&game.catalog[crew.type_index])
                            && game.height_above_ground(pilot).abs() < 0.1,
                        "original ejected pilot did not return to ground control",
                    )?;
                    check.require_ejection_audio(pilot)?;
                    battle.select_objects(&[pilot], true);
                    check.phase = Phase::Done;
                    time.pause();
                    capture.finish_scenario();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::crash;

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_raptor_earns_ejection_veterancy_and_the_original_pilot_lands() {
        let (_, mut battle, mut animations) =
            crate::cleanup_verification::tests::installed_with_animations();
        let game = &mut battle.simulation;
        animations.bind_logical_bones(&mut game.catalog);
        let setup = crash::prepare_pilot_ejection(game).unwrap();
        let jet = setup.aircraft.raptor;
        let rank = game.entities[jet].veterancy;
        assert!(rank > 0);
        crash::begin(game, &setup).unwrap();
        let mut pilot = None;
        let mut opened = false;
        for _ in 0..7200 {
            crash::attack_when_visible(game, &setup).unwrap();
            game.step();
            if pilot.is_none() {
                pilot = game.entities.iter().position(|actor| {
                    actor.producer() == Some(jet)
                        && game.catalog[actor.type_index].name == "AmericaInfantryPilot"
                });
            }
            if let Some(pilot) = pilot {
                let crew = &game.entities[pilot];
                assert!(crew.alive());
                assert_eq!(crew.veterancy, rank);
                if let Some(chute) = crew.contained_by() {
                    opened |= game.entities[chute].parachute_opened();
                } else {
                    assert!(opened && !crew.parachute_rider() && !crew.masked());
                    assert!(game.height_above_ground(pilot).abs() < 0.1);
                    assert_eq!(game.entities[jet].last_attacker(), Some(setup.attacker));
                    return;
                }
            }
        }
        panic!("original pilot failed to emerge and land; pilot={pilot:?} opened={opened}");
    }
}
