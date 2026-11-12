//! Installed weapon FX sounds must be loaded before the real ordered callback.
use super::*;
use crate::{
    cleanup_verification, demo_trap_verification, disarm_verification,
    particles::{self, ParticleInspection, ParticleVisual},
    playtest::Match,
    source_material::SourceMaterial,
    weapon_fx,
};
use bevy::ecs::system::SystemState;
use std::path::Path;

fn fixture() -> App {
    let (assets, battle, animations) = cleanup_verification::tests::installed_with_animations();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let audio = load(&assets, &battle.simulation.catalog, &options).unwrap();
    let particles = particles::load(&assets, &battle.simulation.catalog).unwrap();
    let weapons = weapon_fx::load(&assets, &battle.simulation.catalog).unwrap();
    let lasers = crate::laser_beam::load(&assets, &battle.simulation.catalog).unwrap();
    let selection = crate::selection_feedback::load(&assets).unwrap();
    let mut app = App::new();
    app.register_required_components::<Mesh3d, Visibility>();
    app.insert_resource(battle)
        .insert_resource(animations)
        .insert_resource(audio)
        .insert_resource(particles)
        .insert_resource(weapons)
        .insert_resource(lasers)
        .insert_resource(selection)
        .insert_resource(crate::tree_buffer::Trees::new(
            [1000.0; 2],
            crate::tree_buffer::Lighting {
                ambient: [1.0; 3],
                colors: [[0.0; 3]; 3],
                directions: [[0.0, 0.0, -1.0]; 3],
            },
        ))
        .insert_resource(crate::playtest_view::WorldObjects(Vec::new()))
        .init_resource::<crate::structure_view::Structures>()
        .init_resource::<Time>()
        .init_resource::<Time<Fixed>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<SourceMaterial>>()
        .add_systems(
            Update,
            (weapon_fx::record, particles::step, particles::update).chain(),
        );
    app.world_mut()
        .spawn((crate::playtest_view::WorldCamera, Transform::IDENTITY));
    app
}

fn assert_request(audio: &SourceAudio, name: &str, position: [f32; 3]) {
    let requests: Vec<_> = audio
        .requests
        .iter()
        .filter(|request| request.event == name)
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "missing or duplicated original {name} callback"
    );
    let request = requests[0];
    assert_eq!(request.object, None);
    assert_eq!(
        request.position.unwrap().to_array().map(f32::to_bits),
        [position[0], position[2], -position[1]].map(f32::to_bits)
    );
    assert!(!request.samples.is_empty());
    assert!(!request.sample_names.is_empty());
    assert!(
        request
            .sample_names
            .iter()
            .all(|name| audio.samples.get(name).is_some_and(Option::is_some))
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_weapon_fx_dependencies_queue_the_authored_positional_sounds_only_when_fired() {
    let (assets, battle) = cleanup_verification::tests::installed();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let mut audio = load(&assets, &battle.simulation.catalog, &options).unwrap();
    assert!(audio.requests.is_empty());
    for name in [
        "MineClearedByDozer",
        "MineClearedByWorker",
        "DemoTrapExplosion",
    ] {
        assert!(
            audio.events.contains_key(name),
            "represented FX sound absent: {name}"
        );
    }
    let unused = data::definitions(&assets)
        .unwrap()
        .into_keys()
        .find(|name| !audio.events.contains_key(name))
        .unwrap();
    *audio.random.lock().unwrap() = random::Random::new(37);
    audio.fx(&unused, [10.0, 20.0, 30.0]);
    assert!(audio.requests.is_empty());
    assert_eq!(
        audio.random.lock().unwrap().index(10_000),
        random::Random::new(37).index(10_000)
    );
    for name in ["MineClearedByDozer", "DemoTrapExplosion"] {
        audio.fx(name, [10.0, 20.0, 30.0]);
        assert_request(&audio, name, [10.0, 20.0, 30.0]);
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paid_unupgraded_fighter_missile_crash_fx_has_loaded_audio_before_delayed_death() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    let mut audio = load(&assets, &battle.simulation.catalog, Path::new("")).unwrap();
    let setup = crate::stealth_fighter_verification::prepare(&mut battle.simulation).unwrap();
    assert!(
        !battle
            .simulation
            .has_upgrade(0, "Upgrade_AmericaBunkerBusters")
    );
    let fighter = &battle.simulation.catalog[battle.simulation.entities[setup.aircraft].type_index];
    let kind = fighter
        .weapon_template(0)
        .unwrap()
        .motion
        .as_ref()
        .unwrap()
        .projectile_type
        .unwrap();
    let rules = &battle.simulation.catalog[kind].behavior.bunker_busters[0];
    let crash = rules.crash_fx.clone();
    assert_eq!(crash, "WeaponFX_BunkerBusterIntialImpact");
    assert!(audio.object_fx.contains_key(&rules.detonation_fx));
    let sounds = audio.object_fx.get(&crash).unwrap().clone();
    assert!(sounds.iter().all(|name| audio.events.contains_key(name)));
    assert!(audio.requests.is_empty());
    crate::stealth_fighter_verification::approach(
        &mut battle.simulation,
        setup.aircraft,
        setup.target,
    )
    .unwrap();
    battle
        .simulation
        .attack(setup.aircraft, setup.target)
        .unwrap();
    for _ in 0..9000 {
        battle.simulation.step();
        let Some(event) = battle
            .simulation
            .presentation
            .object_fx_events
            .iter()
            .find(|event| {
                event.effect == crash
                    && event.visible_players & 1 != 0
                    && event.clear_cells & 1 != 0
            })
        else {
            continue;
        };
        let missile = &battle.simulation.entities[event.object];
        assert_eq!(missile.type_index, kind);
        assert_eq!(missile.producer(), Some(setup.aircraft));
        assert!(missile.projectile_id().is_some());
        assert_eq!(event.owner, 0);
        audio.object_fx(event, 0);
        assert_eq!(audio.requests.len(), sounds.len());
        for name in &sounds {
            assert_request(&audio, name, event.position);
        }
        return;
    }
    panic!("unupgraded Fighter never emitted its visible delayed-death crash FX");
}

#[ignore = "requires original game installation"]
#[test]
fn paid_dozer_fire_fx_emits_sound_and_three_renderable_systems_at_the_capture_frame() {
    let mut app = fixture();
    let setup = {
        let mut battle = app.world_mut().resource_mut::<Match>();
        let setup = disarm_verification::prepare(&mut battle.simulation).unwrap();
        battle.simulation.attack(setup.builder, setup.trap).unwrap();
        setup
    };
    let mut trace = disarm_verification::Trace::new();
    for _ in 0..1800 {
        {
            let mut battle = app.world_mut().resource_mut::<Match>();
            battle.simulation.step();
            trace.record(&battle.simulation, &setup).unwrap();
        }
        app.update();
        if trace
            .shot
            .as_ref()
            .is_some_and(|shot| app.world().resource::<Match>().simulation.tick == shot.frame + 2)
        {
            break;
        }
    }
    let shot = trace.shot.as_ref().expect("paid Dozer did not fire");
    let battle = app.world().resource::<Match>();
    assert_eq!(battle.simulation.tick, shot.frame + 2);
    disarm_verification::verify_removed(&battle.simulation, &setup).unwrap();
    let kind = battle.simulation.entities[setup.builder].type_index;
    let generation = battle.generation;
    assert_request(
        app.world().resource::<SourceAudio>(),
        "MineClearedByDozer",
        setup.position,
    );
    // Device-free geometry check; the native capture still proves actual camera/GPU visibility.
    let mut visuals = app
        .world_mut()
        .query_filtered::<&mut ViewVisibility, With<ParticleVisual>>();
    for mut visible in visuals.iter_mut(app.world_mut()) {
        *visible = ViewVisibility::VISIBLE;
    }
    let mut inspector = SystemState::<ParticleInspection>::new(app.world_mut());
    assert!(
        inspector
            .get(app.world())
            .unwrap()
            .weapon_fire(kind, shot.weapon, shot.veterancy, shot.frame, generation)
            .unwrap()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn paid_manual_and_proximity_trap_final_weapon_fx_queue_the_original_explosion() {
    let mut damage_effects = std::collections::BTreeSet::new();
    for mode in [0, 1] {
        let mut app = fixture();
        let (setup, position) = {
            let mut battle = app.world_mut().resource_mut::<Match>();
            let setup = demo_trap_verification::prepare(&mut battle.simulation).unwrap();
            let position = battle.simulation.object_position(setup.trap);
            battle.simulation.switch_weapon(setup.trap, mode).unwrap();
            (setup, position)
        };
        let mut fired = false;
        for _ in 0..180 {
            {
                let mut battle = app.world_mut().resource_mut::<Match>();
                battle.simulation.step();
                fired |= battle.simulation.presentation.shots.iter().any(|shot| {
                    shot.source == setup.trap
                        && battle.simulation.catalog
                            [battle.simulation.entities[setup.trap].type_index]
                            .weapon_template(shot.weapon_index)
                            .is_some_and(|weapon| weapon.name == "DemoTrapDetonationWeapon")
                });
            }
            {
                let battle = app.world().resource::<Match>();
                damage_effects.extend(
                    battle
                        .simulation
                        .presentation
                        .object_fx_events
                        .iter()
                        .map(|event| event.effect.clone()),
                );
            }
            app.update();
            if fired {
                break;
            }
        }
        assert!(
            fired,
            "paid trap mode {mode} did not execute its original final weapon"
        );
        assert_request(
            app.world().resource::<SourceAudio>(),
            "DemoTrapExplosion",
            position,
        );
    }
    for effect in ["FX_DamageTankStruck", "FX_DamageInfantryStruck"] {
        assert!(
            damage_effects.contains(effect),
            "missing actual damage callback {effect}"
        );
    }
}
