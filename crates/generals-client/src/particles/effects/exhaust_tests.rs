//! Original weapon bindings and missile ignition callbacks, including paid aircraft.
use super::{Nugget, WeaponEffects};
use crate::{
    aircraft_verification,
    model_animation::Animations,
    particles::{
        self, Particles, Pose, Style,
        tests::{system, template},
    },
};
use generals_sim::skirmish::{ProjectileFxEvent, ProjectileFxStage, Skirmish, WeaponFxEvent};

pub fn detached_ignition_sound(
    game: &Skirmish,
    particles: &mut Particles,
    animations: &mut Animations,
    missile: usize,
) -> bool {
    for fx in &game.presentation.weapon_fx {
        let WeaponFxEvent::Projectile(event) = fx else {
            continue;
        };
        if !matches!(event.stage, ProjectileFxStage::Ignition { projectile_type } if projectile_type == missile)
        {
            continue;
        }
        let weapon = game.catalog[event.weapon_type]
            .weapon_template(event.weapon_index)
            .unwrap();
        assert!(
            weapon
                .motion
                .as_ref()
                .is_none_or(|motion| motion.projectile_type.is_none())
        );
        particles.projectile_event(game, 1, event, None, animations);
        if std::mem::take(&mut particles.sounds)
            .into_iter()
            .any(|(sound, position)| {
                sound == "A10ThunderboltMissileWeaponSound"
                    && position.map(f32::to_bits) == event.position.map(f32::to_bits)
            })
        {
            return true;
        }
    }
    false
}

#[ignore = "requires original game installation"]
#[test]
fn installed_exhaust_bindings_preserve_normal_and_heroic_aircraft_and_launchers() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let game = &battle.simulation;
    let particles = particles::load(&assets, &game.catalog).unwrap();
    let files =
        [generals_assets::content::scan(assets.read("data/ini/weapon.ini").unwrap()).unwrap()];
    for name in [
        "AmericaJetRaptor",
        "ChinaJetMIG",
        "AmericaVehicleComanche",
        "AmericaInfantryMissileDefender",
        "ChinaInfantryTankHunter",
        "GLAInfantryTunnelDefender",
        "GLAInfantryStingerSoldier",
        "AmericaPatriotBattery",
        "AmericaVehicleHellfireDrone",
        "AmericaVehicleTomahawk",
        "GLAVehicleRocketBuggy",
        "GLAVehicleScudLauncher",
    ] {
        let index = crate::strategy_verification::template(game, name).unwrap();
        let unit = &game.catalog[index];
        for weapon_index in 0..unit.weapon_template_count() {
            let Some(weapon) = unit
                .weapon_template(weapon_index)
                .filter(|weapon| !weapon.name.is_empty())
            else {
                continue;
            };
            let definition =
                crate::skirmish_data::definition(&files, "Weapon", &weapon.name).unwrap();
            let normal = super::value(&definition.lines, "ProjectileExhaust")
                .unwrap()
                .unwrap_or("NONE");
            let heroic = super::veterancy(&definition.lines, "VeterancyProjectileExhaust")
                .unwrap()
                .unwrap_or(normal);
            let binding = particles.bindings[index][weapon_index]
                .as_ref()
                .expect("admitted launcher lost its weapon FX binding");
            for (actual, expected) in binding.exhaust.iter().zip([normal, heroic]) {
                let actual = actual.map(|template| particles.templates[template].name.as_str());
                assert_eq!(
                    actual,
                    (!expected.eq_ignore_ascii_case("NONE")).then_some(expected),
                    "{name}: {}",
                    weapon.name
                );
            }
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn missile_exhaust_waits_for_ignition_and_follows_ignition_fx_at_the_event_pose() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    let source = callback_source(game);
    let position = game.object_position(source);
    assert!(matches!(
        game.visibility_cell(0, [position[0], position[1]]),
        generals_sim::skirmish::visibility::Status::Clear
    ));
    for delay in [0, 12] {
        let mut particles = callback_fixture(game, source);
        let mut animations = Animations::effect_fixture(0);
        let mut event = ProjectileFxEvent {
            weapon_type: game.entities[source].type_index,
            weapon_index: 0,
            object: None,
            object_geometry_radius: None,
            weapon_slot: 0,
            projectile: 42,
            source,
            position,
            axes: Pose::at(position).axes,
            veterancy: 0,
            stage: ProjectileFxStage::Created,
        };
        particles.projectile_event(game, 1, &event, None, &mut animations);
        assert!(
            particles.emitters.is_empty(),
            "launch must not create exhaust"
        );
        for frame in 0..delay {
            particles.logic(frame, &mut |_, high| high, &|_| 0.0);
            assert!(particles.emitters.is_empty());
        }
        event.stage = ProjectileFxStage::Ignition {
            projectile_type: fixture_missile_type(game, source),
        };
        event.position[2] += 17.0;
        event.axes = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        particles.projectile_event(game, 1, &event, None, &mut animations);
        assert_eq!(particles.emitters.len(), 3);
        assert_eq!(
            particles
                .emitters
                .iter()
                .map(|emitter| emitter.template)
                .collect::<Vec<_>>(),
            [0, 2, 1]
        );
        let exhaust = &particles.emitters[2];
        assert_eq!(exhaust.attachment, Some(event.projectile));
        assert_eq!(
            exhaust.transform.position.map(f32::to_bits),
            event.position.map(f32::to_bits)
        );
        assert_eq!(
            exhaust.transform.axes.map(|axis| axis.map(f32::to_bits)),
            event.axes.map(|axis| axis.map(f32::to_bits))
        );
        assert_eq!(exhaust.slave, Some(particles.emitters[1].id));
        assert!(particles.emitters[1].master);
        assert!(particles.emitters[0].attachment.is_none());
    }
}

fn callback_fixture(game: &Skirmish, source: usize) -> Particles {
    let mut exhaust = template("exhaust");
    exhaust.slave = Some(2);
    let mut particles = system(vec![template("ignition"), exhaust, template("flare")]);
    particles.bindings = game.catalog.iter().map(|_| vec![None]).collect();
    particles.missile_ignition = vec![None; game.catalog.len()];
    particles.missile_ignition[fixture_missile_type(game, source)] = Some(0);
    particles.bindings[game.entities[source].type_index][0] = Some(WeaponEffects {
        fire: [None; 2],
        exhaust: [Some(1); 2],
        detonation: None,
        garrison_kill: None,
    });
    particles.lists.push(vec![Nugget::Particle {
        count: 1,
        radius: None,
        height: None,
        template: 0,
        attached: false,
        oriented: true,
        ricochet: false,
        offset: [0.0; 3],
        ground: false,
        caller_radius: false,
        delay: None,
    }]);
    particles
}

fn fixture_missile_type(game: &Skirmish, source: usize) -> usize {
    game.catalog[game.entities[source].type_index]
        .weapon
        .motion
        .as_ref()
        .unwrap()
        .projectile_type
        .unwrap()
}

fn callback_source(game: &mut Skirmish) -> usize {
    // A callback fixture uses the actual missile geometry/weapon declaration;
    // paid production and real event scheduling are checked separately below.
    let source = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0)
        .unwrap();
    game.entities[source].type_index =
        crate::strategy_verification::template(game, "AmericaJetRaptor").unwrap();
    source
}

#[ignore = "requires original game installation"]
#[test]
fn shrouded_ignition_still_creates_exhaust_but_not_visible_ignition_fx() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    let source = callback_source(game);
    let enemy = game
        .entities
        .iter()
        .position(|actor| actor.owner == 1)
        .unwrap();
    let position = game.object_position(enemy);
    assert!(!matches!(
        game.visibility_cell(0, [position[0], position[1]]),
        generals_sim::skirmish::visibility::Status::Clear
    ));
    let mut particles = callback_fixture(game, source);
    let event = ProjectileFxEvent {
        weapon_type: game.entities[source].type_index,
        weapon_index: 0,
        object: None,
        object_geometry_radius: None,
        weapon_slot: 0,
        projectile: 42,
        source,
        position,
        axes: Pose::at(position).axes,
        veterancy: 0,
        stage: ProjectileFxStage::Ignition {
            projectile_type: fixture_missile_type(game, source),
        },
    };
    particles.projectile_event(game, 1, &event, None, &mut Animations::effect_fixture(0));
    assert_eq!(
        particles
            .emitters
            .iter()
            .map(|emitter| emitter.template)
            .collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(particles.emitters[1].attachment, Some(42));
}

#[ignore = "requires original game installation"]
#[test]
fn partial_projectile_footprint_runs_ignition_fx_before_exhaust_even_with_hidden_center() {
    use generals_sim::{
        collision_geometry::Body,
        skirmish::{FlightRules, visibility::Status},
    };
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    let source = callback_source(game);
    let origin = game.object_position(source);
    let weapon = game.catalog[game.entities[source].type_index]
        .weapon_template(0)
        .unwrap();
    let projectile = weapon.motion.as_ref().unwrap().projectile_type.unwrap();
    let FlightRules::Missile(rules) = &game.catalog[projectile].projectile_rules[0] else {
        panic!("actual Raptor missile rules")
    };
    let position = (0..1000_u16)
        .map(|offset| [origin[0] + f32::from(offset), origin[1], 100.0])
        .find(|position| {
            !matches!(
                game.visibility_cell(0, [position[0], position[1]]),
                Status::Clear
            ) && matches!(
                game.visibility_footprint(
                    0,
                    &Body {
                        position: *position,
                        angle: 0.0,
                        geometry: &rules.geometry
                    }
                ),
                Status::Partial
            )
        })
        .expect("original vision boundary intersects actual missile footprint");
    let event = ProjectileFxEvent {
        weapon_type: game.entities[source].type_index,
        weapon_index: 0,
        object: None,
        object_geometry_radius: None,
        weapon_slot: 0,
        projectile: 42,
        source,
        position,
        axes: Pose::at(position).axes,
        veterancy: 0,
        stage: ProjectileFxStage::Ignition {
            projectile_type: fixture_missile_type(game, source),
        },
    };
    let mut particles = callback_fixture(game, source);
    particles.projectile_event(game, 1, &event, None, &mut Animations::effect_fixture(0));
    assert_eq!(
        particles
            .emitters
            .iter()
            .map(|emitter| emitter.template)
            .collect::<Vec<_>>(),
        [0, 2, 1]
    );
    assert_eq!(particles.emitters[2].attachment, Some(42));
}

#[ignore = "requires original game installation"]
#[test]
fn paid_aircraft_missiles_bind_and_emit_original_streaks_and_slave_flares() {
    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let game = &mut battle.simulation;
    let setup = aircraft_verification::prepare(game).unwrap();
    aircraft_verification::research_rocket_pods(game, &setup).unwrap();
    let mut particles = particles::load(&assets, &game.catalog).unwrap();
    for (actor, slot) in [(setup.raptor, 0), (setup.comanche, 1), (setup.comanche, 2)] {
        aircraft_verification::approach(game, actor, setup.target).unwrap();
        if slot == 2 {
            game.fire_weapon_at(actor, slot, game.entities[setup.target].position)
                .unwrap();
        } else {
            game.attack(actor, setup.target).unwrap();
        }
        particles.begin(1, game.tick);
        check_paid_exhaust(game, &mut particles, &mut animations, actor, slot);
        game.stop(actor).unwrap();
    }
}

fn check_paid_exhaust(
    game: &mut Skirmish,
    particles: &mut Particles,
    animations: &mut Animations,
    actor: usize,
    slot: usize,
) {
    let mut created = std::collections::BTreeSet::new();
    let mut ignited = false;
    for _ in 0..3600 {
        game.step();
        for event in &game.presentation.weapon_fx {
            let WeaponFxEvent::Projectile(event) = event else {
                continue;
            };
            if event.source != actor || event.weapon_slot != slot {
                continue;
            }
            particles.projectile_event(game, 1, event, None, animations);
            if matches!(event.stage, ProjectileFxStage::Created) {
                assert!(created.insert(event.projectile));
                assert!(
                    !particles
                        .emitters
                        .iter()
                        .any(|emitter| emitter.attachment == Some(event.projectile))
                );
            } else if matches!(event.stage, ProjectileFxStage::Ignition { .. }) {
                assert!(
                    created.contains(&event.projectile),
                    "ignition must follow the missile birth callback"
                );
                let exhaust = particles
                    .emitters
                    .iter()
                    .find(|emitter| emitter.attachment == Some(event.projectile))
                    .expect("installed aircraft exhaust binding missing");
                assert_eq!(particles.templates[exhaust.template].name, "MissileExhaust");
                assert_eq!(
                    exhaust.transform.position.map(f32::to_bits),
                    event.position.map(f32::to_bits)
                );
                ignited = true;
            }
        }
        for emitter in &mut particles.emitters {
            if let Some(projectile) = game
                .projectiles
                .iter()
                .find(|projectile| emitter.attachment == Some(projectile.id))
            {
                let (pose, shrouded) = super::attached_pose(game, projectile).unwrap();
                emitter.previous = Some(emitter.transform.position);
                emitter.transform = pose;
                emitter.shrouded = shrouded;
            }
        }
        particles.logic(game.tick, &mut particles::random(animations, 1), &|point| {
            game.ground_height(point)
        });
        particles.draw(1.0);
        if ignited && rendered_exhaust(particles, &created) {
            return;
        }
    }
    panic!(
        "paid aircraft {actor} slot {slot} did not render original exhaust; {}",
        aircraft_verification::details(game, actor)
    );
}

fn rendered_exhaust(particles: &Particles, projectiles: &std::collections::BTreeSet<u64>) -> bool {
    for emitter in &particles.emitters {
        if !emitter
            .attachment
            .is_some_and(|id| projectiles.contains(&id))
            || emitter.particles.len() < 2
        {
            continue;
        }
        let template = &particles.templates[emitter.template];
        assert!(matches!(template.drawing.style, Style::Streak));
        assert!(matches!(
            template.drawing.shader,
            crate::particles::Shader::Additive
        ));
        let slave = particles
            .emitters
            .iter()
            .find(|slave| Some(slave.id) == emitter.slave)
            .unwrap();
        assert!(slave.master);
        assert_eq!(Some(slave.template), template.slave);
        assert_eq!(particles.templates[slave.template].name, "MissileLenzflare");
        if slave.particles.is_empty() {
            continue;
        }
        for system in [emitter, slave] {
            let rules = &particles.templates[system.template];
            assert!(particles.textures[rules.texture].image.is_some());
            let mesh = particles::render::mesh(
                system,
                rules,
                bevy::math::Affine3A::IDENTITY,
                bevy::math::Affine3A::IDENTITY,
            )
            .unwrap();
            assert!(mesh.indices().is_some_and(|indices| indices.len() >= 6));
            assert!(
                system
                    .particles
                    .iter()
                    .any(|particle| particle.color.iter().any(|channel| *channel > 0.0))
            );
        }
        return true;
    }
    false
}
