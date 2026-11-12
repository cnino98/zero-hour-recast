//! Native Overlord/Helix `onDelete` overrides, distinct from their `onDie` callbacks.
use super::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn delete_helix_containment(&mut self, id: usize) {
        if let Some(portable) = self.entities[id]
            .attachment_child()
            .filter(|&portable| self.entities[portable].exists())
        {
            self.destroy_maintenance_object(portable);
        }
        self.destroy_container_contents(id);
    }

    pub(in crate::skirmish) fn delete_overlord_containment(&mut self, id: usize) {
        let redirected = self.redirected_container(id);
        if redirected != id {
            while let Some(&rider) = self.passengers(redirected).first() {
                self.remove_passenger_exposed(rider, redirected, false);
            }
            self.entities[id].attachment.redirection_active = false;
            while let Some(&rider) = self.entities[id].containment.passengers.first() {
                self.remove_passenger_exposed(rider, id, false);
                if self.entities[rider].attachment.parent == Some(id) {
                    self.entities[rider].attachment.parent = None;
                }
            }
            self.entities[id].attachment.child = None;
        }
        self.destroy_container_contents(id);
    }

    /// `HelixContain.cpp297` bypasses ordinary transport removal callbacks.
    pub(in crate::skirmish) fn remove_helix_portable(&mut self, id: usize, carrier: usize) -> bool {
        if !self.catalog[self.entities[carrier].type_index].is_helix_container()
            || self.entities[carrier].attachment_child() != Some(id)
            || !self.attack_kind(id, "PORTABLE_STRUCTURE")
        {
            return false;
        }
        // The selected CRC=0 profile clears the actual containedBy pointer.
        self.entities[id].containment.carrier = None;
        self.entities[id].attachment.parent = None;
        self.entities[carrier].attachment.child = None;
        true
    }
}
