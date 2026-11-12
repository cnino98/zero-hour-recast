//! Stationary `GarrisonContain`, SH 2bfaea60.
use super::{GarrisonPlacement, Rules, Skirmish};
use crate::skirmish::Order;

mod exits;
mod points;
mod removal;
mod stations;
#[cfg(test)]
mod tests;

pub struct GarrisonRules {
    pub allow_enemies: bool,
    pub allow_neutral: bool,
    pub immune_to_clear: bool,
    pub heal_frames: Option<f32>,
    /// Non-enclosing containers use pristine STATION bones, not firing windows.
    pub stations: Option<Vec<[f32; 3]>>,
}

pub(super) struct GarrisonState {
    pub original_owner: usize,
    hide_from_nonallies: bool,
    pub points: [Option<usize>; 40],
    pub rally: Option<[f32; 2]>,
    pub placed: [u64; 40],
    pub facing: [f32; 40],
    pub flash: [Option<u64>; 40],
    pub stations: Vec<[f32; 3]>,
}

impl Skirmish {
    pub(in crate::skirmish) fn container_apparent_owner(
        &self,
        id: usize,
        observer: usize,
    ) -> Option<usize> {
        self.container_rules(id)?;
        let actor = &self.entities[id];
        if let Some(state) = &actor.containment.garrison
            && state.hide_from_nonallies
            && !self.allied(actor.owner, observer)
        {
            return Some(state.original_owner);
        }
        Some(actor.owner)
    }

    #[must_use]
    pub fn is_garrison(&self, carrier: usize) -> bool {
        self.entities.get(carrier).is_some_and(|actor| {
            self.catalog[actor.type_index]
                .contain
                .as_ref()
                .is_some_and(|rules| rules.garrison.is_some())
        })
    }

    pub(super) fn container_relationship(&self, id: usize, carrier: usize, rules: &Rules) -> bool {
        let owner = self.entities[id].owner;
        let other = self.entities[carrier].owner;
        if self.entities[carrier].cave_index().is_some() {
            return owner == other || self.passengers(carrier).is_empty();
        }
        if rules.tunnel.is_some() {
            // TunnelContain bypasses OpenContain admission flags; the player
            // ActionManager still forbids entering another faction's structure.
            return owner == other;
        }
        if let Some(heal) = &rules.heal {
            return if self.object_relationship(id, carrier) == 2 {
                rules.relationships.allies
            } else if self.object_relationship(id, carrier) == 0 {
                heal.allow_enemies
            } else {
                heal.allow_neutral
            };
        }
        let Some(garrison) = &rules.garrison else {
            return if self.object_relationship(id, carrier) == 2 {
                rules.relationships.allies
            } else if self.object_relationship(id, carrier) == 0 {
                rules.relationships.enemies
            } else {
                rules.relationships.neutral
            };
        };
        if self.object_relationship(id, carrier) == 2 {
            rules.relationships.allies
        } else if self.object_relationship(id, carrier) == 0 {
            garrison.allow_enemies
        } else {
            garrison.allow_neutral
        }
    }

    pub(super) fn garrison_damage_allows_entry(&self, carrier: usize) -> bool {
        let actor = &self.entities[carrier];
        let unit = &self.catalog[actor.type_index];
        unit.kind_of
            .iter()
            .any(|kind| kind == "GARRISONABLE_UNTIL_DESTROYED")
            || self.body_damage_state(carrier) != 2
    }

    pub(super) fn garrison_board(&mut self, id: usize, carrier: usize) {
        if !self.is_garrison(carrier) {
            return;
        }
        self.entities[id].containment.placement = if self.station_garrison(carrier) {
            GarrisonPlacement::Station
        } else {
            self.center_garrison_rider(id, carrier);
            GarrisonPlacement::Enclosed
        };
        self.recalculate_garrison_owner(carrier);
    }

    pub(super) fn recalculate_garrison_owner(&mut self, carrier: usize) {
        if !self.is_garrison(carrier) {
            return;
        }
        let actor = &mut self.entities[carrier];
        let state = actor.containment.garrison.get_or_insert_with(|| {
            Box::new(GarrisonState {
                original_owner: actor.owner,
                hide_from_nonallies: false,
                points: [None; 40],
                rally: None,
                placed: [0; 40],
                facing: [0.0; 40],
                flash: [None; 40],
                stations: Vec::new(),
            })
        });
        let original = state.original_owner;
        let owner = self
            .passengers(carrier)
            .first()
            .map_or(original, |&id| self.entities[id].owner);
        let riders = self.passengers(carrier);
        let hide = riders
            .first()
            .is_some_and(|&first| !self.entities[first].detected())
            && riders.iter().all(|&id| self.entities[id].stealthed());
        self.entities[carrier]
            .containment
            .garrison
            .as_mut()
            .expect("garrison state")
            .hide_from_nonallies = hide;
        self.transfer_maintenance_owner(carrier, owner);
    }

    pub(super) fn center_garrison_rider(&mut self, id: usize, carrier: usize) {
        let position = self.object_position(carrier);
        self.entities[id].position = [position[0], position[1]];
        self.entities[id].containment.height = position[2];
    }

    pub(in crate::skirmish) fn update_garrison(&mut self, carrier: usize) -> i32 {
        self.update_open_contain(carrier);
        for id in self.passengers(carrier).to_vec() {
            if self.entities[id].is_effectively_dead() {
                self.remove_passenger(id, carrier);
                self.set_safe_occlusion_frame(id, self.update_frame().wrapping_add(30_000));
            }
        }
        self.match_garrison_points(carrier);
        let frames = self.catalog[self.entities[carrier].type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.garrison.as_ref())
            .and_then(|rules| rules.heal_frames);
        if let Some(frames) = frames {
            for id in self.passengers(carrier).to_vec() {
                let actor = &self.entities[id];
                let elapsed =
                    self.update_frame()
                        .wrapping_sub(crate::skirmish::ai_dispatch::native_frame(
                            actor.containment.entered_at,
                        ));
                let amount = if crate::skirmish::progression::integer_real(elapsed) >= frames {
                    actor.max_health
                } else {
                    actor.max_health / frames
                };
                // GarrisonContain intentionally leaves sourceID invalid.
                self.apply_healing(id, None, amount);
            }
        }
        1
    }

    pub(in crate::skirmish) fn garrison_body_state_changed(
        &mut self,
        carrier: usize,
        state: usize,
    ) {
        if state == 2
            && !self.catalog[self.entities[carrier].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "GARRISONABLE_UNTIL_DESTROYED")
        {
            for rider in self.passengers(carrier).to_vec() {
                self.set_order(rider, Order::ExitContainer);
                self.set_last_command_source(rider, crate::skirmish::ai_goals::COMMAND_FROM_AI);
            }
        }
    }

    #[must_use]
    pub fn clearable_garrison(&self, carrier: usize) -> bool {
        self.entities.get(carrier).is_some_and(|actor| {
            actor.alive()
                && actor.loaded()
                && self.catalog[actor.type_index]
                    .contain
                    .as_ref()
                    .and_then(|rules| rules.garrison.as_ref())
                    .is_some_and(|rules| !rules.immune_to_clear)
        })
    }

    pub(in crate::skirmish) fn clear_garrison(
        &mut self,
        source: usize,
        carrier: usize,
        count: u32,
        allowed: &[String],
        forbidden: &[String],
    ) -> u32 {
        if count == 0 || !self.clearable_garrison(carrier) {
            return 0;
        }
        let mut killed = 0;
        for id in self.passengers(carrier).to_vec() {
            let actor = &self.entities[id];
            let kinds = &self.catalog[actor.type_index].kind_of;
            if actor.is_effectively_dead()
                || !allowed.iter().all(|kind| kinds.contains(kind))
                || forbidden.iter().any(|kind| kinds.contains(kind))
            {
                continue;
            }
            if self.entities[source].exists() {
                self.score_kill(source, id);
            }
            self.kill_object(id, crate::skirmish::DeathType::Normal);
            killed += 1;
            if killed == count {
                break;
            }
        }
        killed
    }
}
