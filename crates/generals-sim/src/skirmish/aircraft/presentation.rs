//! `JetAI`'s sampled drawable offset and exhaust conditions precede physics.
use super::{AFTERBURNER, ALLOW_AIR, ENGINE, EXHAUST, LANDING, Phase, TAKEOFF};
use crate::skirmish::{Entity, Skirmish, ai_goals::MainState};

impl Entity {
    #[must_use]
    pub fn aircraft_engine_on(&self) -> bool {
        (self.alive() || self.jet_corpse())
            && self
                .aircraft
                .as_ref()
                .is_some_and(|state| state.flag(ENGINE))
    }

    #[must_use]
    pub fn aircraft_phase(&self) -> Option<&'static str> {
        let phase = self.aircraft.as_ref()?.phase;
        if matches!(self.ai.command.main_state, MainState::Dead) {
            return Some("dead");
        }
        Some(match phase {
            Phase::Idle => "idle",
            Phase::TaxiFromHangar => "taxi from hangar",
            Phase::AwaitTakeoff => "await runway for takeoff",
            Phase::TaxiTakeoff => "taxi to takeoff",
            Phase::Pause => "pause before takeoff",
            Phase::Takeoff => "takeoff",
            Phase::Return => "return for landing",
            Phase::AwaitLanding => "await runway for landing",
            Phase::Landing => "landing",
            Phase::TaxiFromLanding => "taxi from landing",
            Phase::Orient => "orient for parking",
            Phase::Reload => "reload ammunition",
            Phase::ReturnDead => "return to dead airfield",
            Phase::CircleDead => "circle dead airfield",
        })
    }

    #[must_use]
    pub fn aircraft_draw_height(&self) -> f32 {
        self.aircraft
            .as_ref()
            .map_or(0.0, |state| state.draw_height)
    }

    pub(in crate::skirmish) fn aircraft_conditions(&self) -> Vec<&'static str> {
        let Some(state) = &self.aircraft else {
            return Vec::new();
        };
        [(EXHAUST, "JETEXHAUST"), (AFTERBURNER, "JETAFTERBURNER")]
            .into_iter()
            .filter_map(|(flag, name)| state.flag(flag).then_some(name))
            .collect()
    }
}

impl Skirmish {
    pub(in crate::skirmish::aircraft) fn update_aircraft_presentation(&mut self, id: usize) {
        let actor = &self.entities[id];
        let state = actor.aircraft.as_ref().expect("aircraft state");
        let rules = self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules");
        let height = self.height_above_ground(id);
        let deck = self.aircraft_producer(id).map_or(0.0, |producer| {
            self.catalog[self.entities[producer].type_index]
                .parking
                .as_ref()
                .expect("parking rules")
                .deck_height
        });
        let check = matches!(actor.ai.command.main_state, MainState::Jet)
            || height <= 0.0
            || !state.flag(ALLOW_AIR)
            || actor.object_status & (1 << 29) != 0;
        let offset = if check {
            (rules.min_height + deck - height.max(0.0)).max(0.0)
        } else {
            0.0
        };
        let squared = actor
            .physics_velocity()
            .map(|component| component * component);
        let exhaust = state.flag(ALLOW_AIR) && (squared[0] + squared[1] + squared[2]).sqrt() > 0.0;
        if self.entities[id].drawable.present {
            self.entities[id]
                .aircraft
                .as_mut()
                .expect("aircraft state")
                .draw_height = offset;
        }
        self.entities[id]
            .aircraft
            .as_mut()
            .expect("aircraft state")
            .set(EXHAUST, exhaust);
    }

    pub(in crate::skirmish::aircraft) fn update_aircraft_engine(&mut self, id: usize) {
        let actor = &self.entities[id];
        let state = actor.aircraft.as_ref().expect("aircraft state");
        let rules = self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules");
        if !actor.drawable.present || rules.helipad {
            return;
        }
        let height = self.height_above_ground(id);
        let engine = state.flag(TAKEOFF | LANDING)
            || height > -9.0 * self.gravity
            || actor.moving()
            || actor.ai.path_request.pending.is_some();
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.set(ENGINE, engine);
    }
}

impl Skirmish {
    pub(in crate::skirmish::aircraft) fn set_aircraft_afterburner(
        &mut self,
        id: usize,
        enabled: bool,
    ) {
        let state = self.entities[id]
            .aircraft
            .as_mut()
            .expect("Jet constructor");
        if state.flag(AFTERBURNER) == enabled {
            return;
        }
        state.set(AFTERBURNER, enabled);
        let sound = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .unwrap()
            .sounds
            .afterburner
            .clone();
        self.object_sound(
            id,
            sound,
            if enabled {
                crate::skirmish::object_sounds::Action::Play
            } else {
                crate::skirmish::object_sounds::Action::Stop
            },
        );
    }

    pub(in crate::skirmish::aircraft) fn aircraft_low_fuel_sound(&mut self, id: usize) {
        let sound = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .unwrap()
            .sounds
            .low_fuel
            .clone();
        self.object_sound(id, sound, crate::skirmish::object_sounds::Action::Play);
    }

    pub(in crate::skirmish::aircraft) fn aircraft_landing_sound(&mut self, id: usize) {
        if self.entities[id]
            .aircraft
            .as_ref()
            .unwrap()
            .flag(super::LANDING_SOUND_PLAYED)
        {
            return;
        }
        let position = self.object_position(id);
        let point = [position[0], position[1]];
        let ground = self.ground_height(point);
        let layer = self
            .movement_terrain
            .get_highest_layer_for_destination(position, ground, false);
        let ground = self.movement_layer_height(layer, point);
        let deck = self.aircraft_producer(id).map_or(0.0, |producer| {
            self.catalog[self.entities[producer].type_index]
                .parking
                .as_ref()
                .unwrap()
                .deck_height
        });
        if position[2] - 0.25 <= ground + deck {
            self.entities[id]
                .aircraft
                .as_mut()
                .unwrap()
                .set(super::LANDING_SOUND_PLAYED, true);
            let sound = self.catalog[self.entities[id].type_index]
                .aircraft
                .as_ref()
                .unwrap()
                .sounds
                .wheel_screech
                .clone();
            self.position_sound(id, sound, position);
        }
    }
}

impl Entity {
    pub(in crate::skirmish) fn jet_goal_when_none(&self, height: f32) -> Option<[f32; 3]> {
        let state = self.aircraft.as_ref()?;
        if !state.flag(TAKEOFF | LANDING) || !state.flag(ALLOW_AIR) || state.flag(super::CIRCLING) {
            return None;
        }
        let x = self.facing.cos() * 1000.0;
        let y = self.facing.sin() * 1000.0;
        Some([self.position[0] + x, self.position[1] + y, height])
    }
}

impl Entity {
    pub(in crate::skirmish) fn jet_allows_move_aside(&self) -> bool {
        self.aircraft
            .as_ref()
            .is_none_or(|state| state.flag(ALLOW_AIR) && !state.flag(TAKEOFF | LANDING))
    }
}

impl Entity {
    pub(in crate::skirmish) fn jet_idle(&self) -> bool {
        self.aircraft
            .as_ref()
            .is_none_or(|state| state.pending.is_none())
    }
}
