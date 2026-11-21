//! Native `StealthUpdate` pulse and `Drawable/W3DScene` cloak material passes.
use crate::{model_animation::Animations, playtest::Match};
use bevy::prelude::*;
use generals_sim::skirmish::stealth::Event;
use std::collections::BTreeMap;
mod order;
mod render;
pub use order::register;
pub use render::{HeatPart, render};

#[derive(PartialEq)]
enum Look {
    Normal,
    Disguise,
    Friendly,
    FriendlyDetected,
    Detected,
    Invisible,
}

struct State {
    phase: f32,
    minimum: f32,
    opacity: f32,
    heat: f32,
    look: Look,
}
impl State {
    const fn new(random: f32) -> Self {
        Self {
            phase: random,
            minimum: 1.0,
            opacity: 1.0,
            heat: 0.0,
            look: Look::Normal,
        }
    }
    fn pulse(&mut self, mine: bool) {
        if mine {
            self.minimum = 0.0;
            self.opacity = 0.0;
            return;
        }
        let scaled = self.phase.sin() * 0.5;
        let factor = (0.5 + scaled).clamp(0.0, 1.0);
        let amount = (1.0 - self.minimum) * factor;
        self.opacity = self.minimum + amount;
        self.phase += 0.2;
    }
    fn set_look(&mut self, look: Look, opacity: f32, mine: bool) {
        if self.look == look {
            return;
        }
        self.minimum = if matches!(look, Look::Friendly | Look::FriendlyDetected) {
            opacity
        } else {
            1.0
        };
        self.heat = if !mine && matches!(look, Look::FriendlyDetected | Look::Detected) {
            1.0
        } else {
            0.0
        };
        self.look = look;
    }
    fn fade(&mut self, alive: bool, time_scale: f32) {
        if !alive || self.heat <= 0.001 {
            self.heat = 0.0;
        } else {
            let decrement = (1.0 - 0.8) * time_scale;
            self.heat *= 1.0 - decrement;
        }
    }
}

#[derive(Resource)]
pub struct Cloaks {
    states: BTreeMap<usize, State>,
    generation: u32,
    frame: Option<u64>,
}
impl Cloaks {
    pub const fn new() -> Self {
        Self {
            states: BTreeMap::new(),
            generation: u32::MAX,
            frame: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn fixture_opacity(id: usize, opacity: f32) -> Self {
        let mut result = Self::new();
        let mut state = State::new(0.0);
        state.look = Look::Disguise;
        state.opacity = opacity;
        result.states.insert(id, state);
        result
    }

    pub fn base_opacity(&self, id: usize) -> Option<f32> {
        let state = self.states.get(&id)?;
        match state.look {
            Look::Friendly | Look::FriendlyDetected | Look::Disguise => Some(state.opacity),
            Look::Detected | Look::Invisible => Some(0.0),
            Look::Normal => None,
        }
    }
}

pub fn record(
    mut cloaks: ResMut<Cloaks>,
    mut battle: ResMut<Match>,
    mut animations: ResMut<Animations>,
    mut feedback: ResMut<crate::selection_feedback::Feedback>,
) {
    let battle = battle.as_mut();
    if cloaks.frame != Some(battle.simulation.tick) || cloaks.generation != battle.generation {
        for event in &battle.simulation.presentation.stealth_events {
            if let Event::GrantFlash { object } = event {
                feedback.flash_as_selected(battle, *object);
            }
        }
    }
    let game = &mut battle.simulation;
    if cloaks.generation != battle.generation {
        cloaks.states.clear();
        cloaks.frame = None;
        cloaks.generation = battle.generation;
    }
    if !battle.started || battle.paused || cloaks.frame == Some(game.tick) {
        return;
    }
    cloaks.frame = Some(game.tick);
    for (id, actor) in game.entities.iter().enumerate() {
        if game.catalog[actor.type_index].behavior.stealth.is_some()
            && !cloaks.states.contains_key(&id)
        {
            // Native ctor draws from the client stream, not authoritative RNG.
            let phase = animations.effect_random(battle.generation) * std::f32::consts::PI;
            cloaks.states.insert(id, State::new(phase));
        }
    }
    for index in 0..game.presentation.stealth_events.len() {
        let event = &game.presentation.stealth_events[index];
        let (Event::Pulse { object }
        | Event::Look { object, .. }
        | Event::DisguiseOpacity { object, .. }
        | Event::Heat { object }) = *event
        else {
            continue;
        };
        let Some(state) = cloaks.states.get_mut(&object) else {
            continue;
        };
        let actor = &game.entities[object];
        let unit = &game.catalog[actor.type_index];
        let mine = unit.kind_of.iter().any(|kind| kind == "MINE");
        if unit
            .behavior
            .stealth
            .as_ref()
            .is_some_and(|rules| rules.disguise.is_some())
        {
            if actor.alive() {
                match *event {
                    Event::DisguiseOpacity { opacity, .. } => {
                        state.minimum = if opacity < 1.0 { 0.0 } else { 1.0 };
                        state.opacity = opacity;
                    }
                    Event::Pulse { .. } => state.pulse(false),
                    _ => continue,
                }
                state.look = Look::Disguise;
            }
            continue;
        }
        match *event {
            Event::Pulse { .. } => state.pulse(mine),
            Event::Heat { .. } => state.heat = 1.0,
            Event::Look {
                stealthed,
                detected,
                hint,
                ..
            } => {
                if hint && actor.owner == 0 {
                    state.heat = 1.0;
                }
                let friendly = game.allied(actor.owner, 0) || game.defeated[0];
                let look = if !actor.alive() || !stealthed {
                    Look::Normal
                } else if friendly {
                    if detected {
                        Look::FriendlyDetected
                    } else {
                        Look::Friendly
                    }
                } else if detected {
                    Look::Detected
                } else {
                    Look::Invisible
                };
                let minimum = unit
                    .behavior
                    .stealth
                    .as_ref()
                    .expect("stealth visual source module")
                    .friendly_opacity[0];
                game.set_drawable_hidden_by_stealth(object, matches!(look, Look::Invisible));
                state.set_look(look, minimum, mine);
            }
            _ => {}
        }
    }
}

pub fn fade(mut cloaks: ResMut<Cloaks>, mut battle: ResMut<Match>, time: Res<Time>) {
    let battle = battle.as_mut();
    let time = time.into_inner();
    let scale = if battle.paused {
        0.0
    } else {
        time.delta_secs() * 30.0
    };
    for (&id, state) in &mut cloaks.states {
        let alive = battle
            .simulation
            .entities
            .get(id)
            .is_some_and(generals_sim::skirmish::Entity::alive);
        state.fade(alive, scale);
        if !alive {
            if id < battle.simulation.entities.len() {
                battle.simulation.set_drawable_hidden_by_stealth(id, false);
            }
            state.set_look(Look::Normal, 1.0, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn friendly_pulse_uses_native_minimum_and_heat_scan_retriggers_then_fades() {
        let mut state = State::new(std::f32::consts::FRAC_PI_2);
        state.pulse(false);
        state.set_look(Look::Friendly, 0.3, false);
        assert_eq!(state.opacity.to_bits(), 1.0_f32.to_bits());
        state.phase = 0.0;
        state.pulse(false);
        assert_eq!(state.opacity.to_bits(), 0.65_f32.to_bits());
        state.set_look(Look::FriendlyDetected, 0.3, false);
        state.fade(true, 1.0);
        assert_eq!(state.heat.to_bits(), 0.8_f32.to_bits());
        state.set_look(Look::FriendlyDetected, 0.3, false);
        state.fade(true, 1.0);
        assert_eq!(state.heat.to_bits(), (0.8_f32 * 0.8).to_bits());
        state.heat = 1.0;
        state.fade(true, 0.0);
        assert_eq!(state.heat.to_bits(), 1.0_f32.to_bits());
        state.fade(false, 1.0);
        assert_eq!(state.heat.to_bits(), 0.0_f32.to_bits());
    }
}
