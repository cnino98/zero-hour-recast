//! `ChinookAIUpdate::update` runs before the `SupplyTruck` and common AI callbacks.
use super::{Flight, Phase, Skirmish, State};
use crate::skirmish::{
    Order, WeaponFxEvent,
    ai_goals::{COMMAND_FROM_AI, MainState},
    combat::attack::Goal,
};

pub enum VisualEvent {
    RopeCreated(u64),
    RotorWash {
        object: usize,
        position: [f32; 3],
        elevation: f32,
        particle: String,
        visible_players: u16,
    },
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_chinook_module(&mut self, id: usize, registration: usize) {
        self.construct_supply_module(id, registration);
        self.entities[id].chinook = Some(Box::new(State::new(self.object_position(id)[2])));
    }

    fn chinook_healing_airfield(&self, id: usize) -> Option<usize> {
        self.entities[id]
            .chinook
            .as_ref()
            .and_then(|state| state.airfield)
            .filter(|&airfield| {
                self.entities.get(airfield).is_some_and(|actor| {
                    actor.exists() && !actor.is_effectively_dead() && actor.parking.is_some()
                }) && self.attack_kind(airfield, "FS_AIRFIELD")
            })
    }

    pub(in crate::skirmish::chinook) fn set_chinook_healing_airfield(
        &mut self,
        id: usize,
        new: Option<usize>,
    ) {
        if let Some(old) = self.chinook_healing_airfield(id)
            && Some(old) != new
        {
            self.parking_set_healee(old, id, false);
        }
        self.entities[id].chinook.as_mut().unwrap().airfield = new;
    }

    pub(in crate::skirmish) fn container_waiting(&self, id: usize) -> bool {
        !self.entities[id].containment.waiting.is_empty()
    }

    pub(in crate::skirmish) fn update_chinook_module(&mut self, id: usize) -> i32 {
        let contains = self.catalog[self.entities[id].type_index]
            .contain_family()
            .is_some();
        let waiting = contains && self.container_waiting(id);
        self.update_chinook_healing(id, waiting);
        if contains {
            // This is the qualified SupplyTruck::isIdle call, deliberately not
            // the Chinook override that hides pending commands from scripts.
            if matches!(self.entities[id].ai.command.main_state, MainState::Active)
                && matches!(self.entities[id].ai.order, Order::Idle)
            {
                let pending = self.entities[id].chinook.as_mut().unwrap().pending.take();
                if let Some(command) = pending {
                    self.replay_stored_command(id, command);
                } else if waiting && !self.entities[id].landed() {
                    self.set_chinook_state(id, Phase::Land, None, None, COMMAND_FROM_AI);
                } else if !waiting
                    && self.entities[id].landed()
                    && self.entities[id]
                        .chinook
                        .as_ref()
                        .unwrap()
                        .airfield
                        .is_none()
                {
                    self.set_chinook_state(id, Phase::Takeoff, None, None, COMMAND_FROM_AI);
                }
            }
            if self.update_frame() % 10 == 1
                && self.container_fire_allowed(id, None)
                && let Some(victim) = self.entities[id].ai.current_victim
            {
                for rider in self.passengers(id).to_vec() {
                    if self.entities[rider].ai.registration.is_some()
                        && self.entities[rider].ai.current_victim.is_none()
                    {
                        self.ai_attack_goal(rider, Goal::Object(victim), 999);
                    }
                }
            }
        }
        self.record_chinook_rotor_wash(id);
        self.update_supply_module(id)
    }

    fn update_chinook_healing(&mut self, id: usize, waiting: bool) {
        let Some(airfield) = self.chinook_healing_airfield(id) else {
            self.set_chinook_healing_airfield(id, None);
            return;
        };
        let actor = &self.entities[id];
        if actor.landed()
            && !waiting
            && actor.chinook.as_ref().unwrap().pending.is_none()
            && actor.health.partial_cmp(&actor.max_health) == Some(std::cmp::Ordering::Equal)
        {
            self.parking_set_healee(airfield, id, false);
            self.set_chinook_state(id, Phase::Takeoff, None, None, COMMAND_FROM_AI);
            if let Some(position) = self.entities[airfield].rally_point {
                self.native_move_from(id, position, COMMAND_FROM_AI);
            }
        } else {
            self.parking_set_healee(airfield, id, actor.landed());
        }
    }

    fn record_chinook_rotor_wash(&mut self, id: usize) {
        if !matches!(
            self.entities[id].chinook.as_ref().unwrap().flight,
            Flight::Landing(_) | Flight::TakingOff(_) | Flight::Landed
        ) {
            return;
        }
        let mut position = self.object_position(id);
        let original = position[2];
        position[2] = self.ground_height([position[0], position[1]]) + 3.0;
        let mut visible_players = 0;
        for player in 0..self.players.len() {
            if self.visible_to(player, id) {
                visible_players |= 1_u16 << player;
            }
        }
        let particle = self.catalog[self.entities[id].type_index]
            .chinook
            .as_ref()
            .unwrap()
            .rotor_wash
            .clone();
        self.presentation
            .weapon_fx
            .push(WeaponFxEvent::Chinook(VisualEvent::RotorWash {
                object: id,
                position,
                elevation: original - position[2],
                particle,
                visible_players,
            }));
    }

    pub(in crate::skirmish) fn chinook_repair_admits(
        &self,
        id: usize,
        target: usize,
        source: i32,
    ) -> bool {
        let (Some(actor), Some(destination)) = (self.entities.get(id), self.entities.get(target))
        else {
            return false;
        };
        self.object_relationship(id, target) == 2
            && !actor.is_effectively_dead()
            && !self.attack_kind(id, "IMMOBILE")
            && !self.object_disabled(id)
            && !actor.under_construction()
            && !destination.under_construction()
            && !destination.selling()
            && self.attack_kind(id, "VEHICLE")
            && self.attack_kind(target, "FS_AIRFIELD")
            && self.height_above_ground(id) > 0.0
            && actor.health.partial_cmp(&actor.max_health) != Some(std::cmp::Ordering::Equal)
            && (self.computer_player(actor.owner)
                || source == 1
                || self.maintenance_target_visible(actor.owner, target))
    }
}
