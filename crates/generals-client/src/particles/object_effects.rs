//! Original `SpecialAbilityUpdate` disable shower and `PropagandaTowerBehavior` FX.
use super::{
    Particles, Pose, Template,
    effects::{Nugget, parse_list, rng},
};
use generals_assets::content::ContentFile;
use generals_sim::{
    collision_geometry::{Geometry, Shape},
    skirmish::{
        Skirmish, UnitType,
        hacker_disable::{Effect, Event},
        visibility::Status,
    },
};
use num_traits::ToPrimitive;
use std::{collections::BTreeSet, error::Error};

pub(super) struct Attachments(BTreeSet<usize>);

impl Attachments {
    pub(super) const fn new() -> Self {
        Self(BTreeSet::new())
    }
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
}

pub(super) fn load(
    files: &[ContentFile],
    types: &[UnitType],
    templates: &[Template],
    lists: &mut Vec<Vec<Nugget>>,
) -> Result<Vec<[Option<usize>; 2]>, Box<dyn Error>> {
    types
        .iter()
        .map(|unit| {
            let mut result = [None; 2];
            if let Some(rules) = &unit.behavior.propaganda {
                for (slot, name) in result.iter_mut().zip(&rules.pulse) {
                    if !name.eq_ignore_ascii_case("NONE") {
                        let list = parse_list(
                            files,
                            {
                                let fx = crate::skirmish_data::fx_definition(files, name)?
                                    .ok_or_else(|| format!("missing FXList {name}"))?;
                                &fx.lines
                            },
                            templates,
                            lists,
                            true,
                        )?;
                        *slot = Some(lists.len());
                        lists.push(list);
                    }
                }
            }
            Ok(result)
        })
        .collect()
}

impl Particles {
    pub fn emp_sparks(
        &mut self,
        battle: &crate::playtest::Match,
        index: usize,
        animations: &mut crate::model_animation::Animations,
    ) {
        self.begin(battle.generation, battle.simulation.tick);
        let sparks = &battle.simulation.presentation.emp_sparks[index];
        let Some(template) = self
            .templates
            .iter()
            .position(|entry| entry.name == sparks.name)
        else {
            return;
        };
        let geometry = battle.simulation.body_geometry(sparks.target);
        let generation = battle.generation;
        for _ in 0..sparks.count {
            // CRC=0: native particle construction and offsets use client RNG,
            // after the pulse's gameplay callbacks have completed.
            let id = self.create(
                template,
                Pose::at([0.0; 3]),
                None,
                &mut super::random(animations, generation),
            );
            let mut offset = footprint(geometry, &mut super::random(animations, generation));
            offset[2] = animations
                .effect_integer(generation, [3, sparks.height.trunc().to_u32().unwrap_or(0)])
                .to_f32()
                .expect("EMP offset converts to Real");
            let squared = offset.map(|component| component * component);
            let length = ((squared[0] + squared[1]) + squared[2]).sqrt();
            if length > sparks.height {
                offset[2] = (offset[2] * (1.0 / length)) * sparks.height;
            }
            let emitter = self
                .emitters
                .iter_mut()
                .find(|emitter| emitter.id == id)
                .expect("created EMP spark");
            emitter.object_attachment = Some((sparks.target, Pose::at(offset)));
            emitter.remaining = sparks
                .lifetime
                .to_f32()
                .expect("EMP lifetime converts to Real");
            emitter.delay = animations
                .effect_integer(generation, [1, 100])
                .to_f32()
                .expect("EMP delay converts to Real");
        }
    }

    pub fn neutron_scorch(&mut self, battle: &crate::playtest::Match, index: usize) {
        self.begin(battle.generation, battle.simulation.tick);
        let scorch = &battle.simulation.presentation.neutron_scorches[index];
        self.scorches.add(scorch.position, scorch.radius, 0);
    }

    pub(crate) fn ability_feedback(
        &mut self,
        battle: &crate::playtest::Match,
        event: &generals_sim::skirmish::special_ability::FeedbackEvent,
        animations: &mut crate::model_animation::Animations,
    ) {
        use generals_sim::skirmish::special_ability::FeedbackEffect;
        let FeedbackEffect::DisableParticles {
            name,
            lifetime,
            geometry,
        } = &event.effect
        else {
            return;
        };
        self.begin(battle.generation, battle.simulation.tick);
        let Some(template) = self.templates.iter().position(|entry| entry.name == *name) else {
            return;
        };
        let mut random = super::random(animations, battle.generation);
        // Native construction draws precede the footprint offset draws.
        let id = self.create(template, Pose::at([0.0; 3]), None, &mut random);
        let offset = footprint(geometry, &mut random);
        let emitter = self
            .emitters
            .iter_mut()
            .find(|emitter| emitter.id == id)
            .expect("created disable shower");
        emitter.object_attachment = Some((event.target, Pose::at(offset)));
        emitter.remaining = lifetime.to_f32().expect("UnsignedInt converts to Real");
    }

    pub fn attach_particle(
        &mut self,
        battle: &crate::playtest::Match,
        index: usize,
        animations: &mut crate::model_animation::Animations,
    ) {
        self.begin(battle.generation, battle.simulation.tick);
        self.attach_particle_event(
            &battle.simulation.presentation.attached_particles[index],
            &mut super::random(animations, battle.generation),
        );
    }

    fn attach_particle_event(
        &mut self,
        event: &generals_sim::skirmish::helicopter_death::ParticleAttachment,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        let Some(template) = self
            .templates
            .iter()
            .position(|entry| entry.name == event.name)
        else {
            return;
        };
        let local = Pose::at(event.point);
        let id = self.create(template, local.clone(), None, random);
        self.emitters
            .iter_mut()
            .find(|emitter| emitter.id == id)
            .expect("created attached system")
            .object_attachment = Some((event.object, local));
    }

    pub(super) fn object_effects(
        &mut self,
        game: &Skirmish,
        random: &mut impl FnMut(rng::Request) -> rng::Value,
    ) {
        self.attach_created_objects(game, random);
        for (target, lifetime) in disabled_pulses(&game.presentation.disable_events, game.tick) {
            // SH decouples this shower from logic CRC/RNG. Constructor wind
            // draws precede GeometryInfo's client-random footprint draws.
            let Some(template) = self
                .templates
                .iter()
                .position(|template| template.name == "DisabledEffectBinaryShower0")
            else {
                continue;
            };
            self.create(template, Pose::at([0.0; 3]), None, &mut |low, high| {
                rng::real(random, low, high)
            });
            let rules = &game.catalog[game.entities[target].type_index];
            let offset = footprint(&rules.geometry, &mut |low, high| {
                rng::real(random, low, high)
            });
            let emitter = self.emitters.last_mut().expect("created disable shower");
            emitter.object_attachment = Some((target, Pose::at(offset)));
            emitter.remaining =
                f32::from(u16::try_from(lifetime).expect("validated disable FX lifetime"));
        }
        for event in &game.presentation.propaganda_events {
            let unit = &game.entities[event.source];
            if matches!(
                game.visibility_status(0, event.source),
                Status::Clear | Status::Partial
            ) && let Some(index) = self.propaganda[unit.type_index][usize::from(event.upgraded)]
            {
                self.fire(
                    index,
                    &object_pose(game, event.source, [0.0; 3]),
                    &[],
                    &|point| game.ground_height(point),
                    0.0,
                    random,
                );
            }
        }
        let mut destroyed = Vec::new();
        for emitter in &mut self.emitters {
            let Some((id, local)) = emitter.object_attachment.as_ref() else {
                continue;
            };
            let id = *id;
            if emitter.delay > 0.0 {
                continue;
            }
            if game.entities[id].exists() {
                if emitter.previous.is_some() {
                    emitter.previous = Some(emitter.transform.position);
                }
                emitter.transform =
                    super::model_states::compose(&object_pose(game, id, [0.0; 3]), local);
                let position = game.object_position(id);
                emitter.wind_origin = Some(std::array::from_fn(|axis| {
                    position[axis] + local.position[axis]
                }));
                emitter.shrouded = !matches!(
                    game.visibility_status(0, id),
                    Status::Clear | Status::Partial
                );
            } else {
                emitter.transform = local.clone();
                emitter.wind_origin = Some(local.position);
                emitter.object_attachment = None;
                destroyed.push(emitter.id);
            }
        }
        for id in destroyed {
            self.destroy(id);
        }
    }

    fn attach_created_objects(
        &mut self,
        game: &Skirmish,
        random: &mut impl FnMut(rng::Request) -> rng::Value,
    ) {
        for (id, actor) in game.entities.iter().enumerate() {
            if actor.masked() {
                continue;
            }
            let attachment = actor.physical_particle().map(|name| (name, [0.0; 3]));
            let Some((name, point)) = attachment
                .filter(|(name, _)| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
            else {
                continue;
            };
            if self.object_emitters.0.insert(id) {
                // Native findTemplate/createParticleSystem returns null for a
                // missing original template. It does not retry the callback.
                if let Some(template) = self
                    .templates
                    .iter()
                    .position(|template| template.name == name)
                {
                    let local = Pose::at(point);
                    self.create(template, local.clone(), None, &mut |low, high| {
                        rng::real(random, low, high)
                    });
                    self.emitters
                        .last_mut()
                        .expect("created attached particle system")
                        .object_attachment = Some((id, local));
                }
            }
        }
    }
}

// The event buffer retains the previous frame for audio requests initiated
// between logic updates. Disable particles are produced inside logic, once on
// their own frame; replaying the retained event duplicates both FX and RNG draws.
fn disabled_pulses(events: &[Event], frame: u64) -> impl Iterator<Item = (usize, u32)> + '_ {
    events.iter().filter_map(move |event| {
        if event.frame == frame
            && let Effect::Disabled { target, lifetime } = event.effect
        {
            Some((target, lifetime))
        } else {
            None
        }
    })
}

fn object_pose(game: &Skirmish, id: usize, offset: [f32; 3]) -> Pose {
    let (position, basis) = game.object_transform(id);
    let mut pose = Pose {
        position,
        axes: std::array::from_fn(|column| std::array::from_fn(|row| basis[row][column])),
    };
    let shifted = pose.rotate(offset);
    for (point, shift) in pose.position.iter_mut().zip(shifted) {
        *point += shift;
    }
    pose
}

fn footprint(geometry: &Geometry, random: &mut impl FnMut(f32, f32) -> f32) -> [f32; 3] {
    if matches!(geometry.shape, Shape::Box) {
        return [
            random(-geometry.major, geometry.major),
            random(-geometry.minor, geometry.minor),
            0.0,
        ];
    }
    let radius_squared = geometry.major * geometry.major;
    loop {
        let x = random(-geometry.major, geometry.major);
        let y = random(-geometry.major, geometry.major);
        let squared = [x * x, y * y];
        if squared[0] + squared[1] <= radius_squared {
            return [x, y, 0.0];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_audio_events_do_not_duplicate_disable_showers_or_random_draws() {
        let events = [
            Event {
                object: 3,
                frame: 40,
                effect: Effect::Disabled {
                    target: 8,
                    lifetime: 60,
                },
            },
            Event {
                object: 4,
                frame: 41,
                effect: Effect::DisabledEdge { target: 9 },
            },
            Event {
                object: 4,
                frame: 41,
                effect: Effect::Disabled {
                    target: 9,
                    lifetime: 120,
                },
            },
            Event {
                object: 5,
                frame: 41,
                effect: Effect::Reenabled { target: 10 },
            },
        ];
        let mut master = super::super::tests::template("DisabledEffectBinaryShower0");
        master.slave = Some(1);
        let slave = super::super::tests::template("DisabledEffectBinaryShower1");
        let mut particles = super::super::tests::system(vec![master, slave]);
        let mut creations = Vec::new();
        for frame in 40..=42 {
            for pulse in disabled_pulses(&events, frame) {
                creations.push((frame, pulse));
                particles.create(0, Pose::at([0.0; 3]), None, &mut |_, high| high);
            }
        }
        assert_eq!(creations, [(40, (8, 60)), (41, (9, 120))]);
        assert_eq!(particles.next_id, 4);
        assert_eq!(
            particles
                .emitters
                .iter()
                .filter(|emitter| !emitter.master)
                .count(),
            2
        );
    }

    #[test]
    fn circular_footprint_rejects_corners_and_preserves_client_draw_order() {
        let mut values = [10.0, 10.0, 6.0, 8.0].into_iter();
        let geometry = Geometry {
            shape: Shape::Cylinder,
            major: 10.0,
            minor: 1.0,
            height: 5.0,
            is_small: false,
        };
        let offset = footprint(&geometry, &mut |low, high| {
            assert_eq!(
                [low.to_bits(), high.to_bits()],
                [(-10.0_f32).to_bits(), 10.0_f32.to_bits()]
            );
            values.next().unwrap()
        });
        assert_eq!(
            offset.map(f32::to_bits),
            [6.0, 8.0, 0.0_f32].map(f32::to_bits)
        );
        assert!(values.next().is_none());
    }

    #[test]
    fn box_footprint_samples_distinct_authored_radii_without_rejection() {
        let geometry = Geometry {
            shape: Shape::Box,
            major: 12.0,
            minor: 4.0,
            height: 5.0,
            is_small: false,
        };
        let mut calls = Vec::new();
        let offset = footprint(&geometry, &mut |low, high| {
            calls.push([low, high]);
            high
        });
        assert_eq!(
            offset.map(f32::to_bits),
            [12.0, 4.0, 0.0_f32].map(f32::to_bits)
        );
        assert_eq!(
            calls
                .iter()
                .map(|range| range.map(f32::to_bits))
                .collect::<Vec<_>>(),
            vec![
                [-12.0, 12.0_f32].map(f32::to_bits),
                [-4.0, 4.0_f32].map(f32::to_bits)
            ]
        );
    }
}

#[cfg(test)]
mod attachment_tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn original_ocl_particle_attaches_once_follows_debris_and_resets_with_generation() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let list = game
            .catalog
            .iter()
            .find(|unit| unit.name == "ChinaVehicleHelix")
            .unwrap()
            .helicopter_death_rules()
            .next()
            .unwrap()
            .phases[0]
            .objects
            .as_ref()
            .unwrap()
            .clone();
        assert_eq!(list.nuggets[0].particle, "BuggyDebrisTireTrail");
        let source = crate::skirmish_verification::friendly_entity(
            game,
            generals_sim::skirmish::Kind::Builder,
            0,
        )
        .unwrap();
        let debris = game.create_physical_ocl(source, &list).unwrap();
        let attached: Vec<_> = debris
            .iter()
            .copied()
            .filter(|&id| game.entities[id].physical_particle() == Some("BuggyDebrisTireTrail"))
            .collect();
        assert_eq!(attached.len(), 3);
        let mut particles = super::super::data::load(&assets, &game.catalog).unwrap();
        let mut draws = 0;
        particles.object_effects(
            game,
            &mut rng::real_only(|low, _| {
                draws += 1;
                low
            }),
        );
        let created = particles.next_id;
        let first_draws = draws;
        assert!(created >= 3 && first_draws > 0);
        for &id in &attached {
            assert!(particles.emitters.iter().any(|emitter| {
                emitter
                    .object_attachment
                    .as_ref()
                    .is_some_and(|(object, _)| *object == id)
            }));
        }
        particles.object_effects(
            game,
            &mut rng::real_only(|low, _| {
                draws += 1;
                low
            }),
        );
        assert_eq!(particles.next_id, created);
        assert_eq!(
            draws, first_draws,
            "retained creation must not consume constructor RNG again"
        );
        game.step();
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        for &id in &attached {
            let emitter = particles
                .emitters
                .iter()
                .find(|emitter| {
                    emitter
                        .object_attachment
                        .as_ref()
                        .is_some_and(|(object, _)| *object == id)
                })
                .unwrap();
            assert_eq!(
                emitter.transform.position.map(f32::to_bits),
                game.object_position(id).map(f32::to_bits)
            );
        }
        // A finite system ending before its source object must not recreate.
        particles.emitters.clear();
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        assert_eq!(particles.next_id, created);
        assert!(particles.emitters.is_empty());
        particles.begin(battle.generation + 1, game.tick);
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        assert!(particles.next_id > created);
    }

    fn paid_helix_corpse(game: &mut Skirmish) -> (usize, [f32; 3]) {
        use generals_sim::skirmish::{Faction, Kind};

        game.reset(Faction::China, Faction::Usa).unwrap();
        let builder =
            crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0).unwrap();
        crate::skirmish_verification::construct(game, builder, Kind::PowerPlant, 0).unwrap();
        crate::skirmish_verification::construct(game, builder, Kind::SupplyCenter, 0).unwrap();
        let airfield_type = crate::strategy_verification::template(game, "ChinaAirfield").unwrap();
        let airfield =
            crate::skirmish_verification::construct_type(game, builder, airfield_type, 0).unwrap();
        let helicopter_type =
            crate::strategy_verification::template(game, "ChinaVehicleHelix").unwrap();
        let id =
            crate::skirmish_verification::produce_type(game, airfield, helicopter_type, 0).unwrap();
        let target =
            crate::skirmish_verification::friendly_entity(game, Kind::CommandCenter, 1).unwrap();
        let defenders = crate::helix_verification::crash::prepare(game, target).unwrap();
        game.move_to(id, game.entities[defenders[0]].position)
            .unwrap();
        crate::skirmish_verification::advance_until(
            game,
            9000,
            "paid Patriot starts Helix death",
            |game| {
                game.entities[id].helicopter_corpse() && !game.entities[id].helicopter_hit_ground()
            },
        )
        .unwrap();
        assert!(
            game.entities[id]
                .last_attacker()
                .is_some_and(|actor| defenders.contains(&actor))
        );
        let rules = game.catalog[game.entities[id].type_index]
            .helicopter_death_rules()
            .next()
            .unwrap();
        assert_eq!(rules.particle, "SootySmokeTrail");
        assert_eq!(rules.particle_bone, "Propeller02");
        let local = rules.particle_point;
        assert!(
            local.iter().any(|point| point.abs() > 0.0),
            "pristine original rotor point must be bound"
        );
        (id, local)
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_patriot_death_attaches_pristine_helix_smoke_until_final_destruction() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let (id, local) = paid_helix_corpse(game);
        let mut particles = super::super::data::load(&assets, &game.catalog).unwrap();
        for event in &game.presentation.attached_particles {
            particles.attach_particle_event(event, &mut |low, _| low);
        }
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        let emitter = particles
            .emitters
            .iter()
            .find(|emitter| {
                emitter
                    .object_attachment
                    .as_ref()
                    .is_some_and(|(object, _)| *object == id)
            })
            .unwrap();
        let master = emitter.id;
        assert_eq!(
            particles.templates[emitter.template].name,
            "SootySmokeTrail"
        );
        assert_eq!(
            emitter
                .object_attachment
                .as_ref()
                .unwrap()
                .1
                .position
                .map(f32::to_bits),
            local.map(f32::to_bits)
        );
        let expected = object_pose(game, id, local);
        assert_eq!(
            emitter.transform.position.map(f32::to_bits),
            expected.position.map(f32::to_bits)
        );
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        assert_eq!(
            particles
                .emitters
                .iter()
                .filter(|emitter| emitter
                    .object_attachment
                    .as_ref()
                    .is_some_and(|(object, _)| *object == id))
                .count(),
            1
        );
        let until = game.tick + 3000;
        while !game.entities[id].masked() && game.tick < until {
            game.step();
            particles.object_effects(game, &mut rng::real_only(|low, _| low));
            if !game.entities[id].masked() {
                let emitter = particles
                    .emitters
                    .iter()
                    .find(|emitter| emitter.id == master)
                    .unwrap();
                assert!(!emitter.lifecycle.destroyed);
                assert_eq!(
                    emitter.transform.position.map(f32::to_bits),
                    object_pose(game, id, local).position.map(f32::to_bits)
                );
            }
        }
        assert!(
            game.entities[id].masked(),
            "original Helix must reach final object destruction"
        );
        let emitter = particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == master)
            .unwrap();
        assert!(emitter.lifecycle.destroyed);
        assert!(emitter.object_attachment.is_none());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn parked_raptor_attachment_excludes_drawable_instance_height() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let setup = crate::aircraft_verification::prepare(game).unwrap();
        let actor = &game.entities[setup.raptor];
        assert!(actor.aircraft_draw_height() > 0.0);
        let position = game.object_position(setup.raptor);
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("attached")]);
        particles.create(0, Pose::at([0.0; 3]), None, &mut |low, _| low);
        particles.emitters[0].object_attachment = Some((setup.raptor, Pose::at([0.0; 3])));
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        assert_eq!(
            particles.emitters[0].transform.position.map(f32::to_bits),
            position.map(f32::to_bits)
        );
        particles.logic(game.tick, &mut |low, _| low, &|point| {
            game.ground_height(point)
        });
        assert_eq!(
            particles.emitters[0].particles[0]
                .position
                .map(f32::to_bits),
            position.map(f32::to_bits)
        );
    }

    fn original_raptor_hulk(game: &mut Skirmish) -> usize {
        let unit = game
            .catalog
            .iter()
            .find(|unit| unit.name == "AmericaJetRaptor")
            .unwrap();
        let list = generals_sim::skirmish::physical_ocl::lists(unit)
            .find(|list| {
                list.nuggets.iter().any(|nugget| {
                    nugget
                        .names
                        .iter()
                        .any(|name| name == "AmericaJetRaptorHulk")
                })
            })
            .unwrap()
            .clone();
        let source = crate::skirmish_verification::friendly_entity(
            game,
            generals_sim::skirmish::Kind::Builder,
            0,
        )
        .unwrap();
        game.create_physical_ocl(source, &list)
            .unwrap()
            .into_iter()
            .find(|&id| game.catalog[game.entities[id].type_index].name == "AmericaJetRaptorHulk")
            .unwrap()
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn inactive_hulk_retains_attachment_then_destroys_the_entire_slave_chain() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let hulk = original_raptor_hulk(game);
        assert!(!game.entities[hulk].alive());
        assert!(!game.entities[hulk].corpse_present());
        assert!(game.entities[hulk].physical_present());
        game.step();
        assert!(game.entities[hulk].physical_present());
        let mut particles = attached_chain();
        let master = particles.create(0, Pose::at([0.0; 3]), None, &mut |low, _| low);
        particles.emitters.last_mut().unwrap().object_attachment = Some((hulk, Pose::at([0.0; 3])));
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| !emitter.lifecycle.destroyed)
        );
        particles.logic(game.tick, &mut |low, _| low, &|_| 0.0);
        let counts: Vec<_> = particles
            .emitters
            .iter()
            .map(|e| e.particles.len())
            .collect();
        assert!(counts.iter().sum::<usize>() > 0);
        let deadline = game.tick + 600;
        while game.entities[hulk].physical_present() && game.tick < deadline {
            game.step();
            particles.object_effects(game, &mut rng::real_only(|low, _| low));
            if game.entities[hulk].physical_present() {
                assert!(
                    particles
                        .emitters
                        .iter()
                        .all(|emitter| !emitter.lifecycle.destroyed)
                );
                assert_eq!(
                    particles
                        .emitters
                        .last()
                        .unwrap()
                        .transform
                        .position
                        .map(f32::to_bits),
                    game.object_position(hulk).map(f32::to_bits)
                );
                particles.logic(game.tick, &mut |low, _| low, &|_| 0.0);
            }
        }
        assert!(!game.entities[hulk].physical_present());
        assert_eq!(particles.emitters.last().unwrap().id, master);
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| emitter.lifecycle.destroyed)
        );
        assert!(
            particles
                .emitters
                .last()
                .unwrap()
                .object_attachment
                .is_none()
        );
        assert_eq!(
            particles
                .emitters
                .iter()
                .map(|e| e.particles.len())
                .collect::<Vec<_>>(),
            counts,
            "final object destruction must not delete existing particles"
        );
        for frame in 0..610 {
            particles.logic(game.tick + frame, &mut |low, _| low, &|_| 0.0);
        }
        assert!(particles.emitters.is_empty());
    }

    fn attached_chain() -> Particles {
        let templates = (0..3)
            .map(|index| {
                let mut rules = super::super::tests::template("attached chain");
                rules.slave = (index < 2).then_some(index + 1);
                rules.system_lifetime = 0.0;
                rules.lifetime = super::super::Range {
                    low: 600.0,
                    high: 600.0,
                };
                // Hold opacity so this fixture exercises lifetime/destruction,
                // not the shared template's independent two-frame alpha expiry.
                rules.alpha[1].value = super::super::Range {
                    low: 1.0,
                    high: 1.0,
                };
                rules.burst_delay = super::super::Range {
                    low: 1000.0,
                    high: 1000.0,
                };
                rules
            })
            .collect();
        super::super::tests::system(templates)
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn attachment_follows_retained_dead_object_and_stops_at_final_destruction() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let setup = crate::demo_trap_verification::prepare(game).unwrap();
        let mut rules = super::super::tests::template("attached");
        rules.lifetime = super::super::Range {
            low: 90.0,
            high: 90.0,
        };
        rules.alpha[1].value = super::super::Range {
            low: 1.0,
            high: 1.0,
        };
        let mut particles = super::super::tests::system(vec![rules]);
        let id = particles.create(0, Pose::at([0.0; 3]), None, &mut |low, _| low);
        particles.emitters[0].object_attachment = Some((setup.trap, Pose::at([3.0, 4.0, 5.0])));
        game.switch_weapon(setup.trap, 0).unwrap();
        game.step();
        assert!(!game.entities[setup.trap].alive());
        assert!(game.entities[setup.trap].corpse_present());
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        particles.logic(game.tick, &mut |low, _| low, &|point| {
            game.ground_height(point)
        });
        assert_eq!(particles.emitters[0].id, id);
        assert!(!particles.emitters[0].lifecycle.destroyed);
        assert_eq!(particles.emitters[0].particles.len(), 1);
        let start = particles.emitters[0].transform.position;
        game.entities[setup.trap].position[0] += 20.0;
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        assert_eq!(
            particles.emitters[0].transform.position[0].to_bits(),
            (start[0] + 20.0).to_bits()
        );
        for _ in 0..30 {
            game.step();
            particles.object_effects(game, &mut rng::real_only(|low, _| low));
            particles.logic(game.tick, &mut |low, _| low, &|point| {
                game.ground_height(point)
            });
        }
        assert!(!game.entities[setup.trap].corpse_present());
        let emitter = &particles.emitters[0];
        assert!(emitter.object_attachment.is_none());
        assert!(emitter.lifecycle.destroyed);
        assert_eq!(
            emitter.transform.position.map(f32::to_bits),
            [3.0_f32, 4.0, 5.0].map(f32::to_bits)
        );
        assert_eq!(
            emitter.particles.len(),
            1,
            "destruction retains existing particles"
        );
        particles.begin(1, 0);
        assert!(particles.emitters.is_empty());
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn attachment_uses_existing_object_identity_even_without_corpse_or_physical_state() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let setup = crate::aircraft_verification::prepare(game).unwrap();
        let id = setup.raptor;
        // Consumer boundary: internal health changes do not delete an ObjectID.
        game.entities[id].health = 0.0;
        assert!(game.entities[id].exists());
        assert!(!game.entities[id].alive());
        assert!(!game.entities[id].corpse_present());
        assert!(!game.entities[id].physical_present());
        assert!(!game.entities[id].helicopter_corpse());
        assert!(!game.retained_rubble(id));
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("attached")]);
        particles.create(0, Pose::at([0.0; 3]), None, &mut |low, _| low);
        particles.emitters[0].object_attachment = Some((id, Pose::at([3.0, 4.0, 5.0])));
        particles.object_effects(game, &mut rng::real_only(|low, _| low));
        assert!(!particles.emitters[0].lifecycle.destroyed);
        assert_eq!(
            particles.emitters[0].object_attachment.as_ref().unwrap().0,
            id
        );
    }
}
