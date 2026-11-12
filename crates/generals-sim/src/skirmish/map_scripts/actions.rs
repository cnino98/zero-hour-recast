//! `ScriptEngine` primitive operation callbacks in authored action order.
use super::{Action, AudioAction, Counter, Fade, Script, State, native_real, teams, wander};
use crate::skirmish::{DeathType, Skirmish, startup};
use num_traits::ToPrimitive;

impl Skirmish {
    pub(super) fn execute_map_action(
        &mut self,
        state: &mut State,
        definitions: &[Script],
        action: &Action,
    ) {
        match action {
            Action::ToppleDirection(name, direction) => {
                state.set_topple_direction(name, Some(*direction));
            }
            Action::TreeSway(angles, period, randomness) => {
                state.set_sway(*angles, *period, *randomness);
            }
            Action::DrawIconUi(enabled) => self.draw_icon_ui = *enabled,
            Action::PlayerRepair(player, name) => {
                self.execute_map_player_repair(state, player, name);
            }
            Action::PlayerBuild(name, units, enabled) => {
                self.execute_map_player_build(state, name, *units, *enabled);
            }
            Action::TeamHuntCommand(..)
            | Action::TeamAttackArea(..)
            | Action::TeamWander(..)
            | Action::TeamPanic(..)
            | Action::TeamAttitude(..)
            | Action::TeamRelation(..) => self.execute_map_team_action(action),
            Action::SetCaveIndex(name, index) => {
                if let Some(id) = self.named_map_object(name) {
                    self.set_cave_index(id, *index);
                }
            }
            Action::NamedDelete(name) => {
                if let Some(id) = self.named_map_object(name) {
                    self.destroy_maintenance_object(id);
                }
            }
            Action::EnableScript(name, enabled) => {
                if let Some(active) = state.groups.get_mut(name) {
                    *active = *enabled;
                }
                if let Some(index) = definitions.iter().position(|script| script.name == *name) {
                    state.active[index] = *enabled;
                }
            }
            Action::SetCounter(..)
            | Action::DecrementCounter(..)
            | Action::RandomTimer(..)
            | Action::RandomSecondsTimer(..)
            | Action::StopTimer(..) => execute_counter(action, state, &mut self.random),
            Action::FadeAdd(minimum, maximum, frames) => {
                state.fade_add(*minimum, *maximum, *frames, self.tick);
            }
            Action::Sound(name) => state.audio.push(AudioAction::Play(name.clone())),
            Action::AudioVolume(name, volume) => state
                .audio
                .push(AudioAction::OverrideVolume(name.clone(), *volume)),
            Action::NamedKill(name) => {
                if let Some(id) = self.named_map_object(name) {
                    let damage = self.entities[id].max_health;
                    self.apply_native_damage(id, None, damage, DeathType::Normal);
                }
            }
            Action::NamedDamage(name, damage) => {
                if let Some(id) = self.named_map_object(name) {
                    self.apply_native_damage(id, None, native_real(*damage), DeathType::Normal);
                }
            }
            Action::WarehouseValue(index, cash) => {
                // SupplyWarehouseDockUpdate::setCashValue uses ceil on
                // native Real division, including a partial last box.
                let boxes = (native_real(*cash) / native_real(self.rules.supply_value)).ceil();
                if let Some(source) = self
                    .layout
                    .supplies
                    .get(*index)
                    .and_then(|neutral| self.registered_neutral_object(*neutral))
                    .and_then(|object| self.supply_index(object))
                {
                    self.supplies[source].boxes =
                        boxes.to_u32().expect("nonnegative native warehouse boxes");
                }
            }
        }
    }
    fn execute_map_team_action(&mut self, action: &Action) {
        match action {
            Action::TeamHuntCommand(name, button) => {
                let team = (0..self.players.len())
                    .find_map(|owner| self.team_membership.find_team(owner, name));
                if let Some(team) = team {
                    let members = self.team_membership.team_objects(team).to_vec();
                    self.team_command_button_hunt(&members, button);
                }
            }
            Action::TeamAttackArea(name, points) => {
                for id in teams::members(self, &self.map_side_state.members, name) {
                    let _ = self.attack_area(id, points);
                }
            }
            Action::TeamWander(name, label, graph) | Action::TeamPanic(name, label, graph) => {
                let panic = matches!(action, Action::TeamPanic(..));
                for id in teams::members(self, &self.map_side_state.members, name) {
                    if !wander::start(self, id, label, graph, panic) {
                        break;
                    }
                }
            }
            Action::TeamAttitude(name, attitude) => {
                for id in teams::members(self, &self.map_side_state.members, name) {
                    self.entities[id].ai.mood.attitude = *attitude;
                }
            }
            Action::TeamRelation(name, other, relation) => {
                if self.layout.map_teams.iter().any(|team| team.name == *name)
                    && self.layout.map_teams.iter().any(|team| team.name == *other)
                {
                    self.map_side_state
                        .team_relations
                        .entry(name.clone())
                        .or_default()
                        .insert(other.clone(), *relation);
                }
            }
            _ => unreachable!("team action callback"),
        }
    }

    fn execute_map_player_repair(&self, state: &mut State, player: &str, name: &str) {
        let owner = self.map_script_player(state, player);
        if let (Some(owner), Some(target)) = (owner, self.named_map_object(name)) {
            super::build_lists::repair::enqueue(self, state, owner, target);
        }
    }

    fn execute_map_player_build(&mut self, state: &State, name: &str, units: bool, enabled: bool) {
        let owner = self.map_script_player(state, name);
        if let Some(owner) = owner {
            self.set_map_build_allowed(owner, units, enabled);
        }
    }
}

fn execute_counter(action: &Action, state: &mut State, random: &mut [u32; 6]) {
    match action {
        Action::SetCounter(name, value) => {
            state
                .counters
                .entry(name.clone())
                .or_insert(Counter {
                    value: 0,
                    countdown: false,
                })
                .value = *value;
        }
        Action::DecrementCounter(name, amount) => {
            let counter = state.counters.entry(name.clone()).or_insert(Counter {
                value: 0,
                countdown: false,
            });
            counter.value = counter.value.wrapping_sub(*amount);
        }
        Action::RandomTimer(name, minimum, maximum) => {
            let value = if minimum >= maximum {
                *maximum
            } else {
                let width = u32::try_from(i64::from(*maximum) - i64::from(*minimum))
                    .expect("native random range");
                i32::try_from(
                    i64::from(*minimum) + i64::from(startup::random_between(random, width)),
                )
                .expect("bounded native random value")
            };
            state.counters.insert(
                name.clone(),
                Counter {
                    value,
                    countdown: true,
                },
            );
        }
        Action::RandomSecondsTimer(name, minimum, maximum) => {
            let seconds = startup::random_real(random, *minimum, *maximum);
            let frames = ((seconds * 1000.0) * (30.0 / 1000.0)).ceil();
            state.counters.insert(
                name.clone(),
                Counter {
                    value: frames.to_i32().expect("validated native timer"),
                    countdown: true,
                },
            );
        }
        Action::StopTimer(name) => {
            state
                .counters
                .entry(name.clone())
                .or_insert(Counter {
                    value: 0,
                    countdown: false,
                })
                .countdown = false;
        }
        _ => unreachable!("counter-family action callback"),
    }
}

impl State {
    fn fade_add(&mut self, minimum: f32, maximum: f32, frames: [u32; 3], now: u64) {
        let mut fade = Fade {
            start_frame: now,
            minimum,
            maximum,
            increase: frames[0],
            hold: frames[1],
            decrease: frames[2],
            additive: true,
            amount: minimum,
            frame: 0,
        };
        if fade.increase != 0 || fade.advance() {
            self.fade = Some(fade);
        } else {
            self.fade = None;
        }
    }
}
