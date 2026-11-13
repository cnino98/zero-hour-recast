//! `W3DModelDraw` state identity, drawable attachment and particle start/stop.
use super::{Particles, Pose, Template, random};
use crate::{
    light_environment::DrawPose,
    model_animation::{Animations, Players, state_particles::ParticleDraw},
    playtest::Match,
    skirmish_data::{blocks, definition},
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Skirmish, UnitType, visibility::Status};
use std::{collections::BTreeMap, error::Error};
mod tanks;
pub(super) mod view;
mod wheels;

pub(super) struct States {
    bindings: BTreeMap<usize, Vec<Draw>>,
    active: BTreeMap<(usize, usize), Active>,
    incarnations: BTreeMap<usize, u64>,
    wheel_bindings: BTreeMap<usize, wheels::Binding>,
    tank_bindings: BTreeMap<(usize, usize), [Option<usize>; 2]>,
    tanks: BTreeMap<(usize, usize), tanks::Active>,
    wheels: BTreeMap<usize, wheels::Active>,
    pub(super) wheel_sounds: Vec<WheelSound>,
}

pub(super) enum WheelSound {
    Draw {
        object: usize,
        landing: bool,
        sliding: bool,
    },
    Retire(usize),
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
            incarnations: BTreeMap::new(),
            wheel_bindings: BTreeMap::new(),
            tank_bindings: BTreeMap::new(),
            tanks: BTreeMap::new(),
            wheels: BTreeMap::new(),
            wheel_sounds: Vec::new(),
        }
    }
}

pub(super) struct Pending {
    bindings: BTreeMap<usize, Vec<ParticleDraw>>,
    wheels: BTreeMap<usize, wheels::Names>,
    tanks: BTreeMap<(usize, usize), tanks::Names>,
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
            wheels: BTreeMap::new(),
            tanks: BTreeMap::new(),
        };
        let files = [
            "object/americamiscunit",
            "object/americavehicle",
            "object/chinaair",
            "object/chinavehicle",
            "object/glavehicle",
            "object/system",
            "object/americainfantry",
            "object/chinainfantry",
            "object/glainfantry",
            "object/civilianbuilding",
            "object/factionbuilding",
        ]
        .into_iter()
        .map(|name| content::scan(assets.read(&format!("data/ini/{name}.ini"))?))
        .collect::<Result<Vec<_>, generals_assets::AssetError>>()?;
        for (index, unit) in types.iter().enumerate().filter(|(_, unit)| {
            unit.structure_death.is_some()
                || unit.fake_building()
                || crate::skirmish_data::REAL_FAKE_REPLACEMENTS.contains(&unit.name.as_str())
                || unit.combat_bike.is_some()
                || matches!(
                    unit.name.as_str(),
                    "GLAVehicleToxinTruck"
                        | "GLAVehicleCombatBike"
                        | "GLAVehicleBombTruck"
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
                        | "CountermeasureFlare"
                )
        }) {
            let object = definition(&files, "Object", &unit.name)?;
            let blocks = blocks(&object.lines, 0, "Draw")?;
            if (unit.combat_bike.is_some() || unit.name == "GLAVehicleBombTruck")
                && let Some(draw) = blocks
                    .first()
                    .filter(|lines| lines[0].tokens[1] == "W3DTruckDraw")
            {
                result.wheels.insert(index, wheels::Names::load(draw)?);
            }
            let draws = blocks
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
        result.load_motion(&files, types)?;
        Ok(result)
    }

    fn load_motion(
        &mut self,
        files: &[content::ContentFile],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        for (index, unit) in types.iter().enumerate() {
            if self.wheels.contains_key(&index)
                || !unit.kind_of.iter().any(|kind| kind == "VEHICLE")
            {
                continue;
            }
            let Some(object) = files
                .iter()
                .flat_map(|file| &file.declarations)
                .find(|entry| entry.kind == "Object" && entry.name == unit.name)
            else {
                continue;
            };
            let draws = blocks(&object.lines, 0, "Draw")?;
            for (module, draw) in draws.iter().enumerate() {
                if matches!(
                    draw[0].tokens[1].as_str(),
                    "W3DTankDraw" | "W3DOverlordTankDraw"
                ) {
                    self.tanks
                        .insert((index, module), tanks::Names::load(draw)?);
                }
            }
            let Some(draw) = draws.first().filter(|lines| {
                matches!(
                    lines[0].tokens[1].as_str(),
                    "W3DTruckDraw" | "W3DOverlordTruckDraw" | "W3DTankTruckDraw"
                )
            }) else {
                continue;
            };
            self.wheels.insert(index, wheels::Names::load(draw)?);
        }
        Ok(())
    }

    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.bindings
            .values()
            .flatten()
            .flat_map(|draw| draw.states.iter().flatten().map(String::as_str))
    }

    pub(super) fn wheel_names(&self) -> impl Iterator<Item = &str> {
        self.wheels
            .values()
            .flat_map(|wheel| wheel.effects.iter().flatten().map(String::as_str))
    }

    pub(super) fn tank_names(&self) -> impl Iterator<Item = &str> {
        self.tanks
            .values()
            .flat_map(|names| names.0.iter().map(String::as_str))
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("None"))
    }

    pub(super) fn resolve(self, templates: &[Template]) -> Result<States, Box<dyn Error>> {
        let mut result = States::empty();
        for (key, names) in self.tanks {
            result.tank_bindings.insert(key, names.resolve(templates)?);
        }
        for (unit, wheel) in self.wheels {
            result
                .wheel_bindings
                .insert(unit, wheel.resolve(templates)?);
        }
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
        self.incarnations.clear();
        self.tanks.clear();
        self.wheels.clear();
        self.wheel_sounds.clear();
    }
}

pub(super) struct DrawState {
    unit: usize,
    state: usize,
    in_view: bool,
    stopped: bool,
    pose: Pose,
    offsets: Vec<Option<Pose>>,
    wheels: Option<wheels::Sample>,
    tank: Option<tanks::Sample>,
}

#[cfg(test)]
impl Particles {
    pub(crate) fn draw_order_fixture(types: &[usize]) -> Self {
        let mut template = super::tests::template("draw order");
        template.initial_delay = super::Range {
            low: 10.0,
            high: 20.0,
        };
        let mut result = super::tests::system(vec![template]);
        for &kind in types {
            result.model_states.bindings.insert(
                kind,
                (0..2)
                    .map(|_| Draw {
                        states: vec![vec![0]],
                        bones: vec![vec!["None".into()]],
                        animated: false,
                    })
                    .collect(),
            );
        }
        result
    }
    pub(crate) fn draw_order_delays(&self) -> Vec<((usize, usize), f32)> {
        self.emitters
            .iter()
            .map(|emitter| {
                (
                    emitter
                        .drawable_attachment
                        .expect("draw fixture attachment"),
                    emitter.delay,
                )
            })
            .collect()
    }
}

fn model_pose(game: &Skirmish, id: usize, pose: Option<&DrawPose>) -> Pose {
    if let Some(pose) = pose
        && game.catalog[game.entities[id].type_index]
            .combat_bike
            .is_some()
    {
        Pose {
            position: pose.position,
            axes: std::array::from_fn(|column| pose.basis.map(|row| row[column])),
        }
    } else {
        drawable_pose(game, id)
    }
}

pub struct ModuleDraw<'a> {
    pub key: (usize, usize),
    pub players: &'a Players,
    pub pose: Option<&'a DrawPose>,
}

/// One native visible-draw traversal; animation and emitter constructors share RNG.
pub struct DrawFrame<'a> {
    battle: &'a Match,
    visibility: Option<&'a crate::model_animation::draw_visibility::DrawVisibility>,
    region: [[f32; 2]; 2],
    committed: bool,
    states: BTreeMap<(usize, usize), DrawState>,
    projectiles: BTreeMap<usize, &'a generals_sim::skirmish::Projectile>,
    fires: Vec<usize>,
}
impl<'a> DrawFrame<'a> {
    pub(crate) fn new(
        particles: &mut Particles,
        battle: &'a Match,
        visibility: Option<&'a crate::model_animation::draw_visibility::DrawVisibility>,
        region: [[f32; 2]; 2],
        committed: bool,
    ) -> Self {
        let game = &battle.simulation;
        particles.begin(battle.generation, game.tick);
        particles.prepare_fire_drawables(game, &|id| {
            crate::model_animation::draw_visibility::hidden(visibility, battle, id)
        });
        let projectiles = particles.fire_projectile_draws(
            game,
            battle.observer_player.unwrap_or(0),
            &|id, raw| projectile_hidden(visibility, battle, id, raw),
        );
        let fires = particles.fire_object_draws(game);
        let retired: Vec<_> = particles
            .model_states
            .incarnations
            .iter()
            .filter_map(|(&id, &birth)| {
                game.entities
                    .get(id)
                    .is_none_or(|actor| !actor.has_drawable() || actor.drawable_birth() != birth)
                    .then_some(id)
            })
            .collect();
        for id in retired {
            particles.retire_drawable(id);
        }
        Self {
            battle,
            visibility,
            region,
            committed,
            states: BTreeMap::new(),
            projectiles,
            fires,
        }
    }
    pub(crate) fn extra_objects(&self) -> impl Iterator<Item = usize> + '_ {
        self.projectiles
            .keys()
            .copied()
            .chain(self.fires.iter().copied())
    }
    pub(crate) fn in_view(&self, id: usize) -> bool {
        view::contains(self.region, self.battle.simulation.entities[id].position)
    }
    pub(crate) fn module(
        &mut self,
        particles: &mut Particles,
        animations: &mut Animations,
        draw: &ModuleDraw<'_>,
    ) {
        let (id, module) = draw.key;
        let Some((_, state)) = draw
            .players
            .particle_states()
            .find(|&(slot, _)| slot == module)
        else {
            return;
        };
        let game = &self.battle.simulation;
        let unit = &game.entities[id];
        if !drawable_present(game, id) {
            return;
        }
        let draw_type = draw.players.draw_type(unit.type_index);
        particles
            .model_states
            .incarnations
            .insert(id, unit.drawable_birth());
        let binding = particles
            .model_states
            .bindings
            .get(&draw_type)
            .and_then(|draws| draws.get(module));
        let in_view = self.in_view(id);
        let stopped =
            crate::model_animation::draw_visibility::hidden(self.visibility, self.battle, id);
        let needs_bones = particles
            .model_states
            .active
            .get(&draw.key)
            .is_none_or(|active| active.source != (draw_type, state) || active.systems.is_none());
        let offsets = if let Some(binding) = binding
            && (binding.animated || needs_bones)
            && in_view
            && !stopped
        {
            sample_bones(
                animations,
                unit,
                draw.players,
                module,
                &binding.bones[state],
            )
        } else {
            Vec::new()
        };
        let wheels = (module == 0
            && particles
                .model_states
                .wheel_bindings
                .contains_key(&draw_type))
        .then(|| {
            wheel_sample(
                game,
                id,
                self.visibility.map_or_else(
                    || {
                        !matches!(
                            game.visibility_status(self.battle.observer_player.unwrap_or(0), id),
                            Status::Clear | Status::Partial
                        )
                    },
                    |cache| cache.obscured(self.battle, id),
                ),
                in_view && self.committed,
            )
        })
        .flatten();
        let tank = particles
            .model_states
            .tank_bindings
            .contains_key(&(draw_type, module))
            .then(|| game.tread_draw_velocity(id))
            .flatten()
            .filter(|_| draw.players.has_render_object(module))
            .map(|velocity| tanks::Sample {
                pose: drawable_pose(game, id),
                velocity,
                draw: in_view && self.committed && !stopped,
                hidden: stopped,
            });
        if binding.is_none() && wheels.is_none() && tank.is_none() {
            return;
        }
        let state = DrawState {
            unit: draw_type,
            state,
            in_view,
            stopped,
            pose: model_pose(game, id, draw.pose),
            offsets,
            wheels,
            tank,
        };
        let mut random = random(animations, self.battle.generation);
        particles.sync_model_state(draw.key, &state, &mut random);
        if let Some(sample) = &state.tank {
            particles.sync_tank_emitters(draw.key, draw_type, sample, &mut random);
        }
        self.states.insert(draw.key, state);
    }
    pub(crate) fn fire(&self, particles: &mut Particles, animations: &mut Animations, id: usize) {
        let game = &self.battle.simulation;
        let mut random = random(animations, self.battle.generation);
        if let Some(projectile) = self.projectiles.get(&id) {
            let hidden = super::effects::attached_pose_for_player(
                game,
                projectile,
                self.battle.observer_player.unwrap_or(0),
            )
            .is_none_or(|(_, raw)| projectile_hidden(self.visibility, self.battle, id, raw));
            particles.sync_fire_projectile(game, projectile, self.region, hidden, &mut random);
        }
        particles.sync_fire_object(
            game,
            id,
            self.region,
            crate::model_animation::draw_visibility::hidden(self.visibility, self.battle, id),
            &mut random,
        );
    }
    pub(crate) fn finish(self, particles: &mut Particles) {
        particles.prepare_model_states(&self.states);
        particles.update_model_attachments(&self.states);
    }
}

fn projectile_hidden(
    cache: Option<&crate::model_animation::draw_visibility::DrawVisibility>,
    battle: &Match,
    id: usize,
    raw: bool,
) -> bool {
    let actor = &battle.simulation.entities[id];
    actor.drawable_effectively_hidden()
        || actor.masked()
        || cache.map_or(raw, |cache| cache.obscured_at(battle, id, raw))
}

fn wheel_sample(game: &Skirmish, id: usize, obscured: bool, draw: bool) -> Option<wheels::Sample> {
    let motion = game.wheel_draw_motion(id)?;
    let unit = &game.entities[id];
    let (position, basis) = game.object_transform(id);
    let visibility = if obscured {
        wheels::Visibility::Shrouded
    } else if unit.drawable_effectively_hidden() || unit.masked() {
        wheels::Visibility::Hidden
    } else if draw {
        wheels::Visibility::Draw
    } else {
        wheels::Visibility::Retain
    };
    Some(wheels::Sample {
        pose: Pose {
            position,
            axes: std::array::from_fn(|column| basis.map(|row| row[column])),
        },
        visibility,
        motorcycle: game.catalog[unit.type_index].combat_bike.is_some(),
        motion,
    })
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
    #[cfg(test)]
    fn sync_model_states(
        &mut self,
        states: &BTreeMap<(usize, usize), DrawState>,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        self.sync_drawables(states, None, random);
    }

    #[cfg(test)]
    pub(super) fn sync_drawables(
        &mut self,
        states: &BTreeMap<(usize, usize), DrawState>,
        view: Option<(&Skirmish, [[f32; 2]; 2])>,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        if let Some((game, _)) = view {
            let incarnations = states
                .keys()
                .map(|&(id, _)| (id, game.entities[id].drawable_birth()))
                .collect();
            self.retire_replaced_drawables(incarnations);
        }
        self.prepare_model_states(states);
        let mut ordered: std::collections::BTreeSet<_> = states.keys().map(|key| key.0).collect();
        let projectiles = if let Some((game, _)) = view {
            self.prepare_fire_drawables(game, &|id| {
                !matches!(
                    game.visibility_status(0, id),
                    Status::Clear | Status::Partial
                )
            });
            let projectiles = self.fire_projectile_draws(game, 0, &|_, hidden| hidden);
            ordered.extend(projectiles.keys().copied());
            ordered.extend(self.fire_object_draws(game));
            projectiles
        } else {
            BTreeMap::new()
        };
        // Drawable list is prepended. Preserve each object's authored draw
        // module order before visiting the next object.
        let mut ordered: Vec<_> = ordered.into_iter().collect();
        ordered.sort_unstable_by_key(|&id| {
            view.map_or_else(
                || u64::try_from(id).expect("object index"),
                |(game, _)| game.entities[id].drawable_birth(),
            )
        });
        for id in ordered.into_iter().rev() {
            for (&key, draw) in states.range((id, 0)..=(id, usize::MAX)) {
                self.sync_model_state(key, draw, random);
                if let Some(sample) = &draw.tank {
                    self.sync_tank_emitters(key, draw.unit, sample, random);
                }
            }
            if let Some((game, region)) = view {
                if let Some(projectile) = projectiles.get(&id) {
                    self.sync_fire_projectile(
                        game,
                        projectile,
                        region,
                        super::effects::attached_pose(game, projectile)
                            .is_none_or(|(_, hidden)| hidden),
                        random,
                    );
                }
                self.sync_fire_object(
                    game,
                    id,
                    region,
                    !matches!(
                        game.visibility_status(0, id),
                        Status::Clear | Status::Partial
                    ),
                    random,
                );
            }
        }
        self.update_model_attachments(states);
    }

    fn retire_drawable(&mut self, id: usize) {
        self.model_states.incarnations.remove(&id);
        let keys: Vec<_> = self
            .model_states
            .active
            .range((id, 0)..=(id, usize::MAX))
            .map(|(&key, _)| key)
            .collect();
        for key in keys {
            self.stop_model_state(key);
            self.retire_tank_emitters(key);
        }
        if let Some(mut active) = self.model_states.wheels.remove(&id) {
            self.toss_wheel_emitters(&mut active);
            self.model_states.wheel_sounds.push(WheelSound::Retire(id));
        }
        for emitter in &mut self.emitters {
            if emitter
                .drawable_attachment
                .is_some_and(|(object, _)| object == id)
            {
                emitter.drawable_attachment = None;
                emitter.lifecycle.destroyed = true;
                emitter.transform = emitter
                    .drawable_offset
                    .as_ref()
                    .map_or_else(|| Pose::at([0.0; 3]), |offset| offset.pose().clone());
                emitter.wind_origin = Some(emitter.transform.position);
            }
        }
    }

    #[cfg(test)]
    fn retire_replaced_drawables(&mut self, incarnations: BTreeMap<usize, u64>) {
        let replaced: Vec<_> = self
            .model_states
            .incarnations
            .iter()
            .filter_map(|(&id, &birth)| (incarnations.get(&id) != Some(&birth)).then_some(id))
            .collect();
        for id in replaced {
            self.retire_drawable(id);
        }
        self.model_states.incarnations = incarnations;
    }

    fn prepare_model_states(&mut self, states: &BTreeMap<(usize, usize), DrawState>) {
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
        let removed_tanks: Vec<_> = self
            .model_states
            .tanks
            .keys()
            .copied()
            .filter(|key| states.get(key).is_none_or(|draw| draw.tank.is_none()))
            .collect();
        for key in removed_tanks {
            self.retire_tank_emitters(key);
        }
        let removed_wheels: Vec<_> = self
            .model_states
            .wheels
            .keys()
            .copied()
            .filter(|id| {
                states
                    .get(&(*id, 0))
                    .is_none_or(|draw| draw.wheels.is_none())
            })
            .collect();
        for id in removed_wheels {
            if let Some(mut active) = self.model_states.wheels.remove(&id) {
                self.toss_wheel_emitters(&mut active);
                self.model_states.wheel_sounds.push(WheelSound::Retire(id));
            }
        }
    }

    fn sync_model_state(
        &mut self,
        key: (usize, usize),
        draw: &DrawState,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
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
        let binding = self
            .model_states
            .bindings
            .get(&draw.unit)
            .and_then(|draws| draws.get(key.1));
        let Some(binding) = binding.filter(|_| {
            self.model_states.active[&key].systems.is_none() && draw.in_view && !draw.stopped
        }) else {
            if let Some(sample) = &draw.wheels {
                self.sync_wheel_emitters(key.0, draw.unit, sample, random);
            }
            return;
        };
        let templates = binding.states[draw.state].clone();
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
        if let Some(sample) = &draw.wheels {
            self.sync_wheel_emitters(key.0, draw.unit, sample, random);
        }
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
                && self
                    .model_states
                    .bindings
                    .get(&draw.unit)
                    .and_then(|draws| draws.get(key.1))
                    .is_some_and(|binding| binding.animated)
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
    unit.alive()
        || unit.corpse_present()
        || game.retained_rubble(id)
        || unit.has_drawable()
            && matches!(
                game.catalog[unit.type_index].body_kind,
                generals_sim::skirmish::BodyKind::Inactive
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
