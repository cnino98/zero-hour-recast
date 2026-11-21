//! Native vehicle drone creation and `SlavedUpdate`, SH 2bfaea60.
use super::{Entity, Locomotor, Skirmish, progression, startup};

mod repair;
mod spawn;
pub(super) use spawn::validate;
#[cfg(test)]
mod tests;

pub struct Upgrade {
    pub trigger: String,
    pub conflicts: Vec<String>,
    pub object: String,
    pub offset: [f32; 3],
}

pub(in crate::skirmish) enum SlavedInterface {
    Ordinary(usize),
    Mob(usize),
}

pub struct Rules {
    pub guard_range: f32,
    pub guard_wander: f32,
    pub attack_range: f32,
    pub attack_wander: f32,
    pub scout_range: f32,
    pub scout_wander: f32,
    pub spotting_range: f32,
    pub repair: Option<Repair>,
    pub panic: Option<Locomotor>,
    pub panic_speed: f32,
    pub stay_on_master_layer: bool,
}

pub struct Repair {
    pub range: f32,
    pub altitude: [f32; 2],
    pub per_second: f32,
    pub below_percent: i32,
    pub ready_frames: [u32; 2],
    pub weld_frames: [u32; 2],
    pub welding_system: String,
    pub welding_bone: String,
}

enum RepairState {
    None,
    Unpacking,
    Ready,
    Extending,
    Welding,
    Retracting,
}

pub struct WeldEvent {
    pub object: usize,
    pub type_index: usize,
    pub ordinal: usize,
    pub weld: Weld,
}

pub struct Weld {
    pub frame: u64,
    pub lifetime: f32,
    pub position: [f32; 3],
    pub health_ratio: f32,
}

pub(super) struct State {
    parent: Option<usize>,
    offset: [f32; 3],
    wait: i32,
    repair: RepairState,
    repairing: bool,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            parent: None,
            offset: [0.0; 3],
            wait: 0,
            repair: RepairState::None,
            repairing: false,
        }
    }
}

impl Entity {
    pub(in crate::skirmish) fn drone_producer(&self) -> Option<usize> {
        (!self.behaviors.slaves.is_empty())
            .then_some(self.producer)
            .flatten()
    }

    #[must_use]
    pub fn drone_parent(&self) -> Option<usize> {
        self.behaviors.slaves.first().and_then(|state| state.parent)
    }

    #[must_use]
    pub const fn drone_condition(&self) -> Option<&'static str> {
        self.drawable.slave_condition
    }
}

impl Skirmish {
    /// Locate the current drone through its native producer identity.
    #[must_use]
    pub fn produced_drone(&self, producer: usize) -> Option<usize> {
        self.entities
            .iter()
            .rposition(|actor| actor.alive() && actor.drone_producer() == Some(producer))
    }

    pub(in crate::skirmish) fn construct_slaved(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.slaves.len(), ordinal);
        self.entities[id].behaviors.slaves.push(State::new());
    }

    pub(in crate::skirmish) fn created_slaved(&mut self, id: usize, ordinal: usize) {
        if self.entities[id].has_drawable()
            && self.catalog[self.entities[id].type_index].behavior.slaves[ordinal]
                .repair
                .as_ref()
                .is_some_and(|rules| rules.per_second > 0.0)
        {
            self.entities[id].drawable.slave_condition = Some("PACKING");
        }
    }

    pub(in crate::skirmish) fn update_drone(&mut self, id: usize, ordinal: usize) -> i32 {
        let Some(parent) = self.slaved_master(id, ordinal) else {
            return 1;
        };
        if self.catalog[self.entities[id].type_index].behavior.slaves[ordinal].stay_on_master_layer
        {
            self.entities[id].layer = self.entities[parent].layer;
        }
        self.set_drone_spotting(parent, false);
        let rules = &self.catalog[self.entities[id].type_index].behavior.slaves[ordinal];
        let health = if rules
            .repair
            .as_ref()
            .is_some_and(|rules| rules.per_second > 0.0)
        {
            let parent = &self.entities[parent];
            num_traits::ToPrimitive::to_i32(&(parent.health / parent.max_health * 100.0).trunc())
                .expect("native SlavedUpdate health percentage")
        } else {
            100
        };
        if rules
            .repair
            .as_ref()
            .is_some_and(|repair| health <= repair.below_percent)
        {
            self.repair_drone(id, ordinal, parent);
            return 1;
        }
        if rules.attack_range != 0.0 {
            let target = self.entities[parent].ai.current_victim;
            if let Some(target) = target.filter(|&target| self.entities[target].exists()) {
                let position = self.object_position(target);
                let spotting_range = rules.spotting_range;
                self.end_drone_repair(id, ordinal);
                let distance = self.drone_point_distance(id, [position[0], position[1]]);
                self.drone_follow(id, ordinal, parent, position, true);
                if distance < spotting_range * spotting_range {
                    self.set_drone_spotting(parent, true);
                }
                return 1;
            }
        }
        let rules = &self.catalog[self.entities[id].type_index].behavior.slaves[ordinal];
        if rules.scout_range != 0.0
            && let Some(position) = self.entities[parent]
                .ai
                .ground_path
                .as_ref()
                .and_then(super::path_following::Path::last_position)
        {
            let radius = rules.guard_range * 0.5;
            if self.drone_point_distance(parent, [position[0], position[1]]) > radius * radius {
                self.end_drone_repair(id, ordinal);
                self.drone_follow(id, ordinal, parent, position, false);
                return 1;
            }
        }
        if health < 100 {
            self.repair_drone(id, ordinal, parent);
        } else {
            self.drone_guard(id, ordinal, parent);
        }
        1
    }

    fn slaved_master(&mut self, id: usize, ordinal: usize) -> Option<usize> {
        let state = &mut self.entities[id].behaviors.slaves[ordinal];
        if state.wait > 0 {
            state.wait -= 1;
        }
        if matches!(state.repair, RepairState::None) {
            if state.wait > 0 {
                return None;
            }
            state.wait = 30 / 4;
        }
        let parent = state.parent?;
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if actor.ai.registration.is_none()
            || unit
                .locomotor_in_set(actor.ai.locomotor_set, actor.ai.surface_locomotor)
                .is_none()
        {
            return None;
        }
        if !self.entities[parent].exists()
            || self.entities[parent].is_effectively_dead()
            || self.entities[parent].unmanned()
        {
            self.stop_slaved_effects(id, ordinal);
            self.set_unmanned(id);
            self.ai_idle_from_ai(id);
            return None;
        }
        let allied = self.map_team_relationship(parent, id).map_or_else(
            || self.allied(self.entities[parent].owner, self.entities[id].owner),
            |relation| relation == 2,
        );
        if !allied {
            let team = self
                .team_membership
                .team_of(parent)
                .expect("native slaver Team");
            self.defect_to_team(id, team, 0);
        }
        Some(parent)
    }

    fn set_slave_model_condition(&mut self, id: usize, condition: &'static str) {
        if self.entities[id].has_drawable() {
            let actor = &mut self.entities[id];
            for slot in [1, 2] {
                if actor.weapon_set.status.condition[slot]
                    != Some(["PREATTACK_A", "PREATTACK_B", "PREATTACK_C"][slot])
                {
                    actor.weapon_set.status.condition[slot] = None;
                }
            }
            actor.drawable.slave_condition = Some(condition);
        }
    }

    fn slave_random_frames(&mut self, frames: [u32; 2]) -> i32 {
        let [low, high] = frames.map(|frame| i32::from_ne_bytes(frame.to_ne_bytes()));
        if low >= high {
            return high;
        }
        let width =
            u32::try_from(i64::from(high) - i64::from(low)).expect("native signed RNG width");
        let offset = self.random_between(width);
        i32::try_from(i64::from(low) + i64::from(offset)).expect("native signed RNG result")
    }

    fn set_drone_spotting(&mut self, parent: usize, value: bool) {
        let extra = self.passenger_bonus_flags(parent);
        let actor = &mut self.entities[parent];
        if actor.firing.drone_spotting != value {
            actor.firing.drone_spotting = value;
            actor.weapon_bonus_changed(
                &self.catalog[actor.type_index],
                self.tick,
                extra,
                &mut self.random,
            );
        }
    }

    fn drone_follow(
        &mut self,
        id: usize,
        ordinal: usize,
        parent: usize,
        target: [f32; 3],
        attack: bool,
    ) {
        let rules = &self.catalog[self.entities[id].type_index].behavior.slaves[ordinal];
        let (range, wander) = if attack {
            (rules.attack_range, rules.attack_wander)
        } else {
            (rules.scout_range, rules.scout_wander)
        };
        let mut position = target;
        if self.drone_point_distance(id, [target[0], target[1]]) > range * range {
            let origin = self.object_position(parent);
            let vector = [
                target[0] - origin[0],
                target[1] - origin[1],
                target[2] - origin[2],
            ];
            let squares = vector.map(|component| component * component);
            let length = (squares[0] + squares[1] + squares[2]).sqrt();
            for axis in 0..3 {
                let direction = if length == 0.0 {
                    vector[axis]
                } else {
                    vector[axis] / length
                };
                let displacement = direction * range;
                position[axis] = origin[axis] + displacement;
            }
        }
        if wander != 0.0 {
            let offset = self.drone_random_offset(wander);
            position[0] += offset[0];
            position[1] += offset[1];
            self.entities[id]
                .behaviors
                .slaves
                .get_mut(ordinal)
                .expect("SlavedUpdate instance")
                .offset = [
                offset[0],
                offset[1],
                self.ground_height([position[0], position[1]]),
            ];
        }
        self.construction_move(id, position);
    }

    fn drone_guard(&mut self, id: usize, ordinal: usize, parent: usize) {
        let rules = &self.catalog[self.entities[id].type_index].behavior.slaves[ordinal];
        if rules.guard_range == 0.0 {
            return;
        }
        let range = rules.guard_range;
        let wander = rules.guard_wander;
        let origin = self.object_position(parent);
        let offset = self.entities[id]
            .behaviors
            .slaves
            .get(ordinal)
            .expect("SlavedUpdate instance")
            .offset;
        let mut pinned = [origin[0] + offset[0], origin[1] + offset[1], origin[2]];
        self.entities[id]
            .behaviors
            .slaves
            .get_mut(ordinal)
            .expect("SlavedUpdate instance")
            .offset[2] = self.ground_height([pinned[0], pinned[1]]);
        let position = self.object_position(id);
        let far_idle =
            self.entities[id].ai_idle() && squared_distance(position, pinned) > 15.0 * 15.0;
        let stray = range * 2.0;
        if !far_idle && squared_distance(position, origin) <= stray * stray {
            return;
        }
        self.end_drone_repair(id, ordinal);
        if wander != 0.0 {
            // Source tests GuardWanderRange but uses GuardMaxRange for this offset.
            let offset = self.drone_random_offset(range);
            pinned[0] += offset[0];
            pinned[1] += offset[1];
            self.entities[id]
                .behaviors
                .slaves
                .get_mut(ordinal)
                .expect("SlavedUpdate instance")
                .offset = [
                offset[0],
                offset[1],
                self.ground_height([pinned[0], pinned[1]]),
            ];
        }
        self.construction_move(id, pinned);
    }

    fn drone_point_distance(&self, id: usize, position: [f32; 2]) -> f32 {
        let actor = &self.entities[id];
        let dx = position[0] - actor.position[0];
        let dy = position[1] - actor.position[1];
        let squares = [dx * dx, dy * dy];
        let square = squares[0] + squares[1];
        let distance = (square.sqrt() - self.object_radius(id)).max(0.0);
        distance * distance
    }

    fn drone_random_offset(&mut self, radius: f32) -> [f32; 2] {
        // Native GameLogicRandomValue takes Int bounds: 2*PI truncates to 6.
        let angle = progression::integer_real(self.random_between(6));
        [radius * angle.cos(), radius * angle.sin()]
    }
}

fn squared_distance(left: [f32; 3], right: [f32; 3]) -> f32 {
    let dx = right[0] - left[0];
    let dy = right[1] - left[1];
    let dz = right[2] - left[2];
    let squares = [dx * dx, dy * dy, dz * dz];
    squares[0] + squares[1] + squares[2]
}
