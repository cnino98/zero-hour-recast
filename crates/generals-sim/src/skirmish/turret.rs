//! Primary `TurretAI` aim, hold, idle scan and recenter states (SH 2bfaea60).
use super::{LaunchTransform, Skirmish, combat::relative_angle};
use std::f32::consts::{PI, TAU};

mod constructor;
mod machine;
mod opportunity;

pub struct Rules {
    pub controlled_slots: u8,
    pub turn_rate: f32,
    pub pitch_rate: f32,
    pub natural_angle: f32,
    pub natural_pitch: f32,
    pub allows_pitch: bool,
    pub fire_pitch: f32,
    pub minimum_pitch: f32,
    pub ground_pitch: f32,
    pub fire_sweep: [f32; 3],
    pub sweep_speed: [f32; 3],
    pub recenter_frames: u32,
    pub initially_disabled: bool,
    pub idle_angles: [f32; 2],
    pub idle_intervals: [u32; 2],
    pub mounts: [Mount; 3],
}

pub struct Mount {
    pub yaw: [f32; 3],
    pub pitch: [f32; 3],
}

impl Mount {
    pub const ORIGIN: Self = Self {
        yaw: [0.0; 3],
        pitch: [0.0; 3],
    };
}

impl Rules {
    #[must_use]
    pub const fn source_defaults() -> Self {
        Self {
            controlled_slots: 0,
            turn_rate: 0.01,
            pitch_rate: 0.01,
            natural_angle: 0.0,
            natural_pitch: 0.0,
            allows_pitch: false,
            fire_pitch: 0.0,
            minimum_pitch: 0.0,
            ground_pitch: 0.0,
            fire_sweep: [0.0; 3],
            sweep_speed: [1.0; 3],
            recenter_frames: 60,
            initially_disabled: false,
            idle_angles: [0.0; 2],
            idle_intervals: [9_999_999; 2],
            mounts: [Mount::ORIGIN, Mount::ORIGIN, Mount::ORIGIN],
        }
    }
}

enum Phase {
    Idle(Option<u64>),
    Scan(f32),
    Aim,
    Fire,
    Hold(u64),
    Recenter,
}

pub struct State {
    pub(super) enabled: bool,
    pub angle: f32,
    pub pitch: f32,
    pub rotating: bool,
    phase: Phase,
    opportunity: Option<(usize, usize)>,
    target: Option<crate::skirmish::combat::attack::Goal>,
    target_team: Option<usize>,
    forced: bool,
    sleep_until: u32,
    updated_at: Option<u64>,
    sweep: Sweep,
}

struct Sweep {
    positive: bool,
    until: u64,
}

impl State {
    pub(super) fn new(rules: Option<&Rules>) -> Self {
        Self {
            enabled: rules.is_none_or(|rules| !rules.initially_disabled),
            angle: rules.map_or(0.0, |rules| rules.natural_angle),
            pitch: rules.map_or(0.0, |rules| rules.natural_pitch),
            rotating: false,
            phase: Phase::Idle(None),
            opportunity: None,
            target: None,
            target_team: None,
            forced: false,
            sleep_until: 0,
            updated_at: None,
            sweep: Sweep {
                positive: true,
                until: 0,
            },
        }
    }

    pub(super) const fn recenter(&mut self) {
        self.phase = Phase::Recenter;
        self.opportunity = None;
        self.target = None;
        self.sleep_until = 0;
    }

    pub(super) const fn fired(&mut self, frame: u64) {
        self.sweep.until = frame + 3;
    }

    pub(super) fn natural(&self, rules: &Rules) -> bool {
        (self.angle - rules.natural_angle).abs() == 0.0
            && (self.pitch - rules.natural_pitch).abs() == 0.0
    }

    fn turn(&mut self, desired: f32, rate: f32, threshold: f32) -> bool {
        let desired = normalize(desired);
        let delta = normalize(desired - self.angle);
        self.rotating = delta.abs() >= rate;
        self.angle = normalize(if delta.abs() < rate {
            desired
        } else if delta > 0.0 {
            self.angle + rate
        } else {
            self.angle - rate
        });
        // The original deliberately compares the resulting absolute angles.
        (self.angle - desired).abs() <= threshold
    }

    fn aim_yaw(&mut self, relative: f32, rules: &Rules, slot: usize, frame: u64) -> bool {
        let sweep = rules.fire_sweep[slot];
        let mut desired = relative;
        let modifier = if sweep > 0.0 && self.sweep.until != 0 && self.sweep.until > frame {
            if self.sweep.positive {
                desired += sweep;
            } else {
                desired -= sweep;
            }
            rules.sweep_speed[slot]
        } else {
            1.0
        };
        let mut aligned = self.turn(desired, rules.turn_rate * modifier, 0.035);
        // The source admits firing inside the sweep even before the first shot.
        if sweep > 0.0 {
            if aligned {
                self.sweep.positive = !self.sweep.positive;
            }
            aligned = normalize(relative - self.angle).abs() < sweep;
        }
        aligned
    }

    fn pitch(&mut self, desired: f32, rules: &Rules, modifier: f32) -> bool {
        if !rules.allows_pitch {
            return true;
        }
        let desired = normalize(desired);
        let delta = normalize(desired - self.pitch);
        let rate = rules.pitch_rate * modifier;
        self.pitch = normalize(if delta.abs() < rate {
            desired
        } else if delta > 0.0 {
            self.pitch + rate
        } else {
            self.pitch - rate
        });
        self.pitch.to_bits() == desired.to_bits() || self.pitch == 0.0 && desired == 0.0
    }
}

fn normalize(mut angle: f32) -> f32 {
    if angle.is_nan() {
        return 0.0;
    }
    loop {
        if angle <= PI {
            break;
        }
        angle -= TAU;
    }
    loop {
        if angle > -PI {
            break;
        }
        angle += TAU;
    }
    angle
}

impl Skirmish {
    fn rotate_native_turret(&mut self, id: usize, which: usize, position: [f32; 3]) -> bool {
        let extra = self.passenger_bonus_flags(id);
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = if which == 0 {
            unit.turret.as_ref()
        } else {
            unit.secondary_turret.as_ref()
        }
        .expect("allocated turret");
        if !self.turret_state(id, which).enabled {
            return false;
        }
        let angle = relative_angle(actor.position, actor.facing, [position[0], position[1]]);
        let vector = [
            position[0] - actor.position[0],
            position[1] - actor.position[1],
            position[2] - self.object_position(id)[2] - unit.geometry_bounds.max_height / 2.0,
        ];
        let x_squared = vector[0] * vector[0];
        let y_squared = vector[1] * vector[1];
        let z_squared = vector[2] * vector[2];
        let distance = (x_squared + y_squared + z_squared).sqrt();
        let actual = if distance > 0.0 {
            (vector[2] / distance).asin()
        } else {
            0.0
        };
        // TurretAIAimTurretState clears its enemy pointer before this branch;
        // GroundUnitPitch consequently applies to every represented object target.
        let pitch = if rules.fire_pitch > 0.0 {
            rules.fire_pitch
        } else if rules.ground_pitch > 0.0 {
            let ground_pitch = rules.ground_pitch
                * (distance / self.entities[id].weapon_range(unit, extra).max(1.0));
            (actual + ground_pitch).max(rules.minimum_pitch)
        } else {
            actual.max(rules.minimum_pitch)
        };
        let slot = actor.weapon_slot();
        let actor = &mut self.entities[id];
        let turret = if which == 0 {
            &mut actor.ai.turret
        } else {
            actor
                .ai
                .secondary_turret
                .as_mut()
                .expect("allocated turret")
        };
        turret.updated_at = Some(self.tick);
        turret.phase = Phase::Aim;
        let old_angle = turret.angle;
        let yaw_ready = turret.aim_yaw(angle, rules, slot, self.tick);
        if turret.angle.partial_cmp(&old_angle) != Some(std::cmp::Ordering::Equal) {
            self.containment_transform_changed(id);
        }
        let unit = &self.catalog[self.entities[id].type_index];
        let rules = if which == 0 {
            unit.turret.as_ref()
        } else {
            unit.secondary_turret.as_ref()
        }
        .expect("allocated turret");
        let ai = &mut self.entities[id].ai;
        let turret = if which == 0 {
            &mut ai.turret
        } else {
            ai.secondary_turret.as_mut().expect("allocated turret")
        };
        let pitch_ready = turret.pitch(pitch, rules, 1.0);
        yaw_ready && pitch_ready
    }

    pub(super) fn update_idle_turret(&mut self, id: usize) {
        for which in 0..2 {
            if self.has_native_turret(id, which) {
                self.update_native_turret(id, which);
            }
        }
    }
}

pub(super) fn launch(local: &mut LaunchTransform, mount: &Mount, state: &State) {
    // Weapon::calcProjectileLaunchPosition: yaw * pitch * pristine attachment.
    let (sine, cosine) = (-state.pitch).sin_cos();
    let pitch = |point: [f32; 3]| {
        let forward_x = cosine * point[0];
        let elevation_x = sine * point[2];
        let forward_z = -sine * point[0];
        let elevation_z = cosine * point[2];
        [forward_x + elevation_x, point[1], forward_z + elevation_z]
    };
    let (sine, cosine) = state.angle.sin_cos();
    let yaw = |point: [f32; 3]| {
        let forward_x = cosine * point[0];
        let side_x = sine * point[1];
        let forward_y = sine * point[0];
        let side_y = cosine * point[1];
        [forward_x - side_x, forward_y + side_y, point[2]]
    };
    let relative = std::array::from_fn(|axis| local.position[axis] - mount.pitch[axis]);
    let pitched = pitch(relative);
    let relative = std::array::from_fn(|axis| pitched[axis] + mount.pitch[axis] - mount.yaw[axis]);
    let turned = yaw(relative);
    local.position = std::array::from_fn(|axis| turned[axis] + mount.yaw[axis]);
    local.axes = local.axes.map(|axis| yaw(pitch(axis)));
}

#[cfg(test)]
mod tests;
