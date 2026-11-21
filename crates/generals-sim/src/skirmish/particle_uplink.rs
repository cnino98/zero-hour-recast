//! Zero Hour `ParticleUplinkCannonUpdate`, with independent logic and draw radii.
use super::{DamageType, DeathType, Skirmish, object_sounds::Action};
use num_traits::ToPrimitive;

#[cfg(test)]
mod tests;

pub const ORBITAL_BEAM_Z_OFFSET: f32 = 3500.0;
pub const ORBITAL_BEAM_AUDIO_Z_OFFSET: f32 = 500.0;
const ABORT_TYPES: u32 = (1 << 10) | (1 << 6) | (1 << 2) | (1 << 1);
const PACKING: u128 = 1 << 73;
const UNPACKING: u128 = 1 << 74;
const DEPLOYED: u128 = 1 << 75;

pub struct Rules {
    pub power: String,
    pub charge_frames: u32,
    pub antenna_frames: u32,
    pub ready_delay: u32,
    pub firing_frames: u32,
    pub growth_frames: u32,
    pub travel_frames: u32,
    pub damage_per_second: f32,
    pub damage_pulses: u32,
    pub damage_type: DamageType,
    pub death_type: DeathType,
    pub damage_radius_scale: f32,
    pub scorch_marks: u32,
    pub scorch_radius_scale: f32,
    pub reveal_range: f32,
    pub orbital_radius: f32,
    pub launch_fx_interval: u32,
    pub ground_hit_fx: String,
    pub beam_launch_fx: String,
    pub swath_distance: f32,
    pub swath_amplitude: f32,
    pub driving_speed: f32,
    pub fast_driving_speed: f32,
    pub double_click_frames: u32,
    pub remnant: Option<String>,
    pub sounds: [String; 4],
    pub effects: Effects,
}

pub struct Effects {
    pub outer_bone: String,
    pub outer_bone_count: u32,
    pub connector_bone: String,
    pub fire_bone: String,
    pub outer_flares: [String; 3],
    pub connector_lasers: [String; 2],
    pub connector_flares: [String; 2],
    pub base_flares: [String; 3],
    pub orbital_laser: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Charging,
    Preparing,
    AlmostReady,
    Ready,
    Firing,
    Postfire,
    Packing,
}

#[derive(PartialEq, Eq)]
enum OrbitalBeam {
    Absent,
    Widening,
    Decaying(u32),
}

pub(in crate::skirmish) struct State {
    stage: Stage,
    stage_frame: u32,
    orbital: OrbitalBeam,
    start: u32,
    decay: u32,
    initial: [f32; 3],
    current: [f32; 3],
    destination: [f32; 3],
    manual: bool,
    clicks: [u32; 2],
    scorches: u32,
    next_scorch: u32,
    pulses: u32,
    next_pulse: u32,
    next_launch_fx: u32,
}

pub struct Visual {
    pub stage: Stage,
    pub elapsed: u32,
    pub target: Option<OrbitalVisual>,
}

pub struct OrbitalVisual {
    pub position: [f32; 3],
    pub width: f32,
    pub elapsed: u32,
}

pub struct Scorch {
    pub position: [f32; 3],
    pub radius: f32,
}

impl State {
    const fn new() -> Self {
        Self {
            stage: Stage::Idle,
            stage_frame: 0,
            orbital: OrbitalBeam::Absent,
            start: 0,
            decay: 0,
            initial: [0.0; 3],
            current: [0.0; 3],
            destination: [0.0; 3],
            manual: false,
            clicks: [0; 2],
            scorches: 0,
            next_scorch: 0,
            pulses: 0,
            next_pulse: 0,
            next_launch_fx: 0,
        }
    }

    fn width(&self, rules: &Rules, now: u32) -> Option<f32> {
        match self.orbital {
            OrbitalBeam::Absent => None,
            OrbitalBeam::Widening if rules.growth_frames == 0 => Some(1.0),
            OrbitalBeam::Widening => {
                let elapsed = now.wrapping_sub(self.start.wrapping_add(rules.travel_frames));
                Some((real(elapsed) / real(rules.growth_frames)).min(1.0))
            }
            OrbitalBeam::Decaying(start) => {
                Some((1.0 - real(now.wrapping_sub(start)) / real(rules.growth_frames)).max(0.0))
            }
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_particle_uplink(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.particle_uplinks.len(), ordinal);
        self.entities[id]
            .behaviors
            .particle_uplinks
            .push(State::new());
    }

    pub(in crate::skirmish) fn delete_particle_uplink(&mut self, id: usize, ordinal: usize) {
        self.set_uplink_stage(id, ordinal, Stage::Idle);
        let state = &mut self.entities[id].behaviors.particle_uplinks[ordinal];
        state.orbital = OrbitalBeam::Absent;
        state.start = 0;
    }

    pub(in crate::skirmish) fn initiate_particle_uplink(
        &mut self,
        id: usize,
        ordinal: usize,
        power: &str,
        target: [f32; 3],
        script: bool,
    ) -> bool {
        let now = self.update_frame();
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .particle_uplinks[ordinal];
        if rules.power != power {
            return false;
        }
        let state = &mut self.entities[id].behaviors.particle_uplinks[ordinal];
        state.start = if script { now.max(1) } else { now };
        state.decay = state.start.wrapping_add(rules.firing_frames);
        state.orbital = OrbitalBeam::Absent;
        state.manual = !script;
        state.initial = target;
        if script {
            self.set_uplink_stage(id, ordinal, Stage::Ready);
            self.set_special_power_ready_frame(id, power, now);
        } else {
            state.destination = target;
            state.current = target;
        }
        self.mark_special_power_triggered(id, power, Some(target));
        true
    }

    fn particle_uplink_ordinal(&self, id: usize, power: &str) -> Option<usize> {
        let object = self.entities.get(id).filter(|actor| actor.complete())?;
        self.catalog[object.type_index]
            .behavior
            .particle_uplinks
            .iter()
            .position(|rules| rules.power == power)
    }

    #[must_use]
    pub fn particle_uplink_destination_active(&self, id: usize, power: &str) -> bool {
        self.particle_uplink_ordinal(id, power)
            .is_some_and(|ordinal| self.particle_uplink_can_steer(id, ordinal))
    }

    pub(super) fn set_particle_uplink_destination(
        &mut self,
        id: usize,
        ordinal: usize,
        position: [f32; 3],
    ) {
        if self.object_disabled(id) {
            return;
        }
        let now = self.update_frame();
        let state = &mut self.entities[id].behaviors.particle_uplinks[ordinal];
        state.destination = position;
        state.manual = true;
        state.clicks = [state.clicks[1], now];
    }

    pub(super) fn particle_uplink_can_steer(&self, id: usize, ordinal: usize) -> bool {
        matches!(
            self.entities[id].behaviors.particle_uplinks[ordinal].stage,
            Stage::Firing | Stage::Postfire
        )
    }

    pub fn particle_uplink_visuals(&self, id: usize) -> impl Iterator<Item = Visual> + '_ {
        let object = &self.entities[id];
        let rules = &self.catalog[object.type_index].behavior.particle_uplinks;
        let now = self.update_frame();
        object
            .behaviors
            .particle_uplinks
            .iter()
            .zip(rules)
            .map(move |(state, rules)| Visual {
                stage: state.stage,
                elapsed: now.wrapping_sub(state.stage_frame),
                target: state.width(rules, now).map(|width| OrbitalVisual {
                    position: state.current,
                    width,
                    elapsed: now.wrapping_sub(state.start.wrapping_add(rules.travel_frames)),
                }),
            })
    }

    pub(in crate::skirmish) fn update_particle_uplink(&mut self, id: usize, ordinal: usize) -> i32 {
        let object = &self.entities[id];
        if object.selling() {
            if object.behaviors.particle_uplinks[ordinal].stage != Stage::Idle {
                self.delete_particle_uplink(id, ordinal);
            }
            return 1;
        }
        if object.under_construction() || object.is_effectively_dead() {
            return 1;
        }
        let now = self.update_frame();
        let kind = object.type_index;
        let state = &object.behaviors.particle_uplinks[ordinal];
        if state.start != 0 && state.start <= now {
            self.update_particle_attack(id, ordinal);
        } else {
            let power = self.catalog[kind].behavior.particle_uplinks[ordinal]
                .power
                .clone();
            let ready = if self.special_power_ready(id, &power) {
                now
            } else {
                self.special_power_ready_frame(id, &power)
                    .expect("bound Particle Uplink power")
            };
            let rules = &self.catalog[kind].behavior.particle_uplinks[ordinal];
            let almost = ready.wrapping_sub(rules.ready_delay);
            let raise = almost.wrapping_sub(rules.antenna_frames);
            let charge = raise.wrapping_sub(rules.charge_frames);
            let previous = self.entities[id].behaviors.particle_uplinks[ordinal].stage;
            let next = if ready <= now {
                Stage::Ready
            } else if almost <= now {
                Stage::AlmostReady
            } else if raise <= now {
                Stage::Preparing
            } else if charge <= now {
                Stage::Charging
            } else if previous == Stage::Ready {
                Stage::Packing
            } else {
                previous
            };
            self.set_uplink_stage(id, ordinal, next);
        }

        let state = &mut self.entities[id].behaviors.particle_uplinks[ordinal];
        if state.stage == Stage::Firing && state.next_launch_fx <= now {
            let rules = &self.catalog[kind].behavior.particle_uplinks[ordinal];
            state.next_launch_fx = now.wrapping_add(rules.launch_fx_interval);
            let fx = rules.beam_launch_fx.clone();
            let position = self.uplink_bone_position(id, &rules.effects.fire_bone);
            if !fx.is_empty() {
                self.positional_fx_with_radius(id, fx, position, 0.0);
            }
        }
        1
    }

    fn update_particle_attack(&mut self, id: usize, ordinal: usize) {
        let now = self.update_frame();
        let disabled = self.object_disabled_mask(id) & ABORT_TYPES != 0;
        let kind = self.entities[id].type_index;
        let rules = &self.catalog[kind].behavior.particle_uplinks[ordinal];
        let state = &mut self.entities[id].behaviors.particle_uplinks[ordinal];
        if state.decay > now && disabled {
            state.decay = now;
        }
        let birth = state.start.wrapping_add(rules.travel_frames);
        let decay = state.decay.wrapping_add(rules.travel_frames);
        let end = decay.wrapping_add(rules.growth_frames);
        let mut sound = None;
        match state.orbital {
            OrbitalBeam::Absent if birth <= now => {
                state.orbital = OrbitalBeam::Widening;
                state.scorches = 0;
                state.next_scorch = now;
                state.pulses = 0;
                state.next_pulse = now;
                sound = Some(Action::PlayOrbitalLoop(ordinal));
            }
            OrbitalBeam::Widening if decay <= now => {
                if rules.growth_frames != 0 {
                    state.orbital = OrbitalBeam::Decaying(now);
                }
            }
            OrbitalBeam::Decaying(_) if end <= now => {
                state.orbital = OrbitalBeam::Absent;
                state.start = 0;
                sound = Some(Action::StopLoop(ordinal * 4 + 3));
            }
            _ => {}
        }
        if rules.growth_frames == 0 && state.start != 0 && end <= now {
            state.orbital = OrbitalBeam::Absent;
            state.start = 0;
            sound = Some(Action::StopLoop(ordinal * 4 + 3));
        }
        if let Some(action) = sound {
            let mut position = state.initial;
            position[2] += ORBITAL_BEAM_AUDIO_Z_OFFSET;
            let sound = rules.sounds[3].clone();
            self.sound_at(id, sound, position, action);
        }
        if birth <= now && now < end {
            self.drive_particle_attack(id, ordinal, birth, end);
        }
        let state = &self.entities[id].behaviors.particle_uplinks[ordinal];
        let next = if state.start == 0 {
            Stage::Idle
        } else if state
            .decay
            .wrapping_add(self.catalog[kind].behavior.particle_uplinks[ordinal].growth_frames)
            <= now
        {
            Stage::Packing
        } else if state.decay <= now {
            Stage::Postfire
        } else {
            Stage::Firing
        };
        self.set_uplink_stage(id, ordinal, next);
    }

    fn drive_particle_attack(&mut self, id: usize, ordinal: usize, birth: u32, end: u32) {
        let now = self.update_frame();
        let source = self.object_position(id);
        let kind = self.entities[id].type_index;
        let rules = &self.catalog[kind].behavior.particle_uplinks[ordinal];
        let state = &self.entities[id].behaviors.particle_uplinks[ordinal];
        let mut position = if state.manual {
            let fast = state.clicks[1].wrapping_sub(state.clicks[0]) < rules.double_click_frames;
            let per_frame = if fast {
                rules.fast_driving_speed
            } else {
                rules.driving_speed
            } / 30.0;
            drive_toward(state.current, state.destination, per_frame)
        } else {
            let factor = real(now.wrapping_sub(birth)) / real(end.wrapping_sub(birth));
            swath_position(
                source,
                state.initial,
                factor,
                rules.swath_distance,
                rules.swath_amplitude,
            )
        };
        let xy = [position[0], position[1]];
        position[2] = self.ground_height(xy);
        let state = &mut self.entities[id].behaviors.particle_uplinks[ordinal];
        state.current = position;
        let logical_radius = rules.orbital_radius * state.width(rules, now).unwrap_or(0.0);
        let scorch = state.next_scorch <= now;
        let damage = state.next_pulse <= now;
        if scorch {
            state.scorches = state.scorches.wrapping_add(1);
            state.next_scorch = spread_frame(birth, end, state.scorches, rules.scorch_marks);
            let reveal = rules.reveal_range;
            let radius = logical_radius * rules.scorch_radius_scale;
            let fx = rules.ground_hit_fx.clone();
            self.presentation
                .weapon_fx
                .push(super::WeaponFxEvent::ParticleUplinkScorch(
                    self.presentation.particle_uplink_scorches.len(),
                ));
            self.presentation
                .particle_uplink_scorches
                .push(Scorch { position, radius });
            if !fx.is_empty() {
                self.positional_fx_with_radius(id, fx, position, 0.0);
            }
            self.particle_uplink_reveal(self.entities[id].owner, xy, reveal);
        }
        if damage {
            let rules = &self.catalog[kind].behavior.particle_uplinks[ordinal];
            let amount = real(rules.firing_frames / 30) * rules.damage_per_second
                / real(rules.damage_pulses);
            let damage_type = rules.damage_type;
            let death_type = rules.death_type;
            let radius = logical_radius * rules.damage_radius_scale;
            let remnant = rules
                .remnant
                .as_ref()
                .and_then(|name| self.catalog.iter().position(|unit| unit.name == *name));
            let state = &mut self.entities[id].behaviors.particle_uplinks[ordinal];
            state.pulses = state.pulses.wrapping_add(1);
            state.next_pulse = spread_frame(birth, end, state.pulses, rules.damage_pulses);
            for target in self.partition.fastest_range_objects(xy, radius) {
                let actor = &self.entities[target];
                let x = actor.position[0] - xy[0];
                let y = actor.position[1] - xy[1];
                if !actor.is_effectively_dead() && (x * x + y * y) < radius * radius {
                    self.apply_damage(target, Some(id), amount, damage_type, death_type);
                }
            }
            if let Some(kind) = remnant {
                let remnant = self.new_object_on_team(
                    kind,
                    self.entities[id].owner,
                    self.team_membership.team_of(id),
                );
                self.set_object_position(remnant, position);
            }
        }
    }

    fn set_uplink_stage(&mut self, id: usize, ordinal: usize, next: Stage) {
        let now = self.update_frame();
        let object = &mut self.entities[id];
        let state = &mut object.behaviors.particle_uplinks[ordinal];
        if state.stage == next {
            return;
        }
        state.stage = next;
        state.stage_frame = now;
        let (conditions, reset_beam, start, stop) = match next {
            Stage::Idle => (Some(0), false, None, [true; 4]),
            Stage::Charging => (None, true, Some(0), [false, true, true, true]),
            Stage::Preparing => (Some(UNPACKING), true, Some(1), [false, false, true, true]),
            Stage::AlmostReady => (Some(DEPLOYED), true, None, [false; 4]),
            Stage::Ready => (Some(DEPLOYED), true, None, [true, false, true, true]),
            Stage::Firing => (Some(DEPLOYED), false, Some(2), [true, true, false, true]),
            Stage::Postfire => (None, false, None, [false; 4]),
            Stage::Packing => (Some(PACKING), false, None, [false; 4]),
        };
        if let Some(conditions) = conditions {
            let flags = &mut object.drawable.model_conditions;
            flags.set_flag(PACKING | UNPACKING | DEPLOYED, false);
            flags.set_flag(conditions, true);
        }
        if reset_beam {
            state.orbital = OrbitalBeam::Absent;
        }
        if next == Stage::Firing {
            state.next_launch_fx = 0;
        }
        let sounds = self.catalog[object.type_index].behavior.particle_uplinks[ordinal]
            .sounds
            .clone();
        if let Some(index) = start {
            self.object_sound(
                id,
                sounds[index].clone(),
                Action::PlayLoop(ordinal * 4 + index),
            );
        }
        for (index, (name, stop)) in sounds.into_iter().zip(stop).enumerate() {
            if stop {
                self.object_sound(id, name, Action::StopLoop(ordinal * 4 + index));
            }
        }
    }

    fn uplink_bone_position(&self, id: usize, bone: &str) -> [f32; 3] {
        let (position, matrix) = self.object_transform(id);
        self.logical_bone_position(id, bone)
            .map_or(position, |local| {
                std::array::from_fn(|row| {
                    let x = matrix[row][0] * local[0];
                    let y = matrix[row][1] * local[1];
                    let z = matrix[row][2] * local[2];
                    x + y + z + position[row]
                })
            })
    }
}

fn drive_toward(current: [f32; 3], destination: [f32; 3], speed: f32) -> [f32; 3] {
    let vector = std::array::from_fn::<_, 3, _>(|axis| destination[axis] - current[axis]);
    let distance = length(vector);
    let direction = if distance > 0.0 {
        vector.map(|value| value / distance)
    } else {
        vector
    };
    let speed = speed.min(distance);
    let step_x = direction[0] * speed;
    let step_y = direction[1] * speed;
    [current[0] + step_x, current[1] + step_y, current[2]]
}

fn swath_position(
    source: [f32; 3],
    target: [f32; 3],
    factor: f32,
    distance: f32,
    amplitude: f32,
) -> [f32; 3] {
    let heading = std::array::from_fn::<_, 3, _>(|axis| target[axis] - source[axis]);
    let travelled = factor * distance;
    let half_distance = distance * 0.5;
    let distance_offset = travelled - half_distance;
    let swath_angle = factor * std::f32::consts::TAU;
    let radians = swath_angle - std::f32::consts::PI;
    let x = distance_offset + length(heading);
    let y = radians.sin() * amplitude;
    let range = length([x, y, 0.0]);
    let heading = normalize_xy(heading[0], heading[1]);
    let cartesian = normalize_xy(x, y);
    let dot_x = heading[0] * cartesian[0];
    let dot_y = heading[1] * cartesian[1];
    let dot = (dot_x + dot_y).clamp(-0.99999, 0.99999);
    let angle = dot.acos();
    let angle = if heading[1] >= 0.0 { angle } else { -angle };
    let offset_x = range * angle.cos();
    let offset_y = range * angle.sin();
    [source[0] + offset_x, source[1] + offset_y, 0.0]
}

fn normalize_xy(x: f32, y: f32) -> [f32; 2] {
    let x_squared = x * x;
    let y_squared = y * y;
    let distance = (x_squared + y_squared).sqrt();
    let inverse = if distance == 0.0 { 0.0 } else { 1.0 / distance };
    [x * inverse, y * inverse]
}

fn length(v: [f32; 3]) -> f32 {
    let [x, y, z] = v.map(|component| component * component);
    (x + y + z).sqrt()
}

fn spread_frame(start: u32, end: u32, count: u32, total: u32) -> u32 {
    let fraction = real(count) / real(total);
    let elapsed = fraction * real(end.wrapping_sub(start));
    (real(start) + elapsed)
        .to_u32()
        .expect("native Particle Uplink pulse frame")
}

fn real(value: u32) -> f32 {
    value.to_f32().expect("native unsigned frame")
}
