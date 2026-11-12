use super::*;
use crate::{
    model_animation,
    playtest_view::animation_fixture,
    skirmish_verification::{advance_until, friendly_entity, produce_type},
    strategy_verification::{research, template},
};
use bevy::asset::AssetPlugin;
use generals_assets::map::MapData;
use generals_sim::skirmish::Kind;

#[ignore = "requires original game installation"]
#[test]
fn installed_chemical_suits_retain_original_shadow_dimensions_and_offsets() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let data = load(&assets, &battle.simulation.catalog).unwrap();
    for name in [
        "AmericaInfantryRanger",
        "AmericaInfantryMissileDefender",
        "AmericaInfantryPathfinder",
    ] {
        let index = template(&battle.simulation, name).unwrap();
        let decal = data.templates[index].as_ref().unwrap();
        // Installed AmericaInfantry.ini: all three chemical recipients author
        // 14x14 shadows and omit both offsets (ThingTemplate defaults to zero).
        assert!(decal.chemical_suit, "{name}");
        assert_eq!(decal.size, Vec2::splat(14.0), "{name}");
        assert_eq!(decal.offset, Vec2::ZERO, "{name}");
    }
    assert_eq!(TEXTURES[4], "EXChemSuit");
}

fn visibility(app: &mut App, id: usize) -> Visibility {
    let world = app.world_mut();
    let mut decals = world.query::<(&Decal, &Visibility)>();
    let (decal, visible) = decals
        .iter(world)
        .find(|(decal, _)| decal.object == id)
        .unwrap();
    assert_eq!(decal.texture, 4);
    assert_eq!(decal.size, Vec2::splat(14.0));
    let material = world
        .resource::<Assets<SourceMaterial>>()
        .get(&decal.material)
        .unwrap();
    assert_eq!(material.parameters.opacity.to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        material.texture.as_ref(),
        world.resource::<HordeDecals>().handles.get(4)
    );
    *visible
}

fn prepared_actors() -> (VirtualAssets, Match, usize, usize) {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    crate::usa_research_verification::verify(&mut battle.simulation).unwrap();
    let game = &mut battle.simulation;
    let pathfinder = game
        .entities
        .iter()
        .position(|actor| {
            actor.alive()
                && actor.owner == 0
                && actor.chemical_suit()
                && game.catalog[actor.type_index].name == "AmericaInfantryPathfinder"
        })
        .unwrap();
    let factory = friendly_entity(game, Kind::Factory, 0).unwrap();
    let humvee = template(game, "AmericaVehicleHumvee").unwrap();
    let cost = game.catalog[humvee].cost + 100;
    advance_until(
        game,
        18_000,
        "funding original detector for decal regression",
        |game| game.players[0].cash >= cost,
    )
    .unwrap();
    let parent = produce_type(game, factory, humvee, 0).unwrap();
    research(game, parent, "Upgrade_AmericaScoutDrone").unwrap();
    let scout = game.produced_drone(parent).unwrap();
    game.set_detection_enabled(scout, false);
    game.stop(pathfinder).unwrap();
    advance_until(
        game,
        180,
        "stationary upgraded Pathfinder concealment",
        |game| game.entities[pathfinder].stealthed() && !game.entities[pathfinder].detected(),
    )
    .unwrap();
    assert!(game.entities[pathfinder].chemical_suit());
    assert!(game.logically_visible(0, pathfinder));

    // Normal movement updates both original spatial partitions before scanning.
    let origin = game.entities[friendly_entity(game, Kind::CommandCenter, 0).unwrap()].position;
    game.move_to(pathfinder, [origin[0] + 150.0, origin[1]])
        .unwrap();
    advance_until(
        game,
        18_000,
        "Pathfinder moving to clear decal staging",
        |game| {
            !game.entities[pathfinder].moving()
                && matches!(
                    game.entities[pathfinder].ai.order,
                    generals_sim::skirmish::Order::Idle
                )
        },
    )
    .unwrap();
    let point = game.entities[pathfinder].position;
    game.move_to(parent, [point[0] + 60.0, point[1]]).unwrap();
    advance_until(
        game,
        18_000,
        "Scout parent moving to decal staging",
        |game| {
            !game.entities[parent].moving()
                && matches!(
                    game.entities[parent].ai.order,
                    generals_sim::skirmish::Order::Idle
                )
                && game.entities[pathfinder].stealthed()
        },
    )
    .unwrap();
    // Presentation fixture changes only owner to exercise the already-upgraded
    // enemy look; this is not acceptance of an ownership ability.
    game.entities[pathfinder].owner = 1;
    game.step();
    assert!(game.entities[pathfinder].stealthed());
    assert!(!game.logically_visible(0, pathfinder));
    assert!(matches!(
        game.visibility_status(0, pathfinder),
        Status::Clear | Status::Partial
    ));

    (assets, battle, pathfinder, scout)
}

fn fixture(assets: &VirtualAssets, battle: Match, pathfinder: usize) -> App {
    let map = MapData::parse(&assets.read(crate::playtest::MAP_NAME).unwrap().bytes).unwrap();
    let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let data = load(assets, &battle.simulation.catalog).unwrap();
    let mut models = Vec::new();
    let mut animations =
        model_animation::load(assets, &mut models, &battle.simulation.catalog).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<SourceMaterial>();
    let geometry =
        animation_fixture::geometry(&models, &mut app.world_mut().resource_mut::<Assets<Mesh>>());
    animations.retain_models(models, 123);
    app.world_mut().spawn((
        UnitVisual {
            index: pathfinder,
            generation: battle.generation,
        },
        Players(Vec::new()),
        Transform::IDENTITY,
        Visibility::Hidden,
    ));
    app.insert_resource(battle)
        .insert_resource(terrain)
        .insert_resource(data)
        .insert_resource(animations)
        .insert_resource(geometry)
        .insert_resource(model_animation::Clock::new())
        .add_systems(Update, (model_animation::update, update).chain());
    app.update();
    // Newly allocated decal material has not selected its state yet. Its
    // deferred entity must remain hidden until the update binds EXChemSuit.
    let world = app.world_mut();
    let mut decals = world.query::<(&Decal, &Visibility)>();
    let (_, visible) = decals
        .iter(world)
        .find(|(decal, _)| decal.object == pathfinder)
        .unwrap();
    assert_eq!(*visible, Visibility::Hidden);
    app.update();
    app
}

#[ignore = "requires original game installation"]
#[test]
fn upgraded_pathfinder_decal_hides_from_enemy_until_original_scout_detection() {
    let (assets, battle, pathfinder, scout) = prepared_actors();
    let mut app = fixture(&assets, battle, pathfinder);
    assert_eq!(visibility(&mut app, pathfinder), Visibility::Hidden);

    {
        let mut battle = app.world_mut().resource_mut::<Match>();
        let game = &mut battle.simulation;
        game.set_detection_enabled(scout, true);
        advance_until(game, 180, "original Scout reveals upgraded Pathfinder", |game| game.entities[pathfinder].stealthed() && game.entities[pathfinder].detected())
            .unwrap_or_else(|error| panic!("{error}; target {:?} alive={} cloak={} detected={}, scout {:?} alive={}, parent {:?}", game.entities[pathfinder].position, game.entities[pathfinder].alive(), game.entities[pathfinder].stealthed(), game.entities[pathfinder].detected(), game.entities[scout].position, game.entities[scout].alive(), game.entities[game.entities[scout].drone_parent().unwrap()].position));
        assert!(game.logically_visible(0, pathfinder));
    }
    app.update();
    assert_eq!(visibility(&mut app, pathfinder), Visibility::Visible);
}
