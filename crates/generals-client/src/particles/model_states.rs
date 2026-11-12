//! `W3DModelDraw` state identity, drawable attachment and particle start/stop.
use super::{Particles, Pose, Template, random};
use crate::{
    model_animation::{Animations, Players, state_particles::ParticleDraw},
    playtest::Match,
    playtest_view::UnitVisual,
    skirmish_data::{blocks, definition},
};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Skirmish, UnitType, visibility::Status};
use std::{collections::BTreeMap, error::Error};
pub(in crate::particles) mod view;

type CameraView = (&'static Transform, &'static Projection, &'static Camera);

pub(super) struct States {
    bindings: BTreeMap<usize, Vec<Draw>>,
    active: BTreeMap<(usize, usize), Active>,
    frame: Option<u64>,
}

struct Active {
    source: (usize, usize),
    systems: Option<Vec<u64>>,
}

impl States {
    pub(super) const fn empty() -> Self {
        Self {
            bindings: BTreeMap::new(),
            active: BTreeMap::new(),
            frame: None,
        }
    }
}

pub(super) struct Pending {
    bindings: BTreeMap<usize, Vec<ParticleDraw>>,
}

struct Draw {
    states: Vec<Vec<usize>>,
    bones: Vec<Vec<String>>,
    animated: bool,
}

pub(super) enum Attachment {
    Local(Pose),
    World(Pose),
}

impl Attachment {
    const fn pose(&self) -> &Pose {
        match self {
            Self::Local(pose) | Self::World(pose) => pose,
        }
    }

    fn transform(&self, parent: &Pose) -> Pose {
        match self {
            Self::Local(pose) => compose(parent, pose),
            Self::World(pose) => pose.clone(),
        }
    }
}

impl Pending {
    pub(super) fn load(assets: &VirtualAssets, types: &[UnitType]) -> Result<Self, Box<dyn Error>> {
        let mut result = Self {
            bindings: BTreeMap::new(),
        };
        let files = [
            "object/americavehicle",
            "object/chinaair",
            "object/chinavehicle",
            "object/glavehicle",
            "object/system",
            "object/americainfantry",
            "object/chinainfantry",
            "object/glainfantry",
            "object/civilianbuilding",
        ]
        .into_iter()
        .map(|name| content::scan(assets.read(&format!("data/ini/{name}.ini"))?))
        .collect::<Result<Vec<_>, generals_assets::AssetError>>()?;
        for (index, unit) in types.iter().enumerate().filter(|(_, unit)| {
            unit.structure_death.is_some()
                || matches!(
                    unit.name.as_str(),
                    "GLAVehicleToxinTruck"
                        | "ChinaJetMIG"
                        | "ChinaVehicleHelix"
                        | "ChinaHelixGattlingCannon"
                        | "ChinaHelixPropagandaTower"
                        | "ChinaHelixBattleBunker"
                        | "AmericaVehicleDozer"
                        | "ChinaVehicleDozer"
                        | "FlamingInfantry"
                        | "AmericaInfantryPathfinder"
                        | "ChinaInfantryBlackLotus"
                        | "GLAInfantryJarmenKell"
                )
        }) {
            let object = definition(&files, "Object", &unit.name)?;
            let draws = blocks(&object.lines, 0, "Draw")?
                .into_iter()
                .enumerate()
                .filter(|(index, lines)| {
                    *index == 0
                        || lines[0]
                            .tokens
                            .get(1)
                            .is_some_and(|name| name == "W3DModelDraw")
                })
                .map(|(_, lines)| crate::model_animation::state_particles::particle_draw(lines))
                .collect::<Result<_, _>>()?;
            result.bindings.insert(index, draws);
        }
        Ok(result)
    }

    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.bindings
            .values()
            .flatten()
            .flat_map(|draw| draw.states.iter().flatten().map(String::as_str))
    }

    pub(super) fn resolve(self, templates: &[Template]) -> Result<States, Box<dyn Error>> {
        let mut result = States::empty();
        for (unit, draws) in self.bindings {
            let draws = draws.into_iter().map(|draw| {
                let states = draw.states.into_iter().map(|names| {
                    names.into_iter().map(|name| {
                        let index = templates.iter().position(|template| template.name == name)
                            .ok_or_else(|| format!("missing required model-state ParticleSystem {name}"))?;
                        let template = &templates[index];
                        if template.slave.is_some() || template.attached.is_some() {
                            return Err(format!("{name}: model-state particle children are not translated").into());
                        }
                        Ok(index)
                    }).collect::<Result<_, Box<dyn Error>>>()
                }).collect::<Result<_, _>>()?;
                Ok(Draw { states, bones: draw.bones, animated: draw.animated })
            }).collect::<Result<_, Box<dyn Error>>>()?;
            result.bindings.insert(unit, draws);
        }
        Ok(result)
    }
}

impl States {
    pub(super) fn reset(&mut self) {
        self.active.clear();
        self.frame = None;
    }
}

struct DrawState {
    unit: usize,
    state: usize,
    in_view: bool,
    stopped: bool,
    pose: Pose,
    offsets: Vec<Option<Pose>>,
}

pub fn update(
    battle: Res<Match>,
    mut particles: ResMut<Particles>,
    mut animations: ResMut<Animations>,
    visuals: Query<(&UnitVisual, &Players)>,
    camera: Single<CameraView, With<crate::playtest_view::WorldCamera>>,
    terrain: Res<crate::terrain::Terrain>,
) {
    let battle = battle.into_inner();
    if !battle.started || particles.model_states.bindings.is_empty() {
        return;
    }
    let game = &battle.simulation;
    let (transform, projection, camera) = camera.into_inner();
    let region = view::region(
        transform,
        projection,
        camera,
        terrain.into_inner().draw_region(),
    );
    particles.begin(battle.generation, game.tick);
    let mut states = BTreeMap::new();
    for (visual, players) in &visuals {
        if visual.generation != battle.generation {
            continue;
        }
        let unit = &game.entities[visual.index];
        if !particles
            .model_states
            .bindings
            .contains_key(&unit.type_index)
            || !drawable_present(game, visual.index)
        {
            continue;
        }
        for (module, state) in players.particle_states() {
            let key = (visual.index, module);
            let in_view = view::contains(region, unit.position);
            let stopped = emission_stopped(game, visual.index);
            let binding = &particles.model_states.bindings[&unit.type_index][module];
            let active = particles.model_states.active.get(&key);
            let needs_bones = active.is_none_or(|active| {
                active.source != (unit.type_index, state) || active.systems.is_none()
            });
            let update_bones = binding.animated && !needs_bones;
            let offsets = if (update_bones || needs_bones) && in_view && !stopped {
                sample_bones(&animations, unit, players, module, &binding.bones[state])
            } else {
                Vec::new()
            };
            states.insert(
                key,
                DrawState {
                    unit: unit.type_index,
                    state,
                    in_view,
                    stopped,
                    pose: drawable_pose(game, visual.index),
                    offsets,
                },
            );
        }
    }
    particles.update_draw_particles(
        &states,
        game.tick,
        battle.paused,
        &|point| game.ground_height(point),
        &mut random(&mut animations, battle.generation),
    );
}

fn sample_bones(
    animations: &Animations,
    unit: &generals_sim::skirmish::Entity,
    players: &Players,
    module: usize,
    names: &[String],
) -> Vec<Option<Pose>> {
    names
        .iter()
        .map(|name| {
            animations
                .particle_bone(unit, players, module, name)
                .expect("validated model-state particle bone pose")
                .map(|bone| Pose {
                    position: bone.translation,
                    axes: bone.axes,
                })
        })
        .collect()
}

impl Particles {
    fn update_draw_particles(
        &mut self,
        states: &BTreeMap<(usize, usize), DrawState>,
        frame: u64,
        paused: bool,
        ground: &impl Fn([f32; 2]) -> f32,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        self.sync_model_states(states, random);
        if !paused && self.model_states.frame != Some(frame) {
            self.model_states.frame = Some(frame);
            self.model_logic(frame, ground, random);
        }
    }

    fn sync_model_states(
        &mut self,
        states: &BTreeMap<(usize, usize), DrawState>,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        let removed: Vec<_> = self
            .model_states
            .active
            .keys()
            .filter(|key| !states.contains_key(key))
            .copied()
            .collect();
        for key in removed {
            self.stop_model_state(key);
        }
        // Native drawables are prepended; draw modules retain declaration order.
        let mut ordered: Vec<_> = states.iter().collect();
        ordered.sort_unstable_by_key(|(key, _)| (std::cmp::Reverse(key.0), key.1));
        for (&key, draw) in ordered {
            let source = (draw.unit, draw.state);
            if self
                .model_states
                .active
                .get(&key)
                .is_none_or(|active| active.source != source)
            {
                self.stop_model_state(key);
                self.model_states.active.insert(
                    key,
                    Active {
                        source,
                        systems: None,
                    },
                );
            }
            if self.model_states.active[&key].systems.is_some() || !draw.in_view || draw.stopped {
                continue;
            }
            let templates =
                self.model_states.bindings[&draw.unit][key.1].states[draw.state].clone();
            let mut systems = Vec::new();
            for (binding, template) in templates.into_iter().enumerate() {
                let offset = draw
                    .offsets
                    .get(binding)
                    .and_then(Option::as_ref)
                    .map(local_offset);
                let pose = offset.as_ref().map_or_else(
                    || Pose {
                        position: draw.pose.position,
                        axes: draw.pose.axes,
                    },
                    |offset| compose(&draw.pose, offset),
                );
                let system = self.create(template, pose, None, random);
                let emitter = self
                    .emitters
                    .last_mut()
                    .expect("created model-state particles");
                emitter.drawable_attachment = Some(key);
                emitter.drawable_offset = offset.map(Attachment::Local);
                emitter.shrouded = draw.stopped;
                systems.push(system);
            }
            self.model_states
                .active
                .get_mut(&key)
                .expect("tracked model state")
                .systems = Some(systems);
        }
        self.update_model_attachments(states);
    }

    fn stop_model_state(&mut self, key: (usize, usize)) {
        let Some(active) = self.model_states.active.remove(&key) else {
            return;
        };
        // Native destroy stops new emission; existing particles keep their world
        // positions and normal lifespan. A state alias retains the same IDs.
        for emitter in &mut self.emitters {
            if active
                .systems
                .as_ref()
                .is_some_and(|systems| systems.contains(&emitter.id))
            {
                emitter.lifecycle.destroyed = true;
            }
        }
    }

    fn update_model_attachments(&mut self, states: &BTreeMap<(usize, usize), DrawState>) {
        for emitter in &mut self.emitters {
            let Some(key) = emitter.drawable_attachment else {
                continue;
            };
            let Some(draw) = states.get(&key) else {
                emitter.lifecycle.destroyed = true;
                emitter.wind_origin = Some(
                    emitter
                        .drawable_offset
                        .as_ref()
                        .map_or([0.0; 3], |offset| offset.pose().position),
                );
                continue;
            };
            // updateBonesForClientParticleSystems updates only the current
            // state's tracked, nonzero bones. Draining old states retain their
            // final WORLD matrix; missing/NONE bones keep normal attachment.
            if draw.in_view
                && !draw.stopped
                && self.model_states.bindings[&draw.unit][key.1].animated
                && let Some(binding) = self
                    .model_states
                    .active
                    .get(&key)
                    .and_then(|active| active.systems.as_ref())
                    .and_then(|systems| systems.iter().position(|&id| id == emitter.id))
                && let Some(Some(bone)) = draw.offsets.get(binding)
            {
                emitter.drawable_offset = Some(Attachment::World(compose(&draw.pose, bone)));
            }
            emitter.shrouded = draw.stopped;
            // Particle::doWindMotion adds getPosition() to drawable position
            // even when skipParentXfrm makes that stored matrix already WORLD.
            emitter.wind_origin = Some(emitter.drawable_offset.as_ref().map_or(
                draw.pose.position,
                |offset| {
                    std::array::from_fn(|axis| {
                        draw.pose.position[axis] + offset.pose().position[axis]
                    })
                },
            ));
            emitter.transform = emitter.drawable_offset.as_ref().map_or_else(
                || Pose {
                    position: draw.pose.position,
                    axes: draw.pose.axes,
                },
                |offset| offset.transform(&draw.pose),
            );
        }
    }

    fn model_logic(
        &mut self,
        frame: u64,
        ground: &impl Fn([f32; 2]) -> f32,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        for index in 0..self.emitters.len() {
            if self.emitters[index].drawable_attachment.is_some() {
                self.update_emitter(index, frame, random, ground);
                let emitter = &mut self.emitters[index];
                if emitter.previous.is_some() {
                    emitter.previous = Some(emitter.transform.position);
                }
            }
        }
        self.retire();
    }
}

fn local_offset(bone: &Pose) -> Pose {
    // Native recalcBones stores only local position and Z rotation at birth.
    let angle = bone.axes[0][1].atan2(bone.axes[0][0]);
    let (sine, cosine) = angle.sin_cos();
    Pose {
        position: bone.position,
        axes: [[cosine, sine, 0.0], [-sine, cosine, 0.0], [0.0, 0.0, 1.0]],
    }
}

pub(super) fn compose(parent: &Pose, local: &Pose) -> Pose {
    let offset = parent.rotate(local.position);
    Pose {
        position: std::array::from_fn(|axis| parent.position[axis] + offset[axis]),
        axes: local.axes.map(|axis| parent.rotate(axis)),
    }
}

fn drawable_present(game: &Skirmish, id: usize) -> bool {
    let unit = &game.entities[id];
    unit.alive() || unit.corpse_present() || game.retained_rubble(id)
}

fn emission_stopped(game: &Skirmish, id: usize) -> bool {
    game.entities[id].masked()
        || !matches!(
            game.visibility_status(0, id),
            Status::Clear | Status::Partial
        )
}

fn drawable_pose(game: &Skirmish, id: usize) -> Pose {
    let (sine, cosine) = game.entities[id].facing.sin_cos();
    Pose {
        position: game.object_position(id),
        axes: [[cosine, sine, 0.0], [-sine, cosine, 0.0], [0.0, 0.0, 1.0]],
    }
}

#[cfg(test)]
mod tests;
