//! Paid recall versus player Stop, through real selection and presentation systems.
use super::{CameraRig, Match, step};
use crate::{
    cleanup_verification, control_bar, crawler_verification, input, model_animation,
    playtest_view::{self, UnitVisual, WorldCamera, animation_fixture},
    skirmish_verification::crawler,
    source_audio,
    terrain::Terrain,
    veterancy_feedback, world_data,
};
use bevy::{
    asset::AssetPlugin,
    camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo},
    ecs::system::RunSystemOnce,
    prelude::*,
    window::PrimaryWindow,
};
use generals_assets::map::MapData;
use std::time::Duration;

fn fixture() -> App {
    let (assets, battle) = cleanup_verification::tests::installed();
    let root = &generals_assets::game_root().unwrap();
    let options = root.join("UserData/Options.ini");
    let mut data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    let audio = source_audio::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let preferences = crate::options_preferences::Preferences::load(&assets, &options).unwrap();
    let feedback = veterancy_feedback::load(&assets).unwrap();
    let settings = world_data::camera_settings(&assets, root).unwrap();
    let map = MapData::parse(&assets.read(super::MAP_NAME).unwrap().bytes).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let mut models = Vec::new();
    let mut animations =
        model_animation::load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<Image>();
    for (name, image) in std::mem::take(&mut data.textures) {
        data.handles.insert(
            name,
            app.world_mut().resource_mut::<Assets<Image>>().add(image),
        );
    }
    let geometry =
        animation_fixture::geometry(&models, &mut app.world_mut().resource_mut::<Assets<Mesh>>());
    animations.retain_models(models, 123);
    app.insert_resource(battle)
        .insert_resource(data)
        .insert_resource(mouse)
        .insert_resource(audio)
        .insert_resource(preferences)
        .insert_resource(feedback)
        .insert_resource(terrain)
        .insert_resource(geometry)
        .insert_resource(animations)
        .insert_resource(CameraRig {
            focus: Vec2::ZERO,
            distance: settings.max_height,
            generation: 0,
            settings,
        });
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let projection = PerspectiveProjection {
        aspect_ratio: 1.6,
        ..default()
    };
    app.world_mut().spawn((
        Camera {
            computed: ComputedCameraValues {
                clip_from_view: projection.get_clip_from_view(),
                target_info: Some(RenderTargetInfo {
                    physical_size: UVec2::new(1440, 900),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        },
        GlobalTransform::IDENTITY,
        WorldCamera,
    ));
    app
}

fn paid_kill(app: &mut App) -> (crawler::Setup, usize) {
    let mut battle = app.world_mut().resource_mut::<Match>();
    battle.generation += 1;
    battle.clear_drawable_selection();
    let setup = crawler::prepare(&mut battle.simulation).unwrap();
    crawler::assault(&mut battle.simulation, &setup).unwrap();
    let promoted = setup
        .riders
        .iter()
        .copied()
        .find(|&id| battle.simulation.entities[id].veterancy > 0)
        .unwrap();
    assert!(
        battle.simulation.entities[promoted].selectable(
            &battle.simulation.catalog[battle.simulation.entities[promoted].type_index]
        )
    );
    assert!(!battle.simulation.entities[setup.target].alive());
    (setup, promoted)
}

fn frame_actor(app: &mut App, id: usize) {
    let position = app
        .world()
        .resource::<Match>()
        .simulation
        .object_position(id);
    let center = Vec3::new(position[0], position[2], -position[1]) * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = Vec2::new(center.x, -center.z);
    let world = app.world_mut();
    let mut camera = world.query_filtered::<&mut GlobalTransform, With<WorldCamera>>();
    *camera.single_mut(world).unwrap() = GlobalTransform::from(
        Transform::from_translation(center + Vec3::new(0.0, 50.0, 35.0))
            .looking_at(center, Vec3::Y),
    );
}

fn draw(app: &mut App) {
    let world = app.world_mut();
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f64(1.0 / 30.0));
    world.run_system_once(playtest_view::render).unwrap();
    world.insert_resource(model_animation::Clock::new());
    world.run_system_once(model_animation::update).unwrap();
    world.run_system_once(veterancy_feedback::insignia).unwrap();
    world
        .run_system_once(veterancy_feedback::animations)
        .unwrap();
}

fn root(app: &mut App, id: usize) -> Entity {
    let world = app.world_mut();
    world
        .query::<(Entity, &UnitVisual)>()
        .iter(world)
        .find_map(|(entity, visual)| (visual.index == id).then_some(entity))
        .unwrap()
}

fn assert_body(app: &mut App, id: usize, visibility: Visibility) {
    let entity = root(app, id);
    assert_eq!(app.world().get::<Visibility>(entity), Some(&visibility));
    if visibility == Visibility::Visible {
        let children = app.world().get::<Children>(entity).unwrap();
        assert!(
            children.iter().any(|child| {
                let world = app.world();
                world.get::<Visibility>(child) != Some(&Visibility::Hidden)
                    && world.get::<Mesh3d>(child).is_some_and(|mesh| {
                        world
                            .resource::<Assets<Mesh>>()
                            .get(&mesh.0)
                            .is_some_and(|mesh| mesh.count_vertices() > 0)
                    })
            }),
            "exposed promoted infantry lacks a visible original W3D mesh"
        );
    }
}

struct Overlay {
    image: Handle<Image>,
    rect: Option<Rect>,
    top: f32,
}

fn overlay(app: &mut App) -> Overlay {
    let expected = app
        .world()
        .resource::<control_bar::UiData>()
        .image("SCVeter1");
    let world = app.world_mut();
    let mut badges =
        world.query_filtered::<(&Node, &ImageNode), With<veterancy_feedback::Insignia>>();
    let badges: Vec<_> = badges
        .iter(world)
        .filter(|(node, _)| node.display != Display::None)
        .collect();
    assert_eq!(badges.len(), 1);
    assert_eq!(badges[0].1.image, expected.image);
    assert_eq!(badges[0].1.rect, expected.rect);
    let mut promotions =
        world.query_filtered::<(&Node, &ImageNode), With<veterancy_feedback::Promotion>>();
    let (node, image) = promotions.single(world).unwrap();
    assert_ne!(node.display, Display::None);
    assert!(image.color.alpha() > 0.0);
    let Val::Px(top) = node.top else {
        panic!("promotion lacks projected pixel position")
    };
    Overlay {
        image: image.image.clone(),
        rect: image.rect,
        top,
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paid_crawler_recall_prunes_selection_but_player_stop_keeps_the_same_promoted_body_and_overlay() {
    let mut app = fixture();
    let (setup, promoted) = paid_kill(&mut app);
    app.world_mut()
        .resource_mut::<Match>()
        .select_objects(&[promoted], true);
    for _ in 0..600 {
        app.world_mut().run_system_once(step).unwrap();
        if app.world().resource::<Match>().simulation.entities[promoted].masked() {
            break;
        }
    }
    let battle = app.world().resource::<Match>();
    let actor = &battle.simulation.entities[promoted];
    assert!(actor.alive());
    assert_eq!(actor.contained_by(), Some(setup.carrier));
    assert!(
        battle.selected.is_empty(),
        "actual step must prune the recalled rider"
    );
    frame_actor(&mut app, promoted);
    draw(&mut app);
    assert_body(&mut app, promoted, Visibility::Hidden);

    let (setup, promoted) = paid_kill(&mut app);
    let (rank, experience) = {
        let mut battle = app.world_mut().resource_mut::<Match>();
        assert_eq!(
            crawler_verification::hold_promoted(&mut battle, &setup).unwrap(),
            promoted
        );
        let actor = &battle.simulation.entities[promoted];
        (actor.veterancy, actor.experience)
    };
    assert_eq!(rank, 1);
    assert!(experience > 0);
    frame_actor(&mut app, promoted);
    app.world_mut()
        .run_system_once(veterancy_feedback::record)
        .unwrap();
    let mut first: Option<Overlay> = None;
    let mut advanced = false;
    for _ in 0..30 {
        app.world_mut().run_system_once(step).unwrap();
        draw(&mut app);
        let battle = app.world().resource::<Match>();
        let actor = &battle.simulation.entities[promoted];
        assert_eq!(battle.selected, [promoted]);
        assert!(actor.selectable(&battle.simulation.catalog[actor.type_index]));
        assert!(actor.contained_by().is_none());
        assert_eq!(actor.veterancy, rank);
        assert_eq!(actor.experience, experience);
        assert_body(&mut app, promoted, Visibility::Visible);
        let current = overlay(&mut app);
        if let Some(previous) = &first {
            if (current.image != previous.image || current.rect != previous.rect)
                && current.top < previous.top
            {
                advanced = true;
                break;
            }
        } else {
            first = Some(current);
        }
    }
    assert!(
        advanced,
        "the real level-gain overlay did not change image and rise"
    );
}
