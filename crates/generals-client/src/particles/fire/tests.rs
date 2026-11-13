use super::*;
use crate::particles::tests::{system, template};

fn flame_field() -> super::super::Template {
    let mut flame = template("FireWallSegmentSystem");
    flame.system_lifetime = 0.0;
    flame.lifetime.low = 20.0;
    flame.lifetime.high = 20.0;
    flame.alpha[1].frame = 20;
    flame.color[1].frame = 20;
    flame
}

fn advance(particles: &mut Particles, game: &Skirmish, region: [[f32; 2]; 2]) {
    particles.fire_drawables(game, region, &mut |_, high| high);
    particles.advance_frame(game.tick, &mut |_, high| high, &|point| {
        game.ground_height(point)
    });
}

#[ignore = "requires original game installation"]
#[test]
fn invisible_fields_defer_creation_follow_object_pose_and_drain_after_removal() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    let id = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0)
        .unwrap();
    let kind = game.entities[id].type_index;
    let mut particles = system(vec![flame_field()]);
    particles
        .fires
        .bindings
        .insert(kind, Bindings { draw: vec![0] });
    let mut calls = 0;
    particles.fire_drawables(game, [[-2000.0; 2], [-1000.0; 2]], &mut |_, high| {
        calls += 1;
        high
    });
    assert_eq!(calls, 0);
    assert!(particles.emitters.is_empty());
    let region = [[0.0; 2], [10000.0; 2]];
    advance(&mut particles, game, region);
    let emitter = particles
        .emitters
        .iter()
        .find(|emitter| matches!(emitter.fire_attachment,Some(Attachment::Object(object)) if object==id))
        .unwrap();
    let system_id = emitter.id;
    let remaining = emitter.particles[0].remaining;
    let old_particle = emitter.particles[0].position;
    game.entities[id].position[0] += 10.0;
    game.tick += 1;
    particles.fire_drawables(game, region, &mut |_, high| high);
    let emitter = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == system_id)
        .unwrap();
    assert_eq!(
        emitter.particles[0].remaining.to_bits(),
        remaining.to_bits()
    );
    assert_eq!(
        emitter.transform.position.map(f32::to_bits),
        game.object_position(id).map(f32::to_bits)
    );
    assert_eq!(
        emitter.particles[0].position.map(f32::to_bits),
        old_particle.map(f32::to_bits)
    );
    advance(&mut particles, game, region);
    let count = particles.next_id;
    advance(&mut particles, game, region);
    assert_eq!(particles.next_id, count);
    let emitter = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == system_id)
        .unwrap();
    assert_eq!(
        emitter.particles[0].remaining.to_bits(),
        (remaining - 1.0).to_bits()
    );
    game.entities.clear();
    game.tick += 1;
    advance(&mut particles, game, region);
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.lifecycle.destroyed)
    );
    assert!(
        particles
            .emitters
            .iter()
            .any(|emitter| !emitter.particles.is_empty())
    );
    for _ in 0..30 {
        game.tick += 1;
        advance(&mut particles, game, region);
    }
    assert!(particles.emitters.is_empty());
    particles.begin(1, 0);
    assert!(particles.fires.active.is_empty());
    assert!(!particles.fires.bindings.is_empty());
}

#[test]
fn required_fire_templates_resolve_in_authored_order_and_missing_names_fail() {
    let pending = Pending {
        projectiles: BTreeMap::new(),
        module_names: vec!["Storm".into()],
        bindings: BTreeMap::from([(
            4,
            (vec!["Flame".into(), "Ring".into()], vec!["Storm".into()]),
        )]),
    };
    let templates = [template("Ring"), template("Storm"), template("Flame")];
    let resolved = pending.resolve(&templates).unwrap();
    assert_eq!(resolved.bindings[&4].draw, [2, 0]);
    assert_eq!(resolved.module_templates["Storm"], 1);
    let missing = Pending {
        projectiles: BTreeMap::new(),
        module_names: Vec::new(),
        bindings: BTreeMap::from([(4, (vec!["Absent".into()], Vec::new()))]),
    };
    assert!(missing.resolve(&templates).is_err());
}

#[ignore = "requires original game installation"]
#[test]
fn original_inferno_shell_trail_follows_shell_and_drains_after_native_impact() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    let gun = launch_inferno(game, true);
    let projectile = game
        .projectiles
        .iter()
        .find(|projectile| projectile.source == gun)
        .unwrap()
        .id;
    let mut particles = crate::particles::load(&assets, &game.catalog).unwrap();
    let indices = &particles.fires.projectile_bindings[&(game.entities[gun].type_index, 0, false)];
    assert_eq!(indices.len(), 1);
    assert_eq!(particles.templates[indices[0]].name, "InfernoTrail");
    let region = [[-10000.0; 2], [10000.0; 2]];
    particles.begin(0, game.tick);
    advance(&mut particles, game, region);
    let emitter = particles.emitters.iter().find(|emitter| matches!(emitter.fire_attachment, Some(Attachment::Projectile(id)) if id == projectile)).unwrap();
    let system = emitter.id;
    assert_eq!(
        emitter.transform.position.map(f32::to_bits),
        game.projectiles
            .iter()
            .find(|value| value.id == projectile)
            .unwrap()
            .effect_transform(&game.entities)
            .unwrap()
            .0
            .map(f32::to_bits)
    );
    let created = particles.next_id;
    advance(&mut particles, game, region);
    assert_eq!(particles.next_id, created);
    game.stop(gun).unwrap();
    for _ in 0..600 {
        game.step();
        particles.begin(0, game.tick);
        advance(&mut particles, game, region);
        if !game.projectiles.iter().any(|value| value.id == projectile) {
            break;
        }
        let emitter = particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == system)
            .unwrap();
        assert!(
            !emitter.shrouded,
            "close-range shell left the original visible area"
        );
    }
    assert!(!game.projectiles.iter().any(|value| value.id == projectile));
    let emitter = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == system)
        .expect("visible Inferno impact retains the trailing five-frame particles");
    assert!(emitter.lifecycle.destroyed);
    assert!(!emitter.particles.is_empty());
    for _ in 0..600 {
        game.step();
        particles.begin(0, game.tick);
        advance(&mut particles, game, region);
        if particles
            .emitters
            .iter()
            .all(|emitter| emitter.id != system)
        {
            break;
        }
    }
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.id != system)
    );
}

#[ignore = "requires original game installation"]
#[test]
fn original_inferno_trail_drains_in_fog_before_a_long_range_impact() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    let gun = launch_inferno(game, false);
    let projectile = game
        .projectiles
        .iter()
        .find(|shot| shot.source == gun)
        .unwrap()
        .id;
    let mut particles = crate::particles::load(&assets, &game.catalog).unwrap();
    let region = [[-10000.0; 2], [10000.0; 2]];
    particles.begin(0, game.tick);
    advance(&mut particles, game, region);
    let emitter = particles.emitters.iter().find(|emitter| matches!(emitter.fire_attachment, Some(Attachment::Projectile(id)) if id == projectile)).unwrap();
    let system = emitter.id;
    assert!(!emitter.particles.is_empty());
    game.stop(gun).unwrap();
    let mut hidden_at = None;
    let mut drained_while_attached = false;
    for _ in 0..600 {
        game.step();
        particles.begin(0, game.tick);
        advance(&mut particles, game, region);
        if let Some(emitter) = particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == system)
            && emitter.shrouded
            && !emitter.lifecycle.destroyed
        {
            let hidden_at = *hidden_at.get_or_insert(game.tick);
            assert!(
                emitter
                    .particles
                    .iter()
                    .all(|particle| particle.born < hidden_at)
            );
            if game.tick - hidden_at >= 5 {
                assert!(emitter.particles.is_empty());
                drained_while_attached = true;
            }
        }
        if !game.projectiles.iter().any(|shot| shot.id == projectile) {
            break;
        }
    }
    assert!(drained_while_attached);
    assert!(!game.projectiles.iter().any(|shot| shot.id == projectile));
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.id != system)
    );
}

fn launch_inferno(game: &mut generals_sim::skirmish::Skirmish, close: bool) -> usize {
    use crate::skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    };
    use crate::strategy_verification::template as kind;
    use generals_sim::skirmish::{Faction, Kind};

    game.reset(Faction::China, Faction::Usa).unwrap();
    let worker = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, worker, Kind::PowerPlant, 0).unwrap();
    construct(game, worker, Kind::SupplyCenter, 0).unwrap();
    let factory = construct(game, worker, Kind::Factory, 0).unwrap();
    construct_type(
        game,
        worker,
        kind(game, "ChinaPropagandaCenter").unwrap(),
        0,
    )
    .unwrap();
    let gun = produce_type(
        game,
        factory,
        kind(game, "ChinaVehicleInfernoCannon").unwrap(),
        0,
    )
    .unwrap();
    let target = friendly_entity(game, Kind::CommandCenter, 1).unwrap();
    if close {
        // Original range is 300, shroud clearing 300 and target geometry extends
        // weapon reach beyond sight. Move normally to 200 before the visible shot.
        let destination = game.entities[target].position;
        let offset = std::array::from_fn::<_, 2, _>(|axis| {
            game.entities[gun].position[axis] - destination[axis]
        });
        let distance = offset[0].hypot(offset[1]);
        game.move_to(
            gun,
            std::array::from_fn(|axis| {
                let shift = offset[axis] / distance * 200.0;
                destination[axis] + shift
            }),
        )
        .unwrap();
        advance_until(
            game,
            18000,
            "Inferno moving inside native shroud range",
            |game| {
                matches!(
                    game.entities[gun].ai.order,
                    generals_sim::skirmish::Order::Idle
                )
            },
        )
        .unwrap();
    }
    game.attack(gun, target).unwrap();
    advance_until(game, 18000, "ordinary Inferno shell launch", |game| {
        game.projectiles
            .iter()
            .any(|projectile| projectile.source == gun)
    })
    .unwrap();
    gun
}

#[ignore = "requires original game installation"]
#[test]
fn installed_missile_jammed_state_selects_original_sparks_without_normal_state_leakage() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let types = &battle.simulation.catalog;
    let kind = types
        .iter()
        .position(|unit| unit.name == "AmericaInfantryMissileDefender")
        .unwrap();
    let pending = projectiles::load(&assets, types).unwrap();
    assert!(pending[&(kind, 0, false)].is_empty());
    assert_eq!(pending[&(kind, 0, true)], ["SparksMedium"]);
    let particles = crate::particles::load(&assets, types).unwrap();
    let jammed = &particles.fires.projectile_bindings[&(kind, 0, true)];
    assert_eq!(jammed.len(), 1);
    assert_eq!(particles.templates[jammed[0]].name, "SparksMedium");
}

#[ignore = "requires original game installation"]
#[test]
fn rendered_projectile_clear_footprint_records_observer_grace() {
    use crate::{
        model_animation::{
            AnimatedPart,
            draw_visibility::{DrawVisibility, record},
        },
        model_render::PartBinding,
        playtest::Match,
        projectile_render::ProjectileVisual,
    };
    use bevy::prelude::*;
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let gun = launch_inferno(&mut battle.simulation, true);
    let shot = battle
        .simulation
        .projectiles
        .iter()
        .find(|shot| shot.source == gun)
        .unwrap();
    let object = shot.object.unwrap();
    let visual = ProjectileVisual {
        id: shot.id,
        generation: battle.generation,
    };
    let mut app = App::new();
    app.insert_resource(battle)
        .insert_resource(DrawVisibility::new())
        .add_systems(Update, record);
    let parent = app.world_mut().spawn(visual).id();
    let part = app
        .world_mut()
        .spawn((
            ChildOf(parent),
            ViewVisibility::VISIBLE,
            AnimatedPart {
                model: 0,
                binding: std::sync::Arc::new(PartBinding {
                    mesh: 0,
                    pass: 0,
                    vertices: Vec::new(),
                    linear_mapper: None,
                }),
            },
        ))
        .id();
    app.update();
    assert!(!app.world().resource::<DrawVisibility>().obscured_at(
        app.world().resource::<Match>(),
        object,
        true
    ));
    app.world_mut()
        .entity_mut(part)
        .insert(ViewVisibility::HIDDEN);
    app.world_mut().resource_mut::<Match>().simulation.tick += 59;
    app.update();
    assert!(!app.world().resource::<DrawVisibility>().obscured_at(
        app.world().resource::<Match>(),
        object,
        true
    ));
    app.world_mut().resource_mut::<Match>().simulation.tick += 1;
    app.update();
    assert!(app.world().resource::<DrawVisibility>().obscured_at(
        app.world().resource::<Match>(),
        object,
        true
    ));
    app.world_mut()
        .entity_mut(part)
        .insert(ViewVisibility::VISIBLE);
    app.update();
    assert!(!app.world().resource::<DrawVisibility>().obscured_at(
        app.world().resource::<Match>(),
        object,
        true
    ));
    app.world_mut()
        .entity_mut(part)
        .insert(ViewVisibility::HIDDEN);
    app.world_mut().resource_mut::<Match>().observer_player = Some(1);
    app.update();
    assert!(app.world().resource::<DrawVisibility>().obscured_at(
        app.world().resource::<Match>(),
        object,
        true
    ));
}
