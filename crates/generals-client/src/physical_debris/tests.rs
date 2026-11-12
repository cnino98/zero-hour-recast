use super::*;
use crate::{playtest_view, source_material::SourceMaterial, terrain::Terrain};
use bevy::{asset::AssetPlugin, transform::TransformSystems};
use generals_sim::skirmish::{Faction, Kind, Skirmish, physical_ocl::List};

#[test]
fn physical_matrix_keeps_native_axis_conversion_and_nonorthogonal_basis() {
    let position = [100.0, 200.0, 30.0];
    let basis = [[0.0, -1.0, 0.25], [1.0, 0.0, -0.125], [0.0625, 0.0, 1.0]];
    let transform = source_transform(position, basis);
    for point in [
        [0.0; 3],
        [10.0, 0.0, 0.0],
        [0.0, 20.0, 0.0],
        [0.0, 0.0, 40.0],
    ] {
        let source: [f32; 3] = std::array::from_fn(|axis| {
            let products =
                std::array::from_fn::<_, 3, _>(|column| basis[axis][column] * point[column]);
            position[axis] + (products[0] + products[1] + products[2])
        });
        let actual =
            transform.transform_point(Vec3::new(point[0] * 0.1, point[2] * 0.1, -point[1] * 0.1));
        let expected = [source[0] * 0.1, source[2] * 0.1, -source[1] * 0.1];
        assert_eq!(
            actual.to_array().map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }
}

fn fixture() -> (App, List, AssetId<SourceMaterial>) {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let (debris, outpost, list) =
        crate::skirmish_data::physical_ocl::tests::installed_outpost(&assets);
    assert_eq!(debris.initial_health.to_bits(), 0.0_f32.to_bits());
    assert!(debris.physical.is_some());
    if !battle
        .simulation
        .catalog
        .iter()
        .any(|unit| unit.name == debris.name)
    {
        battle.simulation.catalog.push(debris);
    }
    if !battle
        .simulation
        .catalog
        .iter()
        .any(|unit| unit.name == outpost.name)
    {
        battle.simulation.catalog.push(outpost);
    }
    let mut models = Vec::new();
    let index = load(&assets, &battle.simulation.catalog, &mut models).unwrap();
    let dish = *index
        .0
        .get("nvloutpost_d2")
        .expect("original Outpost dish dependency");
    assert!(
        !crate::model_render::parts(&models[dish])
            .unwrap()
            .is_empty(),
        "installed Outpost dish has no renderable parts; meshes={}",
        models[dish].meshes.len()
    );
    let map = generals_assets::map::MapData::parse(
        &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
    )
    .unwrap();
    let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
        .init_asset::<Mesh>()
        .init_asset::<SourceMaterial>()
        .init_resource::<crate::structure_view::Structures>()
        .insert_resource(battle)
        .insert_resource(terrain)
        .insert_resource(index)
        .add_systems(Update, (playtest_view::render, update).chain())
        .add_systems(PostUpdate, transform.after(TransformSystems::Propagate));
    // Headless ownership adapter: real W3D parts, distinguishable original and
    // house material handles, and the production spawn/transform/retire systems.
    let material = crate::model_render::parts(&models[0]).unwrap()[0]
        .material
        .clone();
    let materials: Vec<_> = (0..17)
        .map(|_| {
            app.world_mut()
                .resource_mut::<Assets<SourceMaterial>>()
                .add(material.clone())
        })
        .collect();
    let original = materials[0].id();
    let geometry = playtest_view::animation_fixture::geometry_materials(
        &models,
        &mut app.world_mut().resource_mut::<Assets<Mesh>>(),
        &materials,
    );
    app.insert_resource(geometry);
    let mut animations = Animations::effect_fixture(0);
    animations.retain_models(models, 0);
    app.insert_resource(animations);
    (app, list, original)
}

fn game(app: &mut App) -> Mut<'_, Skirmish> {
    app.world_mut()
        .resource_mut::<Match>()
        .map_unchanged(|battle| &mut battle.simulation)
}

fn root(app: &mut App, actor: usize) -> Option<Entity> {
    let world = app.world_mut();
    world
        .query::<(Entity, &UnitVisual)>()
        .iter(world)
        .find_map(|(entity, visual)| (visual.index == actor).then_some(entity))
}

fn parts(app: &App, entity: Entity) -> Vec<(AssetId<Mesh>, AssetId<SourceMaterial>)> {
    app.world()
        .get::<Children>(entity)
        .unwrap()
        .iter()
        .map(|child| {
            (
                app.world().get::<Mesh3d>(child).unwrap().0.id(),
                app.world()
                    .get::<MeshMaterial3d<SourceMaterial>>(child)
                    .unwrap()
                    .0
                    .id(),
            )
        })
        .collect()
}

fn mobile_origin(game: &mut Skirmish) -> usize {
    let source = crate::outpost_verification::prepare(game).unwrap();
    let origin = Vec2::from_array(game.entities[source].position);
    let enemy =
        crate::skirmish_verification::friendly_entity(game, Kind::CommandCenter, 1).unwrap();
    let direction = (Vec2::from_array(game.entities[enemy].position) - origin).normalize();
    let goal = origin + direction * 300.0;
    game.move_to(source, goal.to_array()).unwrap();
    crate::skirmish_verification::advance_until(
        game,
        3000,
        "Outpost clear-ground debris origin",
        |game| {
            matches!(
                game.entities[source].ai.order,
                generals_sim::skirmish::Order::Idle
            ) && !game.entities[source].moving()
        },
    )
    .unwrap();
    assert!(game.entities[source].alive());
    assert!(Vec2::from_array(game.entities[source].position).distance(origin) > 250.0);
    source
}

#[ignore = "requires original game installation"]
#[test]
fn installed_outpost_debris_keeps_unowned_shared_art_until_native_retirement_and_restart() {
    let (mut app, list, original) = fixture();
    let actors = {
        let mut game = game(&mut app);
        let source = mobile_origin(&mut game);
        (0..4)
            .flat_map(|_| game.create_physical_ocl(source, &list).unwrap())
            .collect::<Vec<_>>()
    };
    // OCL creation installs the object; the next native logic callback updates
    // its partition visibility before a drawable may create visible children.
    game(&mut app).step();
    app.update();
    let roots: Vec<_> = actors
        .iter()
        .map(|&id| root(&mut app, id).unwrap())
        .collect();
    let reference = parts(&app, roots[0]);
    assert!(!reference.is_empty());
    assert!(reference.iter().all(|(_, material)| *material == original));
    let mesh_count = app.world().resource::<Assets<Mesh>>().len();
    for (&id, &entity) in actors.iter().zip(&roots) {
        assert_eq!(parts(&app, entity), reference);
        let game = game(&mut app);
        assert!(!game.entities[id].alive());
        assert!(game.entities[id].physical_present());
    }
    for _ in 0..5 {
        game(&mut app).step();
        app.update();
    }
    for (&id, &entity) in actors.iter().zip(&roots) {
        let game = &app.world().resource::<Match>().simulation;
        assert!(
            app.world().get::<Children>(entity).is_some(),
            "debris after five ticks: actor={id} tick={} physical={} masked={} pos={:?} root_present={}",
            game.tick,
            game.entities[id].physical_present(),
            game.entities[id].masked(),
            game.object_position(id),
            app.world().get_entity(entity).is_ok()
        );
        assert_eq!(parts(&app, entity), reference);
    }
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), mesh_count);
    for _ in 0..1800 {
        if actors
            .iter()
            .all(|&id| !game(&mut app).entities[id].physical_present())
        {
            break;
        }
        game(&mut app).step();
    }
    app.update();
    assert!(actors.iter().all(|&id| root(&mut app, id).is_none()));
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), mesh_count);
    {
        let mut battle = app.world_mut().resource_mut::<Match>();
        battle.generation += 1;
        battle
            .simulation
            .reset(Faction::Usa, Faction::China)
            .unwrap();
    }
    app.update();
    assert!(
        roots
            .iter()
            .all(|&entity| app.world().get_entity(entity).is_err())
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_debris_building_collision_retires_its_drawable_without_retiring_shared_meshes() {
    let (mut app, list, _) = fixture();
    let actor = {
        let mut game = game(&mut app);
        let source =
            crate::skirmish_verification::friendly_entity(&game, Kind::CommandCenter, 0).unwrap();
        // Deliberately launch the real dish through an immobile building. Native
        // PhysicsBehavior destroys it at contact before its resting lifetime.
        game.create_physical_ocl(source, &list).unwrap()[0]
    };
    // The dish overlaps the command center at creation. Render its live
    // drawable before the next partition update delivers that contact.
    app.update();
    let drawable = root(&mut app, actor).unwrap();
    assert!(!parts(&app, drawable).is_empty());
    let meshes = app.world().resource::<Assets<Mesh>>().len();
    for _ in 0..10 {
        game(&mut app).step();
        app.update();
        if !game(&mut app).entities[actor].physical_present() {
            break;
        }
    }
    assert!(game(&mut app).entities[actor].masked());
    assert!(root(&mut app, actor).is_none());
    assert!(app.world().get_entity(drawable).is_err());
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), meshes);
}

#[ignore = "requires original game installation"]
#[test]
fn fogged_physical_ghost_retains_full_matrix_through_deletion_until_revealed() {
    let (mut app, list, _) = fixture();
    let (actor, observer) = {
        let mut game = game(&mut app);
        let source =
            crate::skirmish_verification::friendly_entity(&game, Kind::CommandCenter, 1).unwrap();
        let observer =
            crate::skirmish_verification::friendly_entity(&game, Kind::Builder, 0).unwrap();
        game.entities[source].position = [1000.0, 1000.0];
        game.entities[observer].position = [970.0, 1000.0];
        for unit in &mut game.catalog {
            unit.sight.clearing_range = 0.0;
        }
        let observer_type = game.entities[observer].type_index;
        game.catalog[observer_type].sight.clearing_range = 200.0;
        let actor = game.create_physical_ocl(source, &list).unwrap()[0];
        let kind = game.entities[actor].type_index;
        // Storage regression for the shared full-basis ghost path; GenericDebris
        // itself is mobile. Only this fixture requests the immobile ghost policy.
        game.catalog[kind].sight.immobile = true;
        game.configure_visibility(generals_sim::skirmish::visibility::VisibilityRules {
            cell_size: 40.0,
            unlook_persist_frames: 0,
            use_shroud: false,
        })
        .unwrap();
        assert!(matches!(game.visibility_status(0, actor), Status::Clear));
        (actor, observer)
    };
    app.update();
    let drawable = root(&mut app, actor).unwrap();
    let pose = app
        .world()
        .get::<GlobalTransform>(drawable)
        .unwrap()
        .to_matrix()
        .to_cols_array()
        .map(f32::to_bits);
    {
        let mut game = game(&mut app);
        game.entities[observer].position = [400.0, 400.0];
        for _ in 0..3 {
            game.step();
        }
        assert!(matches!(game.visibility_status(0, actor), Status::Fogged));
    }
    app.update();
    assert_eq!(
        app.world()
            .get::<GlobalTransform>(drawable)
            .unwrap()
            .to_matrix()
            .to_cols_array()
            .map(f32::to_bits),
        pose
    );
    for _ in 0..1800 {
        if !game(&mut app).entities[actor].physical_present() {
            break;
        }
        game(&mut app).step();
    }
    assert!(game(&mut app).entities[actor].masked());
    app.update();
    assert_eq!(root(&mut app, actor), Some(drawable));
    assert_eq!(
        app.world().get::<Visibility>(drawable),
        Some(&Visibility::Visible)
    );
    assert_eq!(
        app.world()
            .get::<GlobalTransform>(drawable)
            .unwrap()
            .to_matrix()
            .to_cols_array()
            .map(f32::to_bits),
        pose
    );
    {
        let mut game = game(&mut app);
        game.entities[observer].position = game.entities[actor].position;
        game.step();
        assert!(matches!(game.visibility_status(0, actor), Status::Clear));
    }
    app.update();
    assert!(root(&mut app, actor).is_none());
}
