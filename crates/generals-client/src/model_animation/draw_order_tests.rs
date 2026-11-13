use super::*;
use crate::{
    particles::Particles,
    playtest_view::{WorldCamera, animation_fixture},
    terrain::Terrain,
};
use bevy::asset::AssetPlugin;

fn draw() -> Draw {
    let mut state = State::empty();
    // A missing animation handle still consumes the native animation choice.
    state.motions = (0..7)
        .map(|index| Motion {
            name: format!("choice{index}"),
            distance: 0.0,
            idle: false,
        })
        .collect();
    Draw {
        truck_wheels: None,
        treads: None,
        track: None,
        supply: BTreeMap::new(),
        upgrade_show: Vec::new(),
        states: vec![state],
        ignored: BTreeSet::new(),
        recoil: muzzle::Rules::new(),
        receives_dynamic_lights: true,
        requires_power: false,
        attach_bone: None,
        attach_offset: [0.0; 3],
    }
}

#[test]
#[ignore = "requires original game installation"]
fn production_draw_walk_interleaves_animation_and_emitter_rng_in_birth_and_module_order() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let map = generals_assets::map::MapData::parse(
        &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
    )
    .unwrap();
    let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let actors: Vec<_> = battle
        .simulation
        .entities
        .iter()
        .enumerate()
        .filter(|(_, actor)| actor.owner == 0 && actor.alive())
        .take(2)
        .map(|(id, actor)| (id, actor.type_index, actor.drawable_birth()))
        .collect();
    assert_eq!(actors.len(), 2);
    let mut animations = Animations::effect_fixture(41);
    animations.draws = (0..battle.simulation.catalog.len())
        .map(|_| vec![draw(), draw()])
        .collect();
    let particles =
        Particles::draw_order_fixture(&actors.iter().map(|&(_, kind, _)| kind).collect::<Vec<_>>());
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>();
    let geometry =
        animation_fixture::geometry(&[], &mut app.world_mut().resource_mut::<Assets<Mesh>>());
    let mut roots = BTreeMap::new();
    for &(id, _, _) in &actors {
        roots.insert(
            id,
            app.world_mut()
                .spawn((
                    UnitVisual {
                        index: id,
                        generation: battle.generation,
                    },
                    Players(Vec::new()),
                ))
                .id(),
        );
    }
    app.world_mut().spawn((
        WorldCamera,
        Transform::from_xyz(0.0, 100.0, 0.0),
        Projection::Perspective(PerspectiveProjection::default()),
        Camera::default(),
    ));
    app.insert_resource(geometry)
        .insert_resource(terrain)
        .insert_resource(animations)
        .insert_resource(particles)
        .insert_resource(Clock::new())
        .insert_resource(battle)
        .add_systems(Update, update);
    app.update();
    let mut expected = playback::Random::new(41);
    let mut order = actors;
    order.sort_unstable_by_key(|&(_, _, birth)| std::cmp::Reverse(birth));
    let mut delays = Vec::new();
    for (id, _, _) in order {
        let players = app.world().get::<Players>(roots[&id]).unwrap();
        for module in 0..2 {
            assert_eq!(players.0[module].choice, expected.index(7));
            delays.push(((id, module), expected.real(10.0, 20.0).trunc()));
        }
    }
    assert_eq!(
        app.world().resource::<Particles>().draw_order_delays(),
        delays
    );
}
