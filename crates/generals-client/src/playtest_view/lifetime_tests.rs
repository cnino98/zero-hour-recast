//! Real object lifetimes with headless ECS ownership of private animated meshes.
use super::*;
use crate::{
    model_animation::{Frame, Players},
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type},
    strategy_verification::template,
};
use bevy::{
    asset::{AssetPlugin, RenderAssetUsages},
    ecs::system::RunSystemOnce,
    render::render_resource::PrimitiveTopology,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish, visibility::Status};

fn fixture() -> App {
    lifetime_fixture(|_| {})
}

fn lifetime_fixture(configure: impl FnOnce(&mut generals_sim::skirmish::MapLayout)) -> App {
    let (assets, battle, _) = crate::cleanup_verification::tests::installed_with_layout(configure);
    let map = generals_assets::map::MapData::parse(
        &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
    )
    .unwrap();
    let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .insert_resource(battle)
        .insert_resource(terrain)
        .add_systems(Update, render);
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 3]),
    );
    app.insert_resource(Geometry {
        colors: vec![[0; 3]; 8],
        models: vec![vec![PreparedPart {
            light_bounds: None,
            submission: None,
            order: model_render::RenderOrder::ordinary(),
            binding: None,
            pickable: false,
            mesh,
            materials: vec![Handle::default()],
        }]],
        generation: u32::MAX,
        spawned: 0,
    });
    app
}

fn game(app: &mut App) -> Mut<'_, Skirmish> {
    app.world_mut()
        .resource_mut::<Match>()
        .map_unchanged(|battle| &mut battle.simulation)
}

fn root(app: &mut App, index: usize) -> Option<Entity> {
    let world = app.world_mut();
    world
        .query::<(Entity, &UnitVisual)>()
        .iter(world)
        .find_map(|(entity, visual)| (visual.index == index).then_some(entity))
}

fn roots(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&UnitVisual>().iter(world).count()
}

fn private_mesh(app: &mut App, parent: Entity) -> AssetId<Mesh> {
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  geometry: Res<Geometry>,
                  mut meshes: ResMut<Assets<Mesh>>| {
                geometry.spawn_unit_parts(
                    &mut commands,
                    parent,
                    0,
                    None,
                    &mut meshes,
                    &Frame {
                        supply: None,
                        // An animated instance owns its cloned mesh even when
                        // this tiny ownership fixture has no deforming bones.
                        meshes: Some(Vec::new()),
                        height_offset: 0.0,
                    },
                );
            },
        )
        .unwrap();
    let child = app.world().get::<Children>(parent).unwrap()[0];
    app.world().get::<Mesh3d>(child).unwrap().0.id()
}

#[ignore = "requires original game installation"]
#[test]
fn produced_suicide_corpses_retire_private_meshes_without_growing_live_drawables() {
    let mut app = fixture();
    let (barracks, target, kind) = {
        let mut game = game(&mut app);
        game.reset(Faction::Gla, Faction::Usa).unwrap();
        let builder = friendly_entity(&game, Kind::Builder, 0).unwrap();
        let barracks = construct(&mut game, builder, Kind::Barracks, 0).unwrap();
        let target = friendly_entity(&game, Kind::CommandCenter, 1).unwrap();
        // Keep the firing-range target alive across repeated real deaths. This
        // fixture checks drawable lifetime, not Command Center damage balance.
        game.entities[target].health = 100_000.0;
        game.entities[target].max_health = 100_000.0;
        // Retain a living observer through the corpse's native lifetime; the
        // dying infantry's own sight no longer reveals its position.
        let center = game.entities[target].position;
        let observation_point = [center[0] + 100.0, center[1] + 120.0];
        game.move_to(builder, observation_point).unwrap();
        advance_until(
            &mut game,
            18_000,
            "observer reaching the target area",
            |game| {
                let position = game.entities[builder].position;
                (position[0] - observation_point[0]).hypot(position[1] - observation_point[1])
                    < 15.0
            },
        )
        .unwrap();
        (
            barracks,
            target,
            template(&game, "GLAInfantryTerrorist").unwrap(),
        )
    };
    app.update();
    let baseline = roots(&mut app);
    let initial_ids = game(&mut app).entities.len();
    let mut produced = Vec::new();
    for _ in 0..8 {
        let actor = {
            let mut game = game(&mut app);
            let actor = produce_type(&mut game, barracks, kind, 0).unwrap();
            crate::terrorist_verification::approach_target(&mut game, actor, target).unwrap();
            game.attack(actor, target).unwrap();
            actor
        };
        produced.push(actor);
        app.update();
        let drawable = root(&mut app, actor).unwrap();
        let mesh = private_mesh(&mut app, drawable);
        {
            let mut game = game(&mut app);
            advance_until(&mut game, 1000, "contact suicide", |game| {
                !game.entities[actor].alive()
            })
            .unwrap();
            assert!(matches!(
                game.entities[actor].death_type(),
                generals_sim::skirmish::DeathType::Suicided
            ));
            assert!(game.entities[actor].corpse_present());
        }
        app.update();
        assert_eq!(root(&mut app, actor), Some(drawable));
        assert!(app.world().get::<Players>(drawable).is_some());
        assert_eq!(
            app.world().get::<Visibility>(drawable),
            Some(&Visibility::Visible)
        );
        {
            let mut game = game(&mut app);
            advance_until(&mut game, 1000, "native corpse destruction", |game| {
                game.entities[actor].masked()
            })
            .unwrap();
        }
        app.update();
        app.update(); // Asset tracking consumes the children's dropped handles.
        assert!(root(&mut app, actor).is_none());
        assert!(app.world().resource::<Assets<Mesh>>().get(mesh).is_none());
        // The native suicide splash can also kill the enemy's repairing Dozer.
        // Retiring that unrelated drawable is correct; each produced actor's
        // exact root/mesh removal above and the live upper bound forbid leaks.
        assert!(roots(&mut app) <= baseline);
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
    }
    check_retained_records(&mut app, initial_ids, &produced);
}

fn check_retained_records(app: &mut App, initial: usize, produced: &[usize]) {
    let game = game(app);
    assert_eq!(produced.len(), 8);
    assert!(produced.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(game.entities.len() >= initial + produced.len());
    for &id in produced {
        assert!(id >= initial);
        assert_eq!(
            game.catalog[game.entities[id].type_index].name,
            "GLAInfantryTerrorist"
        );
        assert!(game.entities[id].masked());
    }
    // Death callbacks can create further real objects (for example crates).
    // Their IDs are independent of drawable retirement and must not be erased.
    for (id, actor) in game.entities.iter().enumerate().skip(initial) {
        if !produced.contains(&id) {
            eprintln!(
                "Lifetime fixture additional native object: id={id} type={} alive={}",
                game.catalog[actor.type_index].name,
                actor.alive()
            );
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn held_live_passengers_keep_the_same_drawable_until_exit_and_restart() {
    let mut app = fixture();
    let setup = crate::transport_verification::prepare(&mut game(&mut app)).unwrap();
    let rider = setup.riders[0];
    app.update();
    let drawable = root(&mut app, rider).unwrap();
    let mesh = private_mesh(&mut app, drawable);
    for _ in 0..3 {
        app.update();
        assert_eq!(root(&mut app, rider), Some(drawable));
        assert_eq!(
            app.world().get::<Visibility>(drawable),
            Some(&Visibility::Hidden)
        );
    }
    {
        let mut game = game(&mut app);
        game.exit_container(rider).unwrap();
        advance_until(&mut game, 1000, "passenger exit", |game| {
            game.entities[rider].contained_by().is_none()
        })
        .unwrap();
    }
    app.update();
    assert_eq!(root(&mut app, rider), Some(drawable));
    assert_eq!(
        app.world().get::<Visibility>(drawable),
        Some(&Visibility::Visible)
    );
    assert!(app.world().resource::<Assets<Mesh>>().get(mesh).is_some());
    {
        let mut battle = app.world_mut().resource_mut::<Match>();
        battle
            .simulation
            .reset(Faction::Usa, Faction::China)
            .unwrap();
        battle.generation += 1;
    }
    app.update();
    app.update();
    assert!(app.world().get_entity(drawable).is_err());
    assert!(app.world().resource::<Assets<Mesh>>().get(mesh).is_none());
}

#[ignore = "requires original game installation"]
#[test]
fn fog_memory_survives_death_until_revealed_and_rubble_survives_clear_sight() {
    let mut app = lifetime_fixture(|layout| {
        let rubble = layout
            .neutral_objects
            .iter_mut()
            .find(|record| record.name == "WaterfrontWherehouse01")
            .unwrap();
        rubble.script_name = Some("RubbleTarget".into());
        let index = layout.neutral_objects.len();
        layout
            .neutral_objects
            .push(generals_sim::skirmish::NeutralObject {
                initial_health: None,
                attitude: None,
                recruitable: None,
                original_team: "teamplayer1".into(),
                script_name: Some("FogTarget".into()),
                name: "ChinaCommandCenter".into(),
                position: [1000.0, 1000.0, 0.0],
                angle: 0.0,
                script_targetable: true,
                indestructible: false,
                selectable: None,
            });
        layout
            .world_object_order
            .push(generals_sim::skirmish::MapWorldPlacement::Neutral(index));
    });
    let (observer, target) = {
        let mut game = game(&mut app);
        game.reset(Faction::Usa, Faction::China).unwrap();
        let observer = friendly_entity(&game, Kind::Builder, 0).unwrap();
        let target = game.named_map_object("FogTarget").unwrap();
        for unit in &mut game.catalog {
            unit.sight.clearing_range = 0.0;
        }
        let kind = game.entities[observer].type_index;
        game.catalog[kind].sight.clearing_range = 200.0;
        game.entities[observer].position = game.entities[target].position;
        game.configure_visibility(generals_sim::skirmish::visibility::VisibilityRules {
            cell_size: 40.0,
            unlook_persist_frames: 0,
            use_shroud: false,
        })
        .unwrap();
        assert!(matches!(game.visibility_status(0, target), Status::Clear));
        (observer, target)
    };
    app.update();
    let drawable = root(&mut app, target).unwrap();
    let pose = *app.world().get::<Transform>(drawable).unwrap();
    let mesh = private_mesh(&mut app, drawable);
    {
        let mut game = game(&mut app);
        game.entities[observer].position = [500.0, 500.0];
        for _ in 0..3 {
            game.step();
        }
        assert!(matches!(game.visibility_status(0, target), Status::Fogged));
        // The visibility record keeps the last seen building after its body is
        // gone. Rendering must keep that frozen snapshot without animation.
        // Exercise the original script damage, die modules and destruction list.
        script_kill(&mut game, "FogTarget");
        assert!(!game.entities[target].exists());
    }
    app.update();
    assert_eq!(root(&mut app, target), Some(drawable));
    assert_eq!(*app.world().get::<Transform>(drawable).unwrap(), pose);
    assert_eq!(
        app.world().get::<Visibility>(drawable),
        Some(&Visibility::Visible)
    );
    {
        let mut game = game(&mut app);
        game.entities[observer].position = game.entities[target].position;
        game.step();
        assert!(matches!(game.visibility_status(0, target), Status::Clear));
    }
    app.update();
    app.update();
    assert!(root(&mut app, target).is_none());
    assert!(app.world().resource::<Assets<Mesh>>().get(mesh).is_none());

    let rubble = {
        let mut game = game(&mut app);
        let id = game.named_map_object("RubbleTarget").unwrap();
        script_kill(&mut game, "RubbleTarget");
        game.entities[observer].position = game.entities[id].position;
        game.step();
        assert!(game.retained_rubble(id));
        assert!(matches!(game.visibility_status(0, id), Status::Clear));
        id
    };
    app.update();
    let retained = root(&mut app, rubble).unwrap();
    for _ in 0..3 {
        app.update();
        assert_eq!(root(&mut app, rubble), Some(retained));
        assert_eq!(
            app.world().get::<Visibility>(retained),
            Some(&Visibility::Visible)
        );
    }
}

fn script_kill(game: &mut Skirmish, object: &str) {
    use generals_sim::skirmish::map_scripts::{Action, Condition, Script};
    game.install_map_scripts(vec![Script {
        owner: String::new(),
        name: "KillBuilding".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::NamedKill(object.into())],
        false_actions: vec![],
    }]);
    game.step();
}

#[ignore = "requires original game installation"]
#[test]
fn a_live_rebuilding_hole_keeps_its_drawable_when_hidden_and_shown_again() {
    let mut app = fixture();
    let hole = {
        let mut game = game(&mut app);
        game.reset(Faction::Gla, Faction::Gla).unwrap();
        let worker = friendly_entity(&game, Kind::Builder, 0).unwrap();
        let barracks = construct(&mut game, worker, Kind::Barracks, 0).unwrap();
        let kind = template(&game, "GLAInfantryTerrorist").unwrap();
        let attacker = produce_type(&mut game, barracks, kind, 0).unwrap();
        let worker = friendly_entity(&game, Kind::Builder, 1).unwrap();
        let target = construct(&mut game, worker, Kind::Barracks, 1).unwrap();
        game.entities[target].health = 1.0;
        let center = game.entities[target].position;
        game.entities[attacker].position = [center[0] + 80.0, center[1]];
        game.attack(attacker, target).unwrap();
        advance_until(&mut game, 1000, "real hole exposure", |game| {
            !game.entities[target].alive()
        })
        .unwrap();
        game.entities
            .iter()
            .position(|actor| {
                actor.alive()
                    && actor.owner == 1
                    && !game.catalog[actor.type_index]
                        .maintenance
                        .rebuild
                        .is_empty()
            })
            .unwrap()
    };
    app.update();
    let drawable = root(&mut app, hole).unwrap();
    let mesh = private_mesh(&mut app, drawable);
    {
        let mut game = game(&mut app);
        advance_until(
            &mut game,
            6000,
            "hole hiding during reconstruction",
            |game| game.entities[hole].masked(),
        )
        .unwrap();
        assert!(game.entities[hole].alive());
    }
    app.update();
    assert_eq!(root(&mut app, hole), Some(drawable));
    assert_eq!(
        app.world().get::<Visibility>(drawable),
        Some(&Visibility::Hidden)
    );
    {
        let mut game = game(&mut app);
        let foundation = game
            .entities
            .iter()
            .enumerate()
            .find_map(|(id, actor)| {
                (actor.owner == 1 && actor.alive() && actor.under_construction()).then_some(id)
            })
            .unwrap();
        game.cancel_construction(1, foundation).unwrap();
        game.step();
        assert!(game.entities[hole].alive());
        assert!(!game.entities[hole].masked());
    }
    app.update();
    assert_eq!(root(&mut app, hole), Some(drawable));
    assert!(app.world().resource::<Assets<Mesh>>().get(mesh).is_some());
}
