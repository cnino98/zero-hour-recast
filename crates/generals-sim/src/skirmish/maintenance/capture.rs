//! Native Object team transfer and defection callbacks.
use super::super::Skirmish;
mod defection;

pub enum CaptureEffect {
    Captured,
    DefectedWithoutDrawable,
    Infiltrated([f32; 3]),
    Detected,
    DefectorTick,
}

pub struct CaptureEvent {
    pub object: usize,
    pub target: usize,
    pub owner: usize,
    pub frame: u64,
    pub effect: CaptureEffect,
}

impl Skirmish {
    pub(in crate::skirmish) fn transfer_maintenance_owner(&mut self, id: usize, owner: usize) {
        let team = self.team_membership.default_team(owner);
        self.transfer_maintenance_team(id, team);
    }

    pub(in crate::skirmish) fn transfer_maintenance_team(&mut self, id: usize, mut team: usize) {
        let mut owner = self.team_membership.team_owner(team);
        // Native neutral Player is active; lobby occupancy is not its life state.
        if owner != self.neutral_owner() && !self.player_active(owner) {
            owner = self.neutral_owner();
            team = self.team_membership.default_team(owner);
        }
        if self.team_membership.team_of(id) == Some(team) {
            return;
        }
        let previous = self.entities[id].owner;
        let unit = &self.catalog[self.entities[id].type_index];
        let adjusts_energy = !self.entities[id].under_construction()
            && unit.energy != 0
            && !(unit.energy > 0 && self.object_disabled(id));
        self.team_membership.set_team(id, None);
        if adjusts_energy {
            self.update_power_excluding(Some(id), Some(previous));
        }
        self.entities[id].owner = owner;
        self.map_unit_produced(None, id);
        self.team_membership.set_team(id, Some(team));
        if adjusts_energy {
            self.update_power_excluding(None, Some(owner));
        }
        // Player::becomingTeamMember skips AutoDeposit callbacks for neutral.
        if owner != self.neutral_owner() && !self.entities[id].maintenance.deposits.is_empty() {
            self.award_initial_deposit(id, 0, owner);
        }
        // Object::onCapture dispatches virtual Idle before behavior callbacks.
        if previous != owner
            && !self.allied(previous, owner)
            && self.entities[id].ai.registration.is_some()
        {
            self.native_idle_from(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
            self.cancel_all_dozer_tasks(id);
        }
        // Object::setOrRestoreTeam invokes onCapture after both influence transitions.
        for index in 0..self.catalog[self.entities[id].type_index]
            .behavior_modules
            .len()
        {
            let modules = &self.catalog[self.entities[id].type_index].behavior_modules;
            let module = &modules[index];
            let ordinal = modules[..index]
                .iter()
                .filter(|prior| prior.family == module.family)
                .count();
            match module.family.as_str() {
                "TunnelContain" => self.capture_tunnel(id, previous, owner),
                "TechBuildingBehavior" => {
                    self.wake_family_update(id, "TechBuildingBehavior", ordinal, 1);
                }
                "PowerPlantUpgrade" => self.on_capture_power_upgrade(id, index, previous, owner),
                "CostModifierUpgrade" => self.capture_cost_modifier(id, index, previous),
                "OverchargeBehavior" => self.on_capture_overcharge(id, ordinal, previous, owner),
                "TransportContain" | "InternetHackContain" => {
                    self.capture_transport_contain(id, previous, owner);
                }
                "OverlordContain" => self.capture_overlord_contain(id, owner),
                "HelixContain" => self.capture_helix_contain(id, owner),
                _ => {}
            }
        }
        self.radar_on_capture(id, previous, owner);
        self.capture_spy_vision(id, previous, owner);
        self.capture_propaganda(id);
        self.capture_battle_plan(id, previous);
        self.observe_visibility(id);
        self.update_power();
    }

    pub(in crate::skirmish) fn capture_event(
        &mut self,
        object: usize,
        target: usize,
        effect: CaptureEffect,
    ) {
        self.presentation.capture_events.push(CaptureEvent {
            object,
            target,
            owner: self.entities[object].owner,
            frame: self.tick,
            effect,
        });
    }
}
