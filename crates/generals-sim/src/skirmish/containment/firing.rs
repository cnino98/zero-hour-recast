//! Native `isPassengerAllowedToFire` virtual dispatch, SH 2bfaea60.
pub(super) struct State {
    pub(super) allowed: bool,
    pub(super) aim_target: Option<usize>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            allowed: false,
            aim_target: None,
        }
    }
}

use super::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn apply_passenger_fire_upgrade(&mut self, id: usize) {
        if self.catalog[self.entities[id].type_index]
            .contain_family()
            .is_some()
        {
            self.entities[id].containment.firing.allowed = true;
        }
    }
}

impl crate::skirmish::UnitType {
    pub(in crate::skirmish) fn contain_family(&self) -> Option<&str> {
        self.behavior_modules
            .iter()
            .find(|module| module.interfaces & 0x40 != 0)
            .map(|module| module.family.as_str())
    }

    pub(in crate::skirmish) fn is_helix_container(&self) -> bool {
        self.contain_family() == Some("HelixContain")
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn container_fire_allowed(
        &self,
        carrier: usize,
        rider: Option<usize>,
    ) -> bool {
        let actor = &self.entities[carrier];
        let unit = &self.catalog[actor.type_index];
        let Some(family) = unit.contain_family() else {
            return false;
        };
        if family == "GarrisonContain" {
            return !actor.subdued();
        }
        if unit.is_helix_container() {
            if actor.contained_by().is_some() {
                return false;
            }
            if rider.is_some() && rider == actor.attachment_child() {
                return true;
            }
            let Some(passenger) = rider
                .and_then(|id| self.entities.get(id))
                .filter(|rider| rider.exists())
            else {
                return false;
            };
            if !self.catalog[passenger.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "INFANTRY")
            {
                return false;
            }
            return self.transport_fire_allowed(carrier, rider);
        }
        if family == "OverlordContain" {
            if let Some(rider) = rider
                .and_then(|id| self.entities.get(id))
                .filter(|rider| rider.exists())
                && !self.catalog[rider.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| matches!(kind.as_str(), "INFANTRY" | "PORTABLE_STRUCTURE"))
            {
                return false;
            }
            if actor.contained_by().is_some() {
                return false;
            }
            return self.transport_fire_allowed(carrier, None);
        }
        if matches!(
            family,
            "OpenContain" | "HealContain" | "ParachuteContain" | "TunnelContain" | "CaveContain"
        ) {
            return self.open_fire_allowed(carrier);
        }
        if matches!(
            family,
            "TransportContain" | "InternetHackContain" | "RiderChangeContain"
        ) {
            return self.transport_fire_allowed(carrier, rider);
        }
        false
    }

    fn transport_fire_allowed(&self, carrier: usize, rider: Option<usize>) -> bool {
        if let Some(rider) = rider
            .and_then(|id| self.entities.get(id))
            .filter(|rider| rider.exists())
            && !self.catalog[rider.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "INFANTRY")
        {
            return false;
        }
        if let Some(parent) = self.entities[carrier].contained_by()
            && matches!(
                self.catalog[self.entities[parent].type_index].contain_family(),
                Some("OverlordContain" | "HelixContain")
            )
        {
            return self.container_fire_allowed(parent, rider);
        }
        self.open_fire_allowed(carrier)
    }

    fn open_fire_allowed(&self, carrier: usize) -> bool {
        let actor = &self.entities[carrier];
        if !actor.containment.firing.allowed {
            return false;
        }
        actor
            .contained_by()
            .is_none_or(|parent| self.container_fire_allowed(parent, None))
    }
}
