//! `TunnelContain` and the player's shared `TunnelTracker`, SH 2bfaea60.

use super::{Order, Skirmish, UnitType};

pub(super) mod cave;
mod native;
#[cfg(test)]
mod tests;

pub struct TunnelRules {
    pub full_heal_frames: f32,
    pub conditions: super::death_callbacks::Conditions,
}

pub(super) struct State {
    created: bool,
    registered: bool,
}

pub(super) struct Network {
    nemesis: Option<usize>,
    nemesis_frame: u32,
    endpoints: Vec<usize>,
    pub(super) passengers: Vec<usize>,
}

impl Network {
    pub(super) const fn new() -> Self {
        Self {
            nemesis: None,
            nemesis_frame: 0,
            endpoints: Vec::new(),
            passengers: Vec::new(),
        }
    }
}

pub(super) fn validate(catalog: &[UnitType]) -> Result<(), &'static str> {
    let mut capacity = None;
    for unit in catalog {
        let Some(contain) = &unit.contain else {
            continue;
        };
        let Some(rules) = &contain.tunnel else {
            continue;
        };
        if !rules.full_heal_frames.is_finite()
            || capacity.is_some_and(|value| value != contain.capacity)
            || contain.garrison.is_some()
            || unit.contain_family() == Some("InternetHackContain")
        {
            return Err("Invalid shared tunnel network rules");
        }
        capacity = Some(contain.capacity);
    }
    Ok(())
}

impl Skirmish {
    #[must_use]
    pub fn is_tunnel(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            self.catalog[actor.type_index]
                .contain
                .as_ref()
                .is_some_and(|rules| rules.tunnel.is_some())
        })
    }

    pub(super) fn tunnel_network(&self, id: usize) -> &Network {
        self.entities[id].cave_index().map_or_else(
            || &self.tunnels[self.entities[id].owner],
            |index| self.caves[index].as_ref().expect("registered cave tracker"),
        )
    }

    pub(super) fn tunnel_network_mut(&mut self, id: usize) -> &mut Network {
        if let Some(index) = self.entities[id].cave_index() {
            self.caves[index].as_mut().expect("registered cave tracker")
        } else {
            &mut self.tunnels[self.entities[id].owner]
        }
    }

    pub(super) fn set_cave_index(&mut self, id: usize, index: usize) {
        let Some(old) = self.entities[id].cave_index() else {
            return;
        };
        if self
            .caves
            .get(old)
            .and_then(Option::as_ref)
            .is_some_and(|network| !network.passengers.is_empty())
            || self
                .caves
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|network| !network.passengers.is_empty())
        {
            return;
        }
        self.unregister_tunnel(id);
        self.entities[id]
            .containment
            .cave
            .as_mut()
            .expect("CaveContain constructor")
            .index = index;
        self.register_tunnel(id);
    }

    pub(super) fn register_tunnel(&mut self, id: usize) {
        if !self.is_tunnel(id) {
            return;
        }
        if let Some(index) = self.entities[id].cave_index() {
            if self.caves.len() <= index {
                self.caves.resize_with(index + 1, || None);
            }
            if self.caves[index].is_none() {
                self.caves[index] = Some(Network::new());
            }
        }
        if let Some(state) = &mut self.entities[id].containment.tunnel {
            state.registered = true;
        }
        let endpoints = &mut self.tunnel_network_mut(id).endpoints;
        if !endpoints.contains(&id) {
            endpoints.push(id);
        }
    }

    pub(super) fn unregister_tunnel(&mut self, id: usize) {
        if !self.is_tunnel(id) {
            return;
        }
        if self.entities[id]
            .containment
            .tunnel
            .as_ref()
            .is_some_and(|state| !state.registered)
        {
            return;
        }
        self.unregister_tunnel_network(id, self.entities[id].owner);
        if let Some(state) = &mut self.entities[id].containment.tunnel {
            state.registered = false;
        }
    }

    fn unregister_tunnel_network(&mut self, id: usize, owner: usize) {
        let network = if let Some(index) = self.entities[id].cave_index() {
            let Some(network) = self.caves.get_mut(index).and_then(Option::as_mut) else {
                return;
            };
            network
        } else {
            &mut self.tunnels[owner]
        };
        if let Some(index) = network
            .endpoints
            .iter()
            .position(|&endpoint| endpoint == id)
        {
            network.endpoints.remove(index);
        }
        let replacement = network.endpoints.first().copied();
        let passengers = network.passengers.clone();
        for rider in passengers {
            if let Some(replacement) = replacement {
                let valid_replacement = self.entities[replacement].exists().then_some(replacement);
                if self.entities[rider].contained_by() == Some(id) {
                    let actor = &mut self.entities[rider];
                    actor.containment.carrier = valid_replacement;
                    actor.maintenance.masked = valid_replacement.is_some();
                    actor.containment.entered_at = self.tick;
                    actor.maintenance.instance_flags |= super::maintenance::INSTANCE_UNSELECTABLE;
                    self.observe_ground_partition(rider);
                }
            } else {
                // Tracker destroys without onRemoving: HELD/world membership stay unchanged.
                self.tunnel_object_removed(rider);
                self.destroy_maintenance_object(rider);
            }
        }
        if replacement.is_none() {
            if let Some(index) = self.entities[id].cave_index() {
                self.caves[index]
                    .as_mut()
                    .expect("cave tracker")
                    .passengers
                    .clear();
            } else {
                self.tunnels[owner].passengers.clear();
            }
        }
    }

    pub(super) fn sell_tunnel(&mut self, id: usize) {
        if self.entities[id].containment.tunnel.is_none() {
            return;
        }
        if self.tunnel_network(id).endpoints.as_slice() == [id] {
            // TunnelContain::onSelling removes immediately before unregistering,
            // so simultaneous sales cannot destroy the last endpoint's cargo.
            let riders = std::mem::take(&mut self.tunnel_network_mut(id).passengers);
            for rider in riders {
                self.remove_passenger(rider, id);
            }
        }
        self.unregister_tunnel(id);
    }

    pub(super) fn container_exit_target(&self, id: usize) -> Option<usize> {
        let state = &self.entities[id].containment;
        state.exit_at.or(state.carrier)
    }

    pub(super) fn destroy_container_contents(&mut self, id: usize) {
        if matches!(self.entities[id].ai.order, Order::ExitContainer)
            && let Some(endpoint) = self.entities[id].containment.exit_entry_to_clear.take()
        {
            self.entities[endpoint].containment.waiting.remove(&id);
        }
        if self.entities[id].containment.tunnel.is_none()
            && self.entities[id].containment.cave.is_none()
        {
            self.unregister_tunnel(id);
        }
        // OpenContain::onDelete destroys the literal list. Each rider's
        // Object::onDestroy removes it through the parent's actual callbacks.
        let passengers = self.entities[id].containment.passengers.clone();
        for rider in passengers {
            self.destroy_maintenance_object(rider);
        }
        self.entities[id].containment.exit_at = None;
        self.entities[id].containment.waiting.clear();
    }
}
