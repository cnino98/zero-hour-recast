//! `CaveContain` callbacks and shared cave-index ownership, SH 2bfaea60.
use crate::skirmish::{Entity, Skirmish};

pub(in crate::skirmish) struct State {
    needs_build_complete: bool,
    pub(super) index: usize,
    original_team: Option<usize>,
}

impl Entity {
    pub(in crate::skirmish) const fn cave_index(&self) -> Option<usize> {
        match &self.containment.cave {
            Some(state) => Some(state.index),
            None => None,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_cave(&mut self, id: usize) {
        self.construct_open_contain(id);
        self.entities[id].containment.cave = Some(State {
            needs_build_complete: true,
            index: 0,
            original_team: None,
        });
    }

    pub(in crate::skirmish) fn created_cave(&mut self, id: usize) {
        let index = self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.cave_index)
            .expect("CaveContain data");
        self.entities[id]
            .containment
            .cave
            .as_mut()
            .expect("CaveContain constructor")
            .index = index;
    }

    pub(in crate::skirmish) fn cave_build_complete(&mut self, id: usize) {
        let state = self.entities[id]
            .containment
            .cave
            .as_mut()
            .expect("CaveContain constructor");
        if !state.needs_build_complete {
            return;
        }
        state.needs_build_complete = false;
        self.register_tunnel(id);
    }

    pub(in crate::skirmish) fn cave_on_die(&mut self, id: usize) {
        let rules = self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.tunnel.as_ref())
            .expect("CaveContain data");
        if self.die_conditions_match(id, &rules.conditions)
            && !self.entities[id].under_construction()
        {
            // CaveSystem::unregisterCave intentionally retains the tracker.
            self.unregister_tunnel(id);
        }
    }

    pub(in crate::skirmish) fn capture_cave_network(&mut self, carrier: usize, rider: usize) {
        let original = self.team_membership.team_of(carrier);
        let Some(state) = &mut self.entities[carrier].containment.cave else {
            return;
        };
        if state.original_team.is_none() || original.is_none() {
            state.original_team = original;
        }
        if self.tunnel_network(carrier).passengers.len() == 1 {
            let team = self
                .team_membership
                .default_team(self.entities[rider].owner);
            self.change_cave_teams(carrier, team, true);
        }
    }

    fn change_cave_teams(&mut self, carrier: usize, team: usize, save_original: bool) {
        for endpoint in self.tunnel_network(carrier).endpoints.clone() {
            if !self.entities[endpoint].exists() {
                continue;
            }
            let original = save_original
                .then(|| self.team_membership.team_of(endpoint))
                .flatten();
            let Some(state) = &mut self.entities[endpoint].containment.cave else {
                continue;
            };
            state.original_team = original;
            self.defect_to_team(endpoint, team, 0);
            self.observe_map_member_team(endpoint);
        }
    }

    pub(in crate::skirmish) fn remove_cave_passenger(&mut self, id: usize, carrier: usize) {
        let Some(index) = self
            .tunnel_network(carrier)
            .passengers
            .iter()
            .position(|&rider| rider == id)
        else {
            return;
        };
        // Cave removes from the tracker before onRemoving's last-passenger edge.
        self.tunnel_network_mut(carrier).passengers.remove(index);
        self.container_removal_sound(carrier, id, false);
        self.clear_disabled_held(id);
        self.partition
            .observe(id, self.object_position(id), self.entities[id].facing);
        self.entities[id].containment.nested_removed = false;
        self.set_object_position(id, self.object_position(carrier));
        self.set_drawable_hidden(id, false);
        self.queue_container_sound(carrier, false);
        if self.tunnel_network(carrier).passengers.is_empty()
            && self.team_membership.team_of(carrier).is_some()
        {
            let original = self.entities[carrier]
                .containment
                .cave
                .as_ref()
                .expect("CaveContain constructor")
                .original_team;
            if let Some(team) = original {
                self.change_cave_teams(carrier, team, false);
            }
        }
        // Object::onRemovedFrom follows both container callbacks. Cave deliberately
        // does not call OpenContain's addOrRemoveObjFromWorld/pathfinder restoration.
        self.tunnel_object_removed(id);
    }
}
