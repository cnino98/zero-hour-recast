//! Authored `TunnelContain` callbacks and `TunnelTracker` healing/nemesis, SH 2bfaea60.
use crate::skirmish::{Skirmish, maintenance};

impl Skirmish {
    pub(in crate::skirmish) fn construct_tunnel(&mut self, id: usize) {
        self.construct_open_contain(id);
        self.entities[id].containment.tunnel = Some(super::State {
            created: false,
            registered: false,
        });
    }

    pub(in crate::skirmish) fn created_tunnel(&mut self, id: usize) {
        let state = self.entities[id]
            .containment
            .tunnel
            .as_mut()
            .expect("TunnelContain constructor");
        if state.created {
            return;
        }
        state.created = true;
        self.register_tunnel(id);
    }

    pub(in crate::skirmish) fn tunnel_on_die(&mut self, id: usize) {
        let rules = self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.tunnel.as_ref())
            .expect("TunnelContain data");
        if self.die_conditions_match(id, &rules.conditions) {
            self.unregister_tunnel(id);
        }
    }

    pub(in crate::skirmish) fn capture_tunnel(&mut self, id: usize, previous: usize, owner: usize) {
        if self.entities[id]
            .containment
            .tunnel
            .as_ref()
            .is_some_and(|state| state.registered)
        {
            // Object has already changed teams before its onCapture visitor.
            self.unregister_tunnel_network(id, previous);
            self.tunnels[owner].endpoints.push(id);
        }
        // OpenContain::onCapture is empty.
    }

    pub(in crate::skirmish) fn update_tunnel(&mut self, id: usize) -> i32 {
        self.update_open_contain(id);
        let actor = &self.entities[id];
        let owner = actor.owner;
        let frames = self.catalog[actor.type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.tunnel.as_ref())
            .expect("TunnelContain data")
            .full_heal_frames;
        // PRESERVE_TUNNEL_HEAL_STACKING=1: every enabled endpoint heals the shared list.
        for rider in self.tunnels[owner].passengers.clone() {
            let actor = &self.entities[rider];
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
            self.apply_healing(rider, None, amount);
        }
        let damage = &self.entities[id].damage_state;
        if damage.retained_frame.wrapping_add(30) > self.update_frame()
            && let Some(attacker) = damage
                .retained_source
                .filter(|&attacker| self.entities[attacker].exists())
            && self.object_relationship(id, attacker) == 0
        {
            self.update_tunnel_nemesis(owner, attacker);
        }
        1
    }

    fn update_tunnel_nemesis(&mut self, owner: usize, target: usize) {
        let current = self.current_tunnel_nemesis(owner);
        if current.is_none() {
            if self.catalog[self.entities[target].type_index]
                .kind_of
                .iter()
                .any(|kind| {
                    matches!(
                        kind.as_str(),
                        "VEHICLE" | "STRUCTURE" | "INFANTRY" | "AIRCRAFT"
                    )
                })
            {
                self.tunnels[owner].nemesis = Some(target);
                self.tunnels[owner].nemesis_frame = self.update_frame();
            }
        } else if current == Some(target) {
            self.tunnels[owner].nemesis_frame = self.update_frame();
        }
    }

    fn current_tunnel_nemesis(&mut self, owner: usize) -> Option<usize> {
        let target = self.tunnels[owner].nemesis?;
        let valid = self.tunnels[owner].nemesis_frame.wrapping_add(120) >= self.update_frame()
            && self.entities.get(target).is_some_and(|actor| {
                actor.exists()
                    && !actor.is_effectively_dead()
                    && (!actor.stealthed()
                        || actor.detected()
                        || actor.disguised_player().is_some())
            });
        if !valid {
            self.tunnels[owner].nemesis = None;
        }
        self.tunnels[owner].nemesis
    }

    pub(in crate::skirmish) fn remove_tunnel_passenger(&mut self, id: usize, carrier: usize) {
        // Unlike OpenContain, this virtual remove has no membership guard.
        self.container_removal_sound(carrier, id, false);
        self.clear_disabled_held(id);
        let position = self.object_position(carrier);
        self.set_object_position(id, position);
        self.initialize_safe_occlusion(id);
        self.containment_world_membership(id, true);
        self.queue_container_sound(carrier, false);
        self.tunnel_object_removed(id);
        if let Some(index) = self
            .tunnel_network(carrier)
            .passengers
            .iter()
            .position(|&rider| rider == id)
        {
            self.tunnel_network_mut(carrier).passengers.remove(index);
        }
    }

    pub(in crate::skirmish::tunnel) fn tunnel_object_removed(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        actor.maintenance.masked = false;
        actor.maintenance.instance_flags &= !maintenance::INSTANCE_UNSELECTABLE;
        actor.containment.carrier = None;
        actor.containment.entered_at = 0;
        self.observe_ground_partition(id);
    }
}
