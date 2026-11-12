//! Original parsed POST model plus the actual script/DIE/scheduler event producer.
use super::*;
use crate::{playtest::MAP_NAME, terrain::Terrain, world_data};
use generals_assets::{
    map::{MapData, MapObject, MapValue},
    w3d::Model,
};
use generals_sim::skirmish::{
    Skirmish, WeaponFxEvent,
    map_scripts::{Action, Condition, Script},
};

fn prepared() -> (Skirmish, Particles, Animations, Model) {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut map = MapData::parse(&assets.read(MAP_NAME).unwrap().bytes).unwrap();
    map.objects.push(MapObject {
        template: "MogadishuHighrise01".into(),
        position: [400.0, 400.0, 0.0],
        angle: 0.0,
        flags: 0,
        properties: BTreeMap::from([
            (
                "originalOwner".into(),
                MapValue::Ascii("teamPlyrCivilian".into()),
            ),
            (
                "objectName".into(),
                MapValue::Ascii("NativeBodyCollapse".into()),
            ),
        ]),
    });
    let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
    let settings = world_data::camera_settings(&assets, root).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let mut animations =
        crate::model_animation::load(&assets, &mut data.models, &data.types).unwrap();
    animations.bind_logical_bones(&mut data.types);
    crate::skirmish_data::structure_death::bind_bones(&mut data.types, &data.models, &animations);
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations).unwrap();
    let particles = super::super::data::load(&assets, &data.types).unwrap();
    let mut game = data.into_simulation(movement, layout).unwrap();
    terrain
        .configure_projectiles(&mut game, projectiles.partition_cell_size)
        .unwrap();
    let model = Model::parse(&assets.read("art/w3d/cbmogdis01_r.w3d").unwrap().bytes).unwrap();
    (game, particles, animations, model)
}
fn command(game: &mut Skirmish, action: Action) {
    game.install_map_scripts(vec![Script {
        owner: "player0".into(),
        name: "BodyNativeCommand".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![action],
        false_actions: Vec::new(),
    }]);
}
fn consume(particles: &mut Particles, game: &Skirmish, animations: &mut Animations) {
    for event in &game.presentation.weapon_fx {
        match event {
            WeaponFxEvent::BodyParticles(index) => {
                particles.body_particle_event(game, 1, *index, animations);
            }
            WeaponFxEvent::BodyParticlesClear(object) => particles.clear_body_particles(*object),
            _ => {}
        }
    }
    particles.object_effects(game, &mut super::super::effects::rng::source(animations, 1));
    particles.logic(
        game.tick,
        &mut super::super::random(animations, 1),
        &|point| game.ground_height(point),
    );
}
#[ignore = "requires original game installation"]
#[test]
fn original_post_model_emits_four_attached_smoke_systems_and_direct_delete_retires_them() {
    let (mut game, mut particles, mut animations, post_model) = prepared();
    let id = game.named_map_object("NativeBodyCollapse").unwrap();
    command(&mut game, Action::NamedKill("NativeBodyCollapse".into()));
    let mut post = None;
    for _ in 0..1000 {
        game.step();
        consume(&mut particles, &game, &mut animations);
        if game.entities[id].post_collapse() {
            let event = game
                .presentation
                .body_particles
                .iter()
                .find(|event| event.pose.object == id)
                .expect("source completion body refresh");
            assert!(
                !game
                    .model_flags(&game.entities[id], [0.5, 0.1])
                    .contains("RUBBLE")
            );
            assert_eq!(event.positions("FireS").len(), 0);
            let expected: Vec<_> = (1..=3)
                .map(|index| {
                    post_model
                        .posed_bone_matrix(&format!("SMOKES{index:02}"), None, 0.0)
                        .unwrap()
                        .unwrap()
                        .map(|row| row[3].to_bits())
                })
                .collect();
            assert_eq!(
                event
                    .positions("SmokeS")
                    .into_iter()
                    .map(|point| point.map(f32::to_bits))
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                event.positions("SmokeM")[0].map(f32::to_bits),
                post_model
                    .posed_bone_matrix("SMOKEM01", None, 0.0)
                    .unwrap()
                    .unwrap()
                    .map(|row| row[3].to_bits())
            );
            post = Some(particles.bodies.systems[&id].clone());
            break;
        }
    }
    let ids = post.expect("source scheduled completion callback");
    assert_eq!(ids.len(), 4);
    let names: Vec<_> = ids
        .iter()
        .map(|id| {
            let emitter = particles
                .emitters
                .iter()
                .find(|emitter| emitter.id == *id)
                .unwrap();
            assert!(!emitter.lifecycle.destroyed);
            assert_eq!(
                emitter.object_attachment.as_ref().unwrap().0,
                game.named_map_object("NativeBodyCollapse").unwrap()
            );
            particles.templates[emitter.template].name.as_str()
        })
        .collect();
    assert_eq!(
        names,
        [
            "SmokeFactionSmall",
            "SmokeFactionSmall",
            "SmokeFactionSmall",
            "SmokeFactionMedium"
        ]
    );
    delete_and_retire(&mut game, &mut particles, &mut animations, id, &ids);
}

fn delete_and_retire(
    game: &mut Skirmish,
    particles: &mut Particles,
    animations: &mut Animations,
    object: usize,
    ids: &[u64],
) {
    command(game, Action::NamedDelete("NativeBodyCollapse".into()));
    game.step();
    consume(particles, game, animations);
    assert!(!game.entities[object].exists());
    assert!(!particles.bodies.systems.contains_key(&object));
    // ParticleSystem::update retires destroyed systems once their live particles
    // are gone; an empty system can already be absent after this logic frame.
    assert!(ids.iter().all(|id| {
        particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == *id)
            .is_none_or(|emitter| emitter.lifecycle.destroyed)
    }));
    for _ in 0..300 {
        game.step();
        consume(particles, game, animations);
        if particles
            .emitters
            .iter()
            .all(|emitter| !ids.contains(&emitter.id))
        {
            break;
        }
    }
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| !ids.contains(&emitter.id))
    );
}
