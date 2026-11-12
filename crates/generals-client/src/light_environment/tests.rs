use super::*;
use crate::light_pulse::Pool;
use crate::skirmish_verification::{advance_until, construct, friendly_entity, produce_type};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{Faction, Kind};

#[test]
fn native_spheres_reject_exact_tangency_before_attenuation() {
    assert!(!intersects(Vec3::ZERO, 2.0, Vec3::new(5.0, 0.0, 0.0), 3.0));
    assert!(intersects(
        Vec3::ZERO,
        2.0,
        Vec3::new(f32::from_bits(5.0_f32.to_bits() - 1), 0.0, 0.0),
        3.0
    ));
    assert!(!intersects(
        Vec3::ZERO,
        2.0,
        Vec3::new(f32::from_bits(5.0_f32.to_bits() + 1), 0.0, 0.0),
        3.0
    ));
}

#[test]
fn source_environment_keeps_ties_and_all_overlapping_ambient_beyond_four_lights() {
    let mut material =
        SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false);
    material.parameters.light_color = [Vec4::new(1.0, 0.0, 0.0, 0.0); 4];
    material.parameters.light_direction =
        std::array::from_fn(|index| Vec3::X.extend(f32::from(u16::try_from(index).unwrap())));
    let pulses: Vec<_> = (0..5)
        .map(|_| Sample {
            position: [0.0; 3],
            color: [1.0, 0.0, 0.0],
            outer_radius: 21.0,
        })
        .collect();
    environment(&mut material.parameters, Vec3::ZERO, 2.0, &pulses);
    // Equal native contributions preserve the four earlier global-light slots.
    assert!(
        material
            .parameters
            .point_position
            .iter()
            .all(|point| point.w.to_bits() == 0.0_f32.to_bits())
    );
    for (index, direction) in material.parameters.light_direction.iter().enumerate() {
        assert_eq!(
            direction.w.to_bits(),
            f32::from(u16::try_from(index).unwrap()).to_bits()
        );
    }
    // All five accepted lights add ambient even though no point won a slot.
    assert_eq!(
        material.parameters.scene_ambient.x.to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn point_coefficients_use_source_double_intermediate_and_native_coordinates() {
    let mut material =
        SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false);
    let radius = 31.7_f32;
    let pulse = Sample {
        position: [1234.567, 2789.123, 17.3],
        color: [1.0, 0.5, 0.25],
        outer_radius: radius,
    };
    let center = Vec3::new(1243.891, 2791.137, 21.47);
    let delta = Vec3::from_array(pulse.position) - center;
    let distance = squared_length(delta).sqrt();
    let expected = (1.0_f64 - (f64::from(distance) - 1.0) / (f64::from(radius) - 1.0))
        .to_f32()
        .unwrap();
    assert_eq!(
        center_attenuation(distance, radius).to_bits(),
        expected.to_bits()
    );
    environment(&mut material.parameters, center, 2.0, &[pulse]);
    assert_eq!(
        material.parameters.point_position[0].w.to_bits(),
        radius.to_bits()
    );
    assert_eq!(
        material.parameters.point_ambient[0].w.to_bits(),
        (8.0_f64 / (f64::from(radius) * f64::from(radius)))
            .to_f32()
            .unwrap()
            .to_bits()
    );
    let raw = DrawPose {
        position: [1234.567, 2789.123, 17.3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    assert_eq!(
        raw.point([0.0; 3]).map(f32::to_bits),
        raw.position.map(f32::to_bits)
    );
}

fn original_model(assets: &VirtualAssets) -> Model {
    let bytes = assets.read("art/w3d/avcrusader.w3d").unwrap();
    let hierarchy = Model::hierarchy_name(&bytes.bytes)
        .unwrap()
        .map(|name| assets.read(&format!("art/w3d/{name}.w3d")).unwrap());
    Model::parse_with_hierarchy(
        &bytes.bytes,
        hierarchy.as_ref().map(|asset| asset.bytes.as_slice()),
    )
    .unwrap()
}

fn original_scene() -> (App, Vec<Entity>, Handle<SourceMaterial>) {
    let (assets, mut battle, animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let kind = battle
        .simulation
        .catalog
        .iter()
        .position(|unit| unit.name == "AmericaTankCrusader")
        .unwrap();
    let game = &mut battle.simulation;
    game.reset(Faction::Usa, Faction::China).unwrap();
    let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, builder, Kind::PowerPlant, 0).unwrap();
    construct(game, builder, Kind::SupplyCenter, 0).unwrap();
    let factory = construct(game, builder, Kind::Factory, 0).unwrap();
    let a = produce_type(game, factory, kind, 0).unwrap();
    let b = produce_type(game, factory, kind, 0).unwrap();
    let home = game.entities[factory].position;
    let destination = [home[0] + 200.0, home[1]];
    game.move_to(b, destination).unwrap();
    advance_until(
        game,
        3600,
        "second paid Crusader outside first light sphere",
        |game| {
            let left = game.entities[a].position;
            let right = game.entities[b].position;
            (left[0] - right[0]).hypot(left[1] - right[1]) > 150.0
        },
    )
    .unwrap();
    let model = original_model(&assets);
    let sphere = model.scene_sphere.as_ref().unwrap();
    let mut parts = crate::model_render::parts(&model).unwrap();
    let part = parts.remove(0);
    let bounds = part.light_bounds.unwrap();
    assert_eq!(
        bounds.center.to_array().map(f32::to_bits),
        sphere.center.map(f32::to_bits)
    );
    assert_eq!(bounds.radius.to_bits(), sphere.radius.to_bits());
    let pose_a = DrawPose::from_object(&battle.simulation, a);
    let pose_b = DrawPose::from_object(&battle.simulation, b);
    let mut pool = Pool::new();
    pool.create(pose_a.point(sphere.center), [1.0, 0.5, 0.25], 30.0, 0, 3);
    pool.advance_frame();
    let mut frame = Frame::default();
    frame.capture(&pool);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
        .init_asset::<SourceMaterial>();
    let material = app
        .world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .add(part.material);
    let handles = [a, b]
        .into_iter()
        .zip([pose_a, pose_b])
        .map(|(index, pose)| {
            let parent = app
                .world_mut()
                .spawn((
                    UnitVisual {
                        index,
                        generation: battle.generation,
                    },
                    pose,
                ))
                .id();
            app.world_mut()
                .spawn((ChildOf(parent), bounds, MeshMaterial3d(material.clone())))
                .id()
        })
        .collect::<Vec<_>>();
    app.insert_resource(crate::selection_feedback::load(&assets).unwrap())
        .insert_resource(crate::stealth_view::Cloaks::new())
        .insert_resource(frame)
        .insert_resource(battle)
        .insert_resource(animations)
        .add_systems(Update, crate::selection_feedback::tint_models);
    (app, handles, material)
}

#[ignore = "requires original game installation"]
#[test]
fn original_model_receiver_uses_prototype_sphere_and_material_instances_are_independent() {
    let (mut app, handles, material) = original_scene();
    app.update();
    let lit = app
        .world()
        .get::<MeshMaterial3d<SourceMaterial>>(handles[0])
        .unwrap()
        .0
        .clone();
    assert_ne!(lit, material);
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<SourceMaterial>>(handles[1])
            .unwrap()
            .0,
        material
    );
    assert!(
        app.world()
            .resource::<Assets<SourceMaterial>>()
            .get(&material)
            .unwrap()
            .parameters
            .point_position
            .iter()
            .all(|point| point.w.to_bits() == 0.0_f32.to_bits())
    );
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<SourceMaterial>>(handles[0])
            .unwrap()
            .0,
        lit
    );
    app.world_mut().resource_mut::<Frame>().active.clear();
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<SourceMaterial>>(handles[0])
            .unwrap()
            .0,
        material
    );
}
