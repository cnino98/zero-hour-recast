//! `W3DRopeDraw` constructor, ordered client RNG, and each-client-frame drawing.
use super::{WeaponFx, line};
use crate::{
    model_animation::Animations, particles::Particles, playtest::Match,
    source_material::SourceMaterial,
};
use bevy::{camera::visibility::RenderLayers, prelude::*};
use generals_sim::skirmish::chinook::{RopeDrawable, VisualEvent};
use num_traits::ToPrimitive;
use std::{collections::BTreeMap, f32::consts::TAU};

struct Drawable {
    axes: Vec<[f32; 2]>,
    phase: f32,
    offset: f32,
    speed: f32,
    released: bool,
    segments: Vec<(Entity, Handle<Mesh>)>,
}

pub(super) struct Ropes {
    generation: Option<u32>,
    drawables: BTreeMap<u64, Drawable>,
    retired: Vec<Entity>,
}

impl Ropes {
    pub(super) const fn new() -> Self {
        Self {
            generation: None,
            drawables: BTreeMap::new(),
            retired: Vec::new(),
        }
    }

    pub(super) fn record(
        &mut self,
        battle: &Match,
        event: &VisualEvent,
        animations: &mut Animations,
        particles: &mut Particles,
    ) {
        if self.generation != Some(battle.generation) {
            for (_, drawable) in std::mem::take(&mut self.drawables) {
                self.retired
                    .extend(drawable.segments.into_iter().map(|(entity, _)| entity));
            }
            self.generation = Some(battle.generation);
        }
        match event {
            VisualEvent::RotorWash {
                position,
                elevation,
                particle,
                visible_players,
                ..
            } => {
                if visible_players & (1 << battle.observer_player.unwrap_or(0)) != 0
                    && animations.effect_real(battle.generation, 0.0, *elevation) < 5.0
                {
                    particles.chinook_rotor_wash(battle, particle, *position, animations);
                }
            }
            VisualEvent::RopeCreated(id) => {
                let rope = battle
                    .simulation
                    .rope_drawables
                    .drawables
                    .iter()
                    .find(|rope| rope.id == *id)
                    .expect("ordered rope drawable creation");
                let count = (rope.maximum / rope.wobble[0])
                    .ceil()
                    .to_usize()
                    .expect("validated rope segment count");
                let axes = (0..count)
                    .map(|_| {
                        let angle = animations.effect_real(battle.generation, 0.0, TAU);
                        [angle.cos(), angle.sin()]
                    })
                    .collect();
                self.drawables.insert(
                    *id,
                    Drawable {
                        axes,
                        phase: 0.0,
                        offset: 0.0,
                        speed: 0.0,
                        released: false,
                        segments: Vec::new(),
                    },
                );
            }
        }
    }
}

impl Drawable {
    fn points(&mut self, rope: &RopeDrawable) -> Vec<([f32; 3], [f32; 3])> {
        if !self.released
            && let Some(release) = &rope.released
        {
            self.released = true;
            self.speed = release.speed;
        }
        let deflection = self.phase.sin() * rope.wobble[1];
        let count = self.axes.len().to_f32().expect("native rope segment count");
        let length = rope.length / count;
        let mut start = [
            rope.position[0],
            rope.position[1],
            rope.position[2] + self.offset,
        ];
        let result = self
            .axes
            .iter()
            .map(|axis| {
                let lateral = axis.map(|component| deflection * component);
                let end = [
                    rope.position[0] + lateral[0],
                    rope.position[1] + lateral[1],
                    start[2] - length,
                ];
                let segment = (start, end);
                start = end;
                segment
            })
            .collect();
        self.phase += rope.wobble[2];
        if self.phase > TAU {
            self.phase -= TAU;
        }
        self.offset += self.speed;
        if let Some(release) = &rope.released {
            self.speed += release.acceleration;
            if self.speed > release.maximum_speed {
                self.speed = release.maximum_speed;
            } else if self.speed < -release.maximum_speed {
                self.speed = -release.maximum_speed;
            }
        }
        result
    }
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    mut effects: ResMut<WeaponFx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
) {
    let battle = battle.into_inner();
    let ropes = &mut effects.ropes;
    for entity in std::mem::take(&mut ropes.retired) {
        commands.entity(entity).despawn();
    }
    let generation = ropes.generation;
    ropes.drawables.retain(|id, drawable| {
        let rope = battle
            .simulation
            .rope_drawables
            .drawables
            .iter()
            .find(|rope| rope.id == *id);
        let valid = battle.started
            && generation == Some(battle.generation)
            && rope.is_some_and(|rope| {
                rope.released
                    .as_ref()
                    .is_none_or(|release| battle.simulation.tick < u64::from(release.expires))
            });
        if !valid {
            for (entity, _) in &drawable.segments {
                commands.entity(*entity).despawn();
            }
            return false;
        }
        let rope = rope.unwrap();
        let points = drawable.points(rope);
        for (index, (start, end)) in points.into_iter().enumerate() {
            let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
            let squared = delta.map(|component| component * component);
            let length = (squared[0] + squared[1] + squared[2]).sqrt();
            let direction = if length == 0.0 {
                delta
            } else {
                delta.map(|component| component * (1.0 / length))
            };
            let axes = line::axes(direction);
            for (soft, opacity) in [1.0, 0.5].into_iter().enumerate() {
                let width = if soft == 0 {
                    rope.width * 0.5
                } else {
                    rope.width
                };
                let vertices = line::vertices(axes, length, width);
                let transform =
                    Transform::from_xyz(start[0] * 0.1, start[2] * 0.1, -start[1] * 0.1);
                if let Some((entity, mesh)) = drawable.segments.get(index * 2 + soft) {
                    meshes
                        .get_mut(mesh)
                        .expect("live rope mesh")
                        .insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
                    commands.entity(*entity).insert(transform);
                } else {
                    let mesh = meshes.add(line::mesh(vertices));
                    let material = SourceMaterial::new(
                        [0.0; 3],
                        [0.0; 3],
                        rope.color,
                        opacity,
                        if soft == 0 {
                            AlphaMode::Opaque
                        } else {
                            AlphaMode::Blend
                        },
                        false,
                    );
                    let entity = commands
                        .spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(materials.add(material)),
                            transform,
                            RenderLayers::layer(2),
                        ))
                        .id();
                    drawable.segments.push((entity, mesh));
                }
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_rope_segments_reuse_random_axes_and_advance_phase_per_client_draw() {
        let rope = RopeDrawable {
            id: 0,
            template: "GenericRope".into(),
            position: [10.0, 20.0, 30.0],
            maximum: 20.0,
            length: 20.0,
            width: 0.5,
            color: [0.9, 0.8, 0.7],
            wobble: [10.0, 2.0, std::f32::consts::FRAC_PI_2],
            released: None,
        };
        let mut draw = Drawable {
            axes: vec![[1.0, 0.0], [0.0, 1.0]],
            phase: 0.0,
            offset: 0.0,
            speed: 0.0,
            released: false,
            segments: Vec::new(),
        };
        let first = draw.points(&rope);
        assert_eq!(
            first[0].0.map(f32::to_bits),
            [10.0_f32, 20.0, 30.0].map(f32::to_bits)
        );
        assert_eq!(
            first[1].1.map(f32::to_bits),
            [10.0_f32, 20.0, 10.0].map(f32::to_bits)
        );
        let next = draw.points(&rope);
        assert_eq!(
            next[0].1.map(f32::to_bits),
            [12.0_f32, 20.0, 20.0].map(f32::to_bits)
        );
        assert_eq!(
            next[1].1.map(f32::to_bits),
            [10.0_f32, 22.0, 10.0].map(f32::to_bits)
        );
        assert_eq!(draw.axes, [[1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(rope.length.to_bits(), 20.0_f32.to_bits());
    }
}
