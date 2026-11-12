//! SH 2bfaea60 `OCLUpdate`, `RETAIL_COMPATIBLE_CRC=0`.
use super::{Skirmish, startup};

#[cfg(test)]
mod tests;
pub struct FactionOcl {
    pub faction: String,
    pub ocl: String,
}

pub struct Rules {
    pub min_delay: u32,
    pub max_delay: u32,
    pub ocl: Option<String>,
    pub faction_ocl: Vec<FactionOcl>,
    pub create_at_edge: bool,
    pub faction_triggered: bool,
}

pub(super) struct Facts<'a> {
    pub frame: u32,
    pub disabled: bool,
    pub under_construction: bool,
    pub playable_color: Option<u32>,
    pub faction: Option<&'a str>,
}

pub(super) struct State {
    next_creation: u32,
    timer_started: u32,
    faction_neutral: bool,
    player_color: u32,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            next_creation: 0,
            timer_started: 0,
            faction_neutral: true,
            player_color: 0,
        }
    }

    pub(super) fn reset(&mut self, rules: &Rules, frame: u32, random: &mut [u32; 6]) {
        // CRC=0 GameLogicRandomValue returns hi without a draw when lo>=hi.
        let delay = if rules.min_delay >= rules.max_delay {
            rules.max_delay
        } else {
            rules.min_delay.wrapping_add(startup::random_between(
                random,
                rules.max_delay - rules.min_delay,
            ))
        };
        self.timer_started = frame;
        self.next_creation = frame.wrapping_add(delay);
    }

    pub(super) fn update<'a>(
        &mut self,
        rules: &'a Rules,
        facts: &Facts<'_>,
        random: &mut [u32; 6],
    ) -> Option<&'a str> {
        let &Facts {
            frame,
            disabled,
            under_construction,
            playable_color,
            faction,
        } = facts;
        // Source shifts an initialized timer before checking changed ownership.
        if self.timer_started > 0 && disabled {
            self.next_creation = self.next_creation.wrapping_add(1);
            self.timer_started = self.timer_started.wrapping_add(1);
            return None;
        }
        if rules.faction_triggered {
            if self.faction_neutral {
                if let Some(color) = playable_color {
                    self.player_color = color;
                    self.faction_neutral = false;
                    self.reset(rules, frame, random);
                }
            } else if let Some(color) = playable_color {
                if color != self.player_color {
                    self.player_color = color;
                    self.reset(rules, frame, random);
                }
            } else {
                self.faction_neutral = true;
            }
            if self.faction_neutral {
                return None;
            }
        }
        if frame < self.next_creation || under_construction {
            return None;
        }
        let first = self.next_creation == 0;
        self.reset(rules, frame, random);
        if first {
            return None;
        }
        if rules.faction_triggered {
            // Reset precedes recipient/template lookup, even when it yields no list.
            let side = faction?;
            rules
                .faction_ocl
                .iter()
                .find(|entry| entry.faction == side)
                .map(|entry| entry.ocl.as_str())
        } else {
            rules.ocl.as_deref()
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_reinforcement_timer(&mut self, id: usize, ordinal: usize) {
        assert_eq!(
            ordinal,
            self.entities[id].behaviors.reinforcement_timers.len()
        );
        self.entities[id]
            .behaviors
            .reinforcement_timers
            .push(State::new());
    }

    pub(in crate::skirmish) fn update_reinforcement_timer(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        let actor = &self.entities[id];
        let player = &self.players[actor.owner];
        let rules = self.catalog[actor.type_index]
            .behavior
            .ocl_update
            .as_ref()
            .expect("OCLUpdate data");
        let facts = Facts {
            frame: self.update_frame(),
            disabled: self.object_disabled(id),
            under_construction: actor.under_construction(),
            playable_color: if player.playable_side {
                Some(player.color.expect("configured original PlayerColor"))
            } else {
                None
            },
            faction: player.side.as_deref(),
        };
        let edge = rules.create_at_edge;
        let request = self.entities[id].behaviors.reinforcement_timers[ordinal]
            .update(rules, &facts, &mut self.random)
            .map(str::to_owned);
        if let Some(name) = request {
            let list = self
                .delivery_lists
                .iter()
                .position(|list| list.name == name)
                .expect("selected native side has bound actual delivery OCL");
            let position = self.object_position(id);
            let creation = if edge {
                self.delivery_closest_edge(position)
            } else {
                position
            };
            self.create_delivery(id, list, creation);
        }
        1
    }
}
