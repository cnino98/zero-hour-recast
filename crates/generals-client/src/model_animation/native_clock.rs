//! Native rendered evidence for the render-relative animation clock.
use super::{AnimatedPart, Animations, Players};
use crate::{
    playtest::Match,
    playtest_view::{Capture, UnitVisual},
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use generals_sim::skirmish::Faction;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

pub fn prepare(battle: &mut Match) -> Result<(), Box<dyn Error>> {
    crate::infantry_verification::prepare_moving(battle, Faction::Usa)?;
    // The original minimum game-speed option leaves render opportunities between
    // logic updates without freezing a unit or extending any model condition.
    battle.game_speed = 15;
    Ok(())
}

type Geometry = BTreeMap<(Entity, AssetId<Mesh>), Vec<[u32; 3]>>;

struct Pose {
    id: usize,
    tick: u64,
    position: [u32; 2],
    clip: String,
    frame: u32,
    geometry: Geometry,
}

#[derive(Default)]
enum Completion {
    #[default]
    Verifying,
    RestoringView,
    Complete,
}

#[derive(Default)]
pub struct Proof {
    previous: Option<Vec<Pose>>,
    verified: BTreeSet<usize>,
    held_renders: u8,
    completion: Completion,
}

type RenderedPart = (
    Entity,
    &'static AnimatedPart,
    &'static Mesh3d,
    &'static ChildOf,
    &'static Visibility,
    &'static ViewVisibility,
);

#[derive(SystemParam)]
pub struct Scene<'w, 's> {
    animations: Res<'w, Animations>,
    visuals: Query<'w, 's, (Entity, &'static UnitVisual, &'static Players)>,
    parts: Query<'w, 's, RenderedPart>,
    meshes: Res<'w, Assets<Mesh>>,
    time: Res<'w, Time<Real>>,
}

pub fn update(
    scene: Scene,
    mut battle: ResMut<Match>,
    mut proof: Local<Proof>,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    match proof.completion {
        Completion::Complete => return,
        Completion::RestoringView => {
            // The intervening Update has removed the normal pause menu.
            // Capture only after that UI state has reached this rendered frame.
            capture.finish_scenario();
            proof.completion = Completion::Complete;
            return;
        }
        Completion::Verifying => {}
    }
    let result = scene.check(&battle, &mut proof);
    match result {
        Ok(true) => {
            if battle.paused {
                proof.held_renders = proof.held_renders.saturating_add(1);
                if proof.held_renders == 3 {
                    eprintln!(
                        "Fractional animation: both original infantry meshes held across three paused renders; simulation and render identities unchanged"
                    );
                    battle.paused = false;
                    clock.unpause();
                    proof.completion = Completion::RestoringView;
                }
            } else {
                battle.paused = true;
                clock.pause();
            }
        }
        Ok(false) => {}
        Err(error) => {
            error!("Fractional animation verification failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl Scene<'_, '_> {
    fn check(self, battle: &Match, proof: &mut Proof) -> Result<bool, Box<dyn Error>> {
        if self.time.elapsed_secs() > 45.0 {
            return Err("no visible same-tick fractional animation pair within 45 seconds".into());
        }
        if battle.selected.len() != 2 {
            return Err("fractional capture requires both normally produced infantry".into());
        }
        let game = &battle.simulation;
        let mut current = Vec::new();
        for &id in &battle.selected {
            let actor = &game.entities[id];
            if !actor.moving() {
                return Ok(false);
            }
            let Some((entity, _, players)) = self.visuals.iter().find(|(_, visual, _)| {
                visual.index == id && visual.generation == battle.generation
            }) else {
                return Ok(false);
            };
            let Some(draw) = self
                .animations
                .inspect(players, actor.type_index)?
                .into_iter()
                .find(|draw| draw.module == 0)
            else {
                return Ok(false);
            };
            let geometry = self.geometry(entity)?;
            if geometry.is_empty() {
                return Ok(false);
            }
            current.push(Pose {
                id,
                tick: game.tick,
                position: actor.position.map(f32::to_bits),
                clip: draw.clip,
                frame: draw.frame.to_bits(),
                geometry,
            });
        }
        if let Some(previous) = &proof.previous {
            for (before, after) in previous.iter().zip(&current) {
                let same_identity =
                    before.id == after.id && before.geometry.keys().eq(after.geometry.keys());
                let same_tick = before.tick == after.tick && before.position == after.position;
                if battle.paused {
                    if !same_identity
                        || !same_tick
                        || before.frame != after.frame
                        || before.geometry != after.geometry
                    {
                        return Err(
                            "paused render changed original animation geometry or identity".into(),
                        );
                    }
                } else if same_tick
                    && same_identity
                    && before.clip == after.clip
                    && before.frame != after.frame
                    && before.geometry != after.geometry
                    && proof.verified.insert(after.id)
                {
                    eprintln!(
                        "Fractional animation: {} tick {} clip {} frame {} -> {}; actual visible mesh positions changed with stable entities/assets and unchanged simulation position",
                        game.catalog[game.entities[after.id].type_index].name,
                        after.tick,
                        after.clip,
                        f32::from_bits(before.frame),
                        f32::from_bits(after.frame),
                    );
                }
            }
        }
        proof.previous = Some(current);
        Ok(battle.selected.iter().all(|id| proof.verified.contains(id)))
    }

    fn geometry(&self, root: Entity) -> Result<Geometry, Box<dyn Error>> {
        let mut result = BTreeMap::new();
        for (entity, part, handle, parent, visibility, view) in &self.parts {
            if parent.parent() != root || *visibility == Visibility::Hidden || !view.get() {
                continue;
            }
            let name = self
                .animations
                .mesh_name(part.model, part.binding.mesh)
                .ok_or("original mesh binding absent")?;
            if name
                .rsplit('.')
                .next()
                .is_some_and(|name| name.starts_with("MUZZLEFX"))
            {
                return Err("moving infantry displays its muzzle flash".into());
            }
            let mesh = self
                .meshes
                .get(handle)
                .ok_or("rendered original mesh absent")?;
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("rendered original positions absent".into());
            };
            if points.is_empty() || points.iter().flatten().any(|value| !value.is_finite()) {
                return Err("rendered original positions empty or nonfinite".into());
            }
            result.insert(
                (entity, handle.id()),
                points.iter().map(|point| point.map(f32::to_bits)).collect(),
            );
        }
        Ok(result)
    }
}
