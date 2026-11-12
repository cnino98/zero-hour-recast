//! Live, normally produced rifle and launcher infantry during ordinary movement.
use crate::{
    model_animation::{AnimatedPart, Animations, Players},
    playtest::Match,
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type},
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::{collections::BTreeMap, error::Error};

type VisibleGeometry = BTreeMap<(usize, usize), Vec<[u32; 3]>>;

pub fn prepare_moving(battle: &mut Match, faction: Faction) -> Result<(), Box<dyn Error>> {
    battle.start(faction);
    let game = &mut battle.simulation;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if faction != Faction::Gla {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let names = match faction {
        Faction::Usa => ["AmericaInfantryRanger", "AmericaInfantryMissileDefender"],
        Faction::China => ["ChinaInfantryRedguard", "ChinaInfantryTankHunter"],
        Faction::Gla => ["GLAInfantryRebel", "GLAInfantryTunnelDefender"],
        Faction::Civilian => return Err("Neutral has no infantry diagnostic".into()),
    };
    let mut selected = Vec::new();
    for name in names {
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .ok_or("Moving infantry diagnostic type missing")?;
        selected.push(produce_type(game, barracks, index, 0)?);
        if selected.len() == 1 {
            separate_rifle(game, selected[0])?;
        }
    }
    for &id in &selected {
        let [x, y] = game.entities[id].position;
        let accepted = [[250.0, 0.0], [-250.0, 0.0], [0.0, 250.0], [0.0, -250.0]]
            .into_iter()
            .any(|[dx, dy]| game.move_to(id, [x + dx, y + dy]).is_ok());
        if !accepted {
            return Err(
                "Produced infantry has no reachable diagnostic movement destination".into(),
            );
        }
    }
    battle.select_objects(&selected, true);
    Ok(())
}

fn separate_rifle(game: &mut Skirmish, id: usize) -> Result<(), Box<dyn Error>> {
    let origin = game.entities[id].position;
    for [dx, dy] in [[0.0, -70.0], [-70.0, 0.0], [70.0, 0.0], [0.0, 70.0]] {
        if game.move_to(id, [origin[0] + dx, origin[1] + dy]).is_err() {
            continue;
        }
        advance_until(game, 30 * 45, "rifle clearing the barracks exit", |game| {
            matches!(game.entities[id].ai.order, Order::Idle)
        })?;
        if Vec2::from_array(game.entities[id].position).distance(Vec2::from_array(origin)) >= 40.0 {
            return Ok(());
        }
    }
    Err("Rifle could not move clear of the producer for an unobscured capture".into())
}

#[derive(SystemParam)]
pub struct MovingScene<'w, 's> {
    animations: Res<'w, Animations>,
    visuals: Query<'w, 's, (Entity, &'static UnitVisual, &'static Players)>,
    parts: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static Mesh3d,
            &'static ChildOf,
            &'static Visibility,
            &'static InheritedVisibility,
        ),
    >,
    meshes: Res<'w, Assets<Mesh>>,
}

pub struct MovingPose {
    id: usize,
    tick: u64,
    position: [f32; 2],
    clip: String,
    frame: f32,
    geometry: VisibleGeometry,
}

pub fn update_moving(
    scene: MovingScene,
    battle: Res<Match>,
    mut previous: Local<Option<Vec<MovingPose>>>,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    if clock.is_paused() {
        return;
    }
    let result = if time.into_inner().elapsed_secs() > 45.0 {
        Err("Moving rifle and launcher poses were not verified within 45 seconds".into())
    } else {
        scene.check(battle.into_inner(), &mut previous)
    };
    match result {
        Ok(true) => {
            clock.pause();
            capture.finish_scenario();
        }
        Ok(false) => {}
        Err(error) => {
            error!("Moving infantry verification failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl MovingScene<'_, '_> {
    fn check(
        self,
        battle: &Match,
        previous: &mut Option<Vec<MovingPose>>,
    ) -> Result<bool, Box<dyn Error>> {
        let game = &battle.simulation;
        if battle.selected.len() != 2 {
            return Err(
                "Moving infantry diagnostic requires its produced rifle and launcher".into(),
            );
        }
        let mut current = Vec::new();
        for &id in &battle.selected {
            let actor = &game.entities[id];
            if !actor.moving() {
                return Ok(false);
            }
            if game.presentation.shots.iter().any(|shot| shot.source == id) {
                return Err("Moving infantry unexpectedly fired during this scenario".into());
            }
            let Some((entity, _, players)) = self
                .visuals
                .iter()
                .find(|(_, unit, _)| unit.index == id && unit.generation == battle.generation)
            else {
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
            let Some(geometry) = self.geometry(entity)? else {
                return Ok(false);
            };
            current.push(MovingPose {
                id,
                tick: game.tick,
                position: actor.position,
                clip: draw.clip,
                frame: draw.frame,
                geometry,
            });
        }
        let Some(first) = previous else {
            *previous = Some(current);
            return Ok(false);
        };
        if first
            .iter()
            .zip(&current)
            .any(|(before, after)| before.id != after.id || before.clip != after.clip)
        {
            // Native transition clips can finish before the first comparable
            // pair. Compare two poses from the newly selected actual clip.
            *previous = Some(current);
            return Ok(false);
        }
        let changed = first.iter().zip(&current).all(|(before, after)| {
            before.id == after.id
                && before.tick + 2 < after.tick
                && before.position.map(f32::to_bits) != after.position.map(f32::to_bits)
                && before.clip == after.clip
                && before.frame.to_bits() != after.frame.to_bits()
                && before.geometry != after.geometry
        });
        if changed {
            for (before, after) in first.iter().zip(current) {
                eprintln!(
                    "Live moving weapon geometry: {} clip {} frame {} -> {}; actual body vertices changed, original muzzle meshes hidden",
                    game.catalog[game.entities[after.id].type_index].name,
                    after.clip,
                    before.frame,
                    after.frame
                );
            }
        }
        Ok(changed)
    }

    fn geometry(&self, entity: Entity) -> Result<Option<VisibleGeometry>, Box<dyn Error>> {
        let mut geometry = BTreeMap::new();
        let mut flashes = 0;
        for (part, handle, parent, visible, inherited) in &self.parts {
            if parent.parent() != entity {
                continue;
            }
            let name = self
                .animations
                .mesh_name(part.model, part.binding.mesh)
                .ok_or("Moving render binding has no source mesh")?;
            if name
                .split_once('.')
                .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("MUZZLEFX01"))
            {
                if *visible != Visibility::Hidden || inherited.get() {
                    return Err(
                        format!("Moving unit incorrectly displays original flash {name}").into(),
                    );
                }
                flashes += 1;
            } else if inherited.get() {
                let mesh = self
                    .meshes
                    .get(handle)
                    .ok_or("Rendered infantry mesh missing")?;
                let Some(VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    return Err("Rendered infantry positions missing".into());
                };
                if points.is_empty() || points.iter().flatten().any(|value| !value.is_finite()) {
                    return Err("Rendered infantry geometry is empty or nonfinite".into());
                }
                geometry.insert(
                    (part.model, part.binding.mesh),
                    points.iter().map(|point| point.map(f32::to_bits)).collect(),
                );
            }
        }
        Ok((flashes > 0 && !geometry.is_empty()).then_some(geometry))
    }
}
