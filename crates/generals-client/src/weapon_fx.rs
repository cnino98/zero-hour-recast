//! Authored rifle Tracer FX: `FXList`, `W3DTracerDraw` and `Line3DClass`.
//! EA GPL-3.0-or-later; pinned `SuperHackers` 2bfaea60. Particle/light nuggets
//! and their client RNG consumption are handled by the shared particle module.
mod data;
mod line;
mod rope;
mod tracer;
use crate::{
    model_animation::{Animations, Players},
    playtest::Match,
    playtest_view::UnitVisual,
    source_material::SourceMaterial,
};
use bevy::{camera::visibility::RenderLayers, prelude::*};
pub use data::load;
pub use rope::update as update_ropes;
pub use tracer::TracerVisual;

struct TracerRules {
    speed: f32,
    decay_at: f32,
    length: f32,
    width: f32,
    color: [f32; 3],
    probability: f32,
}

struct Rifle {
    normal: TracerRules,
    heroic: TracerRules,
    weapon_speed: f32,
}

#[derive(Resource)]
pub struct WeaponFx {
    ropes: rope::Ropes,
    rifles: Vec<Vec<Option<Rifle>>>,
    pending: Vec<TracerVisual>,
    recorded: Option<(u32, u64, usize)>,
}

fn visual_players<'a>(
    visuals: &'a Query<(&UnitVisual, &Players, &Transform)>,
    generation: u32,
    index: usize,
) -> Option<&'a Players> {
    visuals
        .iter()
        .find(|(visual, _, _)| visual.generation == generation && visual.index == index)
        .map(|(_, players, _)| players)
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct RecordedEffects<'w> {
    animations: ResMut<'w, Animations>,
    effects: ResMut<'w, WeaponFx>,
    lasers: ResMut<'w, crate::laser_beam::Lasers>,
    particles: ResMut<'w, crate::particles::Particles>,
    structures: ResMut<'w, crate::structure_view::Structures>,
    audio: ResMut<'w, crate::source_audio::SourceAudio>,
    selection: ResMut<'w, crate::selection_feedback::Feedback>,
    time: Res<'w, Time>,
    trees: ResMut<'w, crate::tree_buffer::Trees>,
    world: Res<'w, crate::playtest_view::WorldObjects>,
    emitters: Option<ResMut<'w, crate::w3d_emitter::Emitters>>,
    clock: Option<Res<'w, crate::model_animation::Clock>>,
}

/// Capture every logic step; render frames may contain multiple logic steps.
/// The current drawable pose precedes the next animation update, as in `Weapon.cpp`.
pub fn record(
    battle: Res<Match>,
    mut recorded: RecordedEffects,
    visuals: Query<(&UnitVisual, &Players, &Transform)>,
    timings: Option<Res<crate::render_performance::Timings>>,
) {
    let timings = timings.map(Res::into_inner);
    let _span = timings.map(|timings| timings.span("weapon_fx_record"));
    let battle = battle.into_inner();
    let events = &battle.simulation.presentation.weapon_fx;
    let start =
        recorded
            .effects
            .begin_recording(battle.generation, battle.simulation.tick, events.len());
    let viewer = battle.observer_player.unwrap_or(0);
    for event in &events[start..] {
        recorded.particles.flush_effect_sounds(&mut recorded.audio);
        recorded
            .audio
            .weapon_event(&battle.simulation, event, viewer);
        recorded.record_event(battle, &visuals, event);
        recorded.particles.flush_effect_sounds(&mut recorded.audio);
    }
}

impl RecordedEffects<'_> {
    fn record_event(
        &mut self,
        battle: &Match,
        visuals: &Query<(&UnitVisual, &Players, &Transform)>,
        event: &generals_sim::skirmish::WeaponFxEvent,
    ) {
        use generals_sim::skirmish::WeaponFxEvent;
        match event {
            WeaponFxEvent::DrawableCreated { .. }
            | WeaponFxEvent::Chinook(_)
            | WeaponFxEvent::DrawableDestroyed(_)
            | WeaponFxEvent::LaserInit(_)
            | WeaponFxEvent::Laser(_)
            | WeaponFxEvent::Shot(_)
            | WeaponFxEvent::Projectile(_) => self.record_drawable_event(battle, visuals, event),
            WeaponFxEvent::BodyParticles(index) => self.particles.body_particle_event(
                &battle.simulation,
                battle.generation,
                *index,
                &mut self.animations,
            ),
            WeaponFxEvent::BodyParticlesClear(object) => {
                self.particles.clear_body_particles(*object);
            }
            WeaponFxEvent::BoneFx(index) => {
                self.particles
                    .bone_event(battle, *index, &mut self.animations);
            }
            WeaponFxEvent::TerrainMovement(index) => {
                self.trees.record_event(
                    battle,
                    &self.world,
                    *index,
                    &mut self.animations,
                    &mut self.particles,
                );
            }
            WeaponFxEvent::AbilityFeedback(index) => {
                let event = &battle.simulation.presentation.ability_feedback[*index];
                self.particles
                    .ability_feedback(battle, event, &mut self.animations);
                if matches!(
                    event.effect,
                    generals_sim::skirmish::special_ability::FeedbackEffect::CaptureFlash
                ) {
                    self.selection
                        .ability_capture_flash(battle, event.target, event.owner);
                }
            }

            WeaponFxEvent::FiringSound { .. }
            | WeaponFxEvent::StopFiringSound(_)
            | WeaponFxEvent::RapidFire { .. }
            | WeaponFxEvent::Pilot(_)
            | WeaponFxEvent::ObjectSound(_) => {} // Source audio owns these sound-only callbacks.
            WeaponFxEvent::AttachedParticle(index) => {
                self.particles
                    .attach_particle(battle, *index, &mut self.animations);
            }
            WeaponFxEvent::SpectreStrafe(index) => {
                self.particles
                    .spectre_strafe(battle, *index, &mut self.animations);
            }

            WeaponFxEvent::Structure(index) => {
                self.structures.record(
                    battle,
                    &battle.simulation.presentation.structure_events[*index],
                    &mut self.animations,
                    &mut self.particles,
                    &mut self.audio,
                );
            }
            WeaponFxEvent::DestroyAttachedParticles(object) => {
                self.particles.destroy_height_die_attachments(*object);
            }
            WeaponFxEvent::SlaveWeld(index) => {
                self.particles.slave_weld_event(
                    &battle.simulation,
                    battle.generation,
                    *index,
                    &mut self.animations,
                );
            }
            WeaponFxEvent::Firestorm(index) => {
                self.particles.firestorm_module_event(
                    &battle.simulation,
                    battle.generation,
                    *index,
                    &mut crate::particles::random(&mut self.animations, battle.generation),
                );
            }
            WeaponFxEvent::ObjectFx(index) => {
                let object = battle.simulation.presentation.object_fx_events[*index].object;
                self.particles.actor_event(
                    battle,
                    *index,
                    visual_players(visuals, battle.generation, object),
                    &mut self.audio,
                    &mut self.animations,
                );
            }
        }
    }
    fn record_drawable_event(
        &mut self,
        battle: &Match,
        visuals: &Query<(&UnitVisual, &Players, &Transform)>,
        event: &generals_sim::skirmish::WeaponFxEvent,
    ) {
        use generals_sim::skirmish::WeaponFxEvent;
        match event {
            WeaponFxEvent::DrawableCreated {
                object,
                type_index,
                validate_logic,
            } => {
                if let Some(emitters) = &mut self.emitters {
                    let now = self
                        .clock
                        .as_ref()
                        .expect("emitter scene requires WW3D clock")
                        .committed_milliseconds();
                    emitters.created(
                        &battle.simulation,
                        battle.generation,
                        *object,
                        *type_index,
                        *validate_logic,
                        now,
                    );
                }
            }
            WeaponFxEvent::Chinook(event) => {
                self.effects
                    .ropes
                    .record(battle, event, &mut self.animations, &mut self.particles);
            }
            WeaponFxEvent::DrawableDestroyed(object) => {
                if let Some(emitters) = &mut self.emitters {
                    emitters.destroyed(*object);
                }
                self.lasers.retire_native(*object, &mut self.particles);
            }
            WeaponFxEvent::LaserInit(index) => {
                self.lasers.initialize_native(
                    battle,
                    *index,
                    &mut self.animations,
                    &mut self.particles,
                    visuals,
                );
            }
            WeaponFxEvent::Laser(index) => {
                self.lasers.record_pulse(
                    battle,
                    *index,
                    &mut self.animations,
                    &mut self.particles,
                    visuals,
                    self.time.elapsed(),
                );
            }
            WeaponFxEvent::Shot(index) => record_shot(
                battle,
                &battle.simulation.presentation.shots[*index],
                visuals,
                &mut self.animations,
                &mut self.particles,
                &mut self.effects,
            ),
            WeaponFxEvent::Projectile(event) => {
                self.particles.projectile_event(
                    &battle.simulation,
                    battle.generation,
                    event,
                    event
                        .object
                        .and_then(|id| visual_players(visuals, battle.generation, id)),
                    &mut self.animations,
                );
            }
            _ => unreachable!("record_event dispatches only drawable weapon callbacks"),
        }
    }
}

fn record_shot(
    battle: &Match,
    shot: &generals_sim::skirmish::Shot,
    visuals: &Query<(&UnitVisual, &Players, &Transform)>,
    animations: &mut Animations,
    particles: &mut crate::particles::Particles,
    effects: &mut WeaponFx,
) {
    let position = shot_position(
        visuals,
        battle.generation,
        &battle.simulation,
        shot,
        animations,
    );
    if !matches!(
        battle
            .simulation
            .visibility_cell(0, [position[0], position[1]]),
        generals_sim::skirmish::visibility::Status::Clear
    ) {
        return;
    }
    particles.fire_weapon(
        &battle.simulation,
        shot,
        position,
        animations,
        (battle.generation, battle.simulation.tick),
    );
    effects.record_tracer(
        &battle.simulation,
        shot,
        position,
        animations,
        battle.generation,
    );
}

impl WeaponFx {
    fn begin_recording(&mut self, generation: u32, tick: u64, count: usize) -> usize {
        let start = match self.recorded {
            Some((previous, frame, cursor)) if previous == generation && frame == tick => cursor,
            Some((previous, _, _)) if previous != generation => {
                self.pending.clear();
                0
            }
            _ => 0,
        };
        self.recorded = Some((generation, tick, count));
        start
    }

    fn record_tracer(
        &mut self,
        game: &generals_sim::skirmish::Skirmish,
        shot: &generals_sim::skirmish::Shot,
        position: [f32; 3],
        animations: &mut Animations,
        generation: u32,
    ) {
        let Some(rifle) = self
            .rifles
            .get(shot.weapon_type)
            .and_then(|slots| slots[shot.weapon_index].as_ref())
        else {
            return;
        };
        let rules = if shot.veterancy == 3 {
            &rifle.heroic
        } else {
            &rifle.normal
        };
        // The original draws even for Probability=1, before testing positions.
        if rules.probability <= animations.effect_random(generation) {
            return;
        }
        match TracerVisual::new(
            rules,
            rifle.weapon_speed,
            position,
            shot.target_position,
            shot.source,
            game.tick,
            generation,
        ) {
            Ok(tracer) => self.pending.push(tracer),
            Err(error) => error!("Original rifle tracer: {error}"),
        }
    }
}

fn shot_position(
    visuals: &Query<(&UnitVisual, &Players, &Transform)>,
    generation: u32,
    game: &generals_sim::skirmish::Skirmish,
    shot: &generals_sim::skirmish::Shot,
    animations: &Animations,
) -> [f32; 3] {
    let actor = &game.entities[shot.source];
    let drawable = visuals
        .iter()
        .find(|(visual, _, _)| visual.generation == generation && visual.index == shot.source);
    let offset = drawable.and_then(|(_, players, _)| {
        match animations.weapon_fire_position(actor, shot.weapon_slot, shot.barrel, players) {
            Ok(position) => position,
            Err(error) => {
                error!("Original weapon FX bone: {error}");
                None
            }
        }
    });
    if actor.attachment_parent().is_some()
        && let (Some(offset), Some((_, _, transform))) = (offset, drawable)
    {
        return attachment_fire_position(offset, transform);
    }
    fire_position(
        if actor.contained_by().is_some() {
            None
        } else {
            offset
        },
        shot,
    )
}

fn attachment_fire_position(offset: [f32; 3], transform: &Transform) -> [f32; 3] {
    // DependencyModelDraw has already resolved the parent drawable's animated
    // bone; the portable weapon's own animated muzzle uses that full matrix.
    let point = transform.transform_point(Vec3::new(offset[0], offset[2], -offset[1]) * 0.1);
    [point.x * 10.0, -point.z * 10.0, point.y * 10.0]
}

fn fire_position(offset: Option<[f32; 3]>, shot: &generals_sim::skirmish::Shot) -> [f32; 3] {
    offset.map_or(shot.source_position, |offset| {
        let (sine, cosine) = shot.source_facing.sin_cos();
        let x_cosine = offset[0] * cosine;
        let y_sine = offset[1] * sine;
        let x_sine = offset[0] * sine;
        let y_cosine = offset[1] * cosine;
        [
            shot.source_position[0] + (x_cosine - y_sine),
            shot.source_position[1] + (x_sine + y_cosine),
            shot.source_position[2] + offset[2],
        ]
    })
}

type TracerMesh = (
    Entity,
    &'static mut TracerVisual,
    &'static Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static mut Transform,
);

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    mut effects: ResMut<WeaponFx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    mut tracers: Query<TracerMesh>,
    timings: Option<Res<crate::render_performance::Timings>>,
) {
    let timings = timings.map(Res::into_inner);
    let _span = timings.map(|timings| timings.span("weapon_fx_draw"));
    let battle = battle.into_inner();
    for (entity, mut tracer, _, material, mut transform) in &mut tracers {
        if !battle.started
            || tracer.generation != battle.generation
            || tracer.expired(battle.simulation.tick)
        {
            commands.entity(entity).despawn();
            continue;
        }
        // Native doDrawModule is called by W3DView::update even when logic is
        // paused. It advances/fades each client draw, not once per logic tick.
        tracer.draw(battle.simulation.tick);
        *transform = tracer.transform();
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.parameters.diffuse.w = tracer.opacity;
        }
    }
    for mut tracer in std::mem::take(&mut effects.pending) {
        if !battle.started
            || tracer.generation != battle.generation
            || tracer.expired(battle.simulation.tick)
        {
            continue;
        }
        tracer.draw(battle.simulation.tick);
        let material = SourceMaterial::new(
            [0.0; 3],
            [0.0; 3],
            tracer.color,
            tracer.opacity,
            AlphaMode::Blend,
            false,
        );
        let mesh = meshes.add(tracer.mesh());
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(material)),
            tracer.transform(),
            RenderLayers::layer(2),
            tracer,
        ));
    }
}

#[cfg(test)]
mod attachment_tests {
    use super::*;

    #[test]
    fn mounted_muzzle_keeps_parent_height_and_rotated_child_bone_offset() {
        let transform = Transform::from_xyz(10.0, 4.0, -20.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let point = attachment_fire_position([5.0, 0.0, 2.0], &transform);
        for (actual, expected) in point.into_iter().zip([100.0, 205.0, 42.0]) {
            assert!((actual - expected).abs() < 0.0001);
        }
    }
}
