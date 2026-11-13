use super::*;
use crate::source_audio::{Sequence, data, playback, speech_tests};
use bevy::{audio::Decodable, prelude::*};
use generals_sim::skirmish::{Faction, MapWorldPlacement, NeutralObject};

fn installed() -> (
    SourceAudio,
    crate::playtest::Match,
    crate::input::Mouse,
    [usize; 2],
) {
    let (assets, mut battle, _) =
        crate::cleanup_verification::tests::installed_with_layout(|layout| {
            for (name, x) in [("WheelOne", 1000.0), ("WheelTwo", 1020.0)] {
                let index = layout.neutral_objects.len();
                layout.neutral_objects.push(NeutralObject {
                    name: "GLAVehicleCombatBike".into(),
                    script_name: Some(name.into()),
                    position: [x, 1000.0, 0.0],
                    angle: 0.0,
                    original_team: "teamplayer1".into(),
                    initial_health: None,
                    attitude: None,
                    recruitable: None,
                    script_targetable: true,
                    indestructible: false,
                    selectable: None,
                });
                layout
                    .world_object_order
                    .push(MapWorldPlacement::Neutral(index));
            }
        });
    battle
        .simulation
        .reset(Faction::Gla, Faction::China)
        .unwrap();
    battle.started = false;
    let objects =
        ["WheelOne", "WheelTwo"].map(|name| battle.simulation.named_map_object(name).unwrap());
    let mut audio = speech_tests::audio();
    audio.units = data::units(&assets, &battle.simulation.catalog).unwrap();
    audio.events = data::definitions(&assets).unwrap();
    audio.settings = data::settings(&assets, std::path::Path::new("")).unwrap();
    audio.samples.clear();
    for name in ["RocketBuggyLand", "CombatBikeMoveStart", "HelixAmbientLoop"] {
        data::load_event(
            &assets,
            &audio.settings,
            &audio.events[name],
            &mut audio.samples,
        )
        .unwrap();
    }
    let point = battle.simulation.object_position(objects[0]);
    audio.listener = Vec3::new(point[0], point[2], -point[1]);
    let mouse = crate::input::load(
        &assets,
        std::path::Path::new(""),
        &battle.simulation.catalog,
    )
    .unwrap();
    (audio, battle, mouse, objects)
}

fn slide(audio: &mut SourceAudio, game: &Skirmish, object: usize, sliding: bool) {
    audio.wheel_sounds(game, object, false, sliding, game.entities[object].owner);
}

#[ignore = "requires original game installation"]
#[test]
fn original_wheel_one_shots_finish_after_stop_and_restart_only_on_a_new_slide() {
    let (mut audio, battle, _, [object, _]) = installed();
    let game = &battle.simulation;
    let unit = game.entities[object].type_index;
    assert_eq!(
        audio.units[unit].wheels,
        ["RocketBuggyLand", "CombatBikeMoveStart"]
    );
    audio.wheel_sounds(game, object, true, true, game.entities[object].owner);
    assert_eq!(audio.requests.len(), 2);
    assert_eq!(audio.requests[0].event, "RocketBuggyLand");
    assert_eq!(audio.requests[1].event, "CombatBikeMoveStart");
    let handle = audio.requests[1].handle.clone().unwrap();
    slide(&mut audio, game, object, true);
    assert_eq!(audio.requests.len(), 2);
    slide(&mut audio, game, object, false);
    assert_eq!(
        audio.requests.len(),
        2,
        "Miles does not cancel pending effect portions"
    );
    let request = &audio.requests[1];
    assert!(request.looping.is_none());
    let expected: usize = request
        .samples
        .iter()
        .map(|sample| sample.clone().count())
        .sum();
    let sequence = Sequence {
        samples: request.samples.clone(),
        looping: None,
    };
    assert!(expected > 0);
    assert_eq!(sequence.decoder().count(), expected);
    slide(&mut audio, game, object, true);
    assert_eq!(audio.requests.len(), 3);
    assert!(!Arc::ptr_eq(
        &handle,
        audio.requests[2].handle.as_ref().unwrap()
    ));
}

#[ignore = "requires original game installation"]
#[test]
fn stopping_a_looping_wheel_handle_preserves_other_objects_and_draw_retirement() {
    let (mut audio, battle, _, [first, second]) = installed();
    let game = &battle.simulation;
    // Exercise the same per-unit callback with an original looping declaration;
    // original Bike powerslide itself is a one-shot and is tested separately.
    audio.units[game.entities[first].type_index].wheels[1] = "HelixAmbientLoop".into();
    slide(&mut audio, game, first, true);
    slide(&mut audio, game, second, true);
    let first_loop = audio.requests[0].looping.clone().unwrap();
    let second_loop = audio.requests[1].looping.clone().unwrap();
    slide(&mut audio, game, first, false);
    assert!(first_loop.stop.load(Ordering::Acquire));
    assert!(!second_loop.stop.load(Ordering::Acquire));
    assert!(
        first_loop.decoder().count() > 0,
        "stopped pending loop still completes its initial portion"
    );
    audio.retire_wheel_sounds(second);
    assert!(
        !second_loop.stop.load(Ordering::Acquire),
        "TruckDraw destruction does not stop audio"
    );
    assert!(!audio.wheels.slides.contains_key(&second));
}

#[ignore = "requires original game installation"]
#[test]
fn wheel_loop_owner_deletion_requests_stop_and_map_reset_retires_pending_players() {
    use generals_sim::skirmish::map_scripts::{Action, Condition, Script};
    let (mut audio, battle, mouse, [first, second]) = installed();
    audio.units[battle.simulation.entities[first].type_index].wheels[1] = "HelixAmbientLoop".into();
    slide(&mut audio, &battle.simulation, first, true);
    slide(&mut audio, &battle.simulation, second, true);
    let first_stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
    let second_stop = audio.requests[1].looping.as_ref().unwrap().stop.clone();
    let mut app = App::new();
    app.init_resource::<Assets<Sequence>>()
        .init_resource::<Time<Real>>()
        .insert_resource(audio)
        .insert_resource(battle)
        .insert_resource(mouse)
        .add_systems(
            PostUpdate,
            crate::source_audio::before_audio_backend(playback::update),
        );
    app.update();
    {
        let mut battle = app.world_mut().resource_mut::<crate::playtest::Match>();
        battle.simulation.install_map_scripts(vec![Script {
            owner: String::new(),
            name: "DeleteWheelOwner".into(),
            active: true,
            one_shot: true,
            difficulty: [true; 3],
            delay_frames: 0,
            subroutine: false,
            group: None,
            conditions: vec![vec![Condition::True]],
            actions: vec![Action::NamedDelete("WheelOne".into())],
            false_actions: Vec::new(),
        }]);
        battle.simulation.step();
        assert!(!battle.simulation.entities[first].exists());
    }
    app.update();
    assert!(first_stop.load(Ordering::Acquire));
    assert!(!second_stop.load(Ordering::Acquire));
    app.world_mut()
        .resource_mut::<crate::playtest::Match>()
        .generation += 1;
    app.update();
    assert!(second_stop.load(Ordering::Acquire));
    let world = app.world_mut();
    assert_eq!(
        world.query::<&AudioPlayer<Sequence>>().iter(world).count(),
        0
    );
    assert!(world.resource::<SourceAudio>().wheels.slides.is_empty());
    assert!(world.resource::<SourceAudio>().requests.is_empty());
}
