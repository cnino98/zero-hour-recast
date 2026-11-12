//! Installed aircraft states must retain real prepared geometry while flying.
use super::*;

#[ignore = "requires original game installation"]
#[test]
fn installed_aircraft_flight_and_rocket_pod_states_keep_visible_prepared_geometry() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    for (name, model) in [
        ("AmericaJetRaptor", "avraptor"),
        ("ChinaJetMIG", "nvmig"),
        ("AmericaVehicleComanche", "avcomanche"),
    ] {
        let index = crate::strategy_verification::template(&battle.simulation, name).unwrap();
        let draw = &animations.draws[index][0];
        for damaged in [false, true] {
            for active in [false, true] {
                for afterburner in [false, true] {
                    let mut flags = BTreeSet::new();
                    if damaged {
                        flags.insert("REALLYDAMAGED".into());
                    }
                    if active {
                        flags.insert("JETEXHAUST".into());
                    }
                    if afterburner {
                        flags.insert("JETAFTERBURNER".into());
                    }
                    if name == "AmericaVehicleComanche" && active {
                        flags.insert("WEAPONSET_PLAYER_UPGRADE".into());
                    }
                    let state = &draw.states[draw.select(&flags)];
                    assert_eq!(
                        state.name,
                        format!("{model}{}", if damaged { "_d" } else { "" })
                    );
                    visible_parts(&animations, &models, state, &flags);
                }
            }
        }
    }
}

fn visible_parts(
    animations: &Animations,
    models: &[Model],
    state: &State,
    flags: &BTreeSet<String>,
) {
    let source = &models[state.model.expect("selected aircraft state was not loaded")];
    let prepared = &models[state.render_model.expect("aircraft render state missing")];
    let clip = state
        .motions
        .first()
        .map(|motion| &animations.clips[&motion.name]);
    let parts = model_render::parts(prepared).unwrap();
    for frame in [0.0, 0.5] {
        let mut pose = source.controlled_pose(clip, frame, &[]).unwrap();
        render_objects::hide_subobjects(source, &mut pose);
        assert!(
            parts.iter().any(|part| {
                let binding = part.binding.as_ref().unwrap();
                let geometry = &pose[binding.mesh];
                geometry.visible
                    && part.material.parameters.opacity > 0.0
                    && !binding.vertices.is_empty()
                    && binding.vertices.iter().all(|&vertex| {
                        geometry.positions[vertex]
                            .iter()
                            .all(|value| value.is_finite())
                    })
            }),
            "{} {flags:?} must retain visible authored aircraft parts",
            state.name
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paid_airfield_unit_and_research_queues_select_original_production_animation() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let game = &mut battle.simulation;
    let setup = crate::aircraft_verification::prepare(game).unwrap();
    game.step();
    let kind = game.entities[setup.airfield].type_index;
    let draw = animations.draws[kind]
        .iter()
        .find(|draw| {
            draw.states.iter().any(|state| {
                state
                    .conditions
                    .iter()
                    .any(|flags| flags.contains("ACTIVELY_CONSTRUCTING"))
            })
        })
        .unwrap();
    let flags = game.model_flags(&game.entities[setup.airfield], animations.damage_thresholds);
    assert_eq!(draw.states[draw.select(&flags)].name, "none");
    let product = crate::strategy_verification::template(game, "AmericaVehicleComanche").unwrap();
    let research = game.catalog[kind]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_ComancheRocketPods")
        .unwrap();
    for upgrade in [false, true] {
        let cash = game.players[0].cash;
        if upgrade {
            game.queue_upgrade(setup.airfield, research).unwrap();
        } else {
            game.train(setup.airfield, product).unwrap();
        }
        assert!(game.players[0].cash < cash);
        assert!(!game.entities[setup.airfield].actively_constructing());
        game.step();
        let flags = game.model_flags(&game.entities[setup.airfield], animations.damage_thresholds);
        let state = &draw.states[draw.select(&flags)];
        assert_eq!(state.name, "abarfrccmd_a9");
        assert_eq!(state.mode, "LOOP");
        visible_parts(&animations, &models, state, &flags);
        let clip = &animations.clips[&state.motions[0].name];
        let first = door_geometry(&animations, &models, state, 0.0, "paid airfield production");
        let middle = door_geometry(
            &animations,
            &models,
            state,
            f32::from(clip.frame_count - 1) * 0.5,
            "paid airfield production",
        );
        assert_ne!(
            first, middle,
            "original production loop must animate real geometry"
        );
        if upgrade {
            game.cancel_upgrade(0, setup.airfield, research).unwrap();
        } else {
            let entry = game.entities[setup.airfield].queue[0].id;
            game.cancel_production(0, setup.airfield, entry).unwrap();
        }
        assert!(game.entities[setup.airfield].actively_constructing());
        game.step();
        let flags = game.model_flags(&game.entities[setup.airfield], animations.damage_thresholds);
        assert_eq!(draw.states[draw.select(&flags)].name, "none");
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_airfield_all_four_doors_keep_geometry_through_open_wait_and_close() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let index =
        crate::strategy_verification::template(&battle.simulation, "AmericaAirfield").unwrap();
    for door in 1..=4 {
        let opening = format!("DOOR_{door}_OPENING");
        let matching: Vec<_> = animations.draws[index]
            .iter()
            .filter(|draw| {
                draw.states.iter().any(|state| {
                    state
                        .conditions
                        .iter()
                        .any(|flags| flags.contains(&opening))
                })
            })
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "door {door} must use its actual Draw module"
        );
        let draw = matching[0];
        for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
            door_sequence(&animations, &models, draw, door, damage);
        }
    }
}

fn door_sequence(
    animations: &Animations,
    models: &[Model],
    draw: &data::Draw,
    door: u32,
    damage: Option<&str>,
) {
    let idle = &draw.states[draw.select(&BTreeSet::new())];
    let closed = door_geometry(animations, models, idle, 0.0, &format!("door={door} base"));
    let mut player = Player::new([0.0; 2], 0);
    let mut random = playback::Random::new(0);
    let mut tick = 0;
    for phase in [
        None,
        Some("OPENING"),
        Some("WAITING_OPEN"),
        Some("CLOSING"),
        None,
    ] {
        let mut flags: BTreeSet<String> = damage.into_iter().map(str::to_owned).collect();
        if let Some(phase) = phase {
            flags.insert(format!("DOOR_{door}_{phase}"));
        }
        playback::select(
            &mut player,
            draw,
            &animations.clips,
            draw.select(&flags),
            &mut random,
        );
        let state = &draw.states[player.state];
        let context = format!(
            "door={door} damage={damage:?} phase={phase:?} state={} name={} conditions={:?}",
            player.state, state.name, state.conditions
        );
        if damage == Some("REALLYDAMAGED") && phase.is_none() {
            // The installed day idle aliases omit REALLYDAMAGED without
            // RUBBLE/NIGHT. Native SparseMatchFinder chooses the authored
            // AWAITING_CONSTRUCTION REALLYDAMAGED Model=NONE partial match.
            assert_eq!(state.name, "none", "{context}");
            assert!(
                state.model.is_none() && state.render_model.is_none(),
                "{context}"
            );
            tick += 300;
            continue;
        }
        playback::update(
            &mut player,
            state,
            &animations.clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(tick),
        );
        let initial = door_geometry(animations, models, state, player.frame, &context);
        tick += 300;
        playback::update(
            &mut player,
            state,
            &animations.clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(tick),
        );
        let settled = door_geometry(animations, models, state, player.frame, &context);
        if phase == Some("WAITING_OPEN") {
            let clip = &animations.clips[&state.motions[player.choice].name];
            assert_eq!(
                player.frame.to_bits(),
                f32::from(clip.frame_count - 1).to_bits(),
                "{context}"
            );
            assert_eq!(
                initial, settled,
                "{context}: manual open door must hold its last pose"
            );
            assert_ne!(settled, closed, "{context}: door must visibly open");
        } else if phase == Some("CLOSING") || phase.is_none() {
            assert_eq!(
                settled, closed,
                "{context}: door must return to its closed geometry"
            );
        }
    }
}

fn door_geometry(
    animations: &Animations,
    models: &[Model],
    state: &State,
    frame: f32,
    context: &str,
) -> Vec<Vec<u8>> {
    let source = &models[state
        .model
        .unwrap_or_else(|| panic!("{context}: selected door state was not preloaded"))];
    let prepared = &models[state
        .render_model
        .unwrap_or_else(|| panic!("{context}: selected door render model absent"))];
    let clip = &animations.clips[&state.motions[0].name];
    let mut pose = source.controlled_pose(Some(clip), frame, &[]).unwrap();
    render_objects::hide_subobjects(source, &mut pose);
    let visible: Vec<_> = model_render::parts(prepared)
        .unwrap()
        .into_iter()
        .filter_map(|mut part| {
            let binding = part.binding.as_ref().unwrap();
            let visible = model_render::deform(&mut part.mesh, binding, &pose).unwrap();
            (visible && !binding.vertices.is_empty() && part.material.parameters.opacity > 0.0)
                .then(|| part.mesh.create_packed_vertex_buffer_data())
        })
        .collect();
    assert!(
        !visible.is_empty(),
        "{context}: selected door pose must retain real visible geometry"
    );
    visible
}

#[ignore = "requires original game installation"]
#[test]
fn installed_scout_body_and_rotor_keep_authored_models_and_original_scale() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let kind =
        crate::strategy_verification::template(&battle.simulation, "AmericaVehicleScoutDrone")
            .unwrap();
    let draw = &animations.draws[kind][0];
    for damaged in [false, true] {
        let mut flags = BTreeSet::new();
        if damaged {
            flags.insert("REALLYDAMAGED".into());
        }
        let state = &draw.states[draw.select(&flags)];
        let name = if damaged { "avscoutdr_d" } else { "avscoutdr" };
        assert_eq!(state.name, name);
        assert_eq!(state.mode, "LOOP");
        assert_eq!(state.motions[0].name, format!("{name}.{name}"));
        visible_parts(&animations, &models, state, &flags);
        let source = &models[state.model.unwrap()];
        let prepared = &models[state.render_model.unwrap()];
        let mut triangles = 0;
        for part in model_render::parts(prepared).unwrap() {
            let binding = part.binding.unwrap();
            let bevy::mesh::VertexAttributeValues::Float32x3(points) =
                part.mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!("original Scout body positions missing");
            };
            for (point, &vertex) in points.iter().zip(&binding.vertices) {
                let original = source.meshes[binding.mesh].positions[vertex];
                let expected = [original[0] * 0.1, original[2] * 0.1, -original[1] * 0.1];
                assert_eq!(point.map(f32::to_bits), expected.map(f32::to_bits));
            }
            triangles += points
                .chunks_exact(3)
                .filter(|triangle| {
                    let a = Vec3::from_array(triangle[0]);
                    let b = Vec3::from_array(triangle[1]);
                    let c = Vec3::from_array(triangle[2]);
                    (b - a).cross(c - a).length_squared() > 0.0
                })
                .count();
        }
        assert!(
            triangles > 0,
            "{name} must contain real original geometry, not an empty visible part"
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_china_hangars_preserve_three_authored_doors_and_no_invented_second_door() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let index =
        crate::strategy_verification::template(&battle.simulation, "ChinaAirfield").unwrap();
    for door in 1..=4 {
        let opening = format!("DOOR_{door}_OPENING");
        let matching: Vec<_> = animations.draws[index]
            .iter()
            .filter(|draw| {
                draw.states.iter().any(|state| {
                    state
                        .conditions
                        .iter()
                        .any(|flags| flags.contains(&opening))
                })
            })
            .collect();
        if door == 2 {
            // Installed ChinaAirfield explicitly comments out its second door draw.
            assert!(matching.is_empty());
        } else {
            assert_eq!(matching.len(), 1);
            for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                door_sequence(&animations, &models, matching[0], door, damage);
            }
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_mig_state_particles_keep_original_wingtips_and_damaged_engine_bones() {
    let (assets, _battle) = crate::cleanup_verification::tests::installed();
    let files =
        [
            generals_assets::content::scan(assets.read("data/ini/object/chinaair.ini").unwrap())
                .unwrap(),
        ];
    let object = crate::skirmish_data::definition(&files, "Object", "ChinaJetMIG").unwrap();
    let lines = crate::skirmish_data::blocks(&object.lines, 0, "Draw").unwrap()[0];
    let draw = crate::model_animation::data::parse(lines).unwrap();
    let binding = state_particles::particle_draw(lines).unwrap();
    assert!(!binding.animated);
    for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
        for afterburner in [false, true] {
            let mut flags: std::collections::BTreeSet<String> =
                damage.into_iter().map(str::to_owned).collect();
            // Moving airborne JetAIUpdate supplies exhaust independently of
            // afterburner; ChinaAir.ini's afterburner states require both.
            flags.insert("JETEXHAUST".into());
            if afterburner {
                flags.insert("JETAFTERBURNER".into());
            }
            let selected = draw.select(&flags);
            let pairs: Vec<_> = binding.bones[selected]
                .iter()
                .zip(&binding.states[selected])
                .map(|(bone, name)| (bone.as_str(), name.as_str()))
                .collect();
            assert!(pairs.contains(&("Wingtip01", "JetContrail")));
            assert!(pairs.contains(&("Wingtip02", "JetContrail")));
            if afterburner {
                // ChinaAir.ini authors the damaged model/bones only for
                // REALLYDAMAGED; DAMAGED retains the normal sparse match.
                let bones = if damage == Some("REALLYDAMAGED") {
                    ["Engine01", "Engine02"]
                } else {
                    ["Exhaust01", "Exhaust02"]
                };
                for bone in bones {
                    assert!(pairs.contains(&(bone, "JetExhaust")));
                }
                assert_eq!(pairs.len(), 4);
            } else {
                assert_eq!(pairs.len(), 2);
            }
        }
    }
}
