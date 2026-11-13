//! Wheel transforms from SH `Drawable::calcPhysicsXformMotorcycle` and `Drawable::calcPhysicsXformWheels`
//! and `W3DTruckDraw::doDrawModule`, using the committed WW3D sync clock.
use crate::{
    light_environment::DrawPose,
    skirmish_data::{
        definition, field,
        locomotor::{self, signed_real},
        scalar,
    },
};
use bevy::prelude::{Quat, Transform, Vec3};
use generals_assets::{
    content::{ContentFile, ContentLine, Declaration},
    w3d::{BoneControl, Model},
};
use generals_sim::skirmish::combat_bike::DrawMotion;
use std::{
    error::Error,
    f32::consts::{FRAC_PI_2, PI, TAU},
};

pub(super) struct Rules {
    bones: [Option<String>; 10],
    spin: f32,
    powerslide_spin: f32,
    normal: Vec<Suspension>,
    sluggish: Vec<Suspension>,
}

struct Suspension {
    four_wheels: bool,
    roll_stiffness: f32,
    roll_damping: f32,
    lateral_acceleration: f32,
    pitch_stiffness: f32,
    pitch_damping: f32,
    axial_damping: f32,
    acceleration_pitch: f32,
    pitch_limits: [f32; 2],
    wheel_extension: f32,
    wheel_turn: f32,
}

impl Rules {
    pub(super) fn load(
        files: &[ContentFile],
        object: &Declaration,
        draw: &[ContentLine],
    ) -> Result<Self, Box<dyn Error>> {
        let mut rules = Self::load_wheels(draw)?;
        let load_set = |set| -> Result<Vec<Suspension>, Box<dyn Error>> {
            locomotor::named_set(&object.lines, set)?
                .into_iter()
                .map(|name| {
                    let definition = definition(files, "Locomotor", name)?;
                    Suspension::load(&definition.lines)
                })
                .collect()
        };
        rules.normal = load_set("SET_NORMAL")?;
        rules.sluggish = load_set("SET_SLUGGISH")?;
        Ok(rules)
    }

    pub(super) fn load_wheels(draw: &[ContentLine]) -> Result<Self, Box<dyn Error>> {
        let mut bones = std::array::from_fn(|_| None);
        for (bone, key) in bones.iter_mut().zip([
            "LeftFrontTireBone",
            "RightFrontTireBone",
            "MidLeftFrontTireBone",
            "MidRightFrontTireBone",
            "LeftRearTireBone",
            "RightRearTireBone",
            "MidLeftRearTireBone",
            "MidRightRearTireBone",
            "MidLeftMidTireBone",
            "MidRightMidTireBone",
        ]) {
            *bone = field(draw, 1, key)
                .map(scalar)
                .transpose()?
                .map(str::to_owned);
        }
        Ok(Self {
            bones,
            spin: signed_real(draw, 1, "TireRotationMultiplier", 0.0)?,
            powerslide_spin: signed_real(draw, 1, "PowerslideRotationAddition", 0.0)?,
            normal: Vec::new(),
            sluggish: Vec::new(),
        })
    }
}

impl Suspension {
    fn load(lines: &[ContentLine]) -> Result<Self, Box<dyn Error>> {
        if field(lines, 0, "HasSuspension")
            .map(scalar)
            .transpose()?
            .is_none_or(|value| !value.eq_ignore_ascii_case("yes"))
            || signed_real(lines, 0, "BounceAmount", 0.0)? != 0.0
        {
            return Err("wheel suspension requires its installed zero-bounce locomotor".into());
        }
        let radians = |key| -> Result<f32, Box<dyn Error>> {
            Ok(signed_real(lines, 0, key, 0.0)? * (PI / 180.0))
        };
        let acceleration = radians("AccelerationPitchLimit")?;
        let deceleration = radians("DecelerationPitchLimit")?;
        let four_wheels = match field(lines, 0, "Appearance").map(scalar).transpose()? {
            Some("FOUR_WHEELS") => true,
            Some("MOTORCYCLE") => false,
            _ => return Err("unsupported wheel suspension appearance".into()),
        };
        Ok(Self {
            four_wheels,
            roll_stiffness: signed_real(lines, 0, "RollStiffness", 0.1)?,
            roll_damping: signed_real(lines, 0, "RollDamping", 0.9)?,
            lateral_acceleration: signed_real(lines, 0, "LateralAccelerationRollFactor", 0.0)?,
            pitch_stiffness: signed_real(lines, 0, "PitchStiffness", 0.1)?,
            pitch_damping: signed_real(lines, 0, "PitchDamping", 0.9)?,
            axial_damping: signed_real(lines, 0, "UniformAxialDamping", 1.0)?,
            acceleration_pitch: signed_real(lines, 0, "ForwardAccelerationPitchFactor", 0.0)?,
            pitch_limits: [
                -acceleration,
                if deceleration == 0.0 {
                    acceleration
                } else {
                    deceleration
                },
            ],
            wheel_extension: signed_real(lines, 0, "MaximumWheelExtension", 0.0)?,
            wheel_turn: radians("FrontWheelTurnAngle")?,
        })
    }
}

pub(super) struct Wheels {
    pitch: f32,
    pitch_rate: f32,
    acceleration_pitch: f32,
    acceleration_pitch_rate: f32,
    roll: f32,
    roll_rate: f32,
    acceleration_roll: f32,
    acceleration_roll_rate: f32,
    heights: [f32; 4],
    steer: f32,
    rotation: [f32; 2],
    powersliding: bool,
    changed: bool,
    chassis: Chassis,
}

struct Chassis {
    totals: [f32; 3],
    previous: [f32; 3],
    sync_time: u32,
}

impl Chassis {
    const fn new() -> Self {
        Self {
            totals: [0.0; 3],
            previous: [0.0; 3],
            sync_time: u32::MAX,
        }
    }

    fn commit(&mut self, previous: [f32; 3], previous_sync: u32, sync: u32) {
        self.totals = self
            .totals
            .map(|value| if value.abs() < 1e-20 { 0.0 } else { value });
        self.previous = if self.sync_time == previous_sync {
            previous
        } else {
            self.totals
        };
        self.sync_time = sync;
    }

    fn apply(&self, phase: f32, transform: &mut Transform, pose: &mut DrawPose) {
        let [pitch, roll, height] = std::array::from_fn(|axis| {
            let previous = self.previous[axis];
            let step = (self.totals[axis] - previous) * phase;
            previous + step
        });
        if pitch == 0.0 && roll == 0.0 && height == 0.0 {
            return;
        }
        let (sp, cp) = pitch.sin_cos();
        let (sr, cr) = (-roll).sin_cos();
        // Drawable post-multiplies the instance basis by Tz, Ry and Rx in that order.
        *pose = pose.compose(DrawPose {
            position: [0.0, 0.0, height],
            basis: [
                [cp, sp * sr, sp * cr],
                [0.0, cr, -sr],
                [-sp, cp * sr, cp * cr],
            ],
        });
        transform.translation +=
            transform.rotation * (Vec3::Y * (transform.scale.y * height * 0.1));
        transform.rotation *= Quat::from_rotation_z(-pitch) * Quat::from_rotation_x(-roll);
    }
}

impl Wheels {
    pub(super) const fn new() -> Self {
        Self {
            pitch: 0.0,
            pitch_rate: 0.0,
            acceleration_pitch: 0.0,
            acceleration_pitch_rate: 0.0,
            roll: 0.0,
            roll_rate: 0.0,
            acceleration_roll: 0.0,
            acceleration_roll_rate: 0.0,
            heights: [0.0; 4],
            steer: 0.0,
            rotation: [0.0; 2],
            powersliding: false,
            changed: false,
            chassis: Chassis::new(),
        }
    }

    pub(super) fn update_chassis(
        &mut self,
        rules: &Rules,
        suspension: &Rules,
        motion: &DrawMotion,
        facing: f32,
        radii: [f32; 2],
        clock: &super::Clock,
    ) {
        let previous = self.chassis.totals;
        self.update(rules, suspension, motion, facing, radii);
        self.chassis.commit(
            previous,
            clock.previous_sync_milliseconds(),
            clock.committed_milliseconds(),
        );
    }

    pub(super) fn apply_chassis(&self, phase: f32, transform: &mut Transform, pose: &mut DrawPose) {
        self.chassis.apply(phase, transform, pose);
    }

    pub(super) fn update(
        &mut self,
        rules: &Rules,
        suspension: &Rules,
        motion: &DrawMotion,
        facing: f32,
        radii: [f32; 2],
    ) {
        let suspension = if motion.locomotor_set == generals_sim::skirmish::locomotor_set::SLUGGISH
        {
            &suspension.sluggish
        } else {
            &suspension.normal
        };
        let suspension = &suspension[motion.surface];
        let old = self.sample();
        self.suspend(suspension, motion, facing, radii);
        let [x, y, z] = motion.velocity;
        let speed = dot([x, y], [x, y]);
        let vertical_squared = z * z;
        let speed = (speed + vertical_squared).sqrt();
        let slide = if self.powersliding {
            rules.powerslide_spin
        } else {
            0.0
        };
        let direction = if motion.backwards { -1.0 } else { 1.0 };
        let front_spin = rules.spin * (direction * speed);
        let rear_spin = rules.spin * (direction * (speed + slide));
        self.rotation[0] = wrap(self.rotation[0] + front_spin);
        self.rotation[1] = wrap(self.rotation[1] + rear_spin);
        // The original computes this after using the prior frame for wheel spin.
        self.powersliding = motion.motive && !motion.airborne && motion.turning != 0;
        self.changed |= self.sample() != old;
    }

    fn suspend(&mut self, rules: &Suspension, motion: &DrawMotion, facing: f32, radii: [f32; 2]) {
        let direction = [facing.cos(), facing.sin()];
        let lateral = [-direction[1], direction[0]];
        let normal = [motion.normal[0], motion.normal[1]];
        let ground_pitch = dot(normal, direction) * FRAC_PI_2;
        let ground_roll = dot(normal, lateral) * FRAC_PI_2;
        let rest_pitch = if motion.airborne { 0.0 } else { ground_pitch };
        if motion.airborne {
            let target = if motion.altitude > -rules.wheel_extension {
                rules.wheel_extension
            } else {
                0.0
            };
            for offset in &mut self.heights[2..] {
                *offset += (target - *offset) / 2.0;
            }
            if rules.four_wheels {
                let pitch_height =
                    radii[0] * (self.pitch + self.acceleration_pitch - ground_pitch).sin();
                let roll_height =
                    radii[1] * (self.roll + self.acceleration_roll - ground_roll).sin();
                self.chassis.totals[2] = pitch_height.abs() / 4.0 + roll_height.abs() / 4.0;
                return;
            }
        }
        let pitch = self.update_pitch(rules, motion, direction, rest_pitch);
        // The motorcycle's authored roll condition is impossible, leaving total roll zero.
        let roll = if rules.four_wheels {
            self.update_roll(rules, motion, lateral, ground_roll)
        } else {
            0.0
        };
        let pitch_error = pitch - ground_pitch;
        let pitch_height = radii[0] * pitch_error.sin();
        let roll_height = radii[1] * (roll - ground_roll).sin();
        self.chassis.totals = [
            pitch,
            roll,
            if motion.airborne {
                0.0
            } else {
                chassis_height(pitch_error.abs(), pitch_height, roll_height)
            },
        ];
        self.suspend_axles(rules, pitch_height, roll_height);
        let mut desired = f32::from(motion.turning) * rules.wheel_turn;
        if dot([motion.velocity[0], motion.velocity[1]], direction) < 0.0 {
            desired = -desired;
        }
        self.steer += (desired - self.steer) / 10.0;
    }

    fn update_pitch(
        &mut self,
        rules: &Suspension,
        motion: &DrawMotion,
        direction: [f32; 2],
        rest_pitch: f32,
    ) -> f32 {
        let spring = -rules.pitch_stiffness * (self.pitch - rest_pitch);
        let damper = -rules.pitch_damping * self.pitch_rate;
        self.pitch_rate += spring + damper;
        if rules.four_wheels && self.pitch_rate > 0.0 {
            self.pitch_rate *= 0.5;
        }
        let pitch_step = self.pitch_rate * rules.axial_damping;
        self.pitch += pitch_step;
        let spring = -rules.pitch_stiffness * self.acceleration_pitch;
        let damper = -rules.pitch_damping * self.acceleration_pitch_rate;
        self.acceleration_pitch_rate += spring + damper;
        self.acceleration_pitch += self.acceleration_pitch_rate;
        let pitch = self.pitch + self.acceleration_pitch;
        if motion.motive {
            let forward_acceleration = dot(direction, motion.acceleration);
            let pitch_impulse = rules.acceleration_pitch * forward_acceleration;
            self.acceleration_pitch_rate -= pitch_impulse;
        }
        self.acceleration_pitch = self
            .acceleration_pitch
            .clamp(rules.pitch_limits[0], rules.pitch_limits[1]);
        pitch
    }

    fn suspend_axles(&mut self, rules: &Suspension, height: f32, roll_height: f32) {
        let axle = if height < 0.0 {
            [
                0.9 * (height / 3.0 + height / 2.0),
                -height / 2.0 + height / 4.0,
            ]
        } else {
            [
                -height / 4.0 + height / 2.0,
                0.9 * (-height / 2.0 + -height / 3.0),
            ]
        };
        let mut target = [axle[0], axle[0], axle[1], axle[1]];
        if rules.four_wheels {
            let height = roll_height;
            let (left, right) = if height > 0.0 {
                (
                    height / 2.0 - height / 4.0,
                    -0.9 * (height / 3.0 + height / 2.0),
                )
            } else {
                (
                    0.9 * (height / 3.0 + height / 2.0),
                    -height / 2.0 + height / 4.0,
                )
            };
            for pair in target.chunks_exact_mut(2) {
                pair[0] += left;
                pair[1] += right;
            }
        }
        for (offset, target) in self.heights.iter_mut().zip(target) {
            *offset = if target < *offset {
                *offset + (target - *offset) / 2.0
            } else {
                target
            };
            *offset = offset.max(rules.wheel_extension);
        }
    }

    fn update_roll(
        &mut self,
        rules: &Suspension,
        motion: &DrawMotion,
        lateral: [f32; 2],
        ground_roll: f32,
    ) -> f32 {
        let spring = -rules.roll_stiffness * (self.roll - ground_roll);
        let damper = -rules.roll_damping * self.roll_rate;
        self.roll_rate += spring + damper;
        let roll_step = self.roll_rate * rules.axial_damping;
        self.roll += roll_step;
        let spring = -rules.roll_stiffness * self.acceleration_roll;
        let damper = -rules.roll_damping * self.acceleration_roll_rate;
        self.acceleration_roll_rate += spring + damper;
        self.acceleration_roll += self.acceleration_roll_rate;
        let roll = self.roll + self.acceleration_roll;
        if motion.motive {
            let acceleration = dot(lateral, motion.acceleration);
            let impulse = rules.lateral_acceleration * acceleration;
            self.acceleration_roll_rate -= impulse;
        }
        self.acceleration_roll = self
            .acceleration_roll
            .clamp(rules.pitch_limits[0], rules.pitch_limits[1]);
        roll
    }

    fn sample(&self) -> [u32; 7] {
        [
            self.heights[0],
            self.heights[1],
            self.heights[2],
            self.heights[3],
            self.steer,
            self.rotation[0],
            self.rotation[1],
        ]
        .map(f32::to_bits)
    }

    pub(super) const fn take_change(&mut self) -> bool {
        std::mem::replace(&mut self.changed, false)
    }

    pub(super) fn controls(&self, rules: &Rules, model: &Model, controls: &mut Vec<BoneControl>) {
        for (index, bone) in rules.bones.iter().enumerate() {
            let Some(bone) = bone else { continue };
            if !model
                .bone_names
                .iter()
                .skip(1)
                .any(|name| name.eq_ignore_ascii_case(bone))
            {
                continue;
            }
            let rear = index >= 4;
            let offset = [0, 1, 0, 1, 2, 3, 2, 3, 2, 3][index];
            let (s, c) = self.rotation[usize::from(rear)].sin_cos();
            let (turn_s, turn_c) = if rear {
                (0.0, 1.0)
            } else {
                self.steer.sin_cos()
            };
            // Matrix3D post-multiplies Rz(steer) Ry(spin); BoneControl stores columns.
            controls.push(BoneControl {
                name: bone.clone(),
                axes: [
                    [turn_c * c, turn_s * c, -s],
                    [-turn_s, turn_c, 0.0],
                    [turn_c * s, turn_s * s, c],
                ],
                translation: [0.0, 0.0, self.heights[offset]],
            });
        }
    }
}

fn chassis_height(pitch_error: f32, pitch_height: f32, roll_height: f32) -> f32 {
    let divisor = if pitch_error > PI / 8.0 {
        ((4.0 * PI / 8.0) + (pitch_error - PI / 8.0)) / pitch_error
    } else {
        4.0
    };
    pitch_height.abs() / divisor + roll_height.abs() / divisor
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    let x = a[0] * b[0];
    let y = a[1] * b[1];
    x + y
}

fn wrap(angle: f32) -> f32 {
    let periods = ((angle + PI) / TAU).floor();
    let offset = TAU * periods;
    angle - offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truck_chassis_interpolates_consecutive_syncs_and_snaps_after_hidden_sync() {
        let mut chassis = Chassis::new();
        chassis.totals = [0.25, 0.5, 2.0];
        chassis.commit([0.0; 3], 0, 33);
        assert_eq!(
            chassis.previous.map(f32::to_bits),
            chassis.totals.map(f32::to_bits)
        );
        let previous = chassis.totals;
        chassis.totals = [0.75, 1.0, 6.0];
        chassis.commit(previous, 33, 66);

        let mut transform = Transform::from_xyz(10.0, 20.0, 30.0).with_scale(Vec3::splat(2.0));
        let mut pose = DrawPose {
            position: [1.0, 2.0, 3.0],
            basis: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
        };
        chassis.apply(0.5, &mut transform, &mut pose);
        assert_eq!(
            pose.position.map(f32::to_bits),
            [1.0_f32, 2.0, 11.0].map(f32::to_bits)
        );
        assert!((transform.translation.y - 20.8).abs() < 0.000_01);
        assert_eq!(transform.scale, Vec3::splat(2.0));
        let [x, y, z] = pose.point([0.0, 1.0, 0.0]);
        let bone = transform.rotation * Vec3::NEG_Z;
        assert!(bone.abs_diff_eq(Vec3::new(x - 1.0, z - 11.0, 2.0 - y) / 2.0, 0.000_001));

        chassis.totals = [0.0, 0.0, 1.0];
        chassis.commit(previous, 99, 133);
        assert_eq!(
            chassis.previous.map(f32::to_bits),
            chassis.totals.map(f32::to_bits)
        );
    }

    #[test]
    fn bomb_truck_mid_front_wheels_follow_their_front_axle_side() {
        let rules = Rules {
            bones: [
                Some("Tire01"),
                Some("Tire02"),
                Some("Tire03"),
                Some("Tire04"),
                Some("Tire05"),
                Some("Tire06"),
                None,
                None,
                None,
                None,
            ]
            .map(|name| name.map(str::to_owned)),
            spin: 0.2,
            powerslide_spin: 0.0,
            normal: vec![Suspension {
                four_wheels: true,
                roll_stiffness: 0.025,
                roll_damping: 0.3,
                lateral_acceleration: 0.33,
                pitch_stiffness: 0.05,
                pitch_damping: 0.8,
                axial_damping: 1.0,
                acceleration_pitch: 0.0,
                pitch_limits: [-0.035, 0.035],
                wheel_extension: 0.0,
                wheel_turn: 22.0_f32.to_radians(),
            }],
            sluggish: Vec::new(),
        };
        let mut motion = DrawMotion {
            velocity: [2.0, 0.0, 0.0],
            acceleration: [0.0; 2],
            normal: [0.0, -0.25, 0.968_245_86],
            altitude: 0.0,
            airborne: false,
            motive: true,
            turning: 1,
            backwards: false,
            locomotor_set: generals_sim::skirmish::locomotor_set::NORMAL,
            surface: 0,
        };
        let mut wheels = Wheels::new();
        wheels.update(&rules, &rules, &motion, 0.0, [12.0, 6.0]);
        let mut model = Model::null();
        model.bone_names.push("ROOT".into());
        model
            .bone_names
            .extend(rules.bones.iter().flatten().cloned());
        let mut controls = Vec::new();
        wheels.controls(&rules, &model, &mut controls);
        assert_eq!(controls.len(), 6);
        assert_eq!(
            controls[0].axes.map(|row| row.map(f32::to_bits)),
            controls[2].axes.map(|row| row.map(f32::to_bits))
        );
        assert_eq!(
            controls[1].translation.map(f32::to_bits),
            controls[3].translation.map(f32::to_bits)
        );
        assert!(controls[0].translation[2] > controls[1].translation[2]);
        assert!(controls[0].axes[1][0] < controls[4].axes[1][0]);
        let steer = wheels.steer.to_bits();
        let front = wheels.heights[0].to_bits();
        motion.airborne = true;
        motion.altitude = 2.0;
        motion.turning = -1;
        wheels.update(&rules, &rules, &motion, 0.0, [12.0, 6.0]);
        assert_eq!(wheels.steer.to_bits(), steer);
        assert_eq!(wheels.heights[0].to_bits(), front);
    }

    #[test]
    fn bike_wheel_spin_uses_the_previous_turn_and_front_steering_pose() {
        let suspension = Suspension {
            four_wheels: false,
            roll_stiffness: 0.0,
            roll_damping: 0.05,
            lateral_acceleration: 0.6,
            pitch_stiffness: 0.02,
            pitch_damping: 0.1,
            axial_damping: 0.9,
            acceleration_pitch: 0.4,
            pitch_limits: [-0.35, 0.09],
            wheel_extension: -1.8,
            wheel_turn: 0.017_453_292,
        };
        let rules = Rules {
            bones: [
                Some("Tire01".into()),
                None,
                None,
                None,
                Some("Tire00".into()),
                None,
                None,
                None,
                None,
                None,
            ],
            spin: 0.2,
            powerslide_spin: 2.5,
            normal: vec![suspension],
            sluggish: Vec::new(),
        };
        let motion = DrawMotion {
            velocity: [2.0, 0.0, 0.0],
            acceleration: [0.0; 2],
            normal: [0.0, 0.0, 1.0],
            altitude: 0.0,
            airborne: false,
            motive: true,
            turning: 1,
            backwards: false,
            locomotor_set: generals_sim::skirmish::locomotor_set::NORMAL,
            surface: 0,
        };
        let mut wheels = Wheels::new();
        wheels.update(&rules, &rules, &motion, 0.0, [7.0, 2.0]);
        assert_eq!(wheels.rotation.map(f32::to_bits), [0.4_f32.to_bits(); 2]);
        assert!(wheels.take_change());
        wheels.update(&rules, &rules, &motion, 0.0, [7.0, 2.0]);
        assert!((wheels.rotation[0] - 0.8).abs() < 0.000_001);
        assert!((wheels.rotation[1] - 1.3).abs() < 0.000_001);
        let mut model = Model::null();
        model.bone_names = vec!["ROOT".into(), "Tire01".into(), "Tire00".into()];
        let mut controls = Vec::new();
        wheels.controls(&rules, &model, &mut controls);
        assert_eq!(controls.len(), 2);
        assert!(controls[0].axes[1][0] < 0.0);
        assert_eq!(
            controls[1].axes[1].map(f32::to_bits),
            [-0.0_f32, 1.0, 0.0].map(f32::to_bits)
        );
        assert!(controls[1].axes[0][2] < controls[0].axes[0][2]);
    }
}
