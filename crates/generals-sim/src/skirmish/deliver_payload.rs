//! SH `DeliverPayloadNugget` and `DeliverPayloadAIUpdate`, original contained-payload path.
use super::{Entity, Skirmish};
mod creation;
mod states;
#[cfg(test)]
mod tests;

pub struct TransportRules {
    pub door_delay: u32,
    pub exit_bone: String,
    pub exit_pitch_rate: f32,
    pub dive_sound: String,
    pub strafe_effects: Vec<String>,
}

// The real OCL creation callback owns its copy while newObject runs constructors.
#[derive(Clone)]
pub struct Payload {
    pub object: String,
    pub count: i32,
}

#[derive(Clone)]
pub struct Rules {
    pub name: String,
    pub transport: String,
    pub payload: Vec<Payload>,
    pub container: String,
    pub transport_start: TransportStart,
    pub release: Release,
    pub attempts: i32,
    pub delivery_distance: f32,
    pub pre_open_distance: f32,
    pub formation_size: u32,
    pub formation_spacing: f32,
    pub convergence: f32,
    pub error_radius: f32,
    pub self_destruct: bool,
    pub visible: Option<VisiblePayload>,
    pub dive: Option<Dive>,
    pub decal: Option<(f32, super::radius_decal::Template)>,
}

#[derive(Clone)]
pub struct TransportStart {
    pub preferred_height: bool,
    pub max_speed: bool,
    pub delay_delivery_max: u32,
}

#[derive(Clone)]
pub struct Release {
    pub drop_offset: [f32; 3],
    pub drop_delay: u32,
    pub parachute_directly: bool,
    pub drop_variance: [f32; 3],
    pub inherit_velocity: bool,
    pub exit_pitch_rate: f32,
    pub fire_weapon: bool,
}

#[derive(Clone)]
pub struct VisiblePayload {
    pub per_interval: u32,
    pub count: u32,
    pub bone: String,
    pub sub_object: String,
    pub object: String,
    pub weapon: String,
}

#[derive(Clone)]
pub struct Dive {
    pub start: f32,
    pub end: f32,
    pub weapon_slot: Option<usize>,
    pub effect: String,
}

#[derive(Clone, Copy)]
enum Phase {
    Approach,
    Delivering,
    Consider,
    Recover,
    HeadOffMap,
    CleanUp,
}

struct Mission {
    list: usize,
    move_to: [f32; 3],
    target: [f32; 3],
    phase: Phase,
    entries: i32,
    drop_delay: u32,
    reentry_frame: u32,
    exit_direction: [f32; 3],
    visible_delivered: u32,
    dive: DiveState,
}

#[derive(PartialEq, Eq)]
enum DiveState {
    Before,
    Diving,
    After,
}

pub(super) struct State {
    mission: Option<Mission>,
    previous_squared: f32,
    pub(super) free_to_exit: bool,
    pub(super) accepting_commands: bool,
    doors_open: Option<bool>,
}

impl State {
    const fn new() -> Self {
        Self {
            mission: None,
            previous_squared: 0.0,
            free_to_exit: false,
            accepting_commands: true,
            doors_open: None,
        }
    }
}

impl Entity {
    pub(in crate::skirmish) fn delivery_doors_open(&self) -> Option<bool> {
        self.delivery.as_ref().and_then(|state| state.doors_open)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_delivery_module(
        &mut self,
        id: usize,
        registration: usize,
    ) {
        self.construct_ai_module(id, registration);
        assert!(self.entities[id].delivery.is_none());
        self.entities[id].delivery = Some(Box::new(State::new()));
    }

    pub(in crate::skirmish) fn update_delivery_module(&mut self, id: usize) -> i32 {
        self.update_radius_decal(id);
        if !self.entities[id].is_ai_dead() {
            self.update_delivery_state(id);
        }
        self.update_delivery_dive(id);
        // Native subclass ignores the base sleep result and remains awake every frame.
        self.update_ai_module(id);
        1
    }

    pub(in crate::skirmish) fn destroy_delivery_module(&mut self, id: usize) {
        // DeliverPayloadAIUpdate's destructor clears the owned decal before
        // AIUpdateInterface releases the nested dock state.
        self.entities[id].radius_decal = None;
        self.cancel_docking(id);
    }

    fn delivery_state(&self, id: usize) -> &State {
        self.entities[id]
            .delivery
            .as_deref()
            .expect("DeliverPayloadAIUpdate instance")
    }
    fn delivery_state_mut(&mut self, id: usize) -> &mut State {
        self.entities[id]
            .delivery
            .as_deref_mut()
            .expect("DeliverPayloadAIUpdate instance")
    }

    fn delivery_mission(&self, id: usize) -> &Mission {
        self.delivery_state(id)
            .mission
            .as_ref()
            .expect("active delivery mission")
    }
    fn delivery_mission_mut(&mut self, id: usize) -> &mut Mission {
        self.delivery_state_mut(id)
            .mission
            .as_mut()
            .expect("active delivery mission")
    }
}
