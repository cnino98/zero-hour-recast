//! SH `LaserUpdate.cpp`: client-owned endpoints, radius and particle lifetime.
use super::{Animations, Players};
use crate::{particles::Particles, playtest::Match, playtest_view::UnitVisual};
use bevy::prelude::*;
use generals_sim::skirmish::laser_update::Event;
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

pub(super) struct Lasers {
    pub(super) active: BTreeMap<usize, State>,
    generation: u32,
}

pub(super) struct State {
    pub(super) parent: Option<usize>,
    target: Option<usize>,
    bone: String,
    pub(super) points: [[f32; 3]; 2],
    pub(super) position: [f32; 3],
    pub(super) radius: Radius,
    emitters: [Option<u64>; 2],
    pub(super) drawable_present: bool,
}

pub(super) struct Radius {
    widening: bool,
    widen_start: u32,
    widen_finish: u32,
    decaying: bool,
    decay_start: u32,
    decay_finish: u32,
    pub(super) width: f32,
}
impl Radius {
    const fn new() -> Self {
        Self {
            widening: false,
            widen_start: 0,
            widen_finish: 0,
            decaying: false,
            decay_start: 0,
            decay_finish: 0,
            width: 1.0,
        }
    }
    const fn initialize(&mut self, delta: i32, now: u32) {
        if delta > 0 {
            self.widening = true;
            self.widen_start = now;
            self.widen_finish = now.wrapping_add(delta.unsigned_abs());
            self.width = 0.0;
        } else if delta < 0 {
            self.decaying = true;
            self.decay_start = now;
            self.decay_finish = now.wrapping_add(delta.unsigned_abs());
            self.width = 1.0;
        }
    }
    fn update(&mut self, now: u32) {
        if self.decaying {
            self.width = 1.0
                - now
                    .wrapping_sub(self.decay_start)
                    .to_f32()
                    .expect("native unsigned Real")
                    / self
                        .decay_finish
                        .wrapping_sub(self.decay_start)
                        .to_f32()
                        .expect("native unsigned Real");
            if self.width <= 0.0 {
                self.width = 0.0;
            }
        } else if self.widening {
            self.width = now
                .wrapping_sub(self.widen_start)
                .to_f32()
                .expect("native unsigned Real")
                / self
                    .widen_finish
                    .wrapping_sub(self.widen_start)
                    .to_f32()
                    .expect("native unsigned Real");
            if self.width >= 1.0 {
                self.width = 1.0;
                self.widening = false;
            }
        }
    }
}
impl State {
    const fn new() -> Self {
        Self {
            parent: None,
            target: None,
            bone: String::new(),
            points: [[0.0; 3]; 2],
            position: [0.0; 3],
            radius: Radius::new(),
            emitters: [None; 2],
            drawable_present: true,
        }
    }
    fn initialize_start(
        &mut self,
        event: &Event,
        generation: u32,
        visuals: &Query<(&UnitVisual, &Players, &Transform)>,
    ) {
        let Some(pose) = &event.parent_pose else {
            return;
        };
        self.points[0] = current_start(
            pose,
            event.parent_drawable.expect("captured parent Drawable"),
            generation,
            &self.bone,
            visuals,
        );
    }
    fn update_start(
        &mut self,
        battle: &Match,
        visuals: &Query<(&UnitVisual, &Players, &Transform)>,
    ) {
        if let Some(parent) = self.parent.filter(|&id| {
            battle.simulation.entities[id].exists() && battle.simulation.entities[id].has_drawable()
        }) {
            let actor = &battle.simulation.entities[parent];
            let pose = generals_sim::skirmish::laser_update::DrawableSnapshot {
                position: actor.drawable_position().expect("tracked parent Drawable"),
                basis: battle.simulation.object_transform(parent).1,
                revision: actor.drawable_transform_revision(),
            };
            self.points[0] = current_start(&pose, parent, battle.generation, &self.bone, visuals);
        }
    }
    fn update_end(&mut self, battle: &Match, punch: f32) {
        if let Some(target) = self.target {
            let target = &battle.simulation.entities[target];
            if !target.exists() || !target.has_drawable() || target.is_effectively_dead() {
                self.lose_target(punch);
            } else if let Some(position) = target.drawable_position() {
                self.points[1] = position;
            }
        }
    }
    fn lose_target(&mut self, punch: f32) {
        if punch > 0.0 {
            let offset =
                std::array::from_fn::<_, 3, _>(|axis| self.points[1][axis] - self.points[0][axis]);
            let offset = offset.map(|value| value * punch);
            self.points[1] = std::array::from_fn(|axis| offset[axis] + self.points[0][axis]);
        }
        self.target = None;
    }
}
fn current_start(
    pose: &generals_sim::skirmish::laser_update::DrawableSnapshot,
    parent: usize,
    generation: u32,
    name: &str,
    visuals: &Query<(&UnitVisual, &Players, &Transform)>,
) -> [f32; 3] {
    if name.is_empty() {
        return pose.position;
    }
    let Some((_, players, transform)) = visuals
        .iter()
        .find(|(visual, _, _)| visual.index == parent && visual.generation == generation)
    else {
        return pose.position;
    };
    let Some((bone, revision)) = players.current_bone(name) else {
        return pose.position;
    };
    if revision == pose.revision {
        // No Object notification since draw(): preserve its complete client
        // transform, including instance/physics transforms and container weld.
        let point = transform.transform_point(
            Vec3::new(
                bone.translation[0],
                bone.translation[2],
                -bone.translation[1],
            ) * 0.1,
        );
        [point.x * 10.0, -point.z * 10.0, point.y * 10.0]
    } else {
        // W3DModelDraw::reactToTransformChange sets the raw Object transform,
        // without draw()'s instance/physics transforms or new turret controls.
        let offset = pose.basis.map(|row| {
            let x = row[0] * bone.translation[0];
            let y = row[1] * bone.translation[1];
            let z = row[2] * bone.translation[2];
            x + y + z
        });
        std::array::from_fn(|axis| pose.position[axis] + offset[axis])
    }
}

impl Lasers {
    pub(super) const fn new() -> Self {
        Self {
            active: BTreeMap::new(),
            generation: 0,
        }
    }
    fn reset_generation(&mut self, generation: u32) {
        if self.generation != generation {
            self.active.clear();
            self.generation = generation;
        }
    }
    pub(super) fn retire(&mut self, object: usize, particles: &mut Particles) {
        if let Some(state) = self.active.remove(&object) {
            particles.retire_optional_laser_emitters(state.emitters);
        }
    }
    pub(super) fn initialize(
        &mut self,
        battle: &Match,
        event: &Event,
        animations: &mut Animations,
        particles: &mut Particles,
        visuals: &Query<(&UnitVisual, &Players, &Transform)>,
    ) {
        self.reset_generation(battle.generation);
        let game = &battle.simulation;
        let rules = game.catalog[game.entities[event.object].type_index]
            .laser_update
            .as_ref()
            .expect("native LaserUpdate metadata");
        let state = self.active.entry(event.object).or_insert_with(State::new);
        state.radius.initialize(event.init.size_delta, event.frame);
        state.bone.clone_from(&event.init.bone);
        if event.init.parent.is_some() {
            if let Some(drawable) = event.parent_drawable {
                state.parent = Some(drawable);
            }
            state.initialize_start(event, battle.generation, visuals);
        } else if let Some(start) = event.init.start {
            state.points[0] = start;
        } else {
            state.drawable_present = false;
            particles.retire_optional_laser_emitters(state.emitters);
            state.emitters = [None; 2];
            return;
        }
        if let Some(end) = event.init.end {
            state.points[1] = end;
        } else if event.init.target.is_some() {
            if let Some(drawable) = event.target_drawable {
                state.target = Some(drawable);
            }
            state.points[1] = event.target_position.expect("native target position");
        } else {
            state.drawable_present = false;
            particles.retire_optional_laser_emitters(state.emitters);
            state.emitters = [None; 2];
            return;
        }
        // Native gates BOTH particle creations on the muzzle system handle.
        // Particle positions change only during initLaser, not clientUpdate.
        let viewer = battle.observer_player.unwrap_or(0);
        if state.emitters[0].is_none() && event.parent_visible & (1 << viewer) != 0 {
            let created = particles.optional_laser_emitters(
                &rules.particles,
                state.points,
                animations,
                battle.generation,
                u64::from(event.frame),
            );
            for (slot, created) in state.emitters.iter_mut().zip(created) {
                if created.is_some() {
                    *slot = created;
                }
            }
        }
        particles.position_laser_emitters(state.emitters, state.points, true);
        state.position = event.init.parent.map_or_else(
            || {
                let sum = std::array::from_fn::<_, 3, _>(|axis| {
                    event.init.start.expect("native start")[axis]
                        + event.init.end.expect("native end")[axis]
                });
                sum.map(|value| value * 0.5)
            },
            |_| event.parent_position.expect("native parent position"),
        );
    }
    pub(super) fn update(
        &mut self,
        battle: &Match,
        particles: &mut Particles,
        visuals: &Query<(&UnitVisual, &Players, &Transform)>,
    ) {
        self.reset_generation(battle.generation);
        if !battle.started {
            for state in self.active.values() {
                particles.retire_optional_laser_emitters(state.emitters);
            }
            self.active.clear();
            return;
        }
        for (&id, state) in &mut self.active {
            if !state.drawable_present {
                continue;
            }
            state.update_start(battle, visuals);
            let rules = battle.simulation.catalog[battle.simulation.entities[id].type_index]
                .laser_update
                .as_ref()
                .expect("native LaserUpdate metadata");
            state.update_end(battle, rules.punch_through);
            state
                .radius
                .update(u32::try_from(battle.simulation.tick).expect("native frame"));
        }
    }
}

#[cfg(test)]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[ignore = "requires original game installation"]
    #[test]
    fn laser_snapshot_keeps_drawn_instance_pose_then_uses_notified_raw_pose_with_old_skeleton() {
        use bevy::ecs::system::SystemState;
        use generals_sim::skirmish::laser_update::{DrawableSnapshot, Initialization};
        let (battle, _, players, actor) = crate::model_animation::laser_snapshot_fixture();
        let (bone, revision) = players.current_bone("Muzzle01").unwrap();
        let local = bone.translation;
        let transform = Transform::from_xyz(10.0, 3.0, -20.0).with_scale(Vec3::splat(2.0));
        let point = transform.transform_point(Vec3::new(local[0], local[2], -local[1]) * 0.1);
        let expected_drawn = [point.x * 10.0, -point.z * 10.0, point.y * 10.0];
        let mut world = World::new();
        world.spawn((
            UnitVisual {
                index: actor,
                generation: battle.generation,
            },
            players,
            transform,
        ));
        let mut query = SystemState::<Query<(&UnitVisual, &Players, &Transform)>>::new(&mut world);
        let mut event = Event {
            object: actor,
            frame: 3,
            init: Initialization {
                parent: Some(actor),
                target: None,
                start: None,
                end: Some([500.0, 600.0, 70.0]),
                bone: "Muzzle01".into(),
                size_delta: 0,
            },
            parent_drawable: Some(actor),
            parent_pose: Some(DrawableSnapshot {
                position: [100.0, 200.0, 30.0],
                basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                revision,
            }),
            target_drawable: None,
            parent_position: Some([100.0, 200.0, 30.0]),
            target_position: None,
            parent_visible: 1,
        };
        let mut state = State::new();
        state.bone.clone_from(&event.init.bone);
        state.initialize_start(&event, battle.generation, &query.get(&world).unwrap());
        assert_eq!(
            state.points[0].map(f32::to_bits),
            expected_drawn.map(f32::to_bits)
        );
        let pose = event.parent_pose.as_mut().unwrap();
        pose.position = [500.0, 600.0, 70.0];
        pose.revision = revision.wrapping_add(1);
        let expected_raw = std::array::from_fn::<_, 3, _>(|axis| pose.position[axis] + local[axis]);
        state.initialize_start(&event, battle.generation, &query.get(&world).unwrap());
        assert_eq!(
            state.points[0].map(f32::to_bits),
            expected_raw.map(f32::to_bits)
        );
        assert_ne!(
            state.points[0].map(f32::to_bits),
            expected_drawn.map(f32::to_bits)
        );
        // Destroying the visual later cannot change an empty/missing bone's
        // original Drawable fallback, and draw-height/instance offsets do not enter it.
        world.clear_entities();
        state.bone.clear();
        state.initialize_start(&event, battle.generation, &query.get(&world).unwrap());
        assert_eq!(
            state.points[0].map(f32::to_bits),
            [500.0_f32, 600.0, 70.0].map(f32::to_bits)
        );
        state.bone = "MissingOriginalBone".into();
        state.initialize_start(&event, battle.generation, &query.get(&world).unwrap());
        assert_eq!(
            state.points[0].map(f32::to_bits),
            [500.0_f32, 600.0, 70.0].map(f32::to_bits)
        );
    }

    #[test]
    fn radius_wraps_clock_and_keeps_decay_precedence_across_reinitialization() {
        let mut radius = Radius::new();
        radius.initialize(10, u32::MAX - 4);
        radius.update(0);
        assert_eq!(radius.width.to_bits(), 0.5_f32.to_bits());
        radius.update(5);
        assert_eq!(radius.width.to_bits(), 1.0_f32.to_bits());
        assert!(!radius.widening);
        radius.initialize(-10, 10);
        radius.update(15);
        assert_eq!(radius.width.to_bits(), 0.5_f32.to_bits());
        radius.initialize(20, 15);
        radius.update(20);
        assert_eq!(radius.width.to_bits(), 0.0_f32.to_bits());
        assert!(radius.decaying);
        assert!(radius.widening);
    }
    #[test]
    fn target_loss_extends_the_last_source_to_target_vector_and_releases_tracking() {
        let mut state = State::new();
        state.target = Some(7);
        state.points = [[1.0, 2.0, 3.0], [4.0, 6.0, 8.0]];
        state.lose_target(2.0);
        assert_eq!(
            state.points[1].map(f32::to_bits),
            [7.0_f32, 10.0, 13.0].map(f32::to_bits)
        );
        assert_eq!(state.target, None);
        state.target = Some(8);
        state.lose_target(-2.0);
        assert_eq!(
            state.points[1].map(f32::to_bits),
            [7.0_f32, 10.0, 13.0].map(f32::to_bits)
        );
        assert_eq!(state.target, None);
    }
}
