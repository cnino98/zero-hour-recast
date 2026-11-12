//! Installed W3D state playback with real ECS parts, without a GPU or timing claim.
use super::*;
use crate::{
    playtest_view::animation_fixture,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    },
    strategy_verification::template,
};
use bevy::{asset::AssetPlugin, mesh::VertexAttributeValues};
use generals_assets::{content::ContentLine, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};

#[ignore = "requires original game installation"]
#[test]
fn paid_satellite_research_keeps_original_both_dish_upgrade_models_loaded() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    game.reset(Faction::China, Faction::Usa).unwrap();
    let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, builder, Kind::PowerPlant, 0).unwrap();
    construct(game, builder, Kind::SupplyCenter, 0).unwrap();
    construct(game, builder, Kind::Factory, 0).unwrap();
    let index = template(game, "ChinaInternetCenter").unwrap();
    let center = construct_type(game, builder, index, 0).unwrap();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &game.catalog).unwrap();
    for (upgrade, condition) in [
        ("Upgrade_ChinaSatelliteHackOne", "USER_1"),
        ("Upgrade_ChinaSatelliteHackTwo", "USER_2"),
    ] {
        crate::strategy_verification::research(game, center, upgrade).unwrap();
        let flags = game.model_flags(&game.entities[center], [0.5, 0.1]);
        assert!(flags.contains(condition));
        for draw in &animations.draws[index] {
            let state = &draw.states[draw.select(&flags)];
            if state.name.is_empty() || state.name == "none" {
                continue;
            }
            let model = &models[state.model.expect("researched building model preloaded")];
            assert!(model.meshes.iter().any(|mesh| !mesh.indices.is_empty()));
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paid_battle_bus_landing_replaces_live_parts_with_loaded_second_life_model() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = crate::battle_bus_verification::prepare(&mut battle.simulation).unwrap();
    let mut models = Vec::new();
    let mut animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>();
    let geometry =
        animation_fixture::geometry(&models, &mut app.world_mut().resource_mut::<Assets<Mesh>>());
    animations.retain_models(models, 123);
    let root = app
        .world_mut()
        .spawn((
            UnitVisual {
                index: setup.bus,
                generation: battle.generation,
            },
            Players(Vec::new()),
            Transform::IDENTITY,
            Visibility::Visible,
        ))
        .id();
    let mut clock = Clock::new();
    clock.sync_ms_for_test(battle.simulation.tick);
    app.insert_resource(geometry)
        .insert_resource(animations)
        .insert_resource(clock)
        .insert_resource(battle)
        .add_systems(Update, update);
    app.update();
    let before = identities(&app, &[root]);
    assert!(!before.is_empty());
    let tick = {
        let mut battle = app.world_mut().resource_mut::<Match>();
        let game = &mut battle.simulation;
        game.attack(setup.attacker, setup.bus).unwrap();
        advance_until(game, 7200, "paid Bus first death", |game| {
            game.entities[setup.bus].battle_bus_second_life()
        })
        .unwrap();
        assert!(
            !game
                .model_flags(&game.entities[setup.bus], [0.5, 0.1])
                .contains("SECOND_LIFE")
        );
        let base = game.entities[setup.target].position;
        game.move_to(setup.attacker, [base[0] + 700.0, base[1]])
            .unwrap();
        advance_until(game, 7200, "paid Bus second-life landing", |game| {
            game.entities[setup.bus].battle_bus_grounded()
        })
        .unwrap();
        assert!(game.entities[setup.bus].alive());
        game.tick
    };
    app.world_mut()
        .resource_mut::<Clock>()
        .sync_ms_for_test(tick);
    app.update();
    let battle = app.world().resource::<Match>();
    let animations = app.world().resource::<Animations>();
    let actor = &battle.simulation.entities[setup.bus];
    let flags = object_conditions(battle, actor, animations.damage_thresholds);
    assert!(flags.contains("SECOND_LIFE"));
    let draw = &animations.draws[actor.type_index][0];
    let state = &draw.states[draw.select(&flags)];
    assert_eq!(state.name, "uvbattbus_d1");
    let model = state
        .render_model
        .expect("landed Bus model was not preloaded");
    let children = app
        .world()
        .get::<Children>(root)
        .expect("landed Bus lost every mesh");
    assert!(!children.is_empty());
    assert!(
        before
            .iter()
            .all(|(child, _)| app.world().get_entity(*child).is_err())
    );
    let meshes = app.world().resource::<Assets<Mesh>>();
    assert!(children.iter().any(|child| {
        let part = app.world().get::<AnimatedPart>(child).unwrap();
        let mesh = meshes.get(&app.world().get::<Mesh3d>(child).unwrap().0).unwrap();
        part.model == model
            && *app.world().get::<Visibility>(child).unwrap() != Visibility::Hidden
            && matches!(mesh.attribute(Mesh::ATTRIBUTE_POSITION), Some(VertexAttributeValues::Float32x3(points)) if points.len() > 3 && points.iter().flatten().all(|value| value.is_finite()))
    }));
}

fn stationed_defenders(game: &mut Skirmish) -> Vec<usize> {
    game.reset(Faction::Usa, Faction::China).unwrap();
    let worker = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, worker, Kind::PowerPlant, 0).unwrap();
    construct(game, worker, Kind::SupplyCenter, 0).unwrap();
    let barracks = construct(game, worker, Kind::Barracks, 0).unwrap();
    let base = construct_type(game, worker, template(game, "AmericaFireBase").unwrap(), 0).unwrap();
    let kind = template(game, "AmericaInfantryMissileDefender").unwrap();
    let mut riders = Vec::new();
    for _ in 0..4 {
        let rider = produce_type(game, barracks, kind, 0).unwrap();
        game.enter(rider, base).unwrap();
        advance_until(game, 18000, "Missile Defender station entry", |game| {
            game.entities[rider].contained_by() == Some(base)
        })
        .unwrap();
        riders.push(rider);
    }
    let enemy_builder = friendly_entity(game, Kind::Builder, 1).unwrap();
    construct(game, enemy_builder, Kind::PowerPlant, 1).unwrap();
    construct(game, enemy_builder, Kind::SupplyCenter, 1).unwrap();
    let factory = construct(game, enemy_builder, Kind::Factory, 1).unwrap();
    let kind = template(game, "ChinaTankBattleMaster").unwrap();
    let target = produce_type(game, factory, kind, 1).unwrap();
    // Durability isolates repeated firing from match outcome. The target uses
    // real movement so its height, collision cells and pathfinder claims agree.
    game.entities[target].health = 100_000.0;
    game.entities[target].max_health = 100_000.0;
    stage_stationed_target(game, base, &riders, target);
    riders
}

fn stage_stationed_target(game: &mut Skirmish, base: usize, riders: &[usize], target: usize) {
    let center = game.entities[base].position;
    let footprint = game.type_for(1, Kind::CommandCenter).unwrap();
    // The renderer regression needs four clear firing lines. Placement alone
    // does not prove LOS; accept a site only after ordinary AI attack onEnter.
    for offset in [
        [200.0, 0.0],
        [0.0, 200.0],
        [-200.0, 0.0],
        [0.0, -200.0],
        [140.0, 140.0],
        [-140.0, 140.0],
        [-140.0, -140.0],
        [140.0, -140.0],
    ] {
        let wanted = [center[0] + offset[0], center[1] + offset[1]];
        if !game.can_place(footprint, wanted) {
            continue;
        }
        game.stop(base).unwrap();
        game.move_to(target, wanted).unwrap();
        advance_until(game, 18000, "durable firing target arrival", |game| {
            let position = game.entities[target].position;
            let dx = position[0] - wanted[0];
            let dy = position[1] - wanted[1];
            let squared = [dx, dy].map(|distance| distance * distance);
            squared[0] + squared[1] < 225.0 && matches!(game.entities[target].ai.order, Order::Idle)
        })
        .unwrap();
        let position = game.object_position(target);
        assert_eq!(
            game.object_transform(target).0.map(f32::to_bits),
            position.map(f32::to_bits)
        );
        assert_eq!(
            position[2].to_bits(),
            game.ground_height([position[0], position[1]]).to_bits()
        );
        game.group_attack_objects(&[base], target, false).unwrap();
        if riders.iter().all(
            |&id| matches!(game.entities[id].ai.order, Order::Attack(victim) if victim == target),
        ) {
            return;
        }
    }
    panic!("no legal nearby target site exposes all four native station firing lines");
}

fn fixture() -> (App, Vec<usize>, Vec<Entity>) {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let riders = stationed_defenders(&mut battle.simulation);
    let mut models = Vec::new();
    let mut animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>();
    let geometry =
        animation_fixture::geometry(&models, &mut app.world_mut().resource_mut::<Assets<Mesh>>());
    animations.retain_models(models, 123);
    let roots = riders
        .iter()
        .map(|&index| {
            app.world_mut()
                .spawn((
                    UnitVisual {
                        index,
                        generation: battle.generation,
                    },
                    Players(Vec::new()),
                    Transform::IDENTITY,
                    Visibility::Visible,
                ))
                .id()
        })
        .collect();
    let mut clock = Clock::new();
    clock.sync_ms_for_test(battle.simulation.tick);
    app.insert_resource(geometry)
        .insert_resource(animations)
        .insert_resource(clock)
        .insert_resource(battle)
        .add_systems(Update, update);
    (app, riders, roots)
}

fn identities(app: &App, roots: &[Entity]) -> Vec<(Entity, AssetId<Mesh>)> {
    roots
        .iter()
        .flat_map(|&root| app.world().get::<Children>(root).unwrap())
        .map(|&child| (child, app.world().get::<Mesh3d>(child).unwrap().0.id()))
        .collect()
}

fn reference_playback(
    app: &App,
    roots: &[Entity],
    players: &mut [Player],
    random: &mut playback::Random,
) {
    let battle = app.world().resource::<Match>();
    let animations = app.world().resource::<Animations>();
    for (&root, player) in roots.iter().zip(players) {
        let id = app.world().get::<UnitVisual>(root).unwrap().index;
        let actor = &battle.simulation.entities[id];
        let draw = &animations.draws[actor.type_index][0];
        let flags = object_conditions(battle, actor, animations.damage_thresholds);
        playback::select(player, draw, &animations.clips, draw.select(&flags), random);
        playback::update(
            player,
            &draw.states[player.state],
            &animations.clips,
            actor.position,
            &app.world().resource::<Clock>().time(battle.simulation.tick),
        );
    }
}

fn check_geometry(app: &App, root: Entity, reference: &Player) -> usize {
    let world = app.world();
    let battle = world.resource::<Match>();
    let animations = world.resource::<Animations>();
    let actor = &battle.simulation.entities[world.get::<UnitVisual>(root).unwrap().index];
    let player = &world.get::<Players>(root).unwrap().0[0];
    assert_eq!(player.state, reference.state);
    assert_eq!(player.pending, reference.pending);
    assert_eq!(player.choice, reference.choice);
    assert_eq!(player.frame.to_bits(), reference.frame.to_bits());
    assert_eq!(player.speed.to_bits(), reference.speed.to_bits());
    let state = &animations.draws[actor.type_index][0].states[player.state];
    let model = &animations.models[state.model.unwrap()];
    let motion = &state.motions[player.choice];
    let mut controls = turret::controls(
        &state.fields,
        actor.ai.turret.angle,
        actor.ai.turret.pitch,
        actor.ai.secondary_turret.as_ref(),
    );
    player.muzzle.controls(&mut controls);
    let mut pose = model
        .controlled_pose(
            Some(&animations.clips[&motion.name]),
            player.frame,
            &controls,
        )
        .unwrap();
    for (mesh, posed) in model.meshes.iter().zip(&mut pose) {
        posed.visible &= mesh.attributes & 0x1000 == 0;
    }
    player.muzzle.apply(&mut pose);
    for &child in world.get::<Children>(root).unwrap() {
        let part = world.get::<AnimatedPart>(child).unwrap();
        let handle = &world.get::<Mesh3d>(child).unwrap().0;
        let mesh = world.resource::<Assets<Mesh>>().get(handle).unwrap();
        let posed = &pose[part.binding.mesh];
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("original position attribute absent")
        };
        for (actual, &vertex) in points
            .iter()
            .zip(&part.binding.vertices)
            .filter(|_| posed.visible)
        {
            let [x, y, z] = posed.positions[vertex];
            assert_eq!(
                actual.map(f32::to_bits),
                [x * 0.1, z * 0.1, -y * 0.1].map(f32::to_bits)
            );
        }
        assert_eq!(
            *world.get::<Visibility>(child).unwrap(),
            if posed.visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            }
        );
    }
    player.state
}

#[ignore = "requires original game installation"]
#[test]
fn four_stationed_missile_defenders_keep_meshes_through_real_repeated_firing_states() {
    let (mut app, riders, roots) = fixture();
    let battle = app.world().resource::<Match>();
    let mut reference: Vec<_> = riders
        .iter()
        .map(|&id| {
            Player::new(
                battle.simulation.entities[id].position,
                battle.simulation.tick,
            )
        })
        .collect();
    let mut random = playback::Random::new(123);
    reference_playback(&app, &roots, &mut reference, &mut random);
    app.update();
    let initial = identities(&app, &roots);
    assert!(!initial.is_empty());
    let assets = app.world().resource::<Assets<Mesh>>().len();
    let mut states = vec![BTreeSet::new(); 4];
    let mut shots = [0; 4];
    for _ in 0..900 {
        {
            let mut battle = app.world_mut().resource_mut::<Match>();
            battle.simulation.step();
            for (count, &id) in shots.iter_mut().zip(&riders) {
                *count += battle
                    .simulation
                    .presentation
                    .shots
                    .iter()
                    .filter(|shot| shot.source == id)
                    .count();
            }
        }
        let tick = app.world().resource::<Match>().simulation.tick;
        app.world_mut()
            .resource_mut::<Clock>()
            .sync_ms_for_test(tick);
        reference_playback(&app, &roots, &mut reference, &mut random);
        app.update();
        assert_eq!(identities(&app, &roots), initial);
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), assets);
        for ((&root, reference), states) in roots.iter().zip(&reference).zip(&mut states) {
            states.insert(check_geometry(&app, root, reference));
        }
    }
    assert!(shots.iter().all(|&count| count >= 8), "shots={shots:?}");
    assert!(states.iter().all(|states| states.len() >= 2));
    let generation = app.world().resource::<Match>().generation;
    assert_eq!(
        app.world_mut()
            .resource_mut::<Animations>()
            .effect_random(generation)
            .to_bits(),
        random.real(0.0, 1.0).to_bits()
    );
    // A paused/repeated logic frame must retain both the original pose and parts.
    app.world_mut().resource_mut::<Match>().paused = true;
    for _ in 0..3 {
        app.update();
        assert_eq!(identities(&app, &roots), initial);
        for (&root, reference) in roots.iter().zip(&reference) {
            check_geometry(&app, root, reference);
        }
    }
}

fn named_state(assets: &VirtualAssets, models: &mut Vec<Model>, fields: &[&[&str]]) -> State {
    let mut state = State::empty();
    state.name = "uitech_skn".into();
    state.fields = fields
        .iter()
        .map(|line| line.iter().map(ToString::to_string).collect())
        .collect();
    let lines: Vec<_> = state
        .fields
        .iter()
        .map(|tokens| ContentLine {
            line: 1,
            tokens: tokens.clone(),
            raw: String::new(),
            assigned: true,
            depth: Some(2),
        })
        .collect();
    state.model = Some(models.len());
    models.push(crate::skirmish_data::state_model(assets, &state.name, &lines).unwrap());
    state
}

fn empty_draw() -> Draw {
    Draw {
        supply: std::collections::BTreeMap::new(),
        upgrade_show: Vec::new(),
        states: Vec::new(),
        ignored: BTreeSet::new(),
        recoil: muzzle::Rules::new(),
        receives_dynamic_lights: true,
        requires_power: false,
        attach_bone: None,
        attach_offset: [0.0; 3],
    }
}

#[ignore = "requires original game installation"]
#[test]
fn render_identity_preserves_turret_guards_module_identity_and_hidden_subobjects() {
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let mut draw = empty_draw();
    draw.states = vec![
        named_state(
            &assets,
            &mut models,
            &[
                &["HideSubObject", "20Cal", "50Cal", "RPG"],
                &["ShowSubObject", "20Cal"],
            ],
        ),
        named_state(
            &assets,
            &mut models,
            &[
                &["HideSubObject", "20Cal", "50Cal", "RPG"],
                &["ShowSubObject", "50Cal"],
            ],
        ),
        named_state(&assets, &mut models, &[&["Turret", "Dum-TurManMVR"]]),
        named_state(
            &assets,
            &mut models,
            &[&["Turret", "dum-turmanmvr"], &["AltTurretPitch", "NONE"]],
        ),
        named_state(
            &assets,
            &mut models,
            &[&["AltTurretPitch", "Dum-TurManMVR"]],
        ),
    ];
    render_objects::bind(&assets, &mut draw, &mut models).unwrap();
    assert_eq!(draw.states[0].render_model, draw.states[1].render_model);
    assert_eq!(draw.states[2].render_model, draw.states[3].render_model);
    assert_ne!(draw.states[0].render_model, draw.states[2].render_model);
    assert_ne!(draw.states[0].render_model, draw.states[4].render_model);
    let prepared = model_render::parts(&models[draw.states[0].render_model.unwrap()]).unwrap();
    for state in &draw.states[..2] {
        let model = &models[state.model.unwrap()];
        let mut pose = model.controlled_pose(None, 0.0, &[]).unwrap();
        render_objects::hide_subobjects(model, &mut pose);
        for part in &prepared {
            let binding = part.binding.as_ref().unwrap();
            assert_eq!(
                pose[binding.mesh].visible,
                model.meshes[binding.mesh].attributes & 0x1000 == 0
            );
        }
        assert!(model.meshes.iter().enumerate().any(|(index, mesh)| {
            mesh.name
                .to_ascii_lowercase()
                .ends_with(if state.model == draw.states[0].model {
                    ".20cal"
                } else {
                    ".50cal"
                })
                && pose[index].visible
                && prepared
                    .iter()
                    .any(|part| part.binding.as_ref().unwrap().mesh == index)
        }));
    }
    let mut other = empty_draw();
    other.states = vec![named_state(&assets, &mut models, &[])];
    render_objects::bind(&assets, &mut other, &mut models).unwrap();
    assert_ne!(draw.states[0].render_model, other.states[0].render_model);
}

fn visible_positions(app: &App, roots: &[Entity]) -> Vec<Vec<[u32; 3]>> {
    identities(app, roots)
        .into_iter()
        .filter(|(child, _)| *app.world().get::<Visibility>(*child).unwrap() != Visibility::Hidden)
        .map(|(_, asset)| {
            let mesh = app.world().resource::<Assets<Mesh>>().get(asset).unwrap();
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing rendered positions")
            };
            points.iter().map(|point| point.map(f32::to_bits)).collect()
        })
        .collect()
}

#[ignore = "requires original game installation"]
#[test]
fn fractional_renders_change_installed_meshes_at_one_tick_and_pause_retains_them() {
    let (mut app, _, roots) = fixture();
    app.update();
    let identity = identities(&app, &roots);
    let tick = app.world().resource::<Match>().simulation.tick;
    let before = visible_positions(&app, &roots);
    let frames: Vec<_> = roots
        .iter()
        .map(|root| app.world().get::<Players>(*root).unwrap().0[0].frame)
        .collect();
    app.world_mut().resource_mut::<Clock>().add_ms_for_test(8);
    app.update();
    assert_eq!(app.world().resource::<Match>().simulation.tick, tick);
    assert_eq!(identities(&app, &roots), identity);
    assert_ne!(
        visible_positions(&app, &roots),
        before,
        "subtick sample must deform original visible meshes"
    );
    assert!(roots.iter().zip(frames).any(|(root, frame)| {
        app.world().get::<Players>(*root).unwrap().0[0]
            .frame
            .to_bits()
            != frame.to_bits()
    }));
    let held = visible_positions(&app, &roots);
    app.world_mut().resource_mut::<Match>().paused = true;
    for _ in 0..3 {
        app.update();
        assert_eq!(visible_positions(&app, &roots), held);
        assert_eq!(identities(&app, &roots), identity);
    }
}
