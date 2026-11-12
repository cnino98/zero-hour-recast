//! Normal production, attacks and live original projectile mesh verification.
use crate::{
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::Match,
    playtest_view::{Capture, WorldCamera},
    projectile_render::{ProjectileVisual, Projectiles},
    skirmish_verification::{advance_until, construct, friendly_entity, produce, produce_type},
};
use bevy::{
    ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*, window::PrimaryWindow,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

#[derive(Resource)]
pub struct ProjectileCapture {
    pub shooter: usize,
    pub target: usize,
    first: Option<(u64, [f32; 3])>,
    passed: bool,
    exhaust: bool,
}

pub fn prepare(game: &mut Skirmish, kind: Kind) -> Result<ProjectileCapture, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if game.players[0].faction != Faction::Gla {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    let producer = if kind == Kind::Tank {
        construct(game, builder, Kind::SupplyCenter, 0)?;
        construct(game, builder, Kind::Factory, 0)?
    } else {
        construct(game, builder, Kind::Barracks, 0)?
    };
    let shooter = if kind == Kind::Tank {
        produce(game, producer, kind, 0)?
    } else {
        let name = match game.players[0].faction {
            Faction::Usa => "AmericaInfantryMissileDefender",
            Faction::China => "ChinaInfantryTankHunter",
            Faction::Gla => "GLAInfantryTunnelDefender",
            Faction::Civilian => return Err("Neutral has no launcher diagnostic".into()),
        };
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .ok_or("faction launcher infantry missing")?;
        produce_type(game, producer, index, 0)?
    };
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    game.attack(shooter, target)?;
    advance_until(
        game,
        30 * 180,
        "produced unit approaching and firing at the enemy base",
        |game| {
            game.projectiles.iter().any(|projectile| {
                projectile.source == shooter
                    && projectile.render_transform(&game.entities).is_some()
            })
        },
    )?;
    Ok(ProjectileCapture {
        shooter,
        target,
        first: None,
        passed: false,
        exhaust: kind != Kind::Tank,
    })
}

pub fn verify_missile(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let scenario = prepare(game, Kind::Infantry)?;
    let projectile = game
        .projectiles
        .iter()
        .find(|projectile| projectile.source == scenario.shooter)
        .ok_or("launcher fired without creating a missile")?;
    let id = projectile.id;
    let (start, _) = projectile
        .render_transform(&game.entities)
        .ok_or("new missile is hidden")?;
    let health = game.entities[scenario.target].health;
    let mut moved = false;
    let mut hidden = false;
    for _ in 0..300 {
        game.step();
        let projectile = game
            .projectiles
            .iter()
            .find(|projectile| projectile.id == id);
        if let Some(projectile) = projectile {
            if let Some((position, axes)) = projectile.render_transform(&game.entities) {
                if position
                    .iter()
                    .chain(axes.iter().flatten())
                    .any(|value| !value.is_finite())
                {
                    return Err("missile flight produced a nonfinite pose".into());
                }
                moved |= Vec3::from_array(position).distance_squared(Vec3::from_array(start)) > 1.0;
            } else {
                hidden = true;
            }
        } else {
            if !moved || !hidden || game.entities[scenario.target].health >= health {
                return Err(format!("missile {id} retired without verified motion, hidden detonation and target damage: moved={moved}, hidden={hidden}, health={health}->{}",
                    game.entities[scenario.target].health).into());
            }
            println!(
                "{}: original missile moved, damaged the enemy building, hid and retired normally at tick {}",
                game.catalog[game.entities[scenario.shooter].type_index].name, game.tick
            );
            return Ok(());
        }
    }
    Err(format!("missile {id} never completed its source detonation lifecycle").into())
}

pub fn prepare_flashbang(game: &mut Skirmish) -> Result<ProjectileCapture, Box<dyn Error>> {
    let (shooter, target) = crate::weapon_verification::prepare_flashbang(game)?;
    Ok(ProjectileCapture {
        shooter,
        target,
        first: None,
        passed: false,
        exhaust: false,
    })
}

#[derive(SystemParam)]
pub struct ProjectileScene<'w, 's> {
    particles: ParticleInspection<'w, 's>,
    models: Res<'w, Projectiles>,
    meshes: Res<'w, Assets<Mesh>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    ui: Res<'w, crate::control_bar::UiData>,
    promotions: Res<'w, crate::playtest_ui::PromotionTransition>,
    projectiles: Query<'w, 's, (Entity, &'static ProjectileVisual, &'static Transform)>,
    parts: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static Mesh3d,
            &'static ChildOf,
            &'static Visibility,
        ),
    >,
}

pub fn update(
    mut scenario: ResMut<ProjectileCapture>,
    battle: Res<Match>,
    scene: ProjectileScene,
    mut capture: ResMut<Capture>,
    time: Res<Time<Real>>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    if scenario.passed {
        return;
    }
    let result = if time.elapsed_secs() > 60.0 {
        Err(
            "Original projectile did not reach two visible flight positions within 60 seconds"
                .into(),
        )
    } else {
        scene.check(battle, &mut scenario)
    };
    match result {
        Ok(true) => {
            scenario.passed = true;
            clock.pause();
            capture.finish_scenario();
        }
        Ok(false) => {}
        Err(error) => {
            error!("Projectile render verification failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl ProjectileScene<'_, '_> {
    fn check(
        self,
        battle: &Match,
        scenario: &mut ProjectileCapture,
    ) -> Result<bool, Box<dyn Error>> {
        if !battle.simulation.entities[scenario.shooter].alive()
            || !battle.simulation.entities[scenario.target].alive()
        {
            return Err("Shooter or target died before visible projectile verification".into());
        }
        let stats = battle.simulation.entities[scenario.shooter].type_index;
        for projectile in &battle.simulation.projectiles {
            if projectile.source != scenario.shooter {
                continue;
            }
            let Some((position, axes)) = projectile.render_transform(&battle.simulation.entities)
            else {
                continue;
            };
            let model = self.models.models[stats][projectile.weapon_slot]
                .ok_or("Fired weapon slot has no original projectile model")?;
            let Some((entity, _, transform)) = self.projectiles.iter().find(|(_, visual, _)| {
                visual.id == projectile.id && visual.generation == battle.generation
            }) else {
                continue;
            };
            let expected = Vec3::new(position[0], position[2], -position[1]) * 0.1;
            let convert = |axis: [f32; 3]| Vec3::new(axis[0], axis[2], -axis[1]);
            let expected_axes = [convert(axes[0]), convert(axes[2]), -convert(axes[1])];
            if transform.translation.distance_squared(expected) > 0.000_001
                || [Vec3::X, Vec3::Y, Vec3::Z]
                    .into_iter()
                    .zip(expected_axes)
                    .any(|(local, expected)| {
                        (transform.rotation * local).distance_squared(expected) > 0.000_001
                    })
            {
                return Err(
                    "Rendered projectile transform disagrees with native flight state".into(),
                );
            }
            let Ok(screen) = self.camera.0.world_to_viewport(self.camera.1, expected) else {
                continue;
            };
            if !self
                .camera
                .0
                .logical_viewport_rect()
                .is_some_and(|area| area.contains(screen))
                || !self.has_mesh(entity, model)
                || crate::playtest_ui::captures_pointer(
                    battle,
                    &self.window,
                    screen,
                    &self.ui,
                    &self.promotions,
                )
            {
                continue;
            }
            if let Some((id, original)) = &scenario.first
                && *id == projectile.id
                && Vec3::from_array(*original).distance_squared(Vec3::from_array(position)) > 1.0
                && (!scenario.exhaust
                    || self.particles.missile(projectile.id, battle.generation)?)
            {
                eprintln!(
                    "Original projectile rendered: projectile {}, source {}, model {model}, native flight {original:?} -> {position:?}",
                    projectile.id, scenario.shooter
                );
                return Ok(true);
            }
            if scenario
                .first
                .as_ref()
                .is_none_or(|(id, _)| *id != projectile.id)
            {
                scenario.first = Some((projectile.id, position));
            }
        }
        Ok(false)
    }
    fn has_mesh(&self, entity: Entity, model: usize) -> bool {
        self.parts.iter().any(|(part, mesh, parent, visibility)| {
            parent.parent() == entity
                && part.model == model
                && !matches!(visibility, Visibility::Hidden)
                && self.meshes.get(&mesh.0).is_some_and(|mesh| {
                    matches!(mesh.attribute(Mesh::ATTRIBUTE_POSITION), Some(VertexAttributeValues::Float32x3(points)) if points.len() >= 3 && points.iter().flatten().all(|value| value.is_finite()))
                })
        })
    }
}
