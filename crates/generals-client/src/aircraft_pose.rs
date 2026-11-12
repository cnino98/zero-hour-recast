//! Original `Drawable::calcPhysicsXformHoverOrWings`, advanced once per drawn logic frame.
use crate::{playtest::Match, playtest_view::UnitVisual};
use bevy::prelude::*;
use generals_sim::skirmish::flight::draw::{Motion, Rules};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

pub struct Pose {
    angle: [f32; 2],
    rate: [f32; 2],
    acceleration: [f32; 2],
    acceleration_rate: [f32; 2],
    modulator: [f32; 2],
    total: [f32; 3],
    previous: [f32; 3],
    frame: Option<u64>,
}

impl Pose {
    const fn new() -> Self {
        Self {
            angle: [0.0; 2],
            rate: [0.0; 2],
            acceleration: [0.0; 2],
            acceleration_rate: [0.0; 2],
            modulator: [0.0; 2],
            total: [0.0; 3],
            previous: [0.0; 3],
            frame: None,
        }
    }

    fn advance(&mut self, frame: u64, facing: f32, motion: &Motion) {
        if self.frame == Some(frame) {
            return;
        }
        self.previous = self.total;
        self.calculate(facing, motion);
        for value in &mut self.total {
            if *value > -1e-20 && *value < 1e-20 {
                *value = 0.0;
            }
        }
        // Original new/undrawn sync frames don't interpolate stale values.
        if self.frame.is_none_or(|previous| previous + 1 != frame) {
            self.previous = self.total;
        }
        self.frame = Some(frame);
    }

    fn calculate(&mut self, facing: f32, motion: &Motion) {
        let rules = motion.rules;
        for axis in 0..2 {
            let spring = -rules.stiffness[axis] * self.angle[axis];
            let damper = -rules.damping[axis] * self.rate[axis];
            self.rate[axis] += spring + damper;
            let movement = self.rate[axis] * rules.axial_damping;
            self.angle[axis] += movement;
            let spring = -rules.stiffness[axis] * self.acceleration[axis];
            let damper = -rules.damping[axis] * self.acceleration_rate[axis];
            self.acceleration_rate[axis] += spring + damper;
            self.acceleration[axis] += self.acceleration_rate[axis];
            self.total[axis] = self.angle[axis] + self.acceleration[axis];
        }
        if motion.motive {
            self.react(rules, facing, motion.velocity, motion.acceleration);
        }
        for angle in &mut self.acceleration {
            if *angle > rules.pitch_limits[1] {
                *angle = rules.pitch_limits[1];
            } else if *angle < -rules.pitch_limits[0] {
                *angle = -rules.pitch_limits[0];
            }
        }
        self.modulator[0] += rules.rudder[1];
        self.modulator[1] += rules.elevator[1];
        self.total[2] = rules.rudder[0] * self.modulator[0].sin();
        let elevator = rules.elevator[0] * self.modulator[1].cos();
        self.total[0] += elevator;
    }

    fn react(&mut self, rules: &Rules, facing: f32, velocity: [f32; 3], acceleration: [f32; 3]) {
        if rules.velocity[2] != 0.0 && velocity[2].abs() > 0.001 {
            let x = velocity[0] * velocity[0];
            let y = velocity[1] * velocity[1];
            let pitch = velocity[2].atan2((x + y).sqrt());
            let change = rules.velocity[2] * pitch;
            self.angle[0] -= change;
        }
        let (sine, cosine) = facing.sin_cos();
        let forward = [cosine, sine];
        let lateral = [-sine, cosine];
        for (axis, direction) in [forward, lateral].into_iter().enumerate() {
            let x = direction[0] * velocity[0];
            let y = direction[1] * velocity[1];
            self.angle[axis] += -(rules.velocity[axis] * (x + y));
            let x = direction[0] * acceleration[0];
            let y = direction[1] * acceleration[1];
            self.acceleration_rate[axis] += -(rules.acceleration[axis] * (x + y));
        }
    }

    fn rotation(&self, phase: f32) -> Quat {
        let angles: [f32; 3] = std::array::from_fn(|axis| {
            let step = (self.total[axis] - self.previous[axis]) * phase;
            self.previous[axis] + step
        });
        // Native XYZ maps to Bevy X,-Z,Y. Preserve Y(pitch), X(-roll), Z(yaw)
        // matrix multiplication order instead of substituting Euler angles.
        Quat::from_rotation_z(-angles[0])
            * Quat::from_rotation_x(-angles[1])
            * Quat::from_rotation_y(angles[2])
    }
}

pub fn update(
    battle: Res<Match>,
    time: Res<Time<Fixed>>,
    mut poses: Local<BTreeMap<(u32, usize), Pose>>,
    mut visuals: Query<(
        &UnitVisual,
        &mut Transform,
        &Visibility,
        Option<&mut crate::light_environment::DrawPose>,
    )>,
) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    poses.retain(|&(generation, _), _| generation == battle.generation);
    for (visual, mut transform, visibility, native_pose) in &mut visuals {
        if visual.generation != battle.generation || matches!(visibility, Visibility::Hidden) {
            continue;
        }
        let actor = &battle.simulation.entities[visual.index];
        if !actor.alive() || !battle.simulation.logically_visible(0, visual.index) {
            continue;
        }
        let Some(motion) = actor.aircraft_draw_motion(
            &battle.simulation.catalog[actor.type_index],
            battle.simulation.tick,
        ) else {
            continue;
        };
        let pose = poses
            .entry((visual.generation, visual.index))
            .or_insert_with(Pose::new);
        pose.advance(battle.simulation.tick, actor.facing, &motion);
        let rotation = pose.rotation(time.overstep_fraction().clamp(0.0, 1.0));
        transform.rotation *= rotation;
        if let Some(mut native) = native_pose {
            let view = Mat3::from_quat(rotation);
            let source =
                [view.x_axis, -view.z_axis, view.y_axis].map(|axis| [axis.x, -axis.z, axis.y]);
            *native = native.compose(crate::light_environment::DrawPose {
                position: [0.0; 3],
                basis: std::array::from_fn(|row| std::array::from_fn(|column| source[column][row])),
            });
        }
    }
}
