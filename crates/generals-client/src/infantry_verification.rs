//! A normally produced rifle infantry unit, paused at its first actual firing frame.
use crate::{
    model_animation::{Animations, Players},
    particles::ParticleInspection,
    playtest::Match,
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type},
    source_material::SourceMaterial,
    weapon_fx::TracerVisual,
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use generals_sim::skirmish::{Faction, Kind};
use std::error::Error;

mod moving;
mod recording;
pub use moving::{prepare_moving, update_moving};
pub use recording::{FiringShots, record};

pub fn prepare(battle: &mut Match, faction: Faction) -> Result<(), Box<dyn Error>> {
    battle.start(faction);
    let game = &mut battle.simulation;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if faction != Faction::Gla {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let name = match faction {
        Faction::Usa => "AmericaInfantryRanger",
        Faction::China => "ChinaInfantryRedguard",
        Faction::Gla => "GLAInfantryRebel",
        Faction::Civilian => return Err("Neutral has no rifle infantry diagnostic".into()),
    };
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or("Rifle infantry diagnostic type missing")?;
    let infantry = produce_type(game, barracks, index, 0)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    game.attack(infantry, target)?;
    advance_until(game, 30 * 180, "rifle infantry firing", |game| {
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == infantry)
    })?;
    println!(
        "Infantry capture: {} produced normally and fired at frame {}",
        game.catalog[game.entities[infantry].type_index].name, game.tick
    );
    battle.select_objects(&[infantry], true);
    Ok(())
}

type TracerMesh = (
    Entity,
    &'static TracerVisual,
    &'static Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
);

#[derive(SystemParam)]
pub struct FiringScene<'w, 's> {
    particles: ParticleInspection<'w, 's>,
    animations: Res<'w, Animations>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Players)>,
    tracers: Query<'w, 's, TracerMesh>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    shots: Res<'w, FiringShots>,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    scene: FiringScene,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    if clock.is_paused() {
        return;
    }
    if time.elapsed_secs() > 45.0 {
        failure_context(battle, &clock);
        error!(
            "Rifle infantry never reached a visible animated firing frame with an authored tracer"
        );
        exit.write(AppExit::error());
        return;
    }
    match scene.check(battle) {
        Ok(Some(tracer)) => {
            // Diagnostic snapshot only: native tracers keep advancing during
            // a paused game. Retain this verified render mesh/material while
            // the existing GPU capture waits for its stable surface frames.
            commands.entity(tracer).remove::<TracerVisual>();
            clock.pause();
            capture.finish_scenario();
        }
        Ok(None) => {}
        Err(error) => {
            error!("Rifle infantry/tracer verification failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

fn failure_context(battle: &Match, clock: &Time<Virtual>) {
    let game = &battle.simulation;
    eprintln!(
        "Rifle timeout: tick {}, started {}, paused {}, virtual paused {}, elapsed {}, delta {}, ended {:?}, outcome {:?}, selected {:?}",
        game.tick,
        battle.started,
        battle.paused,
        clock.is_paused(),
        clock.elapsed_secs(),
        clock.delta_secs(),
        game.ended_at,
        game.outcome,
        battle.selected
    );
    for (id, actor) in game.entities.iter().enumerate().filter(|(_, actor)| {
        actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Infantry
    }) {
        let order = match actor.ai.order {
            generals_sim::skirmish::Order::Attack(target) => format!("Attack({target})"),
            generals_sim::skirmish::Order::Idle => "Idle".into(),
            generals_sim::skirmish::Order::Move(point) => format!("Move({point:?})"),
            _ => "Other".into(),
        };
        let unit = &game.catalog[actor.type_index];
        eprintln!(
            "  actor {id} {} health {}, position {:?}, facing {}, order {order}, weapon {:?}, ranges {}..{}",
            unit.name,
            actor.health,
            actor.position,
            actor.facing,
            actor.weapon_condition(),
            unit.weapon.min_range,
            unit.weapon.range
        );
        for (target, entity) in game.entities.iter().enumerate().filter(|(_, entity)| {
            entity.owner == 1 && game.catalog[entity.type_index].kind == Kind::CommandCenter
        }) {
            eprintln!(
                "  target {target} health {}, position {:?}, center distance {}, visible {}",
                entity.health,
                entity.position,
                Vec2::from_array(actor.position).distance(Vec2::from_array(entity.position)),
                game.visible_to(0, target)
            );
        }
    }
}

impl FiringScene<'_, '_> {
    fn check(self, battle: &Match) -> Result<Option<Entity>, Box<dyn Error>> {
        let animations = self.animations.into_inner();
        let shots = self.shots.into_inner();
        let meshes = self.meshes.into_inner();
        let materials = self.materials.into_inner();
        let Some(&id) = battle.selected.first() else {
            return Ok(None);
        };
        let Some((_, players)) = self.visuals.iter().find(|(visual, _)| visual.index == id) else {
            return Ok(None);
        };
        let type_index = battle.simulation.entities[id].type_index;
        let draws = animations.inspect(players, type_index)?;
        let Some((entity, tracer, mesh, material, shot)) =
            self.tracers
                .iter()
                .find_map(|(entity, tracer, mesh, material)| {
                    (tracer.source == id && tracer.opacity > 0.0)
                        .then(|| shots.find(battle.generation, id, tracer.born))
                        .flatten()
                        .map(|shot| (entity, tracer, mesh, material, shot))
                })
        else {
            return Ok(None);
        };
        let Some(draw) = draws
            .iter()
            .find(|draw| draw.frame > 1.0 && draw.bounds.is_some())
        else {
            return Ok(None);
        };
        let mesh = meshes.get(&mesh.0).ok_or("live tracer mesh missing")?;
        let material = materials
            .get(&material.0)
            .ok_or("live tracer material missing")?;
        check_tracer(mesh, material, tracer)?;
        if !self
            .particles
            .rifle(type_index, shot.veterancy, tracer.born, battle.generation)?
        {
            return Ok(None);
        }
        if Vec3::from_array(tracer.origin).distance(Vec3::from_array(shot.source_position)) <= 0.01
        {
            return Err(
                "rifle FX fell back to object origin instead of animated muzzle bone".into(),
            );
        }
        eprintln!(
            "Original rifle animation/tracer rendered: {}, frame {}, tick {}, muzzle {:?}, prism {}x{}, opacity {}",
            draw.clip,
            draw.frame,
            battle.simulation.tick,
            tracer.origin,
            tracer.length,
            tracer.width,
            tracer.opacity
        );
        Ok(Some(entity))
    }
}

fn check_tracer(
    mesh: &Mesh,
    material: &SourceMaterial,
    tracer: &TracerVisual,
) -> Result<(), Box<dyn Error>> {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Err("tracer positions missing".into());
    };
    if points.len() != 8 || mesh.indices().is_none_or(|indices| indices.len() != 36) {
        return Err("tracer does not contain the original closed Line3D prism".into());
    }
    let distance =
        |a: usize, b: usize| Vec3::from_array(points[a]).distance(Vec3::from_array(points[b]));
    let width = tracer.width * 0.1;
    let length = tracer.length * 0.1;
    if (distance(0, 1) - width).abs() > 0.000_01
        || (distance(0, 2) - width).abs() > 0.000_01
        || (distance(0, 4) - length).abs() > 0.000_01
        || (tracer.width - 0.3).abs() > f32::EPSILON
        || (tracer.length - 10.0).abs() > f32::EPSILON
    {
        return Err("live rifle tracer dimensions differ from installed source FX".into());
    }
    if material.texture.is_some()
        || material.depth_write
        || material.double_sided
        || !matches!(material.alpha_mode, AlphaMode::Blend)
        || material
            .parameters
            .emissive
            .truncate()
            .to_array()
            .map(f32::to_bits)
            != tracer.color.map(f32::to_bits)
        || material.parameters.diffuse.w.to_bits() != tracer.opacity.to_bits()
    {
        return Err("live rifle tracer does not use native prelit alpha-solid material".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn gla_rifle_capture_earns_a_rebel_shot_without_advanced_infantry_prerequisites() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        super::prepare(&mut battle, generals_sim::skirmish::Faction::Gla).unwrap();
        let actor = battle.selected[0];
        let game = &battle.simulation;
        assert_eq!(
            game.catalog[game.entities[actor].type_index].name,
            "GLAInfantryRebel"
        );
        assert!(
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == actor)
        );
        assert!(game.entities.iter().all(|entity| {
            entity.owner != 0 || game.catalog[entity.type_index].name != "GLAPalace"
        }));
        assert!(game.players[0].statistics.units_built > 1);
    }
}
