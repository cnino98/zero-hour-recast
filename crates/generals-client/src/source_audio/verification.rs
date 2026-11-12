use bevy::prelude::*;
use generals_sim::skirmish::Faction;

use super::{SourceAudio, playback};
use crate::playtest::Match;

const SPEECH_EVENTS: [&str; 7] = [
    "EvaChina_Defeat",
    "EvaChina_Victory",
    "EvaGLA_Defeat",
    "EvaGLA_Victory",
    "EvaUSA_Defeat",
    "EvaUSA_Victory",
    "UI_AllCheerSound",
];
const EVENT_COUNT: usize = 4 + SPEECH_EVENTS.len();

#[derive(Resource)]
struct Verification {
    index: usize,
    requested_at: Option<f64>,
    music_due_at: Option<f64>,
    finished: bool,
    settled_frames: u8,
}

pub fn enable_verification(app: &mut App) {
    app.insert_resource(Verification {
        index: 0,
        requested_at: None,
        music_due_at: None,
        finished: false,
        settled_frames: 0,
    })
    .add_systems(First, (update, update_music).chain());
}

fn update(
    mut verification: ResMut<Verification>,
    mut audio: ResMut<SourceAudio>,
    time: Res<Time<Real>>,
    playing: Query<(&playback::Playing, Option<&AudioSink>)>,
    mut exit: MessageWriter<AppExit>,
) {
    let time = time.into_inner();
    if verification.finished || verification.index >= EVENT_COUNT {
        return;
    }
    // The original UI click is only 185ms. Let startup shader/model work settle
    // before measuring it; playback itself retains the original duration/volume.
    if verification.index == 0 && verification.requested_at.is_none() {
        verification.settled_frames = if time.delta_secs() < 0.1 {
            verification.settled_frames.saturating_add(1)
        } else {
            0
        };
        if time.elapsed_secs_f64() < 2.0 || verification.settled_frames < 10 {
            if time.elapsed_secs_f64() > 20.0 {
                eprintln!(
                    "Audio verification FAILED: renderer startup did not settle within 20 seconds"
                );
                verification.finished = true;
                exit.write(AppExit::error());
            }
            return;
        }
    }
    let Some(name) = event_name(&audio, verification.index).map(str::to_owned) else {
        eprintln!(
            "Audio verification FAILED: missing catalog audio binding at {}",
            verification.index
        );
        verification.finished = true;
        exit.write(AppExit::error());
        return;
    };
    let Some(start) = verification.requested_at else {
        if !audio.events.contains_key(&name) {
            eprintln!("Audio verification FAILED: missing source event {name}");
            verification.finished = true;
            exit.write(AppExit::error());
            return;
        }
        if audio.events[&name].speech {
            audio.speech(&name, true);
        } else {
            let position = audio.listener;
            let _ = audio.queue(&name, None, Some(position), None, false);
        }
        verification.requested_at = Some(time.elapsed_secs_f64());
        return;
    };
    if let Some((event, sink)) = playing.iter().find_map(|(event, sink)| {
        sink.filter(|sink| {
            event.event == name && !sink.empty() && sink.position().as_secs_f64() > 0.025
        })
        .map(|sink| (event, sink))
    }) {
        eprintln!(
            "Audio verification: {name}, original WAV samples {:?}, nonempty system-output sink advanced {:?}, volume {:?}",
            event.sample_names,
            sink.position(),
            sink.volume()
        );
        verification.index += 1;
        verification.requested_at = None;
        if verification.index == EVENT_COUNT {
            eprintln!(
                "Audio verification PASSED: original GUI, catalog building selection, unit voice, primary-weapon and seven scripted advisor/cheer recordings reached advancing system-output sinks; human audibility is not inferred."
            );
        }
    } else if time.elapsed_secs_f64() - start > 10.0 {
        eprintln!(
            "Audio verification FAILED: {name} produced no advancing nonempty system-output sink within 10 seconds"
        );
        verification.finished = true;
        exit.write(AppExit::error());
    }
}

fn update_music(
    mut verification: ResMut<Verification>,
    mut battle: ResMut<Match>,
    audio: Res<SourceAudio>,
    sinks: Query<&AudioSink>,
    time: Res<Time<Real>>,
    virtual_time: Res<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let audio = audio.into_inner();
    let time = time.into_inner();
    let virtual_time = virtual_time.into_inner();
    if verification.finished || verification.index < EVENT_COUNT {
        return;
    }
    match music(
        &mut verification,
        &mut battle,
        audio,
        &sinks,
        time.elapsed_secs_f64(),
    ) {
        Ok(true) => {
            verification.finished = true;
            exit.write(AppExit::Success);
        }
        Ok(false) => {}
        Err(error) => {
            let playing = audio.music.playing_track(battle.generation);
            let sink = playing
                .and_then(|(entity, _)| sinks.get(entity).ok())
                .map(|sink| (sink.empty(), sink.is_paused(), sink.position()));
            eprintln!(
                "Audio music verification FAILED: {error}; real={} requested={:?} backend_started={:?} generation={} tick={} virtual_elapsed={} virtual_delta={} virtual_paused={} battle_started={} battle_paused={} playing={playing:?} sink(empty,paused,position)={sink:?}; {}",
                time.elapsed_secs_f64(),
                verification.requested_at,
                verification.music_due_at,
                battle.generation,
                battle.simulation.tick,
                virtual_time.elapsed_secs_f64(),
                virtual_time.delta_secs_f64(),
                virtual_time.is_paused(),
                battle.started,
                battle.paused,
                audio.music.diagnostic(),
            );
            verification.finished = true;
            exit.write(AppExit::error());
        }
    }
}

fn music(
    verification: &mut Verification,
    battle: &mut Match,
    audio: &SourceAudio,
    sinks: &Query<&AudioSink>,
    now: f64,
) -> Result<bool, &'static str> {
    let index = verification.index - EVENT_COUNT;
    let (faction, expected) = match index {
        0 => (Faction::Usa, "Game_USA_01"),
        1 => (Faction::China, "Game_CHI_01"),
        2 => (Faction::Gla, "Game_GLA_01"),
        _ => return Ok(true),
    };
    let Some(_) = verification.requested_at else {
        battle.start(faction);
        verification.requested_at = Some(now);
        return Ok(false);
    };
    let playing = audio.music.playing_track(battle.generation);
    if playing.is_some() && battle.simulation.tick <= 300 {
        return Err("music started before its authored 10-second timer expired");
    }
    if !music_ready(verification, battle.simulation.tick, now)? {
        return Ok(false);
    }
    let Some((entity, name)) = playing else {
        return Ok(false);
    };
    if name != expected {
        return Err("first faction music differs from the installed script");
    }
    if let Ok(sink) = sinks.get(entity)
        && !sink.empty()
        && sink.position().as_secs_f64() > 0.025
    {
        eprintln!(
            "Original music verification: {name}, first source timer elapsed, advancing output sink {:?}",
            sink.position()
        );
        verification.index += 1;
        verification.requested_at = None;
        verification.music_due_at = None;
        if verification.index == EVENT_COUNT + 3 {
            eprintln!(
                "Audio verification PASSED: original click/voice/weapon events and all three authored initial music tracks reached advancing output sinks."
            );
            return Ok(true);
        }
    }
    Ok(false)
}

fn music_ready(verification: &mut Verification, tick: u64, now: f64) -> Result<bool, &'static str> {
    let requested = verification
        .requested_at
        .ok_or("music verification was not requested")?;
    // Source scripts evaluate before Object updates. Frame zero installs the
    // 300-frame timer; the resulting request is observable after update 301.
    if tick <= 300 {
        if now - requested > 90.0 {
            return Err("source faction music timer did not advance within 90 wall seconds");
        }
        return Ok(false);
    }
    let started = verification.music_due_at.get_or_insert(now);
    if now - *started > 20.0 {
        return Err(
            "original faction music produced no advancing sink within 20 seconds after its source timer",
        );
    }
    Ok(true)
}

fn event_name(audio: &SourceAudio, index: usize) -> Option<&str> {
    let object = match index {
        0 => return Some("GUIClick"),
        1 => "AmericaCommandCenter",
        2 | 3 => "AmericaTankCrusader",
        _ => {
            return SPEECH_EVENTS.get(index.checked_sub(4)?).copied();
        }
    };
    let unit = audio.units.iter().find(|unit| unit.name == object)?;
    let name = if index == 3 {
        unit.weapon[0].as_str()
    } else {
        unit.voices.first()?.name.as_str()
    };
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    const fn music_verification() -> super::Verification {
        super::Verification {
            index: super::EVENT_COUNT,
            requested_at: Some(0.0),
            music_due_at: None,
            finished: false,
            settled_frames: 0,
        }
    }

    #[test]
    fn slow_render_wall_time_does_not_expire_a_source_timer_or_extend_backend_wait() {
        let mut check = music_verification();
        assert!(!super::music_ready(&mut check, 1, 8.508_894).unwrap());
        assert!(!super::music_ready(&mut check, 250, 21.0).unwrap());
        assert!(!super::music_ready(&mut check, 300, 25.0).unwrap());
        assert!(check.music_due_at.is_none());
        assert!(super::music_ready(&mut check, 301, 25.0).unwrap());
        assert_eq!(
            check.music_due_at.map(f64::to_bits),
            Some(25.0_f64.to_bits())
        );
        assert!(super::music_ready(&mut check, 301, 45.0).unwrap());
        assert!(super::music_ready(&mut check, 301, 45.001).is_err());
        let mut frozen = music_verification();
        assert!(!super::music_ready(&mut frozen, 1, 90.0).unwrap());
        assert!(super::music_ready(&mut frozen, 1, 90.001).is_err());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_initial_music_requests_match_the_source_timer_readiness_boundary() {
        use super::super::music_scripts::{self, Change};
        use generals_sim::skirmish::Faction;
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        for (faction, expected) in [
            (Faction::Usa, "Game_USA_01"),
            (Faction::China, "Game_CHI_01"),
            (Faction::Gla, "Game_GLA_01"),
        ] {
            battle.start(faction);
            let mut program = music_scripts::load(&assets).unwrap();
            let mut check = music_verification();
            for frame in 0..=300 {
                assert_eq!(battle.simulation.tick, frame);
                let changes = program.advance(&battle.simulation, None);
                let tracks: Vec<_> = changes
                    .iter()
                    .filter_map(|change| {
                        if let Change::Track { name, .. } = change {
                            Some(name.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                if frame < 300 {
                    assert!(tracks.is_empty());
                } else {
                    assert_eq!(tracks, [expected]);
                }
                assert!(!super::music_ready(&mut check, frame, 25.0).unwrap());
                battle.simulation.step();
            }
            assert!(super::music_ready(&mut check, battle.simulation.tick, 25.0).unwrap());
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn native_speech_inventory_keeps_results_and_cheer_when_other_events_are_admitted() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let audio = super::super::load(
            &assets,
            &battle.simulation.catalog,
            std::path::Path::new(""),
        )
        .unwrap();
        assert!(audio.events.contains_key("EvaChina_CashStolen"));
        assert!(audio.samples["echcashs.wav"].is_none());
        for (index, name) in super::SPEECH_EVENTS.into_iter().enumerate() {
            assert_eq!(super::event_name(&audio, index + 4), Some(name));
            let event = &audio.events[name];
            assert!(event.speech);
            assert!(
                event
                    .sounds
                    .iter()
                    .all(|sample| audio.samples[sample].is_some())
            );
        }
        assert_eq!(super::event_name(&audio, super::EVENT_COUNT), None);
    }
}
