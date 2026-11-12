//! Native `DockUpdate` reservations and NORMAL-phase clearance callback.
use crate::skirmish::{ModelConditions, Skirmish, docking_flags};

pub(in crate::skirmish) struct State {
    open: bool,
    crippled: bool,
    inside: bool,
    positions: Option<Positions>,
    owners: Vec<Option<usize>>,
    reached: Vec<bool>,
    active: Option<usize>,
}

// DockUpdate loads pristine bones once, on the first call with a Drawable.
struct Positions {
    enter: [f32; 3],
    action: [f32; 3],
    exit: [f32; 3],
    approaches: Vec<[f32; 3]>,
    approach_bones: usize,
}

pub(super) enum Position {
    Enter,
    Action,
    Exit,
}

pub(super) enum Notification {
    Approach,
    Enter,
    Action,
    Exit,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_dock_module(&mut self, id: usize) {
        let count = self.catalog[self.entities[id].type_index]
            .supply
            .dock
            .as_ref()
            .expect("Dock module data")
            .number_approaches;
        let count = if count == -1 {
            10
        } else {
            usize::try_from(count).expect("Dock approach count")
        };
        assert!(
            self.entities[id].dock.is_none(),
            "one native Dock interface per object"
        );
        self.entities[id].dock = Some(State {
            open: true,
            crippled: false,
            inside: false,
            positions: None,
            owners: vec![None; count],
            reached: vec![false; count],
            active: None,
        });
    }

    pub(in crate::skirmish) fn update_dock_module(&mut self, id: usize) -> i32 {
        let state = self.entities[id].dock.as_mut().expect("Dock constructor");
        if state.active.is_none() && !state.crippled {
            for (owner, reached) in state.owners.iter().zip(&state.reached) {
                if *reached {
                    state.active = *owner;
                    return 1;
                }
            }
        } else if let Some(docker) = state.active
            && self.entities[docker].exists()
            && self.attack_kind(id, "SUPPLY_SOURCE")
            && self.attack_kind(docker, "DOZER")
            && self.attack_kind(docker, "HARVESTER")
            && self.entities[docker].drawable.present
            && self.entities[docker].docking_flags & docking_flags::BEGINNING != 0
        {
            self.entities[docker]
                .drawable
                .model_conditions
                .set_flag(ModelConditions::MOVING, false);
        }
        1
    }

    pub(in crate::skirmish) fn native_dock_set_crippled(&mut self, id: usize, setting: bool) {
        let state = self.entities[id].dock.as_ref().expect("Dock constructor");
        let active = state.active;
        let inside = state.inside;
        if setting
            && self.catalog[self.entities[id].type_index]
                .supply
                .warehouse
                .is_some()
            && let Some(docker) = active
            && self.entities[docker].exists()
        {
            if inside {
                let actor = &self.entities[docker];
                if !actor.is_using_airborne_locomotor(&self.catalog[actor.type_index]) {
                    self.kill_object(docker, crate::skirmish::DeathType::Normal);
                }
            } else if self.entities[docker].ai.supply.constructed() {
                self.ai_idle_from_ai(docker);
                self.entities[docker].ai.supply.force_wanting = true;
            }
        }
        // The warehouse override performs its callbacks before the base setter.
        self.entities[id]
            .dock
            .as_mut()
            .expect("Dock constructor")
            .crippled = setting;
    }

    pub(super) fn native_dock_open(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            actor.exists() && actor.dock.as_ref().is_some_and(|state| state.open)
        })
    }

    pub(super) fn native_dock_available_slot(&self, id: usize, docker: usize) -> Option<usize> {
        let state = self.entities[id].dock.as_ref()?;
        state
            .owners
            .iter()
            .position(|owner| owner.is_none_or(|owner| owner == docker))
            .or_else(|| {
                (self.catalog[self.entities[id].type_index]
                    .supply
                    .dock
                    .as_ref()
                    .expect("Dock data")
                    .number_approaches
                    == -1)
                    .then_some(state.owners.len())
            })
    }

    pub(super) fn native_dock_clear_to_approach(&self, id: usize, docker: usize) -> bool {
        let Some(state) = self.entities[id].dock.as_ref() else {
            return false;
        };
        self.catalog[self.entities[id].type_index]
            .supply
            .dock
            .as_ref()
            .expect("Dock data")
            .number_approaches
            == -1
            || state
                .owners
                .iter()
                .any(|owner| owner.is_none_or(|owner| owner == docker))
    }

    pub(super) fn native_dock_reserve(
        &mut self,
        id: usize,
        docker: usize,
    ) -> Option<(usize, [f32; 3])> {
        self.load_native_dock_positions(id);
        let dynamic = self.catalog[self.entities[id].type_index]
            .supply
            .dock
            .as_ref()
            .expect("Dock data")
            .number_approaches
            == -1;
        let state = self.entities[id].dock.as_mut().expect("Dock constructor");
        let index = state
            .owners
            .iter()
            .position(|owner| owner.is_none_or(|owner| owner == docker));
        let index = match index {
            Some(index) => index,
            None if dynamic => {
                state.owners.push(None);
                state.reached.push(false);
                state.owners.len() - 1
            }
            None => return None,
        };
        state.owners[index] = Some(docker);
        Some((index, self.native_dock_approach_position(id, docker, index)))
    }

    pub(super) fn native_dock_clear_to_advance(
        &self,
        id: usize,
        docker: usize,
        index: Option<usize>,
    ) -> bool {
        let Some(index) = index else {
            return false;
        };
        let state = self.entities[id].dock.as_ref().expect("Dock constructor");
        state.owners[index] == Some(docker)
            && state.reached[index]
            && index > 0
            && state.owners[index - 1].is_none()
    }

    pub(super) fn native_dock_advance(
        &mut self,
        id: usize,
        docker: usize,
        index: usize,
    ) -> Option<(usize, [f32; 3])> {
        self.load_native_dock_positions(id);
        let state = self.entities[id].dock.as_mut().expect("Dock constructor");
        if index == 0 || state.owners[index - 1].is_some() {
            return None;
        }
        state.owners[index - 1] = Some(docker);
        state.reached[index - 1] = false;
        state.owners[index] = None;
        state.reached[index] = false;
        Some((
            index - 1,
            self.native_dock_approach_position(id, docker, index - 1),
        ))
    }

    pub(super) fn native_dock_clear_to_enter(&self, id: usize, docker: usize) -> bool {
        self.entities[id]
            .dock
            .as_ref()
            .expect("Dock constructor")
            .active
            == Some(docker)
    }

    pub(super) fn native_dock_notify(
        &mut self,
        id: usize,
        docker: usize,
        notification: &Notification,
    ) {
        use docking_flags::{ACTIVE, BEGINNING, DOCKING, ENDING};
        let state = self.entities[id].dock.as_mut().expect("Dock constructor");
        match notification {
            Notification::Approach => {
                if let Some(index) = state.owners.iter().position(|owner| *owner == Some(docker)) {
                    state.reached[index] = true;
                }
                return;
            }
            Notification::Enter => {
                state.inside = true;
                if let Some(index) = state.owners.iter().position(|owner| *owner == Some(docker)) {
                    state.owners[index] = None;
                    state.reached[index] = false;
                }
            }
            Notification::Exit => {
                state.inside = false;
                if state.active == Some(docker) {
                    state.active = None;
                }
            }
            Notification::Action => {}
        }
        for actor in [id, docker] {
            let flags = &mut self.entities[actor].docking_flags;
            *flags = match notification {
                Notification::Enter => (*flags & !ENDING) | BEGINNING | DOCKING,
                Notification::Action => (*flags & !BEGINNING) | ACTIVE,
                Notification::Exit => (*flags & !(ACTIVE | DOCKING)) | ENDING,
                Notification::Approach => unreachable!("Approach has no drawable flags"),
            };
        }
    }

    pub(super) fn native_dock_cancel(&mut self, id: usize, docker: usize) {
        let Some(state) = self
            .entities
            .get_mut(id)
            .and_then(|actor| actor.dock.as_mut())
        else {
            return;
        };
        for (owner, reached) in state.owners.iter_mut().zip(&mut state.reached) {
            if *owner == Some(docker) {
                *owner = None;
                *reached = false;
            }
        }
        if state.active == Some(docker) {
            state.active = None;
            state.inside = false;
            self.entities[id].docking_flags = 0;
            if self.entities[docker].exists() {
                self.entities[docker].docking_flags = 0;
            }
        }
    }

    fn load_native_dock_positions(&mut self, id: usize) {
        if !self.entities[id].drawable.present
            || self.entities[id]
                .dock
                .as_ref()
                .expect("Dock constructor")
                .positions
                .is_some()
        {
            return;
        }
        let rules = self.catalog[self.entities[id].type_index]
            .supply
            .dock
            .as_ref()
            .expect("Dock data");
        let positions = Positions {
            enter: rules.enter.unwrap_or([0.0; 3]),
            action: rules.action.unwrap_or([0.0; 3]),
            exit: rules.exit.unwrap_or([0.0; 3]),
            approaches: rules.approaches.clone(),
            approach_bones: rules.approach_bones,
        };
        self.entities[id]
            .dock
            .as_mut()
            .expect("Dock constructor")
            .positions = Some(positions);
    }

    pub(super) fn native_dock_position(
        &mut self,
        id: usize,
        docker: usize,
        kind: &Position,
    ) -> [f32; 3] {
        self.load_native_dock_positions(id);
        let positions = self.entities[id]
            .dock
            .as_ref()
            .expect("Dock constructor")
            .positions
            .as_ref();
        if positions.is_none_or(|positions| {
            positions
                .enter
                .iter()
                .all(|coordinate| coordinate.partial_cmp(&0.0) == Some(std::cmp::Ordering::Equal))
        }) {
            return if matches!(kind, Position::Enter)
                && self.entities[docker]
                    .is_using_airborne_locomotor(&self.catalog[self.entities[docker].type_index])
            {
                self.object_position(id)
            } else {
                self.object_position(docker)
            };
        }
        let positions = positions.expect("nonzero Enter bone");
        let point = match kind {
            Position::Enter => positions.enter,
            Position::Action => positions.action,
            Position::Exit => positions.exit,
        };
        self.native_dock_world_point(id, point)
    }

    fn native_dock_world_point(&self, id: usize, point: [f32; 3]) -> [f32; 3] {
        let origin = self.object_transform(id).0;
        let basis = self.entities[id].object_basis();
        std::array::from_fn(|axis| {
            let x = basis[axis][0] * point[0];
            let y = basis[axis][1] * point[1];
            let z = basis[axis][2] * point[2];
            ((x + y) + z) + origin[axis]
        })
    }

    fn native_dock_approach_position(
        &mut self,
        id: usize,
        docker: usize,
        index: usize,
    ) -> [f32; 3] {
        let state = self.entities[id].dock.as_ref().expect("Dock constructor");
        let positions = state.positions.as_ref();
        let bone = positions
            .and_then(|positions| positions.approaches.get(index))
            .copied()
            .unwrap_or([0.0; 3]);
        let mut working = self.native_dock_world_point(id, bone);
        // Before load, native m_numberApproachPositionBones is -1, not zero.
        if positions.is_some_and(|positions| positions.approach_bones == 0) {
            let center = self.object_position(id);
            let source = self.object_position(docker);
            let delta = std::array::from_fn::<_, 3, _>(|axis| source[axis] - center[axis]);
            let squared = delta.map(|coordinate| coordinate * coordinate);
            let length = (squared[0] + squared[1] + squared[2]).sqrt();
            let inverse = if length == 0.0 { 0.0 } else { 1.0 / length };
            let radius = self.catalog[self.entities[id].type_index].geometry.major / 2.0;
            for (coordinate, delta) in working.iter_mut().zip(delta) {
                let normalized = delta * inverse;
                let offset = normalized * radius;
                *coordinate += offset;
            }
        }
        self.find_native_dock_position(docker, id, working)
    }
}

#[cfg(test)]
#[path = "dock/crippling_tests.rs"]
mod crippling_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "dock/death_tests.rs"]
mod death_tests;
