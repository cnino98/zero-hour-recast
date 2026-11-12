use super::{ParticleInspection, ParticleVisual};
use crate::{cleanup_verification, particles, playtest::Match, source_material::SourceMaterial};
use bevy::{ecs::system::SystemState, prelude::*};
use std::collections::{BTreeMap, BTreeSet};

fn record_bone_events(
    battle: Res<Match>,
    mut particles: ResMut<particles::Particles>,
    mut animations: ResMut<crate::model_animation::Animations>,
) {
    let battle = battle.into_inner();
    for event in &battle.simulation.presentation.weapon_fx {
        if let generals_sim::skirmish::WeaponFxEvent::BoneFx(index) = event {
            particles.bone_event(battle, *index, &mut animations);
        }
    }
}

fn cleanup_app() -> (App, cleanup_verification::Setup) {
    let (assets, mut battle, animations) = cleanup_verification::tests::installed_with_animations();
    let setup = cleanup_verification::prepare(&mut battle.simulation).unwrap();
    battle
        .simulation
        .clean_area(setup.actor, [setup.position[0], setup.position[1]])
        .unwrap();
    let root = &generals_assets::game_root().unwrap();
    let audio = crate::source_audio::load(
        &assets,
        &battle.simulation.catalog,
        &root.join("UserData/Options.ini"),
    )
    .unwrap();
    let particles = particles::load(&assets, &battle.simulation.catalog).unwrap();
    let projectile = battle
        .simulation
        .catalog
        .iter()
        .find(|unit| unit.name == "CleanupStreamProjectile")
        .unwrap();
    let trail = projectile.behavior.bone_fx[0].slots[0][0]
        .particle
        .as_ref()
        .unwrap();
    assert_eq!(trail.name, "CleanupTrail01");
    let mut app = App::new();
    // Mirror VisibilityPlugin's required mesh components without a GPU/view.
    app.register_required_components::<Mesh3d, Visibility>();
    app.insert_resource(battle)
        .insert_resource(animations)
        .insert_resource(audio)
        .insert_resource(particles)
        .init_resource::<Time<Fixed>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<SourceMaterial>>()
        // Feed actual scheduled BoneFX callbacks in timeline order. Native
        // acceptance separately exercises the complete WeaponFx dispatcher.
        .add_systems(
            Update,
            (record_bone_events, particles::step, particles::update).chain(),
        );
    app.world_mut()
        .spawn((crate::playtest_view::WorldCamera, Transform::IDENTITY));
    (app, setup)
}

#[ignore = "requires original game installation"]
#[test]
fn original_cleanup_trail_remains_renderable_after_its_missile_is_removed() {
    let (mut app, setup) = cleanup_app();
    let mut ids = BTreeSet::new();
    let mut trace = cleanup_verification::Progress::new(setup.health);
    let mut systems = BTreeMap::new();
    let mut inspection = SystemState::<ParticleInspection>::new(app.world_mut());
    let mut checked = false;
    let mut live_seen = false;
    let mut last_details = None;
    for _ in 0..1200 {
        let live = {
            let mut battle = app.world_mut().resource_mut::<Match>();
            battle.simulation.step();
            trace.observe(&battle.simulation, &setup);
            let live: BTreeSet<_> = battle
                .simulation
                .projectiles
                .iter()
                .filter(|projectile| trace.projectile_ids().contains(&projectile.id))
                .map(|projectile| projectile.id)
                .collect();
            ids.extend(&live);
            live
        };
        app.update();
        // This device-free test exercises actual uploaded particle meshes; the
        // native scenario separately proves the camera/GPU visibility result.
        let mut visuals = app
            .world_mut()
            .query_filtered::<&mut ViewVisibility, With<ParticleVisual>>();
        for mut visibility in visuals.iter_mut(app.world_mut()) {
            *visibility = ViewVisibility::VISIBLE;
        }
        let retired: BTreeSet<_> = ids.difference(&live).copied().collect();
        let inspector = inspection.get(app.world()).unwrap();
        let game = &app.world().resource::<Match>().simulation;
        // Snapshot real associations before Object destruction clears them.
        // This fixture does not reset the match or reuse emitter generations.
        systems.extend(inspector.cleanup_systems(game, &live));
        let live_systems = systems
            .iter()
            .filter_map(|(&system, projectile)| live.contains(projectile).then_some(system))
            .collect();
        let retired_systems = systems
            .iter()
            .filter_map(|(&system, projectile)| retired.contains(projectile).then_some(system))
            .collect();
        live_seen |= inspector.cleanup_trail(&live_systems, 0).unwrap();
        if let Some(details) = inspector.cleanup_details(&systems.keys().copied().collect(), 0) {
            last_details = Some(format!("live={live:?} retired={retired:?}; {details}"));
        }
        if inspector.cleanup_trail(&retired_systems, 0).unwrap() {
            assert!(!retired.is_empty());
            assert!(
                !inspector
                    .cleanup_trail(&BTreeSet::from([u64::MAX]), 0)
                    .unwrap()
            );
            checked = true;
            break;
        }
    }
    assert!(
        checked,
        "original attached particles did not outlive any cleanup missile; observed {ids:?}; live_seen={live_seen} last_details={last_details:?}"
    );
}
