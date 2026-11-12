//! Invisible fire drawables and native dynamic firestorm particle ownership.
use super::{Particles, Pose, Template};
use crate::skirmish_data::{blocks, definition};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Skirmish, UnitType, visibility::Status};
use std::{collections::BTreeMap, error::Error};
mod instances;
mod projectiles;
#[cfg(test)]
mod tests;

pub(super) enum Attachment {
    Object(usize),
    Projectile(u64),
}

pub(super) struct Pending {
    bindings: BTreeMap<usize, (Vec<String>, Vec<String>)>,
    projectiles: projectiles::Bindings,
    module_names: Vec<String>,
}

pub(super) struct Bindings {
    draw: Vec<usize>,
}

struct Active {
    draw: Option<Vec<u64>>,
}

pub(super) struct Fires {
    bindings: BTreeMap<usize, Bindings>,
    active: BTreeMap<usize, Active>,
    frame: Option<u64>,
    projectile_bindings: BTreeMap<(usize, usize, bool), Vec<usize>>,
    projectiles: BTreeMap<u64, bool>,
    instances: instances::Instances,
    module_templates: BTreeMap<String, usize>,
}

impl Fires {
    pub(super) const fn empty() -> Self {
        Self {
            bindings: BTreeMap::new(),
            active: BTreeMap::new(),
            frame: None,
            projectile_bindings: BTreeMap::new(),
            projectiles: BTreeMap::new(),
            instances: instances::Instances::empty(),
            module_templates: BTreeMap::new(),
        }
    }

    pub(super) fn reset(&mut self) {
        self.active.clear();
        self.frame = None;
        self.projectiles.clear();
        self.instances.reset();
    }
}

impl Pending {
    pub(super) fn load(assets: &VirtualAssets, types: &[UnitType]) -> Result<Self, Box<dyn Error>> {
        let files = [content::scan(assets.read("data/ini/object/system.ini")?)?];
        let mut bindings = BTreeMap::new();
        for (index, unit) in types
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.fire.is_some())
        {
            let object = definition(&files, "Object", &unit.name)?;
            let mut names = Vec::new();
            for draw in blocks(&object.lines, 0, "Draw")? {
                let parsed = crate::model_animation::state_particles::particle_draw(draw)?;
                if parsed.states.len() != 1
                    || parsed.bones[0]
                        .iter()
                        .any(|bone| !bone.eq_ignore_ascii_case("none"))
                {
                    return Err(format!(
                        "{}: fire drawable requires its original single none-bone state",
                        unit.name
                    )
                    .into());
                }
                names.extend(parsed.states.into_iter().flatten());
            }
            let storm = unit
                .fire
                .as_ref()
                .and_then(|rules| rules.storm.as_ref())
                .map_or_else(Vec::new, |storm| storm.particles.clone());
            bindings.insert(index, (names, storm));
        }
        Ok(Self {
            bindings,
            projectiles: projectiles::load(assets, types)?,
            module_names: types
                .iter()
                .flat_map(|unit| &unit.behavior.firestorms)
                .flat_map(|rules| rules.particles.iter().flatten().cloned())
                .collect(),
        })
    }

    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.bindings
            .values()
            .flat_map(|(draw, storm)| draw.iter().chain(storm).map(String::as_str))
            .chain(self.projectiles.values().flatten().map(String::as_str))
            .chain(self.module_names.iter().map(String::as_str))
    }

    pub(super) fn resolve(self, templates: &[Template]) -> Result<Fires, Box<dyn Error>> {
        let mut result = Fires::empty();
        let resolve = |names: Vec<String>| -> Result<Vec<usize>, Box<dyn Error>> {
            names
                .into_iter()
                .map(|name| {
                    templates
                        .iter()
                        .position(|template| template.name == name)
                        .ok_or_else(|| {
                            format!("missing original fire ParticleSystem {name}").into()
                        })
                })
                .collect()
        };
        for name in self.module_names {
            let template = templates
                .iter()
                .position(|template| template.name == name)
                .ok_or_else(|| format!("missing original firestorm ParticleSystem {name}"))?;
            result.module_templates.insert(name, template);
        }
        for (kind, (draw, storm)) in self.bindings {
            resolve(storm)?;
            result.bindings.insert(
                kind,
                Bindings {
                    draw: resolve(draw)?,
                },
            );
        }
        for (weapon, names) in self.projectiles {
            result.projectile_bindings.insert(weapon, resolve(names)?);
        }
        Ok(result)
    }
}

impl Particles {
    pub(super) fn fire_drawables(
        &mut self,
        game: &Skirmish,
        region: [[f32; 2]; 2],
        paused: bool,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        if self.fires.bindings.is_empty() && self.fires.projectile_bindings.is_empty() {
            return;
        }
        self.retire_fire_drawables(game);
        self.fire_projectile_draws(game, region, random);
        // Native Drawable list prepends each newly created object.
        for (id, actor) in game.entities.iter().enumerate().rev() {
            let Some(bindings) = self.fires.bindings.get(&actor.type_index) else {
                continue;
            };
            if bindings.draw.is_empty() || actor.masked() {
                continue;
            }
            let stopped = !matches!(
                game.visibility_status(0, id),
                Status::Clear | Status::Partial
            );
            if self
                .fires
                .active
                .get(&id)
                .is_none_or(|state| state.draw.is_none())
                && !stopped
                && super::model_states::view::contains(region, actor.position)
            {
                let templates = bindings.draw.clone();
                let mut systems = Vec::new();
                for template in templates {
                    let system = self.create(template, object_pose(game, id), None, random);
                    self.emitters
                        .last_mut()
                        .expect("fire drawable system")
                        .fire_attachment = Some(Attachment::Object(id));
                    systems.push(system);
                }
                self.fires.active.entry(id).or_insert_with(active).draw = Some(systems);
            }
        }
        for emitter in &mut self.emitters {
            if let Some(Attachment::Object(id)) = emitter.fire_attachment
                && !emitter.lifecycle.destroyed
            {
                emitter.transform = object_pose(game, id);
                emitter.shrouded = !matches!(
                    game.visibility_status(0, id),
                    Status::Clear | Status::Partial
                );
            }
        }
        if !paused && self.fires.frame != Some(game.tick) {
            self.fires.frame = Some(game.tick);
            for index in 0..self.emitters.len() {
                if self.emitters[index].fire_attachment.is_some() {
                    self.update_emitter(index, game.tick, random, &|point| {
                        game.ground_height(point)
                    });
                }
            }
            self.retire();
        }
    }

    fn retire_fire_drawables(&mut self, game: &Skirmish) {
        let removed: Vec<_> = self
            .fires
            .active
            .keys()
            .copied()
            .filter(|&id| {
                game.entities
                    .get(id)
                    .is_none_or(generals_sim::skirmish::Entity::masked)
            })
            .collect();
        for id in removed {
            let state = self.fires.active.remove(&id).expect("active fire object");
            for emitter in &mut self.emitters {
                if state
                    .draw
                    .as_ref()
                    .is_some_and(|systems| systems.contains(&emitter.id))
                {
                    emitter.lifecycle.destroyed = true;
                    emitter.wind_origin = Some([0.0; 3]);
                }
            }
            // FirestormDynamicGeometry's destructor does not destroy its free systems.
        }
    }
}

const fn active() -> Active {
    Active { draw: None }
}

fn object_pose(game: &Skirmish, id: usize) -> Pose {
    let (sine, cosine) = game.entities[id].facing.sin_cos();
    Pose {
        position: game.object_position(id),
        axes: [[cosine, sine, 0.0], [-sine, cosine, 0.0], [0.0, 0.0, 1.0]],
    }
}

type CameraView = (&'static Transform, &'static Projection, &'static Camera);

pub fn update(
    battle: Res<crate::playtest::Match>,
    mut particles: ResMut<Particles>,
    mut animations: ResMut<crate::model_animation::Animations>,
    camera: Single<CameraView, With<crate::playtest_view::WorldCamera>>,
    terrain: Res<crate::terrain::Terrain>,
) {
    let battle = battle.into_inner();
    if !battle.started
        || (particles.fires.bindings.is_empty() && particles.fires.projectile_bindings.is_empty())
    {
        return;
    }
    let (transform, projection, camera) = camera.into_inner();
    let region = super::model_states::view::region(
        transform,
        projection,
        camera,
        terrain.into_inner().draw_region(),
    );
    particles.begin(battle.generation, battle.simulation.tick);
    particles.fire_drawables(
        &battle.simulation,
        region,
        battle.paused,
        &mut super::random(&mut animations, battle.generation),
    );
}
